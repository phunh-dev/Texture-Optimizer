//! Per-tab cache of decoded images used by `preview_op`.
//!
//! Entries are keyed by `(tab id, path)` and evicted least-recently-used
//! across all tabs once the total decoded size exceeds the budget. When a tab
//! goes to sleep the frontend calls `release_session`, which drops all of
//! that tab's entries at once. An entry is reloaded when the file's mtime or
//! size changed since it was decoded.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use texopt_core::{ImageBuf, OpResult};

/// Default memory budget for decoded preview images (all tabs together).
pub const DEFAULT_SESSION_BUDGET_BYTES: usize = 512 * 1024 * 1024;

/// Identity of the file version an entry was decoded from.
type FileStamp = (u64, u64);

#[derive(Debug)]
struct Entry {
    image: Arc<ImageBuf>,
    bytes: usize,
    stamp: FileStamp,
    last_used: u64,
}

#[derive(Debug, Default)]
struct Inner {
    entries: HashMap<(String, PathBuf), Entry>,
    total_bytes: usize,
    clock: u64,
}

#[derive(Debug)]
pub struct SessionCache {
    budget_bytes: usize,
    inner: Mutex<Inner>,
}

impl Default for SessionCache {
    fn default() -> Self {
        Self::new(DEFAULT_SESSION_BUDGET_BYTES)
    }
}

fn stamp_of(path: &Path) -> FileStamp {
    std::fs::metadata(path)
        .map(|m| (texopt_core::io::mtime_ms(&m), m.len()))
        .unwrap_or((0, 0))
}

impl SessionCache {
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            budget_bytes,
            inner: Mutex::new(Inner::default()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Cached image for `(tab, path)`, or decode it with `loader` and cache it.
    /// Decoding happens outside the lock. Images larger than the whole budget
    /// are returned but not cached.
    pub fn get_or_load(
        &self,
        tab: &str,
        path: &Path,
        loader: impl FnOnce(&Path) -> OpResult<ImageBuf>,
    ) -> OpResult<Arc<ImageBuf>> {
        let stamp = stamp_of(path);
        let key = (tab.to_string(), path.to_path_buf());
        {
            let mut inner = self.lock();
            inner.clock += 1;
            let now = inner.clock;
            if let Some(entry) = inner.entries.get_mut(&key).filter(|e| e.stamp == stamp) {
                entry.last_used = now;
                return Ok(entry.image.clone());
            }
        }

        let image = Arc::new(loader(path)?);
        let bytes = image.as_raw().len();
        if bytes > self.budget_bytes {
            return Ok(image);
        }

        let mut inner = self.lock();
        inner.clock += 1;
        let last_used = inner.clock;
        if let Some(old) = inner.entries.insert(
            key,
            Entry {
                image: image.clone(),
                bytes,
                stamp,
                last_used,
            },
        ) {
            inner.total_bytes -= old.bytes;
        }
        inner.total_bytes += bytes;
        while inner.total_bytes > self.budget_bytes {
            let Some(oldest) = inner
                .entries
                .iter()
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(evicted) = inner.entries.remove(&oldest) {
                inner.total_bytes -= evicted.bytes;
            }
        }
        Ok(image)
    }

    /// Drop every cached image of `tab` (tab went to sleep or was closed).
    pub fn release(&self, tab: &str) {
        let mut inner = self.lock();
        let mut freed = 0;
        inner.entries.retain(|(t, _), e| {
            let keep = t != tab;
            if !keep {
                freed += e.bytes;
            }
            keep
        });
        inner.total_bytes -= freed;
    }

    pub fn total_bytes(&self) -> usize {
        self.lock().total_bytes
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, tab: &str, path: &Path) -> bool {
        self.lock()
            .entries
            .contains_key(&(tab.to_string(), path.to_path_buf()))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use texopt_core::fixtures;

    use super::*;

    /// 10x10 RGBA = 400 bytes per image.
    const IMG_BYTES: usize = 400;

    fn loader(calls: &Cell<u32>) -> impl FnOnce(&Path) -> OpResult<ImageBuf> + '_ {
        move |_| {
            calls.set(calls.get() + 1);
            Ok(fixtures::solid(10, 10, fixtures::RED))
        }
    }

    #[test]
    fn hit_does_not_reload() {
        let cache = SessionCache::new(10 * IMG_BYTES);
        let calls = Cell::new(0);
        let p = Path::new("virtual/a.png");
        let a = cache.get_or_load("t1", p, loader(&calls)).unwrap();
        let b = cache.get_or_load("t1", p, loader(&calls)).unwrap();
        assert_eq!(calls.get(), 1);
        assert!(Arc::ptr_eq(&a, &b));
        // Same path in another tab is a separate entry.
        cache.get_or_load("t2", p, loader(&calls)).unwrap();
        assert_eq!(calls.get(), 2);
        assert_eq!(cache.total_bytes(), 2 * IMG_BYTES);
    }

    #[test]
    fn lru_eviction_by_budget() {
        let cache = SessionCache::new(3 * IMG_BYTES);
        let calls = Cell::new(0);
        let (a, b, c, d) = (
            Path::new("a"),
            Path::new("b"),
            Path::new("c"),
            Path::new("d"),
        );
        cache.get_or_load("t", a, loader(&calls)).unwrap();
        cache.get_or_load("t", b, loader(&calls)).unwrap();
        cache.get_or_load("u", c, loader(&calls)).unwrap();
        // Touch `a` so `b` becomes the least recently used.
        cache.get_or_load("t", a, loader(&calls)).unwrap();
        cache.get_or_load("u", d, loader(&calls)).unwrap();
        assert_eq!(cache.len(), 3);
        assert_eq!(cache.total_bytes(), 3 * IMG_BYTES);
        assert!(cache.contains("t", a));
        assert!(!cache.contains("t", b));
        assert!(cache.contains("u", c));
        assert!(cache.contains("u", d));
        assert_eq!(calls.get(), 4);
    }

    #[test]
    fn oversized_images_are_not_cached() {
        let cache = SessionCache::new(IMG_BYTES - 1);
        let calls = Cell::new(0);
        let img = cache
            .get_or_load("t", Path::new("big"), loader(&calls))
            .unwrap();
        assert_eq!(img.dimensions(), (10, 10));
        assert!(cache.is_empty());
        assert_eq!(cache.total_bytes(), 0);
    }

    #[test]
    fn release_drops_only_that_tab() {
        let cache = SessionCache::new(10 * IMG_BYTES);
        let calls = Cell::new(0);
        cache
            .get_or_load("t1", Path::new("a"), loader(&calls))
            .unwrap();
        cache
            .get_or_load("t1", Path::new("b"), loader(&calls))
            .unwrap();
        cache
            .get_or_load("t2", Path::new("a"), loader(&calls))
            .unwrap();
        cache.release("t1");
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.total_bytes(), IMG_BYTES);
        assert!(cache.contains("t2", Path::new("a")));
        cache
            .get_or_load("t1", Path::new("a"), loader(&calls))
            .unwrap();
        assert_eq!(calls.get(), 4, "released entries are decoded again");
    }

    #[test]
    fn reloads_when_file_changes() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.png");
        fixtures::solid(4, 4, fixtures::RED).save(&p).unwrap();
        let cache = SessionCache::new(10 * IMG_BYTES);
        let img = cache
            .get_or_load("t", &p, texopt_core::io::load_image)
            .unwrap();
        assert_eq!(img.get_pixel(0, 0), &fixtures::RED);

        fixtures::solid(4, 4, fixtures::BLUE).save(&p).unwrap();
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&p)
            .unwrap()
            .set_modified(later)
            .unwrap();
        let img = cache
            .get_or_load("t", &p, texopt_core::io::load_image)
            .unwrap();
        assert_eq!(img.get_pixel(0, 0), &fixtures::BLUE);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.total_bytes(), 64);
    }

    #[test]
    fn loader_errors_propagate_and_cache_nothing() {
        let cache = SessionCache::default();
        let err = cache
            .get_or_load("t", Path::new("x"), |_| {
                Err(texopt_core::OpError::new("BOOM"))
            })
            .unwrap_err();
        assert_eq!(err.code, "BOOM");
        assert!(cache.is_empty());
    }
}
