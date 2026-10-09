//! Incremental export orchestration: merge with the previous atlas on disk,
//! recover kept sprites from page PNGs, overwrite outputs, clean stale files.
mod atlas_support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use atlas_support::{padded, pattern};
use texopt_core::atlas::codes::*;
use texopt_core::atlas::exporters::{
    GenericJsonOptions, GodotOptions, ImageOnlyOptions, UnityOptions,
};
use texopt_core::atlas::workflow::*;
use texopt_core::atlas::{AtlasParams, ExporterConfig, IncrementalMode, build};
use texopt_core::error::codes::{CANCELLED, INVALID_PARAMS};
use texopt_core::{ImageBuf, OpResult};

fn params() -> AtlasParams {
    AtlasParams {
        padding: 1,
        ..AtlasParams::default()
    }
}

fn generic() -> ExporterConfig {
    ExporterConfig::GenericJson(GenericJsonOptions::default())
}

fn unity() -> ExporterConfig {
    ExporterConfig::Unity(UnityOptions::default())
}

fn godot() -> ExporterConfig {
    ExporterConfig::Godot(GodotOptions::default())
}

fn image_only() -> ExporterConfig {
    ExporterConfig::ImageOnly(ImageOnlyOptions::default())
}

fn loader(path: &Path) -> OpResult<Arc<ImageBuf>> {
    texopt_core::io::load_image(path).map(Arc::new)
}

fn src(name: &str, img: &ImageBuf) -> SourceSprite {
    SourceSprite::new(name, Some(format!("C:/art/{name}.png")), img.clone())
}

struct Run {
    params: AtlasParams,
    exporter: ExporterConfig,
    incremental: IncrementalOptions,
}

impl Default for Run {
    fn default() -> Self {
        Self {
            params: params(),
            exporter: generic(),
            incremental: IncrementalOptions::default(),
        }
    }
}

impl Run {
    fn export(&self, dir: &Path, sprites: Vec<SourceSprite>) -> ExportReport {
        self.try_export(dir, sprites).unwrap()
    }

    fn try_export(&self, dir: &Path, sprites: Vec<SourceSprite>) -> OpResult<ExportReport> {
        export_atlas(
            ExportRequest {
                sprites,
                params: &self.params,
                exporter: &self.exporter,
                incremental: self.incremental,
                dir,
                base_name: "atlas",
            },
            &loader,
            &mut |_| {},
            &|| false,
        )
    }
}

fn load_doc(dir: &Path) -> ProjectDocument {
    ProjectDocument::load(&project_path(dir, "atlas"))
        .unwrap()
        .unwrap()
}

/// The untrimmed image of `name` as stored in the exported pages.
fn pixels_in_export(dir: &Path, name: &str) -> ImageBuf {
    let doc = load_doc(dir);
    let s = doc.project.sprite(name).unwrap().clone();
    let n = doc.project.pages.len();
    let page = texopt_core::io::load_image(&dir.join(
        texopt_core::atlas::exporters::page_file_name("atlas", s.page, n),
    ))
    .unwrap();
    recover_sprite(&page, &s, doc.project.params.premultiply_alpha).unwrap()
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = walk(dir)
        .into_iter()
        .map(|p| {
            p.strip_prefix(dir)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    v.sort();
    v
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[test]
fn first_export_writes_pages_metadata_and_project() {
    let dir = tempfile::tempdir().unwrap();
    let a = pattern(20, 12, 1);
    let b = padded(8, 10, 3, 2);
    let report = Run::default().export(dir.path(), vec![src("a", &a), src("b", &b)]);

    assert_eq!(
        files_in(dir.path()),
        ["atlas.json", "atlas.png", "atlas.texatlas.json"]
    );
    let written: Vec<String> = report
        .written
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(written, ["atlas.png", "atlas.json", "atlas.texatlas.json"]);
    assert_eq!(
        report.project_path,
        Some(dir.path().join("atlas.texatlas.json"))
    );
    assert!(report.deleted.is_empty());
    assert_eq!(report.stats.sprite_count, 2);
    assert_eq!(report.stats.pages.len(), 1);
    assert!(report.stats.occupancy > 0.0 && report.stats.occupancy <= 1.0);
    assert!(report.plan.iter().all(|e| e.status == SpriteStatus::New));

    let doc = load_doc(dir.path());
    assert_eq!(doc.project.all_names(), ["a", "b"]);
    assert_eq!(doc.app.generated_files, ["atlas.png", "atlas.json"]);
    assert_eq!(doc.app.exporter.as_deref(), Some("genericJson"));
    assert_eq!(doc.app.sources["a"], "C:/art/a.png");
    // Metadata is valid TexturePacker JSON pointing at the page.
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("atlas.json")).unwrap()).unwrap();
    assert_eq!(meta["meta"]["image"], "atlas.png");
    assert!(meta["frames"]["a"].is_object());
    // Pixels round-trip through the written page.
    assert_eq!(pixels_in_export(dir.path(), "a"), a);
    assert_eq!(pixels_in_export(dir.path(), "b"), b);
}

#[test]
fn second_export_keeps_rects_and_sprites_missing_from_the_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let run = Run::default();
    let a = pattern(20, 12, 1);
    let b = padded(9, 30, 2, 2);
    let c = pattern(14, 14, 3);
    run.export(dir.path(), vec![src("a", &a), src("b", &b)]);
    let before = load_doc(dir.path()).project;

    // b is no longer in the input list (its source may even be gone).
    let report = run.export(dir.path(), vec![src("a", &a), src("c", &c)]);
    let after = load_doc(dir.path());
    assert_eq!(after.project.all_names(), ["a", "b", "c"]);
    for name in ["a", "b"] {
        let (x, y) = (
            before.sprite(name).unwrap(),
            after.project.sprite(name).unwrap(),
        );
        assert_eq!(
            (x.page, x.frame, x.rotated),
            (y.page, y.frame, y.rotated),
            "{name} moved"
        );
    }
    assert_eq!(pixels_in_export(dir.path(), "b"), b);
    assert_eq!(pixels_in_export(dir.path(), "c"), c);
    // The kept sprite remembers where it came from.
    assert_eq!(after.app.sources["b"], "C:/art/b.png");
    let status = |n: &str| report.plan.iter().find(|e| e.name == n).unwrap().status;
    assert_eq!(status("a"), SpriteStatus::Replaced);
    assert_eq!(status("b"), SpriteStatus::Kept);
    assert_eq!(status("c"), SpriteStatus::New);
}

#[test]
fn sync_mode_removes_sprites_not_in_the_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let a = pattern(20, 12, 1);
    let b = pattern(9, 30, 2);
    Run::default().export(dir.path(), vec![src("a", &a), src("b", &b)]);
    let sync = Run {
        incremental: IncrementalOptions {
            mode: IncrementalMode::KeepPositions,
            remove_missing: true,
        },
        ..Run::default()
    };
    let report = sync.export(dir.path(), vec![src("a", &a)]);
    assert_eq!(load_doc(dir.path()).project.all_names(), ["a"]);
    let removed: Vec<_> = report
        .plan
        .iter()
        .filter(|e| e.status == SpriteStatus::Removed)
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(removed, ["b"]);
}

#[test]
fn same_name_input_replaces_the_previous_sprite_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let run = Run::default();
    run.export(
        dir.path(),
        vec![src("a", &pattern(20, 12, 1)), src("b", &pattern(9, 30, 2))],
    );
    let before = load_doc(dir.path()).project;
    let new_a = pattern(20, 12, 77);
    run.export(dir.path(), vec![src("a", &new_a)]);
    let after = load_doc(dir.path()).project;
    assert_eq!(after.all_names(), ["a", "b"]);
    assert_eq!(
        before.sprite("a").unwrap().frame,
        after.sprite("a").unwrap().frame
    );
    assert_ne!(
        before.sprite("a").unwrap().hash,
        after.sprite("a").unwrap().hash
    );
    assert_eq!(pixels_in_export(dir.path(), "a"), new_a);
}

#[test]
fn repack_optimal_still_keeps_missing_sprites() {
    let dir = tempfile::tempdir().unwrap();
    let a = pattern(20, 12, 1);
    let b = pattern(9, 30, 2);
    Run::default().export(dir.path(), vec![src("a", &a), src("b", &b)]);
    let repack = Run {
        incremental: IncrementalOptions {
            mode: IncrementalMode::RepackOptimal,
            remove_missing: false,
        },
        ..Run::default()
    };
    let c = pattern(30, 30, 3);
    repack.export(dir.path(), vec![src("c", &c)]);
    assert_eq!(load_doc(dir.path()).project.all_names(), ["a", "b", "c"]);
    assert_eq!(pixels_in_export(dir.path(), "a"), a);
    assert_eq!(pixels_in_export(dir.path(), "b"), b);
}

fn meta_guid(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap();
    texopt_core::atlas::exporters::unity::parse_meta(&text)
        .guid
        .unwrap()
}

#[test]
fn unity_guid_and_sprite_ids_survive_incremental_exports() {
    let dir = tempfile::tempdir().unwrap();
    let run = Run {
        exporter: unity(),
        ..Run::default()
    };
    run.export(
        dir.path(),
        vec![src("a", &pattern(20, 12, 1)), src("b", &pattern(9, 30, 2))],
    );
    let meta = dir.path().join("atlas.png.meta");
    let guid = meta_guid(&meta);
    let ids =
        texopt_core::atlas::exporters::unity::parse_meta(&std::fs::read_to_string(&meta).unwrap())
            .sprites;

    // Add a sprite (b not in the inputs any more) and export again.
    run.export(
        dir.path(),
        vec![src("a", &pattern(20, 12, 1)), src("c", &pattern(10, 10, 3))],
    );
    assert_eq!(meta_guid(&meta), guid);
    let ids2 =
        texopt_core::atlas::exporters::unity::parse_meta(&std::fs::read_to_string(&meta).unwrap())
            .sprites;
    assert_eq!(ids2["a"], ids["a"]);
    assert_eq!(ids2["b"], ids["b"]);
    assert!(ids2.contains_key("c"));

    // Even if the .meta was deleted, the project's exporter state keeps the guid.
    std::fs::remove_file(&meta).unwrap();
    run.export(dir.path(), vec![src("a", &pattern(20, 12, 1))]);
    assert_eq!(meta_guid(&meta), guid);
}

#[test]
fn shrinking_page_count_removes_only_previously_generated_files() {
    let dir = tempfile::tempdir().unwrap();
    // 40x40 sprites on 64x64 pages: one per page.
    let multi = Run {
        params: AtlasParams {
            max_width: 64,
            max_height: 64,
            multi_page: true,
            padding: 0,
            ..AtlasParams::default()
        },
        exporter: unity(),
        incremental: IncrementalOptions {
            mode: IncrementalMode::KeepPositions,
            remove_missing: true,
        },
    };
    let (a, b) = (pattern(40, 40, 1), pattern(40, 40, 2));
    multi.export(dir.path(), vec![src("a", &a), src("b", &b)]);
    assert_eq!(
        files_in(dir.path()),
        [
            "atlas.texatlas.json",
            "atlas_0.png",
            "atlas_0.png.meta",
            "atlas_1.png",
            "atlas_1.png.meta"
        ]
    );
    // Unknown files that merely look related are never touched.
    std::fs::write(dir.path().join("atlas_2.png"), b"user file").unwrap();
    std::fs::write(dir.path().join("notes.txt"), b"keep").unwrap();

    let report = multi.export(dir.path(), vec![src("a", &a)]);
    assert_eq!(
        files_in(dir.path()),
        [
            "atlas.png",
            "atlas.png.meta",
            "atlas.texatlas.json",
            "atlas_2.png",
            "notes.txt"
        ]
    );
    let mut deleted: Vec<String> = report
        .deleted
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    deleted.sort();
    assert_eq!(
        deleted,
        [
            "atlas_0.png",
            "atlas_0.png.meta",
            "atlas_1.png",
            "atlas_1.png.meta"
        ]
    );
    assert_eq!(pixels_in_export(dir.path(), "a"), a);
}

#[test]
fn godot_tres_of_removed_sprites_are_cleaned_up() {
    let dir = tempfile::tempdir().unwrap();
    let mut run = Run {
        exporter: godot(),
        ..Run::default()
    };
    run.export(
        dir.path(),
        vec![src("a", &pattern(20, 12, 1)), src("b", &pattern(9, 30, 2))],
    );
    assert!(dir.path().join("b.tres").is_file());
    std::fs::write(dir.path().join("custom.tres"), b"user").unwrap();

    run.incremental.remove_missing = true;
    let report = run.export(dir.path(), vec![src("a", &pattern(20, 12, 1))]);
    assert!(!dir.path().join("b.tres").exists());
    assert!(dir.path().join("a.tres").is_file());
    assert!(dir.path().join("custom.tres").is_file());
    assert_eq!(report.deleted, [dir.path().join("b.tres")]);
}

#[test]
fn switching_exporter_keeps_unity_meta_of_a_still_generated_png() {
    let dir = tempfile::tempdir().unwrap();
    let a = pattern(20, 12, 1);
    Run {
        exporter: unity(),
        ..Run::default()
    }
    .export(dir.path(), vec![src("a", &a)]);
    let report = Run::default().export(dir.path(), vec![src("a", &a)]);
    assert!(report.deleted.is_empty());
    assert_eq!(
        files_in(dir.path()),
        [
            "atlas.json",
            "atlas.png",
            "atlas.png.meta",
            "atlas.texatlas.json"
        ]
    );
}

#[test]
fn stale_file_selection_rules() {
    let dir = tempfile::tempdir().unwrap();
    Run::default().export(dir.path(), vec![src("a", &pattern(4, 4, 1))]);
    let mut doc = load_doc(dir.path());
    doc.app.generated_files = vec![
        "atlas_0.png".into(),
        "atlas_0.png.meta".into(),
        "Atlas.PNG".into(),
        "../outside.png".into(),
        "atlas.texatlas.json".into(),
        "sub/x.tres".into(),
    ];
    let stale = stale_files(
        Some(&doc),
        "atlas",
        &["atlas.png".into(), "atlas.json".into()],
    );
    assert_eq!(stale, ["atlas_0.png", "atlas_0.png.meta", "sub/x.tres"]);
    // Older project files without the record: only page PNGs are candidates.
    doc.app.generated_files.clear();
    doc.project.pages.push(doc.project.pages[0]);
    assert_eq!(
        stale_files(Some(&doc), "atlas", &["atlas.png".into()]),
        ["atlas_0.png", "atlas_1.png"]
    );
    assert!(stale_files(None, "atlas", &[]).is_empty());
}

#[test]
fn recovered_pixels_equal_originals_for_rotated_and_trimmed_sprites() {
    // A 64x16 page forces the tall sprite to rotate; margins make it trimmed.
    let params = AtlasParams {
        max_width: 64,
        max_height: 16,
        allow_rotation: true,
        padding: 0,
        ..AtlasParams::default()
    };
    let tall = padded(10, 40, 3, 5);
    let small = padded(6, 4, 2, 6);
    let res = build(
        vec![
            texopt_core::atlas::SpriteInput::new("tall", tall.clone()),
            texopt_core::atlas::SpriteInput::new("small", small.clone()),
        ],
        &params,
        None,
        IncrementalMode::KeepPositions,
    )
    .unwrap();
    let t = res.project.sprite("tall").unwrap();
    assert!(t.rotated && t.trimmed);
    assert_eq!(recover_sprite(&res.pages[t.page], t, false).unwrap(), tall);
    let s = res.project.sprite("small").unwrap();
    assert!(s.trimmed);
    assert_eq!(recover_sprite(&res.pages[s.page], s, false).unwrap(), small);

    // Premultiplied pages: opaque pixels come back exactly.
    let pm = AtlasParams {
        premultiply_alpha: true,
        ..params
    };
    let res = build(
        vec![texopt_core::atlas::SpriteInput::new("tall", tall.clone())],
        &pm,
        None,
        IncrementalMode::KeepPositions,
    )
    .unwrap();
    let t = res.project.sprite("tall").unwrap();
    assert_eq!(recover_sprite(&res.pages[t.page], t, true).unwrap(), tall);

    // A frame outside the page is reported, not panicking.
    let mut bad = t.clone();
    bad.frame.x = 60;
    assert_eq!(
        recover_sprite(&res.pages[0], &bad, false).unwrap_err().code,
        ATLAS_PROJECT_INVALID
    );
}

#[test]
fn rotated_trimmed_sprite_survives_an_incremental_export() {
    let dir = tempfile::tempdir().unwrap();
    let run = Run {
        params: AtlasParams {
            max_width: 64,
            max_height: 16,
            allow_rotation: true,
            padding: 0,
            ..AtlasParams::default()
        },
        ..Run::default()
    };
    let tall = padded(10, 40, 3, 5);
    run.export(dir.path(), vec![src("tall", &tall)]);
    assert!(load_doc(dir.path()).project.sprite("tall").unwrap().rotated);
    let small = pattern(4, 4, 9);
    run.export(dir.path(), vec![src("small", &small)]);
    assert_eq!(pixels_in_export(dir.path(), "tall"), tall);
    assert_eq!(pixels_in_export(dir.path(), "small"), small);
}

#[test]
fn dedupe_aliases_are_kept_too() {
    let dir = tempfile::tempdir().unwrap();
    let run = Run::default();
    let same = pattern(8, 8, 4);
    run.export(dir.path(), vec![src("x", &same), src("y", &same)]);
    assert_eq!(load_doc(dir.path()).project.sprites.len(), 1);
    run.export(dir.path(), vec![src("z", &pattern(5, 5, 1))]);
    let doc = load_doc(dir.path());
    assert_eq!(doc.project.all_names(), ["x", "y", "z"]);
    assert_eq!(pixels_in_export(dir.path(), "y"), same);
}

#[test]
fn missing_page_drops_kept_sprites_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let run = Run::default();
    run.export(
        dir.path(),
        vec![src("a", &pattern(20, 12, 1)), src("b", &pattern(9, 30, 2))],
    );
    std::fs::remove_file(dir.path().join("atlas.png")).unwrap();
    let report = run.export(dir.path(), vec![src("c", &pattern(5, 5, 3))]);
    assert_eq!(load_doc(dir.path()).project.all_names(), ["c"]);
    let lost: Vec<_> = report
        .warnings
        .iter()
        .filter(|w| w.code == ATLAS_SPRITE_NOT_RECOVERED)
        .map(|w| w.params["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(lost, ["a", "b"]);
    assert!(
        report
            .plan
            .iter()
            .filter(|e| e.name != "c")
            .all(|e| e.status == SpriteStatus::Removed)
    );
}

#[test]
fn unsupported_features_are_disabled_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let run = Run {
        params: AtlasParams {
            allow_rotation: true,
            ..params()
        },
        exporter: unity(),
        ..Run::default()
    };
    let report = run.export(dir.path(), vec![src("a", &pattern(9, 30, 2))]);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.code == ATLAS_FEATURE_DISABLED && w.params["feature"] == "rotation")
    );
    assert!(!load_doc(dir.path()).project.params.allow_rotation);
}

#[test]
fn cancellation_and_invalid_targets_write_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let p = params();
    let e = generic();
    let err = export_atlas(
        ExportRequest {
            sprites: vec![src("a", &pattern(4, 4, 1))],
            params: &p,
            exporter: &e,
            incremental: IncrementalOptions::default(),
            dir: dir.path(),
            base_name: "atlas",
        },
        &loader,
        &mut |_| {},
        &|| true,
    )
    .unwrap_err();
    assert_eq!(err.code, CANCELLED);
    assert!(files_in(dir.path()).is_empty());

    let err = export_atlas(
        ExportRequest {
            sprites: vec![src("a", &pattern(4, 4, 1))],
            params: &p,
            exporter: &e,
            incremental: IncrementalOptions::default(),
            dir: dir.path(),
            base_name: "bad/name",
        },
        &loader,
        &mut |_| {},
        &|| false,
    )
    .unwrap_err();
    assert_eq!(err.code, INVALID_PARAMS);
    assert!(files_in(dir.path()).is_empty());
}

#[test]
fn stages_are_reported_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let p = params();
    let e = generic();
    let mut stages = Vec::new();
    export_atlas(
        ExportRequest {
            sprites: vec![src("a", &pattern(4, 4, 1))],
            params: &p,
            exporter: &e,
            incremental: IncrementalOptions::default(),
            dir: dir.path(),
            base_name: "atlas",
        },
        &loader,
        &mut |s| stages.push(s),
        &|| false,
    )
    .unwrap();
    assert_eq!(
        stages,
        [
            ExportStage::Packing,
            ExportStage::Encoding,
            ExportStage::Writing(dir.path().join("atlas.png")),
            ExportStage::Writing(dir.path().join("atlas.json")),
            ExportStage::Writing(dir.path().join("atlas.texatlas.json")),
            ExportStage::Cleaning,
        ]
    );
}

#[test]
fn project_document_round_trip_and_plain_core_projects() {
    let dir = tempfile::tempdir().unwrap();
    Run::default().export(dir.path(), vec![src("a", &pattern(4, 4, 1))]);
    let doc = load_doc(dir.path());
    let again = ProjectDocument::from_json(&doc.to_json()).unwrap();
    assert_eq!(again, doc);
    // A project written by the plain core API (no appData) still loads.
    let plain = ProjectDocument::from_json(&doc.project.to_json()).unwrap();
    assert_eq!(plain.project, doc.project);
    assert_eq!(plain.app, ProjectAppData::default());
    assert!(
        ProjectDocument::load(&dir.path().join("missing.texatlas.json"))
            .unwrap()
            .is_none()
    );
    std::fs::write(dir.path().join("bad.texatlas.json"), "{").unwrap();
    assert_eq!(
        ProjectDocument::load(&dir.path().join("bad.texatlas.json"))
            .unwrap_err()
            .code,
        ATLAS_PROJECT_INVALID
    );
}

#[test]
fn plan_merge_statuses() {
    let dir = tempfile::tempdir().unwrap();
    Run::default().export(
        dir.path(),
        vec![src("a", &pattern(4, 4, 1)), src("b", &pattern(5, 5, 2))],
    );
    let doc = load_doc(dir.path());
    let inputs = vec![
        ("b".to_string(), Some("D:/b.png".to_string())),
        ("c".to_string(), None),
    ];
    let plan = plan_merge(Some(&doc), &inputs, false);
    let got: Vec<(&str, SpriteStatus, Option<&str>)> = plan
        .iter()
        .map(|e| (e.name.as_str(), e.status, e.source_path.as_deref()))
        .collect();
    assert_eq!(
        got,
        [
            ("a", SpriteStatus::Kept, Some("C:/art/a.png")),
            ("b", SpriteStatus::Replaced, Some("D:/b.png")),
            ("c", SpriteStatus::New, None),
        ]
    );
    assert_eq!(
        plan_merge(Some(&doc), &inputs, true)[0].status,
        SpriteStatus::Removed
    );
    assert!(
        plan_merge(None, &inputs, false)
            .iter()
            .all(|e| e.status == SpriteStatus::New)
    );
}

#[test]
fn stats_measure_occupancy() {
    let res = build(
        vec![texopt_core::atlas::SpriteInput::new(
            "a",
            pattern(16, 16, 1),
        )],
        &AtlasParams {
            padding: 0,
            ..AtlasParams::default()
        },
        None,
        IncrementalMode::KeepPositions,
    )
    .unwrap();
    let st = atlas_stats(&res.project);
    assert_eq!(st.pages[0].width, 16);
    assert_eq!(st.pages[0].used_area, 256);
    assert!((st.occupancy - 1.0).abs() < 1e-9);
    assert_eq!(st.frame_count, 1);
}

// ------------------------------------------------------------ Image only

/// Run with 40x40 sprites on 64x64 pages: one sprite per page.
fn one_per_page(exporter: ExporterConfig) -> Run {
    Run {
        params: AtlasParams {
            max_width: 64,
            max_height: 64,
            multi_page: true,
            padding: 0,
            ..AtlasParams::default()
        },
        exporter,
        incremental: IncrementalOptions::default(),
    }
}

fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    files_in(dir)
        .into_iter()
        .map(|f| {
            let bytes = std::fs::read(dir.join(&f)).unwrap();
            (f, bytes)
        })
        .collect()
}

#[test]
fn image_only_writes_only_the_page_png() {
    let dir = tempfile::tempdir().unwrap();
    let a = pattern(20, 12, 1);
    let b = padded(8, 10, 3, 2);
    let mut stages = Vec::new();
    let report = export_atlas(
        ExportRequest {
            sprites: vec![src("a", &a), src("b", &b)],
            params: &params(),
            exporter: &image_only(),
            incremental: IncrementalOptions::default(),
            dir: dir.path(),
            base_name: "atlas",
        },
        &loader,
        &mut |s| stages.push(s),
        &|| false,
    )
    .unwrap();
    assert_eq!(files_in(dir.path()), ["atlas.png"]);
    assert_eq!(report.written, [dir.path().join("atlas.png")]);
    assert_eq!(report.project_path, None);
    assert!(report.deleted.is_empty());
    assert_eq!(report.stats.sprite_count, 2);
    assert!(report.plan.iter().all(|e| e.status == SpriteStatus::New));
    assert!(
        ProjectDocument::load(&project_path(dir.path(), "atlas"))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        stages,
        [
            ExportStage::Packing,
            ExportStage::Encoding,
            ExportStage::Writing(dir.path().join("atlas.png")),
            ExportStage::Cleaning,
        ]
    );
    let page = texopt_core::io::load_image(&dir.path().join("atlas.png")).unwrap();
    assert!(page.width().is_power_of_two() && page.height().is_power_of_two());
}

#[test]
fn image_only_multi_page_writes_numbered_pngs_only() {
    let dir = tempfile::tempdir().unwrap();
    let report = one_per_page(image_only()).export(
        dir.path(),
        vec![src("a", &pattern(40, 40, 1)), src("b", &pattern(40, 40, 2))],
    );
    assert_eq!(files_in(dir.path()), ["atlas_0.png", "atlas_1.png"]);
    assert_eq!(report.written.len(), 2);
    assert_eq!(report.project_path, None);
    for f in ["atlas_0.png", "atlas_1.png"] {
        let page = texopt_core::io::load_image(&dir.path().join(f)).unwrap();
        assert_eq!(page.dimensions(), (64, 64));
    }
}

#[test]
fn image_only_disables_rotation_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let run = Run {
        params: AtlasParams {
            allow_rotation: true,
            max_width: 64,
            max_height: 32,
            ..params()
        },
        exporter: image_only(),
        ..Run::default()
    };
    // 10x50 only fits a 64x32 page when rotated: without rotation it must fail.
    let err = run
        .try_export(dir.path(), vec![src("tall", &pattern(10, 50, 1))])
        .unwrap_err();
    assert_eq!(err.code, ATLAS_DOES_NOT_FIT);
    assert!(files_in(dir.path()).is_empty());

    let report = run.export(dir.path(), vec![src("a", &pattern(9, 30, 2))]);
    let w: Vec<_> = report
        .warnings
        .iter()
        .filter(|w| w.code == ATLAS_FEATURE_DISABLED)
        .collect();
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].params["feature"], "rotation");
    assert_eq!(w[0].params["exporter"], "imageOnly");
    // The page holds the sprite unrotated (9x30 at the top-left corner).
    let page = texopt_core::io::load_image(&dir.path().join("atlas.png")).unwrap();
    assert_eq!(
        image::imageops::crop_imm(&page, 0, 0, 9, 30).to_image(),
        pattern(9, 30, 2)
    );
}

#[test]
fn image_only_ignores_an_existing_atlas_and_never_deletes_its_files() {
    let dir = tempfile::tempdir().unwrap();
    // A two-page Unity atlas with its project file at the same base name.
    one_per_page(unity()).export(
        dir.path(),
        vec![src("a", &pattern(40, 40, 1)), src("b", &pattern(40, 40, 2))],
    );
    let before = snapshot(dir.path());
    assert_eq!(before.len(), 5);

    let c = pattern(12, 12, 3);
    let run = Run {
        exporter: image_only(),
        incremental: IncrementalOptions {
            mode: IncrementalMode::KeepPositions,
            remove_missing: false,
        },
        ..Run::default()
    };
    let report = run.export(dir.path(), vec![src("c", &c)]);

    // Fresh pack: only the current inputs, nothing kept from the old atlas.
    assert_eq!(report.stats.sprite_count, 1);
    let plan: Vec<(&str, SpriteStatus)> = report
        .plan
        .iter()
        .map(|e| (e.name.as_str(), e.status))
        .collect();
    assert_eq!(plan, [("c", SpriteStatus::New)]);
    let page = texopt_core::io::load_image(&dir.path().join("atlas.png")).unwrap();
    assert_eq!(image::imageops::crop_imm(&page, 0, 0, 12, 12).to_image(), c);
    // The old atlas (pages, metas, project) is left exactly as it was.
    assert!(report.deleted.is_empty());
    assert_eq!(report.written, [dir.path().join("atlas.png")]);
    let mut after = snapshot(dir.path());
    after.retain(|(f, _)| f != "atlas.png");
    assert_eq!(after, before);
    // ...and the user is told the project was not used.
    let ignored: Vec<_> = report
        .warnings
        .iter()
        .filter(|w| w.code == ATLAS_PROJECT_IGNORED)
        .collect();
    assert_eq!(ignored.len(), 1);
    assert_eq!(
        ignored[0].params["path"],
        project_path(dir.path(), "atlas").display().to_string()
    );

    // Same at preview time: no previous atlas, every sprite is new.
    let outcome = build_incremental(
        BuildRequest {
            sprites: vec![src("c", &c)],
            params: &params(),
            exporter: &image_only(),
            incremental: IncrementalOptions::default(),
            target: Some(AtlasTarget {
                dir: dir.path(),
                base_name: "atlas",
            }),
        },
        &loader,
    )
    .unwrap();
    assert!(outcome.previous.is_none());
    assert_eq!(outcome.plan.len(), 1);
    assert_eq!(outcome.plan[0].status, SpriteStatus::New);
    assert!(
        outcome
            .warnings
            .iter()
            .any(|w| w.code == ATLAS_PROJECT_IGNORED)
    );
}

#[test]
fn metadata_export_after_image_only_cleans_nothing_it_did_not_write() {
    let dir = tempfile::tempdir().unwrap();
    one_per_page(image_only()).export(
        dir.path(),
        vec![src("a", &pattern(40, 40, 1)), src("b", &pattern(40, 40, 2))],
    );
    // No project was written, so a later metadata export starts fresh and
    // leaves the image-only pages alone.
    let report = Run::default().export(dir.path(), vec![src("a", &pattern(20, 12, 1))]);
    assert!(report.deleted.is_empty());
    assert!(report.plan.iter().all(|e| e.status == SpriteStatus::New));
    assert_eq!(
        files_in(dir.path()),
        [
            "atlas.json",
            "atlas.png",
            "atlas.texatlas.json",
            "atlas_0.png",
            "atlas_1.png"
        ]
    );
}
