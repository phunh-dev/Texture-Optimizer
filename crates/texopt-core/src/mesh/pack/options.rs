//! User options of the 3D texture packer (camelCase JSON, every field optional).

use std::collections::BTreeMap;

use image::Rgba;
use serde::{Deserialize, Serialize};

use crate::atlas::MAX_ATLAS_SIZE;
use crate::mesh::model::{ModelFormat, TextureChannel};
use crate::mesh::uv_remap::{InsetPolicy, OutOfRangePolicy};
use crate::mesh::ExportFormat;
use crate::{OpError, OpResult};

/// Smallest accepted atlas edge.
pub const MIN_ATLAS_SIZE: u32 = 64;
/// Lowest scale `scaleToFit` goes down to (1/8 of the requested scale).
pub const MIN_FIT_DIVISOR: u32 = 8;

/// What the packer writes besides the atlas textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum OutputMode {
    /// Write remapped copies of the models (OBJ/DAE/FBX).
    #[default]
    RewriteModels,
    /// Leave the models untouched; write `<model>.uvremap.json` sidecars plus
    /// the Unity import script that applies them.
    UvRemapData,
}

/// Output model format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum FormatChoice {
    #[default]
    SameAsSource,
    Obj,
    Collada,
    Fbx,
    FbxAscii,
}

impl FormatChoice {
    pub fn resolve(self, source: ModelFormat) -> ExportFormat {
        match self {
            Self::SameAsSource => ExportFormat::same_as(source),
            Self::Obj => ExportFormat::Obj,
            Self::Collada => ExportFormat::Collada,
            Self::Fbx => ExportFormat::Fbx,
            Self::FbxAscii => ExportFormat::FbxAscii,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OutputOptions {
    pub mode: OutputMode,
    pub format: FormatChoice,
    /// Replace the atlased materials of each model by one shared material.
    pub merge_materials: bool,
    pub merged_material_name: String,
    /// Re-import every written model and compare its geometry (see
    /// `mesh::ExportOptions::verify_geometry`). Always on for FBX unless
    /// `allowUnverifiedFbx`.
    pub verify_geometry: bool,
    /// Advanced: allow switching the geometry check off for FBX output.
    pub allow_unverified_fbx: bool,
    /// When a rewritten model fails the geometry check (or needs atlas pages
    /// a single model file cannot reference), write UV Remap Data for that
    /// model instead of failing it.
    pub auto_fallback: bool,
    /// UV Remap Data: copy the untouched source model next to its sidecar so
    /// the output folder can be dropped into a Unity project as is.
    pub copy_source_models: bool,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            mode: OutputMode::RewriteModels,
            format: FormatChoice::SameAsSource,
            merge_materials: true,
            merged_material_name: "AtlasMaterial".into(),
            verify_geometry: true,
            allow_unverified_fbx: false,
            auto_fallback: true,
            copy_source_models: true,
        }
    }
}

impl OutputOptions {
    /// Geometry verification actually used for `format`.
    pub fn effective_verify(&self, format: ExportFormat) -> bool {
        let fbx = matches!(format, ExportFormat::Fbx | ExportFormat::FbxAscii);
        self.verify_geometry || (fbx && !self.allow_unverified_fbx)
    }
}

/// Channels packed by default (every well-known slot).
pub fn default_channels() -> Vec<TextureChannel> {
    vec![
        TextureChannel::BaseColor,
        TextureChannel::Normal,
        TextureChannel::Metallic,
        TextureChannel::Roughness,
        TextureChannel::Occlusion,
        TextureChannel::Emissive,
        TextureChannel::Opacity,
        TextureChannel::Specular,
        TextureChannel::Height,
    ]
}

/// Fill colour for a material that lacks a packed channel (`#rrggbb[aa]`).
pub fn builtin_default(channel: &TextureChannel) -> Rgba<u8> {
    match channel {
        TextureChannel::BaseColor => Rgba([255, 255, 255, 255]),
        TextureChannel::Normal => Rgba([128, 128, 255, 255]),
        TextureChannel::Metallic => Rgba([0, 0, 0, 255]),
        TextureChannel::Roughness => Rgba([255, 255, 255, 255]),
        TextureChannel::Occlusion => Rgba([255, 255, 255, 255]),
        TextureChannel::Emissive => Rgba([0, 0, 0, 255]),
        TextureChannel::Opacity => Rgba([255, 255, 255, 255]),
        TextureChannel::Specular => Rgba([0, 0, 0, 255]),
        TextureChannel::Height => Rgba([128, 128, 128, 255]),
        TextureChannel::Other(_) => Rgba([0, 0, 0, 255]),
    }
}

/// Parse `#rgb`, `#rrggbb` or `#rrggbbaa`.
pub fn parse_hex_color(s: &str) -> Option<Rgba<u8>> {
    let h = s.trim().strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    match h.len() {
        3 => {
            let d = |i: usize| u8::from_str_radix(h.get(i..i + 1)?, 16).ok().map(|v| v * 17);
            Some(Rgba([d(0)?, d(1)?, d(2)?, 255]))
        }
        6 => Some(Rgba([byte(0)?, byte(2)?, byte(4)?, 255])),
        8 => Some(Rgba([byte(0)?, byte(2)?, byte(4)?, byte(6)?])),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PackOptions {
    /// Channels that get an atlas. A channel used by an atlased material but
    /// not listed is packed anyway (`MESH_CHANNEL_FORCED`), otherwise that
    /// texture would be sampled with atlas UVs.
    pub channels: Vec<TextureChannel>,
    /// Max atlas edge (rounded down to a power of two).
    pub max_size: u32,
    pub force_square: bool,
    /// Transparent pixels between extruded rects.
    pub padding: u32,
    /// Edge pixels replicated around every rect (mipmap/bilinear bleeding).
    pub extrude: u32,
    /// Texture scale in percent (1..=100) applied before packing.
    pub texture_scale: u32,
    /// Halve the scale (down to 1/8) until everything fits on one page.
    pub scale_to_fit: bool,
    /// Spill onto more atlas pages (one atlas set per page) when needed.
    pub multi_page: bool,
    pub inset: InsetPolicy,
    pub out_of_range: OutOfRangePolicy,
    /// Fill colour per channel for materials lacking it (`#rrggbb[aa]`);
    /// channels not listed use [`builtin_default`].
    pub missing_defaults: BTreeMap<TextureChannel, String>,
    pub output: OutputOptions,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self {
            channels: default_channels(),
            max_size: 2048,
            force_square: false,
            padding: 4,
            extrude: 4,
            texture_scale: 100,
            scale_to_fit: false,
            multi_page: true,
            inset: InsetPolicy::HalfTexel,
            out_of_range: OutOfRangePolicy::SkipMaterial,
            missing_defaults: BTreeMap::new(),
            output: OutputOptions::default(),
        }
    }
}

impl PackOptions {
    pub fn validate(&self) -> OpResult<()> {
        if self.max_size < MIN_ATLAS_SIZE || self.max_size > MAX_ATLAS_SIZE {
            return Err(OpError::invalid_param("maxSize", "outOfRange")
                .with("min", MIN_ATLAS_SIZE)
                .with("max", MAX_ATLAS_SIZE));
        }
        if self.texture_scale == 0 || self.texture_scale > 100 {
            return Err(OpError::invalid_param("textureScale", "outOfRange")
                .with("min", 1)
                .with("max", 100));
        }
        if 2 * (self.padding + self.extrude) >= self.max_size / 2 {
            return Err(OpError::invalid_param("padding", "tooLarge"));
        }
        let inset = self.inset.pixels();
        if !inset.is_finite() || !(0.0..=64.0).contains(&inset) {
            return Err(OpError::invalid_param("inset", "outOfRange"));
        }
        if let OutOfRangePolicy::BakeRepeat { max_tiles } = self.out_of_range
            && max_tiles == 0
        {
            return Err(OpError::invalid_param("maxTiles", "outOfRange"));
        }
        for (channel, hex) in &self.missing_defaults {
            if parse_hex_color(hex).is_none() {
                return Err(OpError::invalid_param("missingDefaults", "invalidColor")
                    .with("channel", channel.as_key()));
            }
        }
        if self.output.merge_materials && self.output.merged_material_name.trim().is_empty() {
            return Err(OpError::invalid_param("mergedMaterialName", "empty"));
        }
        Ok(())
    }

    /// Fill colour for `channel`.
    pub fn default_color(&self, channel: &TextureChannel) -> Rgba<u8> {
        self.missing_defaults
            .get(channel)
            .and_then(|h| parse_hex_color(h))
            .unwrap_or_else(|| builtin_default(channel))
    }

    /// Smallest edge a packed rect may have so the inset stays inside it.
    pub fn min_rect_edge(&self) -> u32 {
        (2.0 * self.inset.pixels()).floor() as u32 + 1
    }
    pub(crate) fn min_tile_edge(&self) -> u32 {
        self.min_rect_edge().max(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_partial_json() {
        let o: PackOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(o, PackOptions::default());
        assert_eq!(o.padding, 4);
        assert_eq!(o.extrude, 4);
        assert_eq!(o.inset, InsetPolicy::HalfTexel);
        assert!(o.output.verify_geometry);
        let o: PackOptions = serde_json::from_value(serde_json::json!({
            "channels": ["baseColor", "normal"],
            "outOfRange": { "mode": "bakeRepeat", "maxTiles": 3 },
            "inset": { "mode": "pixels", "pixels": 1.5 },
            "missingDefaults": { "normal": "#8080ff" },
            "output": { "mode": "uvRemapData", "format": "collada", "mergeMaterials": false }
        }))
        .unwrap();
        assert_eq!(o.channels, vec![TextureChannel::BaseColor, TextureChannel::Normal]);
        assert_eq!(o.out_of_range, OutOfRangePolicy::BakeRepeat { max_tiles: 3 });
        assert_eq!(o.output.mode, OutputMode::UvRemapData);
        assert_eq!(o.output.format, FormatChoice::Collada);
        assert!(!o.output.merge_materials);
        assert_eq!(o.output.merged_material_name, "AtlasMaterial");
        assert_eq!(o.default_color(&TextureChannel::Normal), Rgba([128, 128, 255, 255]));
        assert_eq!(o.min_rect_edge(), 4);
        o.validate().unwrap();
    }

    #[test]
    fn hex_colors() {
        assert_eq!(parse_hex_color("#fff"), Some(Rgba([255, 255, 255, 255])));
        assert_eq!(parse_hex_color("#102030"), Some(Rgba([16, 32, 48, 255])));
        assert_eq!(parse_hex_color("#10203040"), Some(Rgba([16, 32, 48, 64])));
        assert_eq!(parse_hex_color("102030"), None);
        assert_eq!(parse_hex_color("#12345"), None);
        assert_eq!(parse_hex_color("#zz0000"), None);
    }

    #[test]
    fn validation_codes() {
        let bad = |f: &dyn Fn(&mut PackOptions)| {
            let mut o = PackOptions::default();
            f(&mut o);
            o.validate().unwrap_err().params["param"].clone()
        };
        assert_eq!(bad(&|o| o.max_size = 32), "maxSize");
        assert_eq!(bad(&|o| o.texture_scale = 0), "textureScale");
        assert_eq!(bad(&|o| o.texture_scale = 101), "textureScale");
        assert_eq!(bad(&|o| o.padding = 1000), "padding");
        assert_eq!(
            bad(&|o| o.out_of_range = OutOfRangePolicy::BakeRepeat { max_tiles: 0 }),
            "maxTiles"
        );
        assert_eq!(
            bad(&|o| {
                o.missing_defaults.insert(TextureChannel::Normal, "blue".into());
            }),
            "missingDefaults"
        );
        assert_eq!(bad(&|o| o.output.merged_material_name = " ".into()), "mergedMaterialName");
    }

    #[test]
    fn fbx_always_verified_unless_advanced() {
        let mut o = OutputOptions {
            verify_geometry: false,
            ..Default::default()
        };
        assert!(o.effective_verify(ExportFormat::Fbx));
        assert!(o.effective_verify(ExportFormat::FbxAscii));
        assert!(!o.effective_verify(ExportFormat::Obj));
        o.allow_unverified_fbx = true;
        assert!(!o.effective_verify(ExportFormat::Fbx));
        assert_eq!(FormatChoice::SameAsSource.resolve(ModelFormat::Dae), ExportFormat::Collada);
        assert_eq!(FormatChoice::FbxAscii.resolve(ModelFormat::Obj), ExportFormat::FbxAscii);
    }
}
