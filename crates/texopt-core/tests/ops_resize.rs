use std::collections::HashSet;

use image::Rgba;
use serde_json::json;
use texopt_core::ImageBuf;
use texopt_core::error::codes;
use texopt_core::fixtures::*;
use texopt_core::ops::common::{ALL_FILTERS, ResampleFilter, SnapMode, is_pot};
use texopt_core::ops::resize::{self, ResizeMode, ResizeParams};
use texopt_core::ops::{OpRequest, run};

fn with_mode(mode: ResizeMode) -> ResizeParams {
    ResizeParams {
        mode,
        ..Default::default()
    }
}

fn dims(img: &ImageBuf, p: &ResizeParams) -> (u32, u32) {
    resize::apply(img, p).expect("resize ok").image.dimensions()
}

fn colors(img: &ImageBuf) -> HashSet<[u8; 4]> {
    img.pixels().map(|p| p.0).collect()
}

#[test]
fn defaults_are_sensible() {
    let p = ResizeParams::default();
    assert_eq!(p.mode, ResizeMode::Percent);
    assert_eq!(p.percent, 50.0);
    assert!(p.keep_aspect);
    assert!(p.premultiply_alpha);
    assert!(!p.linear_space);
    assert_eq!(p.filter, ResampleFilter::Lanczos3);
    assert_eq!(p.snap, SnapMode::None);
}

#[test]
fn percent_mode() {
    let img = gradient(100, 50);
    let mut p = with_mode(ResizeMode::Percent);
    p.percent = 50.0;
    assert_eq!(dims(&img, &p), (50, 25));
    p.percent = 200.0;
    assert_eq!(dims(&img, &p), (200, 100));
    p.percent = 33.3;
    assert_eq!(dims(&gradient(10, 10), &p), (3, 3));
    p.percent = 1.0;
    assert_eq!(dims(&gradient(10, 10), &p), (1, 1), "never below 1x1");
}

#[test]
fn exact_mode_keep_aspect_on_off() {
    let img = gradient(100, 50);
    let mut p = ResizeParams {
        mode: ResizeMode::Exact,
        width: 30,
        height: 40,
        ..Default::default()
    };
    p.keep_aspect = false;
    assert_eq!(dims(&img, &p), (30, 40));
    p.keep_aspect = true; // fit inside 30x40 box
    assert_eq!(dims(&img, &p), (30, 15));
    p.width = 400;
    p.height = 100;
    assert_eq!(dims(&img, &p), (200, 100));
}

#[test]
fn fit_width_and_height() {
    let img = gradient(100, 50);
    let mut p = ResizeParams {
        mode: ResizeMode::FitWidth,
        width: 40,
        ..Default::default()
    };
    assert_eq!(dims(&img, &p), (40, 20));
    p.keep_aspect = false;
    assert_eq!(dims(&img, &p), (40, 50));

    let mut p = ResizeParams {
        mode: ResizeMode::FitHeight,
        height: 10,
        ..Default::default()
    };
    assert_eq!(dims(&img, &p), (20, 10));
    p.keep_aspect = false;
    assert_eq!(dims(&img, &p), (100, 10));
}

#[test]
fn longest_side() {
    let mut p = ResizeParams {
        mode: ResizeMode::LongestSide,
        longest_side: 64,
        ..Default::default()
    };
    assert_eq!(dims(&gradient(100, 50), &p), (64, 32));
    assert_eq!(dims(&gradient(50, 100), &p), (32, 64));
    p.keep_aspect = false;
    assert_eq!(dims(&gradient(100, 50), &p), (64, 50));
    assert_eq!(dims(&gradient(50, 100), &p), (50, 64));
}

#[test]
fn every_filter_gives_correct_dims_including_one_pixel_changes() {
    let img = gradient(33, 17);
    for filter in ALL_FILTERS {
        for (w, h) in [(34, 18), (32, 16), (64, 64), (5, 3), (1, 1), (33, 1)] {
            let p = ResizeParams {
                mode: ResizeMode::Exact,
                width: w,
                height: h,
                keep_aspect: false,
                filter,
                ..Default::default()
            };
            let out = resize::apply(&img, &p).unwrap().image;
            assert_eq!(out.dimensions(), (w, h), "{filter:?} -> {w}x{h}");
            assert!(
                out.pixels().all(|px| px[3] == 255),
                "{filter:?}: opaque input stays opaque"
            );
        }
    }
}

#[test]
fn one_by_one_image() {
    let img = solid(1, 1, RED);
    for filter in ALL_FILTERS {
        for linear_space in [false, true] {
            let p = ResizeParams {
                mode: ResizeMode::Exact,
                width: 4,
                height: 3,
                keep_aspect: false,
                filter,
                linear_space,
                ..Default::default()
            };
            let out = resize::apply(&img, &p).unwrap().image;
            assert_eq!(out.dimensions(), (4, 3));
            assert!(out.pixels().all(|px| *px == RED), "{filter:?}");
        }
    }
    let p = ResizeParams {
        percent: 10.0,
        ..Default::default()
    };
    assert_eq!(dims(&img, &p), (1, 1));
}

#[test]
fn nearest_on_pixel_art_adds_no_new_colors() {
    let mut art = checker(8, 8, 2, RED, BLUE);
    art.put_pixel(3, 4, GREEN);
    art.put_pixel(7, 7, Rgba([10, 20, 30, 128]));
    art.put_pixel(0, 5, TRANSPARENT);
    let original = colors(&art);
    for (w, h) in [(24, 24), (5, 5), (13, 7), (9, 9), (7, 8), (1, 1)] {
        let p = ResizeParams {
            mode: ResizeMode::Exact,
            width: w,
            height: h,
            keep_aspect: false,
            filter: ResampleFilter::Nearest,
            linear_space: true,
            premultiply_alpha: true,
            ..Default::default()
        };
        let out = resize::apply(&art, &p).unwrap().image;
        assert_eq!(out.dimensions(), (w, h));
        assert!(
            colors(&out).is_subset(&original),
            "{w}x{h} introduced new colors"
        );
    }
}

#[test]
fn nearest_integer_upscale_replicates_pixels() {
    let art = checker(4, 4, 1, RED, BLUE);
    let p = ResizeParams {
        percent: 200.0,
        filter: ResampleFilter::Nearest,
        ..Default::default()
    };
    let out = resize::apply(&art, &p).unwrap().image;
    assert_eq!(out.dimensions(), (8, 8));
    for (x, y, px) in out.enumerate_pixels() {
        assert_eq!(px, art.get_pixel(x / 2, y / 2));
    }
}

#[test]
fn premultiplied_alpha_avoids_dark_fringe() {
    let img = sprite(64, 64, 16, 16, 32, 32, RED);
    for filter in [
        ResampleFilter::Bilinear,
        ResampleFilter::CatmullRom,
        ResampleFilter::Mitchell,
        ResampleFilter::Lanczos3,
    ] {
        for linear_space in [false, true] {
            let p = ResizeParams {
                mode: ResizeMode::Exact,
                width: 13,
                height: 13,
                keep_aspect: false,
                filter,
                linear_space,
                premultiply_alpha: true,
                ..Default::default()
            };
            let out = resize::apply(&img, &p).unwrap().image;
            let semi: Vec<_> = out.pixels().filter(|px| px[3] > 0 && px[3] < 255).collect();
            assert!(
                !semi.is_empty(),
                "{filter:?}: expected semi-transparent edge pixels"
            );
            for px in out.pixels().filter(|px| px[3] > 0) {
                assert!(
                    px[0] >= 250 && px[1] <= 5 && px[2] <= 5,
                    "{filter:?} linear={linear_space}: fringe pixel {px:?}"
                );
            }
        }
    }
}

#[test]
fn without_premultiply_edges_darken() {
    // Documents why premultiplyAlpha defaults to true.
    let img = sprite(64, 64, 16, 16, 32, 32, RED);
    let p = ResizeParams {
        mode: ResizeMode::Exact,
        width: 13,
        height: 13,
        keep_aspect: false,
        filter: ResampleFilter::Bilinear,
        premultiply_alpha: false,
        ..Default::default()
    };
    let out = resize::apply(&img, &p).unwrap().image;
    assert!(
        out.pixels()
            .any(|px| px[3] > 0 && px[3] < 255 && px[0] < 200)
    );
}

#[test]
fn linear_space_changes_blending() {
    // 50/50 black/white downscaled to 1px: sRGB average ~128, linear-light average ~188.
    let img = checker(2, 2, 1, BLACK, WHITE);
    let base = ResizeParams {
        mode: ResizeMode::Exact,
        width: 1,
        height: 1,
        filter: ResampleFilter::Bilinear,
        ..Default::default()
    };
    let srgb = resize::apply(&img, &base).unwrap().image;
    let lin = resize::apply(
        &img,
        &ResizeParams {
            linear_space: true,
            ..base
        },
    )
    .unwrap()
    .image;
    assert!(
        (126..=129).contains(&srgb.get_pixel(0, 0)[0]),
        "{:?}",
        srgb.get_pixel(0, 0)
    );
    assert!(
        (186..=190).contains(&lin.get_pixel(0, 0)[0]),
        "{:?}",
        lin.get_pixel(0, 0)
    );
}

#[test]
fn snap_after_resize() {
    let img = gradient(100, 50);
    let mut p = ResizeParams {
        percent: 50.0,
        snap: SnapMode::MultipleOf4,
        ..Default::default()
    };
    assert_eq!(dims(&img, &p), (52, 24));
    p.snap = SnapMode::Pot;
    let (w, h) = dims(&img, &p);
    assert!(is_pot(w) && is_pot(h));
    assert_eq!((w, h), (64, 32));
    p.percent = 1.0;
    p.snap = SnapMode::MultipleOf4;
    assert_eq!(dims(&img, &p), (4, 4));
}

#[test]
fn same_size_returns_identical_bytes() {
    let img = gradient(37, 21);
    let p = ResizeParams {
        mode: ResizeMode::Exact,
        width: 37,
        height: 21,
        keep_aspect: false,
        ..Default::default()
    };
    assert_eq!(
        resize::apply(&img, &p).unwrap().image.as_raw(),
        img.as_raw()
    );
}

#[test]
fn invalid_params_and_limits() {
    let img = gradient(10, 10);
    let err = resize::apply(
        &img,
        &ResizeParams {
            percent: 0.0,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.code, codes::INVALID_PARAMS);
    assert_eq!(err.params["param"], json!("percent"));
    let err = resize::apply(
        &img,
        &ResizeParams {
            percent: f64::NAN,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.code, codes::INVALID_PARAMS);
    let err = resize::apply(
        &img,
        &ResizeParams {
            mode: ResizeMode::Exact,
            width: 0,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.code, codes::INVALID_PARAMS);
    assert_eq!(err.params["param"], json!("width"));
    let err = resize::apply(
        &img,
        &ResizeParams {
            mode: ResizeMode::LongestSide,
            longest_side: 0,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.params["param"], json!("longestSide"));
    let err = resize::apply(
        &img,
        &ResizeParams {
            percent: 1_000_000.0,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.code, codes::IMG_TOO_LARGE);
    let err = resize::apply(&ImageBuf::new(0, 0), &ResizeParams::default()).unwrap_err();
    assert_eq!(err.code, codes::IMG_EMPTY);
}

#[test]
fn serde_round_trip_and_partial_json() {
    let p = ResizeParams {
        mode: ResizeMode::FitHeight,
        percent: 12.5,
        width: 7,
        height: 9,
        longest_side: 300,
        keep_aspect: false,
        filter: ResampleFilter::Mitchell,
        linear_space: true,
        premultiply_alpha: false,
        snap: SnapMode::Pot,
    };
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(
        v,
        json!({
            "mode": "fitHeight", "percent": 12.5, "width": 7, "height": 9, "longestSide": 300,
            "keepAspect": false, "filter": "mitchell", "linearSpace": true,
            "premultiplyAlpha": false, "snap": "pot"
        })
    );
    let back: ResizeParams = serde_json::from_value(v).unwrap();
    assert_eq!(back, p);

    let empty: ResizeParams = serde_json::from_str("{}").unwrap();
    assert_eq!(empty, ResizeParams::default());
    let partial: ResizeParams = serde_json::from_str(r#"{"mode":"exact","width":10}"#).unwrap();
    assert_eq!(
        partial,
        ResizeParams {
            mode: ResizeMode::Exact,
            width: 10,
            ..Default::default()
        }
    );
    for (mode, s) in [
        (ResizeMode::Percent, "percent"),
        (ResizeMode::Exact, "exact"),
        (ResizeMode::FitWidth, "fitWidth"),
        (ResizeMode::FitHeight, "fitHeight"),
        (ResizeMode::LongestSide, "longestSide"),
    ] {
        assert_eq!(serde_json::to_value(mode).unwrap(), json!(s));
    }

    let req: OpRequest =
        serde_json::from_value(json!({"kind": "resize", "params": {"percent": 25}})).unwrap();
    let out = run(&gradient(8, 8), &req).unwrap();
    assert_eq!(out.image.dimensions(), (2, 2));
}
