//! End-to-end tests of `mesh::pack` (3D Model Texture Packer) on code-generated
//! fixtures: three models, each with its own solid-colour textures (one with
//! normal maps). Outputs are re-imported with Assimp and the atlases are
//! sampled at the remapped UVs, which must give back the original colours.
#![cfg(feature = "assimp")]

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use image::Rgba;
use texopt_core::ImageBuf;
use texopt_core::fixtures::solid;
use texopt_core::mesh::pack::{
    self, AssimpExporter, FormatChoice, MaterialStatus, ModelExporter, ModelOutcome, OutputMode,
    PackOptions, PackReport, codes,
};
use texopt_core::mesh::remap_json::{self, RemapFile};
use texopt_core::mesh::uv_remap::{InsetPolicy, OutOfRangePolicy, UvOrigin, uv_to_pixel};
use texopt_core::mesh::{
    self, ExportOptions, ExportReport, LoadedModel, Mesh, ModelRemaps, TextureChannel, fixtures,
};
use texopt_core::{OpError, OpResult};

const GREEN: Rgba<u8> = Rgba([40, 200, 60, 255]);
const YELLOW: Rgba<u8> = Rgba([230, 210, 20, 255]);
const PURPLE: Rgba<u8> = Rgba([150, 40, 170, 255]);
const FLAT_NORMAL: Rgba<u8> = Rgba([128, 128, 255, 255]);

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("mesh_pack")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// One quad with material `<name>Mat` sampling `tex` (solid `color`, `size`).
fn quad_obj(dir: &Path, name: &str, color: Rgba<u8>, size: (u32, u32)) -> PathBuf {
    std::fs::create_dir_all(dir.join("tex")).unwrap();
    solid(size.0, size.1, color)
        .save(dir.join("tex").join(format!("{name}.png")))
        .unwrap();
    std::fs::write(
        dir.join(format!("{name}.mtl")),
        format!("newmtl {name}Mat\nKd 1 1 1\nmap_Kd tex/{name}.png\n"),
    )
    .unwrap();
    let path = dir.join(format!("{name}.obj"));
    std::fs::write(
        &path,
        format!(
            "mtllib {name}.mtl\no {name}\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\n\
             vt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nvn 0 0 1\nusemtl {name}Mat\n\
             f 1/1/1 2/2/1 3/3/1 4/4/1\n"
        ),
    )
    .unwrap();
    path
}

/// The three fixture models and the base colour / normal of every material.
struct Scene {
    models: Vec<PathBuf>,
    base: BTreeMap<String, Rgba<u8>>,
    normal: BTreeMap<String, Rgba<u8>>,
}

fn scene(dir: &Path) -> Scene {
    // Model 1: OBJ, MatA red + normal A, MatB blue + normal B (8×8).
    let obj = fixtures::two_quads_obj(&dir.join("m1"));
    // Model 2: OBJ, one green 16×8 texture, no normal map.
    let green = quad_obj(&dir.join("m2"), "Green", GREEN, (16, 8));
    // Model 3: DAE, same layout as model 1 but its own colours, no normal.
    let dae = fixtures::two_quads_dae(&dir.join("m3"));
    solid(8, 8, YELLOW).save(dir.join("m3/tex/red.png")).unwrap();
    solid(8, 8, PURPLE).save(dir.join("m3/tex/blue.png")).unwrap();
    let base = [
        ("two_quads.obj/MatA", fixtures::RED),
        ("two_quads.obj/MatB", fixtures::BLUE),
        ("Green.obj/GreenMat", GREEN),
        ("two_quads.dae/MatA", YELLOW),
        ("two_quads.dae/MatB", PURPLE),
    ];
    let normal = [
        ("two_quads.obj/MatA", fixtures::NORMAL_A),
        ("two_quads.obj/MatB", fixtures::NORMAL_B),
        ("Green.obj/GreenMat", FLAT_NORMAL),
        ("two_quads.dae/MatA", FLAT_NORMAL),
        ("two_quads.dae/MatB", FLAT_NORMAL),
    ];
    let map = |v: &[(&str, Rgba<u8>)]| v.iter().map(|(k, c)| (k.to_string(), *c)).collect();
    Scene {
        models: vec![obj, green, dae],
        base: map(&base),
        normal: map(&normal),
    }
}

fn load(path: &Path) -> ImageBuf {
    texopt_core::io::load_image(path).unwrap()
}

fn sample(img: &ImageBuf, uv: [f32; 2]) -> Rgba<u8> {
    let p = uv_to_pixel(uv, [img.width(), img.height()], UvOrigin::BottomLeft);
    let x = (p[0].floor() as i64).clamp(0, img.width() as i64 - 1) as u32;
    let y = (p[1].floor() as i64).clamp(0, img.height() as i64 - 1) as u32;
    *img.get_pixel(x, y)
}

/// Vertex UVs plus a 5×5 grid inside every face.
fn sample_points(mesh: &Mesh, channel: usize) -> Vec<[f32; 2]> {
    let lerp =
        |a: [f32; 2], b: [f32; 2], k: f32| [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k];
    let uvs = &mesh.uv_channels[channel];
    let mut pts = uvs.clone();
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
                    _ => {}
                }
            }
        }
    }
    pts
}

fn atlas_of(report: &PackReport, page: usize, channel: &TextureChannel, out: &Path) -> ImageBuf {
    let tex = report.pages[page]
        .textures
        .iter()
        .find(|t| &t.channel == channel)
        .unwrap_or_else(|| panic!("no {channel} atlas on page {page}"));
    load(&out.join(&tex.path))
}

fn options(format: FormatChoice, merge: bool) -> PackOptions {
    let mut o = PackOptions {
        max_size: 512,
        ..Default::default()
    };
    o.output.format = format;
    o.output.merge_materials = merge;
    o
}

fn run(models: &[PathBuf], o: &PackOptions, out: &Path) -> PackReport {
    pack::run(models, o, out, "atlas", &AssimpExporter, &mut |_| {}).unwrap()
}

fn assert_pot(report: &PackReport, out: &Path) {
    for page in &report.pages {
        assert!(page.width.is_power_of_two() && page.height.is_power_of_two(), "{page:?}");
        for t in &page.textures {
            let img = load(&out.join(&t.path));
            assert_eq!(img.dimensions(), (page.width, page.height), "{}", t.path);
        }
    }
}

/// Re-import every rewritten model and sample the atlases at its UVs.
fn check_rewritten_sampling(scene: &Scene, report: &PackReport, out: &Path, merged: bool) {
    for mr in &report.models {
        assert_eq!(mr.outcome, ModelOutcome::Rewritten, "{}: {:?}", mr.name, mr.error);
        let output = PathBuf::from(mr.output.as_ref().unwrap());
        assert!(output.starts_with(out));
        let back = mesh::import(&output).unwrap().model;
        let page = mr.pages[0];
        let base = atlas_of(report, page, &TextureChannel::BaseColor, out);
        let expected: HashSet<Rgba<u8>> = scene
            .base
            .iter()
            .filter(|(k, _)| k.starts_with(&format!("{}/", mr.name)))
            .map(|(_, c)| *c)
            .collect();
        let mut found = HashSet::new();
        for m in back.meshes.iter().filter(|m| m.vertex_count > 0) {
            let mat = &back.materials[m.material_index];
            // The atlas is referenced instead of the original texture.
            let tex = &mat.textures[&TextureChannel::BaseColor];
            assert_eq!(tex.raw_path, "atlas_baseColor.png", "{}", mr.name);
            assert!(!mat.textures.values().any(|t| t.raw_path.contains("tex/")));
            if merged {
                assert_eq!(mat.name, "AtlasMaterial");
            }
            for face_pts in m.faces.iter().map(|f| {
                let sub = Mesh {
                    faces: vec![f.clone()],
                    uv_channels: m.uv_channels.clone(),
                    ..m.clone()
                };
                sample_points(&sub, 0)
            }) {
                let colors: HashSet<Rgba<u8>> = face_pts.iter().map(|&uv| sample(&base, uv)).collect();
                assert_eq!(colors.len(), 1, "{}: face samples several colours {colors:?}", mr.name);
                let c = *colors.iter().next().unwrap();
                if !merged {
                    let key = format!("{}/{}", mr.name, mat.name);
                    assert_eq!(Some(&c), scene.base.get(&key), "{key}");
                }
                assert!(expected.contains(&c), "{}: unexpected colour {c:?}", mr.name);
                found.insert(c);
            }
        }
        assert_eq!(found, expected, "{}: not every material came back", mr.name);
    }
}

#[test]
fn rewrite_obj_output_samples_original_colours() {
    let dir = scratch("rewrite_obj");
    let s = scene(&dir.join("src"));
    let out = dir.join("out");
    let report = run(&s.models, &options(FormatChoice::Obj, false), &out);
    assert_eq!(report.pages.len(), 1);
    assert_pot(&report, &out);
    check_rewritten_sampling(&s, &report, &out, false);
    // Merged materials too.
    let out = dir.join("out_merged");
    let report = run(&s.models, &options(FormatChoice::Obj, true), &out);
    check_rewritten_sampling(&s, &report, &out, true);
}

#[test]
fn rewrite_dae_output_samples_original_colours() {
    let dir = scratch("rewrite_dae");
    let s = scene(&dir.join("src"));
    for merge in [false, true] {
        let out = dir.join(format!("out_{merge}"));
        let report = run(&s.models, &options(FormatChoice::Collada, merge), &out);
        assert_pot(&report, &out);
        for m in &report.models {
            assert!(m.output.as_ref().unwrap().ends_with(".dae"));
        }
        check_rewritten_sampling(&s, &report, &out, merge);
    }
}

#[test]
fn rewrite_fbx_output_samples_original_colours() {
    let dir = scratch("rewrite_fbx");
    let fbx = fixtures::two_quads_fbx(&dir.join("src")).unwrap();
    let green = quad_obj(&dir.join("src2"), "Green", GREEN, (16, 8));
    let s = Scene {
        models: vec![fbx, green],
        base: [
            ("two_quads.fbx/MatA".to_string(), fixtures::RED),
            ("two_quads.fbx/MatB".to_string(), fixtures::BLUE),
            ("Green.obj/GreenMat".to_string(), GREEN),
        ]
        .into(),
        normal: BTreeMap::new(),
    };
    let out = dir.join("out");
    let report = run(&s.models, &options(FormatChoice::Fbx, false), &out);
    assert_pot(&report, &out);
    check_rewritten_sampling(&s, &report, &out, false);
}

#[test]
fn every_channel_shares_the_rects_and_missing_channels_are_filled() {
    let dir = scratch("channels");
    let s = scene(&dir.join("src"));
    let out = dir.join("out");
    let mut o = options(FormatChoice::SameAsSource, false);
    o.missing_defaults
        .insert(TextureChannel::Normal, "#102030".into());
    let report = run(&s.models, &o, &out);
    assert_eq!(
        report.channels,
        vec![TextureChannel::BaseColor, TextureChannel::Normal]
    );
    assert_pot(&report, &out);
    let base = atlas_of(&report, 0, &TextureChannel::BaseColor, &out);
    let normal = atlas_of(&report, 0, &TextureChannel::Normal, &out);
    let custom_flat = Rgba([16, 32, 48, 255]);
    let mut rects = Vec::new();
    for mr in &report.models {
        for mat in &mr.materials {
            assert!(mat.status.packed());
            let r = mat.rect.unwrap();
            rects.push(r);
            let key = format!("{}/{}", mr.name, mat.name);
            // Every pixel of the block (and of its extrusion) in every channel.
            for y in r.y.saturating_sub(4)..r.y + r.height + 4 {
                for x in r.x.saturating_sub(4)..r.x + r.width + 4 {
                    assert_eq!(*base.get_pixel(x, y), s.base[&key], "{key} base @{x},{y}");
                    // Only model 1 has normal maps; the others get the fill colour.
                    let want = if mr.name == "two_quads.obj" {
                        s.normal[&key]
                    } else {
                        custom_flat
                    };
                    assert_eq!(*normal.get_pixel(x, y), want, "{key} normal @{x},{y}");
                }
            }
            // Block size follows the base colour texture.
            let expect = if mr.name == "Green.obj" { (16, 8) } else { (8, 8) };
            assert_eq!((r.width, r.height), expect);
        }
    }
    // No overlaps (padding 4 + 2×extrude 4 between blocks).
    for (i, a) in rects.iter().enumerate() {
        for b in &rects[i + 1..] {
            let apart = a.x + a.width + 12 <= b.x
                || b.x + b.width + 12 <= a.x
                || a.y + a.height + 12 <= b.y
                || b.y + b.height + 12 <= a.y;
            assert!(apart, "{a:?} {b:?}");
        }
    }
    // Report JSON written and parseable.
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("atlas.report.json")).unwrap())
            .unwrap();
    assert_eq!(json["models"].as_array().unwrap().len(), 3);
    assert_eq!(json["pages"][0]["textures"][1]["path"], "atlas_normal.png");
    assert!(json["models"][0]["materials"][0]["rect"]["width"].is_number());
}

#[test]
fn channel_selection_forced_resampled_and_missing_textures() {
    let dir = scratch("channel_edge_cases");
    let obj = fixtures::two_quads_obj(&dir.join("m1"));
    // MatA's normal map has another size → resampled; MatB's base colour is missing.
    solid(4, 4, fixtures::NORMAL_A)
        .save(dir.join("m1/tex/red_n.png"))
        .unwrap();
    std::fs::remove_file(dir.join("m1/tex/blue.png")).unwrap();
    let mut o = options(FormatChoice::Obj, false);
    o.channels = vec![TextureChannel::BaseColor];
    let out = dir.join("out");
    let report = run(&[obj], &o, &out);
    let codes_of = |w: &[OpError]| w.iter().map(|e| e.code.clone()).collect::<Vec<_>>();
    assert!(codes_of(&report.warnings).contains(&codes::MESH_CHANNEL_FORCED.to_string()));
    let mw = codes_of(&report.models[0].warnings);
    assert!(mw.contains(&mesh::codes::MESH_TEXTURE_NOT_FOUND.to_string()), "{mw:?}");
    assert!(mw.contains(&codes::MESH_TEXTURE_RESIZED.to_string()), "{mw:?}");
    let base = atlas_of(&report, 0, &TextureChannel::BaseColor, &out);
    let normal = atlas_of(&report, 0, &TextureChannel::Normal, &out);
    let rect_of = |name: &str| {
        report.models[0]
            .materials
            .iter()
            .find(|m| m.name == name)
            .unwrap()
            .clone()
    };
    let a = rect_of("MatA");
    let b = rect_of("MatB");
    assert_eq!(a.layout_channel, Some(TextureChannel::BaseColor));
    // MatB: layout from its normal map; base colour filled with the default (white).
    assert_eq!(b.layout_channel, Some(TextureChannel::Normal));
    let (ra, rb) = (a.rect.unwrap(), b.rect.unwrap());
    assert_eq!((ra.width, ra.height), (8, 8));
    assert_eq!(*normal.get_pixel(ra.x + 3, ra.y + 3), fixtures::NORMAL_A);
    assert_eq!(*base.get_pixel(rb.x + 3, rb.y + 3), Rgba([255, 255, 255, 255]));
    assert_eq!(*normal.get_pixel(rb.x + 3, rb.y + 3), fixtures::NORMAL_B);
}

#[test]
fn uv_remap_data_mode_writes_sidecars_and_script() {
    let dir = scratch("remap_data");
    let s = scene(&dir.join("src"));
    let out = dir.join("out");
    let mut o = options(FormatChoice::SameAsSource, true);
    o.output.mode = OutputMode::UvRemapData;
    let report = run(&s.models, &o, &out);
    let script = out.join(remap_json::UNITY_POSTPROCESSOR_FILE_NAME);
    assert_eq!(
        std::fs::read_to_string(&script).unwrap(),
        remap_json::UNITY_POSTPROCESSOR_CS
    );
    assert_eq!(
        report
            .files
            .iter()
            .filter(|f| f.ends_with(remap_json::UNITY_POSTPROCESSOR_FILE_NAME))
            .count(),
        1
    );
    let base = atlas_of(&report, 0, &TextureChannel::BaseColor, &out);
    for (mr, src) in report.models.iter().zip(&s.models) {
        assert_eq!(mr.outcome, ModelOutcome::RemapData);
        // The untouched model is copied next to its sidecar.
        let copy = PathBuf::from(mr.output.as_ref().unwrap());
        assert_eq!(std::fs::read(&copy).unwrap(), std::fs::read(src).unwrap());
        let sidecar = PathBuf::from(mr.sidecar.as_ref().unwrap());
        assert_eq!(sidecar, remap_json::sidecar_path(&copy));
        let file = RemapFile::from_json(&std::fs::read_to_string(&sidecar).unwrap()).unwrap();
        assert_eq!(file.atlas_pages[0].textures[0].path, "atlas_baseColor.png");
        let entry = &file.models[0];
        assert_eq!(entry.model, copy.file_name().unwrap().to_string_lossy());
        assert_eq!(entry.merged_material_name.as_deref(), Some("AtlasMaterial"));
        // Applying the sidecar to the source UVs (what the Unity script does)
        // samples the original colours.
        let model = mesh::import(src).unwrap().model;
        for m in model.meshes.iter().filter(|m| m.vertex_count > 0) {
            let mat = &model.materials[m.material_index];
            let e = entry
                .materials
                .iter()
                .find(|e| e.material_index == m.material_index)
                .unwrap();
            assert_eq!(e.material_name, mat.name);
            let remap = e.to_remap().unwrap();
            let in_report = mr
                .materials
                .iter()
                .find(|x| x.material_index == m.material_index)
                .unwrap();
            assert_eq!(Some(remap.remap), in_report.remap);
            let key = format!("{}/{}", mr.name, mat.name);
            for uv in sample_points(m, 0) {
                assert_eq!(sample(&base, remap.remap.apply(uv)), s.base[&key], "{key}");
            }
        }
    }
}

/// Exporter that reports a geometry change for DAE outputs (stand-in for
/// Assimp's FBX geometric-pivot bug) and delegates everything else.
struct GeometryBreaker;

impl ModelExporter for GeometryBreaker {
    fn export(
        &self,
        loaded: &LoadedModel,
        remaps: &ModelRemaps,
        options: &ExportOptions,
        out_path: &Path,
    ) -> OpResult<ExportReport> {
        if out_path.extension().is_some_and(|e| e == "dae") {
            return Err(OpError::new(mesh::codes::MESH_EXPORT_GEOMETRY_CHANGED)
                .with("path", out_path.display().to_string())
                .with("format", "collada")
                .with("relError", 0.2));
        }
        AssimpExporter.export(loaded, remaps, options, out_path)
    }
}

#[test]
fn geometry_change_falls_back_to_uv_remap_data() {
    let dir = scratch("fallback");
    let s = scene(&dir.join("src"));
    let out = dir.join("out");
    let o = options(FormatChoice::SameAsSource, false);
    let report = pack::run(&s.models, &o, &out, "atlas", &GeometryBreaker, &mut |_| {}).unwrap();
    let dae = &report.models[2];
    assert_eq!(dae.outcome, ModelOutcome::Fallback);
    let codes_of: Vec<_> = dae.warnings.iter().map(|w| w.code.as_str()).collect();
    assert!(codes_of.contains(&mesh::codes::MESH_EXPORT_GEOMETRY_CHANGED));
    assert!(codes_of.contains(&codes::MESH_FALLBACK_REMAP_DATA));
    // The original DAE is copied under the name the failed rewrite released
    // (`two_quads.*` companions belong to the rewritten two_quads.obj).
    assert!(dae.output.as_ref().unwrap().ends_with("two_quads_2.dae"));
    assert!(!out.join("two_quads_3.dae").exists());
    let sidecar = PathBuf::from(dae.sidecar.as_ref().unwrap());
    RemapFile::from_json(&std::fs::read_to_string(&sidecar).unwrap()).unwrap();
    assert!(out.join(remap_json::UNITY_POSTPROCESSOR_FILE_NAME).is_file());
    assert_eq!(report.count(ModelOutcome::Rewritten), 2);
    assert_eq!(report.count(ModelOutcome::Fallback), 1);

    // Without auto-fallback the model fails with the geometry error.
    let mut o = o;
    o.output.auto_fallback = false;
    let out = dir.join("out_nofallback");
    let report = pack::run(&s.models, &o, &out, "atlas", &GeometryBreaker, &mut |_| {}).unwrap();
    assert_eq!(report.models[2].outcome, ModelOutcome::Failed);
    assert_eq!(
        report.models[2].error.as_ref().unwrap().code,
        mesh::codes::MESH_EXPORT_GEOMETRY_CHANGED
    );
    assert!(!out.join(remap_json::UNITY_POSTPROCESSOR_FILE_NAME).exists());
}

#[test]
fn fbx_output_always_verifies_geometry_unless_advanced() {
    struct Spy(std::cell::RefCell<Vec<bool>>);
    impl ModelExporter for Spy {
        fn export(
            &self,
            loaded: &LoadedModel,
            remaps: &ModelRemaps,
            options: &ExportOptions,
            out_path: &Path,
        ) -> OpResult<ExportReport> {
            self.0.borrow_mut().push(options.verify_geometry);
            AssimpExporter.export(loaded, remaps, options, out_path)
        }
    }
    let dir = scratch("verify");
    let green = quad_obj(&dir.join("src"), "Green", GREEN, (8, 8));
    for (format, allow, expect) in [
        (FormatChoice::Fbx, false, true),
        (FormatChoice::FbxAscii, false, true),
        (FormatChoice::Fbx, true, false),
        (FormatChoice::Obj, false, false),
    ] {
        let mut o = options(format, false);
        o.output.verify_geometry = false;
        o.output.allow_unverified_fbx = allow;
        let spy = Spy(Default::default());
        let out = dir.join(format!("out_{format:?}_{allow}"));
        pack::run(&[green.clone()], &o, &out, "atlas", &spy, &mut |_| {}).unwrap();
        assert_eq!(*spy.0.borrow(), vec![expect], "{format:?} allow={allow}");
    }
}

#[test]
fn out_of_range_policies() {
    let dir = scratch("out_of_range");
    // MatA: u in [0,2] (straddles); MatB: u in [1.25,1.75] (tile 1).
    let tiled = fixtures::tiled_quads_obj(&dir.join("tiled"));
    let green = quad_obj(&dir.join("green"), "Green", GREEN, (8, 8));
    let models = vec![tiled.clone(), green.clone()];
    let status = |r: &PackReport, name: &str| {
        r.models[0]
            .materials
            .iter()
            .find(|m| m.name == name)
            .unwrap()
            .clone()
    };
    let codes_of = |r: &PackReport| {
        r.models[0]
            .warnings
            .iter()
            .map(|w| w.code.clone())
            .collect::<Vec<_>>()
    };

    // skipMaterial: both left out; the tiled model is skipped entirely.
    let mut o = options(FormatChoice::Obj, false);
    let r = run(&models, &o, &dir.join("skip"));
    assert_eq!(status(&r, "MatA").status, MaterialStatus::Skipped);
    assert_eq!(status(&r, "MatB").status, MaterialStatus::Skipped);
    assert_eq!(r.models[0].outcome, ModelOutcome::Skipped);
    assert_eq!(r.models[1].outcome, ModelOutcome::Rewritten);
    assert_eq!(
        codes_of(&r)
            .iter()
            .filter(|c| *c == mesh::codes::MESH_UV_OUT_OF_RANGE)
            .count(),
        2
    );
    // Only out-of-range models → nothing to pack.
    let err = pack::run(&[tiled.clone()], &o, &dir.join("none"), "atlas", &AssimpExporter, &mut |_| {})
        .unwrap_err();
    assert_eq!(err.code, codes::MESH_NOTHING_TO_PACK);

    // clamp
    o.out_of_range = OutOfRangePolicy::Clamp;
    let r = run(&models, &o, &dir.join("clamp"));
    assert_eq!(status(&r, "MatA").status, MaterialStatus::Clamped);
    assert_eq!(r.models[0].outcome, ModelOutcome::Rewritten);

    // wrapIntoTile: MatB moves back into the unit tile; MatA straddles → warning.
    o.out_of_range = OutOfRangePolicy::WrapIntoTile;
    let out = dir.join("wrap");
    let r = run(&models, &o, &out);
    assert_eq!(status(&r, "MatB").status, MaterialStatus::Wrapped);
    assert!(codes_of(&r).contains(&mesh::codes::MESH_UV_WRAP_STRADDLE.to_string()));
    let back = mesh::import(Path::new(r.models[0].output.as_ref().unwrap())).unwrap().model;
    let base = atlas_of(&r, 0, &TextureChannel::BaseColor, &out);
    let b = back
        .meshes
        .iter()
        .find(|m| back.materials[m.material_index].name == "MatB")
        .unwrap();
    for uv in sample_points(b, 0) {
        assert_eq!(sample(&base, uv), fixtures::BLUE);
    }

    // bakeRepeat: MatA needs 2×1 tiles; the block holds the texture twice.
    o.out_of_range = OutOfRangePolicy::BakeRepeat { max_tiles: 2 };
    // Two-colour texture (left red, right blue) to see the repetition.
    let mut two = solid(8, 8, fixtures::RED);
    for y in 0..8 {
        for x in 4..8 {
            two.put_pixel(x, y, fixtures::BLUE);
        }
    }
    two.save(dir.join("tiled/tex/red.png")).unwrap();
    let out = dir.join("bake");
    let r = run(&models, &o, &out);
    let a = status(&r, "MatA");
    assert_eq!(a.status, MaterialStatus::Repeated);
    assert_eq!(a.tiles, [2, 1]);
    let ra = a.rect.unwrap();
    assert_eq!((ra.width, ra.height), (16, 8));
    let base = atlas_of(&r, 0, &TextureChannel::BaseColor, &out);
    let remap = a.remap.unwrap();
    for (u, want) in [(0.2, fixtures::RED), (0.8, fixtures::BLUE), (1.2, fixtures::RED), (1.8, fixtures::BLUE)] {
        assert_eq!(sample(&base, remap.apply([u, 0.5])), want, "u={u}");
    }
    // Same through the exported + re-imported model.
    let back = mesh::import(Path::new(r.models[0].output.as_ref().unwrap())).unwrap().model;
    let m = back
        .meshes
        .iter()
        .find(|m| back.materials[m.material_index].name == "MatA")
        .unwrap();
    let uvs = &m.uv_channels[0];
    let colors: HashSet<_> = uvs.iter().map(|&uv| sample(&base, uv)).collect();
    assert!(colors.is_subset(&[fixtures::RED, fixtures::BLUE].into()));

    // bakeRepeat over the limit: that material is left out with a warning.
    o.out_of_range = OutOfRangePolicy::BakeRepeat { max_tiles: 1 };
    let r = run(&models, &o, &dir.join("bake_limit"));
    assert_eq!(status(&r, "MatA").status, MaterialStatus::TooManyTiles);
    assert_eq!(status(&r, "MatB").status, MaterialStatus::Repeated);
    assert!(codes_of(&r).contains(&mesh::codes::MESH_UV_TOO_MANY_TILES.to_string()));
}

#[test]
fn multi_page_keeps_models_whole_and_falls_back_when_split() {
    let dir = scratch("multi_page");
    let obj = fixtures::two_quads_obj(&dir.join("m1"));
    for f in ["red", "blue", "red_n", "blue_n"] {
        solid(48, 48, fixtures::RED)
            .save(dir.join(format!("m1/tex/{f}.png")))
            .unwrap();
    }
    let green = quad_obj(&dir.join("m2"), "Green", GREEN, (48, 48));
    let mut o = options(FormatChoice::Obj, false);
    o.max_size = 64;
    let out = dir.join("out");
    let report = run(&[obj.clone(), green.clone()], &o, &out);
    assert_eq!(report.pages.len(), 3);
    assert!(out.join("atlas_p2_baseColor.png").is_file());
    assert!(out.join("atlas_p3_normal.png").is_file());
    // Model 1 needs two pages → cannot be rewritten → UV Remap Data.
    assert_eq!(report.models[0].pages.len(), 2);
    assert_eq!(report.models[0].outcome, ModelOutcome::Fallback);
    assert!(report.models[0]
        .warnings
        .iter()
        .any(|w| w.code == codes::MESH_MODEL_SPANS_PAGES));
    assert_eq!(report.models[1].outcome, ModelOutcome::Rewritten);
    let sidecar = report.models[0].sidecar.as_ref().unwrap();
    let file = RemapFile::from_json(&std::fs::read_to_string(sidecar).unwrap()).unwrap();
    let pages: BTreeSet<u32> = file.models[0].materials.iter().map(|m| m.atlas_page).collect();
    assert_eq!(pages.len(), 2);
    assert_pot(&report, &out);

    // Single page only → does not fit.
    o.multi_page = false;
    let err = pack::run(&[obj.clone(), green.clone()], &o, &dir.join("x"), "atlas", &AssimpExporter, &mut |_| {})
        .unwrap_err();
    assert_eq!(err.code, texopt_core::atlas::codes::ATLAS_DOES_NOT_FIT);
    // scaleToFit: halved until it fits one page.
    o.scale_to_fit = true;
    let r = run(&[obj, green], &o, &dir.join("fit"));
    assert_eq!(r.pages.len(), 1);
    assert!(r.scale_percent < 100.0);
    assert!(r.warnings.iter().any(|w| w.code == codes::MESH_TEXTURES_DOWNSCALED));
}

#[test]
fn texture_scale_and_inset() {
    let dir = scratch("scale");
    let green = quad_obj(&dir.join("src"), "Green", GREEN, (64, 32));
    let mut o = options(FormatChoice::Obj, false);
    o.texture_scale = 50;
    o.inset = InsetPolicy::None;
    let r = run(&[green], &o, &dir.join("out"));
    let m = &r.models[0].materials[0];
    let rect = m.rect.unwrap();
    assert_eq!((rect.width, rect.height), (32, 16));
    assert_eq!(m.source_size, Some([64, 32]));
    let page = &r.pages[0];
    let t = m.remap.unwrap().transform;
    assert!((t.scale[0] - 32.0 / page.width as f32).abs() < 1e-6);
    assert!((t.offset[0] - rect.x as f32 / page.width as f32).abs() < 1e-6);
}

#[test]
fn preview_writes_nothing_and_reports_layout() {
    let dir = scratch("preview");
    let s = scene(&dir.join("src"));
    let before: Vec<_> = walk(&dir);
    let mut steps = Vec::new();
    let p = pack::preview(&s.models, &PackOptions::default(), &mut |pr| steps.push(pr.done)).unwrap();
    assert_eq!(walk(&dir), before);
    assert_eq!(p.channel, TextureChannel::BaseColor);
    assert_eq!(p.pages.len(), p.report.pages.len());
    let img = &p.pages[0];
    assert!(img.width().is_power_of_two());
    assert_eq!(steps.last(), Some(&(s.models.len() + 1)));
    for mr in &p.report.models {
        for mat in &mr.materials {
            let r = mat.rect.unwrap();
            let key = format!("{}/{}", mr.name, mat.name);
            assert_eq!(*img.get_pixel(r.x + 1, r.y + 1), s.base[&key]);
        }
    }
    assert!(p.report.pages[0].occupancy > 0.0 && p.report.pages[0].occupancy <= 1.0);
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<_> = walkdir(dir);
    v.sort();
    v
}

fn walkdir(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walkdir(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[test]
fn inspect_lists_materials_textures_and_uv_ranges() {
    let dir = scratch("inspect");
    let tiled = fixtures::tiled_quads_obj(&dir.join("t"));
    let info = pack::inspect(&tiled).unwrap();
    assert_eq!(info.mesh_count, 2);
    assert_eq!(info.vertex_count, 8);
    assert_eq!(info.materials.len(), 2);
    let a = info.materials.iter().find(|m| m.name == "MatA").unwrap();
    assert_eq!(a.uv_channel, Some(0));
    let range = a.uv_range.unwrap();
    assert!(range.out_of_range);
    assert!((range.max[0] - 2.0).abs() < 1e-6);
    let t = &a.textures[0];
    assert_eq!(t.channel, TextureChannel::BaseColor);
    assert!(t.exists);
    assert_eq!((t.width, t.height), (Some(8), Some(8)));
    assert!(t.mtime_ms > 0);

    // Missing texture → exists=false + warning.
    std::fs::remove_file(dir.join("t/tex/blue.png")).unwrap();
    let info = pack::inspect(&tiled).unwrap();
    let b = info.materials.iter().find(|m| m.name == "MatB").unwrap();
    assert!(!b.textures[0].exists);
    assert!(info
        .warnings
        .iter()
        .any(|w| w.code == mesh::codes::MESH_TEXTURE_NOT_FOUND));

    let err = pack::inspect(&dir.join("t/nope.obj")).unwrap_err();
    assert_eq!(err.code, mesh::codes::MESH_IMPORT_FAILED);
}

#[test]
fn invalid_requests() {
    let dir = scratch("invalid");
    let green = quad_obj(&dir.join("src"), "Green", GREEN, (8, 8));
    let o = PackOptions::default();
    let e = |models: &[PathBuf], base: &str| {
        pack::run(models, &o, &dir.join("out"), base, &AssimpExporter, &mut |_| {}).unwrap_err()
    };
    assert_eq!(e(&[], "atlas").code, codes::MESH_NO_MODELS);
    assert_eq!(e(&[green.clone()], "a/b").params["param"], "baseName");
    // A model that fails to import is reported, the others still packed.
    let r = run(&[green, dir.join("missing.obj")], &PackOptions::default(), &dir.join("out2"));
    assert_eq!(r.models[1].outcome, ModelOutcome::Failed);
    assert_eq!(r.models[0].outcome, ModelOutcome::Rewritten);
}
