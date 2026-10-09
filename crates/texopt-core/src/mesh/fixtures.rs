//! Deterministic text fixtures for mesh tests and spikes (no binary files in
//! the repo). Each writer creates the model plus its PNG textures under a
//! directory and returns the model path.
//!
//! Geometry shared by all fixtures: two unit quads, each with its own
//! material and diffuse texture.
//! * `QuadA` / `MatA` → `tex/red.png`  (UVs cover `[0,1]²`)
//! * `QuadB` / `MatB` → `tex/blue.png` (UVs cover `[0.25,0.75]²`)

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use image::Rgba;

use crate::fixtures::solid;

pub const RED: Rgba<u8> = Rgba([220, 30, 30, 255]);
pub const BLUE: Rgba<u8> = Rgba([30, 40, 210, 255]);
pub const NORMAL_A: Rgba<u8> = Rgba([128, 128, 255, 255]);
pub const NORMAL_B: Rgba<u8> = Rgba([100, 150, 240, 255]);

/// UVs per quad, counter-clockwise from the bottom-left corner.
pub const QUAD_A_UVS: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
pub const QUAD_B_UVS: [[f32; 2]; 4] = [[0.25, 0.25], [0.75, 0.25], [0.75, 0.75], [0.25, 0.75]];

const QUAD_A_POS: [[f32; 3]; 4] = [
    [0.0, 0.0, 0.0],
    [1.0, 0.0, 0.0],
    [1.0, 1.0, 0.0],
    [0.0, 1.0, 0.0],
];
const QUAD_B_POS: [[f32; 3]; 4] = [
    [2.0, 0.0, 0.0],
    [3.0, 0.0, 0.0],
    [3.0, 1.0, 0.0],
    [2.0, 1.0, 0.0],
];

fn write(path: &Path, text: &str) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).expect("create fixture dir");
    }
    std::fs::write(path, text).expect("write fixture");
}

/// Write the four 8×8 solid textures into `dir/tex/`.
pub fn write_textures(dir: &Path) {
    std::fs::create_dir_all(dir.join("tex")).expect("create tex dir");
    for (name, color) in [
        ("red.png", RED),
        ("blue.png", BLUE),
        ("red_n.png", NORMAL_A),
        ("blue_n.png", NORMAL_B),
    ] {
        solid(8, 8, color)
            .save(dir.join("tex").join(name))
            .expect("save texture");
    }
}

fn obj_text(mtl_name: &str, uvs_a: &[[f32; 2]; 4], uvs_b: &[[f32; 2]; 4]) -> String {
    let mut s = format!("# texture-optimizer fixture\nmtllib {mtl_name}\n");
    for (name, mat, pos, uvs, base) in [
        ("QuadA", "MatA", &QUAD_A_POS, uvs_a, 0),
        ("QuadB", "MatB", &QUAD_B_POS, uvs_b, 4),
    ] {
        let _ = writeln!(s, "o {name}");
        for p in pos {
            let _ = writeln!(s, "v {} {} {}", p[0], p[1], p[2]);
        }
        for uv in uvs {
            let _ = writeln!(s, "vt {} {}", uv[0], uv[1]);
        }
        let _ = writeln!(s, "vn 0 0 1\nusemtl {mat}");
        let n = base / 4 + 1;
        let i = |k: usize| base + k + 1;
        let _ = writeln!(
            s,
            "f {}/{}/{n} {}/{}/{n} {}/{}/{n} {}/{}/{n}",
            i(0),
            i(0),
            i(1),
            i(1),
            i(2),
            i(2),
            i(3),
            i(3)
        );
    }
    s
}

/// OBJ + MTL with diffuse (`map_Kd`) and normal (`norm`) maps.
pub fn two_quads_obj(dir: &Path) -> PathBuf {
    write_textures(dir);
    write(
        &dir.join("two_quads.mtl"),
        "newmtl MatA\nKd 1 1 1\nmap_Kd tex/red.png\nnorm tex/red_n.png\n\n\
         newmtl MatB\nKd 1 1 1\nmap_Kd tex/blue.png\nnorm tex/blue_n.png\n",
    );
    let path = dir.join("two_quads.obj");
    write(&path, &obj_text("two_quads.mtl", &QUAD_A_UVS, &QUAD_B_UVS));
    path
}

/// UVs of the tiling fixture: `QuadA` repeats its texture 2× in u
/// (`u ∈ [0,2]`), `QuadB` sits entirely in tile `u ∈ [1,2]`.
pub const TILED_A_UVS: [[f32; 2]; 4] = [[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [0.0, 1.0]];
pub const TILED_B_UVS: [[f32; 2]; 4] = [[1.25, 0.25], [1.75, 0.25], [1.75, 0.75], [1.25, 0.75]];

/// OBJ whose UVs leave `[0,1]` (diffuse only).
pub fn tiled_quads_obj(dir: &Path) -> PathBuf {
    write_textures(dir);
    write(
        &dir.join("tiled.mtl"),
        "newmtl MatA\nmap_Kd tex/red.png\n\nnewmtl MatB\nmap_Kd tex/blue.png\n",
    );
    let path = dir.join("tiled.obj");
    write(&path, &obj_text("tiled.mtl", &TILED_A_UVS, &TILED_B_UVS));
    path
}

/// COLLADA 1.4.1 with two `<polylist>` quads and phong materials whose
/// diffuse samples an `<image>`.
pub fn two_quads_dae(dir: &Path) -> PathBuf {
    write_textures(dir);
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<COLLADA xmlns="http://www.collada.org/2005/11/COLLADASchema" version="1.4.1">
  <asset><unit name="meter" meter="1"/><up_axis>Y_UP</up_axis></asset>
  <library_images>
    <image id="img_red" name="img_red"><init_from>tex/red.png</init_from></image>
    <image id="img_blue" name="img_blue"><init_from>tex/blue.png</init_from></image>
  </library_images>
  <library_effects>
"#,
    );
    for (fx, img) in [("fx_A", "img_red"), ("fx_B", "img_blue")] {
        let _ = write!(
            s,
            r#"    <effect id="{fx}"><profile_COMMON>
      <newparam sid="{img}-surface"><surface type="2D"><init_from>{img}</init_from></surface></newparam>
      <newparam sid="{img}-sampler"><sampler2D><source>{img}-surface</source></sampler2D></newparam>
      <technique sid="common"><phong><diffuse><texture texture="{img}-sampler" texcoord="UVMap"/></diffuse></phong></technique>
    </profile_COMMON></effect>
"#
        );
    }
    s.push_str(
        r##"  </library_effects>
  <library_materials>
    <material id="MatA" name="MatA"><instance_effect url="#fx_A"/></material>
    <material id="MatB" name="MatB"><instance_effect url="#fx_B"/></material>
  </library_materials>
  <library_geometries>
"##,
    );
    for (g, pos, uvs) in [
        ("QuadA", &QUAD_A_POS, &QUAD_A_UVS),
        ("QuadB", &QUAD_B_POS, &QUAD_B_UVS),
    ] {
        let p: Vec<String> = pos
            .iter()
            .flat_map(|v| v.iter().map(|c| c.to_string()))
            .collect();
        let t: Vec<String> = uvs
            .iter()
            .flat_map(|v| v.iter().map(|c| c.to_string()))
            .collect();
        let _ = write!(
            s,
            r##"    <geometry id="{g}-mesh" name="{g}"><mesh>
      <source id="{g}-pos"><float_array id="{g}-pos-arr" count="12">{p}</float_array>
        <technique_common><accessor source="#{g}-pos-arr" count="4" stride="3"><param name="X" type="float"/><param name="Y" type="float"/><param name="Z" type="float"/></accessor></technique_common></source>
      <source id="{g}-uv"><float_array id="{g}-uv-arr" count="8">{t}</float_array>
        <technique_common><accessor source="#{g}-uv-arr" count="4" stride="2"><param name="S" type="float"/><param name="T" type="float"/></accessor></technique_common></source>
      <vertices id="{g}-vtx"><input semantic="POSITION" source="#{g}-pos"/></vertices>
      <polylist material="{g}-mat" count="1">
        <input semantic="VERTEX" source="#{g}-vtx" offset="0"/>
        <input semantic="TEXCOORD" source="#{g}-uv" offset="1" set="0"/>
        <vcount>4</vcount><p>0 0 1 1 2 2 3 3</p>
      </polylist>
    </mesh></geometry>
"##,
            p = p.join(" "),
            t = t.join(" ")
        );
    }
    s.push_str("  </library_geometries>\n  <library_visual_scenes><visual_scene id=\"Scene\" name=\"Scene\">\n");
    for (g, mat) in [("QuadA", "MatA"), ("QuadB", "MatB")] {
        let _ = writeln!(
            s,
            r##"    <node id="{g}" name="{g}" type="NODE"><instance_geometry url="#{g}-mesh" name="{g}"><bind_material><technique_common><instance_material symbol="{g}-mat" target="#{mat}"><bind_vertex_input semantic="UVMap" input_semantic="TEXCOORD" input_set="0"/></instance_material></technique_common></bind_material></instance_geometry></node>"##
        );
    }
    s.push_str("  </visual_scene></library_visual_scenes>\n  <scene><instance_visual_scene url=\"#Scene\"/></scene>\n</COLLADA>\n");
    let path = dir.join("two_quads.dae");
    write(&path, &s);
    path
}

/// FBX fixture: the OBJ fixture converted by Assimp's binary FBX exporter
/// (there is no independent FBX writer available; see
/// docs/spikes/3d-assimp.md for the checks against third-party FBX files).
#[cfg(feature = "assimp")]
pub fn two_quads_fbx(dir: &Path) -> crate::OpResult<PathBuf> {
    let obj = two_quads_obj(&dir.join("obj_src"));
    write_textures(dir);
    let loaded = super::import(&obj)?;
    let options = super::ExportOptions {
        format: super::ExportFormat::Fbx,
        merge_materials: false,
        merged_material_name: None,
        verify_geometry: true,
        atlas_textures: Default::default(),
    };
    let path = dir.join("two_quads.fbx");
    super::export(&loaded, &Default::default(), &options, &path)?;
    Ok(path)
}
