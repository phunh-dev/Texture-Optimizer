//! Tests for shared helpers in `ops::common` (size math, anchors, canvas, resample).

use image::Rgba;
use proptest::prelude::*;
use serde_json::json;
use texopt_core::ImageBuf;
use texopt_core::fixtures::*;
use texopt_core::ops::common::*;

#[test]
fn pot_predicates_and_tables() {
    for n in [1u32, 2, 4, 8, 1024, 1 << 31] {
        assert!(is_pot(n), "{n} should be POT");
    }
    for n in [0u32, 3, 6, 1023, 1025] {
        assert!(!is_pot(n), "{n} should not be POT");
    }
    for (n, e) in [
        (0, 1),
        (1, 1),
        (2, 2),
        (3, 4),
        (1023, 1024),
        (1024, 1024),
        (1025, 2048),
        (4097, 8192),
    ] {
        assert_eq!(next_pot(n), e, "next_pot({n})");
    }
    for (n, e) in [
        (0, 1),
        (1, 1),
        (2, 2),
        (3, 2),
        (1023, 512),
        (1024, 1024),
        (1025, 1024),
        (4097, 4096),
    ] {
        assert_eq!(prev_pot(n), e, "prev_pot({n})");
    }
    // Nearest: ties round up.
    for (n, e) in [
        (1, 1),
        (3, 4),
        (5, 4),
        (6, 8),
        (1023, 1024),
        (1025, 1024),
        (1536, 2048),
        (4097, 4096),
    ] {
        assert_eq!(
            round_pot(n, RoundMode::Nearest),
            e,
            "round_pot({n}, nearest)"
        );
    }
    assert_eq!(round_pot(1025, RoundMode::Up), 2048);
    assert_eq!(round_pot(1025, RoundMode::Down), 1024);
    assert_eq!(round_pot(1, RoundMode::Down), 1);
}

#[test]
fn round_to_multiple_tables() {
    use RoundMode::*;
    let cases = [
        // (n, m, nearest, up, down)
        (0, 4, 4, 4, 4),
        (1, 4, 4, 4, 4),
        (3, 4, 4, 4, 4),
        (4, 4, 4, 4, 4),
        (6, 4, 8, 8, 4),
        (1023, 4, 1024, 1024, 1020),
        (1025, 4, 1024, 1028, 1024),
        (4097, 4, 4096, 4100, 4096),
        (15, 10, 20, 20, 10),
        (1023, 10, 1020, 1030, 1020),
    ];
    for (n, m, near, up, down) in cases {
        assert_eq!(round_to_multiple(n, m, Nearest), near, "nearest {n}/{m}");
        assert_eq!(round_to_multiple(n, m, Up), up, "up {n}/{m}");
        assert_eq!(round_to_multiple(n, m, Down), down, "down {n}/{m}");
    }
}

#[test]
fn snap_size_modes() {
    assert_eq!(
        snap_size(50, 25, SnapMode::None, RoundMode::Nearest),
        (50, 25)
    );
    assert_eq!(
        snap_size(50, 25, SnapMode::MultipleOf4, RoundMode::Nearest),
        (52, 24)
    );
    assert_eq!(
        snap_size(50, 25, SnapMode::MultipleOf4, RoundMode::Up),
        (52, 28)
    );
    assert_eq!(
        snap_size(50, 25, SnapMode::Pot, RoundMode::Nearest),
        (64, 32)
    );
    assert_eq!(snap_size(50, 25, SnapMode::Pot, RoundMode::Up), (64, 32));
    assert_eq!(
        snap_size(50, 17, SnapMode::Pot, RoundMode::Nearest),
        (64, 16)
    );
}

#[test]
fn anchor_offsets_pad_and_crop() {
    use Anchor::*;
    let expect = [
        (TopLeft, (0, 0)),
        (Top, (3, 0)),
        (TopRight, (6, 0)),
        (Left, (0, 3)),
        (Center, (3, 3)),
        (Right, (6, 3)),
        (BottomLeft, (0, 6)),
        (Bottom, (3, 6)),
        (BottomRight, (6, 6)),
    ];
    for (a, (x, y)) in expect {
        assert_eq!(anchor_offset(a, 10, 10, 4, 4), (x, y), "pad {a:?}");
        // Cropping is the mirror image: inner bigger than outer => negative offsets.
        assert_eq!(anchor_offset(a, 4, 4, 10, 10), (-x, -y), "crop {a:?}");
    }
}

#[test]
fn place_with_color_and_edge_extend() {
    let src = ImageBuf::from_fn(2, 2, |x, y| Rgba([x as u8 * 100, y as u8 * 100, 7, 255]));
    let fill = Color([1, 2, 3, 4]);
    let out = place(&src, 4, 4, 1, 1, CanvasFill::Color(fill));
    assert_eq!(out.dimensions(), (4, 4));
    assert_eq!(*out.get_pixel(0, 0), Rgba([1, 2, 3, 4]));
    assert_eq!(*out.get_pixel(3, 3), Rgba([1, 2, 3, 4]));
    assert_eq!(out.get_pixel(1, 1), src.get_pixel(0, 0));
    assert_eq!(out.get_pixel(2, 2), src.get_pixel(1, 1));

    let ext = place(&src, 4, 4, 1, 1, CanvasFill::EdgeExtend);
    assert_eq!(ext.get_pixel(0, 0), src.get_pixel(0, 0));
    assert_eq!(ext.get_pixel(3, 0), src.get_pixel(1, 0));
    assert_eq!(ext.get_pixel(0, 3), src.get_pixel(0, 1));
    assert_eq!(ext.get_pixel(3, 3), src.get_pixel(1, 1));

    // Negative offset crops.
    let crop = place(&src, 1, 1, -1, -1, CanvasFill::Color(fill));
    assert_eq!(crop.get_pixel(0, 0), src.get_pixel(1, 1));
}

#[test]
fn resample_same_size_is_identity_and_constant_stays_constant() {
    let img = gradient(9, 7);
    for filter in ALL_FILTERS {
        let opts = ResampleOptions {
            filter,
            ..Default::default()
        };
        assert_eq!(resample(&img, 9, 7, &opts), img);
        let solid_img = solid(5, 3, Rgba([10, 200, 30, 255]));
        for linear_space in [false, true] {
            let opts = ResampleOptions {
                filter,
                linear_space,
                premultiply_alpha: true,
            };
            let out = resample(&solid_img, 11, 2, &opts);
            assert_eq!(out.dimensions(), (11, 2));
            for p in out.pixels() {
                assert_eq!(
                    *p,
                    Rgba([10, 200, 30, 255]),
                    "{filter:?} linear={linear_space}"
                );
            }
        }
    }
}

#[test]
fn srgb_linear_round_trip() {
    for v in 0..=255u8 {
        let f = v as f32 / 255.0;
        let back = linear_to_srgb(srgb_to_linear(f));
        assert_eq!((back * 255.0).round() as u8, v);
    }
}

#[test]
fn shared_enums_serialize_to_stable_values() {
    assert_eq!(
        serde_json::to_value(Color([1, 2, 3, 4])).unwrap(),
        json!([1, 2, 3, 4])
    );
    assert_eq!(
        serde_json::to_value(Anchor::TopLeft).unwrap(),
        json!("topLeft")
    );
    assert_eq!(
        serde_json::to_value(Anchor::BottomRight).unwrap(),
        json!("bottomRight")
    );
    assert_eq!(
        serde_json::to_value(ResampleFilter::CatmullRom).unwrap(),
        json!("catmullRom")
    );
    assert_eq!(
        serde_json::to_value(ResampleFilter::Lanczos3).unwrap(),
        json!("lanczos3")
    );
    assert_eq!(
        serde_json::to_value(SnapMode::MultipleOf4).unwrap(),
        json!("multipleOf4")
    );
    assert_eq!(serde_json::to_value(SnapMode::Pot).unwrap(), json!("pot"));
    assert_eq!(serde_json::to_value(RoundMode::Up).unwrap(), json!("up"));
}

proptest! {
    #[test]
    fn next_pot_is_smallest_pot_at_least_n(n in 1u32..=(1 << 30)) {
        let p = next_pot(n);
        prop_assert!(is_pot(p));
        prop_assert!(p >= n);
        prop_assert!(p == 1 || p / 2 < n);
    }

    #[test]
    fn prev_pot_is_largest_pot_at_most_n(n in 1u32..=u32::MAX) {
        let p = prev_pot(n);
        prop_assert!(is_pot(p));
        prop_assert!(p <= n);
        prop_assert!((p as u64) * 2 > n as u64);
    }

    #[test]
    fn round_pot_nearest_is_closest(n in 1u32..=(1 << 30)) {
        let r = round_pot(n, RoundMode::Nearest) as i64;
        let n = n as i64;
        let lo = prev_pot(n as u32) as i64;
        let hi = next_pot(n as u32) as i64;
        prop_assert!(r == lo || r == hi);
        prop_assert!((r - n).abs() <= (lo - n).abs());
        prop_assert!((r - n).abs() <= (hi - n).abs());
    }

    #[test]
    fn round_to_multiple_invariants(n in 0u32..10_000_000, m in 1u32..2_000) {
        let (n64, m64) = (n as i64, m as i64);
        for mode in [RoundMode::Nearest, RoundMode::Up, RoundMode::Down] {
            let r = round_to_multiple(n, m, mode) as i64;
            prop_assert_eq!(r % m64, 0);
            prop_assert!(r >= m64, "never below m (never 0)");
            if n64 >= m64 {
                match mode {
                    RoundMode::Up => prop_assert!(r >= n64 && r - n64 < m64),
                    RoundMode::Down => prop_assert!(r <= n64 && n64 - r < m64),
                    RoundMode::Nearest => prop_assert!((r - n64).abs() * 2 <= m64),
                }
            } else {
                prop_assert_eq!(r, m64);
            }
        }
    }
}
