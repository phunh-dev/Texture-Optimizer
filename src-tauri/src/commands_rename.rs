//! Pattern Renamer commands: plan, execute (with a persisted, revertable log),
//! revert and list logs.
//!
//! The command bodies are thin wrappers over the Tauri-independent functions
//! of this module ([`plan_paths`], [`execute_paths`], [`revert_last`],
//! [`revert_by_id`], [`list_logs`]) which are unit tested with temp dirs.
//!
//! Safety rules:
//! - `rename_execute` never receives a plan from the UI: it re-plans from the
//!   paths + params against the current disk state and refuses on conflicts,
//!   so a stale preview can never cause an overwrite.
//! - Every executed rename is persisted as JSON in `<appData>/rename-logs/`
//!   (newest [`MAX_LOGS`] kept) so it can be reverted later, even after a restart.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use texopt_core::OpError;
use texopt_core::error::codes;
use texopt_core::io::ImportedFile;
use texopt_core::rename::{
    self, Conflict, ExecuteMode, RenameEntry, RenameLog, RenameLogEntry, RenameParams,
    RenamePlanItem,
};

use crate::error::{AppError, parse_arg};

/// Nothing would change (every in-place name is already the target name).
pub const RENAME_NOTHING_TO_RENAME: &str = "RENAME_NOTHING_TO_RENAME";
/// "Revert last rename" with no remaining rename log.
pub const RENAME_NO_LOG: &str = "RENAME_NO_LOG";
/// The requested rename log does not exist (anymore). Params: `id`.
pub const RENAME_LOG_NOT_FOUND: &str = "RENAME_LOG_NOT_FOUND";

/// Number of rename logs kept on disk (oldest are deleted first).
pub const MAX_LOGS: usize = 20;
/// Sub-folder of the app data dir holding the logs.
pub const LOG_DIR_NAME: &str = "rename-logs";

/// Serializes execute/revert so two batches never interleave on disk or in the log folder.
static RENAME_LOCK: Mutex<()> = Mutex::new(());

/// A [`RenameLog`] persisted on disk, identified by `id` (also its file stem).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRenameLog {
    pub id: String,
    #[serde(flatten)]
    pub log: RenameLog,
}

/// A session file whose path changed: the UI replaces the file at `from` with `file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileUpdate {
    pub from: PathBuf,
    pub file: ImportedFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteOutcome {
    pub log: StoredRenameLog,
    /// In-place mode: one update per renamed file. Copy mode: empty (originals untouched).
    pub updates: Vec<FileUpdate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevertOutcome {
    /// The log that was reverted (and deleted).
    pub log: StoredRenameLog,
    /// In-place mode: files moved back (`from` = renamed path, `file.path` = restored original path).
    pub updates: Vec<FileUpdate>,
    /// Copy mode: copies that were deleted.
    pub removed: Vec<PathBuf>,
}

// ---------------------------------------------------------------------------
// Testable logic

fn source_missing(path: &Path) -> AppError {
    AppError(OpError::new(rename::RENAME_SOURCE_MISSING).with("path", path.display().to_string()))
}

/// Builds rename entries from file metadata and image headers (0×0 when the
/// header cannot be read, e.g. a non-image file).
pub fn build_entries(paths: &[PathBuf]) -> Result<Vec<RenameEntry>, AppError> {
    paths
        .iter()
        .map(|path| {
            let meta = fs::metadata(path).map_err(|_| source_missing(path))?;
            if !meta.is_file() {
                return Err(source_missing(path));
            }
            let (width, height) = image::image_dimensions(path).unwrap_or((0, 0));
            Ok(RenameEntry {
                path: path.clone(),
                width,
                height,
                modified_ms: texopt_core::io::mtime_ms(&meta) as i64,
                size_bytes: meta.len(),
            })
        })
        .collect()
}

fn exists(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}

fn name_key(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Plans a rename against the current disk state.
///
/// In-place: conflicts come from the core planner with a disk probe.
/// Copy: targets are `dir/<new name>`; duplicates are detected by destination
/// name (files from different folders may collide there) and existence is
/// probed in `dir`. The returned `to` is the destination path.
pub fn plan_paths(
    paths: &[PathBuf],
    params: &RenameParams,
    mode: &ExecuteMode,
) -> Result<Vec<RenamePlanItem>, AppError> {
    let entries = build_entries(paths)?;
    match mode {
        ExecuteMode::InPlace => Ok(rename::plan(&entries, params, &exists)?),
        ExecuteMode::CopyTo { dir } => {
            let mut items = rename::plan(&entries, params, &|_: &Path| false)?;
            for it in &mut items {
                if let Some(name) = it.to.file_name().map(|n| n.to_os_string()) {
                    it.to = dir.join(name);
                }
            }
            let mut counts: HashMap<String, usize> = HashMap::new();
            for it in &items {
                *counts.entry(name_key(&it.to)).or_default() += 1;
            }
            for it in &mut items {
                if it.conflict == Some(Conflict::InvalidName) {
                    continue;
                }
                it.conflict = if counts[&name_key(&it.to)] > 1 {
                    Some(Conflict::DuplicateInBatch)
                } else if exists(&it.to) {
                    Some(Conflict::ExistsOnDisk)
                } else {
                    None
                };
            }
            Ok(items)
        }
    }
}

fn inspect(path: &Path) -> ImportedFile {
    texopt_core::io::inspect_file(path).unwrap_or_else(|_| {
        let p = path.display().to_string();
        ImportedFile {
            id: texopt_core::io::file_id(&p),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            ext: texopt_core::io::extension_of(path),
            width: 0,
            height: 0,
            size_bytes: fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            mtime_ms: fs::metadata(path)
                .map(|m| texopt_core::io::mtime_ms(&m))
                .unwrap_or(0),
            path: p,
        }
    })
}

/// Re-plans (never trusting a client plan), refuses on conflicts, executes and
/// persists the log in `log_dir`.
pub fn execute_paths(
    log_dir: &Path,
    paths: &[PathBuf],
    params: &RenameParams,
    mode: &ExecuteMode,
) -> Result<ExecuteOutcome, AppError> {
    let _guard = RENAME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let plan = plan_paths(paths, params, mode)?;
    let conflicts = plan.iter().filter(|i| i.conflict.is_some()).count();
    if conflicts > 0 {
        return Err(OpError::new(rename::RENAME_HAS_CONFLICTS)
            .with("count", conflicts)
            .into());
    }
    let changes = match mode {
        ExecuteMode::InPlace => plan.iter().filter(|i| i.from != i.to).count(),
        ExecuteMode::CopyTo { .. } => plan.len(),
    };
    if changes == 0 {
        return Err(OpError::new(RENAME_NOTHING_TO_RENAME).into());
    }
    let log = rename::execute(&plan, mode)?;
    let updates = match mode {
        ExecuteMode::InPlace => log
            .entries
            .iter()
            .map(|e| FileUpdate {
                from: e.from.clone(),
                file: inspect(&e.to),
            })
            .collect(),
        ExecuteMode::CopyTo { .. } => Vec::new(),
    };
    let stored = persist_log(log_dir, log)?;
    Ok(ExecuteOutcome {
        log: stored,
        updates,
    })
}

fn write_error(path: &Path, e: &dyn std::fmt::Display) -> AppError {
    AppError(
        OpError::new(codes::IO_WRITE_FAILED)
            .with("path", path.display().to_string())
            .with("detail", e.to_string()),
    )
}

fn read_error(path: &Path, e: &dyn std::fmt::Display) -> AppError {
    AppError(
        OpError::new(codes::IO_READ_FAILED)
            .with("path", path.display().to_string())
            .with("detail", e.to_string()),
    )
}

/// Log ids are `<timestamp ms, 16 digits>-<counter, 4 digits>`, so file names sort chronologically.
fn is_valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_digit() || c == '-')
}

fn log_path(log_dir: &Path, id: &str) -> PathBuf {
    log_dir.join(format!("{id}.json"))
}

/// Writes the log as `<id>.json` and deletes the oldest logs beyond [`MAX_LOGS`].
pub fn persist_log(log_dir: &Path, log: RenameLog) -> Result<StoredRenameLog, AppError> {
    fs::create_dir_all(log_dir).map_err(|e| write_error(log_dir, &e))?;
    let ts = log.timestamp.max(0);
    let id = (0u32..)
        .map(|n| format!("{ts:016}-{n:04}"))
        .find(|id| !exists(&log_path(log_dir, id)))
        .expect("an unused log id exists");
    let stored = StoredRenameLog { id, log };
    let path = log_path(log_dir, &stored.id);
    let json = serde_json::to_vec_pretty(&stored).map_err(|e| write_error(&path, &e))?;
    fs::write(&path, json).map_err(|e| write_error(&path, &e))?;
    rotate_logs(log_dir)?;
    Ok(stored)
}

/// Ids of the logs on disk, oldest first.
fn log_ids(log_dir: &Path) -> Result<Vec<String>, AppError> {
    let dir = match fs::read_dir(log_dir) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(read_error(log_dir, &e)),
    };
    let mut ids: Vec<String> = dir
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".json")
                .filter(|id| is_valid_id(id))
                .map(str::to_owned)
        })
        .collect();
    ids.sort();
    Ok(ids)
}

fn rotate_logs(log_dir: &Path) -> Result<(), AppError> {
    let ids = log_ids(log_dir)?;
    if ids.len() > MAX_LOGS {
        for id in &ids[..ids.len() - MAX_LOGS] {
            let path = log_path(log_dir, id);
            fs::remove_file(&path).map_err(|e| write_error(&path, &e))?;
        }
    }
    Ok(())
}

fn read_log(log_dir: &Path, id: &str) -> Result<StoredRenameLog, AppError> {
    let path = log_path(log_dir, id);
    let bytes = fs::read(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError(OpError::new(RENAME_LOG_NOT_FOUND).with("id", id))
        } else {
            read_error(&path, &e)
        }
    })?;
    serde_json::from_slice(&bytes).map_err(|e| read_error(&path, &e))
}

/// Every readable log, newest first (unreadable files are skipped).
pub fn list_logs(log_dir: &Path) -> Result<Vec<StoredRenameLog>, AppError> {
    let mut ids = log_ids(log_dir)?;
    ids.reverse();
    Ok(ids
        .iter()
        .filter_map(|id| read_log(log_dir, id).ok())
        .collect())
}

fn revert_stored(log_dir: &Path, stored: StoredRenameLog) -> Result<RevertOutcome, AppError> {
    rename::revert(&stored.log)?;
    let (updates, removed) = match stored.log.mode {
        ExecuteMode::InPlace => (
            stored
                .log
                .entries
                .iter()
                .map(|e: &RenameLogEntry| FileUpdate {
                    from: e.to.clone(),
                    file: inspect(&e.from),
                })
                .collect(),
            Vec::new(),
        ),
        ExecuteMode::CopyTo { .. } => (
            Vec::new(),
            stored.log.entries.iter().map(|e| e.to.clone()).collect(),
        ),
    };
    let path = log_path(log_dir, &stored.id);
    fs::remove_file(&path).map_err(|e| write_error(&path, &e))?;
    Ok(RevertOutcome {
        log: stored,
        updates,
        removed,
    })
}

/// Reverts the newest log and deletes it (the next call reverts the one before).
pub fn revert_last(log_dir: &Path) -> Result<RevertOutcome, AppError> {
    let _guard = RENAME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let id = log_ids(log_dir)?
        .pop()
        .ok_or_else(|| AppError(OpError::new(RENAME_NO_LOG)))?;
    let stored = read_log(log_dir, &id)?;
    revert_stored(log_dir, stored)
}

/// Reverts a specific log (all-or-nothing) and deletes it.
pub fn revert_by_id(log_dir: &Path, id: &str) -> Result<RevertOutcome, AppError> {
    let _guard = RENAME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !is_valid_id(id) {
        return Err(OpError::new(RENAME_LOG_NOT_FOUND).with("id", id).into());
    }
    let stored = read_log(log_dir, id)?;
    revert_stored(log_dir, stored)
}

// ---------------------------------------------------------------------------
// Tauri commands

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(AppError::unknown)?
}

fn log_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path()
        .app_data_dir()
        .map(|d| d.join(LOG_DIR_NAME))
        .map_err(AppError::unknown)
}

fn parse_inputs(
    paths: Vec<String>,
    params: serde_json::Value,
    mode: Option<serde_json::Value>,
) -> Result<(Vec<PathBuf>, RenameParams, ExecuteMode), AppError> {
    let params: RenameParams = parse_arg("params", params)?;
    let mode: ExecuteMode = match mode {
        Some(v) if !v.is_null() => parse_arg("mode", v)?,
        _ => ExecuteMode::InPlace,
    };
    Ok((paths.into_iter().map(PathBuf::from).collect(), params, mode))
}

/// Preview of the new names (numbering order) with conflicts.
#[tauri::command]
pub async fn rename_plan(
    paths: Vec<String>,
    params: serde_json::Value,
    mode: Option<serde_json::Value>,
) -> Result<Vec<RenamePlanItem>, AppError> {
    let (paths, params, mode) = parse_inputs(paths, params, mode)?;
    blocking(move || plan_paths(&paths, &params, &mode)).await
}

/// Re-plans server-side, refuses on conflicts, renames/copies and persists the log.
#[tauri::command]
pub async fn rename_execute(
    app: AppHandle,
    paths: Vec<String>,
    params: serde_json::Value,
    mode: Option<serde_json::Value>,
) -> Result<ExecuteOutcome, AppError> {
    let (paths, params, mode) = parse_inputs(paths, params, mode)?;
    let dir = log_dir(&app)?;
    blocking(move || execute_paths(&dir, &paths, &params, &mode)).await
}

#[tauri::command]
pub async fn rename_revert_last(app: AppHandle) -> Result<RevertOutcome, AppError> {
    let dir = log_dir(&app)?;
    blocking(move || revert_last(&dir)).await
}

#[tauri::command]
pub async fn rename_revert(app: AppHandle, log_id: String) -> Result<RevertOutcome, AppError> {
    let dir = log_dir(&app)?;
    blocking(move || revert_by_id(&dir, &log_id)).await
}

/// Persisted logs, newest first.
#[tauri::command]
pub async fn rename_logs(app: AppHandle) -> Result<Vec<StoredRenameLog>, AppError> {
    let dir = log_dir(&app)?;
    blocking(move || list_logs(&dir)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn png(path: &Path, w: u32, h: u32) {
        image::RgbaImage::new(w, h).save(path).unwrap();
    }

    struct Fixture {
        _tmp: TempDir,
        root: PathBuf,
        logs: PathBuf,
    }

    fn fixture(names: &[&str]) -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("tex");
        fs::create_dir_all(&root).unwrap();
        for n in names {
            png(&root.join(n), 4, 2);
        }
        let logs = tmp.path().join(LOG_DIR_NAME);
        Fixture {
            _tmp: tmp,
            root,
            logs,
        }
    }

    fn params(v: serde_json::Value) -> RenameParams {
        serde_json::from_value(v).unwrap()
    }

    fn paths(f: &Fixture, names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(|n| f.root.join(n)).collect()
    }

    fn file_names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn entries_read_header_dimensions_and_reject_missing_files() {
        let f = fixture(&["a.png"]);
        let entries = build_entries(&paths(&f, &["a.png"])).unwrap();
        assert_eq!((entries[0].width, entries[0].height), (4, 2));
        assert!(entries[0].size_bytes > 0);
        let err = build_entries(&paths(&f, &["missing.png"])).unwrap_err();
        assert_eq!(err.0.code, rename::RENAME_SOURCE_MISSING);
    }

    #[test]
    fn plan_in_place_uses_tokens_and_flags_existing_files() {
        let f = fixture(&["a.png", "b.png", "taken_2.png"]);
        let p = params(json!({ "template": "{name}_{width}x{height}" }));
        let items = plan_paths(&paths(&f, &["a.png", "b.png"]), &p, &ExecuteMode::InPlace).unwrap();
        assert_eq!(items[0].to, f.root.join("a_4x2.png"));
        assert_eq!(items[1].to, f.root.join("b_4x2.png"));
        assert!(items.iter().all(|i| i.conflict.is_none()));

        let p = params(json!({ "template": "taken_{index}" }));
        let items = plan_paths(&paths(&f, &["a.png", "b.png"]), &p, &ExecuteMode::InPlace).unwrap();
        assert_eq!(items[0].conflict, None);
        assert_eq!(items[1].conflict, Some(Conflict::ExistsOnDisk));
    }

    #[test]
    fn plan_copy_checks_destination_dir_and_cross_folder_duplicates() {
        let f = fixture(&["a.png"]);
        let other = f.root.join("sub");
        fs::create_dir_all(&other).unwrap();
        png(&other.join("a.png"), 2, 2);
        let dest = f.root.parent().unwrap().join("out");
        fs::create_dir_all(&dest).unwrap();
        png(&dest.join("x_a.png"), 1, 1);
        let mode = ExecuteMode::CopyTo { dir: dest.clone() };

        // Unchanged names: in place nothing conflicts, but both copies land on out/a.png.
        let both = vec![f.root.join("a.png"), other.join("a.png")];
        let items = plan_paths(&both, &RenameParams::default(), &mode).unwrap();
        assert_eq!(items[0].to, dest.join("a.png"));
        assert!(
            items
                .iter()
                .all(|i| i.conflict == Some(Conflict::DuplicateInBatch))
        );

        // Destination already has x_a.png; the source folder does not.
        let p = params(json!({ "prefix": "x_" }));
        let items = plan_paths(&both[..1], &p, &mode).unwrap();
        assert_eq!(items[0].conflict, Some(Conflict::ExistsOnDisk));
        // Source-folder files never matter in copy mode.
        png(&f.root.join("y_a.png"), 1, 1);
        let p = params(json!({ "prefix": "y_" }));
        let items = plan_paths(&both[..1], &p, &mode).unwrap();
        assert_eq!(items[0].conflict, None);
    }

    #[test]
    fn plan_reports_invalid_regex_as_app_error() {
        let f = fixture(&["a.png"]);
        let p = params(json!({ "findReplace": [{ "find": "(", "replace": "", "regex": true }] }));
        let err = plan_paths(&paths(&f, &["a.png"]), &p, &ExecuteMode::InPlace).unwrap_err();
        assert_eq!(err.0.code, rename::RENAME_INVALID_REGEX);
        assert_eq!(err.0.params["pattern"], "(");
    }

    #[test]
    fn execute_in_place_renames_persists_log_and_returns_updates() {
        let f = fixture(&["a.png", "b.png"]);
        let p = params(json!({ "template": "tex_{index}", "zeroPad": 2 }));
        let out = execute_paths(
            &f.logs,
            &paths(&f, &["a.png", "b.png"]),
            &p,
            &ExecuteMode::InPlace,
        )
        .unwrap();
        assert_eq!(file_names(&f.root), vec!["tex_01.png", "tex_02.png"]);
        assert_eq!(out.updates.len(), 2);
        assert_eq!(out.updates[0].from, f.root.join("a.png"));
        assert_eq!(out.updates[0].file.name, "tex_01.png");
        assert_eq!(
            (out.updates[0].file.width, out.updates[0].file.height),
            (4, 2)
        );
        let logs = list_logs(&f.logs).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0], out.log);
        assert_eq!(logs[0].log.entries.len(), 2);
        // The serialized shape the UI relies on.
        let v = serde_json::to_value(&out.log).unwrap();
        assert!(v["id"].is_string() && v["timestamp"].is_number());
        assert_eq!(v["mode"], json!({ "kind": "inPlace" }));
        assert!(v["entries"][0]["from"].is_string());
    }

    #[test]
    fn execute_refuses_conflicts_and_changes_nothing() {
        let f = fixture(&["a.png", "b.png"]);
        let p = params(json!({ "template": "same" }));
        let err = execute_paths(
            &f.logs,
            &paths(&f, &["a.png", "b.png"]),
            &p,
            &ExecuteMode::InPlace,
        )
        .unwrap_err();
        assert_eq!(err.0.code, rename::RENAME_HAS_CONFLICTS);
        assert_eq!(err.0.params["count"], 2);
        assert_eq!(file_names(&f.root), vec!["a.png", "b.png"]);
        assert!(list_logs(&f.logs).unwrap().is_empty());
    }

    #[test]
    fn execute_replans_so_a_stale_preview_cannot_overwrite() {
        let f = fixture(&["a.png"]);
        let p = params(json!({ "prefix": "new_" }));
        let files = paths(&f, &["a.png"]);
        let preview = plan_paths(&files, &p, &ExecuteMode::InPlace).unwrap();
        assert_eq!(preview[0].conflict, None);
        // The disk changes after the preview: the target now exists.
        fs::write(f.root.join("new_a.png"), b"precious").unwrap();
        let err = execute_paths(&f.logs, &files, &p, &ExecuteMode::InPlace).unwrap_err();
        assert_eq!(err.0.code, rename::RENAME_HAS_CONFLICTS);
        assert_eq!(fs::read(f.root.join("new_a.png")).unwrap(), b"precious");
        assert!(f.root.join("a.png").exists());

        // A source that vanished after the preview is reported, not ignored.
        fs::remove_file(f.root.join("new_a.png")).unwrap();
        fs::remove_file(f.root.join("a.png")).unwrap();
        let err = execute_paths(&f.logs, &files, &p, &ExecuteMode::InPlace).unwrap_err();
        assert_eq!(err.0.code, rename::RENAME_SOURCE_MISSING);
    }

    #[test]
    fn execute_with_nothing_to_change_is_an_error() {
        let f = fixture(&["a.png"]);
        let err = execute_paths(
            &f.logs,
            &paths(&f, &["a.png"]),
            &RenameParams::default(),
            &ExecuteMode::InPlace,
        )
        .unwrap_err();
        assert_eq!(err.0.code, RENAME_NOTHING_TO_RENAME);
        assert!(list_logs(&f.logs).unwrap().is_empty());
    }

    #[test]
    fn execute_copy_then_revert_deletes_copies() {
        let f = fixture(&["a.png"]);
        let dest = f.root.parent().unwrap().join("out");
        let mode = ExecuteMode::CopyTo { dir: dest.clone() };
        let out = execute_paths(
            &f.logs,
            &paths(&f, &["a.png"]),
            &RenameParams::default(),
            &mode,
        )
        .unwrap();
        assert!(out.updates.is_empty());
        assert!(dest.join("a.png").exists() && f.root.join("a.png").exists());
        let reverted = revert_last(&f.logs).unwrap();
        assert_eq!(reverted.removed, vec![dest.join("a.png")]);
        assert!(reverted.updates.is_empty());
        assert!(!dest.join("a.png").exists() && f.root.join("a.png").exists());
    }

    #[test]
    fn revert_last_restores_names_in_order_and_consumes_logs() {
        let f = fixture(&["a.png"]);
        let files = paths(&f, &["a.png"]);
        execute_paths(
            &f.logs,
            &files,
            &params(json!({ "prefix": "1_" })),
            &ExecuteMode::InPlace,
        )
        .unwrap();
        execute_paths(
            &f.logs,
            &[f.root.join("1_a.png")],
            &params(json!({ "prefix": "2_" })),
            &ExecuteMode::InPlace,
        )
        .unwrap();
        assert_eq!(file_names(&f.root), vec!["2_1_a.png"]);

        let r = revert_last(&f.logs).unwrap();
        assert_eq!(file_names(&f.root), vec!["1_a.png"]);
        assert_eq!(r.updates.len(), 1);
        assert_eq!(r.updates[0].from, f.root.join("2_1_a.png"));
        assert_eq!(r.updates[0].file.name, "1_a.png");

        revert_last(&f.logs).unwrap();
        assert_eq!(file_names(&f.root), vec!["a.png"]);
        assert_eq!(revert_last(&f.logs).unwrap_err().0.code, RENAME_NO_LOG);
    }

    #[test]
    fn revert_by_id_validates_and_keeps_log_on_failure() {
        let f = fixture(&["a.png"]);
        let out = execute_paths(
            &f.logs,
            &paths(&f, &["a.png"]),
            &params(json!({ "suffix": "_x" })),
            &ExecuteMode::InPlace,
        )
        .unwrap();
        assert_eq!(
            revert_by_id(&f.logs, "../../etc").unwrap_err().0.code,
            RENAME_LOG_NOT_FOUND
        );
        assert_eq!(
            revert_by_id(&f.logs, "0000000000000001-0000")
                .unwrap_err()
                .0
                .code,
            RENAME_LOG_NOT_FOUND
        );

        // The renamed file disappeared: revert fails, nothing changes, the log is kept.
        let renamed = f.root.join("a_x.png");
        let parked = f.root.parent().unwrap().join("parked.png");
        fs::rename(&renamed, &parked).unwrap();
        assert_eq!(
            revert_by_id(&f.logs, &out.log.id).unwrap_err().0.code,
            rename::RENAME_SOURCE_MISSING
        );
        assert_eq!(list_logs(&f.logs).unwrap().len(), 1);

        fs::rename(&parked, &renamed).unwrap();
        let r = revert_by_id(&f.logs, &out.log.id).unwrap();
        assert_eq!(r.log.id, out.log.id);
        assert_eq!(file_names(&f.root), vec!["a.png"]);
        assert!(list_logs(&f.logs).unwrap().is_empty());
    }

    #[test]
    fn logs_rotate_keeping_the_newest_twenty() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(LOG_DIR_NAME);
        let mut ids = Vec::new();
        for i in 0..25i64 {
            let log = RenameLog {
                entries: vec![RenameLogEntry {
                    from: PathBuf::from(format!("a{i}.png")),
                    to: PathBuf::from(format!("b{i}.png")),
                }],
                mode: ExecuteMode::InPlace,
                // Same timestamp twice: ids must stay unique and ordered.
                timestamp: 1_700_000_000_000 + i / 2,
            };
            ids.push(persist_log(&dir, log).unwrap().id);
        }
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(sorted, ids, "ids sort chronologically");
        let logs = list_logs(&dir).unwrap();
        assert_eq!(logs.len(), MAX_LOGS);
        assert_eq!(logs[0].id, ids[24], "newest first");
        assert_eq!(logs[MAX_LOGS - 1].id, ids[5]);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), MAX_LOGS);
    }

    #[test]
    fn list_logs_skips_garbage_and_handles_missing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("nope");
        assert!(list_logs(&dir).unwrap().is_empty());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("0000000000000005-0000.json"), b"{ not json").unwrap();
        fs::write(dir.join("notes.txt"), b"x").unwrap();
        assert!(list_logs(&dir).unwrap().is_empty());
    }

    #[test]
    fn parse_inputs_defaults_to_in_place_and_maps_bad_json() {
        let (_, p, mode) = parse_inputs(vec![], json!({}), None).unwrap();
        assert_eq!(p, RenameParams::default());
        assert_eq!(mode, ExecuteMode::InPlace);
        let (_, _, mode) = parse_inputs(
            vec![],
            json!({}),
            Some(json!({ "kind": "copyTo", "dir": "/x" })),
        )
        .unwrap();
        assert_eq!(
            mode,
            ExecuteMode::CopyTo {
                dir: PathBuf::from("/x")
            }
        );
        let err = parse_inputs(vec![], json!({ "case": "weird" }), None).unwrap_err();
        assert_eq!(err.0.code, codes::INVALID_PARAMS);
        assert_eq!(err.0.params["param"], "params");
    }
}
