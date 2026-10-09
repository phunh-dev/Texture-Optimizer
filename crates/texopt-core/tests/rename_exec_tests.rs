use std::fs;
use std::path::{Path, PathBuf};

use texopt_core::rename::{
    Conflict, ExecuteMode, RENAME_HAS_CONFLICTS, RENAME_SOURCE_MISSING, RENAME_TARGET_EXISTS,
    RenameEntry, RenameLog, RenameParams, RenamePlanItem, execute, plan, revert,
};

fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, content).unwrap();
    p
}

fn entry(path: &Path) -> RenameEntry {
    RenameEntry {
        path: path.to_path_buf(),
        width: 1,
        height: 1,
        modified_ms: 0,
        size_bytes: 1,
    }
}

/// Sorted `(file name, content)` of every file in `dir`.
fn snapshot(dir: &Path) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file())
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read_to_string(&p).unwrap(),
            )
        })
        .collect();
    v.sort();
    v
}

fn item(from: &Path, to: &Path) -> RenamePlanItem {
    RenamePlanItem {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        conflict: None,
    }
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

#[test]
fn in_place_execute_then_revert_restores_exact_names() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "Rock.png", "A");
    let b = write(dir.path(), "grass 01.png", "B");
    let c = write(dir.path(), "keep.png", "C");
    let before = snapshot(dir.path());

    let params = RenameParams {
        template: "{name}_x".into(),
        ..RenameParams::default()
    };
    let mut items = plan(&[entry(&a), entry(&b)], &params, &|p: &Path| p.exists()).unwrap();
    items.push(item(&c, &c)); // no-op entries are skipped
    let log = execute(&items, &ExecuteMode::InPlace).unwrap();

    assert_eq!(
        snapshot(dir.path()),
        pairs(&[
            ("Rock_x.png", "A"),
            ("grass 01_x.png", "B"),
            ("keep.png", "C")
        ])
    );
    assert_eq!(log.entries.len(), 2);
    assert_eq!(log.mode, ExecuteMode::InPlace);
    assert!(log.timestamp > 0);

    // The log survives a JSON round-trip (it is persisted for "Revert last rename").
    let json = serde_json::to_string(&log).unwrap();
    assert!(
        json.contains("\"entries\"")
            && json.contains("\"inPlace\"")
            && json.contains("\"timestamp\"")
    );
    let restored: RenameLog = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, log);

    revert(&restored).unwrap();
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn swap_and_three_cycle_work_and_revert() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "a.png", "A");
    let b = write(dir.path(), "b.png", "B");
    let c = write(dir.path(), "c.png", "C");
    let before = snapshot(dir.path());

    let log = execute(&[item(&a, &b), item(&b, &a)], &ExecuteMode::InPlace).unwrap();
    assert_eq!(
        snapshot(dir.path()),
        pairs(&[("a.png", "B"), ("b.png", "A"), ("c.png", "C")])
    );
    revert(&log).unwrap();
    assert_eq!(snapshot(dir.path()), before);

    let log = execute(
        &[item(&a, &b), item(&b, &c), item(&c, &a)],
        &ExecuteMode::InPlace,
    )
    .unwrap();
    assert_eq!(
        snapshot(dir.path()),
        pairs(&[("a.png", "C"), ("b.png", "A"), ("c.png", "B")])
    );
    revert(&log).unwrap();
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn case_only_rename_works() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "rock.png", "A");
    let log = execute(
        &[item(&a, &dir.path().join("ROCK.png"))],
        &ExecuteMode::InPlace,
    )
    .unwrap();
    assert_eq!(snapshot(dir.path()), pairs(&[("ROCK.png", "A")]));
    revert(&log).unwrap();
    assert_eq!(snapshot(dir.path()), pairs(&[("rock.png", "A")]));
}

#[test]
fn copy_to_then_revert_deletes_copies() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let out = dir.path().join("out");
    fs::create_dir(&src).unwrap();
    let a = write(&src, "a.png", "A");
    let b = write(&src, "b.png", "B");

    let params = RenameParams {
        template: "tex_{index}".into(),
        zero_pad: 2,
        ..RenameParams::default()
    };
    let exists_in_out = |p: &Path| out.join(p.file_name().unwrap()).exists();
    let items = plan(&[entry(&a), entry(&b)], &params, &exists_in_out).unwrap();
    let log = execute(&items, &ExecuteMode::CopyTo { dir: out.clone() }).unwrap();

    assert_eq!(snapshot(&src), pairs(&[("a.png", "A"), ("b.png", "B")]));
    assert_eq!(
        snapshot(&out),
        pairs(&[("tex_01.png", "A"), ("tex_02.png", "B")])
    );
    assert_eq!(log.entries[0].to, out.join("tex_01.png"));
    let json = serde_json::to_value(&log).unwrap();
    assert_eq!(json["mode"]["kind"], "copyTo");

    revert(&log).unwrap();
    assert_eq!(snapshot(&out), vec![]);
    assert_eq!(snapshot(&src), pairs(&[("a.png", "A"), ("b.png", "B")]));
}

#[test]
fn execute_refuses_plans_with_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "a.png", "A");
    let b = write(dir.path(), "b.png", "B");
    let params = RenameParams {
        template: "same".into(),
        ..RenameParams::default()
    };
    let items = plan(&[entry(&a), entry(&b)], &params, &|p: &Path| p.exists()).unwrap();
    assert!(
        items
            .iter()
            .all(|i| i.conflict == Some(Conflict::DuplicateInBatch))
    );
    for mode in [
        ExecuteMode::InPlace,
        ExecuteMode::CopyTo {
            dir: dir.path().join("out"),
        },
    ] {
        let err = execute(&items, &mode).unwrap_err();
        assert_eq!(err.code, RENAME_HAS_CONFLICTS);
        assert_eq!(err.params["count"], 2);
    }
    assert_eq!(
        snapshot(dir.path()),
        pairs(&[("a.png", "A"), ("b.png", "B")])
    );
    assert!(!dir.path().join("out").exists());
}

#[test]
fn target_appearing_on_disk_rolls_back_everything() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "a.png", "A");
    let b = write(dir.path(), "b.png", "B");
    write(dir.path(), "y.png", "foreign");
    let before = snapshot(dir.path());

    // Plan computed without disk knowledge, so it has no conflict flags.
    let items = vec![
        item(&a, &dir.path().join("x.png")),
        item(&b, &dir.path().join("y.png")),
    ];
    let err = execute(&items, &ExecuteMode::InPlace).unwrap_err();
    assert_eq!(err.code, RENAME_TARGET_EXISTS);
    assert_eq!(
        snapshot(dir.path()),
        before,
        "no partial rename and no temp files left"
    );

    // Copy mode: second copy collides with an existing file -> first copy removed.
    let out = dir.path().join("out");
    fs::create_dir(&out).unwrap();
    write(&out, "y.png", "foreign");
    let items = vec![
        item(&a, &dir.path().join("x.png")),
        item(&b, &dir.path().join("y.png")),
    ];
    let err = execute(&items, &ExecuteMode::CopyTo { dir: out.clone() }).unwrap_err();
    assert_eq!(err.code, RENAME_TARGET_EXISTS);
    assert_eq!(snapshot(&out), pairs(&[("y.png", "foreign")]));
}

#[test]
fn missing_source_is_rejected_before_touching_anything() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "a.png", "A");
    let items = vec![
        item(&a, &dir.path().join("x.png")),
        item(&dir.path().join("ghost.png"), &dir.path().join("y.png")),
    ];
    let err = execute(&items, &ExecuteMode::InPlace).unwrap_err();
    assert_eq!(err.code, RENAME_SOURCE_MISSING);
    assert_eq!(snapshot(dir.path()), pairs(&[("a.png", "A")]));
}

#[test]
fn revert_refuses_when_renamed_file_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let a = write(dir.path(), "a.png", "A");
    let b = write(dir.path(), "b.png", "B");
    let log = execute(
        &[
            item(&a, &dir.path().join("a2.png")),
            item(&b, &dir.path().join("b2.png")),
        ],
        &ExecuteMode::InPlace,
    )
    .unwrap();
    fs::remove_file(dir.path().join("b2.png")).unwrap();
    let err = revert(&log).unwrap_err();
    assert_eq!(err.code, RENAME_SOURCE_MISSING);
    assert_eq!(
        snapshot(dir.path()),
        pairs(&[("a2.png", "A")]),
        "revert is all-or-nothing"
    );
}
