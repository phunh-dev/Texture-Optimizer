//! Image loading and import scanning.
//!
//! [`scan_paths`] expands the files/folders the user dropped or picked into a
//! flat, naturally sorted, de-duplicated list of [`ImportedFile`]s. Only the
//! image header is read (dimensions), never the pixel data, so scanning
//! thousands of textures stays fast.
//!
//! Rules:
//! - Explicitly listed files are accepted when their extension is in
//!   [`SUPPORTED_EXTENSIONS`]; otherwise they are reported in
//!   [`ScanResult::skipped`] with `IMG_UNSUPPORTED_FORMAT`.
//! - Folders are expanded (one level, or fully when `recursive`). Inside
//!   folders, files whose extension is not accepted (not supported, or not in
//!   [`ScanOptions::extensions`]) are ignored silently — folders of game
//!   assets are full of `.meta`, `.psd`, ... files that are not worth a
//!   warning. The extension filter only applies to folder expansion.
//! - Files with an accepted extension whose header cannot be decoded are
//!   reported with `IMG_DECODE_FAILED`, missing paths with
//!   `SCAN_PATH_NOT_FOUND`.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::error::codes;
use crate::{ImageBuf, OpError, OpResult};

/// File extensions the app can import (lowercase, without dot).
pub const SUPPORTED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tga", "bmp", "webp"];

/// A path given to [`scan_paths`] does not exist. Params: `path`.
pub const SCAN_PATH_NOT_FOUND: &str = "SCAN_PATH_NOT_FOUND";

pub fn load_image(path: &Path) -> OpResult<ImageBuf> {
    let img = image::ImageReader::open(path)
        .map_err(|e| io_read_error(path, &e))?
        .with_guessed_format()
        .map_err(|e| io_read_error(path, &e))?
        .decode()
        .map_err(|e| decode_error(path, &e))?;
    Ok(img.into_rgba8())
}

/// Options of [`scan_paths`] (mirrors the TS `ScanOptions`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScanOptions {
    /// Descend into sub-folders when a folder path is given.
    pub recursive: bool,
    /// Extensions to accept inside folders (case-insensitive, with or without
    /// a leading dot). `None` accepts every supported format.
    pub extensions: Option<Vec<String>>,
}

/// An image discovered by [`scan_paths`] (mirrors the TS `ImportedFile`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedFile {
    /// blake3 hex of the absolute, normalized path (see [`file_id`]).
    pub id: String,
    pub path: String,
    pub name: String,
    /// Lowercase extension without dot.
    pub ext: String,
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
    pub mtime_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedPath {
    pub path: String,
    pub error: OpError,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub files: Vec<ImportedFile>,
    pub skipped: Vec<SkippedPath>,
}

/// Stable id of a file: blake3 hex of its normalized absolute path string.
pub fn file_id(normalized_path: &str) -> String {
    blake3::hash(normalized_path.as_bytes())
        .to_hex()
        .to_string()
}

/// Absolute path with `.`/`..` and symlinks resolved and, on Windows, without
/// the `\\?\` prefix. Fails when the path does not exist.
pub fn normalize_path(path: &Path) -> std::io::Result<PathBuf> {
    dunce::canonicalize(path)
}

/// Lowercase extension of `path` without the dot (empty when none).
pub fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

pub fn is_supported_extension(ext: &str) -> bool {
    SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

/// Modification time in milliseconds since the Unix epoch (0 if unavailable).
pub fn mtime_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Expand `paths` (files and/or folders) into supported images. See the
/// module docs for the exact rules. Never fails as a whole: problems are
/// reported per path in [`ScanResult::skipped`].
pub fn scan_paths<P: AsRef<Path>>(paths: &[P], options: &ScanOptions) -> ScanResult {
    let filter: Option<HashSet<String>> = options.extensions.as_ref().map(|exts| {
        exts.iter()
            .map(|e| e.trim().trim_start_matches('.').to_lowercase())
            .collect()
    });
    let folder_accepts =
        |ext: &str| is_supported_extension(ext) && filter.as_ref().is_none_or(|f| f.contains(ext));

    let mut result = ScanResult::default();
    let mut seen: HashSet<String> = HashSet::new();
    let mut seen_skipped: HashSet<String> = HashSet::new();

    let mut skip = |result: &mut ScanResult, path: String, error: OpError| {
        if seen_skipped.insert(path.clone()) {
            result.skipped.push(SkippedPath { path, error });
        }
    };

    for raw in paths {
        let raw = raw.as_ref();
        let Ok(abs) = normalize_path(raw) else {
            let shown = raw.display().to_string();
            skip(
                &mut result,
                shown.clone(),
                OpError::new(SCAN_PATH_NOT_FOUND).with("path", shown),
            );
            continue;
        };

        if abs.is_dir() {
            let max_depth = if options.recursive { usize::MAX } else { 1 };
            for entry in walkdir::WalkDir::new(&abs)
                .min_depth(1)
                .max_depth(max_depth)
                .follow_links(true)
            {
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        let shown = e.path().unwrap_or(&abs).display().to_string();
                        skip(
                            &mut result,
                            shown.clone(),
                            OpError::new(codes::IO_READ_FAILED)
                                .with("path", shown)
                                .with("detail", e.to_string()),
                        );
                        continue;
                    }
                };
                if !entry.file_type().is_file() || !folder_accepts(&extension_of(entry.path())) {
                    continue;
                }
                match inspect_file(entry.path()) {
                    Ok(file) => {
                        if seen.insert(file.id.clone()) {
                            result.files.push(file);
                        }
                    }
                    Err(error) => skip(&mut result, entry.path().display().to_string(), error),
                }
            }
        } else {
            let ext = extension_of(&abs);
            if !is_supported_extension(&ext) {
                let shown = abs.display().to_string();
                skip(
                    &mut result,
                    shown.clone(),
                    OpError::new(codes::IMG_UNSUPPORTED_FORMAT)
                        .with("format", ext)
                        .with("path", shown),
                );
                continue;
            }
            match inspect_file(&abs) {
                Ok(file) => {
                    if seen.insert(file.id.clone()) {
                        result.files.push(file);
                    }
                }
                Err(error) => skip(&mut result, abs.display().to_string(), error),
            }
        }
    }

    result.files.sort_by(|a, b| natural_cmp(&a.path, &b.path));
    result
}

/// Read metadata + image header of one (already normalized) file.
pub fn inspect_file(path: &Path) -> OpResult<ImportedFile> {
    let abs = normalize_path(path).unwrap_or_else(|_| path.to_path_buf());
    let meta = std::fs::metadata(&abs).map_err(|e| io_read_error(&abs, &e))?;
    let (width, height) = image::ImageReader::open(&abs)
        .map_err(|e| io_read_error(&abs, &e))?
        .with_guessed_format()
        .map_err(|e| io_read_error(&abs, &e))?
        .into_dimensions()
        .map_err(|e| decode_error(&abs, &e))?;
    let path_str = abs.display().to_string();
    Ok(ImportedFile {
        id: file_id(&path_str),
        name: abs
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        ext: extension_of(&abs),
        width,
        height,
        size_bytes: meta.len(),
        mtime_ms: mtime_ms(&meta),
        path: path_str,
    })
}

pub(crate) fn io_read_error(path: &Path, e: &dyn std::fmt::Display) -> OpError {
    OpError::new(codes::IO_READ_FAILED)
        .with("path", path.display().to_string())
        .with("detail", e.to_string())
}

pub(crate) fn decode_error(path: &Path, e: &dyn std::fmt::Display) -> OpError {
    OpError::new(codes::IMG_DECODE_FAILED)
        .with("path", path.display().to_string())
        .with("detail", e.to_string())
}

/// Natural ("human") ordering: digit runs compare by numeric value, the rest
/// case-insensitively, so `img2` < `img10` and `B` sorts next to `b`. Ties
/// fall back to plain byte order to keep the ordering total.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut ia, mut ib) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ia.peek().copied(), ib.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let na = take_digits(&mut ia);
                let nb = take_digits(&mut ib);
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta
                    .len()
                    .cmp(&tb.len())
                    .then_with(|| ta.cmp(tb))
                    .then_with(|| na.len().cmp(&nb.len()));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(ca), Some(cb)) => {
                let ord = ca.to_lowercase().cmp(cb.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                ia.next();
                ib.next();
            }
        }
    }
}

fn take_digits(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut s = String::new();
    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
        s.push(c);
        it.next();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    fn write_png(path: &Path, w: u32, h: u32) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        fixtures::gradient(w, h)
            .save_with_format(path, image::ImageFormat::Png)
            .unwrap();
    }

    fn names(r: &ScanResult) -> Vec<String> {
        r.files.iter().map(|f| f.name.clone()).collect()
    }

    fn opts(recursive: bool) -> ScanOptions {
        ScanOptions {
            recursive,
            extensions: None,
        }
    }

    #[test]
    fn single_file_with_header_info() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hero.png");
        write_png(&p, 37, 21);
        let r = scan_paths(&[&p], &opts(false));
        assert!(r.skipped.is_empty(), "{:?}", r.skipped);
        assert_eq!(r.files.len(), 1);
        let f = &r.files[0];
        assert_eq!((f.width, f.height), (37, 21));
        assert_eq!(f.name, "hero.png");
        assert_eq!(f.ext, "png");
        assert_eq!(f.size_bytes, std::fs::metadata(&p).unwrap().len());
        assert!(f.mtime_ms > 0);
        let abs = normalize_path(&p).unwrap();
        assert_eq!(f.path, abs.display().to_string());
        assert_eq!(f.id, file_id(&f.path));
        assert_eq!(f.id.len(), 64);
    }

    #[test]
    fn dimensions_for_every_supported_format() {
        let dir = tempfile::tempdir().unwrap();
        let img = image::DynamicImage::ImageRgba8(fixtures::gradient(13, 7));
        let mut paths = vec![];
        for (ext, fmt) in [
            ("png", image::ImageFormat::Png),
            ("jpg", image::ImageFormat::Jpeg),
            ("tga", image::ImageFormat::Tga),
            ("bmp", image::ImageFormat::Bmp),
            ("webp", image::ImageFormat::WebP),
        ] {
            let p = dir.path().join(format!("img.{ext}"));
            let data = if fmt == image::ImageFormat::Jpeg {
                image::DynamicImage::ImageRgb8(img.to_rgb8())
            } else {
                img.clone()
            };
            data.save_with_format(&p, fmt).unwrap();
            paths.push(p);
        }
        let r = scan_paths(&paths, &opts(false));
        assert!(r.skipped.is_empty(), "{:?}", r.skipped);
        assert_eq!(r.files.len(), 5);
        for f in &r.files {
            assert_eq!((f.width, f.height), (13, 7), "{}", f.name);
        }
    }

    #[test]
    fn folder_non_recursive_vs_recursive() {
        let dir = tempfile::tempdir().unwrap();
        write_png(&dir.path().join("a.png"), 4, 4);
        write_png(&dir.path().join("b.png"), 4, 4);
        write_png(&dir.path().join("sub/c.png"), 4, 4);
        write_png(&dir.path().join("sub/deeper/d.png"), 4, 4);
        std::fs::write(dir.path().join("readme.txt"), "x").unwrap();
        std::fs::write(dir.path().join("a.png.meta"), "x").unwrap();

        let flat = scan_paths(&[dir.path()], &opts(false));
        assert_eq!(names(&flat), ["a.png", "b.png"]);
        assert!(
            flat.skipped.is_empty(),
            "unsupported files inside folders are ignored silently"
        );

        let deep = scan_paths(&[dir.path()], &opts(true));
        assert_eq!(names(&deep), ["a.png", "b.png", "c.png", "d.png"]);
        assert!(deep.skipped.is_empty());
    }

    #[test]
    fn extension_filter_is_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        write_png(&dir.path().join("upper.PNG"), 4, 4);
        write_png(&dir.path().join("lower.png"), 4, 4);
        fixtures::gradient(4, 4)
            .save_with_format(dir.path().join("t.tga"), image::ImageFormat::Tga)
            .unwrap();

        let only_png = ScanOptions {
            recursive: false,
            extensions: Some(vec!["PNG".into()]),
        };
        let r = scan_paths(&[dir.path()], &only_png);
        assert_eq!(names(&r), ["lower.png", "upper.PNG"]);
        assert!(r.files.iter().all(|f| f.ext == "png"));

        let only_tga = ScanOptions {
            recursive: false,
            extensions: Some(vec![".tga".into()]),
        };
        assert_eq!(names(&scan_paths(&[dir.path()], &only_tga)), ["t.tga"]);

        let all = scan_paths(&[dir.path()], &opts(false));
        assert_eq!(all.files.len(), 3);
    }

    #[test]
    fn unsupported_and_corrupt_files_are_skipped_with_codes() {
        let dir = tempfile::tempdir().unwrap();
        let txt = dir.path().join("notes.txt");
        std::fs::write(&txt, "hello").unwrap();
        let corrupt = dir.path().join("broken.png");
        std::fs::write(&corrupt, b"definitely not a png").unwrap();
        let good = dir.path().join("good.png");
        write_png(&good, 2, 2);

        let r = scan_paths(&[&txt, &corrupt, &good], &opts(false));
        assert_eq!(names(&r), ["good.png"]);
        assert_eq!(r.skipped.len(), 2);
        let code_of = |name: &str| {
            r.skipped
                .iter()
                .find(|s| s.path.ends_with(name))
                .unwrap()
                .error
                .code
                .clone()
        };
        assert_eq!(code_of("notes.txt"), codes::IMG_UNSUPPORTED_FORMAT);
        assert_eq!(code_of("broken.png"), codes::IMG_DECODE_FAILED);

        // Corrupt files found inside a folder are reported too.
        let r = scan_paths(&[dir.path()], &opts(false));
        assert_eq!(names(&r), ["good.png"]);
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].error.code, codes::IMG_DECODE_FAILED);
    }

    #[test]
    fn missing_path_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.png");
        let r = scan_paths(&[&missing], &opts(true));
        assert!(r.files.is_empty());
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].error.code, SCAN_PATH_NOT_FOUND);
        assert_eq!(
            r.skipped[0].error.params["path"],
            missing.display().to_string()
        );
    }

    #[test]
    fn duplicates_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.png");
        write_png(&p, 4, 4);
        let dotted = dir.path().join(".").join("a.png");
        let r = scan_paths(
            &[p.clone(), dotted, dir.path().to_path_buf(), p],
            &opts(true),
        );
        assert_eq!(names(&r), ["a.png"]);
    }

    #[test]
    fn ids_are_stable_and_distinct() {
        let dir = tempfile::tempdir().unwrap();
        write_png(&dir.path().join("a.png"), 4, 4);
        write_png(&dir.path().join("b.png"), 4, 4);
        let first = scan_paths(&[dir.path()], &opts(false));
        let second = scan_paths(
            &[dir.path().join("b.png"), dir.path().join("a.png")],
            &opts(false),
        );
        assert_eq!(first.files, second.files);
        assert_ne!(first.files[0].id, first.files[1].id);
    }

    #[test]
    fn results_are_sorted_naturally() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["img10.png", "img2.png", "Img1.png", "img002.png"] {
            write_png(&dir.path().join(n), 1, 1);
        }
        let r = scan_paths(&[dir.path()], &opts(false));
        assert_eq!(
            names(&r),
            ["Img1.png", "img2.png", "img002.png", "img10.png"]
        );
    }

    #[test]
    fn natural_cmp_basics() {
        assert_eq!(natural_cmp("a2", "a10"), Ordering::Less);
        assert_eq!(natural_cmp("a10", "a2"), Ordering::Greater);
        assert_eq!(natural_cmp("a", "a1"), Ordering::Less);
        assert_eq!(natural_cmp("B", "a"), Ordering::Greater);
        assert_eq!(natural_cmp("x", "x"), Ordering::Equal);
    }

    #[test]
    fn load_image_reports_codes() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.png");
        assert_eq!(
            load_image(&missing).unwrap_err().code,
            codes::IO_READ_FAILED
        );
        let corrupt = dir.path().join("c.png");
        std::fs::write(&corrupt, b"garbage").unwrap();
        assert_eq!(
            load_image(&corrupt).unwrap_err().code,
            codes::IMG_DECODE_FAILED
        );
        let good = dir.path().join("g.png");
        write_png(&good, 3, 5);
        assert_eq!(load_image(&good).unwrap().dimensions(), (3, 5));
    }
}
