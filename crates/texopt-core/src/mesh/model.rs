//! Engine-neutral description of an imported 3D model: just what the texture
//! packer needs (meshes → material, UV channels; materials → textures per
//! channel). Geometry other than UVs/faces stays inside the backend handle.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::uv_remap::{UvRangeReport, uv_range};
use crate::OpError;

/// Model container formats accepted as input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelFormat {
    Fbx,
    Obj,
    Dae,
}

impl ModelFormat {
    /// Detects the format from the file extension (case-insensitive).
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "fbx" => Some(Self::Fbx),
            "obj" => Some(Self::Obj),
            "dae" => Some(Self::Dae),
            _ => None,
        }
    }
}

/// Logical texture slot. Serialized as a plain camelCase string
/// (`"baseColor"`, `"normal"`, …) and `"other:<name>"` for everything else so
/// it can be used as a JSON object key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextureChannel {
    BaseColor,
    Normal,
    Metallic,
    Roughness,
    Occlusion,
    Emissive,
    Opacity,
    Specular,
    Height,
    Other(String),
}

impl TextureChannel {
    pub fn as_key(&self) -> String {
        match self {
            Self::BaseColor => "baseColor".into(),
            Self::Normal => "normal".into(),
            Self::Metallic => "metallic".into(),
            Self::Roughness => "roughness".into(),
            Self::Occlusion => "occlusion".into(),
            Self::Emissive => "emissive".into(),
            Self::Opacity => "opacity".into(),
            Self::Specular => "specular".into(),
            Self::Height => "height".into(),
            Self::Other(name) => format!("other:{name}"),
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "baseColor" => Self::BaseColor,
            "normal" => Self::Normal,
            "metallic" => Self::Metallic,
            "roughness" => Self::Roughness,
            "occlusion" => Self::Occlusion,
            "emissive" => Self::Emissive,
            "opacity" => Self::Opacity,
            "specular" => Self::Specular,
            "height" => Self::Height,
            other => Self::Other(other.strip_prefix("other:").unwrap_or(other).to_string()),
        }
    }
}

impl fmt::Display for TextureChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_key())
    }
}

impl Serialize for TextureChannel {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.as_key())
    }
}

impl<'de> Deserialize<'de> for TextureChannel {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = TextureChannel;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a texture channel string")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<TextureChannel, E> {
                Ok(TextureChannel::from_key(v))
            }
        }
        d.deserialize_str(V)
    }
}

/// Texture addressing mode as stored in the model file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum WrapMode {
    #[default]
    Repeat,
    Clamp,
    Mirror,
    Decal,
}

/// A texture referenced by a material.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureRef {
    /// Resolved location: relative references are joined to the model's
    /// directory; when the referenced file does not exist but a file with the
    /// same name sits next to the model, that one is used (common for FBX
    /// files carrying absolute paths from the artist's machine).
    pub path: PathBuf,
    /// The reference exactly as written in the model file.
    pub raw_path: String,
    /// UV channel (Assimp `uvwsrc`) this texture is sampled with.
    pub uv_channel: u32,
    /// U-axis wrap mode (V is almost always identical).
    pub wrap_mode: WrapMode,
    /// Whether `path` points to an existing file.
    pub exists: bool,
    /// Index into [`Model::embedded_textures`] when the texture is embedded in
    /// the model file instead of living on disk.
    pub embedded_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Material {
    pub name: String,
    pub textures: BTreeMap<TextureChannel, TextureRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mesh {
    pub name: String,
    pub material_index: usize,
    pub vertex_count: usize,
    /// `uv_channels[c][v]` = UV of vertex `v` in channel `c`. Assimp UV
    /// convention: origin bottom-left (v grows upwards). A missing channel in
    /// the middle of the list is an empty Vec.
    pub uv_channels: Vec<Vec<[f32; 2]>>,
    /// Polygon vertex indices (not triangulated). Not serialized: only the
    /// UV wrap analysis needs them.
    #[serde(skip)]
    pub faces: Vec<Vec<u32>>,
}

/// Texture stored inside the model file (FBX "embedded media").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedTexture {
    pub index: usize,
    /// Original file name recorded in the model, if any.
    pub file_name: String,
    /// `png`, `jpg`, … for compressed payloads; `rgba8888`-style hints or
    /// empty for raw texel arrays.
    pub format_hint: String,
    /// True when the payload is an encoded image file (PNG/JPG/…).
    pub compressed: bool,
    /// Pixel size for raw texel arrays; `(byte_len, 0)` for compressed data.
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub format: ModelFormat,
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
    pub embedded_textures: Vec<EmbeddedTexture>,
    /// Non-fatal findings (`MESH_TEXTURE_NOT_FOUND`, …).
    pub warnings: Vec<OpError>,
}

impl Model {
    /// Meshes that use material `material_index`.
    pub fn meshes_with_material(&self, material_index: usize) -> impl Iterator<Item = &Mesh> {
        self.meshes
            .iter()
            .filter(move |m| m.material_index == material_index)
    }

    /// UV bounding box of `uv_channel` over every mesh using the material.
    /// `None` when no mesh uses it or a mesh lacks that channel.
    pub fn material_uv_range(
        &self,
        material_index: usize,
        uv_channel: u32,
    ) -> Option<UvRangeReport> {
        let mut acc: Option<UvRangeReport> = None;
        for mesh in self
            .meshes_with_material(material_index)
            .filter(|m| m.vertex_count > 0)
        {
            let r = uv_range(mesh.uv_channels.get(uv_channel as usize)?)?;
            acc = Some(acc.map_or(r, |a| a.merge(r)));
        }
        acc
    }
}

/// Resolve a texture reference relative to `model_dir`.
/// Returns `(resolved_path, exists)`.
pub fn resolve_texture_path(model_dir: &Path, raw: &str) -> (PathBuf, bool) {
    // Model files authored on Windows often use backslashes.
    let normalized = if cfg!(windows) {
        raw.to_string()
    } else {
        raw.replace('\\', "/")
    };
    let raw_path = PathBuf::from(&normalized);
    let primary = if raw_path.is_absolute() {
        raw_path.clone()
    } else {
        model_dir.join(&raw_path)
    };
    if primary.is_file() {
        return (primary, true);
    }
    // Fallback: same file name next to the model.
    let file_name = normalized.rsplit(['/', '\\']).next().unwrap_or(&normalized);
    if !file_name.is_empty() {
        let sibling = model_dir.join(file_name);
        if sibling.is_file() {
            return (sibling, true);
        }
    }
    (primary, false)
}

/// Path of `target` relative to the directory `base_dir`, with `/`
/// separators (what model files expect). Falls back to the absolute path when
/// no relative path exists (different drive on Windows).
pub fn relative_path(base_dir: &Path, target: &Path) -> String {
    use std::path::Component;
    let base: Vec<Component> = base_dir.components().collect();
    let tgt: Vec<Component> = target.components().collect();
    let common = base
        .iter()
        .zip(tgt.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let same_root = common > 0 || (!base_dir.has_root() && !target.has_root());
    if !same_root {
        return target.to_string_lossy().replace('\\', "/");
    }
    let mut parts: Vec<String> = Vec::new();
    for _ in common..base.len() {
        parts.push("..".into());
    }
    for c in &tgt[common..] {
        parts.push(c.as_os_str().to_string_lossy().into_owned());
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_serializes_as_string_key() {
        let mut m = BTreeMap::new();
        m.insert(TextureChannel::BaseColor, 1);
        m.insert(TextureChannel::Other("shininess".into()), 2);
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, r#"{"baseColor":1,"other:shininess":2}"#);
        let back: BTreeMap<TextureChannel, i32> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn format_from_extension() {
        assert_eq!(
            ModelFormat::from_path(Path::new("a/B.FBX")),
            Some(ModelFormat::Fbx)
        );
        assert_eq!(
            ModelFormat::from_path(Path::new("x.obj")),
            Some(ModelFormat::Obj)
        );
        assert_eq!(
            ModelFormat::from_path(Path::new("x.dae")),
            Some(ModelFormat::Dae)
        );
        assert_eq!(ModelFormat::from_path(Path::new("x.glb")), None);
        assert_eq!(ModelFormat::from_path(Path::new("x.blend")), None);
    }

    #[test]
    fn relative_paths() {
        assert_eq!(
            relative_path(Path::new("/a/b/out"), Path::new("/a/b/out/atlas.png")),
            "atlas.png"
        );
        assert_eq!(
            relative_path(Path::new("/a/b/out"), Path::new("/a/b/tex/atlas.png")),
            "../tex/atlas.png"
        );
        assert_eq!(
            relative_path(Path::new("out"), Path::new("out/sub/x.png")),
            "sub/x.png"
        );
    }
}
