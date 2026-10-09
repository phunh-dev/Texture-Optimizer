//! Resolution Fixer: make both sides a multiple of 4, a multiple of N, or a
//! power of two.
//!
//! Size rules:
//! - Each side is rounded independently to the target (`round`; never below
//!   N, 4 or 1, so never 0). Ties round up.
//! - `round` applies to `method = resample`. `pad` always rounds **up** (never
//!   loses pixels) and `crop` always rounds **down** (never adds pixels).
//! - `pot` with `allowNonSquare = false`: both sides become the larger side.
//!   `allowNonSquare` has no effect on the multiple-of targets.
//! - `maxSize` (0 = unlimited) caps each side to the largest valid value
//!   `<= maxSize`; if no valid value fits (e.g. N > maxSize) → `INVALID_PARAMS`.
//!   Under `pad`, a capped side ends up cropped (anchored).
//! - If the result equals the input size, the input is returned unchanged
//!   (byte-identical, no resampling).
//!
//! Methods:
//! - `resample`: `keepAspect = false` stretches to the target size.
//!   `keepAspect = true` scales uniformly to fit inside the target size, then
//!   pads the remainder with `padColor`, placed by `anchor`.
//! - `pad`: canvas grows, content placed by `anchor`, new area = `padColor`.
//! - `crop`: canvas shrinks, the kept region is chosen by `anchor`.

use serde::{Deserialize, Serialize};

use super::OpOutput;
use super::common::{
    Anchor, CanvasFill, Color, ResampleFilter, ResampleOptions, RoundMode, check_max_dimension,
    ensure_non_empty, place_anchored, prev_pot, resample, round_pot, round_to_multiple,
    scale_dimension,
};
use crate::{ImageBuf, OpError, OpResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ResolutionTarget {
    #[default]
    MultipleOf4,
    MultipleOfN,
    Pot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ResolutionMethod {
    #[default]
    Resample,
    Pad,
    Crop,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ResolutionParams {
    pub target: ResolutionTarget,
    /// Used by `multipleOfN`; must be >= 1.
    pub n: u32,
    pub round: RoundMode,
    pub method: ResolutionMethod,
    pub anchor: Anchor,
    pub filter: ResampleFilter,
    pub keep_aspect: bool,
    pub allow_non_square: bool,
    /// Largest allowed side; 0 = unlimited.
    pub max_size: u32,
    pub pad_color: Color,
}

impl Default for ResolutionParams {
    fn default() -> Self {
        Self {
            target: ResolutionTarget::MultipleOf4,
            n: 8,
            round: RoundMode::Nearest,
            method: ResolutionMethod::Resample,
            anchor: Anchor::Center,
            filter: ResampleFilter::Lanczos3,
            keep_aspect: false,
            allow_non_square: true,
            max_size: 8192,
            pad_color: Color::TRANSPARENT,
        }
    }
}

/// Output size for a `w` x `h` input. Errors on invalid params.
pub fn target_size(w: u32, h: u32, params: &ResolutionParams) -> OpResult<(u32, u32)> {
    let round = match params.method {
        ResolutionMethod::Resample => params.round,
        ResolutionMethod::Pad => RoundMode::Up,
        ResolutionMethod::Crop => RoundMode::Down,
    };
    let multiple = match params.target {
        ResolutionTarget::MultipleOf4 => Some(4),
        ResolutionTarget::MultipleOfN if params.n == 0 => {
            return Err(OpError::invalid_param("n", "must be >= 1"));
        }
        ResolutionTarget::MultipleOfN => Some(params.n),
        ResolutionTarget::Pot => None,
    };
    let fix = |v: u32| match multiple {
        Some(m) => round_to_multiple(v, m, round),
        None => round_pot(v, round),
    };
    let (mut tw, mut th) = (fix(w), fix(h));
    if multiple.is_none() && !params.allow_non_square {
        let s = tw.max(th);
        (tw, th) = (s, s);
    }
    if params.max_size > 0 {
        let cap = match multiple {
            Some(m) => params.max_size / m * m,
            None => prev_pot(params.max_size),
        };
        if cap == 0 {
            return Err(OpError::invalid_param(
                "maxSize",
                "smaller than the smallest valid size",
            ));
        }
        (tw, th) = (tw.min(cap), th.min(cap));
    }
    check_max_dimension(tw, th)?;
    Ok((tw, th))
}

pub fn apply(img: &ImageBuf, params: &ResolutionParams) -> OpResult<OpOutput> {
    ensure_non_empty(img)?;
    let (w, h) = img.dimensions();
    let (tw, th) = target_size(w, h, params)?;
    if (tw, th) == (w, h) {
        return Ok(OpOutput::image(img.clone()));
    }
    let fill = CanvasFill::Color(params.pad_color);
    let image = match params.method {
        ResolutionMethod::Resample => {
            let opts = ResampleOptions {
                filter: params.filter,
                ..Default::default()
            };
            if params.keep_aspect {
                let s = (tw as f64 / w as f64).min(th as f64 / h as f64);
                let iw = scale_dimension(w, s).min(tw);
                let ih = scale_dimension(h, s).min(th);
                let inner = resample(img, iw, ih, &opts);
                place_anchored(&inner, tw, th, params.anchor, fill)
            } else {
                resample(img, tw, th, &opts)
            }
        }
        ResolutionMethod::Pad | ResolutionMethod::Crop => {
            place_anchored(img, tw, th, params.anchor, fill)
        }
    };
    Ok(OpOutput::image(image))
}
