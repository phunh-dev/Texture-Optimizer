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
//!
//! `size=full` serves the full-resolution original instead of a thumbnail
//! (the "before" image of previews): formats the webview can display (PNG,
//! JPEG, WebP, BMP) are served byte-for-byte, others (TGA) are decoded and
//! re-encoded as PNG. Only supported image extensions are served.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use percent_encoding::percent_decode_str;
use tauri::http::{Response, StatusCode, header};
use texopt_core::OpError;
use texopt_core::error::codes;
use texopt_core::thumbs::{THUMB_CONTENT_TYPE, ThumbCache, ThumbSize};

pub const SCHEME: &str = "thumb";

/// `size` query value requesting the full-resolution original.
pub const FULL_SIZE: &str = "full";

/// Extensions served byte-for-byte by `size=full` (the webview decodes them).
const PASSTHROUGH: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("bmp", "image/bmp"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbRequest {
    pub path: PathBuf,
    pub size: ThumbSize,
    /// `size=full`: serve the original image (`size` then stays `Medium`).
    pub full: bool,
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
    let mut full = false;
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key == "size" {
            full = value == FULL_SIZE;
            if !full {
                size = ThumbSize::parse(value).ok_or_else(|| bad("size"))?;
            }
        }
    }
    Ok(ThumbRequest {
        path: PathBuf::from(decoded.into_owned()),
        size,
        full,
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

fn ok_response(content_type: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(body)
        .expect("static response parts are valid")
}

/// `size=full`: the original image at full resolution. 404 when missing,
/// 415 for non-image extensions, 500 when reading/decoding fails.
pub fn full_image_response(path: &Path) -> Response<Vec<u8>> {
    let shown = path.display().to_string();
    if !path.is_file() {
        return error_response(
            StatusCode::NOT_FOUND,
            &OpError::new(texopt_core::io::SCAN_PATH_NOT_FOUND).with("path", shown),
        );
    }
    let ext = texopt_core::io::extension_of(path);
    if !texopt_core::io::is_supported_extension(&ext) {
        return error_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            &OpError::new(codes::IMG_UNSUPPORTED_FORMAT)
                .with("format", ext)
                .with("path", shown),
        );
    }
    if let Some((_, content_type)) = PASSTHROUGH.iter().find(|(e, _)| *e == ext) {
        return match std::fs::read(path) {
            Ok(body) => ok_response(content_type, body),
            Err(e) => error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &OpError::new(codes::IO_READ_FAILED)
                    .with("path", shown)
                    .with("detail", e.to_string()),
            ),
        };
    }
    let encoded = texopt_core::io::load_image(path).and_then(|img| {
        texopt_core::output::encode_png(&img, texopt_core::output::PngCompression::Fast).map_err(
            |e| {
                OpError::new(codes::IMG_ENCODE_FAILED)
                    .with("path", shown.clone())
                    .with("detail", e.to_string())
            },
        )
    });
    match encoded {
        Ok(body) => ok_response("image/png", body),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

/// Full request handling (blocking: may decode and encode an image).
/// 200 + WebP on success, 400 for malformed URLs, 404 when the source file
/// is missing, 500 when decoding/encoding fails. Error bodies are the JSON
/// `{ code, params }` of the failure. `size=full` → [`full_image_response`].
pub fn thumb_response(cache: &ThumbCache, uri: &str) -> Response<Vec<u8>> {
    let request = match parse_thumb_uri(uri) {
        Ok(r) => r,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, &e),
    };
    if request.full {
        return full_image_response(&request.path);
    }
    let cached = match cache.get_or_create(&request.path, request.size) {
        Ok(p) => p,
        Err(e) if e.code == texopt_core::io::SCAN_PATH_NOT_FOUND => {
            return error_response(StatusCode::NOT_FOUND, &e);
        }
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
    };
    match std::fs::read(&cached) {
        Ok(body) => ok_response(THUMB_CONTENT_TYPE, body),
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
    fn parses_full_size() {
        let r = parse_thumb_uri(&windows_url(r"C:\tex\a.tga", "full")).unwrap();
        assert!(r.full);
        assert_eq!(r.path, PathBuf::from(r"C:\tex\a.tga"));
        assert!(!parse_thumb_uri(&unix_url("/a.png", "small")).unwrap().full);
        assert!(!parse_thumb_uri("thumb://localhost/%2Fa.png").unwrap().full);
    }

    #[test]
    fn full_size_serves_originals() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ThumbCache::new(dir.path().join("thumbs"));

        // Displayable formats: the exact original bytes, never a thumbnail.
        let png = dir.path().join("big ảnh.png");
        fixtures::gradient(700, 300).save(&png).unwrap();
        let res = thumb_response(&cache, &unix_url(&png.display().to_string(), "full"));
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(res.body(), &std::fs::read(&png).unwrap());
        assert_eq!(cache.generated_count(), 0, "no thumbnail generated");

        let jpg = dir.path().join("photo.JPG");
        image::DynamicImage::ImageRgba8(fixtures::gradient(40, 20))
            .to_rgb8()
            .save_with_format(&jpg, image::ImageFormat::Jpeg)
            .unwrap();
        let res = thumb_response(&cache, &windows_url(&jpg.display().to_string(), "full"));
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(res.body(), &std::fs::read(&jpg).unwrap());

        // TGA is not displayable: decoded and served as a full-size PNG.
        let tga = dir.path().join("sprite.tga");
        let src = fixtures::gradient(33, 17);
        src.save(&tga).unwrap();
        let res = thumb_response(&cache, &unix_url(&tga.display().to_string(), "full"));
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()[header::CONTENT_TYPE], "image/png");
        let decoded = image::load_from_memory_with_format(res.body(), image::ImageFormat::Png)
            .unwrap()
            .into_rgba8();
        assert_eq!(decoded, src);

        let missing = thumb_response(
            &cache,
            &unix_url(&dir.path().join("nope.png").display().to_string(), "full"),
        );
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        let text = dir.path().join("secret.txt");
        std::fs::write(&text, b"not an image").unwrap();
        let refused = thumb_response(&cache, &unix_url(&text.display().to_string(), "full"));
        assert_eq!(refused.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        let body: OpError = serde_json::from_slice(refused.body()).unwrap();
        assert_eq!(body.code, codes::IMG_UNSUPPORTED_FORMAT);

        let corrupt = dir.path().join("bad.tga");
        std::fs::write(&corrupt, b"nope").unwrap();
        let failed = thumb_response(&cache, &unix_url(&corrupt.display().to_string(), "full"));
        assert_eq!(failed.status(), StatusCode::INTERNAL_SERVER_ERROR);
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
