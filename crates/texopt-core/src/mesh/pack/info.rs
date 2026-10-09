//! Model discovery (folders → model files, no Assimp needed) and the summary
//! shown on model cards (`mesh_scan`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::io::{
    ImportedFile, SCAN_PATH_NOT_FOUND, SkippedPath, extension_of, file_id, mtime_ms, natural_cmp,
    normalize_path,
};
use crate::mesh::model::{Model, ModelFormat, TextureChannel};
use crate::mesh::uv_remap::UvRangeReport;
use crate::mesh::{LoadedModel, SUPPORTED_EXTENSIONS, codes};
use crate::{OpError, OpResult};

pub fn is_model_extension(ext: &str) -> bool {
    SUPPORTED_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
}

/// File entry of a model (same shape as image entries; width/height are 0).
pub fn model_file_entry(path: &Path) -> OpResult<ImportedFile> {
    let abs = normalize_path(path).unwrap_or_else(|_| path.to_path_buf());
    let meta = std::fs::metadata(&abs).map_err(|e| crate::io::io_read_error(&abs, &e))?;
    let path_str = abs.display().to_string();
    Ok(ImportedFile {
        id: file_id(&path_str),
        name: abs
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        ext: extension_of(&abs),
        width: 0,
        height: 0,
        size_bytes: meta.len(),
        mtime_ms: mtime_ms(&meta),
        path: path_str,
    })
}

/// Expand files/folders into `.fbx/.obj/.dae` files (sorted naturally,
/// deduplicated). Unsupported explicit files and missing paths are skipped
/// with a reason; other files inside folders are ignored silently.
pub fn find_models<P: AsRef<Path>>(
    paths: &[P],
    recursive: bool,
) -> (Vec<ImportedFile>, Vec<SkippedPath>) {
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |files: &mut Vec<ImportedFile>, skipped: &mut Vec<SkippedPath>, p: &Path| {
        match model_file_entry(p) {
            Ok(f) => {
                if seen.insert(f.id.clone()) {
                    files.push(f);
                }
            }
            Err(error) => skipped.push(SkippedPath {
                path: p.display().to_string(),
                error,
            }),
        }
    };
    for raw in paths {
        let raw = raw.as_ref();
        let Ok(abs) = normalize_path(raw) else {
            let shown = raw.display().to_string();
            skipped.push(SkippedPath {
                path: shown.clone(),
                error: OpError::new(SCAN_PATH_NOT_FOUND).with("path", shown),
            });
            continue;
        };
        if abs.is_dir() {
            let depth = if recursive { usize::MAX } else { 1 };
            for entry in walkdir::WalkDir::new(&abs)
                .min_depth(1)
                .max_depth(depth)
                .follow_links(true)
                .into_iter()
                .filter_map(Result::ok)
            {
                if entry.file_type().is_file() && is_model_extension(&extension_of(entry.path())) {
                    push(&mut files, &mut skipped, entry.path());
                }
            }
        } else if is_model_extension(&extension_of(&abs)) {
            push(&mut files, &mut skipped, &abs);
        } else {
            let shown = abs.display().to_string();
            skipped.push(SkippedPath {
                path: shown.clone(),
                error: OpError::new(codes::MESH_FORMAT_UNSUPPORTED).with("path", shown),
            });
        }
    }
    files.sort_by(|a, b| natural_cmp(&a.path, &b.path));
    (files, skipped)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureInfo {
    pub channel: TextureChannel,
    /// Resolved path (may not exist).
    pub path: String,
    pub raw_path: String,
    pub exists: bool,
    pub embedded: bool,
    pub uv_channel: u32,
    /// Pixel size read from the file header (`None` when unreadable/missing).
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub mtime_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialInfo {
    pub index: usize,
    pub name: String,
    pub mesh_count: usize,
    pub vertex_count: usize,
    /// UV channel the packer would remap (base colour's, else first texture's).
    pub uv_channel: Option<u32>,
    /// UV bounds of that channel over the material's meshes.
    pub uv_range: Option<UvRangeReport>,
    pub textures: Vec<TextureInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub path: String,
    pub format: ModelFormat,
    pub mesh_count: usize,
    pub vertex_count: usize,
    pub embedded_texture_count: usize,
    /// Materials used by at least one mesh.
    pub materials: Vec<MaterialInfo>,
    pub warnings: Vec<OpError>,
}

/// UV channel the packer remaps for a material.
pub fn material_uv_channel(model: &Model, material: usize) -> Option<u32> {
    let mat = model.materials.get(material)?;
    mat.textures
        .get(&TextureChannel::BaseColor)
        .or_else(|| mat.textures.values().next())
        .map(|t| t.uv_channel)
}

pub fn describe(loaded: &LoadedModel) -> ModelInfo {
    let model = &loaded.model;
    let mut materials = Vec::new();
    for (mi, mat) in model.materials.iter().enumerate() {
        let meshes: Vec<_> = model.meshes_with_material(mi).collect();
        if meshes.is_empty() {
            continue;
        }
        let uv_channel = material_uv_channel(model, mi);
        let textures = mat
            .textures
            .iter()
            .map(|(channel, t)| {
                let dims = if t.exists && t.embedded_index.is_none() {
                    image::image_dimensions(&t.path).ok()
                } else {
                    None
                };
                let mtime = std::fs::metadata(&t.path)
                    .map(|m| mtime_ms(&m))
                    .unwrap_or(0);
                TextureInfo {
                    channel: channel.clone(),
                    path: t.path.display().to_string(),
                    raw_path: t.raw_path.clone(),
                    exists: t.exists,
                    embedded: t.embedded_index.is_some(),
                    uv_channel: t.uv_channel,
                    width: dims.map(|d| d.0),
                    height: dims.map(|d| d.1),
                    mtime_ms: mtime,
                }
            })
            .collect();
        materials.push(MaterialInfo {
            index: mi,
            name: mat.name.clone(),
            mesh_count: meshes.len(),
            vertex_count: meshes.iter().map(|m| m.vertex_count).sum(),
            uv_channel,
            uv_range: uv_channel.and_then(|c| model.material_uv_range(mi, c)),
            textures,
        });
    }
    ModelInfo {
        path: loaded.source_path.display().to_string(),
        format: model.format,
        mesh_count: model.meshes.len(),
        vertex_count: model.meshes.iter().map(|m| m.vertex_count).sum(),
        embedded_texture_count: model.embedded_textures.len(),
        materials,
        warnings: model.warnings.clone(),
    }
}

/// Import a model and summarise it.
pub fn inspect(path: &Path) -> OpResult<ModelInfo> {
    let loaded = crate::mesh::import(path)?;
    Ok(describe(&loaded))
}

/// Paths of the models, resolved to absolute paths when possible.
pub fn absolute(paths: &[String]) -> Vec<PathBuf> {
    paths
        .iter()
        .map(|p| normalize_path(Path::new(p)).unwrap_or_else(|_| PathBuf::from(p)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_models_in_folders_and_skips_others() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        for f in ["a.obj", "B.FBX", "c.png", "sub/d.dae", "sub/e.txt"] {
            std::fs::write(root.join(f), "x").unwrap();
        }
        let (files, skipped) = find_models(&[root], false);
        let names: Vec<_> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["a.obj", "B.FBX"]);
        assert!(skipped.is_empty());
        assert!(files.iter().all(|f| f.width == 0 && f.size_bytes == 1));
        assert_eq!(files[1].ext, "fbx");

        let (files, _) = find_models(&[root], true);
        assert_eq!(files.len(), 3);

        let (files, skipped) = find_models(
            &[
                root.join("c.png"),
                root.join("missing.obj"),
                root.join("a.obj"),
                root.join("a.obj"),
            ],
            false,
        );
        assert_eq!(files.len(), 1);
        let codes: Vec<_> = skipped.iter().map(|s| s.error.code.as_str()).collect();
        assert_eq!(codes, [codes::MESH_FORMAT_UNSUPPORTED, SCAN_PATH_NOT_FOUND]);
    }
}
