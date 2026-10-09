//! Export an imported scene *as Assimp returned it* (no Rust-owned copy) to
//! isolate crashes in Assimp exporters from issues in texopt-core's export.
//! Run: `cargo run --release --bin rawexport -- <in> <format id> <out>`

use std::ffi::{CStr, CString};

use asset_importer_sys as sys;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (input, fmt, out) = (&a[1], &a[2], &a[3]);
    let ci = CString::new(input.as_str()).unwrap();
    let flags = sys::aiPostProcessSteps::aiProcess_JoinIdenticalVertices as u32;
    let s = unsafe { sys::aiImportFile(ci.as_ptr(), flags) };
    assert!(!s.is_null());
    eprintln!("imported, exporting to {fmt} ...");
    let cf = CString::new(fmt.as_str()).unwrap();
    let co = CString::new(out.as_str()).unwrap();
    let r = unsafe { sys::aiExportScene(s, cf.as_ptr(), co.as_ptr(), 0) };
    let err = unsafe { CStr::from_ptr(sys::aiGetErrorString()) }.to_string_lossy();
    eprintln!("result {r:?} {err}");
    unsafe { sys::aiReleaseImport(s) };
}
