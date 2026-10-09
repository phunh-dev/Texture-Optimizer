mod atlas_support;

use atlas_support::*;
use serde_json::json;
use texopt_core::ImageBuf;
use texopt_core::atlas::codes::*;
use texopt_core::atlas::{
    AtlasParams, AtlasProject, AtlasResult, IncrementalMode, PROJECT_VERSION, build,
};

fn base() -> AtlasParams {
    AtlasParams {
        padding: 0,
        trim: false,
        dedupe: false,
        ..AtlasParams::default()
    }
}

fn named(v: &[(&str, ImageBuf)]) -> Vec<(String, ImageBuf)> {
    v.iter().map(|(n, i)| (n.to_string(), i.clone())).collect()
}

fn build_with(
    inputs: &[(String, ImageBuf)],
    params: &AtlasParams,
    prev: Option<&AtlasProject>,
    mode: IncrementalMode,
) -> AtlasResult {
    let r = build(to_inputs(inputs), params, prev, mode).unwrap();
    check_invariants(&r, inputs, params);
    r
}

fn keep(inputs: &[(String, ImageBuf)], params: &AtlasParams, prev: &AtlasProject) -> AtlasResult {
    build_with(inputs, params, Some(prev), IncrementalMode::KeepPositions)
}

fn assert_same_rect(a: &AtlasProject, b: &AtlasProject, name: &str) {
    let sa = a.sprite(name).unwrap();
    let sb = b.sprite(name).unwrap();
    assert_eq!(
        (sa.page, sa.frame, sa.rotated),
        (sb.page, sb.frame, sb.rotated),
        "{name} moved"
    );
}

fn initial() -> Vec<(String, ImageBuf)> {
    named(&[
        ("a", pattern(20, 12, 1)),
        ("b", pattern(9, 30, 2)),
        ("c", pattern(14, 14, 3)),
    ])
}

#[test]
fn adding_sprites_keeps_old_rects() {
    let params = AtlasParams {
        padding: 2,
        extrude: 1,
        border: 1,
        ..base()
    };
    let v1 = build_with(&initial(), &params, None, IncrementalMode::KeepPositions);
    let mut more = initial();
    more.push(("d".into(), pattern(7, 7, 4)));
    more.push(("e".into(), pattern(16, 5, 5)));
    let v2 = keep(&more, &params, &v1.project);
    for n in ["a", "b", "c"] {
        assert_same_rect(&v1.project, &v2.project, n);
    }
    assert!(v2.warnings.is_empty(), "{:?}", v2.warnings);
}

#[test]
fn replacing_same_size_keeps_rect_and_updates_pixels() {
    let params = base();
    let v1 = build_with(&initial(), &params, None, IncrementalMode::KeepPositions);
    let mut changed = initial();
    changed[0].1 = pattern(20, 12, 99);
    let v2 = keep(&changed, &params, &v1.project);
    for n in ["a", "b", "c"] {
        assert_same_rect(&v1.project, &v2.project, n);
    }
    let s = v2.project.sprite("a").unwrap();
    assert_ne!(s.hash, v1.project.sprite("a").unwrap().hash);
    assert_eq!(
        v2.pages[0].get_pixel(s.frame.x + 3, s.frame.y + 4),
        changed[0].1.get_pixel(3, 4)
    );
}

#[test]
fn replacing_with_bigger_size_moves_only_that_sprite() {
    let params = base();
    let v1 = build_with(&initial(), &params, None, IncrementalMode::KeepPositions);
    let mut changed = initial();
    changed[2].1 = pattern(18, 18, 7);
    let v2 = keep(&changed, &params, &v1.project);
    assert_same_rect(&v1.project, &v2.project, "a");
    assert_same_rect(&v1.project, &v2.project, "b");
    let c = v2.project.sprite("c").unwrap();
    assert_eq!((c.frame.w, c.frame.h), (18, 18));
    assert_ne!(c.frame, v1.project.sprite("c").unwrap().frame);
}

#[test]
fn removing_a_sprite_keeps_the_others() {
    let params = base();
    let v1 = build_with(&initial(), &params, None, IncrementalMode::KeepPositions);
    let fewer: Vec<_> = initial().into_iter().filter(|(n, _)| n != "b").collect();
    let v2 = keep(&fewer, &params, &v1.project);
    assert!(v2.project.sprite("b").is_none());
    assert_same_rect(&v1.project, &v2.project, "a");
    assert_same_rect(&v1.project, &v2.project, "c");
    assert_eq!(v2.project.pages, v1.project.pages);
    // b's pixels are gone
    let b = v1.project.sprite("b").unwrap();
    assert_eq!(v2.pages[0].get_pixel(b.frame.x, b.frame.y)[3], 0);
}

fn quads(n: usize) -> Vec<(String, ImageBuf)> {
    (0..n)
        .map(|i| (format!("q{i}"), pattern(32, 32, i as u8)))
        .collect()
}

#[test]
fn freed_space_is_reused_before_growing() {
    let params = base();
    let v1 = build_with(&quads(4), &params, None, IncrementalMode::KeepPositions);
    assert_eq!(
        (v1.project.pages[0].width, v1.project.pages[0].height),
        (64, 64)
    );
    let mut next: Vec<_> = quads(4).into_iter().filter(|(n, _)| n != "q1").collect();
    next.push(("new".into(), pattern(32, 32, 50)));
    let v2 = keep(&next, &params, &v1.project);
    assert_eq!(v2.project.pages, v1.project.pages);
    assert_eq!(
        v2.project.sprite("new").unwrap().frame,
        v1.project.sprite("q1").unwrap().frame
    );
    for n in ["q0", "q2", "q3"] {
        assert_same_rect(&v1.project, &v2.project, n);
    }
}

#[test]
fn page_grows_in_pot_steps_when_full() {
    let params = base();
    let v1 = build_with(&quads(4), &params, None, IncrementalMode::KeepPositions);
    let v2 = keep(&quads(5), &params, &v1.project);
    assert_eq!(v2.project.pages.len(), 1);
    assert_eq!(
        (v2.project.pages[0].width, v2.project.pages[0].height),
        (128, 64)
    );
    for i in 0..4 {
        assert_same_rect(&v1.project, &v2.project, &format!("q{i}"));
    }
}

#[test]
fn new_page_when_max_reached_and_error_without_multipage() {
    let params = AtlasParams {
        max_width: 64,
        max_height: 64,
        multi_page: true,
        ..base()
    };
    let v1 = build_with(&quads(4), &params, None, IncrementalMode::KeepPositions);
    let v2 = keep(&quads(5), &params, &v1.project);
    assert_eq!(v2.project.pages.len(), 2);
    assert_eq!(v2.project.sprite("q4").unwrap().page, 1);
    for i in 0..4 {
        assert_same_rect(&v1.project, &v2.project, &format!("q{i}"));
    }

    let single = AtlasParams {
        multi_page: false,
        ..params
    };
    let err = build(
        to_inputs(&quads(5)),
        &single,
        Some(&v1.project),
        IncrementalMode::KeepPositions,
    )
    .unwrap_err();
    assert_eq!(err.code, ATLAS_DOES_NOT_FIT);
    assert_eq!(err.params["name"], "q4");
}

#[test]
fn repack_optimal_ignores_positions_but_keeps_exporter_state() {
    let params = base();
    let mut v1 = build_with(&initial(), &params, None, IncrementalMode::KeepPositions);
    v1.project.exporter_state =
        json!({"unity": {"pages": {"atlas.png": "0123456789abcdef0123456789abcdef"}}});
    let mut changed = initial();
    changed[2].1 = pattern(18, 18, 7);
    changed.push(("d".into(), pattern(25, 25, 4)));
    let repacked = build_with(
        &changed,
        &params,
        Some(&v1.project),
        IncrementalMode::RepackOptimal,
    );
    let fresh = build_with(&changed, &params, None, IncrementalMode::KeepPositions);
    assert_eq!(repacked.project.sprites, fresh.project.sprites);
    assert_eq!(repacked.project.pages, fresh.project.pages);
    assert_eq!(repacked.project.exporter_state, v1.project.exporter_state);

    // keepPositions carries the state too.
    let kept = keep(&changed, &params, &v1.project);
    assert_eq!(kept.project.exporter_state, v1.project.exporter_state);
}

#[test]
fn spacing_change_falls_back_to_full_repack_with_warning() {
    let v1 = build_with(&initial(), &base(), None, IncrementalMode::KeepPositions);
    let params = AtlasParams {
        padding: 4,
        ..base()
    };
    let v2 = keep(&initial(), &params, &v1.project);
    assert_eq!(v2.warnings.len(), 1);
    assert_eq!(v2.warnings[0].code, ATLAS_LAYOUT_RESET);
    assert_eq!(v2.warnings[0].params["reason"], "spacingChanged");
}

#[test]
fn incremental_with_dedupe_and_trim() {
    let params = AtlasParams {
        trim: true,
        dedupe: true,
        padding: 1,
        ..AtlasParams::default()
    };
    let shared = padded(10, 6, 2, 1);
    let v1_in = named(&[
        ("x", shared.clone()),
        ("y", shared.clone()),
        ("z", padded(8, 8, 1, 2)),
    ]);
    let v1 = build_with(&v1_in, &params, None, IncrementalMode::KeepPositions);
    assert_eq!(v1.project.sprite("y").unwrap().name, "x");
    // y now differs from x: x keeps the rect, y gets its own.
    let v2_in = named(&[
        ("x", shared.clone()),
        ("y", padded(10, 6, 2, 9)),
        ("z", padded(8, 8, 1, 2)),
    ]);
    let v2 = keep(&v2_in, &params, &v1.project);
    assert_same_rect(&v1.project, &v2.project, "x");
    assert_same_rect(&v1.project, &v2.project, "z");
    assert_ne!(
        v2.project.sprite("y").unwrap().frame,
        v2.project.sprite("x").unwrap().frame
    );
}

#[test]
fn project_json_round_trip() {
    let params = AtlasParams {
        dedupe: true,
        trim: true,
        allow_rotation: true,
        ..AtlasParams::default()
    };
    let inputs = named(&[
        ("a", padded(10, 30, 2, 1)),
        ("b", padded(10, 30, 2, 1)),
        ("c", pattern(5, 5, 3)),
    ]);
    let mut r = build_with(&inputs, &params, None, IncrementalMode::KeepPositions);
    r.project.exporter_state =
        json!({"unity": {"sprites": {"a": {"internalID": 5, "spriteID": "x"}}}});
    let text = r.project.to_json();
    let back = AtlasProject::from_json(&text).unwrap();
    assert_eq!(back, r.project);

    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["version"], PROJECT_VERSION);
    let s = &v["sprites"][0];
    for key in [
        "name",
        "hash",
        "page",
        "frame",
        "rotated",
        "trimmed",
        "sourceSize",
        "spriteSourceSize",
        "aliases",
    ] {
        assert!(s.get(key).is_some(), "missing {key}");
    }
    assert!(v["params"]["maxWidth"].is_number());
    assert!(v["pages"][0]["width"].is_number());
    assert!(v.get("exporterState").is_some());
}

#[test]
fn invalid_projects_are_rejected() {
    assert_eq!(
        AtlasProject::from_json("{not json").unwrap_err().code,
        ATLAS_PROJECT_INVALID
    );
    let r = build_with(&initial(), &base(), None, IncrementalMode::KeepPositions);

    let mut p = r.project.clone();
    p.version = 99;
    let err = AtlasProject::from_json(&p.to_json()).unwrap_err();
    assert_eq!(
        (err.code.as_str(), err.params["reason"].as_str()),
        (ATLAS_PROJECT_INVALID, Some("unsupportedVersion"))
    );

    let mut p = r.project.clone();
    p.sprites[0].frame.x = 10_000;
    let err = build(
        to_inputs(&initial()),
        &base(),
        Some(&p),
        IncrementalMode::KeepPositions,
    )
    .unwrap_err();
    assert_eq!(err.code, ATLAS_PROJECT_INVALID);

    let mut p = r.project.clone();
    p.sprites[1].page = 7;
    assert_eq!(p.validate().unwrap_err().params["reason"], "pageIndex");

    let mut p = r.project.clone();
    let dup = p.sprites[0].name.clone();
    p.sprites[1].aliases.push(dup);
    assert_eq!(p.validate().unwrap_err().params["reason"], "duplicateName");
}

#[test]
fn incremental_is_deterministic() {
    let params = AtlasParams {
        padding: 1,
        ..base()
    };
    let v1 = build_with(&initial(), &params, None, IncrementalMode::KeepPositions);
    let mut more = initial();
    more.extend(quads(3));
    let a = keep(&more, &params, &v1.project);
    let b = keep(&more, &params, &v1.project);
    assert_eq!(a.project.to_json(), b.project.to_json());
    assert_eq!(a.pages[0].as_raw(), b.pages[0].as_raw());
}

mod keep_positions_prop {
    use super::*;
    use proptest::prelude::*;
    use texopt_core::atlas::{PackAlgorithm, PackHeuristic, SizeMode};

    #[derive(Debug, Clone, Copy)]
    enum Change {
        Keep,
        Repaint,
        Resize(u32, u32),
        Remove,
    }

    fn arb_change() -> impl Strategy<Value = Change> {
        prop_oneof![
            3 => Just(Change::Keep),
            1 => Just(Change::Repaint),
            1 => (1u32..=40, 1u32..=40).prop_map(|(w, h)| Change::Resize(w, h)),
            1 => Just(Change::Remove),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 200, ..ProptestConfig::default() })]

        /// Random edits in keepPositions mode: invariants hold and every
        /// sprite whose size did not change keeps its exact rect.
        #[test]
        fn keep_positions_invariants(
            initial in prop::collection::vec((1u32..=40, 1u32..=40), 1..12),
            changes in prop::collection::vec(arb_change(), 12),
            added in prop::collection::vec((1u32..=40, 1u32..=40), 0..8),
            (padding, extrude, border) in (0u32..=3, 0u32..=2, 0u32..=3),
            (rot, pot, square, fixed) in (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()),
            skyline in any::<bool>(),
            max in prop_oneof![Just(128u32), Just(256)],
        ) {
            let params = AtlasParams {
                algorithm: if skyline { PackAlgorithm::Skyline } else { PackAlgorithm::MaxRects },
                heuristic: if skyline { PackHeuristic::MinWaste } else { PackHeuristic::BestShortSideFit },
                padding,
                extrude,
                border,
                allow_rotation: rot,
                force_pot: pot,
                force_square: square,
                size_mode: if fixed { SizeMode::Fixed } else { SizeMode::ShrinkToFit },
                multi_page: true,
                trim: false,
                dedupe: false,
                max_width: max,
                max_height: max,
                ..AtlasParams::default()
            };
            let v1_in: Vec<(String, ImageBuf)> = initial
                .iter()
                .enumerate()
                .map(|(i, &(w, h))| (format!("o{i:02}"), pattern(w, h, i as u8)))
                .collect();
            let v1 = build_with(&v1_in, &params, None, IncrementalMode::KeepPositions);

            let mut v2_in = Vec::new();
            let mut unchanged_size = Vec::new();
            for (i, (name, img)) in v1_in.iter().enumerate() {
                match changes[i] {
                    Change::Keep => {
                        v2_in.push((name.clone(), img.clone()));
                        unchanged_size.push(name.clone());
                    }
                    Change::Repaint => {
                        v2_in.push((name.clone(), pattern(img.width(), img.height(), 200 + i as u8)));
                        unchanged_size.push(name.clone());
                    }
                    Change::Resize(w, h) => {
                        v2_in.push((name.clone(), pattern(w, h, 100 + i as u8)));
                        if (w, h) == img.dimensions() {
                            unchanged_size.push(name.clone());
                        }
                    }
                    Change::Remove => {}
                }
            }
            for (i, &(w, h)) in added.iter().enumerate() {
                v2_in.push((format!("n{i:02}"), pattern(w, h, 50 + i as u8)));
            }
            prop_assume!(!v2_in.is_empty());
            let v2 = keep(&v2_in, &params, &v1.project);
            prop_assert!(v2.warnings.is_empty());
            for n in &unchanged_size {
                assert_same_rect(&v1.project, &v2.project, n);
            }
        }
    }
}
