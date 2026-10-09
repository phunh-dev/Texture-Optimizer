//! Staging area for batch results: the image tools never write into user
//! folders on their own. A run writes its results to
//! `<root>/<tabId>/<jobId>/` (the app uses `<appCacheDir>/staging`); the user
//! reviews them and then saves them where they choose.
//!
//! Documented choices:
//! - **Ids** (`tabId`, `jobId`) are single path components made of ASCII
//!   letters, digits, `-` and `_` (no `.`, so never `..`), at most 128 bytes.
//! - **Path safety**: everything this module deletes or reads is first
//!   canonicalized and must be *strictly inside* the canonical root, so a
//!   crafted id, `..` or a symlink can never make it touch other files.
//!   Symlinks inside the staging area are never followed for deletion.
//! - **Staged names** are the source stem + the extension of the chosen
//!   format (no suffix); duplicate stems are made unique by the output
//!   planner (`hero.png`, `hero_1.png`). A metadata sidecar is staged next to
//!   its image as `<image>.json` (see [`crate::output::meta_path`]).
//! - **Save to a folder** copies every staged image (+ sidecar) and applies
//!   the conflict policy; with `autoRename` the sidecar follows the new name.
//! - **Save As** (exactly one staged image) writes the chosen path exactly:
//!   the native dialog already confirmed replacing an existing file, so the
//!   conflict policy does not apply. A missing extension gets the staged one;
//!   another supported image extension re-encodes the image to that format
//!   (with the given encoding settings); an unsupported one is refused. The
//!   sidecar goes to `<chosen>.json`.
//! - Saving never moves or deletes staged files, so it can be repeated.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::codes;
use crate::output::{
    ConflictPolicy, EncodeFormat, OutputMode, OutputSettings, copy_atomic, meta_path,
    resolve_conflict, save_image,
};
use crate::{OpError, OpResult};

/// The results of a job are gone (discarded, replaced or never staged).
/// Params: `tabId`, `jobId`.
pub const STAGING_NOT_FOUND: &str = "STAGING_NOT_FOUND";

/// A path outside the staging area (or the area itself) was refused for a
/// delete/read, or a save target lies inside the staging area. Params: `path`.
pub const STAGING_PATH_REFUSED: &str = "STAGING_PATH_REFUSED";

/// "Save As" needs exactly one staged image. Params: `count`.
pub const SAVE_NEEDS_SINGLE_FILE: &str = "SAVE_NEEDS_SINGLE_FILE";

/// Prefix of `write_atomic` temp files (never part of the results).
const TEMP_PREFIX: &str = ".texopt-";

const MAX_ID_LEN: usize = 128;

/// Reject ids that are not a single safe path component (see module docs).
pub fn validate_id(param: &str, id: &str) -> OpResult<()> {
    let ok = !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(OpError::invalid_param(param, "unsafe id"))
    }
}

fn refused(path: &Path) -> OpError {
    OpError::new(STAGING_PATH_REFUSED).with("path", path.display().to_string())
}

fn io_write(path: &Path, e: &dyn std::fmt::Display) -> OpError {
    OpError::new(codes::IO_WRITE_FAILED)
        .with("path", path.display().to_string())
        .with("detail", e.to_string())
}

/// Output settings of a staged run: the client's encoding choices, but the
/// destination is always the job folder (any client `mode` is ignored) and
/// duplicate names are always renamed (the conflict policy applies at save).
pub fn staging_settings(client: &OutputSettings, job_dir: &Path) -> OutputSettings {
    OutputSettings {
        mode: OutputMode::Folder {
            path: job_dir.display().to_string(),
        },
        conflict: ConflictPolicy::AutoRename,
        ..client.clone()
    }
}

/// One staged result image and its metadata sidecar (if any).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedFile {
    pub image: PathBuf,
    pub sidecar: Option<PathBuf>,
}

/// Where staged results are saved (mirrors the TS `SaveTarget`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SaveTarget {
    /// Copy every staged image into this folder (conflict policy applies).
    Folder { path: String },
    /// "Save As": the single staged image goes to exactly this path.
    File { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFile {
    /// Staged image.
    pub from: String,
    /// Written image.
    pub to: String,
    /// Written sidecar, if the image had one.
    pub sidecar: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveFailure {
    pub path: String,
    pub error: OpError,
}

/// Outcome of a save (mirrors the TS `SaveReport`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveReport {
    /// Folder the files were saved to.
    pub destination: String,
    pub saved: Vec<SavedFile>,
    /// Targets left alone because they exist and the policy is `skip`.
    pub skipped: Vec<String>,
    pub failed: Vec<SaveFailure>,
}

/// The staging root folder and every operation on it.
#[derive(Debug, Clone)]
pub struct StagingRoot {
    root: PathBuf,
}

impl StagingRoot {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// `<root>/<tabId>` (not created).
    pub fn tab_dir(&self, tab_id: &str) -> OpResult<PathBuf> {
        validate_id("tabId", tab_id)?;
        Ok(self.root.join(tab_id))
    }

    /// `<root>/<tabId>/<jobId>` (not created).
    pub fn job_dir(&self, tab_id: &str, job_id: &str) -> OpResult<PathBuf> {
        validate_id("jobId", job_id)?;
        Ok(self.tab_dir(tab_id)?.join(job_id))
    }

    /// Canonical `path` when it is strictly inside the (existing) root.
    pub fn ensure_inside(&self, path: &Path) -> OpResult<PathBuf> {
        let root = dunce::canonicalize(&self.root).map_err(|_| refused(path))?;
        let canonical = dunce::canonicalize(path).map_err(|_| refused(path))?;
        if canonical != root && canonical.starts_with(&root) {
            Ok(canonical)
        } else {
            Err(refused(path))
        }
    }

    /// Delete `path` (file or folder) when it is strictly inside the root.
    /// Returns false when there was nothing to delete. Symlinks are refused.
    pub fn remove_inside(&self, path: &Path) -> OpResult<bool> {
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            return Ok(false);
        };
        if meta.file_type().is_symlink() {
            return Err(refused(path));
        }
        let canonical = self.ensure_inside(path)?;
        let result = if meta.is_dir() {
            std::fs::remove_dir_all(&canonical)
        } else {
            std::fs::remove_file(&canonical)
        };
        result.map_err(|e| io_write(&canonical, &e))?;
        Ok(true)
    }

    /// Delete the previous results of `tab_id` and create an empty job
    /// folder. Returns its canonical path.
    pub fn prepare_job(&self, tab_id: &str, job_id: &str) -> OpResult<PathBuf> {
        let job_dir = self.job_dir(tab_id, job_id)?;
        self.discard_tab(tab_id)?;
        std::fs::create_dir_all(&job_dir).map_err(|e| io_write(&job_dir, &e))?;
        self.ensure_inside(&job_dir)
    }

    /// Delete every staged result of `tab_id`.
    pub fn discard_tab(&self, tab_id: &str) -> OpResult<bool> {
        let dir = self.tab_dir(tab_id)?;
        self.remove_inside(&dir)
    }

    /// Delete the staged results of one job.
    pub fn discard_job(&self, tab_id: &str, job_id: &str) -> OpResult<bool> {
        let dir = self.job_dir(tab_id, job_id)?;
        self.remove_inside(&dir)
    }

    /// Delete the whole staging area (app start).
    pub fn wipe(&self) -> OpResult<bool> {
        match std::fs::symlink_metadata(&self.root) {
            Err(_) => Ok(false),
            Ok(meta) if meta.file_type().is_symlink() => Err(refused(&self.root)),
            Ok(_) => std::fs::remove_dir_all(&self.root)
                .map(|()| true)
                .map_err(|e| io_write(&self.root, &e)),
        }
    }

    fn existing_job_dir(&self, tab_id: &str, job_id: &str) -> OpResult<PathBuf> {
        let dir = self.job_dir(tab_id, job_id)?;
        if !dir.is_dir() {
            return Err(OpError::new(STAGING_NOT_FOUND)
                .with("tabId", tab_id)
                .with("jobId", job_id));
        }
        self.ensure_inside(&dir)
    }

    /// Staged images of a job (naturally sorted by name) with their sidecars.
    pub fn list_job(&self, tab_id: &str, job_id: &str) -> OpResult<Vec<StagedFile>> {
        let dir = self.existing_job_dir(tab_id, job_id)?;
        let entries = std::fs::read_dir(&dir).map_err(|e| crate::io::io_read_error(&dir, &e))?;
        let mut files: Vec<StagedFile> = entries
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path())
            .filter(|p| {
                let name = p.file_name().unwrap_or_default().to_string_lossy();
                !name.starts_with(TEMP_PREFIX)
                    && crate::io::is_supported_extension(&crate::io::extension_of(p))
            })
            .map(|image| {
                let sidecar = Some(meta_path(&image)).filter(|s| s.is_file());
                StagedFile { image, sidecar }
            })
            .collect();
        files.sort_by(|a, b| {
            crate::io::natural_cmp(&a.image.to_string_lossy(), &b.image.to_string_lossy())
        });
        Ok(files)
    }

    /// Refuse save targets inside the staging area (results must leave it).
    fn refuse_inside_staging(&self, target: &Path) -> OpResult<()> {
        let Ok(root) = dunce::canonicalize(&self.root) else {
            return Ok(());
        };
        // The target may not exist yet: check its closest existing ancestor.
        let mut probe = Some(target);
        while let Some(p) = probe {
            if let Ok(canonical) = dunce::canonicalize(p) {
                return if canonical.starts_with(&root) {
                    Err(refused(target))
                } else {
                    Ok(())
                };
            }
            probe = p.parent();
        }
        Ok(())
    }

    /// Copy the staged results of a job to `target` (see module docs).
    /// `encoding` is only used when Save As changes the image format.
    pub fn save_job(
        &self,
        tab_id: &str,
        job_id: &str,
        target: &SaveTarget,
        conflict: ConflictPolicy,
        encoding: &OutputSettings,
    ) -> OpResult<SaveReport> {
        let files = self.list_job(tab_id, job_id)?;
        match target {
            SaveTarget::Folder { path } => self.save_to_folder(&files, path, conflict),
            SaveTarget::File { path } => self.save_as(&files, path, encoding),
        }
    }

    fn save_to_folder(
        &self,
        files: &[StagedFile],
        folder: &str,
        conflict: ConflictPolicy,
    ) -> OpResult<SaveReport> {
        if folder.trim().is_empty() {
            return Err(OpError::invalid_param("target.path", "empty"));
        }
        let dest = PathBuf::from(folder);
        self.refuse_inside_staging(&dest)?;
        let mut report = SaveReport {
            destination: dest.display().to_string(),
            ..SaveReport::default()
        };
        for file in files {
            let Some(name) = file.image.file_name() else {
                continue;
            };
            let wanted = dest.join(name);
            match resolve_conflict(&wanted, conflict) {
                Ok(None) => report.skipped.push(wanted.display().to_string()),
                Ok(Some(to)) => copy_one(file, &to, &mut report, copy_atomic),
                Err(error) => report.failed.push(SaveFailure {
                    path: wanted.display().to_string(),
                    error,
                }),
            }
        }
        Ok(report)
    }

    fn save_as(
        &self,
        files: &[StagedFile],
        path: &str,
        encoding: &OutputSettings,
    ) -> OpResult<SaveReport> {
        let [file] = files else {
            return Err(OpError::new(SAVE_NEEDS_SINGLE_FILE).with("count", files.len()));
        };
        if path.trim().is_empty() {
            return Err(OpError::invalid_param("target.path", "empty"));
        }
        let staged_ext = crate::io::extension_of(&file.image);
        let mut to = PathBuf::from(path);
        if to.extension().is_none_or(|e| e.is_empty()) {
            to.set_extension(&staged_ext);
        }
        self.refuse_inside_staging(&to)?;
        let ext = crate::io::extension_of(&to);
        let wanted = EncodeFormat::from_extension(&ext).ok_or_else(|| {
            OpError::new(codes::IMG_UNSUPPORTED_FORMAT)
                .with("format", ext.clone())
                .with("path", to.display().to_string())
        })?;
        let same = EncodeFormat::from_extension(&staged_ext) == Some(wanted);
        let mut report = SaveReport {
            destination: to
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            ..SaveReport::default()
        };
        copy_one(file, &to, &mut report, |from, to| {
            if same {
                copy_atomic(from, to)
            } else {
                let img = crate::io::load_image(from)?;
                save_image(&img, to, encoding)
            }
        });
        Ok(report)
    }
}

/// Write `file` to `to` with `write_image`, then its sidecar to `<to>.json`,
/// recording the outcome in `report`.
fn copy_one(
    file: &StagedFile,
    to: &Path,
    report: &mut SaveReport,
    write_image: impl Fn(&Path, &Path) -> OpResult<()>,
) {
    if let Err(error) = write_image(&file.image, to) {
        report.failed.push(SaveFailure {
            path: to.display().to_string(),
            error,
        });
        return;
    }
    let mut sidecar = None;
    if let Some(src) = &file.sidecar {
        let dst = meta_path(to);
        match copy_atomic(src, &dst) {
            Ok(()) => sidecar = Some(dst.display().to_string()),
            Err(error) => report.failed.push(SaveFailure {
                path: dst.display().to_string(),
                error,
            }),
        }
    }
    report.saved.push(SavedFile {
        from: file.image.display().to_string(),
        to: to.display().to_string(),
        sidecar,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    struct Env {
        _dir: tempfile::TempDir,
        base: PathBuf,
        staging: StagingRoot,
    }

    fn env() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let base = dunce::canonicalize(dir.path()).unwrap();
        let staging = StagingRoot::new(base.join("cache").join("staging"));
        Env {
            _dir: dir,
            base,
            staging,
        }
    }

    /// Stage `names` (+ a sidecar for names in `with_meta`) for tab/job.
    fn stage(e: &Env, tab: &str, job: &str, names: &[&str], with_meta: &[&str]) -> PathBuf {
        let dir = e.staging.prepare_job(tab, job).unwrap();
        for name in names {
            let p = dir.join(name);
            fixtures::solid(4, 2, fixtures::RED).save(&p).unwrap();
            if with_meta.contains(name) {
                std::fs::write(meta_path(&p), format!("{{\"of\":\"{name}\"}}")).unwrap();
            }
        }
        dir
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn ids_must_be_single_safe_components() {
        for ok in ["tab-1-ab12cd34", "job-7", "A_b-9"] {
            validate_id("tabId", ok).unwrap();
        }
        let long = "a".repeat(MAX_ID_LEN + 1);
        for bad in [
            "", ".", "..", "../x", "a/b", "a\\b", "a b", "a.b", "C:", "é", &long,
        ] {
            let err = validate_id("tabId", bad).unwrap_err();
            assert_eq!(err.code, codes::INVALID_PARAMS, "{bad:?}");
            assert_eq!(err.params["param"], "tabId");
        }
        let e = env();
        assert!(e.staging.job_dir("tab", "..").is_err());
        assert!(e.staging.prepare_job("..", "job-1").is_err());
        assert!(e.staging.discard_tab("../cache").is_err());
        assert!(!e.base.join("cache").join("staging").exists());
    }

    #[test]
    fn prepare_job_replaces_the_previous_results_of_that_tab_only() {
        let e = env();
        let a1 = stage(&e, "tab-a", "job-1", &["x.png"], &[]);
        let b1 = stage(&e, "tab-b", "job-2", &["y.png"], &[]);
        assert!(a1.starts_with(dunce::canonicalize(e.staging.path()).unwrap()));
        assert_eq!(a1, e.staging.path().join("tab-a").join("job-1"));

        let a2 = e.staging.prepare_job("tab-a", "job-3").unwrap();
        assert!(a2.is_dir());
        assert!(names_in(&a2).is_empty());
        assert!(!a1.exists(), "old job of the tab is deleted");
        assert_eq!(names_in(&b1), vec!["y.png"], "other tabs are untouched");
    }

    #[test]
    fn discard_deletes_only_that_tab() {
        let e = env();
        let a = stage(&e, "tab-a", "job-1", &["x.png"], &[]);
        let b = stage(&e, "tab-b", "job-2", &["y.png"], &[]);
        assert!(e.staging.discard_tab("tab-a").unwrap());
        assert!(!a.exists());
        assert!(!e.staging.path().join("tab-a").exists());
        assert!(b.join("y.png").is_file());
        assert!(!e.staging.discard_tab("tab-a").unwrap(), "nothing left");
        assert!(!e.staging.discard_tab("never-staged").unwrap());

        assert!(e.staging.discard_job("tab-b", "job-2").unwrap());
        assert!(!b.exists());
        assert!(e.staging.path().join("tab-b").is_dir());
    }

    #[test]
    fn wipe_removes_the_whole_staging_root_and_nothing_else() {
        let e = env();
        stage(&e, "tab-a", "job-1", &["x.png"], &[]);
        stage(&e, "tab-b", "job-2", &["y.png"], &[]);
        let sibling = e.base.join("cache").join("thumbs");
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(sibling.join("keep.webp"), b"k").unwrap();
        assert!(e.staging.wipe().unwrap());
        assert!(!e.staging.path().exists());
        assert!(sibling.join("keep.webp").is_file());
        assert!(!e.staging.wipe().unwrap(), "missing root is fine");
    }

    #[test]
    fn deleting_outside_the_staging_root_is_refused() {
        let e = env();
        stage(&e, "tab-a", "job-1", &["x.png"], &[]);
        let outside = e.base.join("user");
        std::fs::create_dir_all(&outside).unwrap();
        let precious = outside.join("precious.png");
        std::fs::write(&precious, b"p").unwrap();

        let root = e.staging.path().to_path_buf();
        let dotdot = root.join("..").join("..").join("user").join("precious.png");
        for path in [precious.clone(), outside.clone(), dotdot, root.clone()] {
            let err = e.staging.remove_inside(&path).unwrap_err();
            assert_eq!(err.code, STAGING_PATH_REFUSED, "{path:?}");
        }
        assert!(precious.is_file());
        assert!(
            root.is_dir(),
            "the root itself is never removed by remove_inside"
        );
        assert!(e.staging.ensure_inside(&precious).is_err());
        assert!(e.staging.ensure_inside(&root.join("tab-a")).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_never_lead_outside_the_staging_root() {
        let e = env();
        let outside = e.base.join("user");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("precious.png"), b"p").unwrap();
        std::fs::create_dir_all(e.staging.path()).unwrap();
        std::os::unix::fs::symlink(&outside, e.staging.path().join("tab-evil")).unwrap();
        assert_eq!(
            e.staging.discard_tab("tab-evil").unwrap_err().code,
            STAGING_PATH_REFUSED
        );
        assert!(e.staging.list_job("tab-evil", "job-1").is_err());
        assert!(outside.join("precious.png").is_file());
    }

    #[test]
    fn list_job_returns_sorted_images_with_sidecars() {
        let e = env();
        let dir = stage(
            &e,
            "tab-a",
            "job-1",
            &["b10.png", "b2.png", "a.tga"],
            &["b2.png"],
        );
        std::fs::write(dir.join(".texopt-123.tmp"), b"tmp").unwrap();
        std::fs::write(dir.join(".texopt-x.png"), b"tmp").unwrap();
        let files = e.staging.list_job("tab-a", "job-1").unwrap();
        let names: Vec<_> = files
            .iter()
            .map(|f| f.image.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.tga", "b2.png", "b10.png"]);
        assert_eq!(files[1].sidecar, Some(dir.join("b2.png.json")));
        assert_eq!(files[0].sidecar, None);

        let err = e.staging.list_job("tab-a", "job-0").unwrap_err();
        assert_eq!(err.code, STAGING_NOT_FOUND);
        assert_eq!(err.params["jobId"], "job-0");
    }

    fn png_settings() -> OutputSettings {
        OutputSettings::default()
    }

    #[test]
    fn save_to_folder_applies_each_conflict_policy() {
        let e = env();
        stage(&e, "t", "j", &["a.png", "b.png"], &["a.png"]);
        let folder = |name: &str| {
            let d = e.base.join(name);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("a.png"), b"old").unwrap();
            std::fs::write(d.join("a.png.json"), b"old meta").unwrap();
            d
        };
        let save = |dir: &Path, policy| {
            e.staging
                .save_job(
                    "t",
                    "j",
                    &SaveTarget::Folder {
                        path: dir.display().to_string(),
                    },
                    policy,
                    &png_settings(),
                )
                .unwrap()
        };

        let over = folder("over");
        let r = save(&over, ConflictPolicy::Overwrite);
        assert_eq!(r.destination, over.display().to_string());
        assert_eq!(r.saved.len(), 2);
        assert!(r.skipped.is_empty() && r.failed.is_empty());
        assert_ne!(std::fs::read(over.join("a.png")).unwrap(), b"old");
        assert_eq!(
            std::fs::read_to_string(over.join("a.png.json")).unwrap(),
            "{\"of\":\"a.png\"}"
        );
        assert!(over.join("b.png").is_file());
        assert!(!over.join("b.png.json").exists(), "no meta, no sidecar");

        let skip = folder("skip");
        let r = save(&skip, ConflictPolicy::Skip);
        assert_eq!(r.skipped, vec![skip.join("a.png").display().to_string()]);
        assert_eq!(r.saved.len(), 1);
        assert_eq!(std::fs::read(skip.join("a.png")).unwrap(), b"old");
        assert_eq!(std::fs::read(skip.join("a.png.json")).unwrap(), b"old meta");
        assert!(skip.join("b.png").is_file());

        let rename = folder("rename");
        let r = save(&rename, ConflictPolicy::AutoRename);
        assert_eq!(r.saved.len(), 2);
        assert_eq!(r.saved[0].to, rename.join("a_1.png").display().to_string());
        assert_eq!(
            r.saved[0].sidecar.as_deref(),
            Some(rename.join("a_1.png.json").display().to_string().as_str())
        );
        assert_eq!(std::fs::read(rename.join("a.png")).unwrap(), b"old");
        assert_eq!(
            names_in(&rename),
            vec!["a.png", "a.png.json", "a_1.png", "a_1.png.json", "b.png"]
        );
    }

    #[test]
    fn save_can_be_repeated_and_keeps_the_staged_files() {
        let e = env();
        let staged = stage(&e, "t", "j", &["a.png", "b.png"], &[]);
        for name in ["one", "two"] {
            let dest = e.base.join(name);
            let r = e
                .staging
                .save_job(
                    "t",
                    "j",
                    &SaveTarget::Folder {
                        path: dest.display().to_string(),
                    },
                    ConflictPolicy::AutoRename,
                    &png_settings(),
                )
                .unwrap();
            assert_eq!(r.saved.len(), 2);
            assert_eq!(names_in(&dest), vec!["a.png", "b.png"]);
        }
        assert_eq!(names_in(&staged), vec!["a.png", "b.png"]);
    }

    #[test]
    fn save_as_writes_the_exact_path_with_its_sidecar() {
        let e = env();
        stage(&e, "t", "j", &["hero.png"], &["hero.png"]);
        let dest = e.base.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let chosen = dest.join("My Hero.png");
        std::fs::write(&chosen, b"old").unwrap();
        let save = |path: &Path| {
            e.staging.save_job(
                "t",
                "j",
                &SaveTarget::File {
                    path: path.display().to_string(),
                },
                // The OS dialog confirmed the overwrite: the policy is not used.
                ConflictPolicy::Skip,
                &png_settings(),
            )
        };
        let r = save(&chosen).unwrap();
        assert_eq!(r.destination, dest.display().to_string());
        assert_eq!(r.saved.len(), 1);
        assert_eq!(r.saved[0].to, chosen.display().to_string());
        let img = image::open(&chosen).unwrap();
        assert_eq!((img.width(), img.height()), (4, 2));
        assert_eq!(
            std::fs::read_to_string(dest.join("My Hero.png.json")).unwrap(),
            "{\"of\":\"hero.png\"}"
        );
        assert_eq!(names_in(&dest), vec!["My Hero.png", "My Hero.png.json"]);

        // No extension: the staged one is appended.
        let r = save(&dest.join("plain")).unwrap();
        assert_eq!(r.saved[0].to, dest.join("plain.png").display().to_string());
        assert!(dest.join("plain.png").is_file());

        // Another supported extension re-encodes to that format.
        save(&dest.join("as.tga")).unwrap();
        let tga = image::ImageReader::open(dest.join("as.tga"))
            .unwrap()
            .with_guessed_format()
            .unwrap();
        assert_ne!(tga.format(), Some(image::ImageFormat::Png));
        let decoded = image::ImageReader::with_format(
            std::io::BufReader::new(std::fs::File::open(dest.join("as.tga")).unwrap()),
            image::ImageFormat::Tga,
        )
        .decode()
        .unwrap()
        .into_rgba8();
        assert_eq!(decoded.get_pixel(0, 0), &fixtures::RED);

        // Unsupported extension is refused, nothing written.
        let err = save(&dest.join("x.gif")).unwrap_err();
        assert_eq!(err.code, codes::IMG_UNSUPPORTED_FORMAT);
        assert!(!dest.join("x.gif").exists());
    }

    #[test]
    fn save_as_needs_exactly_one_staged_image() {
        let e = env();
        stage(&e, "t", "j", &["a.png", "b.png"], &[]);
        let err = e
            .staging
            .save_job(
                "t",
                "j",
                &SaveTarget::File {
                    path: e.base.join("x.png").display().to_string(),
                },
                ConflictPolicy::Overwrite,
                &png_settings(),
            )
            .unwrap_err();
        assert_eq!(err.code, SAVE_NEEDS_SINGLE_FILE);
        assert_eq!(err.params["count"], 2);
    }

    #[test]
    fn saving_into_the_staging_area_or_from_missing_jobs_is_refused() {
        let e = env();
        let staged = stage(&e, "t", "j", &["a.png"], &[]);
        let inside = e.staging.path().join("t").join("new-folder");
        let err = e
            .staging
            .save_job(
                "t",
                "j",
                &SaveTarget::Folder {
                    path: inside.display().to_string(),
                },
                ConflictPolicy::Overwrite,
                &png_settings(),
            )
            .unwrap_err();
        assert_eq!(err.code, STAGING_PATH_REFUSED);
        let err = e
            .staging
            .save_job(
                "t",
                "j",
                &SaveTarget::File {
                    path: staged.join("a.png").display().to_string(),
                },
                ConflictPolicy::Overwrite,
                &png_settings(),
            )
            .unwrap_err();
        assert_eq!(err.code, STAGING_PATH_REFUSED);

        let err = e
            .staging
            .save_job(
                "t",
                "gone",
                &SaveTarget::Folder {
                    path: e.base.join("out").display().to_string(),
                },
                ConflictPolicy::Overwrite,
                &png_settings(),
            )
            .unwrap_err();
        assert_eq!(err.code, STAGING_NOT_FOUND);
        let err = e
            .staging
            .save_job(
                "..",
                "j",
                &SaveTarget::Folder {
                    path: e.base.join("out").display().to_string(),
                },
                ConflictPolicy::Overwrite,
                &png_settings(),
            )
            .unwrap_err();
        assert_eq!(err.code, codes::INVALID_PARAMS);
    }

    #[test]
    fn staging_settings_ignore_the_client_destination_and_policy() {
        let client = OutputSettings {
            mode: OutputMode::InPlace,
            format: crate::output::OutputFormat::Webp,
            conflict: ConflictPolicy::Overwrite,
            write_meta: true,
            jpg_quality: 42,
            ..OutputSettings::default()
        };
        let s = staging_settings(&client, Path::new("/stage/t/j"));
        assert_eq!(
            s.mode,
            OutputMode::Folder {
                path: Path::new("/stage/t/j").display().to_string()
            }
        );
        assert_eq!(s.conflict, ConflictPolicy::AutoRename);
        assert_eq!(s.format, crate::output::OutputFormat::Webp);
        assert!(s.write_meta);
        assert_eq!(s.jpg_quality, 42);
    }

    #[test]
    fn save_target_and_report_serde_match_ts() {
        let t: SaveTarget =
            serde_json::from_value(serde_json::json!({ "kind": "folder", "path": "D:/x" }))
                .unwrap();
        assert_eq!(
            t,
            SaveTarget::Folder {
                path: "D:/x".into()
            }
        );
        let t: SaveTarget =
            serde_json::from_value(serde_json::json!({ "kind": "file", "path": "D:/x.png" }))
                .unwrap();
        assert_eq!(
            t,
            SaveTarget::File {
                path: "D:/x.png".into()
            }
        );
        let report = SaveReport {
            destination: "D:/x".into(),
            saved: vec![SavedFile {
                from: "s".into(),
                to: "d".into(),
                sidecar: None,
            }],
            skipped: vec!["k".into()],
            failed: vec![],
        };
        assert_eq!(
            serde_json::to_value(&report).unwrap(),
            serde_json::json!({
                "destination": "D:/x",
                "saved": [{ "from": "s", "to": "d", "sidecar": null }],
                "skipped": ["k"],
                "failed": []
            })
        );
    }
}
