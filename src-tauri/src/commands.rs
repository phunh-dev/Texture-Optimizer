//! Thin Tauri commands over the core library. Names and argument shapes are
//! the contract in `src/lib/ipc/index.ts`; every command fails with
//! [`AppError`] (`{ code, params }`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, State};
use texopt_core::io::{ScanOptions, ScanResult};
use texopt_core::ops::OpRequest;
use texopt_core::output::OutputSettings;

use crate::commands_staging::Staging;
use crate::error::{AppError, parse_arg};
use crate::jobs::{
    JOB_FINISHED_EVENT, JOB_PROGRESS_EVENT, JobFinishedEvent, JobProgressEvent, JobRegistry,
    PROGRESS_INTERVAL, process_file, run_batch, staged_planner,
};
use crate::session::SessionCache;

/// Backend state shared by the commands.
#[derive(Default)]
pub struct AppState {
    pub sessions: SessionCache,
    pub jobs: Arc<JobRegistry>,
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(AppError::unknown)?
}

/// Expand files/folders into supported images with header info.
#[tauri::command]
pub async fn scan_paths(
    paths: Vec<String>,
    options: Option<serde_json::Value>,
) -> Result<ScanResult, AppError> {
    let options: ScanOptions = match options {
        Some(v) => parse_arg("options", v)?,
        None => ScanOptions::default(),
    };
    blocking(move || Ok(texopt_core::io::scan_paths(&paths, &options))).await
}

/// Run one op in memory on a (session-cached) image; returns the packed
/// binary payload described in `preview.rs` as an ArrayBuffer.
#[tauri::command]
pub async fn preview_op(
    app: AppHandle,
    tab_id: String,
    path: String,
    request: serde_json::Value,
) -> Result<Response, AppError> {
    let request: OpRequest = parse_arg("request", request)?;
    blocking(move || {
        let state = tauri::Manager::state::<AppState>(&app);
        let bytes =
            crate::preview::render_preview(&state.sessions, &tab_id, Path::new(&path), |img| {
                texopt_core::ops::run(img, &request)
            })?;
        Ok(Response::new(bytes))
    })
    .await
}

/// Start a background batch job; returns its id immediately. Progress and
/// the final results arrive as `job://progress` / `job://finished` events
/// (both carry `jobId` and `tabId`; for tiny jobs they may arrive before this
/// command's promise resolves, so listeners should be attached beforehand).
///
/// Results are never written into user folders: they go to the tab's staging
/// folder (`<appCacheDir>/staging/<tabId>/<jobId>/`, replacing the tab's
/// previous results) whatever `output.mode` says; `output` only contributes
/// the format/encoding settings. The user then saves them (`save_results`).
#[tauri::command]
pub fn run_op(
    app: AppHandle,
    state: State<'_, AppState>,
    staging: State<'_, Staging>,
    tab_id: String,
    request: serde_json::Value,
    paths: Vec<String>,
    output: serde_json::Value,
) -> Result<String, AppError> {
    let request: OpRequest = parse_arg("request", request)?;
    let settings: OutputSettings = parse_arg("output", output)?;
    texopt_core::staging::validate_id("tabId", &tab_id)?;
    let (job_id, cancel) = state.jobs.create();
    let prepared = staging
        .begin(&tab_id, &job_id)
        .and_then(|dir| staged_planner(&dir, &settings));
    let planner = match prepared {
        Ok(planner) => planner,
        Err(e) => {
            state.jobs.finish(&job_id);
            return Err(e.into());
        }
    };
    let jobs = state.jobs.clone();
    let id = job_id.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let files: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        let outcome = run_batch(
            &files,
            |input| process_file(input, &planner, |img| texopt_core::ops::run(img, &request)),
            |p| {
                let event = JobProgressEvent {
                    job_id: id.clone(),
                    tab_id: tab_id.clone(),
                    done: p.done,
                    total: p.total,
                    current_path: p.current.map(|c| c.display().to_string()),
                };
                if let Err(e) = app.emit(JOB_PROGRESS_EVENT, event) {
                    log::warn!("failed to emit progress: {e}");
                }
            },
            &cancel,
            PROGRESS_INTERVAL,
        );
        jobs.finish(&id);
        tauri::Manager::state::<Staging>(&app).finish(&tab_id, &id);
        let event = JobFinishedEvent {
            job_id: id,
            tab_id,
            cancelled: outcome.cancelled,
            results: outcome.results,
        };
        if let Err(e) = app.emit(JOB_FINISHED_EVENT, event) {
            log::error!("failed to emit job finished: {e}");
        }
    });

    Ok(job_id)
}

/// Request cancellation; the job then finishes with `cancelled: true`.
#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, job_id: String) -> Result<(), AppError> {
    state.jobs.cancel(&job_id).map_err(AppError::from)
}

/// Drop the decoded-image cache of a tab (tab went to sleep / closed).
#[tauri::command]
pub fn release_session(state: State<'_, AppState>, tab_id: String) -> Result<(), AppError> {
    state.sessions.release(&tab_id);
    Ok(())
}
