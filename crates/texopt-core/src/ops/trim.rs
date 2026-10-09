//! Sprite/Texture Trimmer: crop away empty (transparent) borders.
//!
//! - A pixel is empty when `alpha <= alphaThreshold`.
//! - Only enabled sides (`trimLeft/Right/Top/Bottom`) are trimmed.
//! - `margin` keeps up to that many source pixels around the content on
//!   trimmed sides (clamped to the source bounds).
//! - `snap` grows the trimmed rect (rounding up) to a multiple of 4 / POT,
//!   centered on the content. Source pixels under the grown rect are kept;
//!   area outside the source is transparent. So `trimRect` may extend past the
//!   source (negative `x`/`y`, or beyond `sourceSize`).
//! - No non-empty pixel → `TRIM_EMPTY` error, or a 1x1 transparent image when
//!   `emptyBehavior = onePixel`.
//!
//! Output `meta` ([`TrimMeta`]): `{ "sourceSize": {w,h}, "trimRect": {x,y,w,h} }`.
//! Pasting the output at (`x`, `y`) on a transparent `sourceSize` canvas
//! reconstructs the source (exactly, when trimmed pixels were `[0,0,0,0]`).

use serde::{Deserialize, Serialize};

use super::OpOutput;
use super::common::{
    Anchor, CanvasFill, Color, RoundMode, SnapMode, anchor_offset, ensure_non_empty, place,
    snap_size,
};
use crate::{ImageBuf, OpError, OpResult};

/// The image has no pixel above the alpha threshold. Params: `width`, `height`.
pub const TRIM_EMPTY: &str = "TRIM_EMPTY";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TrimEmptyBehavior {
    #[default]
    Error,
    OnePixel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TrimParams {
    /// Pixels with `alpha <= alphaThreshold` count as empty.
    pub alpha_threshold: u8,
    pub margin: u32,
    pub trim_left: bool,
    pub trim_right: bool,
    pub trim_top: bool,
    pub trim_bottom: bool,
    pub snap: SnapMode,
    pub empty_behavior: TrimEmptyBehavior,
}

impl Default for TrimParams {
    fn default() -> Self {
        Self {
            alpha_threshold: 0,
            margin: 0,
            trim_left: true,
            trim_right: true,
            trim_top: true,
            trim_bottom: true,
            snap: SnapMode::None,
            empty_behavior: TrimEmptyBehavior::Error,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrimSize {
    pub w: u32,
    pub h: u32,
}

/// Where the output's top-left sits in source coordinates, and its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrimRect {
    pub x: i64,
    pub y: i64,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrimMeta {
    pub source_size: TrimSize,
    pub trim_rect: TrimRect,
}

/// Bounds `(x0, y0, x1, y1)` (exclusive end) of pixels with alpha > threshold.
fn content_bounds(img: &ImageBuf, threshold: u8) -> Option<(u32, u32, u32, u32)> {
    let (w, h) = img.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for (y, row) in img.as_raw().chunks_exact(w as usize * 4).enumerate() {
        let (pixels, _) = row.as_chunks::<4>();
        let solid = |p: &[u8; 4]| p[3] > threshold;
        if let (Some(first), Some(last)) = (
            pixels.iter().position(solid),
            pixels.iter().rposition(solid),
        ) {
            x0 = x0.min(first as u32);
            x1 = x1.max(last as u32 + 1);
            y0 = y0.min(y as u32);
            y1 = y as u32 + 1;
        }
    }
    (x1 > x0).then_some((x0, y0, x1, y1))
}

pub fn apply(img: &ImageBuf, params: &TrimParams) -> OpResult<OpOutput> {
    ensure_non_empty(img)?;
    let (w, h) = img.dimensions();
    let source_size = TrimSize { w, h };
    let Some((cx0, cy0, cx1, cy1)) = content_bounds(img, params.alpha_threshold) else {
        return match params.empty_behavior {
            TrimEmptyBehavior::Error => {
                Err(OpError::new(TRIM_EMPTY).with("width", w).with("height", h))
            }
            TrimEmptyBehavior::OnePixel => {
                let meta = TrimMeta {
                    source_size,
                    trim_rect: TrimRect {
                        x: 0,
                        y: 0,
                        w: 1,
                        h: 1,
                    },
                };
                Ok(output(ImageBuf::new(1, 1), meta))
            }
        };
    };
    let m = params.margin;
    let x0 = if params.trim_left {
        cx0.saturating_sub(m)
    } else {
        0
    };
    let y0 = if params.trim_top {
        cy0.saturating_sub(m)
    } else {
        0
    };
    let x1 = if params.trim_right {
        cx1.saturating_add(m).min(w)
    } else {
        w
    };
    let y1 = if params.trim_bottom {
        cy1.saturating_add(m).min(h)
    } else {
        h
    };
    let (cw, ch) = (x1 - x0, y1 - y0);
    let (ow, oh) = snap_size(cw, ch, params.snap, RoundMode::Up);
    let (dx, dy) = anchor_offset(Anchor::Center, ow, oh, cw, ch);
    let (rx, ry) = (x0 as i64 - dx, y0 as i64 - dy);
    let image = if (rx, ry, ow, oh) == (0, 0, w, h) {
        img.clone()
    } else {
        place(img, ow, oh, -rx, -ry, CanvasFill::Color(Color::TRANSPARENT))
    };
    let meta = TrimMeta {
        source_size,
        trim_rect: TrimRect {
            x: rx,
            y: ry,
            w: ow,
            h: oh,
        },
    };
    Ok(output(image, meta))
}

fn output(image: ImageBuf, meta: TrimMeta) -> OpOutput {
    OpOutput {
        image,
        meta: Some(serde_json::to_value(meta).expect("trim meta serializes")),
    }
}
