//! Import of generated OBJ / DAE / FBX fixtures through the Assimp backend.
#![cfg(feature = "assimp")]

use std::path::{Path, PathBuf};

use texopt_core::mesh::{self, Model, TextureChannel, codes, fixtures};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("mesh_import")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn material<'a>(model: &'a Model, name: &str) -> (usize, &'a mesh::Material) {
    model
        .materials
        .iter()
        .enumerate()
        .find(|(_, m)| m.name == name)
        .unwrap_or_else(|| panic!("no material {name}: {model:#?}"))
}

/// Shared expectations for the two-quads fixture in any format.
fn assert_two_quads(model: &Model, dir: &Path) {
    let used: Vec<&mesh::Mesh> = model.meshes.iter().filter(|m| m.vertex_count > 0).collect();
    assert_eq!(used.len(), 2, "{model:#?}");
    for (mat_name, tex, uvs) in [
        ("MatA", "red.png", fixtures::QUAD_A_UVS),
        ("MatB", "blue.png", fixtures::QUAD_B_UVS),
    ] {
        let (mi, mat) = material(model, mat_name);
        let base = &mat.textures[&TextureChannel::BaseColor];
        assert!(base.exists, "{base:?}");
        assert_eq!(
            base.path,
            dir.join("tex").join(tex),
            "relative path resolved against the model dir"
        );
        assert_eq!(base.uv_channel, 0);
        assert!(base.embedded_index.is_none());
        let meshes: Vec<_> = model.meshes_with_material(mi).collect();
        assert_eq!(meshes.len(), 1);
        let m = meshes[0];
        assert_eq!(
            m.vertex_count, 4,
            "JoinIdenticalVertices keeps one vertex per corner"
        );
        assert_eq!(m.uv_channels.len(), 1);
        let mut got = m.uv_channels[0].clone();
        let mut want = uvs.to_vec();
        got.sort_by(|a, b| a.partial_cmp(b).unwrap());
        want.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for (g, w) in got.iter().zip(&want) {
            assert!(
                (g[0] - w[0]).abs() < 1e-6 && (g[1] - w[1]).abs() < 1e-6,
                "{got:?} vs {want:?}"
            );
        }
        assert_eq!(m.faces.len(), 1, "quad is not triangulated");
        assert_eq!(m.faces[0].len(), 4);
    }
    assert!(model.warnings.is_empty(), "{:?}", model.warnings);
}

#[test]
fn obj_meshes_materials_and_channels() {
    let dir = scratch("obj");
    let path = fixtures::two_quads_obj(&dir);
    let loaded = mesh::import(&path).unwrap();
    assert_eq!(loaded.model.format, mesh::ModelFormat::Obj);
    assert_two_quads(&loaded.model, &dir);
    let (_, a) = material(&loaded.model, "MatA");
    let normal = &a.textures[&TextureChannel::Normal];
    assert_eq!(normal.raw_path, "tex/red_n.png");
    assert!(normal.exists);
    // Serializes for the frontend with camelCase keys.
    let json = serde_json::to_value(&loaded.model).unwrap();
    assert!(
        json["materials"][1]["textures"]["baseColor"]["rawPath"].is_string(),
        "{json}"
    );
    assert!(json["meshes"][0].get("faces").is_none());
}

#[test]
fn dae_meshes_materials_and_channels() {
    let dir = scratch("dae");
    let path = fixtures::two_quads_dae(&dir);
    let loaded = mesh::import(&path).unwrap();
    assert_eq!(loaded.model.format, mesh::ModelFormat::Dae);
    assert_two_quads(&loaded.model, &dir);
}

#[test]
fn fbx_meshes_materials_and_channels() {
    let dir = scratch("fbx");
    let path = fixtures::two_quads_fbx(&dir).unwrap();
    let loaded = mesh::import(&path).unwrap();
    assert_eq!(loaded.model.format, mesh::ModelFormat::Fbx);
    assert_two_quads(&loaded.model, &dir);
    let (_, a) = material(&loaded.model, "MatA");
    assert!(a.textures.contains_key(&TextureChannel::Normal), "{a:?}");
}

#[test]
fn missing_texture_is_a_warning_and_sibling_fallback_works() {
    let dir = scratch("missing");
    let path = fixtures::two_quads_obj(&dir);
    std::fs::remove_file(dir.join("tex/blue.png")).unwrap();
    // Texture referenced in a sub folder but present next to the model.
    std::fs::rename(dir.join("tex/red.png"), dir.join("red.png")).unwrap();
    let model = mesh::import(&path).unwrap().model;
    let (_, a) = material(&model, "MatA");
    assert!(a.textures[&TextureChannel::BaseColor].exists);
    assert_eq!(
        a.textures[&TextureChannel::BaseColor].path,
        dir.join("red.png")
    );
    let (_, b) = material(&model, "MatB");
    assert!(!b.textures[&TextureChannel::BaseColor].exists);
    assert_eq!(model.warnings.len(), 1, "{:?}", model.warnings);
    assert_eq!(model.warnings[0].code, codes::MESH_TEXTURE_NOT_FOUND);
    assert_eq!(model.warnings[0].params["material"], "MatB");
    assert_eq!(model.warnings[0].params["channel"], "baseColor");
}

#[test]
fn unsupported_and_broken_files() {
    let dir = scratch("errors");
    let err = mesh::import(&dir.join("model.blend")).unwrap_err();
    assert_eq!(err.code, codes::MESH_FORMAT_UNSUPPORTED);
    let broken = dir.join("broken.fbx");
    std::fs::write(&broken, b"definitely not an fbx file").unwrap();
    let err = mesh::import(&broken).unwrap_err();
    assert_eq!(err.code, codes::MESH_IMPORT_FAILED);
    assert!(err.params["detail"].as_str().is_some_and(|d| !d.is_empty()));
    let err = mesh::import(&dir.join("does_not_exist.obj")).unwrap_err();
    assert_eq!(err.code, codes::MESH_IMPORT_FAILED);
}

#[test]
fn backend_reports_version_and_writers() {
    assert!(mesh::backend_version().unwrap().starts_with("6."));
    let formats = mesh::available_export_formats();
    for id in ["obj", "collada", "fbx", "fbxa"] {
        assert!(
            formats.iter().any(|f| f == id),
            "{id} missing in {formats:?}"
        );
    }
}
