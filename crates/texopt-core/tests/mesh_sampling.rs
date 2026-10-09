//! End-to-end sampling check: two solid-colour textures are placed into a
//! fake atlas, the model is remapped/exported/re-imported, and sampling the
//! atlas (loaded through the re-imported material's texture reference) at the
//! new UVs must give back each mesh's original colour.
#![cfg(feature = "assimp")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use image::Rgba;
use texopt_core::ImageBuf;
use texopt_core::fixtures::{GREEN, solid};
use texopt_core::mesh::uv_remap::{
    AtlasRect, InsetPolicy, UvNormalize, UvOrigin, UvRemap, atlas_transform, uv_to_pixel,
};
use texopt_core::mesh::{
    self, ExportFormat, ExportOptions, MaterialRemap, Mesh, TextureChannel, fixtures,
};

const ATLAS: [u32; 2] = [64, 64];
// MatA top-left, MatB bottom-right; the rest is green so a V-flip mistake
// (or an off-by-one at the edges) samples green.
const RECT_A: AtlasRect = AtlasRect {
    x: 0,
    y: 0,
    width: 32,
    height: 32,
};
const RECT_B: AtlasRect = AtlasRect {
    x: 32,
    y: 32,
    width: 32,
    height: 32,
};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("mesh_sampling")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn blit(atlas: &mut ImageBuf, src: &ImageBuf, rect: AtlasRect) {
    // Nearest-neighbour stretch of the source texture into the rect (stand-in for the packer).
    for y in 0..rect.height {
        for x in 0..rect.width {
            let sx = x * src.width() / rect.width;
            let sy = y * src.height() / rect.height;
            atlas.put_pixel(rect.x + x, rect.y + y, *src.get_pixel(sx, sy));
        }
    }
}

fn sample(img: &ImageBuf, uv: [f32; 2]) -> Rgba<u8> {
    let p = uv_to_pixel(uv, [img.width(), img.height()], UvOrigin::BottomLeft);
    let x = (p[0].floor() as i64).clamp(0, img.width() as i64 - 1) as u32;
    let y = (p[1].floor() as i64).clamp(0, img.height() as i64 - 1) as u32;
    *img.get_pixel(x, y)
}

/// Vertex UVs plus a grid of interior points of every face (bilinear for
/// quads, barycentric for triangles — glTF output is triangulated).
fn sample_points(mesh: &Mesh) -> Vec<[f32; 2]> {
    let lerp =
        |a: [f32; 2], b: [f32; 2], k: f32| [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k];
    let uvs = &mesh.uv_channels[0];
    let mut pts = uvs.clone();
    assert!(!mesh.faces.is_empty());
    for face in &mesh.faces {
        let c: Vec<[f32; 2]> = face.iter().map(|&i| uvs[i as usize]).collect();
        for i in 0..=4 {
            for j in 0..=4 {
                let (s, t) = (i as f32 / 4.0, j as f32 / 4.0);
                match c.len() {
                    4 => pts.push(lerp(lerp(c[0], c[1], s), lerp(c[3], c[2], s), t)),
                    3 if i + j <= 4 => {
                        let w0 = 1.0 - s - t;
                        pts.push([
                            w0 * c[0][0] + s * c[1][0] + t * c[2][0],
                            w0 * c[0][1] + s * c[1][1] + t * c[2][1],
                        ]);
                    }
                    3 => {}
                    n => panic!("unexpected face with {n} corners"),
                }
            }
        }
    }
    pts
}

fn check(kind: &str, format: ExportFormat) {
    let ctx = format!("{kind} -> {format:?}");
    let dir = scratch(&format!("{kind}_{}", format.assimp_id()));
    let src = match kind {
        "obj" => fixtures::two_quads_obj(&dir.join("src")),
        "dae" => fixtures::two_quads_dae(&dir.join("src")),
        _ => fixtures::two_quads_fbx(&dir.join("src")).unwrap(),
    };
    let loaded = mesh::import(&src).unwrap();
    let model = &loaded.model;

    // Fake atlas + remaps.
    let out_dir = dir.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    let mut atlas = solid(ATLAS[0], ATLAS[1], GREEN);
    let mut remaps = BTreeMap::new();
    let mut expected_color = BTreeMap::new();
    for (name, rect) in [("MatA", RECT_A), ("MatB", RECT_B)] {
        let mi = model.materials.iter().position(|m| m.name == name).unwrap();
        let tex = texopt_core::io::load_image(
            &model.materials[mi].textures[&TextureChannel::BaseColor].path,
        )
        .unwrap();
        blit(&mut atlas, &tex, rect);
        expected_color.insert(mi, *tex.get_pixel(0, 0));
        let t = atlas_transform(rect, ATLAS, InsetPolicy::HalfTexel, UvOrigin::BottomLeft).unwrap();
        remaps.insert(
            mi,
            MaterialRemap {
                uv_channel: 0,
                remap: UvRemap::new(UvNormalize::None, t),
            },
        );
    }
    atlas.save(out_dir.join("atlas_baseColor.png")).unwrap();
    let mut atlas_textures = BTreeMap::new();
    for m in &model.materials {
        for c in m.textures.keys() {
            atlas_textures.insert(c.clone(), format!("atlas_{c}.png"));
        }
    }
    let options = ExportOptions {
        format,
        merge_materials: true,
        merged_material_name: None,
        verify_geometry: true,
        atlas_textures,
    };
    let out = out_dir.join(format!("model.{}", format.extension()));
    mesh::export(&loaded, &remaps, &options, &out).unwrap();

    let back = mesh::import(&out).unwrap().model;
    let before: Vec<&Mesh> = model.meshes.iter().filter(|m| m.vertex_count > 0).collect();
    let after: Vec<&Mesh> = back.meshes.iter().filter(|m| m.vertex_count > 0).collect();
    assert_eq!(before.len(), after.len(), "{ctx}");
    for (b, a) in before.iter().zip(&after) {
        let atlas_ref = &back.materials[a.material_index].textures[&TextureChannel::BaseColor];
        assert!(atlas_ref.exists, "{ctx}: {atlas_ref:?}");
        let atlas_img = texopt_core::io::load_image(&atlas_ref.path).unwrap();
        let want = expected_color[&b.material_index];
        for uv in sample_points(a) {
            assert_eq!(
                sample(&atlas_img, uv),
                want,
                "{ctx}: mesh {} at uv {uv:?}",
                b.name
            );
        }
    }
}

#[test]
fn sampling_obj_dae_fbx_sources() {
    for kind in ["obj", "dae", "fbx"] {
        for format in [
            ExportFormat::Obj,
            ExportFormat::Collada,
            ExportFormat::Fbx,
            ExportFormat::FbxAscii,
        ] {
            check(kind, format);
        }
    }
}

/// Without the half-texel inset the corner UVs land exactly on the rect edge;
/// with `floor()` sampling the far edge (u = 1) touches the neighbour texel.
/// This documents why the inset is the default.
#[test]
fn zero_inset_touches_neighbour_texels() {
    let t = atlas_transform(RECT_A, ATLAS, InsetPolicy::None, UvOrigin::BottomLeft).unwrap();
    let p = uv_to_pixel(t.apply([1.0, 0.0]), ATLAS, UvOrigin::BottomLeft);
    assert_eq!(
        [p[0].floor(), p[1].floor()],
        [32.0, 32.0],
        "outside RECT_A (0..32)"
    );
    let t = atlas_transform(RECT_A, ATLAS, InsetPolicy::HalfTexel, UvOrigin::BottomLeft).unwrap();
    let p = uv_to_pixel(t.apply([1.0, 0.0]), ATLAS, UvOrigin::BottomLeft);
    assert_eq!([p[0].floor(), p[1].floor()], [31.0, 31.0], "inside RECT_A");
}
