use image::Rgba;
use serde_json::json;
use texopt_core::ImageBuf;
use texopt_core::error::codes;
use texopt_core::fixtures::*;
use texopt_core::ops::common::{Anchor, Color, ResampleFilter, RoundMode};
use texopt_core::ops::resolution::{self, ResolutionMethod, ResolutionParams, ResolutionTarget};

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

const PAD: Rgba<u8> = Rgba([9, 9, 9, 255]);

fn params(target: ResolutionTarget, round: RoundMode) -> ResolutionParams {
    ResolutionParams {
        target,
        round,
        ..Default::default()
    }
}

fn width_for(n: u32, p: &ResolutionParams) -> u32 {
    resolution::target_size(n, 1, p).unwrap().0
}

/// Offset along one axis for anchor factor 0/1/2 (start/center/end) given `diff = outer - inner`.
fn off(factor: u32, diff: i64) -> i64 {
    match factor {
        0 => 0,
        1 => diff / 2,
        _ => diff,
    }
}

#[test]
fn defaults() {
    let p = ResolutionParams::default();
    assert_eq!(p.target, ResolutionTarget::MultipleOf4);
    assert_eq!(p.n, 8);
    assert_eq!(p.round, RoundMode::Nearest);
    assert_eq!(p.method, ResolutionMethod::Resample);
    assert_eq!(p.anchor, Anchor::Center);
    assert!(!p.keep_aspect);
    assert!(p.allow_non_square);
    assert_eq!(p.max_size, 8192);
    assert_eq!(p.pad_color, Color([0, 0, 0, 0]));
}

#[test]
fn multiple_of_4_table() {
    // (n, nearest, up, down)
    for (n, near, up, down) in [
        (1, 4, 4, 4),
        (3, 4, 4, 4),
        (4, 4, 4, 4),
        (6, 8, 8, 4),
        (1023, 1024, 1024, 1020),
        (1025, 1024, 1028, 1024),
        (4097, 4096, 4100, 4096),
    ] {
        assert_eq!(
            width_for(
                n,
                &params(ResolutionTarget::MultipleOf4, RoundMode::Nearest)
            ),
            near,
            "{n} nearest"
        );
        assert_eq!(
            width_for(n, &params(ResolutionTarget::MultipleOf4, RoundMode::Up)),
            up,
            "{n} up"
        );
        assert_eq!(
            width_for(n, &params(ResolutionTarget::MultipleOf4, RoundMode::Down)),
            down,
            "{n} down"
        );
    }
}

#[test]
fn multiple_of_n_table() {
    for (n, near, up, down) in [
        (1, 10, 10, 10),
        (3, 10, 10, 10),
        (15, 20, 20, 10),
        (1023, 1020, 1030, 1020),
        (1025, 1030, 1030, 1020),
        (4097, 4100, 4100, 4090),
    ] {
        for (round, e) in [
            (RoundMode::Nearest, near),
            (RoundMode::Up, up),
            (RoundMode::Down, down),
        ] {
            let p = ResolutionParams {
                n: 10,
                ..params(ResolutionTarget::MultipleOfN, round)
            };
            assert_eq!(width_for(n, &p), e, "{n} {round:?}");
        }
    }
}

#[test]
fn pot_table() {
    for (n, near, up, down) in [
        (1, 1, 1, 1),
        (3, 4, 4, 2),
        (4, 4, 4, 4),
        (1023, 1024, 1024, 512),
        (1025, 1024, 2048, 1024),
        (4097, 4096, 8192, 4096),
    ] {
        assert_eq!(
            width_for(n, &params(ResolutionTarget::Pot, RoundMode::Nearest)),
            near,
            "{n} nearest"
        );
        assert_eq!(
            width_for(n, &params(ResolutionTarget::Pot, RoundMode::Up)),
            up,
            "{n} up"
        );
        assert_eq!(
            width_for(n, &params(ResolutionTarget::Pot, RoundMode::Down)),
            down,
            "{n} down"
        );
    }
}

#[test]
fn apply_matches_target_size_on_real_images() {
    for (w, h) in [(1023, 3), (1025, 1), (3, 4097)] {
        let img = solid(w, h, RED);
        for target in [ResolutionTarget::MultipleOf4, ResolutionTarget::Pot] {
            for round in [RoundMode::Nearest, RoundMode::Up, RoundMode::Down] {
                let p = ResolutionParams {
                    filter: ResampleFilter::Bilinear,
                    ..params(target, round)
                };
                let expected = resolution::target_size(w, h, &p).unwrap();
                let out = resolution::apply(&img, &p).unwrap().image;
                assert_eq!(out.dimensions(), expected);
                assert!(out.pixels().all(|px| *px == RED));
            }
        }
    }
}

#[test]
fn pad_always_grows_and_crop_always_shrinks() {
    // `round` only drives the resample method; pad rounds up, crop rounds down.
    let pad = ResolutionParams {
        method: ResolutionMethod::Pad,
        ..params(ResolutionTarget::Pot, RoundMode::Down)
    };
    assert_eq!(resolution::target_size(1025, 3, &pad).unwrap(), (2048, 4));
    let crop = ResolutionParams {
        method: ResolutionMethod::Crop,
        ..params(ResolutionTarget::Pot, RoundMode::Up)
    };
    assert_eq!(resolution::target_size(1025, 3, &crop).unwrap(), (1024, 2));
}

fn marked(w: u32, h: u32) -> ImageBuf {
    let mut img = solid(w, h, RED);
    img.put_pixel(0, 0, GREEN);
    img.put_pixel(w - 1, h - 1, BLUE);
    img
}

#[test]
fn pad_places_content_by_anchor() {
    let img = marked(5, 6);
    for (anchor, fx, fy) in ANCHORS {
        let p = ResolutionParams {
            method: ResolutionMethod::Pad,
            anchor,
            pad_color: Color(PAD.0),
            ..params(ResolutionTarget::Pot, RoundMode::Nearest)
        };
        let out = resolution::apply(&img, &p).unwrap().image;
        assert_eq!(out.dimensions(), (8, 8));
        let (ox, oy) = (off(fx, 3) as u32, off(fy, 2) as u32);
        assert_eq!(*out.get_pixel(ox, oy), GREEN, "{anchor:?}");
        assert_eq!(*out.get_pixel(ox + 4, oy + 5), BLUE, "{anchor:?}");
        assert_eq!(*out.get_pixel(ox + 1, oy + 1), RED, "{anchor:?}");
        assert_eq!(
            out.pixels().filter(|px| **px == PAD).count(),
            64 - 30,
            "{anchor:?}"
        );
        for (x, y, px) in out.enumerate_pixels() {
            let inside = (ox..ox + 5).contains(&x) && (oy..oy + 6).contains(&y);
            assert_eq!(*px == PAD, !inside, "{anchor:?} at {x},{y}");
        }
    }
}

#[test]
fn crop_selects_region_by_anchor() {
    let img = ImageBuf::from_fn(10, 11, |x, y| Rgba([x as u8 * 20, y as u8 * 20, 0, 255]));
    for (anchor, fx, fy) in ANCHORS {
        let p = ResolutionParams {
            method: ResolutionMethod::Crop,
            anchor,
            ..params(ResolutionTarget::MultipleOf4, RoundMode::Up)
        };
        let out = resolution::apply(&img, &p).unwrap().image;
        assert_eq!(out.dimensions(), (8, 8));
        let (sx, sy) = (-off(fx, -2) as u32, -off(fy, -3) as u32);
        for (x, y, px) in out.enumerate_pixels() {
            assert_eq!(px, img.get_pixel(sx + x, sy + y), "{anchor:?} at {x},{y}");
        }
    }
}

#[test]
fn resample_keep_aspect_places_content_by_anchor() {
    // 2x8 / 8x2 -> square POT 8x8: scale factor 1, the content is padded per anchor.
    for (anchor, fx, fy) in ANCHORS {
        let p = ResolutionParams {
            keep_aspect: true,
            allow_non_square: false,
            anchor,
            pad_color: Color(PAD.0),
            ..params(ResolutionTarget::Pot, RoundMode::Nearest)
        };
        let tall = resolution::apply(&marked(2, 8), &p).unwrap().image;
        assert_eq!(tall.dimensions(), (8, 8));
        let ox = off(fx, 6) as u32;
        assert_eq!(*tall.get_pixel(ox, 0), GREEN, "{anchor:?}");
        assert_eq!(*tall.get_pixel(ox + 1, 7), BLUE, "{anchor:?}");
        assert_eq!(tall.pixels().filter(|px| **px == PAD).count(), 48);

        let wide = resolution::apply(&marked(8, 2), &p).unwrap().image;
        let oy = off(fy, 6) as u32;
        assert_eq!(*wide.get_pixel(0, oy), GREEN, "{anchor:?}");
        assert_eq!(*wide.get_pixel(7, oy + 1), BLUE, "{anchor:?}");
        assert_eq!(wide.pixels().filter(|px| **px == PAD).count(), 48);
    }
}

#[test]
fn resample_keep_aspect_scales_then_pads() {
    // 3x2 -> multiple of 6 (up) = 6x6; uniform scale 2 => 6x4 content + 2 rows padding.
    let img = solid(3, 2, RED);
    let p = ResolutionParams {
        n: 6,
        keep_aspect: true,
        anchor: Anchor::Top,
        filter: ResampleFilter::Nearest,
        pad_color: Color(PAD.0),
        ..params(ResolutionTarget::MultipleOfN, RoundMode::Up)
    };
    let out = resolution::apply(&img, &p).unwrap().image;
    assert_eq!(out.dimensions(), (6, 6));
    for (_, y, px) in out.enumerate_pixels() {
        assert_eq!(*px, if y < 4 { RED } else { PAD });
    }
}

#[test]
fn resample_stretch_ignores_anchor() {
    let img = gradient(5, 3);
    for (anchor, _, _) in ANCHORS {
        let p = ResolutionParams {
            anchor,
            ..params(ResolutionTarget::Pot, RoundMode::Nearest)
        };
        let out = resolution::apply(&img, &p).unwrap().image;
        assert_eq!(out.dimensions(), (4, 4));
        assert!(out.pixels().all(|px| px[3] == 255));
    }
}

#[test]
fn already_valid_is_byte_identical() {
    let img = gradient(64, 32);
    for target in [
        ResolutionTarget::MultipleOf4,
        ResolutionTarget::MultipleOfN,
        ResolutionTarget::Pot,
    ] {
        for method in [
            ResolutionMethod::Resample,
            ResolutionMethod::Pad,
            ResolutionMethod::Crop,
        ] {
            for round in [RoundMode::Nearest, RoundMode::Up, RoundMode::Down] {
                let p = ResolutionParams {
                    target,
                    method,
                    round,
                    keep_aspect: true,
                    ..Default::default()
                };
                let out = resolution::apply(&img, &p).unwrap().image;
                assert_eq!(
                    out.as_raw(),
                    img.as_raw(),
                    "{target:?} {method:?} {round:?}"
                );
            }
        }
    }
    let odd = gradient(12, 20);
    let out = resolution::apply(&odd, &ResolutionParams::default())
        .unwrap()
        .image;
    assert_eq!(out.as_raw(), odd.as_raw());
}

#[test]
fn max_size_clamps_result() {
    let p = ResolutionParams {
        max_size: 4096,
        ..params(ResolutionTarget::Pot, RoundMode::Up)
    };
    assert_eq!(resolution::target_size(5000, 3, &p).unwrap(), (4096, 4));
    let p = ResolutionParams {
        max_size: 1002,
        ..params(ResolutionTarget::MultipleOf4, RoundMode::Up)
    };
    assert_eq!(
        resolution::target_size(1500, 1001, &p).unwrap(),
        (1000, 1000)
    );
    let p = ResolutionParams {
        max_size: 0,
        ..params(ResolutionTarget::Pot, RoundMode::Up)
    };
    assert_eq!(
        resolution::target_size(9000, 3, &p).unwrap(),
        (16384, 4),
        "0 = unlimited"
    );

    let img = gradient(100, 10);
    let p = ResolutionParams {
        max_size: 64,
        ..params(ResolutionTarget::Pot, RoundMode::Nearest)
    };
    let out = resolution::apply(&img, &p).unwrap().image;
    assert_eq!(out.dimensions(), (64, 8));
    assert!(out.width() <= 64 && out.height() <= 64);

    let p = ResolutionParams {
        n: 10,
        max_size: 5,
        ..params(ResolutionTarget::MultipleOfN, RoundMode::Nearest)
    };
    let err = resolution::apply(&img, &p).unwrap_err();
    assert_eq!(err.code, codes::INVALID_PARAMS);
    assert_eq!(err.params["param"], json!("maxSize"));
}

#[test]
fn non_square_vs_square_pot() {
    let p = params(ResolutionTarget::Pot, RoundMode::Nearest);
    assert_eq!(resolution::target_size(100, 30, &p).unwrap(), (128, 32));
    let sq = ResolutionParams {
        allow_non_square: false,
        ..p.clone()
    };
    assert_eq!(resolution::target_size(100, 30, &sq).unwrap(), (128, 128));
    assert_eq!(resolution::target_size(64, 64, &sq).unwrap(), (64, 64));
    let out = resolution::apply(&gradient(100, 30), &sq).unwrap().image;
    assert_eq!(out.dimensions(), (128, 128));
    let sq_capped = ResolutionParams { max_size: 64, ..sq };
    assert_eq!(
        resolution::target_size(100, 30, &sq_capped).unwrap(),
        (64, 64)
    );
}

#[test]
fn invalid_n_and_empty_image() {
    let p = ResolutionParams {
        n: 0,
        ..params(ResolutionTarget::MultipleOfN, RoundMode::Nearest)
    };
    let err = resolution::apply(&gradient(4, 4), &p).unwrap_err();
    assert_eq!(err.code, codes::INVALID_PARAMS);
    assert_eq!(err.params["param"], json!("n"));
    let err = resolution::apply(&ImageBuf::new(0, 3), &ResolutionParams::default()).unwrap_err();
    assert_eq!(err.code, codes::IMG_EMPTY);
}

#[test]
fn serde_round_trip_and_partial_json() {
    let p = ResolutionParams {
        target: ResolutionTarget::MultipleOfN,
        n: 12,
        round: RoundMode::Down,
        method: ResolutionMethod::Crop,
        anchor: Anchor::BottomLeft,
        filter: ResampleFilter::CatmullRom,
        keep_aspect: true,
        allow_non_square: false,
        max_size: 2048,
        pad_color: Color([1, 2, 3, 4]),
    };
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(
        v,
        json!({
            "target": "multipleOfN", "n": 12, "round": "down", "method": "crop", "anchor": "bottomLeft",
            "filter": "catmullRom", "keepAspect": true, "allowNonSquare": false, "maxSize": 2048,
            "padColor": [1, 2, 3, 4]
        })
    );
    assert_eq!(serde_json::from_value::<ResolutionParams>(v).unwrap(), p);
    assert_eq!(
        serde_json::from_str::<ResolutionParams>("{}").unwrap(),
        ResolutionParams::default()
    );
    assert_eq!(
        serde_json::to_value(ResolutionTarget::MultipleOf4).unwrap(),
        json!("multipleOf4")
    );
    assert_eq!(
        serde_json::to_value(ResolutionTarget::Pot).unwrap(),
        json!("pot")
    );
    assert_eq!(
        serde_json::to_value(ResolutionMethod::Pad).unwrap(),
        json!("pad")
    );
    assert_eq!(
        serde_json::to_value(ResolutionMethod::Resample).unwrap(),
        json!("resample")
    );
}
