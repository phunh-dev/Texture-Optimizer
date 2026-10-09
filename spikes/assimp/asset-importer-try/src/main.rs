//! Spike: Assimp via `asset-importer-sys` (prebuilt, static) — raw FFI only.
//!
//! 1. print Assimp version + every compiled-in exporter (`aiGetExportFormat*`)
//! 2. write a tiny OBJ+MTL (2 quads, 2 materials, diffuse textures)
//! 3. import it, export to obj / collada / fbx / fbxa / gltf2, re-import each
//!    output and print vertex counts, first UVs and diffuse texture paths.
//!
//! Run: `cargo run --release -- <out_dir>`

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};

use asset_importer_sys as sys;

fn ai_str(s: &sys::aiString) -> String {
    let bytes: Vec<u8> = s.data[..s.length as usize]
        .iter()
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn last_error() -> String {
    unsafe {
        CStr::from_ptr(sys::aiGetErrorString())
            .to_string_lossy()
            .into_owned()
    }
}

fn list_exporters() {
    unsafe {
        let n = sys::aiGetExportFormatCount();
        println!("== {n} export formats ==");
        for i in 0..n {
            let d = sys::aiGetExportFormatDescription(i);
            let d_ref = &*d;
            println!(
                "  id={:<10} ext={:<6} {}",
                CStr::from_ptr(d_ref.id).to_string_lossy(),
                CStr::from_ptr(d_ref.fileExtension).to_string_lossy(),
                CStr::from_ptr(d_ref.description).to_string_lossy()
            );
            sys::aiReleaseExportFormatDescription(d);
        }
        println!(
            "Assimp version {}.{}.{} rev {:x} flags {:#x}",
            sys::aiGetVersionMajor(),
            sys::aiGetVersionMinor(),
            sys::aiGetVersionPatch(),
            sys::aiGetVersionRevision(),
            sys::aiGetCompileFlags()
        );
    }
}

fn write_obj(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("two_quads.mtl"),
        "newmtl MatA\nKd 1 1 1\nmap_Kd tex/red.png\n\nnewmtl MatB\nKd 1 1 1\nmap_Kd tex/blue.png\n",
    )
    .unwrap();
    let obj = "mtllib two_quads.mtl\n\
o QuadA\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\n\
vt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nvn 0 0 1\n\
usemtl MatA\nf 1/1/1 2/2/1 3/3/1 4/4/1\n\
o QuadB\nv 2 0 0\nv 3 0 0\nv 3 1 0\nv 2 1 0\n\
vt 0.25 0.25\nvt 0.75 0.25\nvt 0.75 0.75\nvt 0.25 0.75\n\
usemtl MatB\nf 5/5/1 6/6/1 7/7/1 8/8/1\n";
    let p = dir.join("two_quads.obj");
    std::fs::write(&p, obj).unwrap();
    p
}

unsafe fn dump(label: &str, scene: *const sys::aiScene) {
    let s = unsafe { &*scene };
    println!(
        "-- {label}: {} meshes, {} materials, {} embedded textures",
        s.mNumMeshes, s.mNumMaterials, s.mNumTextures
    );
    for i in 0..s.mNumMeshes as usize {
        let m = unsafe { &**s.mMeshes.add(i) };
        let uv0 = m.mTextureCoords[0];
        let uvs: Vec<(f32, f32)> = if uv0.is_null() {
            vec![]
        } else {
            (0..m.mNumVertices as usize)
                .map(|k| unsafe { ((*uv0.add(k)).x, (*uv0.add(k)).y) })
                .collect()
        };
        println!(
            "   mesh[{i}] '{}' verts={} faces={} mat={} normals={} uv0={:?}",
            ai_str(&m.mName),
            m.mNumVertices,
            m.mNumFaces,
            m.mMaterialIndex,
            !m.mNormals.is_null(),
            uvs
        );
    }
    for i in 0..s.mNumMaterials as usize {
        let mat = unsafe { *s.mMaterials.add(i) };
        let mut name = sys::aiString::default();
        let key = CString::new("?mat.name").unwrap();
        unsafe { sys::aiGetMaterialString(mat, key.as_ptr(), 0, 0, &mut name) };
        let mut path = sys::aiString::default();
        let mut uvidx = 0u32;
        let r = unsafe {
            sys::aiGetMaterialTexture(
                mat,
                sys::aiTextureType::aiTextureType_DIFFUSE,
                0,
                &mut path,
                std::ptr::null_mut(),
                &mut uvidx,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        let tex = if r == sys::aiReturn::aiReturn_SUCCESS {
            ai_str(&path)
        } else {
            "<none>".into()
        };
        println!(
            "   material[{i}] '{}' diffuse='{}' uvindex={uvidx}",
            ai_str(&name),
            tex
        );
    }
}

fn import(path: &Path) -> *const sys::aiScene {
    let c = CString::new(path.to_str().unwrap()).unwrap();
    let flags = sys::aiPostProcessSteps::aiProcess_Triangulate as u32
        | sys::aiPostProcessSteps::aiProcess_JoinIdenticalVertices as u32;
    let s = unsafe { sys::aiImportFile(c.as_ptr(), flags) };
    if s.is_null() {
        panic!("import {} failed: {}", path.display(), last_error());
    }
    s
}

fn main() {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "spike-out".into()),
    );
    list_exporters();
    let obj = write_obj(&out);
    let scene = import(&obj);
    unsafe { dump("source OBJ", scene) };

    for (fmt, ext) in [
        ("obj", "obj"),
        ("collada", "dae"),
        ("fbx", "fbx"),
        ("fbxa", "fbx"),
        ("gltf2", "gltf"),
    ] {
        let dst = out.join(format!("export_{fmt}.{ext}"));
        let cf = CString::new(fmt).unwrap();
        let cp = CString::new(dst.to_str().unwrap()).unwrap();
        let t = std::time::Instant::now();
        let r = unsafe { sys::aiExportScene(scene, cf.as_ptr(), cp.as_ptr(), 0) };
        if r != sys::aiReturn::aiReturn_SUCCESS {
            println!("!! export {fmt} FAILED: {}", last_error());
            continue;
        }
        println!(
            "== exported {fmt} -> {} ({} bytes, {:?})",
            dst.display(),
            std::fs::metadata(&dst).map(|m| m.len()).unwrap_or(0),
            t.elapsed()
        );
        let c = CString::new(dst.to_str().unwrap()).unwrap();
        let back = unsafe {
            sys::aiImportFile(
                c.as_ptr(),
                sys::aiPostProcessSteps::aiProcess_Triangulate as u32,
            )
        };
        if back.is_null() {
            println!("!! re-import {fmt} FAILED: {}", last_error());
            continue;
        }
        unsafe {
            dump(&format!("re-imported {fmt}"), back);
            sys::aiReleaseImport(back);
        }
    }
    unsafe { sys::aiReleaseImport(scene) };
}
