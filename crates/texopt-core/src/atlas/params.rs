//! Packing parameters sent by the frontend (camelCase JSON, every field optional).

use serde::{Deserialize, Serialize};

use crate::{OpError, OpResult};

/// Largest page edge the packer accepts (matches the largest Unity/GPU texture size).
pub const MAX_ATLAS_SIZE: u32 = 16384;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PackAlgorithm {
    #[default]
    MaxRects,
    Skyline,
}

/// Placement heuristic. `maxRects` accepts the first five, `skyline` accepts
/// `bottomLeft` and `minWaste`; other combinations are rejected with
/// `INVALID_PARAMS { param: "heuristic" }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PackHeuristic {
    #[default]
    BestShortSideFit,
    BestLongSideFit,
    BestAreaFit,
    BottomLeft,
    ContactPoint,
    MinWaste,
}

impl PackHeuristic {
    pub fn supported_by(self, algorithm: PackAlgorithm) -> bool {
        match algorithm {
            PackAlgorithm::MaxRects => !matches!(self, PackHeuristic::MinWaste),
            PackAlgorithm::Skyline => {
                matches!(self, PackHeuristic::BottomLeft | PackHeuristic::MinWaste)
            }
        }
    }
}

/// Order in which sprites are fed to the packer. Numeric keys sort descending,
/// `name` ascending, `none` keeps input order; ties are broken by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SortBy {
    #[default]
    Area,
    MaxSide,
    Height,
    Width,
    Name,
    None,
}

/// `shrinkToFit`: each page is the smallest size (POT when `forcePot`) that
/// holds its sprites. `fixed`: every page is exactly `maxWidth` x `maxHeight`
/// (after POT/square normalisation, see [`AtlasParams::effective_max`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SizeMode {
    #[default]
    ShrinkToFit,
    Fixed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AtlasParams {
    pub algorithm: PackAlgorithm,
    pub heuristic: PackHeuristic,
    pub max_width: u32,
    pub max_height: u32,
    pub force_pot: bool,
    pub force_square: bool,
    /// Transparent pixels between two sprites' extruded areas.
    pub padding: u32,
    /// Sprite edge pixels replicated outward on every side.
    pub extrude: u32,
    /// Transparent pixels kept free along the page edges.
    pub border: u32,
    /// Allow 90 degree clockwise rotation of sprites.
    pub allow_rotation: bool,
    /// Remove transparent borders before packing.
    pub trim: bool,
    /// Pixels with alpha `<= trimThreshold` count as transparent when trimming.
    pub trim_threshold: u8,
    /// Sprites with identical source pixels share one rect (extra names become aliases).
    pub dedupe: bool,
    /// Spill onto extra pages instead of failing with `ATLAS_DOES_NOT_FIT`.
    pub multi_page: bool,
    pub sort_by: SortBy,
    /// Store premultiplied colors in the page images.
    pub premultiply_alpha: bool,
    pub size_mode: SizeMode,
}

impl Default for AtlasParams {
    fn default() -> Self {
        Self {
            algorithm: PackAlgorithm::MaxRects,
            heuristic: PackHeuristic::BestShortSideFit,
            max_width: 2048,
            max_height: 2048,
            force_pot: true,
            force_square: false,
            padding: 2,
            extrude: 0,
            border: 0,
            allow_rotation: false,
            trim: true,
            trim_threshold: 0,
            dedupe: true,
            multi_page: false,
            sort_by: SortBy::Area,
            premultiply_alpha: false,
            size_mode: SizeMode::ShrinkToFit,
        }
    }
}

pub(crate) fn floor_pot(v: u32) -> u32 {
    if v == 0 {
        0
    } else {
        1 << (31 - v.leading_zeros())
    }
}

impl AtlasParams {
    pub fn validate(&self) -> OpResult<()> {
        for (name, v) in [("maxWidth", self.max_width), ("maxHeight", self.max_height)] {
            if v == 0 || v > MAX_ATLAS_SIZE {
                return Err(OpError::invalid_param(name, "outOfRange")
                    .with("min", 1)
                    .with("max", MAX_ATLAS_SIZE));
            }
        }
        if !self.heuristic.supported_by(self.algorithm) {
            return Err(OpError::invalid_param(
                "heuristic",
                "unsupportedForAlgorithm",
            ));
        }
        let (w, h) = self.effective_max();
        let margin = 2 * u64::from(self.border) + 2 * u64::from(self.extrude);
        if u64::from(w) <= margin || u64::from(h) <= margin {
            return Err(OpError::invalid_param("border", "tooLarge"));
        }
        Ok(())
    }

    /// Largest page size allowed: `maxWidth`/`maxHeight` rounded down to a
    /// power of two when `forcePot`, then both set to the smaller one when
    /// `forceSquare`.
    pub fn effective_max(&self) -> (u32, u32) {
        let (mut w, mut h) = (self.max_width, self.max_height);
        if self.force_pot {
            w = floor_pot(w);
            h = floor_pot(h);
        }
        if self.force_square {
            let s = w.min(h);
            w = s;
            h = s;
        }
        (w, h)
    }
}
