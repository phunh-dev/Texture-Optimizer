//! Output settings shared by every batch operation: where results go, in which
//! format, and how they are encoded and written.
//!
//! Documented choices:
//! - **In-place**: the result is written next to the input with the same stem.
//!   With `format: keep` this overwrites the source file (that is the point of
//!   in-place). When the format changes, the new file (`a.png` → `a.webp`) is
//!   written beside the source; source files are never deleted.
//! - **Conflict policy** is applied to existing files *other than the input
//!   itself* (overwriting the input is what in-place means). `autoRename` uses
//!   the engine-friendly `name_1.png`, `name_2.png`, ... scheme (no spaces or
//!   parentheses, which some asset pipelines dislike).
//! - **JPEG** has no alpha channel: translucent pixels are composited over
//!   **white** (what image viewers show for transparent areas) instead of
//!   exposing the arbitrary colors stored under alpha = 0.
//! - **WebP** is always encoded **lossless** (the pure-Rust encoder has no
//!   lossy mode), so `jpgQuality` only affects JPEG.
//! - **PNG optimization** (`optimizePng`) runs oxipng losslessly and keeps the
//!   smaller of the two encodings, so it never makes a file larger.
//! - Writes are atomic: data goes to a temp file in the target folder that is
//!   then renamed over the target.
//! - **Metadata sidecar** (`writeMeta`, default off): when an operation
//!   returns metadata (e.g. trim offsets), it is written as pretty JSON to
//!   `<output>.json` (`hero_opt.png` → `hero_opt.png.json`, see [`meta_path`]),
//!   overwriting an older sidecar of the same output.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use image::ImageEncoder;
use serde::{Deserialize, Serialize};

use crate::error::codes;
use crate::{ImageBuf, OpError, OpResult};

/// The target exists and the conflict policy is `skip`. Params: `path`.
pub const OUTPUT_EXISTS_SKIPPED: &str = "OUTPUT_EXISTS_SKIPPED";

/// Warning: the output was written in another format than "keep" implied.
/// Params: `path` (written file), `from`, `to` (extensions).
pub const OUTPUT_FORMAT_CHANGED: &str = "OUTPUT_FORMAT_CHANGED";

/// True when `keep` would write a JPEG for `input` but `img` has transparent
/// pixels, which JPEG would flatten onto white (e.g. after background
/// removal). An explicit `jpg` choice is respected and never reported here.
pub fn keep_would_drop_alpha(input: &Path, settings: &OutputSettings, img: &ImageBuf) -> bool {
    settings.format == OutputFormat::Keep
        && matches!(crate::io::extension_of(input).as_str(), "jpg" | "jpeg")
        && img.pixels().any(|p| p[3] < 255)
}

/// Highest `_N` suffix tried by `autoRename` before giving up.
const MAX_AUTO_RENAME: u32 = 99_999;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OutputMode {
    InPlace,
    Folder { path: String },
    Suffix { suffix: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputFormat {
    #[default]
    Keep,
    Png,
    Tga,
    Jpg,
    Webp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PngCompression {
    Fast,
    #[default]
    Default,
    Best,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictPolicy {
    Overwrite,
    Skip,
    #[default]
    AutoRename,
}

/// Mirrors the TS `OutputSettings`. Missing fields take the defaults below.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OutputSettings {
    pub mode: OutputMode,
    pub format: OutputFormat,
    pub png_compression: PngCompression,
    /// 1-100 (clamped), JPEG only.
    pub jpg_quality: u8,
    pub optimize_png: bool,
    pub conflict: ConflictPolicy,
    /// Write the op metadata (when there is any) to `<output>.json`.
    /// Omitted from the serialized form when false (backwards compatible).
    #[serde(skip_serializing_if = "is_false")]
    pub write_meta: bool,
}

fn is_false(v: &bool) -> bool {
    !*v
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self {
            mode: OutputMode::Suffix {
                suffix: "_opt".into(),
            },
            format: OutputFormat::Keep,
            png_compression: PngCompression::Default,
            jpg_quality: 90,
            optimize_png: false,
            conflict: ConflictPolicy::AutoRename,
            write_meta: false,
        }
    }
}

impl OutputSettings {
    /// Reject settings that can never produce a valid path.
    pub fn validate(&self) -> OpResult<()> {
        match &self.mode {
            OutputMode::InPlace => {}
            OutputMode::Folder { path } => {
                if path.trim().is_empty() {
                    return Err(OpError::invalid_param("output.mode.path", "empty"));
                }
            }
            OutputMode::Suffix { suffix } => {
                // An empty suffix would resolve to the input path itself and
                // silently overwrite the source.
                if suffix.trim().is_empty() {
                    return Err(OpError::invalid_param("output.mode.suffix", "empty"));
                }
                if suffix.contains(['/', '\\']) {
                    return Err(OpError::invalid_param(
                        "output.mode.suffix",
                        "path separator",
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Encoded file format, decided from the output path's extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeFormat {
    Png,
    Jpeg,
    Tga,
    Webp,
    Bmp,
}

impl EncodeFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "tga" => Some(Self::Tga),
            "webp" => Some(Self::Webp),
            "bmp" => Some(Self::Bmp),
            _ => None,
        }
    }
}

/// Extension (without dot) the output file gets. `keep` preserves the
/// input's extension verbatim (including its case, e.g. `.PNG`, `.jpeg`).
pub fn output_extension(input: &Path, format: OutputFormat) -> String {
    match format {
        OutputFormat::Keep => input
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_else(|| "png".into()),
        OutputFormat::Png => "png".into(),
        OutputFormat::Tga => "tga".into(),
        OutputFormat::Jpg => "jpg".into(),
        OutputFormat::Webp => "webp".into(),
    }
}

/// Where the output of `input` goes, before applying the conflict policy.
pub fn resolve_output_path(input: &Path, settings: &OutputSettings) -> OpResult<PathBuf> {
    settings.validate()?;
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or_else(|| OpError::invalid_param("input", "no file name"))?;
    let ext = output_extension(input, settings.format);
    let dir = input.parent().map(Path::to_path_buf).unwrap_or_default();
    Ok(match &settings.mode {
        OutputMode::InPlace => dir.join(format!("{stem}.{ext}")),
        OutputMode::Folder { path } => PathBuf::from(path).join(format!("{stem}.{ext}")),
        OutputMode::Suffix { suffix } => dir.join(format!("{stem}{suffix}.{ext}")),
    })
}

fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (dunce::canonicalize(a), dunce::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// `dir/stem_N.ext` for the given N.
fn numbered(target: &Path, n: u32) -> PathBuf {
    let stem = target
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match target.extension() {
        Some(ext) => format!("{stem}_{n}.{}", ext.to_string_lossy()),
        None => format!("{stem}_{n}"),
    };
    target.with_file_name(name)
}

/// Resolves output paths for a batch, applying the conflict policy against
/// both the file system and the paths already handed out in this batch (so
/// two inputs never race for the same output, even when run in parallel).
#[derive(Debug)]
pub struct OutputPlanner {
    settings: OutputSettings,
    reserved: Mutex<HashSet<PathBuf>>,
}

impl OutputPlanner {
    pub fn new(settings: OutputSettings) -> OpResult<Self> {
        settings.validate()?;
        Ok(Self {
            settings,
            reserved: Mutex::new(HashSet::new()),
        })
    }

    pub fn settings(&self) -> &OutputSettings {
        &self.settings
    }

    /// Final output path for `input`, or `OUTPUT_EXISTS_SKIPPED` when the
    /// policy is `skip` and the target is taken.
    pub fn plan(&self, input: &Path) -> OpResult<PathBuf> {
        self.plan_with(input, &self.settings)
    }

    /// Release `previous` (a path returned by [`plan`](Self::plan)) and plan
    /// `input` again as if the format were `format`.
    pub fn replan(&self, input: &Path, previous: &Path, format: OutputFormat) -> OpResult<PathBuf> {
        self.reserved
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(previous);
        let settings = OutputSettings {
            format,
            ..self.settings.clone()
        };
        self.plan_with(input, &settings)
    }

    fn plan_with(&self, input: &Path, settings: &OutputSettings) -> OpResult<PathBuf> {
        let target = resolve_output_path(input, settings)?;
        let mut reserved = self.reserved.lock().unwrap_or_else(|e| e.into_inner());
        let taken = |p: &Path, reserved: &HashSet<PathBuf>| {
            reserved.contains(p) || (p.exists() && !same_file(p, input))
        };
        if !taken(&target, &reserved) {
            reserved.insert(target.clone());
            return Ok(target);
        }
        match self.settings.conflict {
            ConflictPolicy::Overwrite if !reserved.contains(&target) => {
                reserved.insert(target.clone());
                Ok(target)
            }
            ConflictPolicy::Skip => {
                Err(OpError::new(OUTPUT_EXISTS_SKIPPED).with("path", target.display().to_string()))
            }
            // Overwrite of a path another input of this batch already claimed
            // falls back to renaming: two results must not clobber each other.
            ConflictPolicy::Overwrite | ConflictPolicy::AutoRename => {
                for n in 1..=MAX_AUTO_RENAME {
                    let candidate = numbered(&target, n);
                    if !taken(&candidate, &reserved) {
                        reserved.insert(candidate.clone());
                        return Ok(candidate);
                    }
                }
                Err(OpError::new(codes::IO_WRITE_FAILED)
                    .with("path", target.display().to_string())
                    .with("detail", "no free file name"))
            }
        }
    }
}

fn encode_error(path: &Path, e: &dyn std::fmt::Display) -> OpError {
    OpError::new(codes::IMG_ENCODE_FAILED)
        .with("path", path.display().to_string())
        .with("detail", e.to_string())
}

fn write_error(path: &Path, e: &dyn std::fmt::Display) -> OpError {
    OpError::new(codes::IO_WRITE_FAILED)
        .with("path", path.display().to_string())
        .with("detail", e.to_string())
}

/// Encode `img` as PNG with the given zlib effort.
pub fn encode_png(img: &ImageBuf, compression: PngCompression) -> image::ImageResult<Vec<u8>> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    let compression = match compression {
        PngCompression::Fast => CompressionType::Fast,
        PngCompression::Default => CompressionType::Default,
        PngCompression::Best => CompressionType::Best,
    };
    let mut out = Vec::new();
    PngEncoder::new_with_quality(&mut out, compression, FilterType::Adaptive).write_image(
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(out)
}

/// Lossless oxipng pass; returns whichever of input/optimized is smaller.
pub fn optimize_png(png: Vec<u8>) -> Vec<u8> {
    let options = oxipng::Options::from_preset(2);
    match oxipng::optimize_from_memory(&png, &options) {
        Ok(optimized) if optimized.len() < png.len() => optimized,
        _ => png,
    }
}

/// Composite straight-alpha RGBA over white and drop alpha (for JPEG).
pub fn flatten_on_white(img: &ImageBuf) -> image::RgbImage {
    image::RgbImage::from_fn(img.width(), img.height(), |x, y| {
        let [r, g, b, a] = img.get_pixel(x, y).0;
        let a = a as u32;
        let over = |c: u8| ((c as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
        image::Rgb([over(r), over(g), over(b)])
    })
}

/// Encode `img` in `format` according to `settings`.
pub fn encode_image(
    img: &ImageBuf,
    format: EncodeFormat,
    settings: &OutputSettings,
) -> image::ImageResult<Vec<u8>> {
    let (w, h) = img.dimensions();
    let mut out = Vec::new();
    match format {
        EncodeFormat::Png => {
            out = encode_png(img, settings.png_compression)?;
            if settings.optimize_png {
                out = optimize_png(out);
            }
        }
        EncodeFormat::Jpeg => {
            let rgb = flatten_on_white(img);
            let quality = settings.jpg_quality.clamp(1, 100);
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality).write_image(
                rgb.as_raw(),
                w,
                h,
                image::ExtendedColorType::Rgb8,
            )?;
        }
        EncodeFormat::Tga => {
            image::codecs::tga::TgaEncoder::new(&mut out).write_image(
                img.as_raw(),
                w,
                h,
                image::ExtendedColorType::Rgba8,
            )?;
        }
        EncodeFormat::Webp => {
            image::codecs::webp::WebPEncoder::new_lossless(&mut out).write_image(
                img.as_raw(),
                w,
                h,
                image::ExtendedColorType::Rgba8,
            )?;
        }
        EncodeFormat::Bmp => {
            image::codecs::bmp::BmpEncoder::new(&mut out).write_image(
                img.as_raw(),
                w,
                h,
                image::ExtendedColorType::Rgba8,
            )?;
        }
    }
    Ok(out)
}

/// Write `data` to `path` atomically (temp file in the same folder + rename).
/// Missing parent folders are created.
pub fn write_atomic(path: &Path, data: &[u8]) -> OpResult<()> {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    std::fs::create_dir_all(&dir).map_err(|e| write_error(path, &e))?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".texopt-")
        .suffix(".tmp")
        .tempfile_in(&dir)
        .map_err(|e| write_error(path, &e))?;
    tmp.write_all(data).map_err(|e| write_error(path, &e))?;
    tmp.as_file()
        .sync_all()
        .map_err(|e| write_error(path, &e))?;
    tmp.persist(path).map_err(|e| write_error(path, &e.error))?;
    Ok(())
}

/// Encode `img` according to `settings` (format from `path`'s extension) and
/// write it atomically.
pub fn save_image(img: &ImageBuf, path: &Path, settings: &OutputSettings) -> OpResult<()> {
    let ext = crate::io::extension_of(path);
    let format = EncodeFormat::from_extension(&ext).ok_or_else(|| {
        OpError::new(codes::IMG_UNSUPPORTED_FORMAT)
            .with("format", ext)
            .with("path", path.display().to_string())
    })?;
    let data = encode_image(img, format, settings).map_err(|e| encode_error(path, &e))?;
    write_atomic(path, &data)
}

/// Sidecar path of an output image's metadata: `<output>.json`.
pub fn meta_path(output: &Path) -> PathBuf {
    let mut name = output.as_os_str().to_os_string();
    name.push(".json");
    PathBuf::from(name)
}

/// Write `meta` as pretty JSON next to `output` (see [`meta_path`]) when
/// `settings.write_meta` is on and there is metadata. Returns the sidecar path
/// when one was written.
pub fn save_meta(
    output: &Path,
    meta: Option<&serde_json::Value>,
    settings: &OutputSettings,
) -> OpResult<Option<PathBuf>> {
    let Some(meta) = meta.filter(|_| settings.write_meta) else {
        return Ok(None);
    };
    let path = meta_path(output);
    let mut data = serde_json::to_vec_pretty(meta).map_err(|e| encode_error(&path, &e))?;
    data.push(b'\n');
    write_atomic(&path, &data)?;
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    fn settings(mode: OutputMode, format: OutputFormat) -> OutputSettings {
        OutputSettings {
            mode,
            format,
            ..OutputSettings::default()
        }
    }

    fn folder(p: &Path) -> OutputMode {
        OutputMode::Folder {
            path: p.display().to_string(),
        }
    }

    fn suffix(s: &str) -> OutputMode {
        OutputMode::Suffix { suffix: s.into() }
    }

    /// Image with varied colors and alpha (incl. fully transparent pixels).
    fn sample() -> ImageBuf {
        ImageBuf::from_fn(23, 17, |x, y| {
            image::Rgba([
                (x * 11) as u8,
                (y * 15) as u8,
                ((x + y) * 7) as u8,
                ((x * y * 5) % 256) as u8,
            ])
        })
    }

    #[test]
    fn empty_or_blank_suffix_is_rejected_so_sources_are_never_overwritten() {
        let input = PathBuf::from("assets").join("hero.png");
        for s in ["", "   "] {
            let err =
                resolve_output_path(&input, &settings(suffix(s), OutputFormat::Keep)).unwrap_err();
            assert_eq!(err.code, codes::INVALID_PARAMS);
            assert_eq!(err.params["param"], "output.mode.suffix");
            assert_eq!(err.params["reason"], "empty");
        }
    }

    #[test]
    fn serde_shape_matches_ts() {
        let json = serde_json::json!({
            "mode": { "kind": "folder", "path": "/out" },
            "format": "webp",
            "pngCompression": "best",
            "jpgQuality": 80,
            "optimizePng": true,
            "conflict": "autoRename"
        });
        let s: OutputSettings = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(
            s.mode,
            OutputMode::Folder {
                path: "/out".into()
            }
        );
        assert_eq!(s.format, OutputFormat::Webp);
        assert_eq!(s.png_compression, PngCompression::Best);
        assert_eq!(s.conflict, ConflictPolicy::AutoRename);
        assert_eq!(serde_json::to_value(&s).unwrap(), json);
        let in_place: OutputSettings =
            serde_json::from_str(r#"{"mode":{"kind":"inPlace"}}"#).unwrap();
        assert_eq!(in_place.mode, OutputMode::InPlace);
        let sfx: OutputSettings =
            serde_json::from_str(r#"{"mode":{"kind":"suffix","suffix":"_x"}}"#).unwrap();
        assert_eq!(sfx.mode, suffix("_x"));
    }

    #[test]
    fn path_resolution_for_every_mode_and_format() {
        let base = PathBuf::from("assets").join("tex");
        let input = base.join("hero.PNG");
        let out = PathBuf::from("out");
        let cases = [
            (OutputFormat::Keep, "PNG"),
            (OutputFormat::Png, "png"),
            (OutputFormat::Tga, "tga"),
            (OutputFormat::Jpg, "jpg"),
            (OutputFormat::Webp, "webp"),
        ];
        for (format, ext) in cases {
            assert_eq!(
                resolve_output_path(&input, &settings(OutputMode::InPlace, format)).unwrap(),
                base.join(format!("hero.{ext}"))
            );
            assert_eq!(
                resolve_output_path(&input, &settings(folder(&out), format)).unwrap(),
                out.join(format!("hero.{ext}"))
            );
            assert_eq!(
                resolve_output_path(&input, &settings(suffix("_opt"), format)).unwrap(),
                base.join(format!("hero_opt.{ext}"))
            );
        }
        // jpeg keeps its spelling with `keep`.
        let jpeg = base.join("a.b.jpeg");
        assert_eq!(
            resolve_output_path(&jpeg, &settings(suffix("-x"), OutputFormat::Keep)).unwrap(),
            base.join("a.b-x.jpeg")
        );
    }

    #[test]
    fn invalid_settings_are_rejected() {
        let input = Path::new("a.png");
        let err = resolve_output_path(
            input,
            &settings(OutputMode::Folder { path: " ".into() }, OutputFormat::Keep),
        )
        .unwrap_err();
        assert_eq!(err.code, codes::INVALID_PARAMS);
        let err =
            resolve_output_path(input, &settings(suffix("a/b"), OutputFormat::Keep)).unwrap_err();
        assert_eq!(err.code, codes::INVALID_PARAMS);
        assert!(
            OutputPlanner::new(settings(
                OutputMode::Folder {
                    path: String::new()
                },
                OutputFormat::Png
            ))
            .is_err()
        );
    }

    #[test]
    fn conflict_overwrite_skip_auto_rename() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in").join("a.png");
        let out = dir.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("a.png"), b"x").unwrap();
        std::fs::write(out.join("a_1.png"), b"x").unwrap();

        let mut s = settings(folder(&out), OutputFormat::Keep);
        s.conflict = ConflictPolicy::Overwrite;
        assert_eq!(
            OutputPlanner::new(s.clone()).unwrap().plan(&input).unwrap(),
            out.join("a.png")
        );

        s.conflict = ConflictPolicy::Skip;
        let err = OutputPlanner::new(s.clone())
            .unwrap()
            .plan(&input)
            .unwrap_err();
        assert_eq!(err.code, OUTPUT_EXISTS_SKIPPED);
        assert_eq!(err.params["path"], out.join("a.png").display().to_string());

        s.conflict = ConflictPolicy::AutoRename;
        let planner = OutputPlanner::new(s.clone()).unwrap();
        assert_eq!(planner.plan(&input).unwrap(), out.join("a_2.png"));
        // A second input mapping to the same name inside the batch gets the next free one.
        assert_eq!(
            planner
                .plan(&dir.path().join("other").join("a.png"))
                .unwrap(),
            out.join("a_3.png")
        );

        // No conflict -> the plain target, for every policy.
        for policy in [
            ConflictPolicy::Overwrite,
            ConflictPolicy::Skip,
            ConflictPolicy::AutoRename,
        ] {
            s.conflict = policy;
            assert_eq!(
                OutputPlanner::new(s.clone())
                    .unwrap()
                    .plan(&dir.path().join("b.png"))
                    .unwrap(),
                out.join("b.png")
            );
        }
    }

    #[test]
    fn overwrite_never_hands_out_the_same_path_twice_in_a_batch() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let mut s = settings(folder(&out), OutputFormat::Png);
        s.conflict = ConflictPolicy::Overwrite;
        let planner = OutputPlanner::new(s).unwrap();
        assert_eq!(
            planner.plan(&dir.path().join("a.png")).unwrap(),
            out.join("a.png")
        );
        assert_eq!(
            planner.plan(&dir.path().join("a.tga")).unwrap(),
            out.join("a_1.png")
        );
    }

    #[test]
    fn in_place_keep_overwrites_the_input_regardless_of_policy() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("a.png");
        fixtures::solid(2, 2, fixtures::RED).save(&input).unwrap();
        for policy in [
            ConflictPolicy::Overwrite,
            ConflictPolicy::Skip,
            ConflictPolicy::AutoRename,
        ] {
            let mut s = settings(OutputMode::InPlace, OutputFormat::Keep);
            s.conflict = policy;
            assert_eq!(OutputPlanner::new(s).unwrap().plan(&input).unwrap(), input);
        }
        // Changing the format in place writes a sibling; an existing sibling is a real conflict.
        std::fs::write(dir.path().join("a.webp"), b"x").unwrap();
        let mut s = settings(OutputMode::InPlace, OutputFormat::Webp);
        s.conflict = ConflictPolicy::AutoRename;
        assert_eq!(
            OutputPlanner::new(s).unwrap().plan(&input).unwrap(),
            dir.path().join("a_1.webp")
        );
    }

    fn reload(path: &Path) -> ImageBuf {
        image::open(path).unwrap().into_rgba8()
    }

    #[test]
    fn round_trip_lossless_formats_are_pixel_equal() {
        let dir = tempfile::tempdir().unwrap();
        let img = sample();
        for ext in ["png", "tga", "webp", "bmp"] {
            let path = dir.path().join(format!("rt.{ext}"));
            save_image(&img, &path, &OutputSettings::default()).unwrap();
            assert_eq!(reload(&path), img, "{ext}");
        }
        for compression in [
            PngCompression::Fast,
            PngCompression::Default,
            PngCompression::Best,
        ] {
            let s = OutputSettings {
                png_compression: compression,
                ..OutputSettings::default()
            };
            let path = dir.path().join("c.png");
            save_image(&img, &path, &s).unwrap();
            assert_eq!(reload(&path), img, "{compression:?}");
        }
    }

    #[test]
    fn jpeg_round_trip_keeps_dimensions_and_flattens_on_white() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rt.jpg");
        save_image(
            &sample(),
            &path,
            &OutputSettings {
                jpg_quality: 95,
                ..OutputSettings::default()
            },
        )
        .unwrap();
        assert_eq!(reload(&path).dimensions(), sample().dimensions());

        let clear = fixtures::solid(16, 16, fixtures::TRANSPARENT);
        let path = dir.path().join("clear.jpeg");
        save_image(&clear, &path, &OutputSettings::default()).unwrap();
        let px = reload(&path).get_pixel(8, 8).0;
        assert!(
            px[0] > 250 && px[1] > 250 && px[2] > 250,
            "transparent pixels become white, got {px:?}"
        );

        let low = dir.path().join("low.jpg");
        let high = dir.path().join("high.jpg");
        let img = fixtures::gradient(64, 64);
        save_image(
            &img,
            &low,
            &OutputSettings {
                jpg_quality: 10,
                ..OutputSettings::default()
            },
        )
        .unwrap();
        save_image(
            &img,
            &high,
            &OutputSettings {
                jpg_quality: 100,
                ..OutputSettings::default()
            },
        )
        .unwrap();
        assert!(std::fs::metadata(&low).unwrap().len() < std::fs::metadata(&high).unwrap().len());
    }

    #[test]
    fn optimize_png_is_lossless_and_not_larger() {
        let dir = tempfile::tempdir().unwrap();
        // Few colors -> oxipng can use a palette and must shrink it.
        let img = fixtures::checker(128, 128, 8, fixtures::RED, fixtures::TRANSPARENT);
        for compression in [PngCompression::Fast, PngCompression::Best] {
            let plain = dir.path().join("plain.png");
            let optimized = dir.path().join("opt.png");
            let base = OutputSettings {
                png_compression: compression,
                ..OutputSettings::default()
            };
            save_image(&img, &plain, &base).unwrap();
            save_image(
                &img,
                &optimized,
                &OutputSettings {
                    optimize_png: true,
                    ..base
                },
            )
            .unwrap();
            assert_eq!(reload(&optimized), img);
            let (p, o) = (
                std::fs::metadata(&plain).unwrap().len(),
                std::fs::metadata(&optimized).unwrap().len(),
            );
            assert!(o <= p, "optimized {o} > plain {p}");
        }
        // Noisy image with alpha: still pixel-identical (incl. RGB under alpha 0).
        let noisy = sample();
        let path = dir.path().join("noisy.png");
        save_image(
            &noisy,
            &path,
            &OutputSettings {
                optimize_png: true,
                ..OutputSettings::default()
            },
        )
        .unwrap();
        assert_eq!(reload(&path), noisy);
    }

    #[test]
    fn save_creates_folders_overwrites_atomically_and_leaves_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("deep").join("x.png");
        save_image(
            &fixtures::solid(4, 4, fixtures::RED),
            &path,
            &OutputSettings::default(),
        )
        .unwrap();
        save_image(
            &fixtures::solid(4, 4, fixtures::BLUE),
            &path,
            &OutputSettings::default(),
        )
        .unwrap();
        assert_eq!(reload(&path).get_pixel(0, 0), &fixtures::BLUE);
        let entries: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("x.png")]);
    }

    #[test]
    fn save_rejects_unknown_extension() {
        let dir = tempfile::tempdir().unwrap();
        let err = save_image(
            &fixtures::solid(1, 1, fixtures::RED),
            &dir.path().join("x.gif"),
            &OutputSettings::default(),
        )
        .unwrap_err();
        assert_eq!(err.code, codes::IMG_UNSUPPORTED_FORMAT);
    }

    #[test]
    fn write_meta_is_optional_and_backwards_compatible() {
        let s: OutputSettings = serde_json::from_str(r#"{"mode":{"kind":"inPlace"}}"#).unwrap();
        assert!(!s.write_meta, "defaults to false");
        assert!(
            serde_json::to_value(&s).unwrap().get("writeMeta").is_none(),
            "false is not serialized"
        );
        let on: OutputSettings = serde_json::from_str(r#"{"writeMeta":true}"#).unwrap();
        assert!(on.write_meta);
        assert_eq!(
            serde_json::to_value(&on).unwrap()["writeMeta"],
            serde_json::json!(true)
        );
    }

    #[test]
    fn meta_sidecar_path_and_contents() {
        assert_eq!(
            meta_path(Path::new("out").join("hero_opt.png").as_path()),
            Path::new("out").join("hero_opt.png.json")
        );
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("ảnh_opt.png");
        let meta = serde_json::json!({ "sourceSize": { "w": 20, "h": 10 }, "trimRect": { "x": -2, "y": 2, "w": 8, "h": 4 } });

        let off = OutputSettings::default();
        assert_eq!(save_meta(&out, Some(&meta), &off).unwrap(), None);
        assert!(!meta_path(&out).exists(), "off by default");

        let on = OutputSettings {
            write_meta: true,
            ..OutputSettings::default()
        };
        assert_eq!(
            save_meta(&out, None, &on).unwrap(),
            None,
            "no meta, no file"
        );
        assert!(!meta_path(&out).exists());

        let written = save_meta(&out, Some(&meta), &on).unwrap();
        assert_eq!(written, Some(dir.path().join("ảnh_opt.png.json")));
        let back: serde_json::Value =
            serde_json::from_slice(&std::fs::read(meta_path(&out)).unwrap()).unwrap();
        assert_eq!(back, meta);

        // A newer run overwrites the sidecar.
        let newer = serde_json::json!({ "n": 2 });
        save_meta(&out, Some(&newer), &on).unwrap();
        let back: serde_json::Value =
            serde_json::from_slice(&std::fs::read(meta_path(&out)).unwrap()).unwrap();
        assert_eq!(back, newer);
    }
}
