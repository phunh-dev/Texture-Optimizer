//! 3D model texture packing: import FBX/OBJ/DAE, describe meshes/materials/
//! textures, compute UV remaps into a shared atlas and export the remapped
//! model (or, as a fallback, JSON remap data + a Unity import script).
//!
//! The atlas packing itself lives in [`crate::atlas`]; this module only takes
//! the resulting rectangles ([`uv_remap::AtlasRect`]) as input.
//!
//! Backend: Assimp 6.0.x through `asset-importer-sys` (feature `assimp`,
//! enabled by default). See `docs/spikes/3d-assimp.md` for the evaluation.
//!
//! Typical flow:
//! 1. [`import`] each model → [`LoadedModel`] (`.model` lists meshes,
//!    materials and per-channel textures).
//! 2. For every material: [`uv_remap::uv_range`] over its meshes' UVs →
//!    [`uv_remap::plan_material`] with the user's [`uv_remap::OutOfRangePolicy`].
//! 3. Pack the textures (same rect for every channel), then build a
//!    [`uv_remap::UvRemap`] per material from [`uv_remap::atlas_transform`].
//! 4. [`export`] the remapped model, and/or write [`remap_json`] sidecars for
//!    the "UV Remap Data" mode.

pub mod fixtures;
pub mod model;
pub mod remap_json;
pub mod uv_remap;

#[cfg(feature = "assimp")]
mod assimp;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use model::{
    EmbeddedTexture, Material, Mesh, Model, ModelFormat, TextureChannel, TextureRef, WrapMode,
};
pub use uv_remap::{
    AtlasRect, InsetPolicy, OutOfRangePolicy, UvNormalize, UvOrigin, UvRangeReport, UvRemap,
    UvTransform,
};

use crate::{OpError, OpResult};

/// Error/warning codes of the mesh module (translated by the frontend via
/// `errors:<CODE>`; never shown raw).
pub mod codes {
    /// Assimp could not read the file. Params: `path`, `detail`.
    pub const MESH_IMPORT_FAILED: &str = "MESH_IMPORT_FAILED";
    /// Writing the model failed. Params: `path`, `format`, `detail`.
    pub const MESH_EXPORT_FAILED: &str = "MESH_EXPORT_FAILED";
    /// Extension/format not supported. Params: `path` or `format`.
    pub const MESH_FORMAT_UNSUPPORTED: &str = "MESH_FORMAT_UNSUPPORTED";
    /// A material's meshes lack the UV channel its textures use. Params:
    /// `material`, `uvChannel` (+ `mesh` when raised by `export`).
    pub const MESH_NO_UVS: &str = "MESH_NO_UVS";
    /// Warning: UVs outside [0,1], material left out of the atlas
    /// (skipMaterial policy). Params: `material`, `min`, `max` ([u, v] arrays).
    pub const MESH_UV_OUT_OF_RANGE: &str = "MESH_UV_OUT_OF_RANGE";
    /// bakeRepeat needs more tiles than allowed. Params: `material`, `tilesU`,
    /// `tilesV`, `maxTiles`.
    pub const MESH_UV_TOO_MANY_TILES: &str = "MESH_UV_TOO_MANY_TILES";
    /// Warning for wrapIntoTile: faces cross tile borders. Params: `mesh`,
    /// `faces`, `vertices`.
    pub const MESH_UV_WRAP_STRADDLE: &str = "MESH_UV_WRAP_STRADDLE";
    /// A remapped material samples a texture channel that has no atlas.
    /// Params: `material`, `channel`.
    pub const MESH_ATLAS_CHANNEL_MISSING: &str = "MESH_ATLAS_CHANNEL_MISSING";
    /// Warning: referenced texture file not found. Params: `material`,
    /// `channel`, `path`.
    pub const MESH_TEXTURE_NOT_FOUND: &str = "MESH_TEXTURE_NOT_FOUND";
    /// The written model re-imports with different world-space geometry (the
    /// output was deleted). Params: `path`, `format`, `relError`. Use the UV
    /// Remap Data mode or another output format.
    pub const MESH_EXPORT_GEOMETRY_CHANGED: &str = "MESH_EXPORT_GEOMETRY_CHANGED";
    /// Built without the `assimp` feature.
    pub const MESH_BACKEND_UNAVAILABLE: &str = "MESH_BACKEND_UNAVAILABLE";
}

/// File extensions accepted by [`import`] (lowercase, without dot).
pub const SUPPORTED_EXTENSIONS: &[&str] = &["fbx", "obj", "dae"];

/// Output formats [`export`] can write. All were round-trip tested
/// (export → re-import → UVs/material references compared, see
/// `tests/mesh_roundtrip.rs`) and checked on third-party FBX files
/// (docs/spikes/3d-assimp.md). glTF is intentionally absent: Assimp 6.0.5's
/// glTF2 writer corrupted the heap on a real-world model during the spike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    /// Wavefront OBJ + MTL (one UV set; no hierarchy/skinning/animation).
    Obj,
    /// COLLADA 1.4 (.dae).
    Collada,
    /// Binary FBX 7.5 (Assimp writer; see `verify_geometry`).
    Fbx,
    /// ASCII FBX 7.5 (Assimp writer).
    FbxAscii,
}

impl ExportFormat {
    /// Assimp exporter id.
    pub fn assimp_id(self) -> &'static str {
        match self {
            Self::Obj => "obj",
            Self::Collada => "collada",
            Self::Fbx => "fbx",
            Self::FbxAscii => "fbxa",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Obj => "obj",
            Self::Collada => "dae",
            Self::Fbx | Self::FbxAscii => "fbx",
        }
    }

    /// Default output format for a given input format ("same as source").
    pub fn same_as(format: ModelFormat) -> Self {
        match format {
            ModelFormat::Fbx => Self::Fbx,
            ModelFormat::Obj => Self::Obj,
            ModelFormat::Dae => Self::Collada,
        }
    }
}

/// Remap of one material: which UV channel to rewrite and how.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialRemap {
    pub uv_channel: u32,
    pub remap: UvRemap,
}

/// Material index → remap. Materials not in the map are left untouched.
pub type ModelRemaps = BTreeMap<usize, MaterialRemap>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    pub format: ExportFormat,
    /// Replace every remapped material by one shared material that references
    /// the atlas textures (fewer draw calls). Non-remapped materials are kept.
    pub merge_materials: bool,
    /// Name of the merged material (default `"AtlasMaterial"`).
    #[serde(default)]
    pub merged_material_name: Option<String>,
    /// Atlas texture per channel, written verbatim into the model file (use
    /// [`model::relative_path`] from the output directory for portability).
    pub atlas_textures: BTreeMap<TextureChannel, String>,
    /// Re-import the written file and compare its world-space bounding box
    /// with the source; on mismatch the output is deleted and
    /// `MESH_EXPORT_GEOMETRY_CHANGED` is returned. Guards against Assimp
    /// writer bugs (e.g. FBX geometric pivots). Default `true`.
    #[serde(default = "default_true")]
    pub verify_geometry: bool,
}

fn default_true() -> bool {
    true
}

/// Non-fatal findings of [`export`] (e.g. `MESH_UV_WRAP_STRADDLE`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub warnings: Vec<OpError>,
}

/// An imported model plus the backend handle needed to export it again.
pub struct LoadedModel {
    pub model: Model,
    pub source_path: PathBuf,
    #[cfg(feature = "assimp")]
    scene: assimp::SceneHandle,
}

impl std::fmt::Debug for LoadedModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedModel")
            .field("source_path", &self.source_path)
            .field("model", &self.model)
            .finish()
    }
}

fn check_import_format(path: &Path) -> OpResult<ModelFormat> {
    ModelFormat::from_path(path).ok_or_else(|| {
        OpError::new(codes::MESH_FORMAT_UNSUPPORTED).with("path", path.display().to_string())
    })
}

/// Import a `.fbx` / `.obj` / `.dae` model.
pub fn import(path: &Path) -> OpResult<LoadedModel> {
    let format = check_import_format(path)?;
    #[cfg(feature = "assimp")]
    {
        let (scene, model) = assimp::import(path, format)?;
        Ok(LoadedModel {
            model,
            source_path: path.to_path_buf(),
            scene,
        })
    }
    #[cfg(not(feature = "assimp"))]
    {
        let _ = format;
        Err(OpError::new(codes::MESH_BACKEND_UNAVAILABLE))
    }
}

/// Per-material decision for the packer (step 2 of the flow).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialAnalysis {
    pub material_index: usize,
    /// UV channel of the material's base-colour texture (else its first texture).
    pub uv_channel: u32,
    pub range: UvRangeReport,
    pub plan: uv_remap::MaterialPlan,
}

/// Analyse every textured material that is used by at least one mesh.
/// Out-of-range materials skipped by the policy get `MaterialPlan::Skip` and a
/// `MESH_UV_OUT_OF_RANGE` warning; materials whose meshes lack the UV channel
/// are left out of the result with a `MESH_NO_UVS` warning. A `bakeRepeat`
/// overflow is a hard error (`MESH_UV_TOO_MANY_TILES`).
pub fn analyze_materials(
    model: &Model,
    policy: OutOfRangePolicy,
) -> OpResult<(Vec<MaterialAnalysis>, Vec<OpError>)> {
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    for (mi, mat) in model.materials.iter().enumerate() {
        let Some(tex) = mat
            .textures
            .get(&TextureChannel::BaseColor)
            .or_else(|| mat.textures.values().next())
        else {
            continue;
        };
        if !model.meshes_with_material(mi).any(|m| m.vertex_count > 0) {
            continue;
        }
        let uv_channel = tex.uv_channel;
        let Some(range) = model.material_uv_range(mi, uv_channel) else {
            warnings.push(
                OpError::new(codes::MESH_NO_UVS)
                    .with("material", mat.name.clone())
                    .with("uvChannel", uv_channel),
            );
            continue;
        };
        let plan = uv_remap::plan_material(&range, policy)
            .map_err(|e| e.with("material", mat.name.clone()))?;
        if plan == uv_remap::MaterialPlan::Skip {
            warnings.push(
                OpError::new(codes::MESH_UV_OUT_OF_RANGE)
                    .with("material", mat.name.clone())
                    .with("min", serde_json::json!(range.min))
                    .with("max", serde_json::json!(range.max)),
            );
        }
        out.push(MaterialAnalysis {
            material_index: mi,
            uv_channel,
            range,
            plan,
        });
    }
    Ok((out, warnings))
}

/// `uvs[mesh_index] = Some(new UVs of the remapped channel)`, `None` for
/// meshes whose material is not remapped.
pub type MeshUvs = Vec<Option<Vec<[f32; 2]>>>;

/// Compute the remapped UVs of every mesh (pure; used by [`export`]), plus
/// warnings (`MESH_UV_WRAP_STRADDLE`).
pub fn remap_model_uvs(model: &Model, remaps: &ModelRemaps) -> OpResult<(MeshUvs, Vec<OpError>)> {
    let mut warnings = Vec::new();
    let mut out = Vec::with_capacity(model.meshes.len());
    for mesh in &model.meshes {
        let Some(r) = remaps.get(&mesh.material_index) else {
            out.push(None);
            continue;
        };
        let uvs = mesh
            .uv_channels
            .get(r.uv_channel as usize)
            .filter(|c| !c.is_empty() || mesh.vertex_count == 0)
            .ok_or_else(|| {
                let material = model
                    .materials
                    .get(mesh.material_index)
                    .map(|m| m.name.clone());
                OpError::new(codes::MESH_NO_UVS)
                    .with("mesh", mesh.name.clone())
                    .with("material", material.unwrap_or_default())
                    .with("uvChannel", r.uv_channel)
            })?;
        let res = r.remap.apply_mesh(uvs, &mesh.faces);
        if res.straddling_faces > 0 || res.conflicting_vertices > 0 {
            warnings.push(
                OpError::new(codes::MESH_UV_WRAP_STRADDLE)
                    .with("mesh", mesh.name.clone())
                    .with("faces", res.straddling_faces)
                    .with("vertices", res.conflicting_vertices),
            );
        }
        out.push(Some(res.uvs));
    }
    Ok((out, warnings))
}

/// Check that every texture sampled through a remapped UV channel has an
/// atlas (otherwise it would be sampled with atlas UVs → garbage).
pub fn validate_atlas_channels(
    model: &Model,
    remaps: &ModelRemaps,
    atlas: &BTreeMap<TextureChannel, String>,
) -> OpResult<()> {
    for (&mi, r) in remaps {
        let mat = model
            .materials
            .get(mi)
            .ok_or_else(|| OpError::invalid_param("remaps", "materialIndexOutOfRange"))?;
        for (channel, tex) in &mat.textures {
            if tex.uv_channel == r.uv_channel && !atlas.contains_key(channel) {
                return Err(OpError::new(codes::MESH_ATLAS_CHANNEL_MISSING)
                    .with("material", mat.name.clone())
                    .with("channel", channel.as_key()));
            }
        }
    }
    Ok(())
}

/// Write the remapped model to `out_path` in `options.format`.
pub fn export(
    loaded: &LoadedModel,
    remaps: &ModelRemaps,
    options: &ExportOptions,
    out_path: &Path,
) -> OpResult<ExportReport> {
    validate_atlas_channels(&loaded.model, remaps, &options.atlas_textures)?;
    if options.merge_materials {
        let mut channels = remaps.values().map(|r| r.uv_channel);
        if let Some(first) = channels.next()
            && channels.any(|c| c != first)
        {
            return Err(OpError::invalid_param("remaps", "uvChannelMismatch"));
        }
    }
    let (uvs, warnings) = remap_model_uvs(&loaded.model, remaps)?;
    if let Some(dir) = out_path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| {
            OpError::new(crate::error::codes::IO_WRITE_FAILED)
                .with("path", dir.display().to_string())
                .with("detail", e.to_string())
        })?;
    }
    #[cfg(feature = "assimp")]
    {
        assimp::export(
            &loaded.scene,
            &loaded.model,
            remaps,
            &uvs,
            options,
            out_path,
        )?;
        Ok(ExportReport { warnings })
    }
    #[cfg(not(feature = "assimp"))]
    {
        let _ = (uvs, warnings);
        Err(OpError::new(codes::MESH_BACKEND_UNAVAILABLE))
    }
}

/// Write embedded textures as image files into `out_dir`; returns
/// `(embedded index, written path)`. Compressed payloads are written as-is
/// (`.png`, `.jpg`, …); raw texel arrays are encoded as PNG.
pub fn extract_embedded_textures(
    loaded: &LoadedModel,
    out_dir: &Path,
) -> OpResult<Vec<(usize, PathBuf)>> {
    #[cfg(feature = "assimp")]
    {
        assimp::extract_embedded(&loaded.scene, &loaded.model, out_dir)
    }
    #[cfg(not(feature = "assimp"))]
    {
        let _ = (loaded, out_dir);
        Err(OpError::new(codes::MESH_BACKEND_UNAVAILABLE))
    }
}

/// Assimp version string of the linked backend, `None` without the feature.
pub fn backend_version() -> Option<String> {
    #[cfg(feature = "assimp")]
    {
        Some(assimp::version())
    }
    #[cfg(not(feature = "assimp"))]
    {
        None
    }
}

/// Assimp exporter ids compiled into the backend (empty without the feature).
pub fn available_export_formats() -> Vec<String> {
    #[cfg(feature = "assimp")]
    {
        assimp::export_format_ids()
    }
    #[cfg(not(feature = "assimp"))]
    {
        Vec::new()
    }
}
