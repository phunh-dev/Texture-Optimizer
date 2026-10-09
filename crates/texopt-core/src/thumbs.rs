//! Disk-cached thumbnails for the image grid.
//!
//! Thumbnails are lossless WebP (keeps alpha, smaller than PNG) whose longest
//! side is the requested size (never upscaled). The cache file name is
//! `blake3(path, mtime, file size, thumb size, THUMB_VERSION)`, so editing a
//! source file (new mtime/size) naturally produces a new entry; stale entries
//! are simply never requested again.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::{ImageBuf, OpError, OpResult};

/// Thumbnail generation failed. Params: `path`, `detail`.
pub const THUMB_FAILED: &str = "THUMB_FAILED";

/// Bump when the thumbnail pipeline changes to invalidate old cache files.
pub const THUMB_VERSION: u32 = 1;

pub const THUMB_EXTENSION: &str = "webp";
pub const THUMB_CONTENT_TYPE: &str = "image/webp";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThumbSize {
    Small,
    Medium,
    Large,
}

impl ThumbSize {
    /// Longest side in pixels.
    pub fn pixels(self) -> u32 {
        match self {
            Self::Small => 128,
            Self::Medium => 256,
            Self::Large => 512,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            _ => None,
        }
    }
}

/// Cache key (hex) for a source file state + size.
pub fn cache_key(path: &Path, mtime_ms: u64, size_bytes: u64, size: ThumbSize) -> String {
    let mut h = blake3::Hasher::new();
    h.update(path.to_string_lossy().as_bytes());
    h.update(&[0]);
    h.update(&mtime_ms.to_le_bytes());
    h.update(&size_bytes.to_le_bytes());
    h.update(size.as_str().as_bytes());
    h.update(&THUMB_VERSION.to_le_bytes());
    h.finalize().to_hex().to_string()
}

/// Downscale so the longest side is at most `max_side`, keeping the aspect
/// ratio (each side at least 1px). Smaller images are returned unchanged.
pub fn make_thumbnail(img: &ImageBuf, max_side: u32) -> ImageBuf {
    let (w, h) = img.dimensions();
    let longest = w.max(h);
    if longest <= max_side || longest == 0 {
        return img.clone();
    }
    let scale = max_side as f64 / longest as f64;
    let tw = ((w as f64 * scale).round() as u32).clamp(1, max_side);
    let th = ((h as f64 * scale).round() as u32).clamp(1, max_side);
    image::imageops::thumbnail(img, tw, th)
}

fn thumb_error(path: &Path, e: &dyn std::fmt::Display) -> OpError {
    OpError::new(THUMB_FAILED)
        .with("path", path.display().to_string())
        .with("detail", e.to_string())
}

/// Thumbnail cache rooted at a folder (the app uses `<app cache>/thumbs`).
#[derive(Debug)]
pub struct ThumbCache {
    dir: PathBuf,
    generated: AtomicU64,
}

impl ThumbCache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            generated: AtomicU64::new(0),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Number of thumbnails generated (cache misses) by this instance.
    pub fn generated_count(&self) -> u64 {
        self.generated.load(Ordering::Relaxed)
    }

    pub fn cache_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.{THUMB_EXTENSION}"))
    }

    /// Path of the cached thumbnail for `src`, generating it on a miss.
    /// Errors: `SCAN_PATH_NOT_FOUND` when the source is missing, otherwise
    /// the decode error or `THUMB_FAILED`.
    pub fn get_or_create(&self, src: &Path, size: ThumbSize) -> OpResult<PathBuf> {
        let meta = std::fs::metadata(src).map_err(|_| {
            OpError::new(crate::io::SCAN_PATH_NOT_FOUND).with("path", src.display().to_string())
        })?;
        let key = cache_key(src, crate::io::mtime_ms(&meta), meta.len(), size);
        let cached = self.cache_path(&key);
        if cached.is_file() {
            return Ok(cached);
        }
        let img = crate::io::load_image(src)?;
        let thumb = make_thumbnail(&img, size.pixels());
        let mut data = Vec::new();
        image::ImageEncoder::write_image(
            image::codecs::webp::WebPEncoder::new_lossless(&mut data),
            thumb.as_raw(),
            thumb.width(),
            thumb.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| thumb_error(src, &e))?;
        crate::output::write_atomic(&cached, &data).map_err(|e| thumb_error(src, &e))?;
        self.generated.fetch_add(1, Ordering::Relaxed);
        Ok(cached)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    fn setup() -> (tempfile::TempDir, ThumbCache) {
        let dir = tempfile::tempdir().unwrap();
        let cache = ThumbCache::new(dir.path().join("thumbs"));
        (dir, cache)
    }

    fn dims(p: &Path) -> (u32, u32) {
        image::image_dimensions(p).unwrap()
    }

    #[test]
    fn generated_size_per_mode() {
        let (dir, cache) = setup();
        let src = dir.path().join("wide.png");
        fixtures::gradient(1000, 500).save(&src).unwrap();
        assert_eq!(
            dims(&cache.get_or_create(&src, ThumbSize::Small).unwrap()),
            (128, 64)
        );
        assert_eq!(
            dims(&cache.get_or_create(&src, ThumbSize::Medium).unwrap()),
            (256, 128)
        );
        assert_eq!(
            dims(&cache.get_or_create(&src, ThumbSize::Large).unwrap()),
            (512, 256)
        );

        let tall = dir.path().join("tall.png");
        fixtures::gradient(300, 900).save(&tall).unwrap();
        assert_eq!(
            dims(&cache.get_or_create(&tall, ThumbSize::Medium).unwrap()),
            (85, 256)
        );
        assert_eq!(cache.generated_count(), 4);
    }

    #[test]
    fn never_upscales() {
        let (dir, cache) = setup();
        let src = dir.path().join("tiny.png");
        fixtures::gradient(40, 20).save(&src).unwrap();
        for size in [ThumbSize::Small, ThumbSize::Medium, ThumbSize::Large] {
            assert_eq!(dims(&cache.get_or_create(&src, size).unwrap()), (40, 20));
        }
        // Extreme aspect ratio keeps at least 1px.
        assert_eq!(
            make_thumbnail(&fixtures::gradient(2000, 1), 128).dimensions(),
            (128, 1)
        );
    }

    #[test]
    fn alpha_is_preserved() {
        let (dir, cache) = setup();
        let src = dir.path().join("sprite.png");
        // Left half transparent, right half opaque red.
        fixtures::sprite(400, 400, 200, 0, 200, 400, fixtures::RED)
            .save(&src)
            .unwrap();
        let thumb = image::open(cache.get_or_create(&src, ThumbSize::Small).unwrap())
            .unwrap()
            .into_rgba8();
        assert_eq!(thumb.dimensions(), (128, 128));
        assert_eq!(thumb.get_pixel(10, 64)[3], 0);
        assert_eq!(thumb.get_pixel(120, 64), &fixtures::RED);
    }

    #[test]
    fn cache_hit_does_not_regenerate() {
        let (dir, cache) = setup();
        let src = dir.path().join("a.png");
        fixtures::gradient(600, 600).save(&src).unwrap();
        let first = cache.get_or_create(&src, ThumbSize::Medium).unwrap();
        let written = std::fs::metadata(&first).unwrap().modified().unwrap();
        let second = cache.get_or_create(&src, ThumbSize::Medium).unwrap();
        assert_eq!(first, second);
        assert_eq!(cache.generated_count(), 1);
        assert_eq!(
            std::fs::metadata(&second).unwrap().modified().unwrap(),
            written
        );

        // A new cache instance on the same folder (app restart) also hits.
        let again = ThumbCache::new(cache.dir());
        assert_eq!(again.get_or_create(&src, ThumbSize::Medium).unwrap(), first);
        assert_eq!(again.generated_count(), 0);
    }

    #[test]
    fn cache_miss_when_source_changes() {
        let (dir, cache) = setup();
        let src = dir.path().join("a.png");
        fixtures::solid(300, 300, fixtures::RED).save(&src).unwrap();
        let first = cache.get_or_create(&src, ThumbSize::Small).unwrap();

        fixtures::solid(300, 300, fixtures::BLUE)
            .save(&src)
            .unwrap();
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&src)
            .unwrap()
            .set_modified(later)
            .unwrap();

        let second = cache.get_or_create(&src, ThumbSize::Small).unwrap();
        assert_ne!(first, second);
        assert_eq!(cache.generated_count(), 2);
        let px = *image::open(&second).unwrap().into_rgba8().get_pixel(5, 5);
        assert_eq!(px, fixtures::BLUE);
    }

    #[test]
    fn keys_differ_by_every_component() {
        let p = Path::new("/a/b.png");
        let base = cache_key(p, 1, 2, ThumbSize::Small);
        assert_eq!(base, cache_key(p, 1, 2, ThumbSize::Small));
        assert_ne!(
            base,
            cache_key(Path::new("/a/c.png"), 1, 2, ThumbSize::Small)
        );
        assert_ne!(base, cache_key(p, 9, 2, ThumbSize::Small));
        assert_ne!(base, cache_key(p, 1, 9, ThumbSize::Small));
        assert_ne!(base, cache_key(p, 1, 2, ThumbSize::Large));
    }

    #[test]
    fn errors_have_codes() {
        let (dir, cache) = setup();
        let missing = cache
            .get_or_create(&dir.path().join("missing.png"), ThumbSize::Small)
            .unwrap_err();
        assert_eq!(missing.code, crate::io::SCAN_PATH_NOT_FOUND);
        let corrupt = dir.path().join("bad.png");
        std::fs::write(&corrupt, b"nope").unwrap();
        let err = cache.get_or_create(&corrupt, ThumbSize::Small).unwrap_err();
        assert_eq!(err.code, crate::error::codes::IMG_DECODE_FAILED);
    }

    #[test]
    fn size_parsing() {
        for s in [ThumbSize::Small, ThumbSize::Medium, ThumbSize::Large] {
            assert_eq!(ThumbSize::parse(s.as_str()), Some(s));
        }
        assert_eq!(ThumbSize::parse("huge"), None);
    }
}
