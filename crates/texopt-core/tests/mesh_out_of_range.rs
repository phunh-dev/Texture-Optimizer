//! UVs outside [0,1]: detection, each policy, and export of the result.
#![cfg(feature = "assimp")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use image::Rgba;
use texopt_core::ImageBuf;
use texopt_core::fixtures::{GREEN, solid};
use texopt_core::mesh::uv_remap::{
    AtlasRect, InsetPolicy, MaterialPlan, OutOfRangePolicy, UvNormalize, UvOrigin, UvRemap,
    atlas_transform, uv_to_pixel,
};
use texopt_core::mesh::{
    self, ExportFormat, ExportOptions, LoadedModel, MaterialRemap, ModelRemaps, TextureChannel,
    codes, fixtures,
};

const LEFT: Rgba<u8> = Rgba([250, 200, 0, 255]);
const RIGHT: Rgba<u8> = Rgba([0, 120, 250, 255]);

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("mesh_oor")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn load(name: &str) -> (PathBuf, LoadedModel) {
    let dir = scratch(name);
    let path = fixtures::tiled_quads_obj(&dir);
    (dir, mesh::import(&path).unwrap())
}

fn idx(l: &LoadedModel, name: &str) -> usize {
    l.model
        .materials
        .iter()
        .position(|m| m.name == name)
        .unwrap()
}

fn options() -> ExportOptions {
    let mut atlas = BTreeMap::new();
    atlas.insert(TextureChannel::BaseColor, "atlas.png".to_string());
    ExportOptions {
        format: ExportFormat::Fbx,
        merge_materials: false,
        merged_material_name: None,
        verify_geometry: true,
        atlas_textures: atlas,
    }
}

#[test]
fn detects_out_of_range_per_material() {
    let (_, l) = load("detect");
    let a = l.model.material_uv_range(idx(&l, "MatA"), 0).unwrap();
    assert!(a.out_of_range);
    assert_eq!((a.min, a.max), ([0.0, 0.0], [2.0, 1.0]));
    let b = l.model.material_uv_range(idx(&l, "MatB"), 0).unwrap();
    assert!(b.out_of_range);
    assert_eq!((b.min, b.max), ([1.25, 0.25], [1.75, 0.75]));
    assert!(l.model.material_uv_range(idx(&l, "MatA"), 5).is_none());

    let in_range = mesh::import(&fixtures::two_quads_obj(&scratch("detect_in"))).unwrap();
    let mi = in_range
        .model
        .materials
        .iter()
        .position(|m| m.name == "MatA")
        .unwrap();
    assert!(
        !in_range
            .model
            .material_uv_range(mi, 0)
            .unwrap()
            .out_of_range
    );
}

#[test]
fn skip_policy_warns_and_leaves_material_untouched() {
    let (dir, l) = load("skip");
    let (analysis, warnings) =
        mesh::analyze_materials(&l.model, OutOfRangePolicy::SkipMaterial).unwrap();
    assert_eq!(analysis.len(), 2);
    assert!(analysis.iter().all(|a| a.plan == MaterialPlan::Skip));
    assert_eq!(warnings.len(), 2);
    assert!(
        warnings
            .iter()
            .all(|w| w.code == codes::MESH_UV_OUT_OF_RANGE)
    );
    assert_eq!(warnings[0].params["max"], serde_json::json!([2.0, 1.0]));
    // Nothing remapped → export keeps original UVs and textures.
    let out = dir.join("out/skip.fbx");
    mesh::export(&l, &ModelRemaps::new(), &options(), &out).unwrap();
    let back = mesh::import(&out).unwrap().model;
    let a = back.materials.iter().find(|m| m.name == "MatA").unwrap();
    assert_eq!(
        a.textures[&TextureChannel::BaseColor].raw_path,
        "tex/red.png"
    );
    let mesh_a = back
        .meshes
        .iter()
        .find(|m| back.materials[m.material_index].name == "MatA")
        .unwrap();
    let mut got = mesh_a.uv_channels[0].clone();
    got.sort_by(|x, y| x.partial_cmp(y).unwrap());
    let mut want = fixtures::TILED_A_UVS.to_vec();
    want.sort_by(|x, y| x.partial_cmp(y).unwrap());
    assert_eq!(got, want);
}

fn remap_all(l: &LoadedModel, policy: OutOfRangePolicy) -> ModelRemaps {
    let (analysis, _) = mesh::analyze_materials(&l.model, policy).unwrap();
    let rects = [
        AtlasRect {
            x: 0,
            y: 0,
            width: 64,
            height: 32,
        },
        AtlasRect {
            x: 0,
            y: 32,
            width: 64,
            height: 32,
        },
    ];
    analysis
        .iter()
        .zip(rects)
        .filter_map(|(a, rect)| match a.plan {
            MaterialPlan::Skip => None,
            MaterialPlan::Remap { normalize } => {
                let t =
                    atlas_transform(rect, [64, 64], InsetPolicy::HalfTexel, UvOrigin::BottomLeft)
                        .unwrap();
                Some((
                    a.material_index,
                    MaterialRemap {
                        uv_channel: a.uv_channel,
                        remap: UvRemap::new(normalize, t),
                    },
                ))
            }
        })
        .collect()
}

fn export_and_reimport(
    l: &LoadedModel,
    remaps: &ModelRemaps,
    out: &Path,
) -> (mesh::ExportReport, mesh::Model) {
    let report = mesh::export(l, remaps, &options(), out).unwrap();
    (report, mesh::import(out).unwrap().model)
}

fn assert_exported_uvs(l: &LoadedModel, remaps: &ModelRemaps, back: &mesh::Model) {
    let (expected, _) = mesh::remap_model_uvs(&l.model, remaps).unwrap();
    for (i, m) in l
        .model
        .meshes
        .iter()
        .enumerate()
        .filter(|(_, m)| m.vertex_count > 0)
    {
        let b = back
            .meshes
            .iter()
            .find(|b| {
                back.materials[b.material_index].name == l.model.materials[m.material_index].name
            })
            .unwrap();
        let want = expected[i].as_ref().unwrap();
        for (g, w) in b.uv_channels[0].iter().zip(want) {
            assert!(
                (g[0] - w[0]).abs() < 1e-5 && (g[1] - w[1]).abs() < 1e-5,
                "{:?} vs {want:?}",
                b.uv_channels[0]
            );
        }
    }
}

#[test]
fn clamp_policy() {
    let (dir, l) = load("clamp");
    let remaps = remap_all(&l, OutOfRangePolicy::Clamp);
    assert_eq!(remaps.len(), 2);
    assert!(
        remaps
            .values()
            .all(|r| r.remap.normalize == UvNormalize::Clamp)
    );
    let (report, back) = export_and_reimport(&l, &remaps, &dir.join("out/clamp.fbx"));
    assert!(report.warnings.is_empty());
    assert_exported_uvs(&l, &remaps, &back);
    // Every remapped UV lies inside its atlas rect.
    for m in back.meshes.iter().filter(|m| m.vertex_count > 0) {
        for uv in &m.uv_channels[0] {
            assert!((0.0..=1.0).contains(&uv[0]) && (0.0..=1.0).contains(&uv[1]));
        }
    }
}

#[test]
fn wrap_policy_reports_straddling_faces() {
    let (dir, l) = load("wrap");
    let remaps = remap_all(&l, OutOfRangePolicy::WrapIntoTile);
    assert!(
        remaps
            .values()
            .all(|r| r.remap.normalize == UvNormalize::Wrap)
    );
    let (report, back) = export_and_reimport(&l, &remaps, &dir.join("out/wrap.fbx"));
    // MatA spans u ∈ [0,2] → its single quad straddles; MatB lives in tile 1 → clean.
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert_eq!(report.warnings[0].code, codes::MESH_UV_WRAP_STRADDLE);
    assert_eq!(report.warnings[0].params["mesh"], "QuadA");
    assert_eq!(report.warnings[0].params["faces"], 1);
    assert_exported_uvs(&l, &remaps, &back);
    // MatB was moved from tile 1 to tile 0 exactly: same as the in-range quad.
    let mb = idx(&l, "MatB");
    let t = remaps[&mb].remap.transform;
    let mesh_b = back
        .meshes
        .iter()
        .find(|m| back.materials[m.material_index].name == "MatB")
        .unwrap();
    let mut got = mesh_b.uv_channels[0].clone();
    let mut want: Vec<[f32; 2]> = fixtures::QUAD_B_UVS.iter().map(|&uv| t.apply(uv)).collect();
    got.sort_by(|x, y| x.partial_cmp(y).unwrap());
    want.sort_by(|x, y| x.partial_cmp(y).unwrap());
    for (g, w) in got.iter().zip(&want) {
        assert!((g[0] - w[0]).abs() < 1e-5 && (g[1] - w[1]).abs() < 1e-5);
    }
}

#[test]
fn bake_repeat_tile_counts_and_limit() {
    let (_, l) = load("repeat_limit");
    let (analysis, _) =
        mesh::analyze_materials(&l.model, OutOfRangePolicy::BakeRepeat { max_tiles: 4 }).unwrap();
    let a = analysis
        .iter()
        .find(|x| x.material_index == idx(&l, "MatA"))
        .unwrap();
    assert_eq!(
        a.plan,
        MaterialPlan::Remap {
            normalize: UvNormalize::Repeat {
                origin: [0, 0],
                tiles: [2, 1]
            }
        }
    );
    assert_eq!(a.plan.repeat_tiles(), [2, 1]);
    let b = analysis
        .iter()
        .find(|x| x.material_index == idx(&l, "MatB"))
        .unwrap();
    assert_eq!(
        b.plan,
        MaterialPlan::Remap {
            normalize: UvNormalize::Repeat {
                origin: [1, 0],
                tiles: [1, 1]
            }
        }
    );
    let err = mesh::analyze_materials(&l.model, OutOfRangePolicy::BakeRepeat { max_tiles: 1 })
        .unwrap_err();
    assert_eq!(err.code, codes::MESH_UV_OUT_OF_RANGE);
    assert_eq!(err.params["material"], "MatA");
    assert_eq!(err.params["tilesU"], 2);
}

fn sample(img: &ImageBuf, uv: [f32; 2]) -> Rgba<u8> {
    let p = uv_to_pixel(uv, [img.width(), img.height()], UvOrigin::BottomLeft);
    *img.get_pixel(
        (p[0].floor() as u32).min(img.width() - 1),
        (p[1].floor() as u32).min(img.height() - 1),
    )
}

/// MatA's texture is two-coloured (left/right halves) and tiled twice in u.
/// The packer stand-in stores it repeated 2×1 in the atlas rect; sampling the
/// atlas at remapped UVs must equal sampling the texture at `fract(uv)`.
#[test]
fn bake_repeat_sampling_matches_tiled_texture() {
    let (dir, _) = load("repeat_sampling");
    let mut tex = solid(8, 8, LEFT);
    for y in 0..8 {
        for x in 4..8 {
            tex.put_pixel(x, y, RIGHT);
        }
    }
    tex.save(dir.join("tex/red.png")).unwrap();
    let l = mesh::import(&dir.join("tiled.obj")).unwrap();
    let remaps = remap_all(&l, OutOfRangePolicy::BakeRepeat { max_tiles: 4 });
    let ma = idx(&l, "MatA");
    let ra = remaps[&ma];
    let UvNormalize::Repeat { tiles, .. } = ra.remap.normalize else {
        panic!("expected repeat")
    };
    assert_eq!(tiles, [2, 1]);

    // Atlas: rect (0,0,64,32) holds the texture repeated 2×1, each copy 32×32.
    let mut atlas = solid(64, 64, GREEN);
    for y in 0..32 {
        for x in 0..64 {
            atlas.put_pixel(x, y, *tex.get_pixel((x % 32) * 8 / 32, y * 8 / 32));
        }
    }
    atlas.save(dir.join("atlas.png")).unwrap();
    let out = dir.join("repeat.fbx");
    mesh::export(&l, &remaps, &options(), &out).unwrap();
    let back = mesh::import(&out).unwrap();
    let mat = back
        .model
        .materials
        .iter()
        .find(|m| m.name == "MatA")
        .unwrap();
    let atlas_back =
        texopt_core::io::load_image(&mat.textures[&TextureChannel::BaseColor].path).unwrap();
    assert_exported_uvs(&l, &remaps, &back.model);

    for u in [
        0.1f32, 0.3, 0.45, 0.55, 0.7, 0.9, 1.1, 1.3, 1.45, 1.55, 1.7, 1.9,
    ] {
        for v in [0.1f32, 0.5, 0.9] {
            let want = sample(&tex, [u.fract(), v]);
            assert_eq!(
                sample(&atlas_back, ra.remap.apply([u, v])),
                want,
                "uv ({u}, {v})"
            );
        }
    }
}
