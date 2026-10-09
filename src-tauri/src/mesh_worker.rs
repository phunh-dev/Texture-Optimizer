//! Out-of-process 3D worker.
//!
//! Assimp is C++ running in-process; a crash in an importer/exporter (seen
//! with its glTF writer, see docs/spikes/3d-assimp.md) would take the whole
//! app down. Every mesh import/export therefore runs in a child process: the
//! app re-launches its own executable with [`WORKER_ARG`], which `main.rs`
//! handles before Tauri starts ([`run_if_requested`]).
//!
//! # Protocol (version [`PROTOCOL_VERSION`], JSON Lines, UTF-8)
//! * stdin: exactly one line, a [`WorkerEnvelope`]
//!   `{"protocol":1,"request":{"kind":"scan"|"preview"|"pack",…}}`.
//! * stdout: one [`WorkerMessage`] per line, tagged by `type`:
//!   `progress` (`done`, `total`, `current`), `item` (scan: one per model, in
//!   request order), then exactly one terminal `result` (`value`) or `error`
//!   (`error` = `{code, params}`). Lines that are not valid messages (stray
//!   library output) are ignored by the client.
//! * The worker exits 0 after the terminal message. A missing terminal
//!   message (crash, kill, non-zero exit) becomes `MESH_WORKER_CRASHED`.
//!
//! Cancellation kills the child process.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use texopt_core::mesh::pack::{self, AssimpExporter, ModelInfo, PackOptions, PackProgress};
use texopt_core::output::{PngCompression, encode_png};
use texopt_core::{OpError, OpResult};

/// Command-line switch that turns the app executable into a worker.
pub const WORKER_ARG: &str = "--mesh-worker";
pub const PROTOCOL_VERSION: u32 = 1;

/// The worker process died or exited without a result. Params: `exitCode`
/// (null when killed by a signal), `detail` (tail of its stderr), and `path`
/// when a scan was processing a specific model.
pub const MESH_WORKER_CRASHED: &str = "MESH_WORKER_CRASHED";
/// The worker could not be started. Params: `detail`.
pub const MESH_WORKER_SPAWN_FAILED: &str = "MESH_WORKER_SPAWN_FAILED";

/// Test hook: when set, the worker aborts (simulated native crash) before
/// handling a model whose path contains this value (`*` = right after reading
/// the request). Never set in production.
pub const CRASH_ENV: &str = "TEXOPT_MESH_WORKER_TEST_CRASH";

/// Default longest edge of preview atlas images sent back to the UI.
pub const DEFAULT_PREVIEW_MAX: u32 = 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WorkerRequest {
    /// Import each model and describe it (one `item` per path).
    Scan { paths: Vec<String> },
    /// Layout + base-colour atlas preview; nothing written.
    Preview {
        models: Vec<String>,
        options: PackOptions,
        #[serde(default = "default_preview_max")]
        preview_max_size: u32,
    },
    /// Full run writing into `output_dir`.
    Pack {
        models: Vec<String>,
        options: PackOptions,
        output_dir: String,
        base_name: String,
    },
}

fn default_preview_max() -> u32 {
    DEFAULT_PREVIEW_MAX
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerEnvelope {
    pub protocol: u32,
    pub request: WorkerRequest,
}

/// Scan outcome of one model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanItem {
    pub path: String,
    pub info: Option<ModelInfo>,
    pub error: Option<OpError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WorkerMessage {
    Progress {
        done: usize,
        total: usize,
        current: Option<String>,
    },
    Item {
        index: usize,
        item: ScanItem,
    },
    Result {
        value: Value,
    },
    Error {
        error: OpError,
    },
}

impl WorkerMessage {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Result { .. } | Self::Error { .. })
    }
}

/// One JSON line (no trailing newline).
pub fn encode<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("worker messages always serialize")
}

/// Parse a stdout line; `None` for anything that is not a message.
pub fn decode_message(line: &str) -> Option<WorkerMessage> {
    serde_json::from_str(line.trim()).ok()
}

pub fn decode_envelope(line: &str) -> OpResult<WorkerEnvelope> {
    let env: WorkerEnvelope = serde_json::from_str(line.trim())
        .map_err(|e| OpError::invalid_param("workerRequest", &e.to_string()))?;
    if env.protocol != PROTOCOL_VERSION {
        return Err(OpError::invalid_param("workerRequest", "protocolVersion")
            .with("version", env.protocol));
    }
    Ok(env)
}

// ------------------------------------------------------------------ worker

/// Preview result sent back to the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPage {
    pub width: u32,
    pub height: u32,
    /// Base64 PNG of the (possibly downscaled) page.
    pub png: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPayload {
    pub report: pack::PackReport,
    pub channel: texopt_core::mesh::TextureChannel,
    pub images: Vec<PreviewPage>,
}

fn crash_hook(path: &str) {
    if let Ok(v) = std::env::var(CRASH_ENV)
        && !v.is_empty()
        && v != "*"
        && path.contains(&v)
    {
        std::process::abort();
    }
}

fn progress_msg(p: PackProgress) -> WorkerMessage {
    WorkerMessage::Progress {
        done: p.done,
        total: p.total,
        current: p.current,
    }
}

fn to_paths(models: &[String]) -> Vec<PathBuf> {
    models.iter().map(PathBuf::from).collect()
}

/// Execute a request in this process, emitting messages (the last one is
/// terminal). Used by the worker main loop and directly by tests.
pub fn handle(request: WorkerRequest, emit: &mut dyn FnMut(WorkerMessage)) {
    let terminal = match request {
        WorkerRequest::Scan { paths } => {
            let total = paths.len();
            for (index, path) in paths.into_iter().enumerate() {
                crash_hook(&path);
                let item = match pack::inspect(Path::new(&path)) {
                    Ok(info) => ScanItem {
                        path: path.clone(),
                        info: Some(info),
                        error: None,
                    },
                    Err(e) => ScanItem {
                        path: path.clone(),
                        info: None,
                        error: Some(e),
                    },
                };
                emit(WorkerMessage::Item { index, item });
                emit(WorkerMessage::Progress {
                    done: index + 1,
                    total,
                    current: Some(path),
                });
            }
            WorkerMessage::Result { value: Value::Null }
        }
        WorkerRequest::Preview {
            models,
            options,
            preview_max_size,
        } => {
            models.iter().for_each(|m| crash_hook(m));
            let result = pack::preview(&to_paths(&models), &options, &mut |p| emit(progress_msg(p)))
                .and_then(|p| {
                    let images = p
                        .pages
                        .iter()
                        .map(|img| {
                            let small = pack::preview_downscale(img, preview_max_size);
                            let png = encode_png(&small, PngCompression::Fast).map_err(|e| {
                                OpError::new(texopt_core::error::codes::IMG_ENCODE_FAILED)
                                    .with("path", "preview")
                                    .with("detail", e.to_string())
                            })?;
                            Ok(PreviewPage {
                                width: img.width(),
                                height: img.height(),
                                png: base64::engine::general_purpose::STANDARD.encode(png),
                            })
                        })
                        .collect::<OpResult<Vec<_>>>()?;
                    Ok(PreviewPayload {
                        report: p.report,
                        channel: p.channel,
                        images,
                    })
                });
            match result {
                Ok(payload) => WorkerMessage::Result {
                    value: serde_json::to_value(payload).unwrap_or(Value::Null),
                },
                Err(error) => WorkerMessage::Error { error },
            }
        }
        WorkerRequest::Pack {
            models,
            options,
            output_dir,
            base_name,
        } => {
            models.iter().for_each(|m| crash_hook(m));
            match pack::run(
                &to_paths(&models),
                &options,
                Path::new(&output_dir),
                &base_name,
                &AssimpExporter,
                &mut |p| emit(progress_msg(p)),
            ) {
                Ok(report) => WorkerMessage::Result {
                    value: serde_json::to_value(report).unwrap_or(Value::Null),
                },
                Err(error) => WorkerMessage::Error { error },
            }
        }
    };
    emit(terminal);
}

/// Worker main loop: one request from `input`, messages to `output`.
/// Returns the process exit code.
pub fn serve(input: impl BufRead, mut output: impl Write) -> i32 {
    let mut line = String::new();
    let mut input = input;
    let mut send = |msg: WorkerMessage| {
        let _ = writeln!(output, "{}", encode(&msg));
        let _ = output.flush();
    };
    if let Err(e) = input.read_line(&mut line) {
        send(WorkerMessage::Error {
            error: OpError::invalid_param("workerRequest", &e.to_string()),
        });
        return 2;
    }
    match decode_envelope(&line) {
        Ok(env) => {
            if std::env::var(CRASH_ENV).is_ok_and(|v| v == "*") {
                std::process::abort();
            }
            handle(env.request, &mut send);
            0
        }
        Err(error) => {
            send(WorkerMessage::Error { error });
            2
        }
    }
}

/// Called first thing in `main`: runs the worker and returns its exit code
/// when the process was started with [`WORKER_ARG`].
pub fn run_if_requested() -> Option<i32> {
    if !std::env::args().skip(1).any(|a| a == WORKER_ARG) {
        return None;
    }
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    Some(serve(stdin.lock(), stdout.lock()))
}

// ------------------------------------------------------------------ client

/// How to start a worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl WorkerCommand {
    /// This executable with [`WORKER_ARG`].
    pub fn current_exe() -> OpResult<Self> {
        let program = std::env::current_exe()
            .map_err(|e| OpError::new(MESH_WORKER_SPAWN_FAILED).with("detail", e.to_string()))?;
        Ok(Self::new(program))
    }

    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: vec![WORKER_ARG.into()],
            env: Vec::new(),
        }
    }

    fn spawn(&self) -> OpResult<Child> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.spawn()
            .map_err(|e| OpError::new(MESH_WORKER_SPAWN_FAILED).with("detail", e.to_string()))
    }
}

/// Why a call produced no value.
#[derive(Debug, Clone, PartialEq)]
pub enum CallError {
    /// The cancel flag was raised; the worker was killed.
    Cancelled,
    Failed(OpError),
}

impl From<OpError> for CallError {
    fn from(e: OpError) -> Self {
        Self::Failed(e)
    }
}

/// What the client saw on stdout.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Collected {
    pub terminal: Option<WorkerMessage>,
    pub items: usize,
}

/// Feed decoded messages to `on_message` until the terminal one. Pure: used
/// on the child's stdout and directly in tests.
pub fn collect(
    lines: impl IntoIterator<Item = String>,
    on_message: &mut dyn FnMut(&WorkerMessage),
) -> Collected {
    let mut out = Collected::default();
    for line in lines {
        let Some(msg) = decode_message(&line) else {
            continue;
        };
        if matches!(msg, WorkerMessage::Item { .. }) {
            out.items += 1;
        }
        on_message(&msg);
        if msg.is_terminal() {
            out.terminal = Some(msg);
            break;
        }
    }
    out
}

/// Turn what was collected plus the exit status into the call result.
pub fn finish(collected: Collected, exit_code: Option<i32>, stderr: &str) -> Result<Value, OpError> {
    match collected.terminal {
        Some(WorkerMessage::Result { value }) => Ok(value),
        Some(WorkerMessage::Error { error }) => Err(error),
        _ => Err(crash_error(exit_code, stderr)),
    }
}

pub fn crash_error(exit_code: Option<i32>, stderr: &str) -> OpError {
    let tail: String = {
        let t = stderr.trim();
        let start = t
            .char_indices()
            .rev()
            .nth(499)
            .map(|(i, _)| i)
            .unwrap_or(0);
        t[start..].to_string()
    };
    OpError::new(MESH_WORKER_CRASHED)
        .with("exitCode", exit_code.map_or(Value::Null, Value::from))
        .with("detail", tail)
}

const POLL: Duration = Duration::from_millis(50);

/// Run one request in a fresh worker. `on_message` sees every message
/// (progress, items, terminal). Raising `cancel` kills the worker.
pub fn call(
    command: &WorkerCommand,
    request: &WorkerRequest,
    on_message: &mut dyn FnMut(&WorkerMessage),
    cancel: Option<&AtomicBool>,
) -> Result<Value, CallError> {
    let mut child = command.spawn()?;
    let envelope = WorkerEnvelope {
        protocol: PROTOCOL_VERSION,
        request: request.clone(),
    };
    if let Some(mut stdin) = child.stdin.take() {
        // A worker that died at once surfaces below as a crash.
        let _ = writeln!(stdin, "{}", encode(&envelope));
        let _ = stdin.flush();
    }
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = BufReader::new(stderr).take(1 << 20).read_to_string(&mut s);
        s
    });
    let (tx, rx) = mpsc::channel::<String>();
    let out_reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(l) => {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut collected = Collected::default();
    let mut cancelled = false;
    loop {
        if cancel.is_some_and(|c| c.load(Ordering::SeqCst)) {
            let _ = child.kill();
            cancelled = true;
            break;
        }
        match rx.recv_timeout(POLL) {
            Ok(line) => {
                let c = collect([line], on_message);
                collected.items += c.items;
                if c.terminal.is_some() {
                    collected.terminal = c.terminal;
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let status = child.wait().ok();
    let _ = out_reader.join();
    let stderr = err_reader.join().unwrap_or_default();
    if cancelled {
        return Err(CallError::Cancelled);
    }
    finish(collected, status.and_then(|s| s.code()), &stderr).map_err(CallError::Failed)
}

/// Scan models, restarting the worker after a crash: the model being
/// processed when it died gets `MESH_WORKER_CRASHED` (with `path`), the
/// remaining ones are scanned by a new worker.
pub fn scan_models(
    command: &WorkerCommand,
    paths: &[String],
    on_progress: &mut dyn FnMut(usize, usize),
    cancel: Option<&AtomicBool>,
) -> Result<Vec<ScanItem>, CallError> {
    let mut out: Vec<ScanItem> = Vec::with_capacity(paths.len());
    while out.len() < paths.len() {
        let start = out.len();
        let request = WorkerRequest::Scan {
            paths: paths[start..].to_vec(),
        };
        let mut batch: Vec<ScanItem> = Vec::new();
        let result = call(
            command,
            &request,
            &mut |m| {
                if let WorkerMessage::Item { item, .. } = m {
                    batch.push(item.clone());
                    on_progress(start + batch.len(), paths.len());
                }
            },
            cancel,
        );
        out.extend(batch);
        match result {
            Ok(_) => {}
            Err(CallError::Failed(e)) if e.code == MESH_WORKER_CRASHED => {
                if let Some(path) = paths.get(out.len()) {
                    out.push(ScanItem {
                        path: path.clone(),
                        info: None,
                        error: Some(e.with("path", path.clone())),
                    });
                    on_progress(out.len(), paths.len());
                }
            }
            Err(e) => return Err(e),
        }
        if out.len() == start {
            // No progress at all (should not happen); avoid looping forever.
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use texopt_core::mesh::fixtures;

    use super::*;

    #[test]
    fn request_json_shapes() {
        let env = WorkerEnvelope {
            protocol: 1,
            request: WorkerRequest::Pack {
                models: vec!["a.obj".into()],
                options: PackOptions::default(),
                output_dir: "out".into(),
                base_name: "atlas".into(),
            },
        };
        let json: Value = serde_json::from_str(&encode(&env)).unwrap();
        assert_eq!(json["protocol"], 1);
        assert_eq!(json["request"]["kind"], "pack");
        assert_eq!(json["request"]["outputDir"], "out");
        assert_eq!(json["request"]["baseName"], "atlas");
        assert_eq!(json["request"]["options"]["padding"], 4);
        assert_eq!(decode_envelope(&encode(&env)).unwrap(), env);

        let scan = r#"{"protocol":1,"request":{"kind":"scan","paths":["x.fbx"]}}"#;
        assert_eq!(
            decode_envelope(scan).unwrap().request,
            WorkerRequest::Scan {
                paths: vec!["x.fbx".into()]
            }
        );
        let preview = r#"{"protocol":1,"request":{"kind":"preview","models":[],"options":{}}}"#;
        match decode_envelope(preview).unwrap().request {
            WorkerRequest::Preview {
                preview_max_size, ..
            } => assert_eq!(preview_max_size, DEFAULT_PREVIEW_MAX),
            other => panic!("{other:?}"),
        }
        let err = decode_envelope(r#"{"protocol":9,"request":{"kind":"scan","paths":[]}}"#).unwrap_err();
        assert_eq!(err.params["reason"], "protocolVersion");
        assert!(decode_envelope("garbage").is_err());
    }

    #[test]
    fn message_json_shapes_round_trip() {
        let msgs = [
            WorkerMessage::Progress {
                done: 1,
                total: 3,
                current: Some("a.obj".into()),
            },
            WorkerMessage::Item {
                index: 0,
                item: ScanItem {
                    path: "a.obj".into(),
                    info: None,
                    error: Some(OpError::new("MESH_IMPORT_FAILED").with("path", "a.obj")),
                },
            },
            WorkerMessage::Result {
                value: serde_json::json!({ "ok": true }),
            },
            WorkerMessage::Error {
                error: OpError::new("X"),
            },
        ];
        let lines: Vec<String> = msgs.iter().map(encode).collect();
        assert!(lines.iter().all(|l| !l.contains('\n')));
        assert_eq!(
            serde_json::from_str::<Value>(&lines[0]).unwrap(),
            serde_json::json!({ "type": "progress", "done": 1, "total": 3, "current": "a.obj" })
        );
        assert_eq!(serde_json::from_str::<Value>(&lines[1]).unwrap()["type"], "item");
        for (l, m) in lines.iter().zip(&msgs) {
            assert_eq!(&decode_message(l).unwrap(), m);
        }
        assert_eq!(decode_message("Assimp: some log line"), None);
        assert_eq!(decode_message(r#"{"type":"nope"}"#), None);
    }

    #[test]
    fn collect_stops_at_terminal_and_skips_noise() {
        let lines = vec![
            "noise".to_string(),
            encode(&WorkerMessage::Progress {
                done: 1,
                total: 2,
                current: None,
            }),
            encode(&WorkerMessage::Result {
                value: Value::from(7),
            }),
            encode(&WorkerMessage::Progress {
                done: 2,
                total: 2,
                current: None,
            }),
        ];
        let mut seen = 0;
        let c = collect(lines, &mut |_| seen += 1);
        assert_eq!(seen, 2);
        assert_eq!(finish(c, Some(0), "").unwrap(), Value::from(7));
    }

    #[test]
    fn missing_terminal_is_a_crash() {
        let lines = vec![encode(&WorkerMessage::Progress {
            done: 1,
            total: 2,
            current: None,
        })];
        let c = collect(lines, &mut |_| {});
        let e = finish(c, Some(-1073740940), "heap corruption\n").unwrap_err();
        assert_eq!(e.code, MESH_WORKER_CRASHED);
        assert_eq!(e.params["exitCode"], -1073740940);
        assert_eq!(e.params["detail"], "heap corruption");
        let e = finish(Collected::default(), None, "").unwrap_err();
        assert_eq!(e.params["exitCode"], Value::Null);
        // An error message is passed through, whatever the exit code.
        let c = collect(
            vec![encode(&WorkerMessage::Error {
                error: OpError::new("MESH_NO_MODELS"),
            })],
            &mut |_| {},
        );
        assert_eq!(finish(c, Some(0), "").unwrap_err().code, "MESH_NO_MODELS");
    }

    #[test]
    fn serve_rejects_bad_requests() {
        let mut out = Vec::new();
        let code = serve(&b"{\"protocol\":1}\n"[..], &mut out);
        assert_eq!(code, 2);
        let msg = decode_message(std::str::from_utf8(&out).unwrap()).unwrap();
        assert!(matches!(msg, WorkerMessage::Error { .. }));
    }

    #[test]
    fn serve_scan_in_process() {
        let dir = tempfile::tempdir().unwrap();
        let obj = fixtures::two_quads_obj(dir.path());
        let req = WorkerEnvelope {
            protocol: 1,
            request: WorkerRequest::Scan {
                paths: vec![
                    obj.display().to_string(),
                    dir.path().join("missing.obj").display().to_string(),
                ],
            },
        };
        let mut out = Vec::new();
        let code = serve(format!("{}\n", encode(&req)).as_bytes(), &mut out);
        assert_eq!(code, 0);
        let text = String::from_utf8(out).unwrap();
        let mut items = Vec::new();
        let c = collect(text.lines().map(String::from), &mut |m| {
            if let WorkerMessage::Item { item, .. } = m {
                items.push(item.clone());
            }
        });
        assert_eq!(finish(c, Some(0), "").unwrap(), Value::Null);
        assert_eq!(items.len(), 2);
        let info = items[0].info.as_ref().unwrap();
        assert_eq!(info.materials.len(), 2);
        assert_eq!(info.mesh_count, 2);
        assert_eq!(items[1].error.as_ref().unwrap().code, "MESH_IMPORT_FAILED");
    }

    #[test]
    fn handle_preview_and_pack_in_process() {
        let dir = tempfile::tempdir().unwrap();
        let obj = fixtures::two_quads_obj(&dir.path().join("src"));
        let models = vec![obj.display().to_string()];
        let mut msgs = Vec::new();
        handle(
            WorkerRequest::Preview {
                models: models.clone(),
                options: PackOptions::default(),
                preview_max_size: 16,
            },
            &mut |m| msgs.push(m),
        );
        let Some(WorkerMessage::Result { value }) = msgs.pop() else {
            panic!("no result: {msgs:?}");
        };
        assert!(msgs.iter().all(|m| matches!(m, WorkerMessage::Progress { .. })));
        let payload: PreviewPayload = serde_json::from_value(value).unwrap();
        assert_eq!(payload.images.len(), 1);
        let png = base64::engine::general_purpose::STANDARD
            .decode(&payload.images[0].png)
            .unwrap();
        let img = image::load_from_memory(&png).unwrap();
        assert!(img.width() <= 16 && img.height() <= 16);
        assert!(payload.images[0].width.is_power_of_two());

        let out = dir.path().join("out");
        let mut msgs = Vec::new();
        handle(
            WorkerRequest::Pack {
                models,
                options: PackOptions::default(),
                output_dir: out.display().to_string(),
                base_name: "atlas".into(),
            },
            &mut |m| msgs.push(m),
        );
        let Some(WorkerMessage::Result { value }) = msgs.pop() else {
            panic!("no result: {msgs:?}");
        };
        let report: pack::PackReport = serde_json::from_value(value).unwrap();
        assert_eq!(report.count(pack::ModelOutcome::Rewritten), 1);
        assert!(out.join("atlas_baseColor.png").is_file());
        assert!(out.join("atlas.report.json").is_file());

        let mut msgs = Vec::new();
        handle(
            WorkerRequest::Pack {
                models: vec![],
                options: PackOptions::default(),
                output_dir: out.display().to_string(),
                base_name: "atlas".into(),
            },
            &mut |m| msgs.push(m),
        );
        assert!(matches!(msgs.last(), Some(WorkerMessage::Error { error }) if error.code == "MESH_NO_MODELS"));
    }
}
