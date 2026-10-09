//! Commands of the "3D Texture Packer" tab. Every Assimp call runs in a
//! worker process (`mesh_worker`); these commands only discover files, start
//! workers and translate their messages into results / `job://` events.
//!
//! Payloads (camelCase, TS mirror in `src/tabs/meshPack/ipc.ts`):
//! * `mesh_scan(paths, recursive?)` → [`MeshScanResult`]
//! * `mesh_preview_pack(tabId, models, options)` → `PreviewPayload`
//!   (`{ report, channel, images: [{ width, height, png (base64) }] }`)
//! * `mesh_pack(tabId, models, options, outputDir, baseName)` → job id;
//!   `job://finished` results: one entry per model (`meta.kind = "model"`)
//!   plus a final summary entry (`meta.kind = "summary"`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use texopt_core::io::{ImportedFile, SkippedPath};
use texopt_core::mesh::pack::{self, ModelInfo, ModelOutcome, PackOptions, PackReport};
use texopt_core::OpError;

use crate::commands::AppState;
use crate::error::{AppError, parse_arg};
use crate::jobs::{
    JOB_FINISHED_EVENT, JOB_PROGRESS_EVENT, JobFileResult, JobFinishedEvent, JobProgressEvent,
};
use crate::mesh_worker::{self, CallError, ScanItem, WorkerCommand, WorkerMessage, WorkerRequest};

/// Per-tab preview cancellation: a new preview kills the previous worker.
#[derive(Default)]
pub struct MeshState {
    previews: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl MeshState {
    fn begin_preview(&self, tab: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        let mut map = self.previews.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = map.insert(tab.to_string(), flag.clone()) {
            old.store(true, Ordering::SeqCst);
        }
        flag
    }

    fn end_preview(&self, tab: &str, flag: &Arc<AtomicBool>) {
        let mut map = self.previews.lock().unwrap_or_else(|e| e.into_inner());
        if map.get(tab).is_some_and(|f| Arc::ptr_eq(f, flag)) {
            map.remove(tab);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedModel {
    pub file: ImportedFile,
    pub info: ModelInfo,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshScanResult {
    pub models: Vec<ScannedModel>,
    /// Unsupported/missing paths and models that failed to import.
    pub skipped: Vec<SkippedPath>,
}

fn call_error(e: CallError) -> AppError {
    match e {
        CallError::Cancelled => AppError(OpError::new(texopt_core::error::codes::CANCELLED)),
        CallError::Failed(e) => AppError(e),
    }
}

/// Discovery + per-model scan through the worker (testable without Tauri).
pub fn scan_with(
    command: &WorkerCommand,
    paths: &[String],
    recursive: bool,
) -> Result<MeshScanResult, AppError> {
    let (files, mut skipped) = pack::find_models(paths, recursive);
    if files.is_empty() {
        return Ok(MeshScanResult {
            models: Vec::new(),
            skipped,
        });
    }
    let model_paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    let items = mesh_worker::scan_models(command, &model_paths, &mut |_, _| {}, None)
        .map_err(call_error)?;
    let mut models = Vec::new();
    for (file, item) in files.into_iter().zip(items) {
        match item {
            ScanItem {
                info: Some(info), ..
            } => models.push(ScannedModel { file, info }),
            ScanItem { path, error, .. } => skipped.push(SkippedPath {
                path,
                error: error.unwrap_or_else(|| OpError::new(crate::error::UNKNOWN)),
            }),
        }
    }
    Ok(MeshScanResult { models, skipped })
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(AppError::unknown)?
}

/// Expand files/folders into `.fbx/.obj/.dae` models and describe each one.
#[tauri::command]
pub async fn mesh_scan(
    paths: Vec<String>,
    recursive: Option<bool>,
) -> Result<MeshScanResult, AppError> {
    blocking(move || {
        let command = WorkerCommand::current_exe()?;
        scan_with(&command, &paths, recursive.unwrap_or(false))
    })
    .await
}

/// Compute the layout and a base-colour atlas preview; writes nothing.
/// A newer preview of the same tab cancels this one (`CANCELLED`).
#[tauri::command]
pub async fn mesh_preview_pack(
    state: State<'_, MeshState>,
    tab_id: String,
    models: Vec<String>,
    options: Value,
) -> Result<Value, AppError> {
    let options: PackOptions = parse_arg("options", options)?;
    options.validate()?;
    let flag = state.begin_preview(&tab_id);
    let flag2 = flag.clone();
    let result = blocking(move || {
        let command = WorkerCommand::current_exe()?;
        let request = WorkerRequest::Preview {
            models,
            options,
            preview_max_size: mesh_worker::DEFAULT_PREVIEW_MAX,
        };
        mesh_worker::call(&command, &request, &mut |_| {}, Some(&flag2)).map_err(call_error)
    })
    .await;
    state.end_preview(&tab_id, &flag);
    result
}

/// `job://finished` results for a pack report.
pub fn job_results(output_dir: &str, report: &PackReport) -> Vec<JobFileResult> {
    let mut results: Vec<JobFileResult> = report
        .models
        .iter()
        .map(|m| JobFileResult {
            input: m.source.clone(),
            output: m.output.clone().or_else(|| m.sidecar.clone()),
            error: if m.outcome == ModelOutcome::Failed {
                Some(m.error.clone().unwrap_or_else(|| OpError::new(crate::error::UNKNOWN)))
            } else {
                None
            },
            meta: Some(serde_json::json!({
                "kind": "model",
                "outcome": m.outcome,
                "sidecar": m.sidecar,
                "files": m.files,
                "warnings": m.warnings,
            })),
        })
        .collect();
    results.push(JobFileResult {
        input: output_dir.to_string(),
        output: report.report_path.clone(),
        error: None,
        meta: Some(serde_json::json!({
            "kind": "summary",
            "rewritten": report.count(ModelOutcome::Rewritten),
            "fallback": report.count(ModelOutcome::Fallback),
            "remapData": report.count(ModelOutcome::RemapData),
            "skipped": report.count(ModelOutcome::Skipped),
            "failed": report.count(ModelOutcome::Failed),
            "pages": report.pages.len(),
            "files": report.files,
            "warnings": report.warnings,
        })),
    });
    results
}

/// Results when the whole run failed (bad options, nothing fits, crash…).
pub fn failed_results(output_dir: &str, error: OpError) -> Vec<JobFileResult> {
    vec![JobFileResult {
        input: output_dir.to_string(),
        output: None,
        error: Some(error),
        meta: Some(serde_json::json!({ "kind": "summary" })),
    }]
}

/// Start a pack job (worker process); returns the job id. Progress and the
/// final results arrive as `job://progress` / `job://finished`.
#[tauri::command]
pub fn mesh_pack(
    app: AppHandle,
    state: State<'_, AppState>,
    tab_id: String,
    models: Vec<String>,
    options: Value,
    output_dir: String,
    base_name: String,
) -> Result<String, AppError> {
    let options: PackOptions = parse_arg("options", options)?;
    options.validate()?;
    pack::validate_base_name(&base_name)?;
    if output_dir.trim().is_empty() {
        return Err(AppError(OpError::invalid_param("outputDir", "empty")));
    }
    let command = WorkerCommand::current_exe()?;
    let (job_id, cancel) = state.jobs.create();
    let jobs = state.jobs.clone();
    let id = job_id.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let request = WorkerRequest::Pack {
            models,
            options,
            output_dir: output_dir.clone(),
            base_name,
        };
        let result = mesh_worker::call(
            &command,
            &request,
            &mut |m| {
                if let WorkerMessage::Progress {
                    done,
                    total,
                    current,
                } = m
                {
                    let event = JobProgressEvent {
                        job_id: id.clone(),
                        tab_id: tab_id.clone(),
                        done: *done,
                        total: *total,
                        current_path: current.clone(),
                    };
                    if let Err(e) = app.emit(JOB_PROGRESS_EVENT, event) {
                        log::warn!("failed to emit progress: {e}");
                    }
                }
            },
            Some(&cancel),
        );
        jobs.finish(&id);
        let (cancelled, results) = match result {
            Ok(value) => match serde_json::from_value::<PackReport>(value) {
                Ok(report) => (false, job_results(&output_dir, &report)),
                Err(e) => (
                    false,
                    failed_results(&output_dir, OpError::new(crate::error::UNKNOWN).with("detail", e.to_string())),
                ),
            },
            Err(CallError::Cancelled) => (true, Vec::new()),
            Err(CallError::Failed(e)) => (false, failed_results(&output_dir, e)),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The TS defaults (`buildPackOptions(meshDefaults())`, see
    /// `src/tabs/meshPack/schema.test.ts`) and `PackOptions::default()` agree
    /// through this shared file; the built-in fill colours are spelled out.
    #[test]
    fn rust_defaults_match_the_frontend_fixture() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../src/tabs/meshPack/rustDefaults.json");
        let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut defaults = PackOptions::default();
        for c in pack::default_channels() {
            let px = pack::builtin_default(&c).0;
            defaults
                .missing_defaults
                .insert(c, format!("#{:02x}{:02x}{:02x}", px[0], px[1], px[2]));
        }
        assert_eq!(serde_json::to_value(&defaults).unwrap(), fixture);
        // And the fixture parses back to the defaults.
        let parsed: PackOptions = serde_json::from_value(fixture).unwrap();
        assert_eq!(parsed, defaults);
    }

    #[test]
    fn every_mesh_code_has_en_and_vi_messages() {
        use texopt_core::mesh::pack::codes as p;
        let codes = [
            mesh_worker::MESH_WORKER_CRASHED,
            mesh_worker::MESH_WORKER_SPAWN_FAILED,
            p::MESH_NO_MODELS,
            p::MESH_NOTHING_TO_PACK,
            p::MESH_CHANNEL_FORCED,
            p::MESH_TEXTURE_RESIZED,
            p::MESH_TEXTURE_UNREADABLE,
            p::MESH_MATERIAL_NO_TEXTURES,
            p::MESH_MODEL_SPANS_PAGES,
            p::MESH_FALLBACK_REMAP_DATA,
            p::MESH_TEXTURES_DOWNSCALED,
            p::MESH_MODEL_SKIPPED,
        ];
        for lang in ["en", "vi"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../src/locales/{lang}/errors.json"));
            let json: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            for c in codes {
                assert!(json.get(c).and_then(Value::as_str).is_some_and(|s| !s.is_empty()), "{lang}: {c}");
            }
        }
    }

    #[test]
    fn preview_flags_cancel_the_previous_one() {
        let s = MeshState::default();
        let a = s.begin_preview("t1");
        let b = s.begin_preview("t1");
        let c = s.begin_preview("t2");
        assert!(a.load(Ordering::SeqCst));
        assert!(!b.load(Ordering::SeqCst) && !c.load(Ordering::SeqCst));
        s.end_preview("t1", &a); // stale: does not remove b
        assert!(s.previews.lock().unwrap().contains_key("t1"));
        s.end_preview("t1", &b);
        assert!(!s.previews.lock().unwrap().contains_key("t1"));
    }

    #[test]
    fn job_results_summarise_outcomes() {
        let mut report: PackReport = serde_json::from_value(serde_json::json!({
            "version": 1, "generator": "texture-optimizer", "uvOrigin": "bottomLeft",
            "scalePercent": 100.0, "channels": ["baseColor"], "pages": [],
            "models": [], "files": ["out/atlas_baseColor.png"], "reportPath": "out/atlas.report.json",
            "warnings": []
        }))
        .unwrap();
        let model = |outcome: &str, error: Value| {
            serde_json::from_value::<pack::ModelReport>(serde_json::json!({
                "source": "a.obj", "name": "a.obj", "format": "obj", "outcome": outcome,
                "output": null, "outputFormat": null, "sidecar": "out/a.uvremap.json",
                "files": [], "pages": [0], "materials": [], "warnings": [], "error": error
            }))
            .unwrap()
        };
        report.models.push(model("rewritten", Value::Null));
        report.models.push(model("fallback", Value::Null));
        report.models.push(model(
            "failed",
            serde_json::json!({ "code": "MESH_EXPORT_FAILED", "params": {} }),
        ));
        let r = job_results("out", &report);
        assert_eq!(r.len(), 4);
        assert_eq!(r[0].error, None);
        assert_eq!(r[1].output.as_deref(), Some("out/a.uvremap.json"));
        assert_eq!(r[2].error.as_ref().unwrap().code, "MESH_EXPORT_FAILED");
        let summary = r[3].meta.as_ref().unwrap();
        assert_eq!(summary["kind"], "summary");
        assert_eq!(summary["rewritten"], 1);
        assert_eq!(summary["fallback"], 1);
        assert_eq!(summary["failed"], 1);
        assert_eq!(r[3].output.as_deref(), Some("out/atlas.report.json"));
        let f = failed_results("out", OpError::new("MESH_WORKER_CRASHED"));
        assert_eq!(f[0].error.as_ref().unwrap().code, "MESH_WORKER_CRASHED");
    }
}
