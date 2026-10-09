use image::Rgba;
use serde_json::{Value, json};
use texopt_core::ImageBuf;
use texopt_core::fixtures::{
    BLUE, GREEN, RED, TRANSPARENT, WHITE, checker, rect_on, solid, sprite,
};
use texopt_core::ops::bg_remove::{
    self, BG_CHECKER_NOT_DETECTED, BgMode, BgRemoveParams, ColorMetric, FillMode,
};
use texopt_core::ops::common::Color;
use texopt_core::ops::{OpOutput, OpRequest, run};

const GRAY: Rgba<u8> = Rgba([204, 204, 204, 255]);

fn params(mode: BgMode) -> BgRemoveParams {
    BgRemoveParams {
        mode,
        ..BgRemoveParams::default()
    }
}

fn meta(out: &OpOutput) -> &Value {
    out.meta.as_ref().expect("bg_remove always returns meta")
}

fn removed(out: &OpOutput) -> u64 {
    meta(out)["removedPixels"].as_u64().unwrap()
}

/// Deterministic pseudo-random noise in [-amp, amp].
fn noise(seed: &mut u32, amp: i32) -> i32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    ((*seed >> 16) % (2 * amp as u32 + 1)) as i32 - amp
}

/// White background with per-channel noise (values in 255-amp..=255) and a red
/// square. Every 4th background pixel stays exactly white.
fn noisy_white_with_red(amp: i32) -> ImageBuf {
    let mut seed = 7u32;
    let mut img = rect_on(40, 40, WHITE, 12, 12, 16, 16, RED);
    for (x, y, p) in img.enumerate_pixels_mut() {
        if *p == WHITE && (x + y) % 4 != 0 {
            let mut c = [0u8; 3];
            for v in &mut c {
                *v = (255 - noise(&mut seed, amp).abs()) as u8;
            }
            *p = Rgba([c[0], c[1], c[2], 255]);
        }
    }
    img
}

fn assert_only_rect_opaque(img: &ImageBuf, x0: u32, y0: u32, w: u32, h: u32, fg: Rgba<u8>) {
    for (x, y, p) in img.enumerate_pixels() {
        let inside = x >= x0 && x < x0 + w && y >= y0 && y < y0 + h;
        if inside {
            assert_eq!(*p, fg, "foreground pixel ({x},{y}) changed");
        } else {
            assert_eq!(p[3], 0, "background pixel ({x},{y}) not removed: {p:?}");
        }
    }
}

#[test]
fn removes_pure_white_background() {
    let img = rect_on(32, 32, WHITE, 8, 8, 16, 16, RED);
    let out = bg_remove::apply(&img, &params(BgMode::White)).unwrap();
    assert_only_rect_opaque(&out.image, 8, 8, 16, 16, RED);
    assert_eq!(removed(&out), 32 * 32 - 16 * 16);
    assert_eq!(meta(&out)["detectedMode"], "white");
    assert_eq!(meta(&out)["checkerCellSize"], Value::Null);
}

#[test]
fn removes_noisy_white_within_tolerance() {
    let img = noisy_white_with_red(10);
    let p = BgRemoveParams {
        tolerance: 10.0,
        ..params(BgMode::White)
    };
    let out = bg_remove::apply(&img, &p).unwrap();
    assert_only_rect_opaque(&out.image, 12, 12, 16, 16, RED);
}

#[test]
fn tolerance_zero_only_removes_exact_matches() {
    let img = noisy_white_with_red(10);
    let exact = img.pixels().filter(|p| **p == WHITE).count() as u64;
    assert!(
        exact > 0 && exact < 40 * 40 - 256,
        "fixture must mix exact and noisy whites"
    );

    let p0 = BgRemoveParams {
        tolerance: 0.0,
        fill: FillMode::Global,
        ..params(BgMode::White)
    };
    let out0 = bg_remove::apply(&img, &p0).unwrap();
    assert_eq!(removed(&out0), exact);

    let p_hi = BgRemoveParams {
        tolerance: 20.0,
        fill: FillMode::Global,
        ..params(BgMode::White)
    };
    let out_hi = bg_remove::apply(&img, &p_hi).unwrap();
    assert_eq!(removed(&out_hi), 40 * 40 - 256);
}

#[test]
fn tolerance_controls_near_white_background() {
    let near = Rgba([250, 250, 250, 255]);
    let img = rect_on(16, 16, near, 4, 4, 8, 8, BLUE);
    let strict = BgRemoveParams {
        tolerance: 0.0,
        ..params(BgMode::White)
    };
    assert_eq!(removed(&bg_remove::apply(&img, &strict).unwrap()), 0);
    let loose = BgRemoveParams {
        tolerance: 5.0,
        ..params(BgMode::White)
    };
    let out = bg_remove::apply(&img, &loose).unwrap();
    assert_only_rect_opaque(&out.image, 4, 4, 8, 8, BLUE);
}

fn checker_with_red(cell: u32) -> ImageBuf {
    let mut img = checker(128, 128, cell, WHITE, GRAY);
    for y in 40..88 {
        for x in 40..88 {
            img.put_pixel(x, y, RED);
        }
    }
    img
}

fn assert_bg_colors(out: &OpOutput, expected: &[[u8; 4]]) {
    let mut got: Vec<[u8; 4]> = serde_json::from_value(meta(out)["bgColors"].clone()).unwrap();
    let mut exp = expected.to_vec();
    got.sort();
    exp.sort();
    assert_eq!(got, exp);
}

#[test]
fn checker_auto_detected_for_8_16_32_px_cells() {
    for cell in [8u32, 16, 32] {
        let img = checker_with_red(cell);
        for mode in [BgMode::Auto, BgMode::Checker] {
            let out = bg_remove::apply(&img, &params(mode)).unwrap();
            assert_eq!(
                meta(&out)["detectedMode"],
                "checker",
                "cell {cell}, mode {mode:?}"
            );
            assert_eq!(meta(&out)["checkerCellSize"], json!(cell), "cell {cell}");
            assert_bg_colors(&out, &[WHITE.0, GRAY.0]);
            assert_only_rect_opaque(&out.image, 40, 40, 48, 48, RED);
        }
    }
}

#[test]
fn noisy_checker_is_detected_and_removed() {
    let mut seed = 3u32;
    let mut img = checker_with_red(16);
    for p in img.pixels_mut() {
        if *p != RED {
            for k in 0..3 {
                p[k] = (p[k] as i32 + noise(&mut seed, 6)).clamp(0, 255) as u8;
            }
        }
    }
    let out = bg_remove::apply(&img, &params(BgMode::Auto)).unwrap();
    assert_eq!(meta(&out)["detectedMode"], "checker");
    assert_eq!(meta(&out)["checkerCellSize"], json!(16));
    let colors: Vec<[u8; 4]> = serde_json::from_value(meta(&out)["bgColors"].clone()).unwrap();
    for c in colors {
        let near = |t: Rgba<u8>| (0..3).all(|k| (c[k] as i32 - t[k] as i32).abs() <= 3);
        assert!(near(WHITE) || near(GRAY), "{c:?}");
    }
    assert_only_rect_opaque(&out.image, 40, 40, 48, 48, RED);
}

#[test]
fn checker_with_phase_offset_and_global_fill() {
    let cell = 16;
    let mut img = ImageBuf::from_fn(100, 90, |x, y| {
        if (((x + 5) / cell) + ((y + 11) / cell)) % 2 == 0 {
            WHITE
        } else {
            GRAY
        }
    });
    for y in 30..60 {
        for x in 30..60 {
            img.put_pixel(x, y, BLUE);
        }
    }
    for fill in [FillMode::FloodFromEdges, FillMode::Global] {
        let out = bg_remove::apply(
            &img,
            &BgRemoveParams {
                fill,
                ..params(BgMode::Auto)
            },
        )
        .unwrap();
        assert_eq!(meta(&out)["checkerCellSize"], json!(cell));
        assert_only_rect_opaque(&out.image, 30, 30, 30, 30, BLUE);
    }
}

#[test]
fn checker_with_explicit_cell_size() {
    let img = checker_with_red(16);
    let p = BgRemoveParams {
        checker_cell_size: Some(16),
        ..params(BgMode::Checker)
    };
    let out = bg_remove::apply(&img, &p).unwrap();
    assert_eq!(meta(&out)["checkerCellSize"], json!(16));
    assert_only_rect_opaque(&out.image, 40, 40, 48, 48, RED);
}

#[test]
fn checker_mode_fails_on_solid_background() {
    let img = rect_on(64, 64, WHITE, 8, 8, 8, 8, RED);
    let err = bg_remove::apply(&img, &params(BgMode::Checker)).unwrap_err();
    assert_eq!(err.code, BG_CHECKER_NOT_DETECTED);
}

#[test]
fn removes_custom_color() {
    let img = rect_on(24, 24, GREEN, 6, 6, 10, 10, BLUE);
    let p = BgRemoveParams {
        color: Color([0, 255, 0, 255]),
        ..params(BgMode::Color)
    };
    let out = bg_remove::apply(&img, &p).unwrap();
    assert_only_rect_opaque(&out.image, 6, 6, 10, 10, BLUE);
    assert_eq!(meta(&out)["detectedMode"], "color");
}

#[test]
fn auto_mode_uses_dominant_border_color() {
    let img = rect_on(24, 24, GREEN, 6, 6, 10, 10, BLUE);
    let out = bg_remove::apply(&img, &params(BgMode::Auto)).unwrap();
    assert_eq!(meta(&out)["detectedMode"], "color");
    assert_bg_colors(&out, &[GREEN.0]);
    assert_only_rect_opaque(&out.image, 6, 6, 10, 10, BLUE);

    let white = rect_on(24, 24, WHITE, 6, 6, 10, 10, BLUE);
    let out = bg_remove::apply(&white, &params(BgMode::Auto)).unwrap();
    assert_eq!(meta(&out)["detectedMode"], "white");
}

/// White canvas, red ring (3 px thick) around an enclosed white hole.
fn ring_with_hole() -> ImageBuf {
    let mut img = rect_on(32, 32, WHITE, 8, 8, 16, 16, RED);
    for y in 11..21 {
        for x in 11..21 {
            img.put_pixel(x, y, WHITE);
        }
    }
    img
}

#[test]
fn flood_fill_keeps_enclosed_region_global_removes_it() {
    let img = ring_with_hole();
    let flood = bg_remove::apply(&img, &params(BgMode::White)).unwrap();
    assert_eq!(
        *flood.image.get_pixel(15, 15),
        WHITE,
        "enclosed white must stay opaque"
    );
    assert_eq!(flood.image.get_pixel(0, 0)[3], 0);
    assert_eq!(removed(&flood), 32 * 32 - 16 * 16);

    let global = bg_remove::apply(
        &img,
        &BgRemoveParams {
            fill: FillMode::Global,
            ..params(BgMode::White)
        },
    )
    .unwrap();
    assert_eq!(
        global.image.get_pixel(15, 15)[3],
        0,
        "global removes enclosed white"
    );
    assert_eq!(removed(&global), 32 * 32 - 16 * 16 + 100);
    assert_eq!(*global.image.get_pixel(9, 9), RED);
}

#[test]
fn feather_ramps_alpha_only_within_band() {
    let (x0, y0, w, h) = (10u32, 10u32, 20u32, 20u32);
    let img = rect_on(40, 40, WHITE, x0, y0, w, h, RED);
    let feather = 3u32;
    let out = bg_remove::apply(
        &img,
        &BgRemoveParams {
            feather,
            ..params(BgMode::White)
        },
    )
    .unwrap();
    for (x, y, p) in out.image.enumerate_pixels() {
        let inside = x >= x0 && x < x0 + w && y >= y0 && y < y0 + h;
        if !inside {
            assert_eq!(p[3], 0);
            continue;
        }
        // 1-based depth from the nearest rectangle edge.
        let depth = (x - x0 + 1).min(x0 + w - x).min(y - y0 + 1).min(y0 + h - y);
        if depth <= feather {
            assert!(
                p[3] > 0 && p[3] < 255,
                "({x},{y}) depth {depth} alpha {}",
                p[3]
            );
        } else {
            assert_eq!(p[3], 255, "({x},{y}) depth {depth} must be opaque");
        }
        assert_eq!(
            [p[0], p[1], p[2]],
            [255, 0, 0],
            "feather must not change color"
        );
    }
    // Alpha grows monotonically with depth along a row.
    let row: Vec<u8> = (0..=feather)
        .map(|d| out.image.get_pixel(x0 + d, y0 + h / 2)[3])
        .collect();
    assert!(row.windows(2).all(|w| w[0] < w[1]), "{row:?}");
}

/// Dark sprite on white with a 1-px anti-aliased ring (50% blend with white).
fn dark_sprite_with_white_halo() -> (ImageBuf, Vec<(u32, u32)>) {
    let dark = Rgba([40, 40, 40, 255]);
    let blend = Rgba([147, 147, 147, 255]);
    let mut img = rect_on(48, 48, WHITE, 11, 11, 26, 26, blend);
    for y in 12..36 {
        for x in 12..36 {
            img.put_pixel(x, y, dark);
        }
    }
    let ring = img
        .enumerate_pixels()
        .filter(|(_, _, p)| **p == blend)
        .map(|(x, y, _)| (x, y))
        .collect();
    (img, ring)
}

fn mean_brightness(img: &ImageBuf, pts: &[(u32, u32)]) -> f64 {
    let sum: f64 = pts
        .iter()
        .map(|&(x, y)| {
            let p = img.get_pixel(x, y);
            assert!(p[3] > 0, "edge pixel ({x},{y}) should not be fully removed");
            (p[0] as f64 + p[1] as f64 + p[2] as f64) / 3.0
        })
        .sum();
    sum / pts.len() as f64
}

#[test]
fn defringe_reduces_white_halo_brightness() {
    let (img, ring) = dark_sprite_with_white_halo();
    let base = BgRemoveParams {
        tolerance: 10.0,
        ..params(BgMode::White)
    };
    let before = bg_remove::apply(&img, &base).unwrap();
    let b = mean_brightness(&before.image, &ring);
    assert!(
        (b - 147.0).abs() < 0.5,
        "without defringe the halo stays: {b}"
    );

    let after = bg_remove::apply(
        &img,
        &BgRemoveParams {
            defringe: true,
            defringe_strength: 100.0,
            ..base.clone()
        },
    )
    .unwrap();
    let a = mean_brightness(&after.image, &ring);
    assert!(
        a < 60.0,
        "defringe should un-blend the halo toward the dark sprite: {a}"
    );
    for &(x, y) in &ring {
        let alpha = after.image.get_pixel(x, y)[3];
        assert!(alpha > 100 && alpha < 155, "halo alpha ~50%: {alpha}");
    }
    // Interior untouched.
    assert_eq!(*after.image.get_pixel(24, 24), Rgba([40, 40, 40, 255]));

    let half = bg_remove::apply(
        &img,
        &BgRemoveParams {
            defringe: true,
            defringe_strength: 50.0,
            ..base
        },
    )
    .unwrap();
    let h = mean_brightness(&half.image, &ring);
    assert!(
        h < b && h > a,
        "half strength sits between: {a} < {h} < {b}"
    );
}

#[test]
fn already_transparent_image_is_unchanged() {
    let img = solid(20, 20, TRANSPARENT);
    for mode in [BgMode::Auto, BgMode::White, BgMode::Color] {
        for fill in [FillMode::FloodFromEdges, FillMode::Global] {
            let p = BgRemoveParams {
                fill,
                feather: 2,
                defringe: true,
                ..params(mode)
            };
            let out = bg_remove::apply(&img, &p).unwrap();
            assert_eq!(out.image, img);
            assert_eq!(removed(&out), 0);
        }
    }
    let spr = sprite(20, 20, 5, 5, 10, 10, RED);
    let out = bg_remove::apply(
        &spr,
        &BgRemoveParams {
            feather: 2,
            ..params(BgMode::Auto)
        },
    )
    .unwrap();
    assert_eq!(out.image, spr);
    assert_eq!(removed(&out), 0);
}

#[test]
fn lab_and_rgb_metrics_both_remove_noisy_white() {
    let img = noisy_white_with_red(6);
    for (metric, tolerance) in [(ColorMetric::Rgb, 5.0), (ColorMetric::Lab, 8.0)] {
        let p = BgRemoveParams {
            metric,
            tolerance,
            ..params(BgMode::White)
        };
        let out = bg_remove::apply(&img, &p).unwrap();
        assert_only_rect_opaque(&out.image, 12, 12, 16, 16, RED);
    }
}

#[test]
fn lab_metric_is_perceptual() {
    // Same RGB distance (40) from white, but lowering green is perceptually a
    // much bigger change (ΔE ≈ 27) than lowering blue (ΔE ≈ 20).
    let yellowish = solid(8, 8, Rgba([255, 255, 215, 255]));
    let pinkish = solid(8, 8, Rgba([255, 215, 255, 255]));
    let run_with = |img: &ImageBuf, metric, tolerance| {
        removed(
            &bg_remove::apply(
                img,
                &BgRemoveParams {
                    metric,
                    tolerance,
                    ..params(BgMode::White)
                },
            )
            .unwrap(),
        )
    };
    // RGB treats both the same (40 / 441.7 ≈ 9.06 %).
    assert_eq!(run_with(&yellowish, ColorMetric::Rgb, 10.0), 64);
    assert_eq!(run_with(&pinkish, ColorMetric::Rgb, 10.0), 64);
    assert_eq!(run_with(&yellowish, ColorMetric::Rgb, 8.0), 0);
    assert_eq!(run_with(&pinkish, ColorMetric::Rgb, 8.0), 0);
    // Lab separates them.
    assert_eq!(run_with(&yellowish, ColorMetric::Lab, 23.0), 64);
    assert_eq!(run_with(&pinkish, ColorMetric::Lab, 23.0), 0);
}

#[test]
fn rejects_out_of_range_params() {
    let img = solid(4, 4, WHITE);
    for p in [
        BgRemoveParams {
            tolerance: 101.0,
            ..BgRemoveParams::default()
        },
        BgRemoveParams {
            tolerance: f32::NAN,
            ..BgRemoveParams::default()
        },
        BgRemoveParams {
            defringe_strength: -1.0,
            ..BgRemoveParams::default()
        },
        BgRemoveParams {
            checker_cell_size: Some(0),
            ..BgRemoveParams::default()
        },
    ] {
        assert_eq!(
            bg_remove::apply(&img, &p).unwrap_err().code,
            "INVALID_PARAMS"
        );
    }
}

#[test]
fn params_serde_contract() {
    let p: BgRemoveParams = serde_json::from_str("{}").unwrap();
    assert_eq!(p, BgRemoveParams::default());
    let p: BgRemoveParams = serde_json::from_value(json!({
        "mode": "checker", "checkerCellSize": 16, "fill": "global", "metric": "lab",
        "tolerance": 12.5, "feather": 2, "defringe": true, "defringeStrength": 50,
        "color": [1, 2, 3, 255]
    }))
    .unwrap();
    assert_eq!(p.mode, BgMode::Checker);
    assert_eq!(p.checker_cell_size, Some(16));
    assert_eq!(p.fill, FillMode::Global);
    assert_eq!(p.metric, ColorMetric::Lab);
    assert_eq!(p.color, Color([1, 2, 3, 255]));
    let v = serde_json::to_value(BgRemoveParams::default()).unwrap();
    for key in [
        "mode",
        "color",
        "checkerCellSize",
        "fill",
        "tolerance",
        "metric",
        "feather",
        "defringe",
        "defringeStrength",
    ] {
        assert!(v.get(key).is_some(), "missing key {key}");
    }
    assert_eq!(v["mode"], "auto");
    assert_eq!(v["fill"], "floodFromEdges");

    let req: OpRequest =
        serde_json::from_value(json!({"kind": "bgRemove", "params": {"mode": "white"}})).unwrap();
    let out = run(&rect_on(8, 8, WHITE, 2, 2, 4, 4, RED), &req).unwrap();
    assert_eq!(removed(&out), 48);
}
