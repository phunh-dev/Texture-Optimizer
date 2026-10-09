//! Engine exporters. The user picks exactly one [`ExporterConfig`] per export.
//!
//! [`ExporterConfig::ImageOnly`] writes the page PNG(s) and nothing else: no
//! metadata, and the app workflow writes no project file for it either (see
//! `workflow::export_atlas`), so such an atlas cannot be updated
//! incrementally.
//!
//! Capability handling: [`build`](super::build) is exporter-agnostic. The app
//! may call [`adapt_params`] first, which switches off features the exporter
//! cannot represent and returns `ATLAS_FEATURE_DISABLED` warnings. [`export`]
//! itself never silently drops information: if the atlas uses a feature the
//! exporter does not support it fails with `ATLAS_EXPORTER_UNSUPPORTED
//! { feature, exporter }`.

pub mod generic;
pub mod godot;
pub mod unity;
pub mod unreal;

use std::collections::BTreeMap;

use image::{ExtendedColorType, ImageEncoder};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use generic::{GenericJsonOptions, JsonFormat};
pub use godot::{GodotOptions, GodotVersion};
pub use unity::{UnityCompression, UnityFilterMode, UnityOptions, UnityPivot, UnityVersion};
pub use unreal::{Paper2dExtension, UnrealOptions};

use super::incremental::{AtlasProject, ProjectSprite};
use super::params::AtlasParams;
use super::{AtlasResult, codes};
use crate::error::codes as core_codes;
use crate::{ImageBuf, OpError, OpResult};

/// Normalised pivot point. The axis convention depends on the exporter: JSON
/// exporters use TexturePacker's (0,0 = top-left), Unity uses its own
/// (0,0 = bottom-left).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pivot {
    pub x: f64,
    pub y: f64,
}

impl Default for Pivot {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

/// Options of [`ExporterConfig::ImageOnly`]: none (`{}` on the wire).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageOnlyOptions {}

/// `{ "kind": "genericJson" | "unity" | "godot" | "unreal" | "imageOnly", "options": { ... } }`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "options", rename_all = "camelCase")]
pub enum ExporterConfig {
    GenericJson(GenericJsonOptions),
    Unity(UnityOptions),
    Godot(GodotOptions),
    Unreal(UnrealOptions),
    /// Only the packed page image(s), no metadata.
    ImageOnly(ImageOnlyOptions),
}

impl ExporterConfig {
    /// Stable id used in error params (`exporter`).
    pub fn id(&self) -> &'static str {
        match self {
            ExporterConfig::GenericJson(_) => "genericJson",
            ExporterConfig::Unity(_) => "unity",
            ExporterConfig::Godot(_) => "godot",
            ExporterConfig::Unreal(_) => "unreal",
            ExporterConfig::ImageOnly(_) => "imageOnly",
        }
    }

    /// False for [`ExporterConfig::ImageOnly`]: only page images are
    /// produced, so there is no project to merge with or clean up after.
    pub fn writes_metadata(&self) -> bool {
        !matches!(self, ExporterConfig::ImageOnly(_))
    }

    /// Unity sprite rects and Godot `AtlasTexture` regions cannot be rotated;
    /// without metadata (image only) nobody could tell a sprite was rotated.
    pub fn supports_rotation(&self) -> bool {
        matches!(
            self,
            ExporterConfig::GenericJson(_) | ExporterConfig::Unreal(_)
        )
    }

    /// Every exporter writes one metadata file (set) per page.
    pub fn supports_multipage(&self) -> bool {
        true
    }

    /// Generic JSON / Paper2D carry trim offsets, Godot uses `margin`, Unity
    /// compensates through the sprite pivot.
    pub fn supports_trim(&self) -> bool {
        true
    }
}

/// Previous contents of files the exporter may update, keyed by the same
/// relative path the exporter writes (forward slashes). Used to keep Unity
/// GUIDs / internalIDs stable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExistingFiles {
    files: BTreeMap<String, Vec<u8>>,
}

impl ExistingFiles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        self.insert(path, bytes);
        self
    }

    pub fn insert(&mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) {
        self.files
            .insert(path.into().replace('\\', "/"), bytes.into());
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(&path.replace('\\', "/")).map(Vec::as_slice)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportOutput {
    /// Files to write, relative to the export folder: page PNGs first, then metadata.
    pub files: Vec<(String, Vec<u8>)>,
    /// New value for `AtlasProject.exporter_state`.
    pub exporter_state: Value,
    pub warnings: Vec<OpError>,
}

/// `<base>.png` for a single page, `<base>_<i>.png` otherwise.
pub fn page_file_name(base: &str, index: usize, page_count: usize) -> String {
    format!("{}.png", page_stem(base, index, page_count))
}

pub(crate) fn page_stem(base: &str, index: usize, page_count: usize) -> String {
    if page_count <= 1 {
        base.to_string()
    } else {
        format!("{base}_{index}")
    }
}

/// Paths of existing files [`export`] would like to read (pass their contents
/// through [`ExistingFiles`]).
pub fn files_to_read(base_name: &str, page_count: usize, config: &ExporterConfig) -> Vec<String> {
    match config {
        ExporterConfig::Unity(_) => (0..page_count)
            .map(|i| format!("{}.meta", page_file_name(base_name, i, page_count)))
            .collect(),
        _ => Vec::new(),
    }
}

/// Switch off params the exporter cannot represent. Returns the adjusted
/// params and one `ATLAS_FEATURE_DISABLED` warning per change.
pub fn adapt_params(params: &AtlasParams, config: &ExporterConfig) -> (AtlasParams, Vec<OpError>) {
    let mut p = params.clone();
    let mut warnings = Vec::new();
    let mut disable = |flag: &mut bool, supported: bool, feature: &str| {
        if *flag && !supported {
            *flag = false;
            warnings.push(
                OpError::new(codes::ATLAS_FEATURE_DISABLED)
                    .with("feature", feature)
                    .with("exporter", config.id()),
            );
        }
    };
    disable(
        &mut p.allow_rotation,
        config.supports_rotation(),
        "rotation",
    );
    disable(&mut p.multi_page, config.supports_multipage(), "multiPage");
    disable(&mut p.trim, config.supports_trim(), "trim");
    (p, warnings)
}

fn unsupported(feature: &str, config: &ExporterConfig) -> OpError {
    OpError::new(codes::ATLAS_EXPORTER_UNSUPPORTED)
        .with("feature", feature)
        .with("exporter", config.id())
}

pub(crate) fn check_capabilities(project: &AtlasProject, config: &ExporterConfig) -> OpResult<()> {
    if !config.supports_rotation() && project.sprites.iter().any(|s| s.rotated) {
        return Err(unsupported("rotation", config));
    }
    if !config.supports_multipage() && project.pages.len() > 1 {
        return Err(unsupported("multiPage", config));
    }
    if !config.supports_trim() && project.sprites.iter().any(|s| s.trimmed) {
        return Err(unsupported("trim", config));
    }
    Ok(())
}

pub(crate) fn encode_png(img: &ImageBuf, path: &str) -> OpResult<Vec<u8>> {
    let mut buf = Vec::new();
    image::codecs::png::PngEncoder::new(&mut buf)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|e| {
            OpError::new(core_codes::IMG_ENCODE_FAILED)
                .with("path", path.to_string())
                .with("detail", e.to_string())
        })?;
    Ok(buf)
}

/// Every exported frame name (primary names and aliases) of `page`, sorted by name.
pub(crate) fn page_frames(project: &AtlasProject, page: usize) -> Vec<(&str, &ProjectSprite)> {
    let mut v: Vec<(&str, &ProjectSprite)> = project
        .sprites
        .iter()
        .filter(|s| s.page == page)
        .flat_map(|s| {
            std::iter::once(s.name.as_str())
                .chain(s.aliases.iter().map(String::as_str))
                .map(move |n| (n, s))
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(b.0));
    v
}

fn validate_base_name(base: &str) -> OpResult<()> {
    if base.is_empty() || base == "." || base == ".." || base.contains(['/', '\\']) {
        return Err(OpError::invalid_param("baseName", "invalid"));
    }
    Ok(())
}

/// Produce every file of the export (pure; the app writes them).
pub fn export(
    result: &AtlasResult,
    base_name: &str,
    config: &ExporterConfig,
    existing: &ExistingFiles,
) -> OpResult<ExportOutput> {
    validate_base_name(base_name)?;
    let project = &result.project;
    check_capabilities(project, config)?;
    let count = result.pages.len();
    let mut files = Vec::new();
    for (i, img) in result.pages.iter().enumerate() {
        let path = page_file_name(base_name, i, count);
        let bytes = encode_png(img, &path)?;
        files.push((path, bytes));
    }
    let mut warnings = Vec::new();
    let mut state = project.exporter_state.clone();
    match config {
        ExporterConfig::GenericJson(o) => files.extend(generic::export(project, base_name, o)?),
        ExporterConfig::Unreal(o) => files.extend(unreal::export(project, base_name, o)?),
        ExporterConfig::Godot(o) => files.extend(godot::export(project, base_name, o)?),
        ExporterConfig::Unity(o) => {
            let out = unity::export(project, base_name, o, existing)?;
            files.extend(out.files);
            warnings.extend(out.warnings);
            if !state.is_object() {
                state = Value::Object(Default::default());
            }
            state["unity"] = out.state;
        }
        ExporterConfig::ImageOnly(_) => {}
    }
    Ok(ExportOutput {
        files,
        exporter_state: state,
        warnings,
    })
}
