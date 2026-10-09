mod atlas_support;

use atlas_support::*;
use image::Rgba;
use texopt_core::ImageBuf;
use texopt_core::atlas::{AtlasParams, IncrementalMode, SizeMode, build};
use texopt_core::fixtures;

fn base() -> AtlasParams {
    AtlasParams {
        padding: 0,
        trim: false,
        dedupe: false,
        ..AtlasParams::default()
    }
}

#[test]
fn frames_contain_the_source_pixels() {
    let inputs: Vec<(String, ImageBuf)> = vec![
        ("a".into(), pattern(13, 7, 1)),
        ("b".into(), pattern(5, 9, 2)),
        ("c".into(), pattern(20, 3, 3)),
    ];
    let params = AtlasParams {
        padding: 3,
        border: 2,
        ..base()
    };
    let res = build(
        to_inputs(&inputs),
        &params,
        None,
        IncrementalMode::KeepPositions,
    )
    .unwrap();
    check_invariants(&res, &inputs, &params);
    // Explicit spot check of one pixel.
    let s = res.project.sprite("a").unwrap();
    assert_eq!(
        res.pages[0].get_pixel(s.frame.x + 12, s.frame.y + 6),
        inputs[0].1.get_pixel(12, 6)
    );
}

#[test]
fn rotated_sprite_is_stored_clockwise() {
    // A 10x40 sprite only fits a 64x16 page when rotated.
    let src = pattern(10, 40, 5);
    let inputs = vec![("r".to_string(), src.clone())];
    let params = AtlasParams {
        allow_rotation: true,
        size_mode: SizeMode::Fixed,
        max_width: 64,
        max_height: 16,
        ..base()
    };
    let res = build(
        to_inputs(&inputs),
        &params,
        None,
        IncrementalMode::KeepPositions,
    )
    .unwrap();
    let s = &res.project.sprites[0];
    assert!(s.rotated);
    assert_eq!((s.frame.w, s.frame.h), (40, 10));
    let page = &res.pages[0];
    // Clockwise: source top-left lands at the frame's top-right,
    // source bottom-left at the frame's top-left.
    assert_eq!(
        page.get_pixel(s.frame.x + 39, s.frame.y),
        src.get_pixel(0, 0)
    );
    assert_eq!(page.get_pixel(s.frame.x, s.frame.y), src.get_pixel(0, 39));
    assert_eq!(
        page.get_pixel(s.frame.x, s.frame.y + 9),
        src.get_pixel(9, 39)
    );
    check_invariants(&res, &inputs, &params);
}

#[test]
fn extrude_replicates_edge_pixels() {
    let inputs = vec![
        ("e".to_string(), pattern(6, 4, 7)),
        ("f".to_string(), pattern(3, 3, 8)),
    ];
    let params = AtlasParams {
        extrude: 2,
        padding: 1,
        border: 1,
        ..base()
    };
    let res = build(
        to_inputs(&inputs),
        &params,
        None,
        IncrementalMode::KeepPositions,
    )
    .unwrap();
    check_invariants(&res, &inputs, &params);
    let s = res.project.sprite("e").unwrap();
    let page = &res.pages[0];
    let (fx, fy) = (s.frame.x, s.frame.y);
    // left of the first column, above the first row, corner and bottom-right
    assert_eq!(page.get_pixel(fx - 1, fy + 2), inputs[0].1.get_pixel(0, 2));
    assert_eq!(page.get_pixel(fx - 2, fy + 2), inputs[0].1.get_pixel(0, 2));
    assert_eq!(page.get_pixel(fx + 3, fy - 2), inputs[0].1.get_pixel(3, 0));
    assert_eq!(page.get_pixel(fx - 2, fy - 2), inputs[0].1.get_pixel(0, 0));
    assert_eq!(page.get_pixel(fx + 7, fy + 5), inputs[0].1.get_pixel(5, 3));
    // the border strip stays transparent
    for x in 0..page.width() {
        assert_eq!(page.get_pixel(x, 0)[3], 0);
    }
}

#[test]
fn extrude_of_rotated_sprite_uses_rotated_edges() {
    let src = pattern(4, 30, 3);
    let inputs = vec![("r".to_string(), src)];
    let params = AtlasParams {
        allow_rotation: true,
        extrude: 1,
        size_mode: SizeMode::Fixed,
        max_width: 64,
        max_height: 8,
        ..base()
    };
    let res = build(
        to_inputs(&inputs),
        &params,
        None,
        IncrementalMode::KeepPositions,
    )
    .unwrap();
    assert!(res.project.sprites[0].rotated);
    check_invariants(&res, &inputs, &params);
}

#[test]
fn trimmed_frame_holds_the_trimmed_pixels() {
    let mut img = fixtures::sprite(16, 16, 4, 5, 6, 3, fixtures::GREEN);
    img.put_pixel(4, 5, fixtures::BLUE);
    let inputs = vec![("t".to_string(), img)];
    let params = AtlasParams {
        trim: true,
        extrude: 1,
        ..base()
    };
    let res = build(
        to_inputs(&inputs),
        &params,
        None,
        IncrementalMode::KeepPositions,
    )
    .unwrap();
    let s = &res.project.sprites[0];
    assert_eq!(
        res.pages[0].get_pixel(s.frame.x, s.frame.y),
        &fixtures::BLUE
    );
    assert_eq!(
        res.pages[0].get_pixel(s.frame.x + 5, s.frame.y + 2),
        &fixtures::GREEN
    );
    check_invariants(&res, &inputs, &params);
}

#[test]
fn premultiply_alpha_scales_colors() {
    let img = fixtures::solid(2, 2, Rgba([200, 100, 50, 128]));
    let inputs = vec![("p".to_string(), img)];
    let params = AtlasParams {
        premultiply_alpha: true,
        ..base()
    };
    let res = build(
        to_inputs(&inputs),
        &params,
        None,
        IncrementalMode::KeepPositions,
    )
    .unwrap();
    let s = &res.project.sprites[0];
    assert_eq!(
        res.pages[0].get_pixel(s.frame.x, s.frame.y),
        &Rgba([100, 50, 25, 128])
    );
    let res = build(
        to_inputs(&inputs),
        &base(),
        None,
        IncrementalMode::KeepPositions,
    )
    .unwrap();
    assert_eq!(res.pages[0].get_pixel(0, 0), &Rgba([200, 100, 50, 128]));
}

#[test]
fn background_outside_sprites_is_transparent() {
    let inputs = vec![
        ("a".to_string(), pattern(10, 10, 1)),
        ("b".to_string(), pattern(3, 17, 2)),
    ];
    let params = AtlasParams {
        padding: 2,
        ..base()
    };
    let res = build(
        to_inputs(&inputs),
        &params,
        None,
        IncrementalMode::KeepPositions,
    )
    .unwrap();
    let page = &res.pages[0];
    let covered: u32 = res
        .project
        .sprites
        .iter()
        .map(|s| s.frame.w * s.frame.h)
        .sum();
    let opaque = page.pixels().filter(|p| p[3] != 0).count() as u32;
    assert_eq!(opaque, covered);
}
