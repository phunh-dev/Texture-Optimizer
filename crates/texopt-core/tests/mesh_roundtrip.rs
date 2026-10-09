//! Remap → export → re-import round trips for every writable format.
#![cfg(feature = "assimp")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use texopt_core::mesh::uv_remap::{
    AtlasRect, InsetPolicy, UvNormalize, UvOrigin, UvRemap, atlas_transform,
};
use texopt_core::mesh::{
    self, ExportFormat, ExportOptions, MaterialRemap, Model, ModelRemaps, TextureChannel, fixtures,
};

const ALL_FORMATS: [ExportFormat; 4] = [
    ExportFormat::Obj,
    ExportFormat::Collada,
    ExportFormat::Fbx,
    ExportFormat::FbxAscii,
];

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("mesh_roundtrip")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn source(kind: &str, dir: &Path) -> PathBuf {
    match kind {
        "obj" => fixtures::two_quads_obj(dir),
        "dae" => fixtures::two_quads_dae(dir),
        "fbx" => fixtures::two_quads_fbx(dir).unwrap(),
        _ => unreachable!(),
    }
}

fn mat_index(model: &Model, name: &str) -> usize {
    model
        .materials
        .iter()
        .position(|m| m.name == name)
        .unwrap_or_else(|| panic!("no {name} in {:#?}", model.materials))
}

/// MatA → top-left quarter, MatB → bottom-right quarter of a 256² atlas.
fn remaps_for(model: &Model) -> ModelRemaps {
    let mut r = BTreeMap::new();
    for (name, rect) in [
        (
            "MatA",
            AtlasRect {
                x: 0,
                y: 0,
                width: 128,
                height: 128,
            },
        ),
        (
            "MatB",
            AtlasRect {
                x: 128,
                y: 128,
                width: 128,
                height: 128,
            },
        ),
    ] {
        let t = atlas_transform(
            rect,
            [256, 256],
            InsetPolicy::HalfTexel,
            UvOrigin::BottomLeft,
        )
        .unwrap();
        r.insert(
            mat_index(model, name),
            MaterialRemap {
                uv_channel: 0,
                remap: UvRemap::new(UvNormalize::None, t),
            },
        );
    }
    r
}

fn atlas_textures(model: &Model) -> BTreeMap<TextureChannel, String> {
    let mut channels: BTreeMap<TextureChannel, String> = BTreeMap::new();
    for m in &model.materials {
        for c in m.textures.keys() {
            channels.insert(c.clone(), format!("atlas_{c}.png"));
        }
    }
    channels
}

fn used_meshes(model: &Model) -> Vec<&mesh::Mesh> {
    model.meshes.iter().filter(|m| m.vertex_count > 0).collect()
}

fn assert_uvs_close(got: &[[f32; 2]], want: &[[f32; 2]], ctx: &str) {
    assert_eq!(got.len(), want.len(), "{ctx}");
    for (g, w) in got.iter().zip(want) {
        assert!(
            (g[0] - w[0]).abs() < 1e-5 && (g[1] - w[1]).abs() < 1e-5,
            "{ctx}: got {got:?}, want {want:?}"
        );
    }
}

fn round_trip(kind: &str, format: ExportFormat, merge: bool) {
    let ctx = format!("{kind} -> {format:?} merge={merge}");
    let dir = scratch(&format!("{kind}_{}_{merge}", format.assimp_id()));
    let src = source(kind, &dir.join("src"));
    let loaded = mesh::import(&src).unwrap();
    let remaps = remaps_for(&loaded.model);
    let atlas = atlas_textures(&loaded.model);
    let options = ExportOptions {
        format,
        merge_materials: merge,
        merged_material_name: Some("Atlas".into()),
        verify_geometry: true,
        atlas_textures: atlas.clone(),
    };
    let out = dir
        .join("out")
        .join(format!("remapped.{}", format.extension()));
    let report =
        mesh::export(&loaded, &remaps, &options, &out).unwrap_or_else(|e| panic!("{ctx}: {e:?}"));
    assert!(report.warnings.is_empty(), "{ctx}: {:?}", report.warnings);

    let back = mesh::import(&out).unwrap_or_else(|e| panic!("{ctx}: re-import {e:?}"));
    let before = used_meshes(&loaded.model);
    let after = used_meshes(&back.model);
    assert_eq!(before.len(), after.len(), "{ctx}");
    for (b, a) in before.iter().zip(&after) {
        assert_eq!(
            b.vertex_count, a.vertex_count,
            "{ctx}: vertex count of {}",
            b.name
        );
        let r = &remaps[&b.material_index];
        let want: Vec<[f32; 2]> = b.uv_channels[0]
            .iter()
            .map(|&uv| r.remap.apply(uv))
            .collect();
        assert_uvs_close(&a.uv_channels[0], &want, &format!("{ctx} mesh {}", b.name));

        let mat = &back.model.materials[a.material_index];
        if merge {
            assert_eq!(mat.name, "Atlas", "{ctx}");
        } else {
            assert_eq!(
                mat.name, loaded.model.materials[b.material_index].name,
                "{ctx}"
            );
        }
        let base = &mat.textures[&TextureChannel::BaseColor];
        assert_eq!(base.raw_path, "atlas_baseColor.png", "{ctx}");
        assert_eq!(
            base.path,
            out.parent().unwrap().join("atlas_baseColor.png"),
            "{ctx}"
        );
        assert!(
            !mat.textures
                .values()
                .any(|t| t.raw_path.starts_with("tex/")),
            "{ctx}: original texture left: {mat:?}"
        );
    }
    if merge {
        let idx: Vec<usize> = after.iter().map(|m| m.material_index).collect();
        assert!(
            idx.windows(2).all(|w| w[0] == w[1]),
            "{ctx}: meshes share one material"
        );
    }
}

#[test]
fn obj_source_all_formats() {
    for format in ALL_FORMATS {
        round_trip("obj", format, false);
        round_trip("obj", format, true);
    }
}

#[test]
fn dae_source_all_formats() {
    for format in ALL_FORMATS {
        round_trip("dae", format, false);
        round_trip("dae", format, true);
    }
}

#[test]
fn fbx_source_all_formats() {
    for format in ALL_FORMATS {
        round_trip("fbx", format, false);
        round_trip("fbx", format, true);
    }
}

/// Which non-albedo channels each writer preserves (OBJ source has a normal
/// map). Documented in docs/spikes/3d-assimp.md.
#[test]
fn normal_map_channel_survival() {
    let mut matrix = Vec::new();
    for format in ALL_FORMATS {
        let dir = scratch(&format!("normal_{}", format.assimp_id()));
        let loaded = mesh::import(&fixtures::two_quads_obj(&dir.join("src"))).unwrap();
        let options = ExportOptions {
            format,
            merge_materials: true,
            merged_material_name: None,
            verify_geometry: true,
            atlas_textures: atlas_textures(&loaded.model),
        };
        let out = dir.join(format!("m.{}", format.extension()));
        mesh::export(&loaded, &remaps_for(&loaded.model), &options, &out).unwrap();
        let back = mesh::import(&out).unwrap().model;
        let mat = &back.materials[used_meshes(&back)[0].material_index];
        assert_eq!(mat.name, "AtlasMaterial");
        let get = |c: TextureChannel| mat.textures.get(&c).map(|t| t.raw_path.clone());
        matrix.push((
            format,
            get(TextureChannel::Normal),
            get(TextureChannel::Height),
        ));
    }
    eprintln!("normal channel survival (format, normal, height): {matrix:?}");
    for (format, normal, height) in &matrix {
        if *format == ExportFormat::Obj {
            // Assimp's OBJ writer emits normal maps as `bump`/`map_bump`, which
            // every OBJ reader (Assimp included) classifies as a height/bump map.
            assert_eq!(normal.as_deref(), None);
            assert_eq!(height.as_deref(), Some("atlas_normal.png"));
        } else {
            assert_eq!(normal.as_deref(), Some("atlas_normal.png"), "{format:?}");
        }
    }
}

#[test]
fn merge_keeps_unremapped_materials() {
    let dir = scratch("partial_merge");
    let loaded = mesh::import(&fixtures::two_quads_obj(&dir.join("src"))).unwrap();
    let mut remaps = remaps_for(&loaded.model);
    let mat_b = mat_index(&loaded.model, "MatB");
    remaps.remove(&mat_b);
    let options = ExportOptions {
        format: ExportFormat::Fbx,
        merge_materials: true,
        merged_material_name: None,
        verify_geometry: true,
        atlas_textures: atlas_textures(&loaded.model),
    };
    let out = dir.join("m.fbx");
    mesh::export(&loaded, &remaps, &options, &out).unwrap();
    let back = mesh::import(&out).unwrap().model;
    let meshes = used_meshes(&back);
    let a = &back.materials[meshes[0].material_index];
    let b = &back.materials[meshes[1].material_index];
    assert_eq!(a.name, "AtlasMaterial");
    assert_eq!(b.name, "MatB");
    assert_eq!(
        b.textures[&TextureChannel::BaseColor].raw_path,
        "tex/blue.png"
    );
    assert_uvs_close(
        &meshes[1].uv_channels[0],
        &loaded.model.meshes[1].uv_channels[0],
        "unremapped mesh keeps its UVs",
    );
}

#[test]
fn export_validation_errors() {
    let dir = scratch("validation");
    let loaded = mesh::import(&fixtures::two_quads_obj(&dir.join("src"))).unwrap();
    let remaps = remaps_for(&loaded.model);
    // Normal map sampled through the remapped channel but no normal atlas.
    let mut atlas = atlas_textures(&loaded.model);
    atlas.remove(&TextureChannel::Normal);
    let options = ExportOptions {
        format: ExportFormat::Obj,
        merge_materials: false,
        merged_material_name: None,
        verify_geometry: true,
        atlas_textures: atlas,
    };
    let err = mesh::export(&loaded, &remaps, &options, &dir.join("x.obj")).unwrap_err();
    assert_eq!(err.code, mesh::codes::MESH_ATLAS_CHANNEL_MISSING);
    assert_eq!(err.params["channel"], "normal");
    // Remap on a UV channel the meshes do not have.
    let mut bad = remaps.clone();
    for r in bad.values_mut() {
        r.uv_channel = 3;
    }
    let options = ExportOptions {
        atlas_textures: BTreeMap::new(),
        ..options
    };
    let err = mesh::export(&loaded, &bad, &options, &dir.join("y.obj")).unwrap_err();
    assert_eq!(err.code, mesh::codes::MESH_NO_UVS);
    assert_eq!(err.params["uvChannel"], 3);
    assert!(
        err.params["material"]
            .as_str()
            .is_some_and(|m| m.starts_with("Mat"))
    );
}
