//! Spawns the real app executable in `--mesh-worker` mode.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use texopt_core::mesh::fixtures;
use texopt_core::mesh::pack::{ModelOutcome, PackOptions, PackReport};
use texture_optimizer_lib::commands_mesh::scan_with;
use texture_optimizer_lib::mesh_worker::{
    CRASH_ENV, CallError, MESH_WORKER_CRASHED, WorkerCommand, WorkerMessage, WorkerRequest, call,
    scan_models,
};

fn worker() -> WorkerCommand {
    WorkerCommand::new(env!("CARGO_BIN_EXE_texture-optimizer"))
}

fn crashing(on: &str) -> WorkerCommand {
    let mut w = worker();
    w.env.push((CRASH_ENV.into(), on.into()));
    w
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("mesh_worker")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn pack_through_the_worker_process_streams_progress() {
    let dir = scratch("pack");
    let obj = fixtures::two_quads_obj(&dir.join("src"));
    let out = dir.join("out");
    let request = WorkerRequest::Pack {
        models: vec![obj.display().to_string()],
        options: PackOptions::default(),
        output_dir: out.display().to_string(),
        base_name: "atlas".into(),
    };
    let mut progress = Vec::new();
    let value = call(
        &worker(),
        &request,
        &mut |m| {
            if let WorkerMessage::Progress { done, total, .. } = m {
                progress.push((*done, *total));
            }
        },
        None,
    )
    .unwrap();
    let report: PackReport = serde_json::from_value(value).unwrap();
    assert_eq!(report.count(ModelOutcome::Rewritten), 1);
    assert!(out.join("atlas_baseColor.png").is_file());
    assert_eq!(progress.last(), Some(&(3, 3)));
}

#[test]
fn scan_through_the_worker_and_recover_from_a_crash() {
    let dir = scratch("scan");
    let a = fixtures::two_quads_obj(&dir.join("a"));
    let b = fixtures::two_quads_dae(&dir.join("crash_me"));
    let c = fixtures::tiled_quads_obj(&dir.join("c"));
    let paths: Vec<String> = [&a, &b, &c]
        .iter()
        .map(|p| p.display().to_string())
        .collect();

    let items = scan_models(&worker(), &paths, &mut |_, _| {}, None).unwrap();
    assert!(items.iter().all(|i| i.info.is_some()), "{items:?}");

    // The worker aborts on the second model: it is reported as crashed, the
    // third one is scanned by a fresh worker.
    let mut seen = Vec::new();
    let items = scan_models(
        &crashing("crash_me"),
        &paths,
        &mut |d, t| seen.push((d, t)),
        None,
    )
    .unwrap();
    assert_eq!(items.len(), 3);
    assert!(items[0].info.is_some());
    let err = items[1].error.as_ref().unwrap();
    assert_eq!(err.code, MESH_WORKER_CRASHED);
    assert_eq!(err.params["path"], paths[1].as_str());
    assert!(
        items[2].info.is_some(),
        "third model must be scanned by a new worker"
    );
    assert_eq!(seen.last(), Some(&(3, 3)));

    // Folder discovery + scan as the command does it.
    let result = scan_with(&crashing("crash_me"), &[dir.display().to_string()], true).unwrap();
    assert_eq!(result.models.len(), 2);
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].error.code, MESH_WORKER_CRASHED);
    assert!(
        result
            .models
            .iter()
            .all(|m| m.file.width == 0 && !m.file.id.is_empty())
    );
}

#[test]
fn crash_maps_to_mesh_worker_crashed() {
    let request = WorkerRequest::Scan { paths: vec![] };
    let err = call(&crashing("*"), &request, &mut |_| {}, None).unwrap_err();
    match err {
        CallError::Failed(e) => {
            assert_eq!(e.code, MESH_WORKER_CRASHED);
            assert!(!e.params["exitCode"].is_null() || cfg!(unix));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn cancel_kills_the_worker() {
    let cancel = AtomicBool::new(true);
    let request = WorkerRequest::Scan { paths: vec![] };
    let err = call(&worker(), &request, &mut |_| {}, Some(&cancel)).unwrap_err();
    assert_eq!(err, CallError::Cancelled);
}

#[test]
fn spawn_failure_is_reported() {
    let w = WorkerCommand::new("Z:/definitely/not/here.exe");
    let err = call(
        &w,
        &WorkerRequest::Scan { paths: vec![] },
        &mut |_| {},
        None,
    )
    .unwrap_err();
    assert!(matches!(err, CallError::Failed(e) if e.code == "MESH_WORKER_SPAWN_FAILED"));
}
