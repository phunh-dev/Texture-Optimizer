//! Incremental export orchestration used by the app (Tauri independent).
//!
//! An atlas is identified by its output folder + base name. When
//! `<dir>/<base>.texatlas.json` exists it is the *previous* atlas:
//!
//! * sprites of the previous atlas that are not in the current input list are
//!   kept (their pixels are recovered from the previous page PNGs, see
//!   [`recover_sprite`]) unless [`IncrementalOptions::remove_missing`] is set;
//! * inputs whose name matches a previous sprite replace it;
//! * the merged set is built with the chosen [`IncrementalMode`] and every
//!   output (pages, metadata, project) is overwritten;
//! * files the previous export generated that the new one does not produce
//!   (e.g. `<base>_1.png` after the page count shrank, `.tres` of removed
//!   sprites) are deleted. Unknown files are never touched.
//!
//! App-level data (source paths, generated files) is stored in the project
//! file under the extra top-level key `appData`, which [`AtlasProject`]
//! ignores, so project files stay readable by the plain core API.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use image::imageops;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::exporters::{ExistingFiles, ExporterConfig, adapt_params, export, files_to_read, page_file_name};
use super::incremental::{AtlasProject, IncrementalMode, ProjectSprite};
use super::params::AtlasParams;
use super::{AtlasResult, SpriteInput, build, codes};
use crate::error::codes as core_codes;
use crate::output::write_atomic;
use crate::{ImageBuf, OpError, OpResult};

/// Suffix of the project file written next to the pages.
pub const PROJECT_SUFFIX: &str = ".texatlas.json";
/// Top-level key of [`ProjectAppData`] inside the project file.
pub const APP_DATA_KEY: &str = "appData";
/// Longest accepted base name (characters).
pub const MAX_BASE_NAME_LEN: usize = 120;

/// `<base>.texatlas.json`
pub fn project_file_name(base: &str) -> String {
    format!("{base}{PROJECT_SUFFIX}")
}

pub fn project_path(dir: &Path, base: &str) -> PathBuf {
    dir.join(project_file_name(base))
}

/// Sprite name used for an input file: its file stem.
pub fn sprite_name_from_path(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

const RESERVED_WINDOWS: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Base names must be valid file names on Windows, macOS and Linux.
/// Fails with `INVALID_PARAMS { param: "baseName", reason }`, reason one of
/// `empty`, `tooLong`, `invalidChars`, `trailingDotOrSpace`, `reserved`.
pub fn validate_base_name(base: &str) -> OpResult<()> {
    let fail = |reason: &str| Err(OpError::invalid_param("baseName", reason));
    if base.trim().is_empty() {
        return fail("empty");
    }
    if base.chars().count() > MAX_BASE_NAME_LEN {
        return fail("tooLong");
    }
    if base
        .chars()
        .any(|c| "\\/:*?\"<>|".contains(c) || c.is_control())
    {
        return fail("invalidChars");
    }
    if base.ends_with('.') || base.ends_with(' ') || base.starts_with(' ') {
        return fail("trailingDotOrSpace");
    }
    let stem = base.split('.').next().unwrap_or("").to_ascii_lowercase();
    if RESERVED_WINDOWS.contains(&stem.as_str()) {
        return fail("reserved");
    }
    Ok(())
}

/// How a build treats an existing atlas at the output location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct IncrementalOptions {
    pub mode: IncrementalMode,
    /// Sync mode: drop previous sprites that are not in the current inputs.
    pub remove_missing: bool,
}

/// App data stored in the project file next to the core fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectAppData {
    /// Sprite name (primary or alias) -> source file path at the last export.
    pub sources: BTreeMap<String, String>,
    /// Every file the last export wrote, relative to the output folder
    /// (forward slashes), excluding the project file itself.
    pub generated_files: Vec<String>,
    /// Exporter id of the last export (`genericJson`, `unity`, ...).
    pub exporter: Option<String>,
}

/// A project file: the core [`AtlasProject`] plus [`ProjectAppData`].
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectDocument {
    pub project: AtlasProject,
    pub app: ProjectAppData,
}

impl ProjectDocument {
    pub fn from_json(text: &str) -> OpResult<Self> {
        let project = AtlasProject::from_json(text)?;
        let app = serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|v| v.get(APP_DATA_KEY).cloned())
            .and_then(|a| serde_json::from_value(a).ok())
            .unwrap_or_default();
        Ok(Self { project, app })
    }

    pub fn to_json(&self) -> String {
        let mut v = serde_json::to_value(&self.project).unwrap_or(Value::Null);
        if let Value::Object(map) = &mut v {
            map.insert(
                APP_DATA_KEY.to_string(),
                serde_json::to_value(&self.app).unwrap_or(Value::Null),
            );
        }
        serde_json::to_string_pretty(&v).unwrap_or_default()
    }

    /// `Ok(None)` when the file does not exist.
    pub fn load(path: &Path) -> OpResult<Option<Self>> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_json(&text).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(OpError::new(core_codes::IO_READ_FAILED)
                .with("path", path.display().to_string())
                .with("detail", e.to_string())),
        }
    }
}

/// What happens to a sprite name when the inputs are merged with the previous atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpriteStatus {
    /// Only in the current inputs.
    New,
    /// In both: the input replaces the previous sprite.
    Replaced,
    /// Only in the previous atlas: kept (pixels recovered from its pages).
    Kept,
    /// Only in the previous atlas and dropped (sync mode, or not recoverable).
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanEntry {
    pub name: String,
    pub status: SpriteStatus,
    pub source_path: Option<String>,
}

/// Merge plan of `inputs` (`(name, source path)`, later duplicates win) with
/// the previous atlas, sorted by name.
pub fn plan_merge(
    previous: Option<&ProjectDocument>,
    inputs: &[(String, Option<String>)],
    remove_missing: bool,
) -> Vec<PlanEntry> {
    let mut entries: BTreeMap<String, PlanEntry> = BTreeMap::new();
    let prev_names: BTreeSet<String> = previous
        .map(|p| p.project.all_names().into_iter().collect())
        .unwrap_or_default();
    for (name, path) in inputs {
        let status = if prev_names.contains(name) {
            SpriteStatus::Replaced
        } else {
            SpriteStatus::New
        };
        entries.insert(
            name.clone(),
            PlanEntry {
                name: name.clone(),
                status,
                source_path: path.clone(),
            },
        );
    }
    if let Some(prev) = previous {
        for name in &prev_names {
            if entries.contains_key(name) {
                continue;
            }
            entries.insert(
                name.clone(),
                PlanEntry {
                    name: name.clone(),
                    status: if remove_missing {
                        SpriteStatus::Removed
                    } else {
                        SpriteStatus::Kept
                    },
                    source_path: prev.app.sources.get(name).cloned(),
                },
            );
        }
    }
    entries.into_values().collect()
}

fn unpremultiply(img: &mut ImageBuf) {
    for p in img.pixels_mut() {
        let a = u32::from(p.0[3]);
        if a == 0 {
            p.0 = [0, 0, 0, 0];
            continue;
        }
        for c in &mut p.0[..3] {
            *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
        }
    }
}

fn project_invalid(reason: &str, name: &str) -> OpError {
    OpError::new(codes::ATLAS_PROJECT_INVALID)
        .with("reason", reason)
        .with("name", name.to_string())
}

/// Rebuild the untrimmed source image of `sprite` from its page: crop
/// `frame`, undo the 90 degree clockwise rotation, un-premultiply when the
/// page was premultiplied, and place it at `spriteSourceSize` in a
/// transparent `sourceSize` canvas. Pixels removed by trimming come back as
/// transparent black (exact for trim threshold 0 with clean transparency).
pub fn recover_sprite(page: &ImageBuf, sprite: &ProjectSprite, premultiplied: bool) -> OpResult<ImageBuf> {
    let f = sprite.frame;
    if u64::from(f.x) + u64::from(f.w) > u64::from(page.width())
        || u64::from(f.y) + u64::from(f.h) > u64::from(page.height())
        || f.w == 0
        || f.h == 0
    {
        return Err(project_invalid("pageMismatch", &sprite.name));
    }
    let mut px = imageops::crop_imm(page, f.x, f.y, f.w, f.h).to_image();
    if sprite.rotated {
        px = imageops::rotate270(&px);
    }
    if premultiplied {
        unpremultiply(&mut px);
    }
    let ss = sprite.sprite_source_size;
    let src = sprite.source_size;
    if px.dimensions() != (ss.w, ss.h)
        || u64::from(ss.x) + u64::from(ss.w) > u64::from(src.w)
        || u64::from(ss.y) + u64::from(ss.h) > u64::from(src.h)
    {
        return Err(project_invalid("frameSize", &sprite.name));
    }
    if (ss.x, ss.y, ss.w, ss.h) == (0, 0, src.w, src.h) {
        return Ok(px);
    }
    let mut out = ImageBuf::new(src.w, src.h);
    imageops::replace(&mut out, &px, i64::from(ss.x), i64::from(ss.y));
    Ok(out)
}

/// One decoded input image.
#[derive(Debug, Clone)]
pub struct SourceSprite {
    pub name: String,
    pub path: Option<String>,
    pub image: Arc<ImageBuf>,
}

impl SourceSprite {
    pub fn new(name: impl Into<String>, path: Option<String>, image: impl Into<Arc<ImageBuf>>) -> Self {
        Self {
            name: name.into(),
            path,
            image: image.into(),
        }
    }

    /// Name from the file stem of `path`.
    pub fn from_path(path: &Path, image: impl Into<Arc<ImageBuf>>) -> Self {
        Self::new(
            sprite_name_from_path(path),
            Some(path.display().to_string()),
            image,
        )
    }
}

/// Decodes a previous page PNG (the app passes a cached loader).
pub type PageLoader<'a> = dyn Fn(&Path) -> OpResult<Arc<ImageBuf>> + 'a;

/// Output folder + base name of an atlas.
#[derive(Debug, Clone, Copy)]
pub struct AtlasTarget<'a> {
    pub dir: &'a Path,
    pub base_name: &'a str,
}

pub struct BuildRequest<'a> {
    pub sprites: Vec<SourceSprite>,
    pub params: &'a AtlasParams,
    pub exporter: &'a ExporterConfig,
    pub incremental: IncrementalOptions,
    /// Existing atlas to merge with; `None` builds a fresh atlas.
    pub target: Option<AtlasTarget<'a>>,
}

#[derive(Debug, Clone)]
pub struct BuildOutcome {
    pub result: AtlasResult,
    /// Params after [`adapt_params`].
    pub params: AtlasParams,
    /// Feature-disabled, recovery and packing warnings.
    pub warnings: Vec<OpError>,
    pub plan: Vec<PlanEntry>,
    pub previous: Option<ProjectDocument>,
    /// Source path of every name in the new atlas (when known).
    pub sources: BTreeMap<String, String>,
}

/// Merge the inputs with the previous atlas at `target` (if any) and pack.
pub fn build_incremental(req: BuildRequest, load_page: &PageLoader) -> OpResult<BuildOutcome> {
    let (params, mut warnings) = adapt_params(req.params, req.exporter);
    let previous = match req.target {
        Some(t) => {
            validate_base_name(t.base_name)?;
            ProjectDocument::load(&project_path(t.dir, t.base_name))?
        }
        None => None,
    };
    let remove_missing = req.incremental.remove_missing;
    let input_list: Vec<(String, Option<String>)> = req
        .sprites
        .iter()
        .map(|s| (s.name.clone(), s.path.clone()))
        .collect();
    let mut plan = plan_merge(previous.as_ref(), &input_list, remove_missing);
    let input_names: BTreeSet<&str> = req.sprites.iter().map(|s| s.name.as_str()).collect();

    let mut kept: Vec<SourceSprite> = Vec::new();
    let mut lost: BTreeSet<String> = BTreeSet::new();
    if let (Some(prev), Some(t)) = (&previous, req.target)
        && !remove_missing
    {
        let pages = &prev.project.pages;
        let mut loaded: BTreeMap<usize, Result<Arc<ImageBuf>, String>> = BTreeMap::new();
        for s in &prev.project.sprites {
            let names: Vec<&String> = std::iter::once(&s.name)
                .chain(&s.aliases)
                .filter(|n| !input_names.contains(n.as_str()))
                .collect();
            if names.is_empty() {
                continue;
            }
            let file = page_file_name(t.base_name, s.page, pages.len());
            let page_path = t.dir.join(&file);
            let page = loaded
                .entry(s.page)
                .or_insert_with(|| {
                    let info = pages[s.page];
                    match load_page(&page_path) {
                        Ok(img) if img.dimensions() == (info.width, info.height) => Ok(img),
                        Ok(_) => Err("pageMismatch".to_string()),
                        Err(e) => Err(e.code),
                    }
                })
                .clone();
            let recovered = page
                .and_then(|img| {
                    recover_sprite(&img, s, prev.project.params.premultiply_alpha)
                        .map_err(|e| e.code)
                })
                .map(Arc::new);
            match recovered {
                Ok(img) => {
                    for n in names {
                        kept.push(SourceSprite {
                            name: n.clone(),
                            path: prev.app.sources.get(n).cloned(),
                            image: img.clone(),
                        });
                    }
                }
                Err(reason) => {
                    for n in names {
                        warnings.push(
                            OpError::new(codes::ATLAS_SPRITE_NOT_RECOVERED)
                                .with("name", n.clone())
                                .with("path", page_path.display().to_string())
                                .with("reason", reason.clone()),
                        );
                        lost.insert(n.clone());
                    }
                }
            }
        }
    }
    for e in &mut plan {
        if lost.contains(&e.name) {
            e.status = SpriteStatus::Removed;
        }
    }

    let mut sources: BTreeMap<String, String> = BTreeMap::new();
    let mut inputs: Vec<SpriteInput> = Vec::with_capacity(req.sprites.len() + kept.len());
    for s in req.sprites.into_iter().chain(kept) {
        if let Some(p) = &s.path {
            sources.insert(s.name.clone(), p.clone());
        }
        let image = Arc::try_unwrap(s.image).unwrap_or_else(|a| (*a).clone());
        inputs.push(SpriteInput::new(s.name, image));
    }
    let result = build(
        inputs,
        &params,
        previous.as_ref().map(|p| &p.project),
        req.incremental.mode,
    )?;
    warnings.extend(result.warnings.iter().cloned());
    let names: BTreeSet<String> = result.project.all_names().into_iter().collect();
    sources.retain(|k, _| names.contains(k));
    Ok(BuildOutcome {
        result,
        params,
        warnings,
        plan,
        previous,
        sources,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageStats {
    pub width: u32,
    pub height: u32,
    /// Unique rects on the page.
    pub sprite_count: usize,
    /// Sum of the sprite frame areas (pixels).
    pub used_area: u64,
    /// `used_area / (width * height)`, 0..1.
    pub occupancy: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AtlasStats {
    pub pages: Vec<PageStats>,
    /// Unique rects.
    pub sprite_count: usize,
    /// Every exported name (rects + dedupe aliases).
    pub frame_count: usize,
    /// Over all pages, 0..1.
    pub occupancy: f64,
}

pub fn atlas_stats(project: &AtlasProject) -> AtlasStats {
    let mut pages: Vec<PageStats> = project
        .pages
        .iter()
        .map(|p| PageStats {
            width: p.width,
            height: p.height,
            sprite_count: 0,
            used_area: 0,
            occupancy: 0.0,
        })
        .collect();
    for s in &project.sprites {
        if let Some(p) = pages.get_mut(s.page) {
            p.sprite_count += 1;
            p.used_area += u64::from(s.frame.w) * u64::from(s.frame.h);
        }
    }
    let mut total_area = 0u64;
    let mut total_used = 0u64;
    for p in &mut pages {
        let area = u64::from(p.width) * u64::from(p.height);
        total_area += area;
        total_used += p.used_area;
        p.occupancy = if area > 0 { p.used_area as f64 / area as f64 } else { 0.0 };
    }
    AtlasStats {
        pages,
        sprite_count: project.sprites.len(),
        frame_count: project.all_names().len(),
        occupancy: if total_area > 0 {
            total_used as f64 / total_area as f64
        } else {
            0.0
        },
    }
}

/// A relative path made only of normal components (no root, `..`, drive).
fn is_safe_relative(rel: &str) -> bool {
    let p = Path::new(rel);
    !rel.is_empty() && p.components().all(|c| matches!(c, Component::Normal(_)))
}

/// Files of the previous export that the new export does not produce and
/// may therefore be deleted. Only paths recorded by the previous export (or,
/// for project files without that record, its page PNGs) are candidates;
/// comparisons are case-insensitive so nothing just written is removed on
/// case-insensitive file systems, and a Unity `.meta` survives while its
/// asset is still generated.
pub fn stale_files(previous: Option<&ProjectDocument>, base_name: &str, new_files: &[String]) -> Vec<String> {
    let Some(prev) = previous else {
        return Vec::new();
    };
    let candidates: Vec<String> = if prev.app.generated_files.is_empty() {
        let n = prev.project.pages.len();
        (0..n).map(|i| page_file_name(base_name, i, n)).collect()
    } else {
        prev.app.generated_files.clone()
    };
    let norm = |s: &str| s.replace('\\', "/").to_lowercase();
    let fresh: BTreeSet<String> = new_files.iter().map(|f| norm(f)).collect();
    let project_file = norm(&project_file_name(base_name));
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for c in candidates {
        let c = c.replace('\\', "/");
        let key = norm(&c);
        if !is_safe_relative(&c) || fresh.contains(&key) || key == project_file || !seen.insert(key.clone()) {
            continue;
        }
        if let Some(asset) = key.strip_suffix(".meta")
            && fresh.contains(asset)
        {
            continue;
        }
        out.push(c);
    }
    out
}

/// Progress stages reported by [`export_atlas`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportStage {
    Packing,
    Encoding,
    /// About to write this file.
    Writing(PathBuf),
    Cleaning,
}

pub struct ExportRequest<'a> {
    pub sprites: Vec<SourceSprite>,
    pub params: &'a AtlasParams,
    pub exporter: &'a ExporterConfig,
    pub incremental: IncrementalOptions,
    pub dir: &'a Path,
    pub base_name: &'a str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub project_path: PathBuf,
    /// Every written file (pages, metadata, then the project file).
    pub written: Vec<PathBuf>,
    /// Stale files of the previous export that were deleted.
    pub deleted: Vec<PathBuf>,
    pub warnings: Vec<OpError>,
    pub plan: Vec<PlanEntry>,
    pub stats: AtlasStats,
}

fn cancelled() -> OpError {
    OpError::new(core_codes::CANCELLED)
}

/// Full incremental export: merge, pack, export, write, clean up.
/// `is_cancelled` is checked between stages; nothing is written once it
/// returns true (the result is then `CANCELLED`).
pub fn export_atlas(
    req: ExportRequest,
    load_page: &PageLoader,
    on_stage: &mut dyn FnMut(ExportStage),
    is_cancelled: &dyn Fn() -> bool,
) -> OpResult<ExportReport> {
    validate_base_name(req.base_name)?;
    if req.dir.as_os_str().is_empty() {
        return Err(OpError::invalid_param("outputDir", "empty"));
    }
    on_stage(ExportStage::Packing);
    let outcome = build_incremental(
        BuildRequest {
            sprites: req.sprites,
            params: req.params,
            exporter: req.exporter,
            incremental: req.incremental,
            target: Some(AtlasTarget {
                dir: req.dir,
                base_name: req.base_name,
            }),
        },
        load_page,
    )?;
    if is_cancelled() {
        return Err(cancelled());
    }

    on_stage(ExportStage::Encoding);
    let mut existing = ExistingFiles::new();
    for rel in files_to_read(req.base_name, outcome.result.pages.len(), req.exporter) {
        if let Ok(bytes) = std::fs::read(req.dir.join(&rel)) {
            existing.insert(rel, bytes);
        }
    }
    let output = export(&outcome.result, req.base_name, req.exporter, &existing)?;
    if is_cancelled() {
        return Err(cancelled());
    }

    let mut warnings = outcome.warnings;
    warnings.extend(output.warnings.iter().cloned());
    let mut project = outcome.result.project;
    project.exporter_state = output.exporter_state;
    let generated: Vec<String> = output.files.iter().map(|(p, _)| p.replace('\\', "/")).collect();
    let doc = ProjectDocument {
        project,
        app: ProjectAppData {
            sources: outcome.sources,
            generated_files: generated.clone(),
            exporter: Some(req.exporter.id().to_string()),
        },
    };

    let mut written = Vec::with_capacity(output.files.len() + 1);
    for (rel, bytes) in &output.files {
        let path = req.dir.join(rel);
        on_stage(ExportStage::Writing(path.clone()));
        write_atomic(&path, bytes)?;
        written.push(path);
    }
    let project_file = project_path(req.dir, req.base_name);
    on_stage(ExportStage::Writing(project_file.clone()));
    write_atomic(&project_file, doc.to_json().as_bytes())?;
    written.push(project_file.clone());

    on_stage(ExportStage::Cleaning);
    let mut deleted = Vec::new();
    for rel in stale_files(outcome.previous.as_ref(), req.base_name, &generated) {
        let path = req.dir.join(&rel);
        match std::fs::symlink_metadata(&path) {
            Ok(m) if m.is_file() => match std::fs::remove_file(&path) {
                Ok(()) => deleted.push(path),
                Err(e) => warnings.push(
                    OpError::new(codes::ATLAS_STALE_DELETE_FAILED)
                        .with("path", path.display().to_string())
                        .with("detail", e.to_string()),
                ),
            },
            _ => {}
        }
    }

    Ok(ExportReport {
        project_path: project_file,
        written,
        deleted,
        warnings,
        plan: outcome.plan,
        stats: atlas_stats(&doc.project),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_name_rules() {
        for ok in ["atlas", "ui_icons", "hero.sprites", "Ätlas-2"] {
            assert!(validate_base_name(ok).is_ok(), "{ok}");
        }
        let reason = |b: &str| validate_base_name(b).unwrap_err().params["reason"].clone();
        assert_eq!(reason(""), "empty");
        assert_eq!(reason("   "), "empty");
        assert_eq!(reason("a/b"), "invalidChars");
        assert_eq!(reason("a:b"), "invalidChars");
        assert_eq!(reason("a?"), "invalidChars");
        assert_eq!(reason("atlas."), "trailingDotOrSpace");
        assert_eq!(reason("atlas "), "trailingDotOrSpace");
        assert_eq!(reason("CON"), "reserved");
        assert_eq!(reason("lpt1.x"), "reserved");
        assert_eq!(reason(&"a".repeat(MAX_BASE_NAME_LEN + 1)), "tooLong");
        assert_eq!(
            validate_base_name("").unwrap_err().params["param"],
            "baseName"
        );
    }

    #[test]
    fn safe_relative_paths() {
        assert!(is_safe_relative("a.png"));
        assert!(is_safe_relative("sub/a.tres"));
        assert!(!is_safe_relative("../a.png"));
        assert!(!is_safe_relative("/etc/passwd"));
        assert!(!is_safe_relative(""));
        assert!(!is_safe_relative("sub/../../a"));
    }

    #[test]
    fn unpremultiply_round_trips_opaque_and_clears_transparent() {
        let mut img = ImageBuf::from_pixel(1, 2, image::Rgba([10, 20, 30, 255]));
        img.put_pixel(0, 1, image::Rgba([5, 5, 5, 0]));
        unpremultiply(&mut img);
        assert_eq!(img.get_pixel(0, 0).0, [10, 20, 30, 255]);
        assert_eq!(img.get_pixel(0, 1).0, [0, 0, 0, 0]);
    }
}
