//! Single-image operations. Each submodule exposes:
//! - `<Name>Params`: `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]`
//!   with `#[serde(rename_all = "camelCase", default)]` so the frontend can send partial params;
//! - `pub fn apply(img: &ImageBuf, params: &<Name>Params) -> OpResult<OpOutput>`.

pub mod bg_remove;
pub mod common;
pub mod pot_pad;
pub mod resize;
pub mod resolution;
pub mod trim;

use serde::{Deserialize, Serialize};

use crate::{ImageBuf, OpResult};

/// Result of an operation: the new image plus optional JSON metadata
/// (e.g. trim offsets needed to restore the original pivot).
#[derive(Debug, Clone)]
pub struct OpOutput {
    pub image: ImageBuf,
    pub meta: Option<serde_json::Value>,
}

impl OpOutput {
    pub fn image(image: ImageBuf) -> Self {
        Self { image, meta: None }
    }
}

/// Tagged request sent by the frontend: `{ "kind": "resize", "params": { ... } }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "params", rename_all = "camelCase")]
pub enum OpRequest {
    Resize(resize::ResizeParams),
    Resolution(resolution::ResolutionParams),
    PotPad(pot_pad::PotPadParams),
    Trim(trim::TrimParams),
    BgRemove(bg_remove::BgRemoveParams),
}

pub fn run(img: &ImageBuf, req: &OpRequest) -> OpResult<OpOutput> {
    match req {
        OpRequest::Resize(p) => resize::apply(img, p),
        OpRequest::Resolution(p) => resolution::apply(img, p),
        OpRequest::PotPad(p) => pot_pad::apply(img, p),
        OpRequest::Trim(p) => trim::apply(img, p),
        OpRequest::BgRemove(p) => bg_remove::apply(img, p),
    }
}
