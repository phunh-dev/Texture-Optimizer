//! Assimp backend (raw FFI through `asset-importer-sys`).
//!
//! Import: `aiImportFileExWithProperties` → read-only scene kept alive in a
//! [`SceneHandle`] for the later export.
//!
//! Export without touching Assimp-owned memory: we build a *Rust-owned*
//! `aiScene` that bitwise-copies the imported scene's top-level struct, points
//! `mMeshes` at Rust-owned `aiMesh` copies whose remapped UV channel points at
//! Rust `Vec`s, and `mMaterials` at Rust-owned `aiMaterial`s whose property
//! lists mix the original property pointers with Rust-owned replacements
//! (atlas texture paths, merged material name). Everything else (vertices,
//! faces, nodes, bones, animations, embedded textures) is shared with the
//! imported scene. `aiExportScene` deep-copies its input (`SceneCombiner::
//! CopyScene`, which explicitly supports user-allocated scenes) and never
//! frees it, so no C++ allocator ever sees Rust memory and vice versa.

use std::collections::BTreeMap;
use std::ffi::{CStr, CString, c_char, c_uint};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::Mutex;

use asset_importer_sys as sys;

use super::model::{
    self, EmbeddedTexture, Material, Mesh, Model, ModelFormat, TextureChannel, TextureRef, WrapMode,
};
use super::{ExportOptions, ModelRemaps, codes};
use crate::{OpError, OpResult};

/// Assimp's C API keeps the last error message in a global string; serialise
/// every call so concurrent imports/exports (rayon, parallel tests) are sound.
static ASSIMP_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    ASSIMP_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

const MAX_UV_CHANNELS: usize = 8;
const KEY_TEX_FILE: &str = "$tex.file";
const KEY_TEX_UVWSRC: &str = "$tex.uvwsrc";
const KEY_TEX_MAPMODE_U: &str = "$tex.mapmodeu";
const KEY_MAT_NAME: &str = "?mat.name";

/// Owned imported scene (released with `aiReleaseImport`).
pub(super) struct SceneHandle(NonNull<sys::aiScene>);

// SAFETY: the scene is never mutated after import and Assimp scene data has no
// thread affinity; all FFI calls touching it are serialised by ASSIMP_LOCK.
unsafe impl Send for SceneHandle {}
unsafe impl Sync for SceneHandle {}

impl SceneHandle {
    fn scene(&self) -> &sys::aiScene {
        // SAFETY: pointer is valid until Drop.
        unsafe { self.0.as_ref() }
    }
}

impl Drop for SceneHandle {
    fn drop(&mut self) {
        let _g = lock();
        // SAFETY: obtained from aiImportFileExWithProperties, released once.
        unsafe { sys::aiReleaseImport(self.0.as_ptr()) };
    }
}

fn ai_string(s: &sys::aiString) -> String {
    let len = (s.length as usize).min(s.data.len());
    let bytes: Vec<u8> = s.data[..len].iter().map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn make_ai_string(text: &str) -> OpResult<sys::aiString> {
    let bytes = text.as_bytes();
    let mut s = sys::aiString {
        length: 0,
        data: [0; 1024],
    };
    if bytes.len() >= s.data.len() {
        return Err(OpError::invalid_param("path", "tooLong"));
    }
    for (d, &b) in s.data.iter_mut().zip(bytes) {
        *d = b as c_char;
    }
    s.length = bytes.len() as u32;
    Ok(s)
}

fn last_error() -> String {
    // SAFETY: aiGetErrorString always returns a valid C string.
    unsafe {
        CStr::from_ptr(sys::aiGetErrorString())
            .to_string_lossy()
            .into_owned()
    }
}

/// # Safety
/// `ptr` must be valid for `len` elements when `len > 0`.
unsafe fn slice<'a, T>(ptr: *const T, len: usize) -> &'a [T] {
    if ptr.is_null() || len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(ptr, len) }
    }
}

pub(super) fn version() -> String {
    let _g = lock();
    // SAFETY: plain getters.
    unsafe {
        format!(
            "{}.{}.{}",
            sys::aiGetVersionMajor(),
            sys::aiGetVersionMinor(),
            sys::aiGetVersionPatch()
        )
    }
}

pub(super) fn export_format_ids() -> Vec<String> {
    let _g = lock();
    let mut out = Vec::new();
    // SAFETY: descriptions are valid until released.
    unsafe {
        for i in 0..sys::aiGetExportFormatCount() {
            let d = sys::aiGetExportFormatDescription(i);
            if d.is_null() {
                continue;
            }
            out.push(CStr::from_ptr((*d).id).to_string_lossy().into_owned());
            sys::aiReleaseExportFormatDescription(d);
        }
    }
    out
}

// ---------------------------------------------------------------- materials

/// Assimp texture type (semantic) ↔ logical channel, in priority order: when
/// a material has both DIFFUSE and BASE_COLOR, DIFFUSE wins the `baseColor`
/// slot in [`Material::textures`] (both get replaced on export).
const SEMANTICS: &[(u32, &str)] = &[
    (1, "baseColor"),                // DIFFUSE
    (12, "baseColor"),               // BASE_COLOR
    (6, "normal"),                   // NORMALS
    (13, "normal"),                  // NORMAL_CAMERA
    (15, "metallic"),                // METALNESS
    (16, "roughness"),               // DIFFUSE_ROUGHNESS
    (17, "occlusion"),               // AMBIENT_OCCLUSION
    (10, "occlusion"),               // LIGHTMAP
    (4, "emissive"),                 // EMISSIVE
    (14, "emissive"),                // EMISSION_COLOR
    (8, "opacity"),                  // OPACITY
    (2, "specular"),                 // SPECULAR
    (5, "height"),                   // HEIGHT (OBJ `bump`/`map_bump` lands here)
    (7, "other:shininess"),          // SHININESS
    (3, "other:ambient"),            // AMBIENT
    (9, "other:displacement"),       // DISPLACEMENT
    (11, "other:reflection"),        // REFLECTION
    (27, "other:metallicRoughness"), // GLTF_METALLIC_ROUGHNESS
    (19, "other:sheen"),
    (20, "other:clearcoat"),
    (21, "other:transmission"),
    (22, "other:mayaBase"),
    (23, "other:mayaSpecular"),
    (24, "other:mayaSpecularColor"),
    (25, "other:mayaSpecularRoughness"),
    (26, "other:anisotropy"),
    (18, "other:unknown"),
];

fn semantic_rank(semantic: u32) -> usize {
    SEMANTICS
        .iter()
        .position(|(s, _)| *s == semantic)
        .unwrap_or(SEMANTICS.len())
}

fn channel_for(semantic: u32, index: u32) -> TextureChannel {
    let base = SEMANTICS
        .iter()
        .find(|(s, _)| *s == semantic)
        .map(|(_, k)| TextureChannel::from_key(k))
        .unwrap_or_else(|| TextureChannel::Other(format!("type{semantic}")));
    if index == 0 {
        base
    } else {
        TextureChannel::Other(format!(
            "{}#{index}",
            base.as_key().trim_start_matches("other:")
        ))
    }
}

/// Preferred Assimp semantic used when *writing* an atlas channel.
fn semantic_for(channel: &TextureChannel) -> u32 {
    let key = channel.as_key();
    SEMANTICS
        .iter()
        .find(|(_, k)| *k == key)
        .map(|(s, _)| *s)
        .unwrap_or(18)
}

struct RawProp<'a> {
    key: String,
    semantic: u32,
    index: u32,
    data: &'a [u8],
}

/// # Safety
/// `mat` must be a valid material of a live scene.
unsafe fn material_props<'a>(
    mat: *const sys::aiMaterial,
) -> Vec<(*mut sys::aiMaterialProperty, RawProp<'a>)> {
    let m = unsafe { &*mat };
    let ptrs = unsafe { slice(m.mProperties, m.mNumProperties as usize) };
    ptrs.iter()
        .filter(|p| !p.is_null())
        .map(|&p| {
            let prop = unsafe { &*p };
            let data = unsafe { slice(prop.mData as *const u8, prop.mDataLength as usize) };
            (
                p,
                RawProp {
                    key: ai_string(&prop.mKey),
                    semantic: prop.mSemantic,
                    index: prop.mIndex,
                    data,
                },
            )
        })
        .collect()
}

/// Decode an `aiPTI_String` payload (u32 length + bytes + NUL).
fn prop_string(data: &[u8]) -> Option<String> {
    let len = u32::from_ne_bytes(data.get(..4)?.try_into().ok()?) as usize;
    let bytes = data.get(4..4 + len)?;
    Some(String::from_utf8_lossy(bytes).into_owned())
}

fn prop_int(data: &[u8]) -> Option<i32> {
    Some(i32::from_ne_bytes(data.get(..4)?.try_into().ok()?))
}

fn find_int(
    props: &[(*mut sys::aiMaterialProperty, RawProp)],
    key: &str,
    semantic: u32,
    index: u32,
) -> Option<i32> {
    props
        .iter()
        .find(|(_, p)| p.key == key && p.semantic == semantic && p.index == index)
        .and_then(|(_, p)| prop_int(p.data))
}

fn wrap_mode(v: Option<i32>) -> WrapMode {
    match v {
        Some(1) => WrapMode::Clamp,
        Some(2) => WrapMode::Mirror,
        Some(3) => WrapMode::Decal,
        _ => WrapMode::Repeat,
    }
}

// ------------------------------------------------------------------ import

pub(super) fn import(path: &Path, format: ModelFormat) -> OpResult<(SceneHandle, Model)> {
    let fail = |detail: String| {
        OpError::new(codes::MESH_IMPORT_FAILED)
            .with("path", path.display().to_string())
            .with("detail", detail)
    };
    let c_path = CString::new(path.to_str().ok_or_else(|| fail("non-UTF-8 path".into()))?)
        .map_err(|e| fail(e.to_string()))?;
    let handle = {
        let _g = lock();
        raw_import(&c_path).map_err(fail)?
    };
    let model_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let model = read_model(handle.scene(), format, &model_dir);
    Ok((handle, model))
}

/// Import with the module's fixed settings. Caller must hold ASSIMP_LOCK.
///
/// Only `JoinIdenticalVertices` (indexed meshes, no triangulation so quads
/// survive). FBX pivots are kept as `$AssimpFbx$` helper nodes (Assimp's
/// default): `IMPORT_FBX_PRESERVE_PIVOTS = false` was tried and drops
/// rotations on 3ds-Max style files (see docs/spikes/3d-assimp.md).
fn raw_import(c_path: &CStr) -> Result<SceneHandle, String> {
    let flags = sys::aiPostProcessSteps::aiProcess_JoinIdenticalVertices as c_uint;
    // SAFETY: valid C string; the returned scene is owned by the handle.
    let scene = unsafe { sys::aiImportFile(c_path.as_ptr(), flags) };
    NonNull::new(scene as *mut sys::aiScene)
        .map(SceneHandle)
        .ok_or_else(last_error)
}

/// World-space bounding box of all mesh vertices (node transforms applied).
fn world_bounds(scene: &sys::aiScene) -> Option<([f32; 3], [f32; 3])> {
    fn mul(a: &sys::aiMatrix4x4, b: &sys::aiMatrix4x4) -> sys::aiMatrix4x4 {
        let ra = [
            [a.a1, a.a2, a.a3, a.a4],
            [a.b1, a.b2, a.b3, a.b4],
            [a.c1, a.c2, a.c3, a.c4],
            [a.d1, a.d2, a.d3, a.d4],
        ];
        let rb = [
            [b.a1, b.a2, b.a3, b.a4],
            [b.b1, b.b2, b.b3, b.b4],
            [b.c1, b.c2, b.c3, b.c4],
            [b.d1, b.d2, b.d3, b.d4],
        ];
        let r = |i: usize, j: usize| (0..4).map(|k| ra[i][k] * rb[k][j]).sum::<f32>();
        sys::aiMatrix4x4 {
            a1: r(0, 0),
            a2: r(0, 1),
            a3: r(0, 2),
            a4: r(0, 3),
            b1: r(1, 0),
            b2: r(1, 1),
            b3: r(1, 2),
            b4: r(1, 3),
            c1: r(2, 0),
            c2: r(2, 1),
            c3: r(2, 2),
            c4: r(2, 3),
            d1: r(3, 0),
            d2: r(3, 1),
            d3: r(3, 2),
            d4: r(3, 3),
        }
    }
    fn walk(
        scene: &sys::aiScene,
        node: *const sys::aiNode,
        parent: &sys::aiMatrix4x4,
        bb: &mut Option<([f32; 3], [f32; 3])>,
        depth: usize,
    ) {
        if node.is_null() || depth > 1024 {
            return;
        }
        // SAFETY: nodes/meshes of a valid scene.
        let n = unsafe { &*node };
        let m = mul(parent, &n.mTransformation);
        let meshes = unsafe { slice(scene.mMeshes, scene.mNumMeshes as usize) };
        for &mi in unsafe { slice(n.mMeshes, n.mNumMeshes as usize) } {
            let Some(&mp) = meshes.get(mi as usize) else {
                continue;
            };
            let mesh = unsafe { &*mp };
            for p in unsafe { slice(mesh.mVertices, mesh.mNumVertices as usize) } {
                let w = [
                    m.a1 * p.x + m.a2 * p.y + m.a3 * p.z + m.a4,
                    m.b1 * p.x + m.b2 * p.y + m.b3 * p.z + m.b4,
                    m.c1 * p.x + m.c2 * p.y + m.c3 * p.z + m.c4,
                ];
                let (lo, hi) = bb.get_or_insert((w, w));
                for a in 0..3 {
                    lo[a] = lo[a].min(w[a]);
                    hi[a] = hi[a].max(w[a]);
                }
            }
        }
        for &c in unsafe { slice(n.mChildren, n.mNumChildren as usize) } {
            walk(scene, c, &m, bb, depth + 1);
        }
    }
    let ident = sys::aiMatrix4x4 {
        a1: 1.0,
        b2: 1.0,
        c3: 1.0,
        d4: 1.0,
        ..Default::default()
    };
    let mut bb = None;
    walk(scene, scene.mRootNode, &ident, &mut bb, 0);
    bb
}

/// Largest absolute difference between two world bounding boxes, relative to
/// the source diagonal (0 when both are empty, infinity when only one is).
fn bounds_error(a: Option<([f32; 3], [f32; 3])>, b: Option<([f32; 3], [f32; 3])>) -> f32 {
    match (a, b) {
        (None, None) => 0.0,
        (Some((alo, ahi)), Some((blo, bhi))) => {
            let diag = (0..3)
                .map(|i| (ahi[i] - alo[i]).powi(2))
                .sum::<f32>()
                .sqrt()
                .max(1e-6);
            (0..3)
                .map(|i| (alo[i] - blo[i]).abs().max((ahi[i] - bhi[i]).abs()))
                .fold(0.0, f32::max)
                / diag
        }
        _ => f32::INFINITY,
    }
}

fn read_model(scene: &sys::aiScene, format: ModelFormat, model_dir: &Path) -> Model {
    // SAFETY (whole fn): arrays/counts come from a valid imported scene.
    let mesh_ptrs = unsafe { slice(scene.mMeshes, scene.mNumMeshes as usize) };
    let meshes = mesh_ptrs
        .iter()
        .filter(|p| !p.is_null())
        .map(|&p| unsafe { read_mesh(&*p) })
        .collect();

    let tex_ptrs = unsafe { slice(scene.mTextures, scene.mNumTextures as usize) };
    let embedded_textures: Vec<EmbeddedTexture> = tex_ptrs
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.is_null())
        .map(|(i, &p)| {
            let t = unsafe { &*p };
            let hint: String = t
                .achFormatHint
                .iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as u8 as char)
                .collect();
            EmbeddedTexture {
                index: i,
                file_name: ai_string(&t.mFilename),
                format_hint: hint,
                compressed: t.mHeight == 0,
                width: t.mWidth,
                height: t.mHeight,
            }
        })
        .collect();

    let mut warnings = Vec::new();
    let mat_ptrs = unsafe { slice(scene.mMaterials, scene.mNumMaterials as usize) };
    let materials = mat_ptrs
        .iter()
        .map(|&p| {
            if p.is_null() {
                return Material {
                    name: String::new(),
                    textures: BTreeMap::new(),
                };
            }
            let props = unsafe { material_props(p) };
            let name = props
                .iter()
                .find(|(_, q)| q.key == KEY_MAT_NAME)
                .and_then(|(_, q)| prop_string(q.data))
                .unwrap_or_default();
            let mut entries: Vec<(u32, u32, String)> = props
                .iter()
                .filter(|(_, q)| q.key == KEY_TEX_FILE)
                .filter_map(|(_, q)| prop_string(q.data).map(|s| (q.semantic, q.index, s)))
                // FBX files often carry empty texture slots (e.g. emissive = "").
                .filter(|(_, _, s)| !s.trim().is_empty())
                .collect();
            entries.sort_by_key(|(s, i, _)| (semantic_rank(*s), *i));
            let mut textures = BTreeMap::new();
            for (semantic, index, raw) in entries {
                let channel = channel_for(semantic, index);
                if textures.contains_key(&channel) {
                    continue;
                }
                let embedded_index = embedded_lookup(&raw, &embedded_textures);
                let (resolved, exists) = if embedded_index.is_some() {
                    (PathBuf::from(&raw), false)
                } else {
                    model::resolve_texture_path(model_dir, &raw)
                };
                if !exists && embedded_index.is_none() {
                    warnings.push(
                        OpError::new(codes::MESH_TEXTURE_NOT_FOUND)
                            .with("material", name.clone())
                            .with("channel", channel.as_key())
                            .with("path", resolved.display().to_string()),
                    );
                }
                let uv_channel = find_int(&props, KEY_TEX_UVWSRC, semantic, index)
                    .unwrap_or(0)
                    .max(0) as u32;
                let tex = TextureRef {
                    path: resolved,
                    raw_path: raw,
                    uv_channel,
                    wrap_mode: wrap_mode(find_int(&props, KEY_TEX_MAPMODE_U, semantic, index)),
                    exists,
                    embedded_index,
                };
                textures.insert(channel, tex);
            }
            Material { name, textures }
        })
        .collect();

    Model {
        format,
        meshes,
        materials,
        embedded_textures,
        warnings,
    }
}

fn embedded_lookup(raw: &str, embedded: &[EmbeddedTexture]) -> Option<usize> {
    if let Some(idx) = raw.strip_prefix('*') {
        return idx.parse::<usize>().ok().filter(|&i| i < embedded.len());
    }
    let file = |s: &str| {
        s.rsplit(['/', '\\'])
            .next()
            .unwrap_or(s)
            .to_ascii_lowercase()
    };
    let wanted = file(raw);
    embedded
        .iter()
        .position(|e| !e.file_name.is_empty() && file(&e.file_name) == wanted)
}

/// # Safety
/// `m` must be a valid mesh of a live scene.
unsafe fn read_mesh(m: &sys::aiMesh) -> Mesh {
    let n = m.mNumVertices as usize;
    let mut uv_channels: Vec<Vec<[f32; 2]>> = (0..MAX_UV_CHANNELS)
        .map(|c| {
            unsafe { slice(m.mTextureCoords[c], n) }
                .iter()
                .map(|v| [v.x, v.y])
                .collect()
        })
        .collect();
    while uv_channels.last().is_some_and(Vec::is_empty) {
        uv_channels.pop();
    }
    let faces = unsafe { slice(m.mFaces, m.mNumFaces as usize) }
        .iter()
        .map(|f| unsafe { slice(f.mIndices, f.mNumIndices as usize) }.to_vec())
        .collect();
    Mesh {
        name: ai_string(&m.mName),
        material_index: m.mMaterialIndex as usize,
        vertex_count: n,
        uv_channels,
        faces,
    }
}

// ------------------------------------------------------------------ export

/// A property allocated on the Rust side (its `mData` points into `data`).
struct OwnedProp {
    prop: Box<sys::aiMaterialProperty>,
    _data: Vec<u8>,
}

impl OwnedProp {
    fn new(
        key: &str,
        semantic: u32,
        index: u32,
        ty: sys::aiPropertyTypeInfo,
        mut data: Vec<u8>,
    ) -> OpResult<Self> {
        let prop = Box::new(sys::aiMaterialProperty {
            mKey: make_ai_string(key)?,
            mSemantic: semantic,
            mIndex: index,
            mDataLength: data.len() as u32,
            mType: ty,
            mData: data.as_mut_ptr() as *mut c_char,
        });
        Ok(Self { prop, _data: data })
    }

    fn string(key: &str, semantic: u32, index: u32, value: &str) -> OpResult<Self> {
        if value.len() >= 1024 {
            return Err(OpError::invalid_param("path", "tooLong"));
        }
        let mut data = (value.len() as u32).to_ne_bytes().to_vec();
        data.extend_from_slice(value.as_bytes());
        data.push(0);
        Self::new(
            key,
            semantic,
            index,
            sys::aiPropertyTypeInfo::aiPTI_String,
            data,
        )
    }

    fn int(key: &str, semantic: u32, index: u32, value: i32) -> OpResult<Self> {
        Self::new(
            key,
            semantic,
            index,
            sys::aiPropertyTypeInfo::aiPTI_Integer,
            value.to_ne_bytes().to_vec(),
        )
    }

    fn ptr(&mut self) -> *mut sys::aiMaterialProperty {
        &mut *self.prop
    }
}

/// Rust-owned scene passed to `aiExportScene`. Inner `Vec`/`Box` heap buffers
/// never move once created; `meshes`/`materials` are only pointed to after
/// they are complete ([`ExportScene::finish`]).
#[derive(Default)]
struct ExportScene {
    meshes: Vec<sys::aiMesh>,
    mesh_ptrs: Vec<*mut sys::aiMesh>,
    uv_buffers: Vec<Vec<sys::aiVector3D>>,
    materials: Vec<sys::aiMaterial>,
    material_ptrs: Vec<*mut sys::aiMaterial>,
    prop_lists: Vec<Vec<*mut sys::aiMaterialProperty>>,
    owned_props: Vec<OwnedProp>,
}

impl ExportScene {
    fn add_material(&mut self, mut props: Vec<*mut sys::aiMaterialProperty>) {
        self.materials.push(sys::aiMaterial {
            mProperties: props.as_mut_ptr(),
            mNumProperties: props.len() as u32,
            mNumAllocated: props.len() as u32,
        });
        self.prop_lists.push(props);
    }

    /// Scene header pointing at our meshes/materials (call once, after all
    /// meshes and materials were added).
    fn finish(&mut self, src: &sys::aiScene) -> sys::aiScene {
        self.mesh_ptrs = self
            .meshes
            .iter_mut()
            .map(|m| m as *mut sys::aiMesh)
            .collect();
        self.material_ptrs = self
            .materials
            .iter_mut()
            .map(|m| m as *mut sys::aiMaterial)
            .collect();
        // Bitwise copy of the imported scene header; mPrivate stays valid
        // (owned by the original, which outlives the export call).
        let mut scene: sys::aiScene = *src;
        scene.mMeshes = self.mesh_ptrs.as_mut_ptr();
        scene.mNumMeshes = self.mesh_ptrs.len() as u32;
        scene.mMaterials = self.material_ptrs.as_mut_ptr();
        scene.mNumMaterials = self.material_ptrs.len() as u32;
        scene
    }

    fn own(&mut self, p: OwnedProp) -> *mut sys::aiMaterialProperty {
        self.owned_props.push(p);
        self.owned_props.last_mut().expect("just pushed").ptr()
    }
}

pub(super) fn export(
    handle: &SceneHandle,
    model: &Model,
    remaps: &ModelRemaps,
    uvs: &[Option<Vec<[f32; 2]>>],
    options: &ExportOptions,
    out_path: &Path,
) -> OpResult<()> {
    let fail = |detail: String| {
        OpError::new(codes::MESH_EXPORT_FAILED)
            .with("path", out_path.display().to_string())
            .with("format", options.format.assimp_id())
            .with("detail", detail)
    };
    let src = handle.scene();
    let mut es = ExportScene::default();

    // --- materials
    // SAFETY: arrays of the live imported scene.
    let src_mats = unsafe { slice(src.mMaterials, src.mNumMaterials as usize) };
    let mut new_index: Vec<u32> = (0..src_mats.len() as u32).collect();
    let atlas_semantic_paths = |semantic: u32| -> Option<&String> {
        options.atlas_textures.get(&channel_for(semantic, 0))
    };

    if options.merge_materials && !remaps.is_empty() {
        let (&base_index, base_remap) = remaps.iter().next().expect("non-empty");
        let base = unsafe { material_props(src_mats[base_index]) };
        let mut props: Vec<*mut sys::aiMaterialProperty> = base
            .iter()
            .filter(|(_, q)| !q.key.starts_with("$tex.") && q.key != KEY_MAT_NAME)
            .map(|(p, _)| *p)
            .collect();
        let name = options
            .merged_material_name
            .clone()
            .unwrap_or_else(|| "AtlasMaterial".into());
        props.push(es.own(OwnedProp::string(KEY_MAT_NAME, 0, 0, &name)?));
        for (channel, path) in &options.atlas_textures {
            let semantic = semantic_for(channel);
            props.push(es.own(OwnedProp::string(KEY_TEX_FILE, semantic, 0, path)?));
            props.push(es.own(OwnedProp::int(
                KEY_TEX_UVWSRC,
                semantic,
                0,
                base_remap.uv_channel as i32,
            )?));
        }
        es.add_material(props);
        let mut next = 1u32;
        for (i, &mp) in src_mats.iter().enumerate() {
            if remaps.contains_key(&i) {
                new_index[i] = 0;
            } else {
                new_index[i] = next;
                next += 1;
                let props = unsafe { material_props(mp) }
                    .into_iter()
                    .map(|(p, _)| p)
                    .collect();
                es.add_material(props);
            }
        }
    } else {
        for (i, &mp) in src_mats.iter().enumerate() {
            let props = unsafe { material_props(mp) };
            let Some(remap) = remaps.get(&i) else {
                es.add_material(props.into_iter().map(|(p, _)| p).collect());
                continue;
            };
            let mut list = Vec::with_capacity(props.len());
            for (p, q) in &props {
                let uv = find_int(&props, KEY_TEX_UVWSRC, q.semantic, q.index)
                    .unwrap_or(0)
                    .max(0) as u32;
                match atlas_semantic_paths(q.semantic) {
                    Some(path)
                        if q.key == KEY_TEX_FILE && q.index == 0 && uv == remap.uv_channel =>
                    {
                        list.push(es.own(OwnedProp::string(
                            KEY_TEX_FILE,
                            q.semantic,
                            q.index,
                            path,
                        )?));
                    }
                    _ => list.push(*p),
                }
            }
            es.add_material(list);
        }
    }

    // --- meshes
    let src_meshes = unsafe { slice(src.mMeshes, src.mNumMeshes as usize) };
    for (i, &mp) in src_meshes.iter().enumerate() {
        // SAFETY: bitwise copy of a valid mesh; we only replace pointer fields
        // with Rust-owned buffers and never free the copy through Assimp.
        let mut mesh: sys::aiMesh = unsafe { *mp };
        let mi = mesh.mMaterialIndex as usize;
        if let Some(&ni) = new_index.get(mi) {
            mesh.mMaterialIndex = ni;
        }
        if let (Some(Some(new_uvs)), Some(remap)) = (uvs.get(i), remaps.get(&mi)) {
            let ch = remap.uv_channel as usize;
            let n = mesh.mNumVertices as usize;
            if ch >= MAX_UV_CHANNELS || new_uvs.len() != n {
                return Err(fail(format!(
                    "uv buffer mismatch on mesh {}",
                    model.meshes.get(i).map(|m| m.name.as_str()).unwrap_or("")
                )));
            }
            let old = unsafe { slice(mesh.mTextureCoords[ch], n) };
            let mut buf: Vec<sys::aiVector3D> = new_uvs
                .iter()
                .enumerate()
                .map(|(k, uv)| sys::aiVector3D {
                    x: uv[0],
                    y: uv[1],
                    z: old.get(k).map_or(0.0, |v| v.z),
                })
                .collect();
            mesh.mTextureCoords[ch] = buf.as_mut_ptr();
            if mesh.mNumUVComponents[ch] == 0 {
                mesh.mNumUVComponents[ch] = 2;
            }
            es.uv_buffers.push(buf);
        }
        es.meshes.push(mesh);
    }

    // --- scene
    let scene = es.finish(src);

    let c_fmt = CString::new(options.format.assimp_id()).expect("static id");
    let c_out = CString::new(
        out_path
            .to_str()
            .ok_or_else(|| fail("non-UTF-8 path".into()))?,
    )
    .map_err(|e| fail(e.to_string()))?;
    {
        let _g = lock();
        // SAFETY: `scene` and every buffer it references live in `es` until the
        // end of this function; Assimp only reads the input scene.
        let r = unsafe { sys::aiExportScene(&scene, c_fmt.as_ptr(), c_out.as_ptr(), 0) };
        if r != sys::aiReturn::aiReturn_SUCCESS {
            return Err(fail(last_error()));
        }
    }
    drop(es);

    if options.verify_geometry {
        // The lock must not be held when `back` is dropped (Drop locks).
        let back = {
            let _g = lock();
            raw_import(&c_out)
        };
        let err = match &back {
            Ok(b) => bounds_error(world_bounds(src), world_bounds(b.scene())),
            Err(_) => f32::INFINITY,
        };
        drop(back);
        if err > GEOMETRY_TOLERANCE {
            let _ = std::fs::remove_file(out_path);
            return Err(OpError::new(codes::MESH_EXPORT_GEOMETRY_CHANGED)
                .with("path", out_path.display().to_string())
                .with("format", options.format.assimp_id())
                .with("relError", if err.is_finite() { err as f64 } else { -1.0 }));
        }
    }
    Ok(())
}

/// Max world-bbox deviation, relative to the bbox diagonal, accepted by the
/// post-export verification (ASCII FBX float printing gives ~1e-7).
const GEOMETRY_TOLERANCE: f32 = 1e-4;

// --------------------------------------------------------------- embedded

pub(super) fn extract_embedded(
    handle: &SceneHandle,
    model: &Model,
    out_dir: &Path,
) -> OpResult<Vec<(usize, PathBuf)>> {
    let write_err = |p: &Path, e: String| {
        OpError::new(crate::error::codes::IO_WRITE_FAILED)
            .with("path", p.display().to_string())
            .with("detail", e)
    };
    std::fs::create_dir_all(out_dir).map_err(|e| write_err(out_dir, e.to_string()))?;
    let src = handle.scene();
    let texs = unsafe { slice(src.mTextures, src.mNumTextures as usize) };
    let mut out = Vec::new();
    for info in &model.embedded_textures {
        let Some(&tp) = texs.get(info.index) else {
            continue;
        };
        if tp.is_null() {
            continue;
        }
        // SAFETY: valid texture of the live scene.
        let t = unsafe { &*tp };
        let stem = Path::new(&info.file_name.replace('\\', "/"))
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("embedded_{}", info.index));
        if t.mHeight == 0 {
            let ext = if info.format_hint.is_empty() {
                "bin".to_string()
            } else {
                info.format_hint.clone()
            };
            let path = out_dir.join(format!("{stem}.{ext}"));
            let bytes = unsafe { slice(t.pcData as *const u8, t.mWidth as usize) };
            std::fs::write(&path, bytes).map_err(|e| write_err(&path, e.to_string()))?;
            out.push((info.index, path));
        } else {
            let texels = unsafe { slice(t.pcData, t.mWidth as usize * t.mHeight as usize) };
            let img = image::RgbaImage::from_fn(t.mWidth, t.mHeight, |x, y| {
                let px = texels[(y * t.mWidth + x) as usize];
                image::Rgba([px.r, px.g, px.b, px.a])
            });
            let path = out_dir.join(format!("{stem}.png"));
            img.save(&path)
                .map_err(|e| write_err(&path, e.to_string()))?;
            out.push((info.index, path));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_error_is_relative_to_the_diagonal() {
        let a = Some(([0.0, 0.0, 0.0], [3.0, 4.0, 0.0])); // diagonal 5
        assert_eq!(bounds_error(a, a), 0.0);
        let b = Some(([0.0, -1.0, 0.0], [3.0, 4.0, 0.0]));
        assert!((bounds_error(a, b) - 0.2).abs() < 1e-6);
        assert_eq!(bounds_error(None, None), 0.0);
        assert_eq!(bounds_error(a, None), f32::INFINITY);
    }

    #[test]
    fn semantic_channel_mapping() {
        assert_eq!(channel_for(1, 0), TextureChannel::BaseColor);
        assert_eq!(channel_for(12, 0), TextureChannel::BaseColor);
        assert_eq!(channel_for(5, 0), TextureChannel::Height);
        assert_eq!(
            channel_for(1, 1),
            TextureChannel::Other("baseColor#1".into())
        );
        assert_eq!(channel_for(99, 0), TextureChannel::Other("type99".into()));
        assert_eq!(semantic_for(&TextureChannel::BaseColor), 1);
        assert_eq!(semantic_for(&TextureChannel::Normal), 6);
        assert_eq!(semantic_for(&TextureChannel::Other("shininess".into())), 7);
        assert!(semantic_rank(1) < semantic_rank(12));
    }

    #[test]
    fn material_property_payloads() {
        let p = OwnedProp::string(KEY_TEX_FILE, 1, 0, "atlas.png").unwrap();
        assert_eq!(p.prop.mDataLength, 4 + 9 + 1);
        assert_eq!(prop_string(&p._data).as_deref(), Some("atlas.png"));
        let i = OwnedProp::int(KEY_TEX_UVWSRC, 1, 0, 3).unwrap();
        assert_eq!(prop_int(&i._data), Some(3));
        assert_eq!(ai_string(&i.prop.mKey), KEY_TEX_UVWSRC);
        assert!(OwnedProp::string(KEY_TEX_FILE, 1, 0, &"x".repeat(2000)).is_err());
    }
}
