//! Shared helpers for the atlas integration tests.
#![allow(dead_code)]

use image::Rgba;
use texopt_core::ImageBuf;
use texopt_core::atlas::{AtlasParams, AtlasResult, SizeMode, SpriteInput};

/// Opaque image whose every pixel differs (detects flips/rotations/offsets).
pub fn pattern(w: u32, h: u32, seed: u8) -> ImageBuf {
    ImageBuf::from_fn(w, h, |x, y| {
        Rgba([
            (x as u8).wrapping_mul(7).wrapping_add(seed),
            (y as u8)
                .wrapping_mul(13)
                .wrapping_add(seed.wrapping_mul(3)),
            seed.wrapping_mul(31).wrapping_add((x + y) as u8),
            255,
        ])
    })
}

/// `pattern` content of `w x h` surrounded by `margin` transparent pixels.
pub fn padded(w: u32, h: u32, margin: u32, seed: u8) -> ImageBuf {
    let inner = pattern(w, h, seed);
    let mut img = ImageBuf::new(w + 2 * margin, h + 2 * margin);
    image::imageops::replace(&mut img, &inner, i64::from(margin), i64::from(margin));
    img
}

pub fn input(name: &str, img: ImageBuf) -> SpriteInput {
    SpriteInput::new(name, img)
}

/// Verify every packing invariant of `res` against the inputs it was built from.
pub fn check_invariants(res: &AtlasResult, inputs: &[(String, ImageBuf)], params: &AtlasParams) {
    let proj = &res.project;
    assert_eq!(res.pages.len(), proj.pages.len());
    let (mw, mh) = params.effective_max();
    for (img, info) in res.pages.iter().zip(&proj.pages) {
        assert_eq!((img.width(), img.height()), (info.width, info.height));
        assert!(
            info.width <= mw && info.height <= mh,
            "page {info:?} exceeds max {mw}x{mh}"
        );
        if params.force_pot {
            assert!(
                info.width.is_power_of_two() && info.height.is_power_of_two(),
                "page {info:?} not POT"
            );
        }
        if params.force_square {
            assert_eq!(info.width, info.height, "page not square");
        }
        if params.size_mode == SizeMode::Fixed {
            assert_eq!((info.width, info.height), (mw, mh));
        }
    }

    // Names: every input exactly once (primary or alias).
    let mut expected: Vec<String> = inputs.iter().map(|(n, _)| n.clone()).collect();
    expected.sort();
    assert_eq!(proj.all_names(), expected);
    if !params.dedupe {
        assert!(proj.sprites.iter().all(|s| s.aliases.is_empty()));
    }

    let e = params.extrude;
    for s in &proj.sprites {
        let page = proj.pages[s.page];
        let f = s.frame;
        let ss = s.sprite_source_size;
        // Rotation flag consistent with rect dims.
        if s.rotated {
            assert!(params.allow_rotation);
            assert_eq!((f.w, f.h), (ss.h, ss.w), "{}", s.name);
        } else {
            assert_eq!((f.w, f.h), (ss.w, ss.h), "{}", s.name);
        }
        assert!(ss.x + ss.w <= s.source_size.w && ss.y + ss.h <= s.source_size.h);
        assert_eq!(
            s.trimmed,
            (ss.w, ss.h) != (s.source_size.w, s.source_size.h)
        );
        if !params.trim {
            assert!(!s.trimmed);
        }
        // Inside the page minus border (extruded area included).
        assert!(
            f.x >= params.border + e && f.y >= params.border + e,
            "{} at {f:?}",
            s.name
        );
        assert!(
            f.x + f.w + e + params.border <= page.width,
            "{} right {f:?} page {page:?}",
            s.name
        );
        assert!(
            f.y + f.h + e + params.border <= page.height,
            "{} bottom {f:?} page {page:?}",
            s.name
        );
    }

    // No overlap; extruded areas at least `padding` apart.
    for (i, a) in proj.sprites.iter().enumerate() {
        for b in &proj.sprites[i + 1..] {
            if a.page != b.page {
                continue;
            }
            let (ax0, ay0) = (
                i64::from(a.frame.x) - i64::from(e),
                i64::from(a.frame.y) - i64::from(e),
            );
            let (ax1, ay1) = (
                i64::from(a.frame.x + a.frame.w + e),
                i64::from(a.frame.y + a.frame.h + e),
            );
            let (bx0, by0) = (
                i64::from(b.frame.x) - i64::from(e),
                i64::from(b.frame.y) - i64::from(e),
            );
            let (bx1, by1) = (
                i64::from(b.frame.x + b.frame.w + e),
                i64::from(b.frame.y + b.frame.h + e),
            );
            let gap_x = (bx0 - ax1).max(ax0 - bx1);
            let gap_y = (by0 - ay1).max(ay0 - by1);
            assert!(
                gap_x.max(gap_y) >= i64::from(params.padding),
                "{} {:?} and {} {:?} too close (padding {})",
                a.name,
                a.frame,
                b.name,
                b.frame,
                params.padding
            );
        }
    }

    // Pixels: frame == source (rotated clockwise when flagged); extrude == edge.
    if !params.premultiply_alpha {
        for s in &proj.sprites {
            let src = &inputs.iter().find(|(n, _)| *n == s.name).expect("input").1;
            let page = &res.pages[s.page];
            let (f, ss) = (s.frame, s.sprite_source_size);
            for v in 0..ss.h {
                for u in 0..ss.w {
                    let want = src.get_pixel(ss.x + u, ss.y + v);
                    let (ax, ay) = if s.rotated {
                        (f.x + ss.h - 1 - v, f.y + u)
                    } else {
                        (f.x + u, f.y + v)
                    };
                    assert_eq!(page.get_pixel(ax, ay), want, "{} pixel ({u},{v})", s.name);
                }
            }
            for y in f.y - e..f.y + f.h + e {
                for x in f.x - e..f.x + f.w + e {
                    let cx = x.clamp(f.x, f.x + f.w - 1);
                    let cy = y.clamp(f.y, f.y + f.h - 1);
                    assert_eq!(
                        page.get_pixel(x, y),
                        page.get_pixel(cx, cy),
                        "{} extrude at ({x},{y})",
                        s.name
                    );
                }
            }
            // Aliases really are identical images.
            for a in &s.aliases {
                let other = &inputs.iter().find(|(n, _)| n == a).expect("alias input").1;
                assert_eq!(other, src);
            }
        }
    }
}

pub fn to_inputs(v: &[(String, ImageBuf)]) -> Vec<SpriteInput> {
    v.iter()
        .map(|(n, i)| SpriteInput::new(n.clone(), i.clone()))
        .collect()
}
