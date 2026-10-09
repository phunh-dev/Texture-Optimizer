//! Batch job runner, independent of Tauri so it can be unit tested.
//!
//! [`run_batch`] processes files in parallel (rayon) and keeps results in
//! input order. Cancellation is cooperative: the flag is checked before each
//! file starts, so files already in flight finish and every file not started
//! yet gets a `CANCELLED` error. A failing file never aborts the batch.
//! Progress callbacks are serialized, monotonic, throttled to one per
//! `min_interval`, and the final `(total, total)` call always happens.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use texopt_core::error::codes;
use texopt_core::ops::OpOutput;
use texopt_core::output::{OutputPlanner, save_image, save_meta};
use texopt_core::{ImageBuf, OpError, OpResult};

/// `cancel_job` was called with an unknown or already finished job. Params: `jobId`.
pub const JOB_NOT_FOUND: &str = "JOB_NOT_FOUND";

/// Max progress events per second (~20/s).
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

pub const JOB_PROGRESS_EVENT: &str = "job://progress";
pub const JOB_FINISHED_EVENT: &str = "job://finished";

/// Mirrors the TS `JobFileResult`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobFileResult {
    pub input: String,
    pub output: Option<String>,
    pub error: Option<OpError>,
    pub meta: Option<Value>,
}

/// Mirrors the TS `JobProgressEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgressEvent {
    pub job_id: String,
    pub tab_id: String,
    pub done: usize,
    pub total: usize,
    pub current_path: Option<String>,
}

/// Mirrors the TS `JobFinishedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobFinishedEvent {
    pub job_id: String,
    pub tab_id: String,
    pub cancelled: bool,
    pub results: Vec<JobFileResult>,
}

/// What a per-file operation produces on success.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileOutput {
    pub output: Option<PathBuf>,
    pub meta: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchOutcome {
    pub results: Vec<JobFileResult>,
    /// True when at least one file was not processed because of cancellation.
    pub cancelled: bool,
}

/// Progress snapshot handed to the progress callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    /// Last finished file (`None` on the final call).
    pub current: Option<PathBuf>,
}

struct Throttle {
    done: usize,
    last_emit: Option<Instant>,
    emitted_final: bool,
}

/// Run `op` on every file. See the module docs for the guarantees.
pub fn run_batch<F, P>(
    files: &[PathBuf],
    op: F,
    progress: P,
    cancel: &AtomicBool,
    min_interval: Duration,
) -> BatchOutcome
where
    F: Fn(&Path) -> Result<FileOutput, OpError> + Sync,
    P: Fn(Progress) + Sync,
{
    let total = files.len();
    let throttle = Mutex::new(Throttle {
        done: 0,
        last_emit: None,
        emitted_final: false,
    });

    let finish_one = |path: &Path| {
        let mut t = throttle.lock().unwrap_or_else(|e| e.into_inner());
        t.done += 1;
        let now = Instant::now();
        let due = t.done == total
            || t.last_emit
                .is_none_or(|last| now.duration_since(last) >= min_interval);
        if due {
            t.last_emit = Some(now);
            t.emitted_final |= t.done == total;
            progress(Progress {
                done: t.done,
                total,
                current: Some(path.to_path_buf()),
            });
        }
    };

    let results: Vec<JobFileResult> = files
        .par_iter()
        .map(|path| {
            let input = path.display().to_string();
            let result = if cancel.load(Ordering::SeqCst) {
                JobFileResult {
                    input,
                    output: None,
                    error: Some(OpError::new(codes::CANCELLED)),
                    meta: None,
                }
            } else {
                match op(path) {
                    Ok(out) => JobFileResult {
                        input,
                        output: out.output.map(|p| p.display().to_string()),
                        error: None,
                        meta: out.meta,
                    },
                    Err(error) => JobFileResult {
                        input,
                        output: None,
                        error: Some(error),
                        meta: None,
                    },
                }
            };
            finish_one(path);
            result
        })
        .collect();

    let t = throttle.into_inner().unwrap_or_else(|e| e.into_inner());
    if !t.emitted_final {
        progress(Progress {
            done: total,
            total,
            current: None,
        });
    }
    let cancelled = results
        .iter()
        .any(|r| r.error.as_ref().is_some_and(|e| e.code == codes::CANCELLED));
    BatchOutcome { results, cancelled }
}

/// The standard per-file pipeline of `run_op`: plan the output path (so a
/// `skip` conflict costs no decoding), decode, run `op`, encode + write, then
/// write the op metadata to `<output>.json` when `writeMeta` is on.
pub fn process_file(
    input: &Path,
    planner: &OutputPlanner,
    op: impl Fn(&ImageBuf) -> OpResult<OpOutput>,
) -> Result<FileOutput, OpError> {
    let target = planner.plan(input)?;
    let img = texopt_core::io::load_image(input)?;
    let out = op(&img)?;
    save_image(&out.image, &target, planner.settings())?;
    save_meta(&target, out.meta.as_ref(), planner.settings())?;
    Ok(FileOutput {
        output: Some(target),
        meta: out.meta,
    })
}

/// Live jobs and their cancellation flags.
#[derive(Debug, Default)]
pub struct JobRegistry {
    next: AtomicU64,
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl JobRegistry {
    /// Register a new job; returns its id and cancellation flag.
    pub fn create(&self) -> (String, Arc<AtomicBool>) {
        let id = format!("job-{}", self.next.fetch_add(1, Ordering::Relaxed) + 1);
        let flag = Arc::new(AtomicBool::new(false));
        self.jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.clone(), flag.clone());
        (id, flag)
    }

    pub fn cancel(&self, id: &str) -> Result<(), OpError> {
        match self.jobs.lock().unwrap_or_else(|e| e.into_inner()).get(id) {
            Some(flag) => {
                flag.store(true, Ordering::SeqCst);
                Ok(())
            }
            None => Err(OpError::new(JOB_NOT_FOUND).with("jobId", id)),
        }
    }

    pub fn finish(&self, id: &str) {
        self.jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(id)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    fn files(n: usize) -> Vec<PathBuf> {
        (0..n).map(|i| PathBuf::from(format!("f{i}.png"))).collect()
    }

    #[test]
    fn all_files_processed_in_order() {
        let input = files(200);
        let calls = AtomicUsize::new(0);
        let out = run_batch(
            &input,
            |p| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(FileOutput {
                    output: Some(p.with_extension("out")),
                    meta: Some(serde_json::json!({ "n": p.display().to_string() })),
                })
            },
            |_| {},
            &AtomicBool::new(false),
            Duration::ZERO,
        );
        assert!(!out.cancelled);
        assert_eq!(calls.load(Ordering::SeqCst), 200);
        assert_eq!(out.results.len(), 200);
        for (i, r) in out.results.iter().enumerate() {
            assert_eq!(r.input, format!("f{i}.png"));
            assert_eq!(r.output.as_deref(), Some(format!("f{i}.out").as_str()));
            assert_eq!(
                r.meta,
                Some(serde_json::json!({ "n": format!("f{i}.png") }))
            );
            assert!(r.error.is_none());
        }
    }

    #[test]
    fn progress_is_monotonic_and_ends_at_total() {
        let seen: Mutex<Vec<Progress>> = Mutex::new(Vec::new());
        let input = files(100);
        run_batch(
            &input,
            |_| Ok(FileOutput::default()),
            |p| seen.lock().unwrap().push(p),
            &AtomicBool::new(false),
            Duration::ZERO,
        );
        let seen = seen.into_inner().unwrap();
        // Unthrottled: one call per file, strictly increasing.
        assert_eq!(seen.len(), 100);
        for w in seen.windows(2) {
            assert!(w[1].done > w[0].done);
        }
        let last = seen.last().unwrap();
        assert_eq!((last.done, last.total), (100, 100));
        assert!(seen.iter().all(|p| p.total == 100));
    }

    #[test]
    fn progress_is_throttled_but_final_is_always_sent() {
        let seen: Mutex<Vec<Progress>> = Mutex::new(Vec::new());
        let input = files(50);
        run_batch(
            &input,
            |_| {
                std::thread::sleep(Duration::from_millis(2));
                Ok(FileOutput::default())
            },
            |p| seen.lock().unwrap().push(p),
            &AtomicBool::new(false),
            Duration::from_secs(3600),
        );
        let seen = seen.into_inner().unwrap();
        // First file emits immediately, then nothing until the final one.
        assert_eq!(seen.len(), 2, "{seen:?}");
        assert_eq!(seen[0].done, 1);
        assert_eq!((seen[1].done, seen[1].total), (50, 50));
    }

    #[test]
    fn empty_batch_reports_final_progress() {
        let seen: Mutex<Vec<Progress>> = Mutex::new(Vec::new());
        let out = run_batch(
            &[],
            |_| Ok(FileOutput::default()),
            |p| seen.lock().unwrap().push(p),
            &AtomicBool::new(false),
            Duration::ZERO,
        );
        assert!(out.results.is_empty());
        assert!(!out.cancelled);
        assert_eq!(
            seen.into_inner().unwrap(),
            vec![Progress {
                done: 0,
                total: 0,
                current: None
            }]
        );
    }

    #[test]
    fn cancellation_stops_early_and_marks_the_rest() {
        let input = files(400);
        let cancel = AtomicBool::new(false);
        let processed = AtomicUsize::new(0);
        let seen: Mutex<Vec<Progress>> = Mutex::new(Vec::new());
        let out = run_batch(
            &input,
            |_| {
                if processed.fetch_add(1, Ordering::SeqCst) + 1 == 5 {
                    cancel.store(true, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_millis(1));
                Ok(FileOutput::default())
            },
            |p| seen.lock().unwrap().push(p),
            &cancel,
            Duration::ZERO,
        );
        let processed = processed.load(Ordering::SeqCst);
        assert!(out.cancelled);
        assert!(processed < 400, "processed {processed}");
        let cancelled: Vec<_> = out
            .results
            .iter()
            .filter(|r| r.error.as_ref().is_some_and(|e| e.code == codes::CANCELLED))
            .collect();
        let ok = out.results.iter().filter(|r| r.error.is_none()).count();
        assert_eq!(ok, processed);
        assert_eq!(cancelled.len(), 400 - processed);
        assert!(cancelled.iter().all(|r| r.output.is_none()));
        assert_eq!(out.results.len(), 400);
        assert_eq!(seen.into_inner().unwrap().last().unwrap().done, 400);
    }

    #[test]
    fn cancelled_before_start_processes_nothing() {
        let input = files(10);
        let out = run_batch(
            &input,
            |_| panic!("must not run"),
            |_| {},
            &AtomicBool::new(true),
            Duration::ZERO,
        );
        assert!(out.cancelled);
        assert!(
            out.results
                .iter()
                .all(|r| r.error.as_ref().unwrap().code == codes::CANCELLED)
        );
    }

    #[test]
    fn per_file_errors_do_not_abort_the_batch() {
        let input = files(20);
        let out = run_batch(
            &input,
            |p| {
                let n: usize = p.to_string_lossy()[1..]
                    .trim_end_matches(".png")
                    .parse()
                    .unwrap();
                if n.is_multiple_of(3) {
                    Err(OpError::new(codes::IMG_DECODE_FAILED)
                        .with("path", p.display().to_string()))
                } else {
                    Ok(FileOutput {
                        output: Some(p.to_path_buf()),
                        meta: None,
                    })
                }
            },
            |_| {},
            &AtomicBool::new(false),
            Duration::ZERO,
        );
        assert!(!out.cancelled);
        for (i, r) in out.results.iter().enumerate() {
            if i.is_multiple_of(3) {
                let e = r.error.as_ref().unwrap();
                assert_eq!(e.code, codes::IMG_DECODE_FAILED);
                assert_eq!(e.params["path"], format!("f{i}.png"));
                assert!(r.output.is_none());
            } else {
                assert!(r.error.is_none());
                assert!(r.output.is_some());
            }
        }
    }

    #[test]
    fn payloads_serialize_like_the_ts_types() {
        let finished = JobFinishedEvent {
            job_id: "job-1".into(),
            tab_id: "tab".into(),
            cancelled: false,
            results: vec![JobFileResult {
                input: "a.png".into(),
                output: None,
                error: Some(OpError::new(codes::CANCELLED)),
                meta: None,
            }],
        };
        assert_eq!(
            serde_json::to_value(&finished).unwrap(),
            serde_json::json!({
                "jobId": "job-1", "tabId": "tab", "cancelled": false,
                "results": [{ "input": "a.png", "output": null, "error": { "code": "CANCELLED", "params": {} }, "meta": null }]
            })
        );
        let progress = JobProgressEvent {
            job_id: "j".into(),
            tab_id: "t".into(),
            done: 1,
            total: 2,
            current_path: Some("a".into()),
        };
        assert_eq!(
            serde_json::to_value(&progress).unwrap(),
            serde_json::json!({ "jobId": "j", "tabId": "t", "done": 1, "total": 2, "currentPath": "a" })
        );
    }

    fn invert(img: &ImageBuf) -> OpResult<OpOutput> {
        let mut out = img.clone();
        image::imageops::invert(&mut out);
        Ok(OpOutput {
            image: out,
            meta: Some(serde_json::json!({ "w": img.width() })),
        })
    }

    #[test]
    fn end_to_end_with_files_and_output_settings() {
        use texopt_core::fixtures;
        use texopt_core::output::{
            ConflictPolicy, OUTPUT_EXISTS_SKIPPED, OutputFormat, OutputMode, OutputSettings,
        };

        let dir = tempfile::tempdir().unwrap();
        let out_dir = dir.path().join("out");
        let mut inputs = vec![];
        for i in 0..6 {
            let p = dir.path().join(format!("img{i}.png"));
            fixtures::rect_on(8 + i, 4, fixtures::WHITE, 0, 0, 2, 2, fixtures::RED)
                .save(&p)
                .unwrap();
            inputs.push(p);
        }
        let corrupt = dir.path().join("corrupt.png");
        std::fs::write(&corrupt, b"nope").unwrap();
        inputs.insert(3, corrupt);
        // An existing output that the `skip` policy must leave alone.
        std::fs::create_dir_all(&out_dir).unwrap();
        std::fs::write(out_dir.join("img0.tga"), b"keep me").unwrap();

        let planner = OutputPlanner::new(OutputSettings {
            mode: OutputMode::Folder {
                path: out_dir.display().to_string(),
            },
            format: OutputFormat::Tga,
            conflict: ConflictPolicy::Skip,
            ..OutputSettings::default()
        })
        .unwrap();
        let outcome = run_batch(
            &inputs,
            |p| process_file(p, &planner, invert),
            |_| {},
            &AtomicBool::new(false),
            Duration::ZERO,
        );

        assert!(!outcome.cancelled);
        assert_eq!(outcome.results.len(), 7);
        assert_eq!(
            outcome.results[0].error.as_ref().unwrap().code,
            OUTPUT_EXISTS_SKIPPED
        );
        assert_eq!(std::fs::read(out_dir.join("img0.tga")).unwrap(), b"keep me");
        assert_eq!(
            outcome.results[3].error.as_ref().unwrap().code,
            codes::IMG_DECODE_FAILED
        );
        for (idx, i) in [(1, 1), (2, 2), (4, 3), (5, 4), (6, 5)] {
            let r = &outcome.results[idx];
            assert!(r.error.is_none(), "{r:?}");
            let out = PathBuf::from(r.output.as_ref().unwrap());
            assert_eq!(out, out_dir.join(format!("img{i}.tga")));
            assert_eq!(r.meta, Some(serde_json::json!({ "w": 8 + i })));
            let written = image::open(&out).unwrap().into_rgba8();
            let mut expected =
                fixtures::rect_on(8 + i, 4, fixtures::WHITE, 0, 0, 2, 2, fixtures::RED);
            image::imageops::invert(&mut expected);
            assert_eq!(written, expected);
        }
    }

    #[test]
    fn write_meta_saves_trim_offsets_next_to_each_output() {
        use texopt_core::fixtures;
        use texopt_core::output::{OutputSettings, meta_path};

        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sprite.png");
        fixtures::sprite(20, 10, 4, 2, 6, 3, fixtures::RED)
            .save(&p)
            .unwrap();
        let req: texopt_core::ops::OpRequest =
            serde_json::from_value(serde_json::json!({ "kind": "trim", "params": {} })).unwrap();

        let off = OutputPlanner::new(OutputSettings::default()).unwrap();
        let out = process_file(&p, &off, |img| texopt_core::ops::run(img, &req)).unwrap();
        assert!(
            !meta_path(out.output.as_ref().unwrap()).exists(),
            "off by default"
        );

        let settings: OutputSettings = serde_json::from_value(serde_json::json!({
            "mode": { "kind": "suffix", "suffix": "_t" },
            "writeMeta": true
        }))
        .unwrap();
        let on = OutputPlanner::new(settings).unwrap();
        let out = process_file(&p, &on, |img| texopt_core::ops::run(img, &req)).unwrap();
        let image_path = out.output.unwrap();
        assert_eq!(image_path, dir.path().join("sprite_t.png"));
        let sidecar = dir.path().join("sprite_t.png.json");
        let written: Value = serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
        let expected = serde_json::json!({
            "sourceSize": { "w": 20, "h": 10 },
            "trimRect": { "x": 4, "y": 2, "w": 6, "h": 3 }
        });
        assert_eq!(written, expected);
        assert_eq!(out.meta, Some(expected));

        // Ops without metadata never write a sidecar.
        let resize: texopt_core::ops::OpRequest =
            serde_json::from_value(serde_json::json!({ "kind": "resize", "params": {} })).unwrap();
        let q = dir.path().join("plain.png");
        fixtures::gradient(8, 8).save(&q).unwrap();
        let out = process_file(&q, &on, |img| texopt_core::ops::run(img, &resize)).unwrap();
        assert!(!meta_path(out.output.as_ref().unwrap()).exists());
    }

    #[test]
    fn real_ops_flow_through_process_file() {
        // Whatever the state of the op implementations (stub or real), the
        // pipeline reports either an OpError code or a written file.
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.png");
        texopt_core::fixtures::gradient(8, 8).save(&p).unwrap();
        let planner = OutputPlanner::new(texopt_core::output::OutputSettings::default()).unwrap();
        let req = texopt_core::ops::OpRequest::Resize(Default::default());
        match process_file(&p, &planner, |img| texopt_core::ops::run(img, &req)) {
            Ok(out) => assert!(out.output.unwrap().exists()),
            Err(e) => assert!(!e.code.is_empty()),
        }
    }

    #[test]
    fn registry_cancel_and_finish() {
        let reg = JobRegistry::default();
        let (a, flag_a) = reg.create();
        let (b, _) = reg.create();
        assert_ne!(a, b);
        assert!(reg.is_running(&a));
        reg.cancel(&a).unwrap();
        assert!(flag_a.load(Ordering::SeqCst));
        reg.finish(&a);
        let err = reg.cancel(&a).unwrap_err();
        assert_eq!(err.code, JOB_NOT_FOUND);
        assert_eq!(err.params["jobId"], a);
        assert!(reg.is_running(&b));
    }
}
