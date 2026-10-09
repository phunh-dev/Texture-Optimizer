//! 3D Model Texture Packer: the whole pipeline behind the "3D Texture Packer"
//! tab, independent of Tauri.
//!
//! 1. [`prepare`]: import every model, analyse each textured material's UVs
//!    with the [`OutOfRangePolicy`](crate::mesh::OutOfRangePolicy), collect
//!    the textures it samples through that UV set (embedded ones are
//!    extracted to a temp folder).
//! 2. Layout: one block per material (materials of a model sharing the same
//!    textures share a block), sized from the base-colour texture (else the
//!    largest one) × texture scale × bake-repeat tiles; packed POT, never
//!    rotated (see `layout.rs`).
//! 3. Compose one atlas per channel with the same rects; a channel texture
//!    whose size differs from the block is resampled (`MESH_TEXTURE_RESIZED`),
//!    a missing one is filled with the channel default colour.
//! 4. Remap: [`atlas_transform`](crate::mesh::uv_remap::atlas_transform) of
//!    the block (bottom-left UV origin, Assimp convention) after the
//!    material's UV normalisation.
//! 5. Output: rewritten models ([`ModelExporter`]) or UV Remap Data sidecars
//!    plus the Unity script; geometry-check failures fall back to UV Remap
//!    Data per model. A `<base>.report.json` lists everything.
//!
//! [`preview`] runs steps 1–4 for the base-colour channel only and writes
//! nothing.

mod compose;
pub mod info;
mod layout;
pub mod options;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use compose::preview_downscale;
pub use info::{
    MaterialInfo, ModelInfo, TextureInfo, describe, find_models, inspect, is_model_extension,
    material_uv_channel, model_file_entry,
};
pub use options::{
    FormatChoice, OutputMode, OutputOptions, PackOptions, builtin_default, default_channels,
    parse_hex_color,
};

use crate::mesh::model::{ModelFormat, TextureChannel, relative_path};
use crate::mesh::remap_json::{
    self, AtlasPage, ChannelPath, MaterialRemapEntry, ModelRemapEntry, RemapFile,
};
use crate::mesh::uv_remap::{
    AtlasRect, MaterialPlan, UvNormalize, UvOrigin, UvRangeReport, UvRemap, atlas_transform,
    plan_material,
};
use crate::mesh::{
    ExportFormat, ExportOptions, ExportReport, LoadedModel, MaterialRemap, ModelRemaps,
    codes as mesh_codes,
};
use crate::output::{PngCompression, encode_png, write_atomic};
use crate::{ImageBuf, OpError, OpResult};

/// Codes added by the packer (translated via `errors:<CODE>`).
pub mod codes {
    /// No model was given.
    pub const MESH_NO_MODELS: &str = "MESH_NO_MODELS";
    /// No material of any model can be atlased.
    pub const MESH_NOTHING_TO_PACK: &str = "MESH_NOTHING_TO_PACK";
    /// Warning: a channel used by an atlased material was not selected and
    /// was packed anyway (its texture would be sampled with atlas UVs).
    /// Params: `channel`.
    pub const MESH_CHANNEL_FORCED: &str = "MESH_CHANNEL_FORCED";
    /// Warning: a channel texture has another size than the material's
    /// layout texture and was resampled. Params: `material`, `channel`,
    /// `width`, `height`, `targetWidth`, `targetHeight`.
    pub const MESH_TEXTURE_RESIZED: &str = "MESH_TEXTURE_RESIZED";
    /// Warning: a texture file exists but could not be read. Params:
    /// `material`, `channel`, `path`, `detail`.
    pub const MESH_TEXTURE_UNREADABLE: &str = "MESH_TEXTURE_UNREADABLE";
    /// Warning: none of the material's textures could be loaded; the
    /// material was left out of the atlas. Params: `material`.
    pub const MESH_MATERIAL_NO_TEXTURES: &str = "MESH_MATERIAL_NO_TEXTURES";
    /// The model's materials landed on several atlas pages, which a single
    /// model file cannot reference. Params: `model`, `pages`.
    pub const MESH_MODEL_SPANS_PAGES: &str = "MESH_MODEL_SPANS_PAGES";
    /// Warning: the model was written as UV Remap Data instead of being
    /// rewritten. Params: `model`, `reason` (code of the cause).
    pub const MESH_FALLBACK_REMAP_DATA: &str = "MESH_FALLBACK_REMAP_DATA";
    /// Warning: textures were downscaled to fit one page. Params: `percent`.
    pub const MESH_TEXTURES_DOWNSCALED: &str = "MESH_TEXTURES_DOWNSCALED";
    /// Warning: a model has no material that could be atlased; it was not
    /// written. Params: `model`.
    pub const MESH_MODEL_SKIPPED: &str = "MESH_MODEL_SKIPPED";
}

/// Report schema version.
pub const REPORT_VERSION: u32 = 1;
/// UVs from `mesh::import` use the bottom-left origin (Assimp/OpenGL).
pub const UV_ORIGIN: UvOrigin = UvOrigin::BottomLeft;

// ------------------------------------------------------------------ report

/// How a material was treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MaterialStatus {
    /// UVs inside [0,1]; atlased.
    InRange,
    /// Out of range, clamped (clamp policy); atlased.
    Clamped,
    /// Out of range, faces moved into the unit tile; atlased.
    Wrapped,
    /// Out of range, texture baked repeated into its block; atlased.
    Repeated,
    /// Out of range and left out (skipMaterial policy).
    Skipped,
    /// bakeRepeat needed more tiles than allowed; left out.
    TooManyTiles,
    /// The meshes lack the UV channel; left out.
    NoUvs,
    /// No loadable texture; left out.
    NoTextures,
}

impl MaterialStatus {
    pub fn packed(self) -> bool {
        matches!(
            self,
            Self::InRange | Self::Clamped | Self::Wrapped | Self::Repeated
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialReport {
    pub material_index: usize,
    pub name: String,
    pub status: MaterialStatus,
    pub uv_channel: Option<u32>,
    pub uv_range: Option<UvRangeReport>,
    /// Channels of the textures sampled through `uv_channel`.
    pub channels: Vec<TextureChannel>,
    /// Channel whose texture defined the block size.
    pub layout_channel: Option<TextureChannel>,
    /// Size of that texture.
    pub source_size: Option<[u32; 2]>,
    /// Size of one tile in the atlas (after scaling).
    pub tile_size: Option<[u32; 2]>,
    pub tiles: [u32; 2],
    pub page: Option<usize>,
    /// Block in atlas pixels (top-left origin), identical for every channel.
    pub rect: Option<AtlasRect>,
    pub remap: Option<UvRemap>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelOutcome {
    /// Remapped model written.
    Rewritten,
    /// Rewriting failed the geometry check (or needed several pages); UV
    /// Remap Data written instead.
    Fallback,
    /// UV Remap Data written (chosen mode).
    RemapData,
    /// Nothing to atlas in this model.
    Skipped,
    /// Import or export failed (`error`).
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelReport {
    pub source: String,
    pub name: String,
    pub format: Option<ModelFormat>,
    pub outcome: ModelOutcome,
    /// Written model (rewritten file, or the copy next to the sidecar).
    pub output: Option<String>,
    pub output_format: Option<ExportFormat>,
    pub sidecar: Option<String>,
    /// Every file written for this model.
    pub files: Vec<String>,
    /// Atlas pages used by the model.
    pub pages: Vec<usize>,
    pub materials: Vec<MaterialReport>,
    pub warnings: Vec<OpError>,
    pub error: Option<OpError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageReport {
    pub index: usize,
    pub width: u32,
    pub height: u32,
    /// Fraction of the page covered by blocks (without padding/extrude).
    pub occupancy: f64,
    /// Written atlas file per channel (empty for previews).
    pub textures: Vec<ChannelPath>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackReport {
    pub version: u32,
    pub generator: String,
    pub uv_origin: UvOrigin,
    /// Effective texture scale in percent (after `scaleToFit`).
    pub scale_percent: f64,
    /// Channels that got an atlas.
    pub channels: Vec<TextureChannel>,
    pub pages: Vec<PageReport>,
    pub models: Vec<ModelReport>,
    /// Every file written (atlases, models, sidecars, script, report).
    pub files: Vec<String>,
    pub report_path: Option<String>,
    pub warnings: Vec<OpError>,
}

impl PackReport {
    pub fn count(&self, outcome: ModelOutcome) -> usize {
        self.models.iter().filter(|m| m.outcome == outcome).count()
    }
}

/// Progress callback payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackProgress {
    pub done: usize,
    pub total: usize,
    pub current: Option<String>,
}

// ---------------------------------------------------------------- exporter

/// Writes a remapped model. The default goes through Assimp; tests inject
/// failures (e.g. `MESH_EXPORT_GEOMETRY_CHANGED`) through this seam.
pub trait ModelExporter {
    fn export(
        &self,
        loaded: &LoadedModel,
        remaps: &ModelRemaps,
        options: &ExportOptions,
        out_path: &Path,
    ) -> OpResult<ExportReport>;
}

/// [`crate::mesh::export`].
pub struct AssimpExporter;

impl ModelExporter for AssimpExporter {
    fn export(
        &self,
        loaded: &LoadedModel,
        remaps: &ModelRemaps,
        options: &ExportOptions,
        out_path: &Path,
    ) -> OpResult<ExportReport> {
        crate::mesh::export(loaded, remaps, options, out_path)
    }
}

// ----------------------------------------------------------------- prepare

#[derive(Debug, Clone)]
struct TexSource {
    path: PathBuf,
    size: [u32; 2],
}

/// Textures shared by one or more materials of a model → one atlas block.
#[derive(Debug, Clone)]
struct Slot {
    /// Loadable textures per channel.
    textures: BTreeMap<TextureChannel, TexSource>,
    /// Channels referenced on the remapped UV set (loadable or not).
    channels: BTreeSet<TextureChannel>,
    layout_channel: TextureChannel,
    source_size: [u32; 2],
    tiles: [u32; 2],
    /// First material name (for messages).
    label: String,
}

#[derive(Debug, Clone)]
struct PreparedMaterial {
    index: usize,
    name: String,
    status: MaterialStatus,
    uv_channel: Option<u32>,
    range: Option<UvRangeReport>,
    normalize: UvNormalize,
    channels: Vec<TextureChannel>,
    slot: Option<usize>,
}

struct PreparedModel {
    source: PathBuf,
    name: String,
    loaded: Option<LoadedModel>,
    error: Option<OpError>,
    warnings: Vec<OpError>,
    materials: Vec<PreparedMaterial>,
    slots: Vec<Slot>,
    /// Keeps extracted embedded textures alive.
    _tmp: Option<tempfile::TempDir>,
}

impl PreparedModel {
    fn failed(source: &Path, error: OpError) -> Self {
        Self {
            source: source.to_path_buf(),
            name: file_name(source),
            loaded: None,
            error: Some(error),
            warnings: Vec::new(),
            materials: Vec::new(),
            slots: Vec::new(),
            _tmp: None,
        }
    }
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn status_of(normalize: UvNormalize) -> MaterialStatus {
    match normalize {
        UvNormalize::None => MaterialStatus::InRange,
        UvNormalize::Clamp => MaterialStatus::Clamped,
        UvNormalize::Wrap => MaterialStatus::Wrapped,
        UvNormalize::Repeat { .. } => MaterialStatus::Repeated,
    }
}

fn prepare(source: &Path, options: &PackOptions) -> PreparedModel {
    let loaded = match crate::mesh::import(source) {
        Ok(l) => l,
        Err(e) => return PreparedModel::failed(source, e),
    };
    let model = &loaded.model;
    let mut warnings = model.warnings.clone();

    // Embedded textures → temp files.
    let mut embedded: HashMap<usize, PathBuf> = HashMap::new();
    let mut tmp = None;
    let needs_embedded = model
        .materials
        .iter()
        .flat_map(|m| m.textures.values())
        .any(|t| t.embedded_index.is_some());
    if needs_embedded {
        match tempfile::tempdir() {
            Ok(dir) => {
                match crate::mesh::extract_embedded_textures(&loaded, dir.path()) {
                    Ok(list) => embedded.extend(list),
                    Err(e) => warnings.push(e),
                }
                tmp = Some(dir);
            }
            Err(e) => warnings.push(
                OpError::new(crate::error::codes::IO_WRITE_FAILED)
                    .with("path", std::env::temp_dir().display().to_string())
                    .with("detail", e.to_string()),
            ),
        }
    }

    let mut materials = Vec::new();
    let mut slots: Vec<Slot> = Vec::new();
    let mut slot_keys: HashMap<String, usize> = HashMap::new();
    for (mi, mat) in model.materials.iter().enumerate() {
        if !model.meshes_with_material(mi).any(|m| m.vertex_count > 0) || mat.textures.is_empty() {
            continue;
        }
        let uv_channel = info::material_uv_channel(model, mi);
        let mut pm = PreparedMaterial {
            index: mi,
            name: mat.name.clone(),
            status: MaterialStatus::NoTextures,
            uv_channel,
            range: None,
            normalize: UvNormalize::None,
            channels: Vec::new(),
            slot: None,
        };
        let Some(uv) = uv_channel else {
            materials.push(pm);
            continue;
        };
        pm.channels = mat
            .textures
            .iter()
            .filter(|(_, t)| t.uv_channel == uv)
            .map(|(c, _)| c.clone())
            .collect();
        let Some(range) = model.material_uv_range(mi, uv) else {
            warnings.push(
                OpError::new(mesh_codes::MESH_NO_UVS)
                    .with("material", mat.name.clone())
                    .with("uvChannel", uv),
            );
            pm.status = MaterialStatus::NoUvs;
            materials.push(pm);
            continue;
        };
        pm.range = Some(range);
        let normalize = match plan_material(&range, options.out_of_range) {
            Err(e) => {
                warnings.push(e.with("material", mat.name.clone()));
                pm.status = MaterialStatus::TooManyTiles;
                materials.push(pm);
                continue;
            }
            Ok(MaterialPlan::Skip) => {
                warnings.push(
                    OpError::new(mesh_codes::MESH_UV_OUT_OF_RANGE)
                        .with("material", mat.name.clone())
                        .with("min", serde_json::json!(range.min))
                        .with("max", serde_json::json!(range.max)),
                );
                pm.status = MaterialStatus::Skipped;
                materials.push(pm);
                continue;
            }
            Ok(MaterialPlan::Remap { normalize }) => normalize,
        };
        pm.normalize = normalize;
        let tiles = MaterialPlan::Remap { normalize }.repeat_tiles();

        let mut loadable: BTreeMap<TextureChannel, TexSource> = BTreeMap::new();
        for (channel, tex) in mat.textures.iter().filter(|(_, t)| t.uv_channel == uv) {
            let path = match tex.embedded_index.and_then(|i| embedded.get(&i)) {
                Some(p) => p.clone(),
                None if tex.exists => tex.path.clone(),
                None => continue, // MESH_TEXTURE_NOT_FOUND already reported by import
            };
            match image::image_dimensions(&path) {
                Ok((w, h)) if w > 0 && h > 0 => {
                    loadable.insert(channel.clone(), TexSource { path, size: [w, h] });
                }
                Ok(_) | Err(_) => warnings.push(
                    OpError::new(codes::MESH_TEXTURE_UNREADABLE)
                        .with("material", mat.name.clone())
                        .with("channel", channel.as_key())
                        .with("path", path.display().to_string())
                        .with("detail", "unreadable image header"),
                ),
            }
        }
        let layout_channel = if loadable.contains_key(&TextureChannel::BaseColor) {
            Some(TextureChannel::BaseColor)
        } else {
            loadable
                .iter()
                .max_by_key(|(c, t)| {
                    (
                        u64::from(t.size[0]) * u64::from(t.size[1]),
                        std::cmp::Reverse((*c).clone()),
                    )
                })
                .map(|(c, _)| c.clone())
        };
        let Some(layout_channel) = layout_channel else {
            warnings.push(
                OpError::new(codes::MESH_MATERIAL_NO_TEXTURES).with("material", mat.name.clone()),
            );
            pm.status = MaterialStatus::NoTextures;
            materials.push(pm);
            continue;
        };
        pm.status = status_of(normalize);
        // Materials with the same textures (and tiling) share one block.
        let key = format!(
            "{tiles:?}|{}",
            mat.textures
                .iter()
                .filter(|(_, t)| t.uv_channel == uv)
                .map(|(c, t)| format!("{}={}", c.as_key(), t.path.display()))
                .collect::<Vec<_>>()
                .join("|")
        );
        let slot = *slot_keys.entry(key).or_insert_with(|| {
            slots.push(Slot {
                source_size: loadable[&layout_channel].size,
                textures: loadable,
                channels: pm.channels.iter().cloned().collect(),
                layout_channel,
                tiles,
                label: mat.name.clone(),
            });
            slots.len() - 1
        });
        pm.slot = Some(slot);
        materials.push(pm);
    }

    PreparedModel {
        source: source.to_path_buf(),
        name: file_name(source),
        loaded: Some(loaded),
        error: None,
        warnings,
        materials,
        slots,
        _tmp: tmp,
    }
}

// ------------------------------------------------------------------- plan

/// Everything decided before pixels are composed.
struct Plan {
    models: Vec<PreparedModel>,
    /// `(model, slot)` per layout item.
    items: Vec<(usize, usize)>,
    /// Per item: page + rect + tile size.
    placed: Vec<(usize, AtlasRect, [u32; 2])>,
    pages: Vec<layout::PageSize>,
    scale_percent: f64,
    channels: Vec<TextureChannel>,
    warnings: Vec<OpError>,
}

impl Plan {
    fn slot_placement(&self, model: usize, slot: usize) -> Option<(usize, AtlasRect, [u32; 2])> {
        self.items
            .iter()
            .position(|&it| it == (model, slot))
            .map(|i| self.placed[i])
    }

    fn remap_of(
        &self,
        model: usize,
        pm: &PreparedMaterial,
        options: &PackOptions,
    ) -> OpResult<Option<(usize, MaterialRemap)>> {
        let (Some(slot), Some(uv)) = (pm.slot, pm.uv_channel) else {
            return Ok(None);
        };
        let Some((page, rect, _)) = self.slot_placement(model, slot) else {
            return Ok(None);
        };
        let size = self.pages[page];
        let t = atlas_transform(rect, [size.width, size.height], options.inset, UV_ORIGIN)?;
        Ok(Some((
            page,
            MaterialRemap {
                uv_channel: uv,
                remap: UvRemap::new(pm.normalize, t),
            },
        )))
    }

    fn model_pages(&self, model: usize) -> Vec<usize> {
        let set: BTreeSet<usize> = self
            .items
            .iter()
            .zip(&self.placed)
            .filter(|((m, _), _)| *m == model)
            .map(|(_, p)| p.0)
            .collect();
        set.into_iter().collect()
    }
}

fn scaled_tile(size: [u32; 2], scale: f64, min_edge: u32) -> [u32; 2] {
    let s = |v: u32| ((f64::from(v) * scale).round() as u32).max(min_edge);
    [s(size[0]), s(size[1])]
}

fn build_plan(
    models: &[PathBuf],
    options: &PackOptions,
    progress: &mut dyn FnMut(PackProgress),
    total: usize,
) -> OpResult<Plan> {
    options.validate()?;
    if models.is_empty() {
        return Err(OpError::new(codes::MESH_NO_MODELS));
    }
    let mut prepared = Vec::with_capacity(models.len());
    for (i, path) in models.iter().enumerate() {
        prepared.push(prepare(path, options));
        progress(PackProgress {
            done: i + 1,
            total,
            current: Some(path.display().to_string()),
        });
    }
    let items: Vec<(usize, usize)> = prepared
        .iter()
        .enumerate()
        .flat_map(|(m, pm)| (0..pm.slots.len()).map(move |s| (m, s)))
        .collect();
    if items.is_empty() {
        return Err(OpError::new(codes::MESH_NOTHING_TO_PACK));
    }

    let min_edge = options.min_tile_edge();
    let make = |scale: f64| -> Vec<layout::Item> {
        items
            .iter()
            .map(|&(m, s)| {
                let slot = &prepared[m].slots[s];
                let tile = scaled_tile(slot.source_size, scale, min_edge);
                layout::Item {
                    model: m,
                    w: tile[0] * slot.tiles[0],
                    h: tile[1] * slot.tiles[1],
                    label: format!("{} / {}", prepared[m].name, slot.label),
                }
            })
            .collect()
    };
    let planned = layout::plan_layout(options, &make)?;
    let mut warnings = Vec::new();
    let scale_percent = f64::from(options.texture_scale) / f64::from(planned.divisor);
    if planned.divisor > 1 {
        warnings.push(OpError::new(codes::MESH_TEXTURES_DOWNSCALED).with("percent", scale_percent));
    }
    let placed = items
        .iter()
        .zip(&planned.layout.placed)
        .map(|(&(m, s), &(page, rect))| {
            let tiles = prepared[m].slots[s].tiles;
            (page, rect, [rect.width / tiles[0], rect.height / tiles[1]])
        })
        .collect();

    // Atlas channels: every channel an atlased material samples through its
    // remapped UV set (otherwise it would read garbage), selected or not.
    let present: BTreeSet<TextureChannel> = prepared
        .iter()
        .flat_map(|pm| pm.slots.iter().flat_map(|s| s.channels.iter().cloned()))
        .collect();
    let selected: BTreeSet<TextureChannel> = options.channels.iter().cloned().collect();
    for forced in present.difference(&selected) {
        warnings.push(OpError::new(codes::MESH_CHANNEL_FORCED).with("channel", forced.as_key()));
    }
    let mut channels: Vec<TextureChannel> = present.into_iter().collect();
    // Base colour first (preview + report readability).
    channels.sort_by_key(|c| (*c != TextureChannel::BaseColor, c.clone()));

    Ok(Plan {
        models: prepared,
        items,
        placed,
        pages: planned.layout.pages,
        scale_percent,
        channels,
        warnings,
    })
}

// ---------------------------------------------------------------- compose

/// Atlas page `page` of `channel`. Pushes `MESH_TEXTURE_RESIZED` /
/// `MESH_TEXTURE_UNREADABLE` warnings into `model_warnings[model]` when
/// `warn` is set.
fn compose_page(
    plan: &Plan,
    page: usize,
    channel: &TextureChannel,
    options: &PackOptions,
    mut model_warnings: Option<&mut [Vec<OpError>]>,
) -> ImageBuf {
    let size = plan.pages[page];
    let mut img = ImageBuf::new(size.width, size.height);
    let fill = options.default_color(channel);
    let mut cache: HashMap<PathBuf, Option<ImageBuf>> = HashMap::new();
    for (&(m, s), &(p, rect, tile)) in plan.items.iter().zip(&plan.placed) {
        if p != page {
            continue;
        }
        let slot = &plan.models[m].slots[s];
        let tile_img = match slot.textures.get(channel) {
            Some(src) => {
                let loaded = cache
                    .entry(src.path.clone())
                    .or_insert_with(|| crate::io::load_image(&src.path).ok());
                match loaded {
                    Some(img) => {
                        if src.size != slot.source_size
                            && let Some(w) = model_warnings.as_deref_mut()
                        {
                            w[m].push(
                                OpError::new(codes::MESH_TEXTURE_RESIZED)
                                    .with("material", slot.label.clone())
                                    .with("channel", channel.as_key())
                                    .with("width", src.size[0])
                                    .with("height", src.size[1])
                                    .with("targetWidth", slot.source_size[0])
                                    .with("targetHeight", slot.source_size[1]),
                            );
                        }
                        compose::fit(img, tile)
                    }
                    None => {
                        if let Some(w) = model_warnings.as_deref_mut() {
                            w[m].push(
                                OpError::new(codes::MESH_TEXTURE_UNREADABLE)
                                    .with("material", slot.label.clone())
                                    .with("channel", channel.as_key())
                                    .with("path", src.path.display().to_string())
                                    .with("detail", "decode failed"),
                            );
                        }
                        compose::solid(tile, fill)
                    }
                }
            }
            None => compose::solid(tile, fill),
        };
        let block = compose::repeat(&tile_img, slot.tiles);
        debug_assert_eq!(block.dimensions(), (rect.width, rect.height));
        compose::blit_extruded(&mut img, &block, rect.x, rect.y, options.extrude);
    }
    img
}

fn occupancy(plan: &Plan, page: usize) -> f64 {
    let size = plan.pages[page];
    let used: u64 = plan
        .placed
        .iter()
        .filter(|p| p.0 == page)
        .map(|p| u64::from(p.1.width) * u64::from(p.1.height))
        .sum();
    used as f64 / (f64::from(size.width) * f64::from(size.height))
}

fn material_reports(plan: &Plan, model: usize, options: &PackOptions) -> Vec<MaterialReport> {
    let pm = &plan.models[model];
    pm.materials
        .iter()
        .map(|mat| {
            let slot = mat.slot.map(|s| &pm.slots[s]);
            let placement = mat.slot.and_then(|s| plan.slot_placement(model, s));
            let remap = plan
                .remap_of(model, mat, options)
                .ok()
                .flatten()
                .map(|(_, r)| r.remap);
            MaterialReport {
                material_index: mat.index,
                name: mat.name.clone(),
                status: mat.status,
                uv_channel: mat.uv_channel,
                uv_range: mat.range,
                channels: mat.channels.clone(),
                layout_channel: slot.map(|s| s.layout_channel.clone()),
                source_size: slot.map(|s| s.source_size),
                tile_size: placement.map(|p| p.2),
                tiles: slot.map_or([1, 1], |s| s.tiles),
                page: placement.map(|p| p.0),
                rect: placement.map(|p| p.1),
                remap,
            }
        })
        .collect()
}

fn base_report(plan: &Plan, options: &PackOptions) -> PackReport {
    PackReport {
        version: REPORT_VERSION,
        generator: remap_json::GENERATOR.into(),
        uv_origin: UV_ORIGIN,
        scale_percent: plan.scale_percent,
        channels: plan.channels.clone(),
        pages: (0..plan.pages.len())
            .map(|i| PageReport {
                index: i,
                width: plan.pages[i].width,
                height: plan.pages[i].height,
                occupancy: occupancy(plan, i),
                textures: Vec::new(),
            })
            .collect(),
        models: (0..plan.models.len())
            .map(|m| {
                let pm = &plan.models[m];
                let packed = pm.materials.iter().any(|x| x.slot.is_some());
                ModelReport {
                    source: pm.source.display().to_string(),
                    name: pm.name.clone(),
                    format: pm.loaded.as_ref().map(|l| l.model.format),
                    outcome: if pm.error.is_some() {
                        ModelOutcome::Failed
                    } else if !packed {
                        ModelOutcome::Skipped
                    } else if options.output.mode == OutputMode::UvRemapData {
                        ModelOutcome::RemapData
                    } else {
                        ModelOutcome::Rewritten
                    },
                    output: None,
                    output_format: None,
                    sidecar: None,
                    files: Vec::new(),
                    pages: plan.model_pages(m),
                    materials: material_reports(plan, m, options),
                    warnings: pm.warnings.clone(),
                    error: pm.error.clone(),
                }
            })
            .collect(),
        files: Vec::new(),
        report_path: None,
        warnings: plan.warnings.clone(),
    }
}

// ----------------------------------------------------------------- preview

/// Layout + base-colour atlas pages (first atlas channel when no material
/// has a base colour), nothing written.
pub struct PackPreview {
    pub report: PackReport,
    pub channel: TextureChannel,
    pub pages: Vec<ImageBuf>,
}

pub fn preview(
    models: &[PathBuf],
    options: &PackOptions,
    progress: &mut dyn FnMut(PackProgress),
) -> OpResult<PackPreview> {
    let total = models.len() + 1;
    let plan = build_plan(models, options, progress, total)?;
    let channel = plan
        .channels
        .first()
        .cloned()
        .unwrap_or(TextureChannel::BaseColor);
    let mut model_warnings: Vec<Vec<OpError>> = vec![Vec::new(); plan.models.len()];
    let pages = (0..plan.pages.len())
        .map(|p| {
            compose_page(
                &plan,
                p,
                &channel,
                options,
                Some(model_warnings.as_mut_slice()),
            )
        })
        .collect();
    let mut report = base_report(&plan, options);
    for (m, w) in model_warnings.into_iter().enumerate() {
        report.models[m].warnings.extend(w);
    }
    progress(PackProgress {
        done: total,
        total,
        current: None,
    });
    Ok(PackPreview {
        report,
        channel,
        pages,
    })
}

// --------------------------------------------------------------------- run

/// Base name of output files: non-empty, no path separators or characters
/// Windows rejects.
pub fn validate_base_name(base: &str) -> OpResult<()> {
    let bad = base.trim().is_empty()
        || base != base.trim()
        || base.ends_with('.')
        || base.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        });
    if bad {
        return Err(OpError::invalid_param("baseName", "invalid"));
    }
    Ok(())
}

/// `<base>_<channel>.png` (page 1) or `<base>_p<N>_<channel>.png`.
pub fn atlas_file_name(base: &str, page: usize, channel: &TextureChannel) -> String {
    let key: String = channel
        .as_key()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if page == 0 {
        format!("{base}_{key}.png")
    } else {
        format!("{base}_p{}_{key}.png", page + 1)
    }
}

/// Picks file names inside the output folder that collide neither with each
/// other (case-insensitive) nor with the source models.
struct NamePicker {
    used: HashSet<String>,
    protected: HashSet<PathBuf>,
}

impl NamePicker {
    fn reserve(&mut self, name: &str) {
        self.used.insert(name.to_lowercase());
    }

    /// Names reserved together with a model file: the model, its sidecar
    /// (`<stem>.uvremap.json`) and the OBJ material library (`<stem>.mtl`).
    fn companions(stem: &str, ext: &str) -> [String; 3] {
        [
            format!("{stem}.{ext}").to_lowercase(),
            format!("{stem}{}", remap_json::SIDECAR_SUFFIX).to_lowercase(),
            format!("{stem}.mtl").to_lowercase(),
        ]
    }

    fn pick(&mut self, dir: &Path, stem: &str, ext: &str) -> PathBuf {
        let mut n = 1;
        loop {
            let s = match n {
                1 => stem.to_string(),
                _ => format!("{stem}_{n}"),
            };
            let names = Self::companions(&s, ext);
            let path = dir.join(format!("{s}.{ext}"));
            let is_source = self.protected.contains(&normalized(&path));
            if !is_source && names.iter().all(|x| !self.used.contains(x)) {
                self.used.extend(names);
                return path;
            }
            n += 1;
        }
    }

    /// Undo [`NamePicker::pick`] (the file was not written).
    fn release(&mut self, path: &Path) {
        let ext = crate::io::extension_of(path);
        for name in Self::companions(&stem_of(path), &ext) {
            self.used.remove(&name);
        }
    }
}

fn normalized(p: &Path) -> PathBuf {
    let s = crate::io::normalize_path(p).unwrap_or_else(|_| p.to_path_buf());
    PathBuf::from(s.display().to_string().to_lowercase())
}

fn stem_of(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "model".into())
}

fn write_text(path: &Path, text: &str) -> OpResult<()> {
    write_atomic(path, text.as_bytes())
}

fn io_err(path: &Path, e: &dyn std::fmt::Display) -> OpError {
    OpError::new(crate::error::codes::IO_WRITE_FAILED)
        .with("path", path.display().to_string())
        .with("detail", e.to_string())
}

struct Writer<'a> {
    plan: &'a Plan,
    options: &'a PackOptions,
    out_dir: &'a Path,
    names: NamePicker,
    /// Atlas file per (page, channel), relative to `out_dir`.
    atlas_files: Vec<Vec<ChannelPath>>,
    script_written: Option<PathBuf>,
}

impl Writer<'_> {
    fn atlas_pages(&self) -> Vec<AtlasPage> {
        self.plan
            .pages
            .iter()
            .enumerate()
            .map(|(i, p)| AtlasPage {
                index: i as u32,
                width: p.width,
                height: p.height,
                textures: self.atlas_files[i].clone(),
            })
            .collect()
    }

    /// UV Remap Data for one model: optional copy of the source model, the
    /// sidecar and (once) the Unity script.
    fn write_remap_data(&mut self, m: usize, report: &mut ModelReport) -> OpResult<()> {
        let pm = &self.plan.models[m];
        let ext = crate::io::extension_of(&pm.source);
        let model_path = if self.options.output.copy_source_models {
            let dst = self.names.pick(self.out_dir, &stem_of(&pm.source), &ext);
            std::fs::copy(&pm.source, &dst).map_err(|e| io_err(&dst, &e))?;
            report.files.push(dst.display().to_string());
            report.output = Some(dst.display().to_string());
            dst
        } else {
            pm.source.clone()
        };
        let model_name = file_name(&model_path);
        let mut materials = Vec::new();
        for mat in &pm.materials {
            let src_mat = &pm.loaded.as_ref().expect("loaded").model.materials[mat.index];
            match self.plan.remap_of(m, mat, self.options)? {
                Some((page, remap)) => materials.push(MaterialRemapEntry::remapped(
                    mat.index,
                    src_mat,
                    page as u32,
                    &remap,
                )),
                None => materials.push(MaterialRemapEntry::skipped(mat.index, src_mat)),
            }
        }
        let file = RemapFile::new(
            self.atlas_pages(),
            vec![ModelRemapEntry {
                model: model_name,
                merged_material_name: self
                    .options
                    .output
                    .merge_materials
                    .then(|| self.options.output.merged_material_name.clone()),
                materials,
            }],
        );
        let sidecar = if self.options.output.copy_source_models {
            // Reserved together with the copied model.
            self.out_dir.join(format!(
                "{}{}",
                stem_of(&model_path),
                remap_json::SIDECAR_SUFFIX
            ))
        } else {
            // `<stem>.uvremap.json` (`<stem>_2…` if two models share a stem).
            self.names
                .pick(self.out_dir, &stem_of(&model_path), "uvremap.json")
        };
        file.write(&sidecar)?;
        report.files.push(sidecar.display().to_string());
        report.sidecar = Some(sidecar.display().to_string());
        if self.script_written.is_none() {
            let script = self.out_dir.join(remap_json::UNITY_POSTPROCESSOR_FILE_NAME);
            write_text(&script, remap_json::UNITY_POSTPROCESSOR_CS)?;
            self.script_written = Some(script);
        }
        Ok(())
    }

    fn rewrite(
        &mut self,
        m: usize,
        report: &mut ModelReport,
        exporter: &dyn ModelExporter,
    ) -> OpResult<()> {
        let pm = &self.plan.models[m];
        let loaded = pm.loaded.as_ref().expect("loaded");
        let format = self.options.output.format.resolve(loaded.model.format);
        report.output_format = Some(format);
        if report.pages.len() > 1 {
            return Err(OpError::new(codes::MESH_MODEL_SPANS_PAGES)
                .with("model", pm.name.clone())
                .with("pages", report.pages.len()));
        }
        let page = report.pages[0];
        let mut remaps = ModelRemaps::new();
        for mat in &pm.materials {
            if let Some((_, r)) = self.plan.remap_of(m, mat, self.options)? {
                remaps.insert(mat.index, r);
            }
        }
        let out = self
            .names
            .pick(self.out_dir, &stem_of(&pm.source), format.extension());
        let atlas_textures = self.atlas_files[page]
            .iter()
            .map(|cp| {
                (
                    cp.channel.clone(),
                    relative_path(self.out_dir, &self.out_dir.join(&cp.path)),
                )
            })
            .collect();
        let export_options = ExportOptions {
            format,
            merge_materials: self.options.output.merge_materials,
            merged_material_name: Some(self.options.output.merged_material_name.clone()),
            atlas_textures,
            verify_geometry: self.options.output.effective_verify(format),
        };
        let rep = match exporter.export(loaded, &remaps, &export_options, &out) {
            Ok(rep) => rep,
            Err(e) => {
                self.names.release(&out);
                return Err(e);
            }
        };
        report.warnings.extend(rep.warnings);
        report.files.push(out.display().to_string());
        if format == ExportFormat::Obj {
            let mtl = out.with_extension("mtl");
            if mtl.is_file() {
                report.files.push(mtl.display().to_string());
            }
        }
        report.output = Some(out.display().to_string());
        Ok(())
    }
}

/// Full pipeline: writes atlases, models/sidecars and `<base>.report.json`
/// into `out_dir`. Per-model failures are reported, not returned; an `Err`
/// means nothing usable could be produced (bad options, nothing to pack,
/// atlas does not fit, write failure of an atlas).
pub fn run(
    models: &[PathBuf],
    options: &PackOptions,
    out_dir: &Path,
    base_name: &str,
    exporter: &dyn ModelExporter,
    progress: &mut dyn FnMut(PackProgress),
) -> OpResult<PackReport> {
    validate_base_name(base_name)?;
    if out_dir.as_os_str().is_empty() {
        return Err(OpError::invalid_param("outputDir", "empty"));
    }
    let total = 2 * models.len() + 1;
    let plan = build_plan(models, options, progress, total)?;
    std::fs::create_dir_all(out_dir).map_err(|e| io_err(out_dir, &e))?;

    let mut report = base_report(&plan, options);
    let mut model_warnings: Vec<Vec<OpError>> = vec![Vec::new(); plan.models.len()];
    let mut names = NamePicker {
        used: HashSet::new(),
        protected: plan.models.iter().map(|m| normalized(&m.source)).collect(),
    };

    // Atlases.
    let mut atlas_files: Vec<Vec<ChannelPath>> = vec![Vec::new(); plan.pages.len()];
    for (page, files) in atlas_files.iter_mut().enumerate() {
        for channel in &plan.channels {
            let img = compose_page(
                &plan,
                page,
                channel,
                options,
                Some(model_warnings.as_mut_slice()),
            );
            let name = atlas_file_name(base_name, page, channel);
            names.reserve(&name);
            let path = out_dir.join(&name);
            let png = encode_png(&img, PngCompression::Default).map_err(|e| {
                OpError::new(crate::error::codes::IMG_ENCODE_FAILED)
                    .with("path", path.display().to_string())
                    .with("detail", e.to_string())
            })?;
            write_atomic(&path, &png)?;
            report.files.push(path.display().to_string());
            files.push(ChannelPath {
                channel: channel.clone(),
                path: name,
            });
        }
        report.pages[page].textures = files.clone();
    }
    // Resampling warnings are reported once per material/channel.
    for (m, w) in model_warnings.into_iter().enumerate() {
        let mut seen = HashSet::new();
        for warning in w {
            if seen.insert(serde_json::to_string(&warning).unwrap_or_default()) {
                report.models[m].warnings.push(warning);
            }
        }
    }
    let report_name = format!("{base_name}.report.json");
    names.reserve(&report_name);
    progress(PackProgress {
        done: models.len() + 1,
        total,
        current: None,
    });

    let mut writer = Writer {
        plan: &plan,
        options,
        out_dir,
        names,
        atlas_files,
        script_written: None,
    };
    for m in 0..plan.models.len() {
        let mut mr = report.models[m].clone();
        match mr.outcome {
            ModelOutcome::Rewritten => {
                if let Err(e) = writer.rewrite(m, &mut mr, exporter) {
                    let fallback = options.output.auto_fallback
                        && (e.code == mesh_codes::MESH_EXPORT_GEOMETRY_CHANGED
                            || e.code == codes::MESH_MODEL_SPANS_PAGES);
                    if fallback {
                        mr.warnings.push(e.clone());
                        mr.warnings.push(
                            OpError::new(codes::MESH_FALLBACK_REMAP_DATA)
                                .with("model", mr.name.clone())
                                .with("reason", e.code.clone()),
                        );
                        mr.output = None;
                        match writer.write_remap_data(m, &mut mr) {
                            Ok(()) => mr.outcome = ModelOutcome::Fallback,
                            Err(e2) => {
                                mr.outcome = ModelOutcome::Failed;
                                mr.error = Some(e2);
                            }
                        }
                    } else {
                        mr.outcome = ModelOutcome::Failed;
                        mr.output = None;
                        mr.error = Some(e);
                    }
                }
            }
            ModelOutcome::RemapData => {
                if let Err(e) = writer.write_remap_data(m, &mut mr) {
                    mr.outcome = ModelOutcome::Failed;
                    mr.error = Some(e);
                }
            }
            ModelOutcome::Skipped => {
                mr.warnings
                    .push(OpError::new(codes::MESH_MODEL_SKIPPED).with("model", mr.name.clone()));
            }
            ModelOutcome::Failed | ModelOutcome::Fallback => {}
        }
        report.files.extend(mr.files.iter().cloned());
        report.models[m] = mr;
        progress(PackProgress {
            done: models.len() + 2 + m,
            total,
            current: Some(plan.models[m].source.display().to_string()),
        });
    }
    if let Some(script) = &writer.script_written {
        report.files.push(script.display().to_string());
    }
    let report_path = out_dir.join(report_name);
    report.report_path = Some(report_path.display().to_string());
    report.files.push(report_path.display().to_string());
    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| OpError::invalid_param("report", &e.to_string()))?;
    write_text(&report_path, &json)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_names() {
        validate_base_name("atlas").unwrap();
        validate_base_name("my atlas_01").unwrap();
        for bad in ["", " a", "a/b", "a\\b", "a:b", "a.", "a*"] {
            assert!(validate_base_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn atlas_names() {
        assert_eq!(
            atlas_file_name("atlas", 0, &TextureChannel::BaseColor),
            "atlas_baseColor.png"
        );
        assert_eq!(
            atlas_file_name("atlas", 1, &TextureChannel::Normal),
            "atlas_p2_normal.png"
        );
        assert_eq!(
            atlas_file_name("a", 0, &TextureChannel::Other("x#1".into())),
            "a_other-x-1.png"
        );
    }

    #[test]
    fn name_picker_avoids_collisions_and_sources() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("crate.obj");
        std::fs::write(&src, "x").unwrap();
        let mut p = NamePicker {
            used: HashSet::new(),
            protected: [normalized(&src)].into_iter().collect(),
        };
        assert_eq!(
            p.pick(dir.path(), "crate", "obj"),
            dir.path().join("crate_2.obj")
        );
        assert_eq!(
            p.pick(dir.path(), "crate", "dae"),
            dir.path().join("crate.dae")
        );
        // crate_2.* is taken by the first pick's sidecar/mtl companions.
        assert_eq!(
            p.pick(dir.path(), "Crate", "DAE"),
            dir.path().join("Crate_3.DAE")
        );
        // Same stem, other extension: the sidecar name would collide.
        assert_eq!(
            p.pick(dir.path(), "crate", "fbx"),
            dir.path().join("crate_4.fbx")
        );
        let q = p.pick(dir.path(), "box", "obj");
        p.release(&q);
        assert_eq!(p.pick(dir.path(), "box", "fbx"), dir.path().join("box.fbx"));
    }

    #[test]
    fn tile_scaling_respects_min_edge() {
        assert_eq!(scaled_tile([64, 32], 0.5, 2), [32, 16]);
        assert_eq!(scaled_tile([1, 1], 1.0, 2), [2, 2]);
        assert_eq!(scaled_tile([3, 3], 0.25, 2), [2, 2]);
    }
}
