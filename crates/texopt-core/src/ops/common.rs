//! Types shared by several operations. Their serialized shape is part of the
//! frontend contract: add variants freely, but do not rename existing ones.

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ResampleFilter {
    Nearest,
    Bilinear,
    CatmullRom,
    Mitchell,
    #[default]
    Lanczos3,
}

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
    #[default]
    Nearest,
    Up,
    Down,
}

/// Straight-alpha RGBA color, serialized as `[r, g, b, a]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Color(pub [u8; 4]);
