//! Types and helpers shared by several operations.
//!
//! The serialized shape of the types here is part of the frontend contract:
//! add variants freely, but do not rename existing ones.
//!
//! Helpers:
//! - size math: [`is_pot`], [`next_pot`], [`prev_pot`], [`round_pot`],
//!   [`round_to_multiple`], [`snap_dimension`], [`snap_size`];
//! - canvas: [`anchor_offset`], [`place`], [`place_anchored`] (pad and/or crop
//!   with a fill color or edge replication);
//! - resampling: [`resample`] with [`ResampleOptions`] (linear light,
//!   premultiplied alpha), plus sRGB transfer functions;
//! - validation: [`ensure_non_empty`], [`check_max_dimension`].

mod resample;

pub use resample::{ResampleOptions, linear_to_srgb, resample, srgb_to_linear};

use image::Rgba;
use serde::{Deserialize, Serialize};

use crate::error::codes;
use crate::{ImageBuf, OpError, OpResult};

/// Hard cap for any output side, protecting against runaway allocations.
pub const MAX_DIMENSION: u32 = 32768;

/// 9-position anchor used when padding or cropping a canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    #[default]
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    /// Horizontal and vertical placement factors: 0 = start, 1 = center, 2 = end.
    fn factors(self) -> (u8, u8) {
        match self {
            Anchor::TopLeft => (0, 0),
            Anchor::Top => (1, 0),
            Anchor::TopRight => (2, 0),
            Anchor::Left => (0, 1),
            Anchor::Center => (1, 1),
            Anchor::Right => (2, 1),
            Anchor::BottomLeft => (0, 2),
            Anchor::Bottom => (1, 2),
            Anchor::BottomRight => (2, 2),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ResampleFilter {
    /// Pixel replication: never introduces new colors (pixel art).
    Nearest,
    /// Triangle filter (support 1).
    Bilinear,
    /// Cubic B=0, C=0.5 (sharp).
    CatmullRom,
    /// Cubic B=1/3, C=1/3 (balanced).
    Mitchell,
    /// Windowed sinc, 3 lobes (sharpest, may ring).
    #[default]
    Lanczos3,
}

/// Every filter, in UI order.
pub const ALL_FILTERS: [ResampleFilter; 5] = [
    ResampleFilter::Nearest,
    ResampleFilter::Bilinear,
    ResampleFilter::CatmullRom,
    ResampleFilter::Mitchell,
    ResampleFilter::Lanczos3,
];

/// Snap a dimension after an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SnapMode {
    #[default]
    None,
    MultipleOf4,
    Pot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum RoundMode {
    /// Closest valid value; ties round up.
    #[default]
    Nearest,
    Up,
    Down,
}

/// Straight-alpha RGBA color, serialized as `[r, g, b, a]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Color(pub [u8; 4]);

impl Color {
    pub const TRANSPARENT: Color = Color([0, 0, 0, 0]);
}

impl From<Color> for Rgba<u8> {
    fn from(c: Color) -> Self {
        Rgba(c.0)
    }
}

// ---------------------------------------------------------------------------
// Size math
// ---------------------------------------------------------------------------

/// True for 1, 2, 4, 8, ... (0 is not a power of two).
pub fn is_pot(n: u32) -> bool {
    n.is_power_of_two()
}

/// Smallest power of two `>= n` (`next_pot(0) == 1`); saturates at 2^31.
pub fn next_pot(n: u32) -> u32 {
    n.max(1).checked_next_power_of_two().unwrap_or(1 << 31)
}

/// Largest power of two `<= n` (`prev_pot(0) == 1`, never 0).
pub fn prev_pot(n: u32) -> u32 {
    if n <= 1 {
        1
    } else {
        1 << (31 - n.leading_zeros())
    }
}

/// Round `n` to a power of two. Ties (equal distance) round up; never returns 0.
pub fn round_pot(n: u32, mode: RoundMode) -> u32 {
    match mode {
        RoundMode::Up => next_pot(n),
        RoundMode::Down => prev_pot(n),
        RoundMode::Nearest => {
            let lo = prev_pot(n);
            let hi = next_pot(n);
            if lo >= n || hi <= lo {
                // n is already POT (or 0), or the upper bound saturated.
                lo
            } else if n - lo < hi - n {
                lo
            } else {
                hi
            }
        }
    }
}

/// Round `n` to a multiple of `m`. Ties round up. The result is never below
/// `m` (so never 0); `m == 0` is treated as 1.
pub fn round_to_multiple(n: u32, m: u32, mode: RoundMode) -> u32 {
    let m = m.max(1) as u64;
    let n = n as u64;
    let down = n / m * m;
    let up = if down == n { n } else { down + m };
    let r = match mode {
        RoundMode::Down => down,
        RoundMode::Up => up,
        RoundMode::Nearest => {
            if n - down < up - n {
                down
            } else {
                up
            }
        }
    };
    let max_multiple = u32::MAX as u64 / m * m;
    r.max(m).min(max_multiple) as u32
}

/// Snap one dimension: multiple of 4 or power of two, rounded with `round`.
pub fn snap_dimension(n: u32, snap: SnapMode, round: RoundMode) -> u32 {
    match snap {
        SnapMode::None => n.max(1),
        SnapMode::MultipleOf4 => round_to_multiple(n, 4, round),
        SnapMode::Pot => round_pot(n, round),
    }
}

/// [`snap_dimension`] applied to both sides.
pub fn snap_size(w: u32, h: u32, snap: SnapMode, round: RoundMode) -> (u32, u32) {
    (
        snap_dimension(w, snap, round),
        snap_dimension(h, snap, round),
    )
}

/// `round(n * factor)`, at least 1, saturating at `u32::MAX`.
pub fn scale_dimension(n: u32, factor: f64) -> u32 {
    let v = (n as f64 * factor).round();
    if v.is_nan() || v < 1.0 {
        1
    } else if v >= u32::MAX as f64 {
        u32::MAX
    } else {
        v as u32
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// `IMG_EMPTY` for zero-area input images.
pub fn ensure_non_empty(img: &ImageBuf) -> OpResult<()> {
    if img.width() == 0 || img.height() == 0 {
        return Err(OpError::new(codes::IMG_EMPTY)
            .with("width", img.width())
            .with("height", img.height()));
    }
    Ok(())
}

/// `IMG_TOO_LARGE` (`max`, `width`, `height`) when a side exceeds [`MAX_DIMENSION`].
pub fn check_max_dimension(w: u32, h: u32) -> OpResult<()> {
    if w > MAX_DIMENSION || h > MAX_DIMENSION {
        return Err(too_large(MAX_DIMENSION, w, h));
    }
    Ok(())
}

/// Build an `IMG_TOO_LARGE` error with `max`, `width`, `height` params.
pub fn too_large(max: u32, w: u32, h: u32) -> OpError {
    OpError::new(codes::IMG_TOO_LARGE)
        .with("max", max)
        .with("width", w)
        .with("height", h)
}

// ---------------------------------------------------------------------------
// Canvas
// ---------------------------------------------------------------------------

/// Position of an `inner` box inside an `outer` box for `anchor`.
///
/// Positive offsets mean padding (inner is smaller), negative offsets mean
/// cropping (inner is larger; `-offset` is the crop origin inside inner).
/// Centering truncates toward zero, i.e. an odd remainder biases the content
/// toward the top-left.
pub fn anchor_offset(
    anchor: Anchor,
    outer_w: u32,
    outer_h: u32,
    inner_w: u32,
    inner_h: u32,
) -> (i64, i64) {
    let (fx, fy) = anchor.factors();
    let axis = |factor: u8, outer: u32, inner: u32| {
        let diff = outer as i64 - inner as i64;
        match factor {
            0 => 0,
            1 => diff / 2,
            _ => diff,
        }
    };
    (axis(fx, outer_w, inner_w), axis(fy, outer_h, inner_h))
}

/// How [`place`] fills canvas pixels not covered by the source image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasFill {
    Color(Color),
    /// Replicate the nearest edge pixel of the source outward.
    EdgeExtend,
}

/// Draw `img` onto a new `w` x `h` canvas with its top-left corner at
/// (`off_x`, `off_y`). Parts of `img` outside the canvas are cropped, uncovered
/// canvas pixels use `fill`. This single primitive implements pad, crop and
/// "extract a rect that may extend past the source".
pub fn place(img: &ImageBuf, w: u32, h: u32, off_x: i64, off_y: i64, fill: CanvasFill) -> ImageBuf {
    let (sw, sh) = img.dimensions();
    match fill {
        CanvasFill::EdgeExtend if sw > 0 && sh > 0 => {
            let xs: Vec<u32> = (0..w as i64)
                .map(|x| (x - off_x).clamp(0, sw as i64 - 1) as u32)
                .collect();
            let ys: Vec<u32> = (0..h as i64)
                .map(|y| (y - off_y).clamp(0, sh as i64 - 1) as u32)
                .collect();
            ImageBuf::from_fn(w, h, |x, y| *img.get_pixel(xs[x as usize], ys[y as usize]))
        }
        CanvasFill::EdgeExtend | CanvasFill::Color(_) => {
            let color = match fill {
                CanvasFill::Color(c) => c,
                CanvasFill::EdgeExtend => Color::TRANSPARENT,
            };
            let mut out = ImageBuf::from_pixel(w, h, color.into());
            let x0 = off_x.max(0);
            let x1 = (off_x + sw as i64).min(w as i64);
            let y0 = off_y.max(0);
            let y1 = (off_y + sh as i64).min(h as i64);
            if x0 < x1 && y0 < y1 {
                let src = img.as_raw();
                let dst: &mut [u8] = &mut out;
                let len = (x1 - x0) as usize * 4;
                for y in y0..y1 {
                    let sy = (y - off_y) as usize;
                    let sx = (x0 - off_x) as usize;
                    let s = (sy * sw as usize + sx) * 4;
                    let d = (y as usize * w as usize + x0 as usize) * 4;
                    dst[d..d + len].copy_from_slice(&src[s..s + len]);
                }
            }
            out
        }
    }
}

/// [`place`] with the offset derived from `anchor` (pad and/or crop per axis).
pub fn place_anchored(
    img: &ImageBuf,
    w: u32,
    h: u32,
    anchor: Anchor,
    fill: CanvasFill,
) -> ImageBuf {
    let (ox, oy) = anchor_offset(anchor, w, h, img.width(), img.height());
    place(img, w, h, ox, oy, fill)
}
