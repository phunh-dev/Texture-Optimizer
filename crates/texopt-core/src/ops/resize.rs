//! Resize: scale an image by percent, to an exact box, to a width/height, or
//! by its longest side, with a selectable resampling filter.
//!
//! Size rules (all results are rounded half-up and at least 1x1):
//! - `percent`: both sides scaled by `percent / 100` (aspect always kept).
//! - `exact`: `keepAspect` = fit inside `width` x `height` (contain);
//!   otherwise stretch to exactly `width` x `height`.
//! - `fitWidth` / `fitHeight`: that side becomes `width` / `height`; the other
//!   side scales proportionally if `keepAspect`, else stays unchanged.
//! - `longestSide`: the longer side becomes `longestSide`; the other side
//!   scales proportionally if `keepAspect`, else stays unchanged.
//!
//! `snap` is applied last by rounding each side to the *nearest* multiple of 4
//! / power of two (the image is resampled to the snapped size, not padded).

use serde::{Deserialize, Serialize};

use super::OpOutput;
use super::common::{
    ResampleFilter, ResampleOptions, RoundMode, SnapMode, check_max_dimension, ensure_non_empty,
    resample, scale_dimension, snap_size,
};
use crate::{ImageBuf, OpError, OpResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ResizeMode {
    #[default]
    Percent,
    Exact,
    FitWidth,
    FitHeight,
    LongestSide,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ResizeParams {
    pub mode: ResizeMode,
    /// Used by `percent` mode; must be > 0.
    pub percent: f64,
    /// Used by `exact` and `fitWidth`.
    pub width: u32,
    /// Used by `exact` and `fitHeight`.
    pub height: u32,
    /// Used by `longestSide`.
    pub longest_side: u32,
    pub keep_aspect: bool,
    pub filter: ResampleFilter,
    pub linear_space: bool,
    pub premultiply_alpha: bool,
    pub snap: SnapMode,
}

impl Default for ResizeParams {
    fn default() -> Self {
        Self {
            mode: ResizeMode::Percent,
            percent: 50.0,
            width: 1024,
            height: 1024,
            longest_side: 1024,
            keep_aspect: true,
            filter: ResampleFilter::Lanczos3,
            linear_space: false,
            premultiply_alpha: true,
            snap: SnapMode::None,
        }
    }
}

fn positive(value: u32, name: &str) -> OpResult<f64> {
    if value == 0 {
        return Err(OpError::invalid_param(name, "must be > 0"));
    }
    Ok(value as f64)
}

/// Output size for a `w` x `h` input, after snapping. Errors on invalid params.
pub fn target_size(w: u32, h: u32, params: &ResizeParams) -> OpResult<(u32, u32)> {
    let (wf, hf) = (w as f64, h as f64);
    let keep = params.keep_aspect;
    let (tw, th) = match params.mode {
        ResizeMode::Percent => {
            let p = params.percent;
            if !p.is_finite() || p <= 0.0 {
                return Err(OpError::invalid_param("percent", "must be > 0"));
            }
            (scale_dimension(w, p / 100.0), scale_dimension(h, p / 100.0))
        }
        ResizeMode::Exact => {
            let bw = positive(params.width, "width")?;
            let bh = positive(params.height, "height")?;
            if keep {
                let s = (bw / wf).min(bh / hf);
                (scale_dimension(w, s), scale_dimension(h, s))
            } else {
                (params.width, params.height)
            }
        }
        ResizeMode::FitWidth => {
            let bw = positive(params.width, "width")?;
            (
                params.width,
                if keep { scale_dimension(h, bw / wf) } else { h },
            )
        }
        ResizeMode::FitHeight => {
            let bh = positive(params.height, "height")?;
            (
                if keep { scale_dimension(w, bh / hf) } else { w },
                params.height,
            )
        }
        ResizeMode::LongestSide => {
            let l = positive(params.longest_side, "longestSide")?;
            let s = l / wf.max(hf);
            match (keep, w >= h) {
                (true, _) => (scale_dimension(w, s), scale_dimension(h, s)),
                (false, true) => (params.longest_side, h),
                (false, false) => (w, params.longest_side),
            }
        }
    };
    let (tw, th) = snap_size(tw, th, params.snap, RoundMode::Nearest);
    check_max_dimension(tw, th)?;
    Ok((tw, th))
}

pub fn apply(img: &ImageBuf, params: &ResizeParams) -> OpResult<OpOutput> {
    ensure_non_empty(img)?;
    let (w, h) = target_size(img.width(), img.height(), params)?;
    let opts = ResampleOptions {
        filter: params.filter,
        linear_space: params.linear_space,
        premultiply_alpha: params.premultiply_alpha,
    };
    Ok(OpOutput::image(resample(img, w, h, &opts)))
}
