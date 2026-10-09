//! POT Padding: grow the canvas to a power-of-two (or fixed) size without
//! resampling; the original pixels are copied 1:1 and placed by `anchor`.
//!
//! Size rules:
//! - `nextPot`: each side → next power of two of `max(side, minSize)`.
//! - `squarePot`: both sides → next power of two of `max(w, h, minSize)`.
//! - `fixed`: exactly `width` x `height` (need not be POT; `minSize` ignored).
//!   If the image is larger on either side → `IMG_TOO_LARGE`
//!   (`max` = the exceeded fixed side, `width`/`height` = image size).
//! - `maxSize` (0 = unlimited): if the required size exceeds it →
//!   `IMG_TOO_LARGE` (`max` = maxSize, `width`/`height` = required size).
//! - Already at the target size → returned unchanged.

use serde::{Deserialize, Serialize};

use super::OpOutput;
use super::common::{
    Anchor, CanvasFill, Color, check_max_dimension, ensure_non_empty, next_pot, place_anchored,
    too_large,
};
use crate::{ImageBuf, OpError, OpResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PotTarget {
    #[default]
    NextPot,
    SquarePot,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PadFill {
    #[default]
    Transparent,
    /// Fill with `color`.
    Color,
    /// Replicate the nearest edge pixel outward.
    EdgeExtend,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PotPadParams {
    pub target: PotTarget,
    /// Used by `fixed`.
    pub width: u32,
    /// Used by `fixed`.
    pub height: u32,
    pub anchor: Anchor,
    pub fill: PadFill,
    /// Used by `fill = color`.
    pub color: Color,
    /// Minimum side before rounding to POT; 0 = none. Ignored by `fixed`.
    pub min_size: u32,
    /// Largest allowed side; 0 = unlimited.
    pub max_size: u32,
}

impl Default for PotPadParams {
    fn default() -> Self {
        Self {
            target: PotTarget::NextPot,
            width: 1024,
            height: 1024,
            anchor: Anchor::Center,
            fill: PadFill::Transparent,
            color: Color([0, 0, 0, 255]),
            min_size: 0,
            max_size: 8192,
        }
    }
}

/// Output size for a `w` x `h` input. Errors if it cannot fit.
pub fn target_size(w: u32, h: u32, params: &PotPadParams) -> OpResult<(u32, u32)> {
    let min = params.min_size;
    let (tw, th) = match params.target {
        PotTarget::NextPot => (next_pot(w.max(min)), next_pot(h.max(min))),
        PotTarget::SquarePot => {
            let s = next_pot(w.max(h).max(min));
            (s, s)
        }
        PotTarget::Fixed => {
            if params.width == 0 {
                return Err(OpError::invalid_param("width", "must be > 0"));
            }
            if params.height == 0 {
                return Err(OpError::invalid_param("height", "must be > 0"));
            }
            if w > params.width || h > params.height {
                let max = if w > params.width {
                    params.width
                } else {
                    params.height
                };
                return Err(too_large(max, w, h));
            }
            (params.width, params.height)
        }
    };
    if params.max_size > 0 && (tw > params.max_size || th > params.max_size) {
        return Err(too_large(params.max_size, tw, th));
    }
    check_max_dimension(tw, th)?;
    Ok((tw, th))
}

pub fn apply(img: &ImageBuf, params: &PotPadParams) -> OpResult<OpOutput> {
    ensure_non_empty(img)?;
    let (tw, th) = target_size(img.width(), img.height(), params)?;
    if (tw, th) == img.dimensions() {
        return Ok(OpOutput::image(img.clone()));
    }
    let fill = match params.fill {
        PadFill::Transparent => CanvasFill::Color(Color::TRANSPARENT),
        PadFill::Color => CanvasFill::Color(params.color),
        PadFill::EdgeExtend => CanvasFill::EdgeExtend,
    };
    Ok(OpOutput::image(place_anchored(
        img,
        tw,
        th,
        params.anchor,
        fill,
    )))
}
