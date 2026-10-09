//! "UV Remap Data" fallback: instead of rewriting the model file, export the
//! atlas plus a JSON sidecar describing every material's UV transform, and
//! let the engine apply it at import time ([`UNITY_POSTPROCESSOR_CS`]).
//!
//! The JSON is deliberately shaped for Unity's `JsonUtility` (no
//! dictionaries, no tagged unions, arrays for vectors). One sidecar per model,
//! named `<model stem>.uvremap.json` and placed next to the model file
//! ([`sidecar_path`]).
//!
//! ```json
//! {
//!   "version": 1,
//!   "generator": "texture-optimizer",
//!   "uvOrigin": "bottomLeft",
//!   "atlasPages": [
//!     { "index": 0, "width": 2048, "height": 2048,
//!       "textures": [ { "channel": "baseColor", "path": "atlas_baseColor.png" } ] }
//!   ],
//!   "models": [
//!     { "model": "crate.fbx", "mergedMaterialName": "AtlasMaterial",
//!       "materials": [
//!         { "materialIndex": 1, "materialName": "Crate", "skipped": false,
//!           "atlasPage": 0, "uvChannel": 0,
//!           "offset": [0.0, 0.5], "scale": [0.5, 0.5],
//!           "normalize": "none", "repeatOrigin": [0, 0], "repeatTiles": [1, 1],
//!           "originalTextures": [ { "channel": "baseColor", "path": "tex/crate.png" } ] } ] } ]
//! }
//! ```
//!
//! Applying an entry to a UV: `n = normalize(uv)`; `uv' = offset + n * scale`
//! with `normalize` = `none` (identity), `clamp` (clamp to [0,1]), `wrap`
//! (subtract the face's integer tile, decided per triangle), `repeat`
//! (`(uv - repeatOrigin) / repeatTiles`). UVs use the bottom-left origin.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::MaterialRemap;
use super::model::{Material, TextureChannel};
use super::uv_remap::{UvNormalize, UvOrigin, UvRemap, UvTransform};
use crate::{OpError, OpResult};

pub const FORMAT_VERSION: u32 = 1;
pub const GENERATOR: &str = "texture-optimizer";
pub const SIDECAR_SUFFIX: &str = ".uvremap.json";

/// Unity Editor script applying the sidecars on model import. Drop it into any
/// `Editor/` folder of the Unity project.
pub const UNITY_POSTPROCESSOR_CS: &str = include_str!("unity_remap_postprocessor.cs");
pub const UNITY_POSTPROCESSOR_FILE_NAME: &str = "TextureOptimizerUvRemapPostprocessor.cs";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPath {
    pub channel: TextureChannel,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtlasPage {
    pub index: u32,
    pub width: u32,
    pub height: u32,
    pub textures: Vec<ChannelPath>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum NormalizeMode {
    #[default]
    None,
    Clamp,
    Wrap,
    Repeat,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialRemapEntry {
    pub material_index: usize,
    pub material_name: String,
    /// Material left out of the atlas (UVs out of range + skip policy).
    pub skipped: bool,
    pub atlas_page: u32,
    pub uv_channel: u32,
    pub offset: [f32; 2],
    pub scale: [f32; 2],
    pub normalize: NormalizeMode,
    pub repeat_origin: [i32; 2],
    pub repeat_tiles: [u32; 2],
    pub original_textures: Vec<ChannelPath>,
}

fn original_textures(material: &Material) -> Vec<ChannelPath> {
    material
        .textures
        .iter()
        .map(|(c, t)| ChannelPath {
            channel: c.clone(),
            path: t.raw_path.clone(),
        })
        .collect()
}

impl MaterialRemapEntry {
    pub fn remapped(
        material_index: usize,
        material: &Material,
        atlas_page: u32,
        remap: &MaterialRemap,
    ) -> Self {
        let (normalize, repeat_origin, repeat_tiles) = match remap.remap.normalize {
            UvNormalize::None => (NormalizeMode::None, [0, 0], [1, 1]),
            UvNormalize::Clamp => (NormalizeMode::Clamp, [0, 0], [1, 1]),
            UvNormalize::Wrap => (NormalizeMode::Wrap, [0, 0], [1, 1]),
            UvNormalize::Repeat { origin, tiles } => (NormalizeMode::Repeat, origin, tiles),
        };
        Self {
            material_index,
            material_name: material.name.clone(),
            skipped: false,
            atlas_page,
            uv_channel: remap.uv_channel,
            offset: remap.remap.transform.offset,
            scale: remap.remap.transform.scale,
            normalize,
            repeat_origin,
            repeat_tiles,
            original_textures: original_textures(material),
        }
    }

    pub fn skipped(material_index: usize, material: &Material) -> Self {
        Self {
            material_index,
            material_name: material.name.clone(),
            skipped: true,
            atlas_page: 0,
            uv_channel: 0,
            offset: UvTransform::IDENTITY.offset,
            scale: UvTransform::IDENTITY.scale,
            normalize: NormalizeMode::None,
            repeat_origin: [0, 0],
            repeat_tiles: [1, 1],
            original_textures: original_textures(material),
        }
    }

    /// Back to the in-memory remap (`None` for skipped materials).
    pub fn to_remap(&self) -> Option<MaterialRemap> {
        if self.skipped {
            return None;
        }
        let normalize = match self.normalize {
            NormalizeMode::None => UvNormalize::None,
            NormalizeMode::Clamp => UvNormalize::Clamp,
            NormalizeMode::Wrap => UvNormalize::Wrap,
            NormalizeMode::Repeat => UvNormalize::Repeat {
                origin: self.repeat_origin,
                tiles: self.repeat_tiles,
            },
        };
        Some(MaterialRemap {
            uv_channel: self.uv_channel,
            remap: UvRemap::new(
                normalize,
                UvTransform {
                    offset: self.offset,
                    scale: self.scale,
                },
            ),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRemapEntry {
    /// Model file name (matched case-insensitively by the engine script).
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_material_name: Option<String>,
    pub materials: Vec<MaterialRemapEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemapFile {
    pub version: u32,
    pub generator: String,
    pub uv_origin: UvOrigin,
    pub atlas_pages: Vec<AtlasPage>,
    pub models: Vec<ModelRemapEntry>,
}

impl RemapFile {
    pub fn new(atlas_pages: Vec<AtlasPage>, models: Vec<ModelRemapEntry>) -> Self {
        Self {
            version: FORMAT_VERSION,
            generator: GENERATOR.into(),
            uv_origin: UvOrigin::BottomLeft,
            atlas_pages,
            models,
        }
    }

    pub fn to_json(&self) -> OpResult<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| OpError::invalid_param("remapFile", &e.to_string()))
    }

    pub fn from_json(json: &str) -> OpResult<Self> {
        let f: Self = serde_json::from_str(json)
            .map_err(|e| OpError::invalid_param("remapFile", &e.to_string()))?;
        if f.version != FORMAT_VERSION {
            return Err(OpError::invalid_param("remapFile", "unsupportedVersion")
                .with("version", f.version));
        }
        Ok(f)
    }

    pub fn write(&self, path: &Path) -> OpResult<()> {
        std::fs::write(path, self.to_json()?).map_err(|e| {
            OpError::new(crate::error::codes::IO_WRITE_FAILED)
                .with("path", path.display().to_string())
                .with("detail", e.to_string())
        })
    }
}

/// `<dir>/<stem>.uvremap.json` next to the model file.
pub fn sidecar_path(model_path: &Path) -> PathBuf {
    let stem = model_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    model_path.with_file_name(format!("{stem}{SIDECAR_SUFFIX}"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::mesh::model::{TextureRef, WrapMode};

    fn material() -> Material {
        let mut textures = BTreeMap::new();
        textures.insert(
            TextureChannel::BaseColor,
            TextureRef {
                path: PathBuf::from("/m/tex/red.png"),
                raw_path: "tex/red.png".into(),
                uv_channel: 0,
                wrap_mode: WrapMode::Repeat,
                exists: true,
                embedded_index: None,
            },
        );
        Material {
            name: "MatA".into(),
            textures,
        }
    }

    #[test]
    fn json_shape_and_round_trip() {
        let remap = MaterialRemap {
            uv_channel: 0,
            remap: UvRemap::new(
                UvNormalize::Repeat {
                    origin: [0, -1],
                    tiles: [2, 2],
                },
                UvTransform {
                    offset: [0.5, 0.25],
                    scale: [0.5, 0.25],
                },
            ),
        };
        let file = RemapFile::new(
            vec![AtlasPage {
                index: 0,
                width: 256,
                height: 256,
                textures: vec![ChannelPath {
                    channel: TextureChannel::BaseColor,
                    path: "atlas_baseColor.png".into(),
                }],
            }],
            vec![ModelRemapEntry {
                model: "two_quads.obj".into(),
                merged_material_name: None,
                materials: vec![
                    MaterialRemapEntry::remapped(1, &material(), 0, &remap),
                    MaterialRemapEntry::skipped(2, &material()),
                ],
            }],
        );
        let json = file.to_json().unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["version"], 1);
        assert_eq!(v["uvOrigin"], "bottomLeft");
        assert_eq!(v["atlasPages"][0]["textures"][0]["channel"], "baseColor");
        let m = &v["models"][0]["materials"][0];
        assert_eq!(m["materialName"], "MatA");
        assert_eq!(m["normalize"], "repeat");
        assert_eq!(m["repeatOrigin"], serde_json::json!([0, -1]));
        assert_eq!(m["offset"], serde_json::json!([0.5, 0.25]));
        assert_eq!(m["originalTextures"][0]["path"], "tex/red.png");
        assert!(v["models"][0].get("mergedMaterialName").is_none());
        assert_eq!(v["models"][0]["materials"][1]["skipped"], true);

        let back = RemapFile::from_json(&json).unwrap();
        assert_eq!(back, file);
        assert_eq!(back.models[0].materials[0].to_remap(), Some(remap));
        assert_eq!(back.models[0].materials[1].to_remap(), None);
    }

    #[test]
    fn rejects_unknown_version() {
        let json =
            r#"{"version":2,"generator":"x","uvOrigin":"bottomLeft","atlasPages":[],"models":[]}"#;
        assert!(RemapFile::from_json(json).is_err());
    }

    #[test]
    fn sidecar_name() {
        assert_eq!(
            sidecar_path(Path::new("a/b/Crate.FBX")),
            PathBuf::from("a/b/Crate.uvremap.json")
        );
    }

    #[test]
    fn unity_script_matches_json_contract() {
        for field in [
            "materialIndex",
            "materialName",
            "skipped",
            "atlasPage",
            "uvChannel",
            "offset",
            "scale",
            "normalize",
            "repeatOrigin",
            "repeatTiles",
            "originalTextures",
            "mergedMaterialName",
            "uvremap.json",
            "AssetPostprocessor",
            "OnPostprocessModel",
        ] {
            assert!(
                UNITY_POSTPROCESSOR_CS.contains(field),
                "script lacks {field}"
            );
        }
    }
}
