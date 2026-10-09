use image::Rgba;
use serde_json::json;
use texopt_core::ImageBuf;
use texopt_core::error::codes;
use texopt_core::fixtures::*;
use texopt_core::ops::common::{Anchor, Color};
use texopt_core::ops::pot_pad::{self, PadFill, PotPadParams, PotTarget};

const ANCHORS: [(Anchor, u32, u32); 9] = [
    (Anchor::TopLeft, 0, 0),
    (Anchor::Top, 1, 0),
    (Anchor::TopRight, 2, 0),
    (Anchor::Left, 0, 1),
    (Anchor::Center, 1, 1),
    (Anchor::Right, 2, 1),
    (Anchor::BottomLeft, 0, 2),
    (Anchor::Bottom, 1, 2),
    (Anchor::BottomRight, 2, 2),
];

fn off(factor: u32, diff: u32) -> u32 {
    match factor {
        0 => 0,
        1 => diff / 2,
        _ => diff,
    }
}

fn pad(img: &ImageBuf, p: &PotPadParams) -> ImageBuf {
    pot_pad::apply(img, p).expect("pot pad ok").image
}

/// Every pixel has a unique color so placement can be verified exactly.
fn unique(w: u32, h: u32) -> ImageBuf {
    ImageBuf::from_fn(w, h, |x, y| {
        Rgba([x as u8 * 30 + 10, y as u8 * 30 + 10, 77, 255])
    })
}

#[test]
fn defaults() {
    let p = PotPadParams::default();
    assert_eq!(p.target, PotTarget::NextPot);
    assert_eq!(p.fill, PadFill::Transparent);
    assert_eq!(p.anchor, Anchor::Center);
    assert_eq!(p.min_size, 0);
    assert_eq!(p.max_size, 8192);
}

#[test]
fn targets() {
    let img = gradient(100, 30);
    assert_eq!(pad(&img, &PotPadParams::default()).dimensions(), (128, 32));
    let sq = PotPadParams {
        target: PotTarget::SquarePot,
        ..Default::default()
    };
    assert_eq!(pad(&img, &sq).dimensions(), (128, 128));
    let fixed = PotPadParams {
        target: PotTarget::Fixed,
        width: 256,
        height: 64,
        ..Default::default()
    };
    assert_eq!(pad(&img, &fixed).dimensions(), (256, 64));
    assert_eq!(pot_pad::target_size(100, 30, &fixed).unwrap(), (256, 64));
}

#[test]
fn anchors_place_content() {
    let img = unique(5, 6);
    for (anchor, fx, fy) in ANCHORS {
        let p = PotPadParams {
            anchor,
            ..Default::default()
        };
        let out = pad(&img, &p);
        assert_eq!(out.dimensions(), (8, 8));
        let (ox, oy) = (off(fx, 3), off(fy, 2));
        for (x, y, px) in out.enumerate_pixels() {
            let inside = (ox..ox + 5).contains(&x) && (oy..oy + 6).contains(&y);
            if inside {
                assert_eq!(px, img.get_pixel(x - ox, y - oy), "{anchor:?} at {x},{y}");
            } else {
                assert_eq!(*px, TRANSPARENT, "{anchor:?} at {x},{y}");
            }
        }
    }
}

#[test]
fn fill_color() {
    let img = unique(5, 5);
    let p = PotPadParams {
        fill: PadFill::Color,
        color: Color([1, 2, 3, 200]),
        anchor: Anchor::TopLeft,
        ..Default::default()
    };
    let out = pad(&img, &p);
    assert_eq!(out.dimensions(), (8, 8));
    for (x, y, px) in out.enumerate_pixels() {
        if x >= 5 || y >= 5 {
            assert_eq!(*px, Rgba([1, 2, 3, 200]), "border {x},{y}");
        } else {
            assert_eq!(px, img.get_pixel(x, y));
        }
    }
}

#[test]
fn fill_edge_extend() {
    let img = unique(5, 5);
    let p = PotPadParams {
        fill: PadFill::EdgeExtend,
        ..Default::default()
    };
    let out = pad(&img, &p);
    assert_eq!(out.dimensions(), (8, 8));
    // Content offset is (1, 1); border pixels replicate the nearest edge pixel.
    let checks = [
        ((0, 0), (0, 0)),
        ((0, 3), (0, 2)),
        ((3, 0), (2, 0)),
        ((7, 7), (4, 4)),
        ((6, 1), (4, 0)),
        ((7, 3), (4, 2)),
        ((2, 7), (1, 4)),
    ];
    for ((x, y), (sx, sy)) in checks {
        assert_eq!(out.get_pixel(x, y), img.get_pixel(sx, sy), "border {x},{y}");
    }
    for (x, y, px) in out.enumerate_pixels() {
        let sx = (x as i64 - 1).clamp(0, 4) as u32;
        let sy = (y as i64 - 1).clamp(0, 4) as u32;
        assert_eq!(px, img.get_pixel(sx, sy));
    }
}

#[test]
fn already_pot_is_unchanged() {
    let img = gradient(64, 32);
    assert_eq!(pad(&img, &PotPadParams::default()).as_raw(), img.as_raw());
    let sq = PotPadParams {
        target: PotTarget::SquarePot,
        ..Default::default()
    };
    let square = gradient(64, 64);
    assert_eq!(pad(&square, &sq).as_raw(), square.as_raw());
    assert_eq!(
        pad(&img, &sq).dimensions(),
        (64, 64),
        "non-square POT grows to square"
    );
    let fixed = PotPadParams {
        target: PotTarget::Fixed,
        width: 64,
        height: 32,
        ..Default::default()
    };
    assert_eq!(pad(&img, &fixed).as_raw(), img.as_raw());
}

#[test]
fn min_size() {
    let img = gradient(10, 10);
    let p = PotPadParams {
        min_size: 100,
        ..Default::default()
    };
    assert_eq!(pad(&img, &p).dimensions(), (128, 128));
    let p = PotPadParams {
        min_size: 4,
        ..Default::default()
    };
    assert_eq!(pad(&img, &p).dimensions(), (16, 16));
}

#[test]
fn too_large_errors() {
    let p = PotPadParams {
        max_size: 4096,
        ..Default::default()
    };
    let err = pot_pad::apply(&solid(5000, 10, RED), &p).unwrap_err();
    assert_eq!(err.code, codes::IMG_TOO_LARGE);
    assert_eq!(err.params["max"], json!(4096));
    assert_eq!(err.params["width"], json!(8192));
    assert_eq!(err.params["height"], json!(16));

    let fixed = PotPadParams {
        target: PotTarget::Fixed,
        width: 256,
        height: 256,
        ..Default::default()
    };
    let err = pot_pad::apply(&solid(300, 100, RED), &fixed).unwrap_err();
    assert_eq!(err.code, codes::IMG_TOO_LARGE);
    assert_eq!(err.params["max"], json!(256));
    assert_eq!(err.params["width"], json!(300));
    assert_eq!(err.params["height"], json!(100));

    let fixed_too_big = PotPadParams {
        target: PotTarget::Fixed,
        width: 9000,
        height: 16,
        ..Default::default()
    };
    let err = pot_pad::apply(&solid(4, 4, RED), &fixed_too_big).unwrap_err();
    assert_eq!(err.code, codes::IMG_TOO_LARGE);
    assert_eq!(err.params["max"], json!(8192));

    let unlimited = PotPadParams {
        max_size: 0,
        ..Default::default()
    };
    assert_eq!(
        pot_pad::target_size(9000, 3, &unlimited).unwrap(),
        (16384, 4)
    );

    let err = pot_pad::apply(&ImageBuf::new(0, 0), &PotPadParams::default()).unwrap_err();
    assert_eq!(err.code, codes::IMG_EMPTY);
}

#[test]
fn serde_round_trip_and_partial_json() {
    let p = PotPadParams {
        target: PotTarget::Fixed,
        width: 512,
        height: 256,
        anchor: Anchor::TopRight,
        fill: PadFill::EdgeExtend,
        color: Color([5, 6, 7, 8]),
        min_size: 32,
        max_size: 4096,
    };
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(
        v,
        json!({
            "target": "fixed", "width": 512, "height": 256, "anchor": "topRight", "fill": "edgeExtend",
            "color": [5, 6, 7, 8], "minSize": 32, "maxSize": 4096
        })
    );
    assert_eq!(serde_json::from_value::<PotPadParams>(v).unwrap(), p);
    assert_eq!(
        serde_json::from_str::<PotPadParams>("{}").unwrap(),
        PotPadParams::default()
    );
    assert_eq!(
        serde_json::to_value(PotTarget::NextPot).unwrap(),
        json!("nextPot")
    );
    assert_eq!(
        serde_json::to_value(PotTarget::SquarePot).unwrap(),
        json!("squarePot")
    );
    assert_eq!(
        serde_json::to_value(PadFill::Transparent).unwrap(),
        json!("transparent")
    );
    assert_eq!(
        serde_json::to_value(PadFill::Color).unwrap(),
        json!("color")
    );
}
