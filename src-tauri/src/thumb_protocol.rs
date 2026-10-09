//! `thumb` custom URI scheme serving disk-cached thumbnails.
//!
//! The frontend builds URLs with `convertFileSrc(path, 'thumb')` plus
//! `?size=small|medium|large&m=<mtimeMs>`. `convertFileSrc` percent-encodes
//! the whole path (`encodeURIComponent`) as the first path segment:
//! - Windows / Android: `http://thumb.localhost/C%3A%5Cdir%5Ca.png?size=small&m=1`
//! - macOS / Linux:     `thumb://localhost/%2Fhome%2Fu%2Fa.png?size=small&m=1`
//!
//! `m` is only a cache-buster for the webview (a changed file gets a new URL);
//! the disk cache key is computed from the real file metadata. Responses are
//! therefore immutable and cached aggressively.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use percent_encoding::percent_decode_str;
use tauri::http::{Response, StatusCode, header};
use texopt_core::OpError;
use texopt_core::error::codes;
use texopt_core::thumbs::{THUMB_CONTENT_TYPE, ThumbCache, ThumbSize};

pub const SCHEME: &str = "thumb";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbRequest {
    pub path: PathBuf,
    pub size: ThumbSize,
}

/// Parse a `thumb` request URI (either platform form) into path + size.
/// `size` defaults to `medium` when absent.
pub fn parse_thumb_uri(uri: &str) -> Result<ThumbRequest, OpError> {
    let bad = |reason: &str| OpError::invalid_param("uri", reason);
    let rest = uri
        .split_once("://")
        .map(|(_, r)| r)
        .ok_or_else(|| bad("no scheme"))?;
    let rest = rest.split('#').next().unwrap_or_default();
    // Drop the authority (`localhost` / `thumb.localhost`).
    let path_and_query = rest
        .find('/')
        .map(|i| &rest[i + 1..])
        .ok_or_else(|| bad("no path"))?;
    let (raw_path, query) = path_and_query
        .split_once('?')
        .unwrap_or((path_and_query, ""));
    let decoded = percent_decode_str(raw_path)
        .decode_utf8()
        .map_err(|_| bad("path is not UTF-8"))?;
    if decoded.is_empty() {
        return Err(bad("empty path"));
    }
    let mut size = ThumbSize::Medium;
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key == "size" {
            size = ThumbSize::parse(value).ok_or_else(|| bad("size"))?;
        }
    }
    Ok(ThumbRequest {
        path: PathBuf::from(decoded.into_owned()),
        size,
    })
}

fn error_response(status: StatusCode, error: &OpError) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CACHE_CONTROL, "no-store")
        .body(serde_json::to_vec(error).unwrap_or_default())
        .expect("static response parts are valid")
}

/// Full request handling (blocking: may decode and encode an image).
/// 200 + WebP on success, 400 for malformed URLs, 404 when the source file
/// is missing, 500 when decoding/encoding fails. Error bodies are the JSON
/// `{ code, params }` of the failure.
pub fn thumb_response(cache: &ThumbCache, uri: &str) -> Response<Vec<u8>> {
    let request = match parse_thumb_uri(uri) {
        Ok(r) => r,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, &e),
    };
    let cached = match cache.get_or_create(&request.path, request.size) {
        Ok(p) => p,
        Err(e) if e.code == texopt_core::io::SCAN_PATH_NOT_FOUND => {
            return error_response(StatusCode::NOT_FOUND, &e);
        }
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
    };
    match std::fs::read(&cached) {
        Ok(body) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, THUMB_CONTENT_TYPE)
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
            .body(body)
            .expect("static response parts are valid"),
        Err(e) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &OpError::new(codes::IO_READ_FAILED)
                .with("path", cached.display().to_string())
                .with("detail", e.to_string()),
        ),
    }
}

/// Managed state behind the protocol: a lazily created cache (the app cache
/// dir is only known once the app handle exists) and a small dedicated
/// thread pool, so thumbnail work never runs on the UI thread, never starves
/// batch jobs (global rayon pool) and is bounded in concurrency.
pub struct ThumbService {
    cache: OnceLock<Arc<ThumbCache>>,
    pool: rayon::ThreadPool,
}

impl ThumbService {
    pub fn new() -> Self {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get() / 2)
            .unwrap_or(2)
            .clamp(2, 4);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("thumb-{i}"))
            .build()
            .expect("failed to build thumbnail thread pool");
        Self {
            cache: OnceLock::new(),
            pool,
        }
    }

    pub fn cache(&self, dir: impl FnOnce() -> PathBuf) -> Arc<ThumbCache> {
        self.cache
            .get_or_init(|| Arc::new(ThumbCache::new(dir())))
            .clone()
    }

    pub fn spawn(&self, job: impl FnOnce() + Send + 'static) {
        self.pool.spawn(job);
    }
}

impl Default for ThumbService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use texopt_core::fixtures;

    use super::*;

    /// What `encodeURIComponent` produces.
    fn encode_uri_component(s: &str) -> String {
        const SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
            .remove(b'-')
            .remove(b'_')
            .remove(b'.')
            .remove(b'!')
            .remove(b'~')
            .remove(b'*')
            .remove(b'\'')
            .remove(b'(')
            .remove(b')');
        percent_encoding::utf8_percent_encode(s, SET).to_string()
    }

    fn windows_url(path: &str, size: &str) -> String {
        format!(
            "http://thumb.localhost/{}?size={size}&m=1700000000000",
            encode_uri_component(path)
        )
    }

    fn unix_url(path: &str, size: &str) -> String {
        format!(
            "thumb://localhost/{}?size={size}&m=1700000000000",
            encode_uri_component(path)
        )
    }

    #[test]
    fn parses_windows_form() {
        let r =
            parse_thumb_uri(&windows_url(r"C:\Users\Admin\Textures\hero.png", "small")).unwrap();
        assert_eq!(r.path, PathBuf::from(r"C:\Users\Admin\Textures\hero.png"));
        assert_eq!(r.size, ThumbSize::Small);
        let https = parse_thumb_uri("https://thumb.localhost/D%3A%5Ca.png?size=large").unwrap();
        assert_eq!(https.path, PathBuf::from(r"D:\a.png"));
        assert_eq!(https.size, ThumbSize::Large);
    }

    #[test]
    fn parses_unix_form() {
        let r = parse_thumb_uri(&unix_url("/home/me/tex/a.png", "large")).unwrap();
        assert_eq!(r.path, PathBuf::from("/home/me/tex/a.png"));
        assert_eq!(r.size, ThumbSize::Large);
    }

    #[test]
    fn unicode_and_spaces_round_trip() {
        for path in [
            r"C:\Người dùng\ảnh mới\sprite #1 (copy)&50%.png",
            "/Users/me/テクスチャ/my file+v2.png",
        ] {
            for url in [windows_url(path, "medium"), unix_url(path, "medium")] {
                let r = parse_thumb_uri(&url).unwrap();
                assert_eq!(r.path, PathBuf::from(path), "{url}");
                assert_eq!(r.size, ThumbSize::Medium);
            }
        }
    }

    #[test]
    fn defaults_and_rejections() {
        assert_eq!(
            parse_thumb_uri("thumb://localhost/%2Fa.png").unwrap().size,
            ThumbSize::Medium
        );
        assert_eq!(
            parse_thumb_uri("thumb://localhost/%2Fa.png?m=5#frag")
                .unwrap()
                .path,
            PathBuf::from("/a.png")
        );
        for bad in [
            "no-scheme",
            "thumb://localhost",
            "thumb://localhost/",
            "thumb://localhost/%2Fa.png?size=huge",
            "thumb://localhost/%FF",
        ] {
            assert_eq!(
                parse_thumb_uri(bad).unwrap_err().code,
                codes::INVALID_PARAMS,
                "{bad}"
            );
        }
    }

    #[test]
    fn responses() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ThumbCache::new(dir.path().join("thumbs"));
        let src = dir.path().join("ảnh có dấu.png");
        fixtures::gradient(400, 200).save(&src).unwrap();
        let url = unix_url(&src.display().to_string(), "small");

        let ok = thumb_response(&cache, &url);
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(ok.headers()[header::CONTENT_TYPE], THUMB_CONTENT_TYPE);
        assert!(
            ok.headers()[header::CACHE_CONTROL]
                .to_str()
                .unwrap()
                .contains("max-age")
        );
        let img = image::load_from_memory_with_format(ok.body(), image::ImageFormat::WebP).unwrap();
        assert_eq!((img.width(), img.height()), (128, 64));

        // Second request is served from the disk cache.
        assert_eq!(thumb_response(&cache, &url).status(), StatusCode::OK);
        assert_eq!(cache.generated_count(), 1);

        let missing = thumb_response(
            &cache,
            &unix_url(&dir.path().join("nope.png").display().to_string(), "small"),
        );
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let body: OpError = serde_json::from_slice(missing.body()).unwrap();
        assert_eq!(body.code, texopt_core::io::SCAN_PATH_NOT_FOUND);

        let corrupt = dir.path().join("bad.png");
        std::fs::write(&corrupt, b"nope").unwrap();
        let failed = thumb_response(
            &cache,
            &windows_url(&corrupt.display().to_string(), "small"),
        );
        assert_eq!(failed.status(), StatusCode::INTERNAL_SERVER_ERROR);

        assert_eq!(
            thumb_response(&cache, "garbage").status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn service_pool_runs_jobs_off_thread() {
        let service = ThumbService::new();
        let (tx, rx) = std::sync::mpsc::channel();
        let caller = std::thread::current().id();
        service.spawn(move || tx.send(std::thread::current().id()).unwrap());
        assert_ne!(rx.recv().unwrap(), caller);
        let dir = tempfile::tempdir().unwrap();
        let a = service.cache(|| dir.path().join("t"));
        let b = service.cache(|| unreachable!("initialized once"));
        assert!(Arc::ptr_eq(&a, &b));
    }
}
