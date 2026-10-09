//! Applying a rename plan on disk (all-or-nothing) and reverting it from a log.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::{
    RENAME_HAS_CONFLICTS, RENAME_IO_FAILED, RENAME_ROLLBACK_FAILED, RENAME_SOURCE_MISSING,
    RENAME_TARGET_EXISTS, RenamePlanItem,
};
use crate::{OpError, OpResult};

/// JSON: `{"kind":"inPlace"}` or `{"kind":"copyTo","dir":"..."}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExecuteMode {
    /// Rename the files where they are.
    InPlace,
    /// Copy each file into `dir` under its new name; originals stay untouched.
    CopyTo { dir: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameLogEntry {
    pub from: PathBuf,
    /// Final path (the copy's path in copy mode).
    pub to: PathBuf,
}

/// Record of an executed plan, persisted by the app for "Revert last rename".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameLog {
    pub entries: Vec<RenameLogEntry>,
    pub mode: ExecuteMode,
    /// Execution time, ms since the Unix epoch.
    pub timestamp: i64,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn exists(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}

fn path_str(p: &Path) -> String {
    p.display().to_string()
}

fn io_error(from: &Path, to: &Path, e: &io::Error) -> OpError {
    OpError::new(RENAME_IO_FAILED)
        .with("from", path_str(from))
        .with("to", path_str(to))
        .with("detail", e.to_string())
}

fn require_sources<'a>(paths: impl IntoIterator<Item = &'a Path>) -> OpResult<()> {
    for p in paths {
        if !fs::metadata(p).is_ok_and(|m| m.is_file()) {
            return Err(OpError::new(RENAME_SOURCE_MISSING).with("path", path_str(p)));
        }
    }
    Ok(())
}

/// Applies `plan`. Refuses plans with conflicts; on any failure every step
/// already done is undone, so the disk is left as it was.
pub fn execute(plan: &[RenamePlanItem], mode: &ExecuteMode) -> OpResult<RenameLog> {
    let conflicts = plan.iter().filter(|i| i.conflict.is_some()).count();
    if conflicts > 0 {
        return Err(OpError::new(RENAME_HAS_CONFLICTS).with("count", conflicts));
    }
    let entries: Vec<RenameLogEntry> = match mode {
        ExecuteMode::InPlace => plan
            .iter()
            .filter(|i| i.from != i.to)
            .map(|i| RenameLogEntry {
                from: i.from.clone(),
                to: i.to.clone(),
            })
            .collect(),
        ExecuteMode::CopyTo { dir } => plan
            .iter()
            .map(|i| RenameLogEntry {
                from: i.from.clone(),
                to: dir.join(i.to.file_name().unwrap_or(i.to.as_os_str())),
            })
            .collect(),
    };
    require_sources(entries.iter().map(|e| e.from.as_path()))?;

    let moves: Vec<(PathBuf, PathBuf)> = entries
        .iter()
        .map(|e| (e.from.clone(), e.to.clone()))
        .collect();
    match mode {
        ExecuteMode::InPlace => move_batch(&moves)?,
        ExecuteMode::CopyTo { dir } => copy_batch(dir, &moves)?,
    }
    Ok(RenameLog {
        entries,
        mode: mode.clone(),
        timestamp: now_ms(),
    })
}

/// Undoes an executed plan: renames back (in place) or deletes the copies.
pub fn revert(log: &RenameLog) -> OpResult<()> {
    match log.mode {
        ExecuteMode::InPlace => {
            require_sources(log.entries.iter().map(|e| e.to.as_path()))?;
            let moves: Vec<(PathBuf, PathBuf)> = log
                .entries
                .iter()
                .map(|e| (e.to.clone(), e.from.clone()))
                .collect();
            move_batch(&moves)
        }
        ExecuteMode::CopyTo { .. } => {
            for e in &log.entries {
                match fs::remove_file(&e.to) {
                    Ok(()) => {}
                    Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                    Err(err) => return Err(io_error(&e.to, &e.to, &err)),
                }
            }
            Ok(())
        }
    }
}

/// Undo already performed renames (newest first). Returns paths that could not be restored.
fn rollback(done: &[(PathBuf, PathBuf)]) -> Vec<String> {
    let mut failed = Vec::new();
    for (src, dst) in done.iter().rev() {
        if fs::rename(dst, src).is_err() {
            failed.push(path_str(dst));
        }
    }
    failed
}

fn fail_with_rollback(done: &[(PathBuf, PathBuf)], err: OpError) -> OpError {
    rollback_error(rollback(done), err)
}

/// `err` if the rollback was clean, else a rollback failure listing `failed`.
fn rollback_error(failed: Vec<String>, err: OpError) -> OpError {
    if failed.is_empty() {
        err
    } else {
        let detail = err
            .params
            .get("detail")
            .cloned()
            .unwrap_or_else(|| err.code.clone().into());
        OpError::new(RENAME_ROLLBACK_FAILED)
            .with("paths", failed)
            .with("detail", detail)
    }
}

fn temp_path(from: &Path, i: usize) -> PathBuf {
    let dir = from.parent().unwrap_or(Path::new(""));
    let pid = std::process::id();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    (0u32..)
        .map(|n| dir.join(format!(".texopt-rename-{pid}-{stamp}-{i}-{n}.tmp")))
        .find(|p| !exists(p))
        .expect("an unused temp name exists")
}

/// Two-phase rename: every source first moves to a unique temp name, then
/// to its target. This makes swaps/cycles and case-only renames safe, and
/// any target still present in phase two belongs to someone else.
fn move_batch(moves: &[(PathBuf, PathBuf)]) -> OpResult<()> {
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(moves.len() * 2);
    let mut temps = Vec::with_capacity(moves.len());
    for (i, (from, _)) in moves.iter().enumerate() {
        let tmp = temp_path(from, i);
        if let Err(e) = fs::rename(from, &tmp) {
            return Err(fail_with_rollback(&done, io_error(from, &tmp, &e)));
        }
        done.push((from.clone(), tmp.clone()));
        temps.push(tmp);
    }
    for (tmp, (_, to)) in temps.iter().zip(moves) {
        if exists(to) {
            let err = OpError::new(RENAME_TARGET_EXISTS).with("path", path_str(to));
            return Err(fail_with_rollback(&done, err));
        }
        if let Err(e) = fs::rename(tmp, to) {
            return Err(fail_with_rollback(&done, io_error(tmp, to, &e)));
        }
        done.push((tmp.clone(), to.clone()));
    }
    Ok(())
}

fn copy_batch(dir: &Path, moves: &[(PathBuf, PathBuf)]) -> OpResult<()> {
    fs::create_dir_all(dir).map_err(|e| io_error(dir, dir, &e))?;
    let mut copied: Vec<&Path> = Vec::with_capacity(moves.len());
    let undo = |copied: &[&Path], err: OpError| {
        let failed: Vec<String> = copied
            .iter()
            .filter(|p| fs::remove_file(p).is_err())
            .map(|p| path_str(p))
            .collect();
        rollback_error(failed, err)
    };
    for (from, to) in moves {
        if exists(to) {
            return Err(undo(
                &copied,
                OpError::new(RENAME_TARGET_EXISTS).with("path", path_str(to)),
            ));
        }
        if let Err(e) = fs::copy(from, to) {
            return Err(undo(&copied, io_error(from, to, &e)));
        }
        copied.push(to);
    }
    Ok(())
}
