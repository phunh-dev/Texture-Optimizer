//! Smart Atlas commands: `atlas_preview`, `atlas_export`, `atlas_load_project`.
//!
//! The orchestration (merge with the previous atlas, recovery, export,
//! cleanup) lives in `texopt_core::atlas::workflow`; this module only decodes
//! inputs through the tab's session cache, packs the preview payload and
//! drives the job events.
//!
//! `atlas_preview` payload (binary, reaches the webview as an ArrayBuffer like
//! `preview_op`, so multi-megabyte page PNGs are neither base64-inflated by a
//! third nor parsed as JSON strings):
//! `headerLen (u32 LE) | header (UTF-8 JSON, headerLen bytes) | page PNGs`
//! where `header.pages[i].byteLength` gives each PNG's length, in page order.
//! Decoded by `decodeAtlasPreview` in `src/tabs/atlas/ipc.ts`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use rayon::prelude::*;
use serde::Serialize;
use serde_json::{Value, json};
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, State};
use texopt_core::atlas::workflow::{
    AtlasStats, AtlasTarget, BuildOutcome, BuildRequest, ExportReport, ExportRequest, ExportStage,
    IncrementalOptions, PlanEntry, ProjectDocument, SourceSprite, SpriteStatus, atlas_stats,
    build_incremental, export_atlas, plan_merge, project_path, sprite_name_from_path,
    validate_base_name,
};
use texopt_core::atlas::{AtlasParams, ExporterConfig, Frame, PageInfo, Size};
use texopt_core::error::codes;
use texopt_core::output::{PngCompression, encode_png};
use texopt_core::{ImageBuf, OpError, OpResult};

use crate::commands::AppState;
use crate::error::{AppError, parse_arg};
use crate::jobs::{
    JOB_FINISHED_EVENT, JOB_PROGRESS_EVENT, JobFileResult, JobFinishedEvent, JobProgressEvent,
    PROGRESS_INTERVAL,
};
use crate::session::SessionCache;

// ---------------------------------------------------------------------------
// Tauri-independent helpers (unit tested below)

/// Decode every input through the session cache (parallel). Sprite names are
/// the file stems. `on_loaded` runs after each file; `is_cancelled` is
/// checked before each decode.
pub fn load_sources(
    cache: &SessionCache,
    tab: &str,
    paths: &[String],
    on_loaded: &(dyn Fn(&str) + Sync),
    is_cancelled: &(dyn Fn() -> bool + Sync),
) -> OpResult<Vec<SourceSprite>> {
    paths
        .par_iter()
        .map(|p| {
            if is_cancelled() {
                return Err(OpError::new(codes::CANCELLED));
            }
            let path = Path::new(p);
            let img = cache.get_or_load(tab, path, texopt_core::io::load_image)?;
            on_loaded(p);
            Ok(SourceSprite::new(
                sprite_name_from_path(path),
                Some(p.clone()),
                img,
            ))
        })
        .collect()
}

/// Folder + base name when both are usable for merging, else `None`.
pub fn preview_target<'a>(dir: Option<&'a str>, base: Option<&'a str>) -> Option<AtlasTarget<'a>> {
    match (dir, base) {
        (Some(d), Some(b)) if !d.trim().is_empty() && validate_base_name(b).is_ok() => {
            Some(AtlasTarget {
                dir: Path::new(d),
                base_name: b,
            })
        }
        _ => None,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPage {
    pub width: u32,
    pub height: u32,
    pub byte_length: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewFrame {
    pub name: String,
    pub aliases: Vec<String>,
    pub page: usize,
    pub frame: Frame,
    pub rotated: bool,
    pub trimmed: bool,
    pub source_size: Size,
    pub sprite_source_size: Frame,
    pub source_path: Option<String>,
    /// Merge status of the primary name (`None` when there is no previous atlas).
    pub status: Option<SpriteStatus>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewHeader {
    pub pages: Vec<PreviewPage>,
    pub frames: Vec<PreviewFrame>,
    pub stats: AtlasStats,
    pub warnings: Vec<OpError>,
    pub plan: Vec<PlanEntry>,
    pub has_previous: bool,
    /// Params actually used (after exporter adaptation).
    pub params: AtlasParams,
}

/// Pack a build outcome into the binary `atlas_preview` payload.
pub fn encode_atlas_preview(outcome: &BuildOutcome) -> OpResult<Vec<u8>> {
    let pngs: Vec<Vec<u8>> = outcome
        .result
        .pages
        .par_iter()
        .map(|img: &ImageBuf| {
            encode_png(img, PngCompression::Fast).map_err(|e| {
                OpError::new(codes::IMG_ENCODE_FAILED)
                    .with("path", "atlas preview")
                    .with("detail", e.to_string())
            })
        })
        .collect::<OpResult<_>>()?;
    let project = &outcome.result.project;
    let has_previous = outcome.previous.is_some();
    let frames = project
        .sprites
        .iter()
        .map(|s| PreviewFrame {
            name: s.name.clone(),
            aliases: s.aliases.clone(),
            page: s.page,
            frame: s.frame,
            rotated: s.rotated,
            trimmed: s.trimmed,
            source_size: s.source_size,
            sprite_source_size: s.sprite_source_size,
            source_path: outcome.sources.get(&s.name).cloned(),
            status: if has_previous {
                outcome
                    .plan
                    .iter()
                    .find(|e| e.name == s.name)
                    .map(|e| e.status)
            } else {
                None
            },
        })
        .collect();
    let header = PreviewHeader {
        pages: project
            .pages
            .iter()
            .zip(&pngs)
            .map(|(p, png)| PreviewPage {
                width: p.width,
                height: p.height,
                byte_length: png.len(),
            })
            .collect(),
        frames,
        stats: atlas_stats(project),
        warnings: outcome.warnings.clone(),
        plan: outcome.plan.clone(),
        has_previous,
        params: outcome.params.clone(),
    };
    let header = serde_json::to_vec(&header).map_err(|e| {
        OpError::new(codes::IMG_ENCODE_FAILED)
            .with("path", "atlas preview")
            .with("detail", e.to_string())
    })?;
    let total = 4 + header.len() + pngs.iter().map(Vec::len).sum::<usize>();
    let mut buf = Vec::with_capacity(total);
    buf.extend_from_slice(&(header.len() as u32).to_le_bytes());
    buf.extend_from_slice(&header);
    for png in &pngs {
        buf.extend_from_slice(png);
    }
    Ok(buf)
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub path: String,
    pub pages: Vec<PageInfo>,
    /// Unique rects.
    pub sprite_count: usize,
    /// Every name including dedupe aliases.
    pub frame_count: usize,
    pub exporter: Option<String>,
    pub params: AtlasParams,
    pub stats: AtlasStats,
    /// What the current inputs would do to it.
    pub plan: Vec<PlanEntry>,
}

/// Summary of the project file at `path` (`None` when it does not exist),
/// with the merge plan for `inputs`.
pub fn project_summary(
    path: &Path,
    inputs: &[String],
    remove_missing: bool,
) -> OpResult<Option<ProjectSummary>> {
    let Some(doc) = ProjectDocument::load(path)? else {
        return Ok(None);
    };
    let list: Vec<(String, Option<String>)> = inputs
        .iter()
        .map(|p| (sprite_name_from_path(Path::new(p)), Some(p.clone())))
        .collect();
    let plan = plan_merge(Some(&doc), &list, remove_missing);
    Ok(Some(ProjectSummary {
        path: path.display().to_string(),
        pages: doc.project.pages.clone(),
        sprite_count: doc.project.sprites.len(),
        frame_count: doc.project.all_names().len(),
        exporter: doc.app.exporter.clone(),
        params: doc.project.params.clone(),
        stats: atlas_stats(&doc.project),
        plan,
    }))
}

/// `job://finished` results of a successful export: one entry per written
/// file, the project file (image only: the first page) first carrying the
/// export summary as `meta` (`{ kind: "atlasExport", outputDir, projectPath
/// (null for image only), written, deleted, warnings, stats, plan }`).
pub fn export_results(dir: &Path, report: &ExportReport) -> Vec<JobFileResult> {
    let head = report
        .project_path
        .as_ref()
        .or_else(|| report.written.first())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| dir.display().to_string());
    let summary = json!({
        "kind": "atlasExport",
        "outputDir": dir.display().to_string(),
        "projectPath": report.project_path.as_ref().map(|p| p.display().to_string()),
        "written": report.written.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "deleted": report.deleted.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "warnings": report.warnings,
        "stats": report.stats,
        "plan": report.plan,
    });
    let mut results = vec![JobFileResult {
        input: head.clone(),
        output: Some(head.clone()),
        error: None,
        meta: Some(summary),
    }];
    results.extend(
        report
            .written
            .iter()
            .filter(|p| p.display().to_string() != head)
            .map(|p| JobFileResult {
                input: head.clone(),
                output: Some(p.display().to_string()),
                error: None,
                meta: None,
            }),
    );
    results
}

/// `input` of the single result reported when an export fails: the project
/// file, or for the image-only exporter (which writes none) the main page.
pub fn failure_input(dir: &Path, base_name: &str, exporter: &ExporterConfig) -> String {
    if exporter.writes_metadata() {
        project_path(dir, base_name).display().to_string()
    } else {
        dir.join(format!("{base_name}.png")).display().to_string()
    }
}

/// Progress of an export job: `total = inputs + 3` (pack, encode, write).
pub struct ExportProgress<F: Fn(usize, usize, Option<String>) + Sync> {
    emit: F,
    total: usize,
    done: AtomicUsize,
    last: Mutex<Option<Instant>>,
}

impl<F: Fn(usize, usize, Option<String>) + Sync> ExportProgress<F> {
    pub fn new(inputs: usize, emit: F) -> Self {
        Self {
            emit,
            total: inputs + 3,
            done: AtomicUsize::new(0),
            last: Mutex::new(None),
        }
    }

    pub fn total(&self) -> usize {
        self.total
    }

    /// One input decoded (throttled to `PROGRESS_INTERVAL`).
    pub fn loaded(&self, path: &str) {
        let done = self.done.fetch_add(1, Ordering::SeqCst) + 1;
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if last.is_none_or(|l| now.duration_since(l) >= PROGRESS_INTERVAL) {
            *last = Some(now);
            (self.emit)(done, self.total, Some(path.to_string()));
        }
    }

    /// A stage started: `done` jumps to `inputs + stage_index` (always emitted).
    pub fn stage(&self, stage: &ExportStage, page_hint: &Path) {
        let inputs = self.total - 3;
        let (done, current) = match stage {
            ExportStage::Packing => (inputs, page_hint.display().to_string()),
            ExportStage::Encoding => (inputs + 1, page_hint.display().to_string()),
            ExportStage::Writing(p) => (inputs + 2, p.display().to_string()),
            ExportStage::Cleaning => (inputs + 2, page_hint.display().to_string()),
        };
        self.done.store(done, Ordering::SeqCst);
        (self.emit)(done, self.total, Some(current));
    }
}

fn parse_incremental(v: Option<Value>) -> Result<IncrementalOptions, AppError> {
    match v {
        Some(Value::Null) | None => Ok(IncrementalOptions::default()),
        Some(v) => parse_arg("incrementalOptions", v),
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(AppError::unknown)?
}

// ---------------------------------------------------------------------------
// Commands

/// Build the atlas in memory (merged with the existing atlas at
/// `outputDir`/`baseName` when given) and return the binary payload
/// described in the module docs.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn atlas_preview(
    app: AppHandle,
    tab_id: String,
    paths: Vec<String>,
    params: Value,
    exporter: Value,
    incremental_options: Option<Value>,
    output_dir: Option<String>,
    base_name: Option<String>,
) -> Result<Response, AppError> {
    let params: AtlasParams = parse_arg("params", params)?;
    let exporter: ExporterConfig = parse_arg("exporter", exporter)?;
    let incremental = parse_incremental(incremental_options)?;
    blocking(move || {
        let state = tauri::Manager::state::<AppState>(&app);
        let cache = &state.sessions;
        let sprites = load_sources(cache, &tab_id, &paths, &|_| {}, &|| false)?;
        let page_loader = |p: &Path| cache.get_or_load(&tab_id, p, texopt_core::io::load_image);
        let outcome = build_incremental(
            BuildRequest {
                sprites,
                params: &params,
                exporter: &exporter,
                incremental,
                target: preview_target(output_dir.as_deref(), base_name.as_deref()),
            },
            &page_loader,
        )?;
        Ok(Response::new(encode_atlas_preview(&outcome)?))
    })
    .await
}

/// Start an export job; returns its id. Progress arrives as `job://progress`
/// (inputs, then pack / encode / write) and the outcome as `job://finished`
/// (see [`export_results`]; on failure a single result carrying the error).
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn atlas_export(
    app: AppHandle,
    state: State<'_, AppState>,
    tab_id: String,
    paths: Vec<String>,
    params: Value,
    exporter: Value,
    output_dir: String,
    base_name: String,
    incremental_options: Option<Value>,
) -> Result<String, AppError> {
    let params: AtlasParams = parse_arg("params", params)?;
    let exporter: ExporterConfig = parse_arg("exporter", exporter)?;
    let incremental = parse_incremental(incremental_options)?;
    validate_base_name(&base_name)?;
    if output_dir.trim().is_empty() {
        return Err(OpError::invalid_param("outputDir", "empty").into());
    }
    let (job_id, cancel) = state.jobs.create();
    let jobs = state.jobs.clone();
    let id = job_id.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let state = tauri::Manager::state::<AppState>(&app);
        let dir = PathBuf::from(&output_dir);
        let emit = |done: usize, total: usize, current: Option<String>| {
            let event = JobProgressEvent {
                job_id: id.clone(),
                tab_id: tab_id.clone(),
                done,
                total,
                current_path: current,
            };
            if let Err(e) = app.emit(JOB_PROGRESS_EVENT, event) {
                log::warn!("failed to emit progress: {e}");
            }
        };
        let progress = ExportProgress::new(paths.len(), emit);
        let outcome = run_export(
            &state.sessions,
            &tab_id,
            &paths,
            &params,
            &exporter,
            incremental,
            &dir,
            &base_name,
            &progress,
            &cancel,
        );
        jobs.finish(&id);
        let (cancelled, results) = match outcome {
            Ok(report) => (false, export_results(&dir, &report)),
            Err(e) if e.code == codes::CANCELLED => (true, Vec::new()),
            Err(e) => (
                false,
                vec![JobFileResult {
                    input: failure_input(&dir, &base_name, &exporter),
                    output: None,
                    error: Some(e),
                    meta: None,
                }],
            ),
        };
        let event = JobFinishedEvent {
            job_id: id,
            tab_id,
            cancelled,
            results,
        };
        if let Err(e) = app.emit(JOB_FINISHED_EVENT, event) {
            log::error!("failed to emit job finished: {e}");
        }
    });

    Ok(job_id)
}

/// The export pipeline of `atlas_export` (decode with progress, then
/// [`export_atlas`]).
#[allow(clippy::too_many_arguments)]
pub fn run_export<F: Fn(usize, usize, Option<String>) + Sync>(
    cache: &SessionCache,
    tab: &str,
    paths: &[String],
    params: &AtlasParams,
    exporter: &ExporterConfig,
    incremental: IncrementalOptions,
    dir: &Path,
    base_name: &str,
    progress: &ExportProgress<F>,
    cancel: &AtomicBool,
) -> OpResult<ExportReport> {
    let is_cancelled = || cancel.load(Ordering::SeqCst);
    let sprites = load_sources(cache, tab, paths, &|p| progress.loaded(p), &is_cancelled)?;
    let page_hint = dir.join(format!("{base_name}.png"));
    let page_loader = |p: &Path| cache.get_or_load(tab, p, texopt_core::io::load_image);
    export_atlas(
        ExportRequest {
            sprites,
            params,
            exporter,
            incremental,
            dir,
            base_name,
        },
        &page_loader,
        &mut |stage| progress.stage(&stage, &page_hint),
        &is_cancelled,
    )
}

/// Summary of the atlas project at `path` (`null` when there is none) and
/// what the current inputs `paths` would do to it.
#[tauri::command]
pub async fn atlas_load_project(
    path: String,
    paths: Option<Vec<String>>,
    remove_missing: Option<bool>,
) -> Result<Option<ProjectSummary>, AppError> {
    blocking(move || {
        Ok(project_summary(
            Path::new(&path),
            &paths.unwrap_or_default(),
            remove_missing.unwrap_or(false),
        )?)
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use texopt_core::atlas::IncrementalMode;
    use texopt_core::atlas::exporters::{
        GenericJsonOptions, GodotOptions, ImageOnlyOptions, UnityOptions, UnrealOptions,
    };
    use texopt_core::fixtures;

    use super::*;

    fn u32_at(buf: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(buf[at..at + 4].try_into().unwrap())
    }

    fn write_inputs(dir: &Path) -> Vec<String> {
        let a = dir.join("a.png");
        let b = dir.join("b.png");
        fixtures::gradient(20, 10).save(&a).unwrap();
        fixtures::sprite(16, 16, 4, 4, 8, 6, fixtures::RED)
            .save(&b)
            .unwrap();
        vec![a.display().to_string(), b.display().to_string()]
    }

    fn generic() -> ExporterConfig {
        ExporterConfig::GenericJson(GenericJsonOptions::default())
    }

    #[test]
    fn preview_payload_has_header_and_page_pngs() {
        let dir = tempfile::tempdir().unwrap();
        let paths = write_inputs(dir.path());
        let cache = SessionCache::default();
        let sprites = load_sources(&cache, "tab", &paths, &|_| {}, &|| false).unwrap();
        assert!(cache.contains("tab", Path::new(&paths[0])));
        let outcome = build_incremental(
            BuildRequest {
                sprites,
                params: &AtlasParams::default(),
                exporter: &generic(),
                incremental: IncrementalOptions::default(),
                target: None,
            },
            &|p: &Path| cache.get_or_load("tab", p, texopt_core::io::load_image),
        )
        .unwrap();
        let buf = encode_atlas_preview(&outcome).unwrap();
        let len = u32_at(&buf, 0) as usize;
        let header: Value = serde_json::from_slice(&buf[4..4 + len]).unwrap();
        assert_eq!(header["pages"].as_array().unwrap().len(), 1);
        let png_len = header["pages"][0]["byteLength"].as_u64().unwrap() as usize;
        assert_eq!(4 + len + png_len, buf.len());
        let page = image::load_from_memory(&buf[4 + len..])
            .unwrap()
            .into_rgba8();
        assert_eq!(page, outcome.result.pages[0]);
        assert_eq!(
            (
                header["pages"][0]["width"].as_u64(),
                header["pages"][0]["height"].as_u64()
            ),
            (
                Some(u64::from(page.width())),
                Some(u64::from(page.height()))
            )
        );
        let frames = header["frames"].as_array().unwrap();
        let names: Vec<&str> = frames.iter().map(|f| f["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["a", "b"]);
        let b = &frames[1];
        assert_eq!(b["trimmed"], true);
        assert_eq!(b["frame"]["w"], 8);
        assert_eq!(b["sourceSize"], json!({ "w": 16, "h": 16 }));
        assert_eq!(
            b["spriteSourceSize"],
            json!({ "x": 4, "y": 4, "w": 8, "h": 6 })
        );
        assert_eq!(b["sourcePath"], paths[1].as_str());
        assert_eq!(b["status"], Value::Null);
        assert_eq!(header["stats"]["spriteCount"], 2);
        assert_eq!(header["hasPrevious"], false);
        assert_eq!(header["warnings"], json!([]));
        assert_eq!(header["params"]["maxWidth"], 2048);
    }

    #[test]
    fn preview_target_requires_folder_and_valid_base() {
        assert!(preview_target(Some("C:/out"), Some("atlas")).is_some());
        assert!(preview_target(Some(""), Some("atlas")).is_none());
        assert!(preview_target(Some("C:/out"), Some("a/b")).is_none());
        assert!(preview_target(None, Some("atlas")).is_none());
        assert!(preview_target(Some("C:/out"), None).is_none());
    }

    #[test]
    fn export_job_pipeline_reports_progress_and_results() {
        let dir = tempfile::tempdir().unwrap();
        let paths = write_inputs(dir.path());
        let out = dir.path().join("out");
        let events: Mutex<Vec<(usize, usize, Option<String>)>> = Mutex::new(Vec::new());
        let progress = ExportProgress::new(paths.len(), |d, t, c| {
            events.lock().unwrap().push((d, t, c))
        });
        let cache = SessionCache::default();
        let report = run_export(
            &cache,
            "tab",
            &paths,
            &AtlasParams::default(),
            &generic(),
            IncrementalOptions::default(),
            &out,
            "atlas",
            &progress,
            &AtomicBool::new(false),
        )
        .unwrap();
        let events = events.into_inner().unwrap();
        assert!(events.iter().all(|(_, t, _)| *t == 5));
        assert!(
            events.windows(2).all(|w| w[0].0 <= w[1].0),
            "monotonic: {events:?}"
        );
        assert_eq!(events.last().unwrap().0, 4);
        assert!(
            events.iter().any(|(_, _, c)| c.as_deref()
                == Some(out.join("atlas.png").display().to_string().as_str()))
        );

        let results = export_results(&out, &report);
        let outputs: Vec<String> = results.iter().map(|r| r.output.clone().unwrap()).collect();
        assert_eq!(
            outputs,
            [
                out.join("atlas.texatlas.json").display().to_string(),
                out.join("atlas.png").display().to_string(),
                out.join("atlas.json").display().to_string(),
            ]
        );
        let meta = results[0].meta.as_ref().unwrap();
        assert_eq!(meta["kind"], "atlasExport");
        assert_eq!(meta["outputDir"], out.display().to_string());
        assert_eq!(meta["written"].as_array().unwrap().len(), 3);
        assert_eq!(meta["stats"]["spriteCount"], 2);
        assert!(results.iter().all(|r| r.error.is_none()));

        // Cancelled before decoding: nothing written.
        let other = dir.path().join("cancelled");
        let progress = ExportProgress::new(paths.len(), |_, _, _| {});
        let err = run_export(
            &cache,
            "tab",
            &paths,
            &AtlasParams::default(),
            &generic(),
            IncrementalOptions::default(),
            &other,
            "atlas",
            &progress,
            &AtomicBool::new(true),
        )
        .unwrap_err();
        assert_eq!(err.code, codes::CANCELLED);
        assert!(!other.exists());
    }

    #[test]
    fn image_only_export_reports_the_page_and_no_project() {
        let dir = tempfile::tempdir().unwrap();
        let paths = write_inputs(dir.path());
        let out = dir.path().join("out");
        let cache = SessionCache::default();
        let progress = ExportProgress::new(paths.len(), |_, _, _| {});
        let report = run_export(
            &cache,
            "tab",
            &paths,
            &AtlasParams::default(),
            &ExporterConfig::ImageOnly(ImageOnlyOptions::default()),
            IncrementalOptions::default(),
            &out,
            "sheet",
            &progress,
            &AtomicBool::new(false),
        )
        .unwrap();
        let mut files: Vec<String> = std::fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        files.sort();
        assert_eq!(files, ["sheet.png"]);

        let results = export_results(&out, &report);
        let page = out.join("sheet.png").display().to_string();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].output.as_deref(), Some(page.as_str()));
        assert_eq!(results[0].input, page);
        let meta = results[0].meta.as_ref().unwrap();
        assert_eq!(meta["kind"], "atlasExport");
        assert_eq!(meta["projectPath"], Value::Null);
        assert_eq!(meta["outputDir"], out.display().to_string());
        assert_eq!(meta["written"], json!([page]));
        assert_eq!(meta["deleted"], json!([]));
        assert_eq!(
            failure_input(
                &out,
                "sheet",
                &ExporterConfig::ImageOnly(ImageOnlyOptions::default())
            ),
            page
        );
        assert_eq!(
            failure_input(&out, "sheet", &generic()),
            out.join("sheet.texatlas.json").display().to_string()
        );
    }

    #[test]
    fn project_summary_lists_merge_plan() {
        let dir = tempfile::tempdir().unwrap();
        let paths = write_inputs(dir.path());
        let out = dir.path().join("out");
        let cache = SessionCache::default();
        let progress = ExportProgress::new(paths.len(), |_, _, _| {});
        run_export(
            &cache,
            "tab",
            &paths,
            &AtlasParams::default(),
            &generic(),
            IncrementalOptions::default(),
            &out,
            "atlas",
            &progress,
            &AtomicBool::new(false),
        )
        .unwrap();
        let project = project_path(&out, "atlas");
        let c = dir.path().join("c.png").display().to_string();
        let summary = project_summary(&project, &[paths[1].clone(), c.clone()], false)
            .unwrap()
            .unwrap();
        assert_eq!(summary.sprite_count, 2);
        assert_eq!(summary.frame_count, 2);
        assert_eq!(summary.exporter.as_deref(), Some("genericJson"));
        assert_eq!(summary.pages.len(), 1);
        let plan: Vec<(&str, SpriteStatus)> = summary
            .plan
            .iter()
            .map(|e| (e.name.as_str(), e.status))
            .collect();
        assert_eq!(
            plan,
            [
                ("a", SpriteStatus::Kept),
                ("b", SpriteStatus::Replaced),
                ("c", SpriteStatus::New),
            ]
        );
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(
            json["plan"][0],
            json!({ "name": "a", "status": "kept", "sourcePath": paths[0] })
        );
        assert!(
            project_summary(&out.join("none.texatlas.json"), &[], false)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn incremental_options_parse_with_defaults() {
        assert_eq!(
            parse_incremental(None).unwrap(),
            IncrementalOptions::default()
        );
        assert_eq!(
            parse_incremental(Some(
                json!({ "mode": "repackOptimal", "removeMissing": true })
            ))
            .unwrap(),
            IncrementalOptions {
                mode: IncrementalMode::RepackOptimal,
                remove_missing: true
            }
        );
        let err = parse_incremental(Some(json!({ "mode": "nope" }))).unwrap_err();
        assert_eq!(err.0.params["param"], "incrementalOptions");
    }

    /// The frontend schema defaults are checked against this same fixture
    /// (`src/tabs/atlas/rustDefaults.json`, see `schema.test.ts`), so TS and
    /// Rust defaults cannot drift apart.
    #[test]
    fn rust_defaults_match_the_frontend_fixture() {
        let actual = json!({
            "params": AtlasParams::default(),
            "exporters": {
                "genericJson": GenericJsonOptions::default(),
                "unity": UnityOptions::default(),
                "godot": GodotOptions::default(),
                "unreal": UnrealOptions::default(),
                "imageOnly": ImageOnlyOptions::default(),
            },
            "incremental": IncrementalOptions::default(),
        });
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/tabs/atlas/rustDefaults.json");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let expected: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        assert_eq!(
            actual,
            expected,
            "update {}:\n{}",
            path.display(),
            serde_json::to_string_pretty(&actual).unwrap()
        );
        // Every exporter config round-trips through the tagged JSON shape.
        for kind in ["genericJson", "unity", "godot", "unreal", "imageOnly"] {
            let cfg: ExporterConfig = serde_json::from_value(
                json!({ "kind": kind, "options": expected["exporters"][kind] }),
            )
            .unwrap();
            assert_eq!(cfg.id(), kind);
        }
    }
}
