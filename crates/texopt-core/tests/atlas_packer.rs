mod atlas_support;

use atlas_support::*;
use proptest::prelude::*;
use texopt_core::ImageBuf;
use texopt_core::atlas::codes::*;
use texopt_core::atlas::{
    AtlasParams, IncrementalMode, PackAlgorithm, PackHeuristic, SizeMode, SortBy, SpriteInput,
    build,
};
use texopt_core::fixtures;

const COMBOS: [(PackAlgorithm, PackHeuristic); 7] = [
    (PackAlgorithm::MaxRects, PackHeuristic::BestShortSideFit),
    (PackAlgorithm::MaxRects, PackHeuristic::BestLongSideFit),
    (PackAlgorithm::MaxRects, PackHeuristic::BestAreaFit),
    (PackAlgorithm::MaxRects, PackHeuristic::BottomLeft),
    (PackAlgorithm::MaxRects, PackHeuristic::ContactPoint),
    (PackAlgorithm::Skyline, PackHeuristic::BottomLeft),
    (PackAlgorithm::Skyline, PackHeuristic::MinWaste),
];

const SORTS: [SortBy; 6] = [
    SortBy::Area,
    SortBy::MaxSide,
    SortBy::Height,
    SortBy::Width,
    SortBy::Name,
    SortBy::None,
];

fn params_base() -> AtlasParams {
    AtlasParams {
        padding: 0,
        trim: false,
        dedupe: false,
        ..AtlasParams::default()
    }
}

fn run(
    inputs: &[(String, ImageBuf)],
    params: &AtlasParams,
) -> texopt_core::OpResult<texopt_core::atlas::AtlasResult> {
    build(
        to_inputs(inputs),
        params,
        None,
        IncrementalMode::KeepPositions,
    )
}

fn arb_params() -> impl Strategy<Value = AtlasParams> {
    (
        0usize..COMBOS.len(),
        0usize..SORTS.len(),
        (0u32..=3, 0u32..=2, 0u32..=3),
        (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()),
        (any::<bool>(), any::<bool>(), any::<bool>()),
        prop_oneof![Just(64u32), Just(100), Just(128), Just(256)],
        prop_oneof![Just(64u32), Just(128), Just(200), Just(256)],
    )
        .prop_map(
            |(
                combo,
                sort,
                (padding, extrude, border),
                (rot, pot, square, fixed),
                (multi, trim, dedupe),
                mw,
                mh,
            )| {
                AtlasParams {
                    algorithm: COMBOS[combo].0,
                    heuristic: COMBOS[combo].1,
                    sort_by: SORTS[sort],
                    padding,
                    extrude,
                    border,
                    allow_rotation: rot,
                    force_pot: pot,
                    force_square: square,
                    size_mode: if fixed {
                        SizeMode::Fixed
                    } else {
                        SizeMode::ShrinkToFit
                    },
                    multi_page: multi,
                    trim,
                    trim_threshold: 0,
                    dedupe,
                    max_width: mw,
                    max_height: mh,
                    premultiply_alpha: false,
                }
            },
        )
}

fn arb_sprites() -> impl Strategy<Value = Vec<(String, ImageBuf)>> {
    // `dup == 0` copies the previous sprite's pixels so dedupe gets exercised.
    prop::collection::vec((1u32..=40, 1u32..=40, 0u8..3, 0u32..3, 0u8..4), 1..18).prop_map(
        |specs| {
            let mut out: Vec<(String, ImageBuf)> = Vec::new();
            for (i, (w, h, seed, margin, dup)) in specs.into_iter().enumerate() {
                let img = match out.last() {
                    Some((_, prev)) if dup == 0 => prev.clone(),
                    _ => padded(w, h, margin, seed),
                };
                out.push((format!("s{i:02}"), img));
            }
            out
        },
    )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 300, ..ProptestConfig::default() })]

    #[test]
    fn packing_invariants(inputs in arb_sprites(), params in arb_params()) {
        match run(&inputs, &params) {
            Ok(res) => {
                check_invariants(&res, &inputs, &params);
                if params.dedupe {
                    // identical inputs share one rect
                    for (a, ia) in &inputs {
                        for (b, ib) in &inputs {
                            if a < b && ia == ib {
                                let sa = res.project.sprite(a).unwrap();
                                let sb = res.project.sprite(b).unwrap();
                                prop_assert_eq!(&sa.name, &sb.name);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                prop_assert_eq!(e.code.as_str(), ATLAS_DOES_NOT_FIT);
                if params.multi_page {
                    // Only a sprite that cannot fit an empty max page may fail.
                    let (mw, mh) = params.effective_max();
                    let bw = i64::from(mw) - 2 * i64::from(params.border) + i64::from(params.padding);
                    let bh = i64::from(mh) - 2 * i64::from(params.border) + i64::from(params.padding);
                    let grow = i64::from(2 * params.extrude + params.padding);
                    let w = e.params["width"].as_i64().unwrap() + grow;
                    let h = e.params["height"].as_i64().unwrap() + grow;
                    let fits = (w <= bw && h <= bh) || (params.allow_rotation && h <= bw && w <= bh);
                    prop_assert!(!fits, "{:?} rejected but fits", e);
                }
            }
        }
    }
}

#[test]
fn empty_input_is_an_error() {
    let err = build(
        vec![],
        &AtlasParams::default(),
        None,
        IncrementalMode::KeepPositions,
    )
    .unwrap_err();
    assert_eq!(err.code, ATLAS_EMPTY);
}

#[test]
fn sprite_larger_than_max_does_not_fit() {
    let params = AtlasParams {
        max_width: 64,
        max_height: 64,
        ..params_base()
    };
    let inputs = vec![("big".to_string(), pattern(65, 10, 1))];
    let err = run(&inputs, &params).unwrap_err();
    assert_eq!(err.code, ATLAS_DOES_NOT_FIT);
    assert_eq!(err.params["name"], "big");
    assert_eq!(err.params["width"], 65);
    assert_eq!(err.params["height"], 10);
    assert_eq!(err.params["maxWidth"], 64);
    assert_eq!(err.params["maxHeight"], 64);

    // Even with multiPage.
    let err = run(
        &inputs,
        &AtlasParams {
            multi_page: true,
            ..params.clone()
        },
    )
    .unwrap_err();
    assert_eq!(err.code, ATLAS_DOES_NOT_FIT);

    // Rotation lets a 10x65 sprite fit a 128x64 page.
    let p = AtlasParams {
        max_width: 128,
        max_height: 64,
        allow_rotation: true,
        ..params_base()
    };
    let inputs = vec![("tall".to_string(), pattern(10, 65, 1))];
    let res = run(&inputs, &p).unwrap();
    assert!(res.project.sprites[0].rotated);
    check_invariants(&res, &inputs, &p);
}

#[test]
fn too_many_sprites_without_multipage_fails_and_with_multipage_spills() {
    let inputs: Vec<(String, ImageBuf)> = (0..10)
        .map(|i| (format!("t{i}"), pattern(30, 30, i as u8)))
        .collect();
    let params = AtlasParams {
        max_width: 64,
        max_height: 64,
        ..params_base()
    };
    let err = run(&inputs, &params).unwrap_err();
    assert_eq!(err.code, ATLAS_DOES_NOT_FIT);

    let params = AtlasParams {
        multi_page: true,
        ..params
    };
    let res = run(&inputs, &params).unwrap();
    // 4 sprites of 30x30 per 64x64 page -> 3 pages.
    assert_eq!(res.project.pages.len(), 3);
    assert_eq!(res.pages.len(), 3);
    check_invariants(&res, &inputs, &params);
    // Last page is shrunk to fit its 2 sprites.
    let last = res.project.pages[2];
    assert!(last.width * last.height < 64 * 64, "{last:?}");
}

#[test]
fn shrink_to_fit_picks_smallest_pot() {
    let inputs: Vec<(String, ImageBuf)> = (0..4)
        .map(|i| (format!("q{i}"), pattern(32, 32, i as u8)))
        .collect();
    let res = run(&inputs, &params_base()).unwrap();
    assert_eq!(
        (res.project.pages[0].width, res.project.pages[0].height),
        (64, 64)
    );

    let three: Vec<_> = inputs[..2].to_vec();
    let res = run(&three, &params_base()).unwrap();
    assert_eq!(
        (res.project.pages[0].width, res.project.pages[0].height),
        (64, 32)
    );

    let sq = AtlasParams {
        force_square: true,
        ..params_base()
    };
    let res = run(&three, &sq).unwrap();
    assert_eq!(
        (res.project.pages[0].width, res.project.pages[0].height),
        (64, 64)
    );
}

#[test]
fn non_pot_shrinks_to_content() {
    let params = AtlasParams {
        force_pot: false,
        border: 1,
        ..params_base()
    };
    let inputs = vec![("a".to_string(), pattern(30, 20, 1))];
    let res = run(&inputs, &params).unwrap();
    assert_eq!(
        (res.project.pages[0].width, res.project.pages[0].height),
        (32, 22)
    );
    check_invariants(&res, &inputs, &params);

    let params = AtlasParams {
        force_pot: false,
        force_square: true,
        ..params_base()
    };
    let res = run(&inputs, &params).unwrap();
    assert_eq!(
        (res.project.pages[0].width, res.project.pages[0].height),
        (30, 30)
    );
}

#[test]
fn fixed_size_mode_uses_max_size() {
    let params = AtlasParams {
        size_mode: SizeMode::Fixed,
        max_width: 256,
        max_height: 128,
        ..params_base()
    };
    let inputs = vec![("a".to_string(), pattern(5, 5, 1))];
    let res = run(&inputs, &params).unwrap();
    assert_eq!(
        (res.project.pages[0].width, res.project.pages[0].height),
        (256, 128)
    );
    // non-POT max is rounded down when forcePot
    let params = AtlasParams {
        max_width: 300,
        max_height: 200,
        ..params
    };
    let res = run(&inputs, &params).unwrap();
    assert_eq!(
        (res.project.pages[0].width, res.project.pages[0].height),
        (256, 128)
    );
}

#[test]
fn dedupe_shares_one_rect() {
    let img = pattern(16, 16, 9);
    let inputs = vec![
        ("b_copy".to_string(), img.clone()),
        ("a_orig".to_string(), img.clone()),
        ("c_other".to_string(), pattern(16, 16, 10)),
    ];
    let params = AtlasParams {
        dedupe: true,
        ..params_base()
    };
    let res = run(&inputs, &params).unwrap();
    assert_eq!(res.project.sprites.len(), 2);
    let s = res.project.sprite("b_copy").unwrap();
    assert_eq!(s.name, "a_orig");
    assert_eq!(s.aliases, vec!["b_copy".to_string()]);
    check_invariants(&res, &inputs, &params);

    let res = run(
        &inputs,
        &AtlasParams {
            dedupe: false,
            ..params
        },
    )
    .unwrap();
    assert_eq!(res.project.sprites.len(), 3);
}

#[test]
fn trim_keeps_source_size_and_offset() {
    let params = AtlasParams {
        trim: true,
        ..params_base()
    };
    let inputs = vec![(
        "t".to_string(),
        fixtures::sprite(20, 10, 3, 2, 5, 4, fixtures::RED),
    )];
    let res = run(&inputs, &params).unwrap();
    let s = &res.project.sprites[0];
    assert!(s.trimmed);
    assert_eq!((s.source_size.w, s.source_size.h), (20, 10));
    let ss = s.sprite_source_size;
    assert_eq!((ss.x, ss.y, ss.w, ss.h), (3, 2, 5, 4));
    assert_eq!((s.frame.w, s.frame.h), (5, 4));
    check_invariants(&res, &inputs, &params);

    // Threshold: alpha <= threshold counts as transparent.
    let mut img = fixtures::sprite(10, 10, 4, 4, 2, 2, fixtures::RED);
    img.put_pixel(0, 0, image::Rgba([0, 0, 0, 10]));
    let inputs = vec![("t".to_string(), img)];
    let res = run(
        &inputs,
        &AtlasParams {
            trim_threshold: 10,
            ..params.clone()
        },
    )
    .unwrap();
    assert_eq!(res.project.sprites[0].sprite_source_size.w, 2);
    let res = run(
        &inputs,
        &AtlasParams {
            trim_threshold: 9,
            ..params.clone()
        },
    )
    .unwrap();
    assert_eq!(res.project.sprites[0].sprite_source_size.w, 6);

    // Fully transparent sprite trims to 1x1.
    let inputs = vec![(
        "empty".to_string(),
        fixtures::solid(8, 8, fixtures::TRANSPARENT),
    )];
    let res = run(&inputs, &params).unwrap();
    let s = &res.project.sprites[0];
    assert_eq!((s.frame.w, s.frame.h, s.source_size.w), (1, 1, 8));
}

#[test]
fn output_is_deterministic() {
    let inputs: Vec<(String, ImageBuf)> = (0..25)
        .map(|i| {
            (
                format!("d{i}"),
                padded(3 + (i * 7) % 29, 2 + (i * 11) % 31, i % 3, (i % 4) as u8),
            )
        })
        .collect();
    for (algorithm, heuristic) in COMBOS {
        let params = AtlasParams {
            algorithm,
            heuristic,
            allow_rotation: true,
            extrude: 1,
            padding: 2,
            border: 1,
            ..AtlasParams::default()
        };
        let a = run(&inputs, &params).unwrap();
        let b = run(&inputs, &params).unwrap();
        assert_eq!(a.project.to_json(), b.project.to_json());
        assert_eq!(a.pages.len(), b.pages.len());
        for (pa, pb) in a.pages.iter().zip(&b.pages) {
            assert_eq!(pa.as_raw(), pb.as_raw());
        }
        check_invariants(&a, &inputs, &params);
    }
}

#[test]
fn duplicate_input_names_keep_last_and_warn() {
    let inputs = vec![
        SpriteInput::new("x", pattern(4, 4, 1)),
        SpriteInput::new("x", pattern(6, 6, 2)),
    ];
    let res = build(inputs, &params_base(), None, IncrementalMode::KeepPositions).unwrap();
    assert_eq!(res.project.sprites.len(), 1);
    assert_eq!(res.project.sprites[0].frame.w, 6);
    assert_eq!(res.warnings[0].code, ATLAS_DUPLICATE_NAME);
}

#[test]
fn invalid_params_are_rejected() {
    let inputs = vec![("a".to_string(), pattern(4, 4, 1))];
    let p = AtlasParams {
        algorithm: PackAlgorithm::Skyline,
        heuristic: PackHeuristic::ContactPoint,
        ..params_base()
    };
    assert_eq!(run(&inputs, &p).unwrap_err().code, "INVALID_PARAMS");
    let p = AtlasParams {
        heuristic: PackHeuristic::MinWaste,
        ..params_base()
    };
    assert_eq!(run(&inputs, &p).unwrap_err().code, "INVALID_PARAMS");
    let p = AtlasParams {
        max_width: 0,
        ..params_base()
    };
    assert_eq!(run(&inputs, &p).unwrap_err().code, "INVALID_PARAMS");
    let p = AtlasParams {
        max_width: 16,
        max_height: 16,
        border: 8,
        ..params_base()
    };
    assert_eq!(run(&inputs, &p).unwrap_err().code, "INVALID_PARAMS");
}

#[test]
fn params_json_shape_is_camel_case_with_defaults() {
    let p: AtlasParams =
        serde_json::from_str(r#"{"maxWidth":1024,"sortBy":"maxSide","heuristic":"bottomLeft"}"#)
            .unwrap();
    assert_eq!(p.max_width, 1024);
    assert_eq!(p.max_height, 2048);
    assert_eq!(p.sort_by, SortBy::MaxSide);
    assert_eq!(p.heuristic, PackHeuristic::BottomLeft);
    assert!(p.force_pot);
    let v = serde_json::to_value(AtlasParams::default()).unwrap();
    for key in [
        "algorithm",
        "heuristic",
        "maxWidth",
        "maxHeight",
        "forcePot",
        "forceSquare",
        "padding",
        "extrude",
        "border",
        "allowRotation",
        "trim",
        "trimThreshold",
        "dedupe",
        "multiPage",
        "sortBy",
        "premultiplyAlpha",
        "sizeMode",
    ] {
        assert!(v.get(key).is_some(), "missing {key}");
    }
    assert_eq!(v["sizeMode"], "shrinkToFit");
    assert_eq!(v["algorithm"], "maxRects");
    assert_eq!(v["heuristic"], "bestShortSideFit");
}

#[test]
fn many_sprites_pack_reasonably_fast_and_valid() {
    let inputs: Vec<(String, ImageBuf)> = (0..300)
        .map(|i| {
            (
                format!("m{i:03}"),
                pattern(4 + (i * 13) % 37, 4 + (i * 17) % 41, i as u8),
            )
        })
        .collect();
    for (algorithm, heuristic) in [COMBOS[0], COMBOS[4], COMBOS[6]] {
        let params = AtlasParams {
            algorithm,
            heuristic,
            allow_rotation: true,
            ..AtlasParams::default()
        };
        let res = run(&inputs, &params).unwrap();
        check_invariants(&res, &inputs, &params);
        let page = res.project.pages[0];
        let used: u64 = res
            .project
            .sprites
            .iter()
            .map(|s| u64::from(s.frame.w + 2) * u64::from(s.frame.h + 2))
            .sum();
        // Packing efficiency sanity check (> 45% of the page used).
        assert!(
            used * 100 / (u64::from(page.width) * u64::from(page.height)) > 45,
            "{page:?} used {used}"
        );
    }
}
