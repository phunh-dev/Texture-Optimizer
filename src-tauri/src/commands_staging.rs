//! Staged results of the image tools: `run_op` processes into
//! `<appCacheDir>/staging/<tabId>/<jobId>/` (see `texopt_core::staging`), the
//! user reviews the results and saves them with `save_results`, or drops them
//! with `discard_results` (also called when the tab closes). The whole staging
//! area is wiped on app start.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::State;
use texopt_core::OpResult;
use texopt_core::io::ImportedFile;
use texopt_core::output::{ConflictPolicy, OutputSettings};
use texopt_core::staging::{SaveReport, SaveTarget, StagingRoot};

use crate::error::{AppError, parse_arg};

/// Folder name of the staging area inside the app cache dir.
pub const STAGING_DIR: &str = "staging";

/// Managed state: the staging root (known once the app handle exists) and
/// the job whose results each tab currently shows.
#[derive(Debug, Default)]
pub struct Staging {
    root: OnceLock<StagingRoot>,
    current: Mutex<HashMap<String, String>>,
}

impl Staging {
    /// Set the root and wipe what a previous app session left there.
    /// Returns false when the root was already set (it never changes).
    pub fn init(&self, root: PathBuf) -> bool {
        let staging = StagingRoot::new(root);
        if let Err(e) = staging.wipe() {
            log::warn!("could not wipe the staging area: {e}");
        }
        self.root.set(staging).is_ok()
    }

    pub fn root(&self) -> &StagingRoot {
        self.root.get_or_init(|| {
            StagingRoot::new(
                std::env::temp_dir()
                    .join("texture-optimizer")
                    .join(STAGING_DIR),
            )
        })
    }

    fn current(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.current.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Start a staged job: drop the tab's previous results and create the
    /// (canonical) job folder.
    pub fn begin(&self, tab_id: &str, job_id: &str) -> OpResult<PathBuf> {
        let dir = self.root().prepare_job(tab_id, job_id)?;
        self.current()
            .insert(tab_id.to_string(), job_id.to_string());
        Ok(dir)
    }

    /// A job ended. When its results were discarded meanwhile (tab closed or
    /// results dropped while running), delete what the job still wrote.
    pub fn finish(&self, tab_id: &str, job_id: &str) {
        let still_current = self.current().get(tab_id).is_some_and(|j| j == job_id);
        if !still_current && let Err(e) = self.root().discard_job(tab_id, job_id) {
            log::warn!("could not delete discarded results: {e}");
        }
    }

    /// Delete every staged result of a tab.
    pub fn discard(&self, tab_id: &str) -> OpResult<bool> {
        self.current().remove(tab_id);
        self.root().discard_tab(tab_id)
    }
}

/// A staged result image (mirrors the TS `StagedResult`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedResult {
    /// Header info of the staged image (path inside the staging area).
    pub file: ImportedFile,
    /// Its metadata sidecar (`<image>.json`), if any.
    pub sidecar: Option<String>,
}

/// Staged images of a job, naturally sorted, with header info for the grid.
pub fn list_staged(
    staging: &StagingRoot,
    tab_id: &str,
    job_id: &str,
) -> OpResult<Vec<StagedResult>> {
    staging
        .list_job(tab_id, job_id)?
        .into_iter()
        .map(|f| {
            Ok(StagedResult {
                file: texopt_core::io::inspect_file(&f.image)?,
                sidecar: f.sidecar.map(|p| p.display().to_string()),
            })
        })
        .collect()
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(AppError::unknown)?
}

/// Header info of the staged results of a job.
#[tauri::command]
pub async fn list_results(
    staging: State<'_, Staging>,
    tab_id: String,
    job_id: String,
) -> Result<Vec<StagedResult>, AppError> {
    let root = staging.root().clone();
    blocking(move || Ok(list_staged(&root, &tab_id, &job_id)?)).await
}

/// Copy the staged results of a job to a folder (conflict policy applies)
/// or, for a single image, to the exact "Save As" path. `output` gives the
/// encoding settings used when Save As changes the format.
#[tauri::command]
pub async fn save_results(
    staging: State<'_, Staging>,
    tab_id: String,
    job_id: String,
    target: serde_json::Value,
    conflict: serde_json::Value,
    output: Option<serde_json::Value>,
) -> Result<SaveReport, AppError> {
    let target: SaveTarget = parse_arg("target", target)?;
    let conflict: ConflictPolicy = parse_arg("conflict", conflict)?;
    let encoding: OutputSettings = match output {
        Some(v) => parse_arg("output", v)?,
        None => OutputSettings::default(),
    };
    let root = staging.root().clone();
    blocking(move || Ok(root.save_job(&tab_id, &job_id, &target, conflict, &encoding)?)).await
}

/// Delete the staged results of a tab (Discard, or the tab was closed).
#[tauri::command]
pub fn discard_results(staging: State<'_, Staging>, tab_id: String) -> Result<(), AppError> {
    staging.discard(&tab_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use texopt_core::fixtures;

    use super::*;

    fn staging() -> (tempfile::TempDir, Staging) {
        let dir = tempfile::tempdir().unwrap();
        let s = Staging::default();
        let root = texopt_core::io::normalize_path(dir.path())
            .unwrap()
            .join(STAGING_DIR);
        assert!(s.init(root));
        (dir, s)
    }

    #[test]
    fn init_wipes_leftovers_of_a_previous_session() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(STAGING_DIR);
        let old = root.join("tab-old").join("job-1");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("a.png"), b"x").unwrap();
        let s = Staging::default();
        assert!(s.init(root.clone()));
        assert!(!root.exists());
        assert!(!s.init(root), "root is set once");
    }

    #[test]
    fn begin_finish_and_discard() {
        let (_dir, s) = staging();
        let job1 = s.begin("tab-a", "job-1").unwrap();
        assert!(job1.is_dir());
        s.finish("tab-a", "job-1");
        assert!(job1.is_dir(), "results of the current job stay");

        // Discarded while running: what the job still writes is cleaned up.
        let job2 = s.begin("tab-a", "job-2").unwrap();
        assert!(!job1.exists(), "a new run replaces the previous results");
        assert!(s.discard("tab-a").unwrap());
        std::fs::create_dir_all(&job2).unwrap();
        std::fs::write(job2.join("late.png"), b"x").unwrap();
        s.finish("tab-a", "job-2");
        assert!(!job2.exists());

        assert!(s.begin("../x", "job-3").is_err());
        assert!(s.discard("a/b").is_err());
    }

    #[test]
    fn list_staged_returns_header_info_and_sidecars() {
        let (_dir, s) = staging();
        let job = s.begin("tab-a", "job-1").unwrap();
        fixtures::solid(6, 3, fixtures::RED)
            .save(job.join("b.png"))
            .unwrap();
        fixtures::solid(2, 2, fixtures::RED)
            .save(job.join("a.png"))
            .unwrap();
        std::fs::write(job.join("a.png.json"), b"{}").unwrap();
        let list = list_staged(s.root(), "tab-a", "job-1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].file.name, "a.png");
        assert_eq!(
            list[0].sidecar.as_deref(),
            Some(job.join("a.png.json").display().to_string().as_str())
        );
        assert_eq!((list[1].file.width, list[1].file.height), (6, 3));
        assert_eq!(list[1].file.path, job.join("b.png").display().to_string());
        assert_eq!(
            serde_json::to_value(&list[1]).unwrap()["sidecar"],
            serde_json::Value::Null
        );
        assert_eq!(
            list_staged(s.root(), "tab-a", "job-9").unwrap_err().code,
            texopt_core::staging::STAGING_NOT_FOUND
        );
    }
}
