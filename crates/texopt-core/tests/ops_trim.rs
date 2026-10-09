use image::Rgba;
use serde_json::json;
use texopt_core::ImageBuf;
use texopt_core::error::codes;
use texopt_core::fixtures::*;
use texopt_core::ops::OpOutput;
use texopt_core::ops::common::SnapMode;
use texopt_core::ops::trim::{
    self, TRIM_EMPTY, TrimEmptyBehavior, TrimMeta, TrimParams, TrimRect, TrimSize,
};

fn trim_ok(img: &ImageBuf, p: &TrimParams) -> (ImageBuf, TrimMeta) {
    let OpOutput { image, meta } = trim::apply(img, p).expect("trim ok");
    let meta: TrimMeta = serde_json::from_value(meta.expect("trim meta")).unwrap();
    (image, meta)
}

fn rect(x: i64, y: i64, w: u32, h: u32) -> TrimRect {
    TrimRect { x, y, w, h }
}

/// Paste the trimmed image back into a transparent canvas of the source size.
fn reconstruct(out: &ImageBuf, meta: &TrimMeta) -> ImageBuf {
    let mut canvas = solid(meta.source_size.w, meta.source_size.h, TRANSPARENT);
    for (x, y, px) in out.enumerate_pixels() {
        let (cx, cy) = (meta.trim_rect.x + x as i64, meta.trim_rect.y + y as i64);
        if cx >= 0
            && cy >= 0
            && (cx as u32) < meta.source_size.w
            && (cy as u32) < meta.source_size.h
        {
            canvas.put_pixel(cx as u32, cy as u32, *px);
        } else {
            assert_eq!(
                *px, TRANSPARENT,
                "pixels outside the source must be transparent"
            );
        }
    }
    canvas
}

#[test]
fn defaults() {
    let p = TrimParams::default();
    assert_eq!(p.alpha_threshold, 0);
    assert_eq!(p.margin, 0);
    assert!(p.trim_left && p.trim_right && p.trim_top && p.trim_bottom);
    assert_eq!(p.snap, SnapMode::None);
    assert_eq!(p.empty_behavior, TrimEmptyBehavior::Error);
}

#[test]
fn basic_trim_and_meta_json_shape() {
    let img = sprite(20, 10, 4, 2, 6, 3, RED);
    let out = trim::apply(&img, &TrimParams::default()).unwrap();
    assert_eq!(out.image.dimensions(), (6, 3));
    assert!(out.image.pixels().all(|px| *px == RED));
    assert_eq!(
        out.meta.unwrap(),
        json!({ "sourceSize": { "w": 20, "h": 10 }, "trimRect": { "x": 4, "y": 2, "w": 6, "h": 3 } })
    );
}

#[test]
fn fully_transparent_error() {
    let img = solid(8, 8, TRANSPARENT);
    let err = trim::apply(&img, &TrimParams::default()).unwrap_err();
    assert_eq!(err.code, TRIM_EMPTY);
    assert_eq!(err.code, "TRIM_EMPTY");
    assert_eq!(err.params["width"], json!(8));
    assert_eq!(err.params["height"], json!(8));
}

#[test]
fn fully_transparent_one_pixel() {
    let img = solid(8, 6, Rgba([255, 255, 255, 3]));
    let p = TrimParams {
        alpha_threshold: 3,
        empty_behavior: TrimEmptyBehavior::OnePixel,
        ..Default::default()
    };
    let (out, meta) = trim_ok(&img, &p);
    assert_eq!(out.dimensions(), (1, 1));
    assert_eq!(*out.get_pixel(0, 0), TRANSPARENT);
    assert_eq!(meta.source_size, TrimSize { w: 8, h: 6 });
    assert_eq!(meta.trim_rect, rect(0, 0, 1, 1));
}

#[test]
fn fully_opaque_unchanged() {
    let img = gradient(20, 10);
    let (out, meta) = trim_ok(&img, &TrimParams::default());
    assert_eq!(out.as_raw(), img.as_raw());
    assert_eq!(meta.trim_rect, rect(0, 0, 20, 10));
    // Even with the max threshold below 255, opaque pixels stay.
    let (out, _) = trim_ok(
        &img,
        &TrimParams {
            alpha_threshold: 254,
            ..Default::default()
        },
    );
    assert_eq!(out.as_raw(), img.as_raw());
    // Threshold 255 treats everything as empty.
    let err = trim::apply(
        &img,
        &TrimParams {
            alpha_threshold: 255,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.code, TRIM_EMPTY);
}

#[test]
fn threshold_boundary() {
    let mut img = solid(10, 10, TRANSPARENT);
    img.put_pixel(1, 1, Rgba([0, 255, 0, 10]));
    img.put_pixel(5, 6, Rgba([255, 0, 0, 11]));
    // alpha == threshold counts as empty, threshold + 1 is kept.
    let (out, meta) = trim_ok(
        &img,
        &TrimParams {
            alpha_threshold: 10,
            ..Default::default()
        },
    );
    assert_eq!(meta.trim_rect, rect(5, 6, 1, 1));
    assert_eq!(*out.get_pixel(0, 0), Rgba([255, 0, 0, 11]));
    let (_, meta) = trim_ok(
        &img,
        &TrimParams {
            alpha_threshold: 9,
            ..Default::default()
        },
    );
    assert_eq!(meta.trim_rect, rect(1, 1, 5, 6));
    let err = trim::apply(
        &img,
        &TrimParams {
            alpha_threshold: 11,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.code, TRIM_EMPTY);
}

#[test]
fn margin_is_kept_and_clamped() {
    let img = sprite(10, 10, 4, 4, 2, 2, RED);
    let (out, meta) = trim_ok(
        &img,
        &TrimParams {
            margin: 2,
            ..Default::default()
        },
    );
    assert_eq!(meta.trim_rect, rect(2, 2, 6, 6));
    assert_eq!(*out.get_pixel(0, 0), TRANSPARENT);
    assert_eq!(*out.get_pixel(2, 2), RED);
    let (_, meta) = trim_ok(
        &img,
        &TrimParams {
            margin: 3,
            ..Default::default()
        },
    );
    assert_eq!(meta.trim_rect, rect(1, 1, 8, 8));
    let (out, meta) = trim_ok(
        &img,
        &TrimParams {
            margin: 100,
            ..Default::default()
        },
    );
    assert_eq!(meta.trim_rect, rect(0, 0, 10, 10));
    assert_eq!(out.as_raw(), img.as_raw());
}

#[test]
fn per_side_toggles() {
    let img = sprite(10, 12, 3, 4, 2, 5, RED); // content x 3..5, y 4..9
    let all = TrimParams::default();
    let cases = [
        (
            TrimParams {
                trim_left: false,
                ..all.clone()
            },
            rect(0, 4, 5, 5),
        ),
        (
            TrimParams {
                trim_right: false,
                ..all.clone()
            },
            rect(3, 4, 7, 5),
        ),
        (
            TrimParams {
                trim_top: false,
                ..all.clone()
            },
            rect(3, 0, 2, 9),
        ),
        (
            TrimParams {
                trim_bottom: false,
                ..all.clone()
            },
            rect(3, 4, 2, 8),
        ),
        (
            TrimParams {
                trim_left: false,
                trim_right: false,
                trim_top: false,
                trim_bottom: false,
                ..all.clone()
            },
            rect(0, 0, 10, 12),
        ),
        (
            TrimParams {
                trim_left: false,
                margin: 1,
                ..all.clone()
            },
            rect(0, 3, 6, 7),
        ),
    ];
    for (p, expected) in cases {
        let (out, meta) = trim_ok(&img, &p);
        assert_eq!(meta.trim_rect, expected, "{p:?}");
        assert_eq!(out.dimensions(), (expected.w, expected.h));
        assert_eq!(reconstruct(&out, &meta), img, "{p:?}");
    }
}

#[test]
fn snap_grows_canvas_centered() {
    let img = sprite(16, 16, 4, 2, 3, 5, RED);
    let (out, meta) = trim_ok(
        &img,
        &TrimParams {
            snap: SnapMode::Pot,
            ..Default::default()
        },
    );
    assert_eq!(out.dimensions(), (4, 8));
    assert_eq!(meta.trim_rect, rect(4, 1, 4, 8));
    assert_eq!(*out.get_pixel(0, 1), RED);
    assert_eq!(*out.get_pixel(3, 1), TRANSPARENT);
    assert_eq!(*out.get_pixel(0, 0), TRANSPARENT);
    assert_eq!(reconstruct(&out, &meta), img);

    let (out, meta) = trim_ok(
        &img,
        &TrimParams {
            snap: SnapMode::MultipleOf4,
            ..Default::default()
        },
    );
    assert_eq!(out.dimensions(), (4, 8));
    assert_eq!(meta.trim_rect, rect(4, 1, 4, 8));

    // Snapped canvas may extend past the source; that area is transparent.
    let img = sprite(6, 6, 0, 0, 5, 5, BLUE);
    let (out, meta) = trim_ok(
        &img,
        &TrimParams {
            snap: SnapMode::Pot,
            ..Default::default()
        },
    );
    assert_eq!(out.dimensions(), (8, 8));
    assert_eq!(meta.trim_rect, rect(-1, -1, 8, 8));
    assert_eq!(*out.get_pixel(0, 0), TRANSPARENT);
    assert_eq!(*out.get_pixel(1, 1), BLUE);
    assert_eq!(*out.get_pixel(5, 5), BLUE);
    assert_eq!(*out.get_pixel(6, 6), TRANSPARENT);
    assert_eq!(reconstruct(&out, &meta), img);
}

#[test]
fn meta_round_trip_reconstructs_original() {
    let mut img = sprite(40, 30, 7, 5, 10, 8, RED);
    for (x, y) in [(20, 20), (21, 20), (33, 26)] {
        img.put_pixel(x, y, Rgba([0, 128, 255, 77]));
    }
    img.put_pixel(9, 22, Rgba([1, 2, 3, 1]));
    let configs = [
        TrimParams::default(),
        TrimParams {
            margin: 3,
            ..Default::default()
        },
        TrimParams {
            margin: 50,
            ..Default::default()
        },
        TrimParams {
            snap: SnapMode::Pot,
            ..Default::default()
        },
        TrimParams {
            snap: SnapMode::MultipleOf4,
            margin: 1,
            ..Default::default()
        },
        TrimParams {
            trim_top: false,
            trim_right: false,
            ..Default::default()
        },
        TrimParams {
            trim_left: false,
            trim_bottom: false,
            snap: SnapMode::Pot,
            ..Default::default()
        },
    ];
    for p in configs {
        let (out, meta) = trim_ok(&img, &p);
        assert_eq!(meta.source_size, TrimSize { w: 40, h: 30 });
        assert_eq!(out.dimensions(), (meta.trim_rect.w, meta.trim_rect.h));
        assert_eq!(reconstruct(&out, &meta), img, "{p:?}");
    }
}

#[test]
fn empty_input_image() {
    let err = trim::apply(&ImageBuf::new(0, 0), &TrimParams::default()).unwrap_err();
    assert_eq!(err.code, codes::IMG_EMPTY);
}

#[test]
fn serde_round_trip_and_partial_json() {
    let p = TrimParams {
        alpha_threshold: 12,
        margin: 3,
        trim_left: false,
        trim_right: true,
        trim_top: false,
        trim_bottom: true,
        snap: SnapMode::MultipleOf4,
        empty_behavior: TrimEmptyBehavior::OnePixel,
    };
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(
        v,
        json!({
            "alphaThreshold": 12, "margin": 3, "trimLeft": false, "trimRight": true, "trimTop": false,
            "trimBottom": true, "snap": "multipleOf4", "emptyBehavior": "onePixel"
        })
    );
    assert_eq!(serde_json::from_value::<TrimParams>(v).unwrap(), p);
    assert_eq!(
        serde_json::from_str::<TrimParams>("{}").unwrap(),
        TrimParams::default()
    );
    assert_eq!(
        serde_json::to_value(TrimEmptyBehavior::Error).unwrap(),
        json!("error")
    );
}
