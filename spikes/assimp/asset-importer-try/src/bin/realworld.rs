//! Spike part 2: third-party FBX files (written by the Autodesk FBX SDK, e.g.
//! the sample models shipped with the Unity editor) through the production
//! `texopt_core::mesh` pipeline: import → remap every textured material into a
//! quarter of a fake atlas → export to each format → re-import, then compare
//! UVs and (via raw Assimp) world-space bounding boxes, node / bone /
//! animation / normal / tangent / vertex-colour counts.
//!
//! Run: `cargo run --release --bin realworld -- <out_dir> <model.fbx>...`

use std::collections::BTreeMap;
use std::ffi::CString;
use std::path::{Path, PathBuf};

use asset_importer_sys as sys;
use texopt_core::mesh::uv_remap::{
    AtlasRect, InsetPolicy, UvNormalize, UvOrigin, UvRemap, atlas_transform,
};
use texopt_core::mesh::{
    self, ExportFormat, ExportOptions, MaterialRemap, ModelRemaps, OutOfRangePolicy, TextureChannel,
};

#[derive(Debug, Default, PartialEq)]
struct RawStats {
    nodes: usize,
    meshes: usize,
    vertices: usize,
    faces: usize,
    bones: usize,
    animations: usize,
    meshes_with_normals: usize,
    meshes_with_tangents: usize,
    meshes_with_colors: usize,
    uv_channels: usize,
    bbox_min: [f32; 3],
    bbox_max: [f32; 3],
    pivot_kinds: std::collections::BTreeSet<String>,
}

fn mul(a: &sys::aiMatrix4x4, b: &sys::aiMatrix4x4) -> sys::aiMatrix4x4 {
    let ar = [
        [a.a1, a.a2, a.a3, a.a4],
        [a.b1, a.b2, a.b3, a.b4],
        [a.c1, a.c2, a.c3, a.c4],
        [a.d1, a.d2, a.d3, a.d4],
    ];
    let br = [
        [b.a1, b.a2, b.a3, b.a4],
        [b.b1, b.b2, b.b3, b.b4],
        [b.c1, b.c2, b.c3, b.c4],
        [b.d1, b.d2, b.d3, b.d4],
    ];
    let mut r = [[0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            r[i][j] = (0..4).map(|k| ar[i][k] * br[k][j]).sum();
        }
    }
    sys::aiMatrix4x4 {
        a1: r[0][0],
        a2: r[0][1],
        a3: r[0][2],
        a4: r[0][3],
        b1: r[1][0],
        b2: r[1][1],
        b3: r[1][2],
        b4: r[1][3],
        c1: r[2][0],
        c2: r[2][1],
        c3: r[2][2],
        c4: r[2][3],
        d1: r[3][0],
        d2: r[3][1],
        d3: r[3][2],
        d4: r[3][3],
    }
}

unsafe fn walk(
    scene: &sys::aiScene,
    node: *const sys::aiNode,
    parent: &sys::aiMatrix4x4,
    st: &mut RawStats,
) {
    let n = unsafe { &*node };
    st.nodes += 1;
    if std::env::var("NODES").is_ok() {
        let len = n.mName.length as usize;
        let name: String = n.mName.data[..len]
            .iter()
            .map(|&c| c as u8 as char)
            .collect();
        if let Some(pos) = name.find("$AssimpFbx$") {
            st.pivot_kinds.insert(name[pos..].to_string());
        }
    }
    let m = mul(parent, &n.mTransformation);
    for k in 0..n.mNumMeshes as usize {
        let mi = unsafe { *n.mMeshes.add(k) } as usize;
        let mesh = unsafe { &**scene.mMeshes.add(mi) };
        for v in 0..mesh.mNumVertices as usize {
            let p = unsafe { *mesh.mVertices.add(v) };
            let w = [
                m.a1 * p.x + m.a2 * p.y + m.a3 * p.z + m.a4,
                m.b1 * p.x + m.b2 * p.y + m.b3 * p.z + m.b4,
                m.c1 * p.x + m.c2 * p.y + m.c3 * p.z + m.c4,
            ];
            for a in 0..3 {
                st.bbox_min[a] = st.bbox_min[a].min(w[a]);
                st.bbox_max[a] = st.bbox_max[a].max(w[a]);
            }
        }
    }
    for c in 0..n.mNumChildren as usize {
        unsafe { walk(scene, *n.mChildren.add(c), &m, st) };
    }
}

fn raw_stats(path: &Path) -> RawStats {
    let c = CString::new(path.to_str().unwrap()).unwrap();
    let flags = sys::aiPostProcessSteps::aiProcess_JoinIdenticalVertices as u32;
    // NOPIVOTS=1 → same import settings as texopt_core::mesh::import.
    let s = unsafe {
        let store = sys::aiCreatePropertyStore();
        if std::env::var("NOPIVOTS").is_ok() {
            sys::aiSetImportPropertyInteger(store, c"IMPORT_FBX_PRESERVE_PIVOTS".as_ptr(), 0);
        }
        let s = sys::aiImportFileExWithProperties(c.as_ptr(), flags, std::ptr::null_mut(), store);
        sys::aiReleasePropertyStore(store);
        s
    };
    assert!(!s.is_null(), "raw import failed {}", path.display());
    let scene = unsafe { &*s };
    let mut st = RawStats {
        bbox_min: [f32::MAX; 3],
        bbox_max: [f32::MIN; 3],
        ..Default::default()
    };
    st.meshes = scene.mNumMeshes as usize;
    st.animations = scene.mNumAnimations as usize;
    for i in 0..st.meshes {
        let m = unsafe { &**scene.mMeshes.add(i) };
        st.vertices += m.mNumVertices as usize;
        st.faces += m.mNumFaces as usize;
        st.bones += m.mNumBones as usize;
        st.meshes_with_normals += usize::from(!m.mNormals.is_null());
        st.meshes_with_tangents += usize::from(!m.mTangents.is_null());
        st.meshes_with_colors += usize::from(!m.mColors[0].is_null());
        st.uv_channels = st
            .uv_channels
            .max(m.mTextureCoords.iter().filter(|p| !p.is_null()).count());
    }
    let ident = sys::aiMatrix4x4 {
        a1: 1.0,
        a2: 0.0,
        a3: 0.0,
        a4: 0.0,
        b1: 0.0,
        b2: 1.0,
        b3: 0.0,
        b4: 0.0,
        c1: 0.0,
        c2: 0.0,
        c3: 1.0,
        c4: 0.0,
        d1: 0.0,
        d2: 0.0,
        d3: 0.0,
        d4: 1.0,
    };
    unsafe {
        walk(scene, scene.mRootNode, &ident, &mut st);
        sys::aiReleaseImport(s);
    }
    st
}

fn main() {
    let mut args = std::env::args().skip(1);
    let first = args.next().expect("out dir");
    if first == "--stats" {
        for f in args {
            println!("{f}\n  {:?}", raw_stats(Path::new(&f)));
        }
        return;
    }
    let out_root = PathBuf::from(first);
    for input in args {
        let input = PathBuf::from(input);
        let name = input.file_stem().unwrap().to_string_lossy().into_owned();
        println!("==================== {}", input.display());
        let t = std::time::Instant::now();
        let loaded = match mesh::import(&input) {
            Ok(l) => l,
            Err(e) => {
                println!("IMPORT FAILED: {e:?}");
                continue;
            }
        };
        println!("import: {:?}", t.elapsed());
        let m = &loaded.model;
        println!(
            "meshes={} materials={} embedded={:?} warnings={:?}",
            m.meshes.len(),
            m.materials.len(),
            m.embedded_textures,
            m.warnings
                .iter()
                .map(|w| (&w.code, w.params.get("channel"), w.params.get("path")))
                .collect::<Vec<_>>()
        );
        for (i, mat) in m.materials.iter().enumerate() {
            let tex: Vec<String> = mat
                .textures
                .iter()
                .map(|(c, t)| format!("{c}={} (uv{})", t.raw_path, t.uv_channel))
                .collect();
            println!(
                "  material[{i}] '{}' {:?} range={:?}",
                mat.name,
                tex,
                m.material_uv_range(i, 0)
            );
        }
        if !m.embedded_textures.is_empty() {
            let ex =
                mesh::extract_embedded_textures(&loaded, &out_root.join(&name).join("embedded"))
                    .unwrap();
            println!("  extracted embedded: {ex:?}");
        }
        let before = raw_stats(&input);
        println!("  raw source: {before:?}");

        // Remap every material that has UV0 into one of 4 quadrants, ignoring
        // textures (so untextured materials are exercised too).
        let (analysis, warn) = mesh::analyze_materials(m, OutOfRangePolicy::WrapIntoTile).unwrap();
        println!(
            "  analysis: {} textured materials, warnings {:?}",
            analysis.len(),
            warn.iter().map(|w| &w.code).collect::<Vec<_>>()
        );
        let mut remaps = ModelRemaps::new();
        for (i, _) in m.materials.iter().enumerate() {
            if m.material_uv_range(i, 0).is_none() {
                continue;
            }
            let q = (i % 4) as u32;
            let rect = AtlasRect {
                x: (q % 2) * 512,
                y: (q / 2) * 512,
                width: 512,
                height: 512,
            };
            let t = atlas_transform(
                rect,
                [1024, 1024],
                InsetPolicy::HalfTexel,
                UvOrigin::BottomLeft,
            )
            .unwrap();
            let range = m.material_uv_range(i, 0).unwrap();
            let normalize = if range.out_of_range {
                UvNormalize::Wrap
            } else {
                UvNormalize::None
            };
            remaps.insert(
                i,
                MaterialRemap {
                    uv_channel: 0,
                    remap: UvRemap::new(normalize, t),
                },
            );
        }
        let mut atlas = BTreeMap::new();
        for mat in &m.materials {
            for c in mat.textures.keys() {
                atlas.insert(c.clone(), format!("atlas_{c}.png"));
            }
        }
        atlas
            .entry(TextureChannel::BaseColor)
            .or_insert_with(|| "atlas_baseColor.png".into());
        let (expected, _) = mesh::remap_model_uvs(m, &remaps).unwrap();

        for format in [
            ExportFormat::Fbx,
            ExportFormat::FbxAscii,
            ExportFormat::Obj,
            ExportFormat::Collada,
        ] {
            for merge in [false, true] {
                let out = out_root.join(&name).join(format!(
                    "{}_{merge}.{}",
                    format.assimp_id(),
                    format.extension()
                ));
                let opts = ExportOptions {
                    format,
                    merge_materials: merge,
                    merged_material_name: None,
                    verify_geometry: std::env::var("NOVERIFY").is_err(),
                    atlas_textures: atlas.clone(),
                };
                let t = std::time::Instant::now();
                if let Err(e) = mesh::export(&loaded, &remaps, &opts, &out) {
                    println!("  {format:?} merge={merge}: EXPORT FAILED {e:?}");
                    continue;
                }
                let dt = t.elapsed();
                let back = match mesh::import(&out) {
                    Ok(b) => b,
                    Err(e) => {
                        println!("  {format:?} merge={merge}: REIMPORT FAILED {e:?}");
                        continue;
                    }
                };
                // UV comparison mesh-by-mesh (same order expected).
                let mut max_err = 0f32;
                let mut vert_mismatch = 0;
                let mut compared = 0;
                for (i, (a, b)) in m.meshes.iter().zip(&back.model.meshes).enumerate() {
                    if a.vertex_count != b.vertex_count {
                        vert_mismatch += 1;
                        if merge {
                            println!(
                                "      mesh[{i}] '{}' {} verts {} uvch -> '{}' {} verts {} uvch",
                                a.name,
                                a.vertex_count,
                                a.uv_channels.len(),
                                b.name,
                                b.vertex_count,
                                b.uv_channels.len()
                            );
                        }
                        continue;
                    }
                    let want = expected[i]
                        .as_ref()
                        .map(|v| v.as_slice())
                        .or(a.uv_channels.first().map(|v| v.as_slice()))
                        .unwrap_or(&[]);
                    let got = b.uv_channels.first().map(|v| v.as_slice()).unwrap_or(&[]);
                    if want.len() != got.len() {
                        vert_mismatch += 1;
                        continue;
                    }
                    compared += 1;
                    for (w, g) in want.iter().zip(got) {
                        max_err = max_err.max((w[0] - g[0]).abs()).max((w[1] - g[1]).abs());
                    }
                }
                let after = raw_stats(&out);
                let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
                let bbox_err = (0..3)
                    .map(|a| {
                        (before.bbox_min[a] - after.bbox_min[a])
                            .abs()
                            .max((before.bbox_max[a] - after.bbox_max[a]).abs())
                    })
                    .fold(0f32, f32::max);
                println!(
                    "  {format:?} merge={merge}: {size} B, export {dt:?}; meshes {}->{} compared={compared} vert_mismatch={vert_mismatch} max_uv_err={max_err:e}; mats {}; bbox_err={bbox_err:e}",
                    m.meshes.len(),
                    back.model.meshes.len(),
                    back.model.materials.len(),
                );
                if after.nodes != before.nodes
                    || after.bones != before.bones
                    || after.animations != before.animations
                    || after.meshes_with_tangents != before.meshes_with_tangents
                    || after.meshes_with_colors != before.meshes_with_colors
                    || after.meshes_with_normals != before.meshes_with_normals
                    || after.uv_channels != before.uv_channels
                {
                    println!("      raw after: {after:?}");
                }
            }
        }
    }
}
