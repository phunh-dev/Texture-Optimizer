# Spike M1: reading and writing FBX / OBJ / DAE from Rust

Date: 2026-10-09. Machine: Windows 11 Pro, Rust 1.99.0 (MSVC), VS Build Tools,
**no CMake, no LLVM/libclang**, network available.

## TL;DR

| | Read | Write back | Notes |
|---|---|---|---|
| **OBJ (+MTL)** | yes | **yes, reliable** | one UV set only; no hierarchy, skinning or animation (format limits); normal maps come back as `bump` (height) |
| **DAE (COLLADA 1.4)** | yes | **yes, reliable** | geometry/hierarchy exact in every test; mesh order and names can change (`Quad_1`); bones without weights dropped (150 → 79 on a skinned character) |
| **FBX (binary/ASCII 7.5)** | yes | **yes, with a safety net** | world geometry exact on 11 of 12 third-party files, skinning and animation kept. **Wrong world geometry on files that use FBX geometric pivots (3ds Max style)**: we re-import every written file and refuse it (`MESH_EXPORT_GEOMETRY_CHANGED`) |
| glTF 2 | (yes) | **no** | Assimp 6.0.5's glTF2 writer **corrupted the heap** (process crash `0xC0000374`) on a real model. Not offered |

**Library:** Assimp 6.0.5 through the [`asset-importer-sys`](https://crates.io/crates/asset-importer-sys) 0.8.0 crate,
features `prebuilt` + `static-link`. It links a prebuilt static Assimp downloaded once from the crate's GitHub release. It uses
pregenerated bindings, so it needs **neither CMake nor libclang**. The only native compile step is a small C++ bridge built with the
`cc` crate (MSVC, clang or gcc). `russimp` **does not build** here (details below).

**Recommendation for M6:**
* OBJ and DAE: write the remapped model directly.
* FBX: write directly by default, with `verify_geometry` on (the default). If the check fails, or the user prefers not to rewrite FBX,
  use the **"UV Remap Data" mode**: export the atlas plus `<model>.uvremap.json`, and let the shipped Unity `AssetPostprocessor`
  (`mesh::remap_json::UNITY_POSTPROCESSOR_CS`) remap UVs at import time. The original FBX stays untouched.
* Run mesh import/export **in a worker process** in the Tauri app. Assimp is C++ in-process, and a crash in an exporter (seen with
  glTF) would take the whole app down.

---

## 1. Build options evaluated

### 1a. `russimp` 3.2.1 / `russimp-sys` 2.0.2: does NOT build

`spikes/assimp/russimp-try` (`russimp = { version = "3.2.1", features = ["prebuilt"] }`):

```
$ cargo build            (36 s, exit 101)
   Compiling russimp-sys v2.0.2
error: failed to run custom build command for `russimp-sys v2.0.2`
  thread 'main' panicked at ...\bindgen-0.63.0\lib.rs:2338:31:
  Unable to find libclang: "couldn't find any valid shared libraries matching:
  ['clang.dll', 'libclang.dll'], set the `LIBCLANG_PATH` environment variable ..."
```

* `russimp-sys/build.rs` **always** runs bindgen, so libclang is mandatory even with `prebuilt`. The `prebuilt` archive itself
  downloaded fine.
* `static-link` implies `build-assimp`, which needs **CMake**.
* Its prebuilt archives cover `x86_64` Windows, macOS and Linux only. There is **no `aarch64-apple-darwin`** (Apple Silicon)
  archive. The last release was v2.0.3 in May 2024.
* Verdict: rejected. It would need LLVM on every dev and CI machine and still has no Apple Silicon binaries.

### 1b. `asset-importer-sys` 0.8.0 (Assimp 6.0.5): works

`spikes/assimp/asset-importer-try` (`features = ["prebuilt", "static-link"]`):

```
$ cargo build --release   (68.5 s clean, incl. download + rustls/ring for the build script; exit 0)
   Compiling asset-importer-sys v0.8.0
   Compiling asset-importer-try v0.0.0
    Finished `release` profile [optimized] target(s) in 1m 08s
```

* The build script downloads `asset-importer-0.8.0-x86_64-pc-windows-msvc-static-md.tar.gz` (21.3 MB). It unpacks to a 150 MB
  `assimp-vc143-mt.lib` plus `zlibstatic.lib`, and checks the Assimp version against `manifest.txt` (`assimp_version=6.0.5`).
* Bindings: `src/bindings_pregenerated.rs` is copied, so bindgen and libclang are not involved.
* The C++ bridge (`wrapper.cpp`) is compiled by `cc` with the platform C++ compiler. CRT flavour (`/MD` vs `/MT`) follows Rust's
  `crt-static`.
* **Archive cache:** `$CARGO_TARGET_DIR/asset-importer-prebuilt` when that variable is set. Otherwise it is
  `<cargo registry src>/target/asset-importer-prebuilt` (165 MB unpacked). For offline or CI builds, set
  `ASSET_IMPORTER_PACKAGE_DIR` (a directory holding the `.tar.gz`) or `ASSET_IMPORTER_CACHE_DIR`. `ASSET_IMPORTER_OFFLINE` or
  `CARGO_NET_OFFLINE` make a missing archive a hard error instead of a download.
* **Prebuilt targets in the v0.8.0 release:**
  * `x86_64-pc-windows-msvc`: static/dylib × md/mt
  * `x86_64-apple-darwin` and `aarch64-apple-darwin`: static/dylib
  * `x86_64-unknown-linux-gnu`: static/dylib
  * Missing: `aarch64-unknown-linux-gnu`, `aarch64-pc-windows-msvc`. Those targets would need `build-assimp`, which requires CMake.
* System libraries it links: Windows `user32 gdi32 shell32 ole32 oleaut32 uuid advapi32`; macOS `c++` and the Foundation
  framework; Linux `stdc++` and `z` (needs `zlib1g-dev` on Ubuntu runners).
* **Not verified here:** macOS and Linux builds (no such machine). CI must cover them.
* **Binary size:** release `asset-importer-try.exe` (Assimp plus a tiny `main`) is **7.33 MB**, against a 0.13 MB hello world. That
  is about **+7.2 MB** for all Assimp importers and exporters. Trimming it needs a CMake build with `ASSIMP_BUILD_*_IMPORTER=OFF`,
  which is not possible with prebuilt archives.
* **Incremental build cost:** about 1 s (only the bridge). A clean `cargo build -p texopt-core` took 65 s including every
  dependency.

### 1c. Format-specific crates (`tobj`, `quick-xml`, `ufbx`): not needed

These were kept as plan B. They would mean a hand-written COLLADA writer and an ASCII FBX writer, which is weeks of work and hard to
get right. Assimp read and wrote all three formats here, so plan B was not pursued.

## 2. Exporters in the linked build

`cargo run --release -- <out>` (spike `src/main.rs`, `aiGetExportFormatCount` / `aiGetExportFormatDescription`):

```
== 22 export formats ==
  id=collada    ext=dae    COLLADA - Digital Asset Exchange Schema
  id=x          ext=x      X Files
  id=stp        ext=stp    Step Files
  id=obj        ext=obj    Wavefront OBJ format
  id=objnomtl   ext=obj    Wavefront OBJ format without material file
  id=stl        ext=stl    Stereolithography
  id=stlb       ext=stl    Stereolithography (binary)
  id=ply        ext=ply    Stanford Polygon Library
  id=plyb       ext=ply    Stanford Polygon Library (binary)
  id=3ds        ext=3ds    Autodesk 3DS (legacy)
  id=gltf2      ext=gltf   GL Transmission Format v. 2
  id=glb2       ext=glb    GL Transmission Format v. 2 (binary)
  id=gltf       ext=gltf   GL Transmission Format
  id=glb        ext=glb    GL Transmission Format (binary)
  id=assbin     ext=assbin Assimp Binary File
  id=assxml     ext=assxml Assimp XML Document
  id=x3d        ext=x3d    Extensible 3D
  id=fbx        ext=fbx    Autodesk FBX (binary)
  id=fbxa       ext=fbx    Autodesk FBX (ascii)
  id=3mf        ext=3mf    The 3MF-File-Format
  id=pbrt       ext=pbrt   pbrt-v4 scene description file
  id=assjson    ext=json   Assimp JSON Document
Assimp version 6.0.5 rev 392a658f flags 0x14
```

`fbx`, `fbxa`, `collada` and `obj` are all present (test `backend_reports_version_and_writers`).

## 3. How export works without fighting Assimp's allocator

Assimp scenes are C++ objects, and Rust must not free or reallocate their memory. `mesh::export`:

1. It bitwise-copies the imported `aiScene` header into a Rust-owned struct.
2. `mMeshes` points to Rust-owned `aiMesh` copies. Only the remapped UV channel is redirected to a Rust `Vec`. Vertices, faces,
   bones and animations are shared with the original.
3. `mMaterials` points to Rust-owned `aiMaterial`s. Their property lists mix the original property pointers with Rust-owned
   `$tex.file` and `?mat.name` properties (atlas paths, merged material).
4. It calls `aiExportScene`. Assimp's `Exporter::Export` deep-copies its input through `SceneCombiner::CopyScene`, which explicitly
   supports user-allocated scenes, and never frees it.

All Assimp calls go through one global mutex, because the C API keeps the last error in a global string.

## 4. Round trips with generated fixtures (automated, `cargo test -p texopt-core`)

Fixtures are generated by code in `crates/texopt-core/src/mesh/fixtures.rs`, so there are no binary files in the repo:

* **OBJ+MTL:** two quads, `MatA` (`map_Kd tex/red.png`, `norm tex/red_n.png`) and `MatB` (blue). UVs are `[0,1]²` and
  `[0.25,0.75]²`.
* **DAE:** the same scene written as COLLADA 1.4.1, using `<polylist>` and phong/diffuse `<texture>`.
* **FBX:** the OBJ converted by Assimp's binary FBX writer (`fixtures::two_quads_fbx`). There is no independent FBX writer, so
  third-party FBX files are covered in §5.
* **Tiled OBJ:** UVs `u ∈ [0,2]` and `[1.25,1.75]`, used for the out-of-range policies.

Results (all pass):

* **Import:** mesh, material and texture-channel extraction, relative path resolution, a missing-texture warning, the same-name
  fallback next to the model, and import errors.
* **Round trip:** every source format × {OBJ, DAE, FBX, FBX-ASCII} × {keep materials, merge materials}. UVs match the remap within
  1e-5. Vertex counts are equal. Texture references point to `atlas_baseColor.png` (and the normal atlas). No `tex/…` reference
  remains. With merging, all meshes share one material named `Atlas`, and non-remapped materials are kept.
* **Sampling:** red and blue solid textures are placed in a 64² atlas (top-left and bottom-right; the rest is green, so a V-flip
  bug would sample green). After export and re-import, the atlas is read through the re-imported material's texture path. Sampling
  it at every vertex plus a 5×5 interior grid per face gives back each mesh's original colour, for 3 sources × 4 writers.
* **Out of range:**
  * detection per material;
  * `skipMaterial` warns and leaves the material untouched;
  * `clamp`;
  * `wrapIntoTile` warns about the straddling quad (`MESH_UV_WRAP_STRADDLE`);
  * `bakeRepeat`: the tile counts and the `maxTiles` error are checked, and sampling a 2×1 repeated two-colour texture matches
    `fract(uv)` sampling of the original.
* **Normal-map survival:** the normal map survives in every format except OBJ. Assimp's OBJ writer emits `bump` and `map_bump`, and
  every OBJ reader, Assimp included, reads those back as a height map. This is asserted explicitly.

## 5. Third-party FBX files (spike `src/bin/realworld.rs`, not in CI)

Twelve FBX files shipped inside the Unity 6000.0.83f1 editor, all written by the Autodesk FBX SDK, are not redistributed. Each goes
through import (`mesh::import`), then remaps every material into a quadrant of a 1024² atlas (half-texel inset; `wrap` for
out-of-range UVs), then export, then re-import. The re-import is compared on UVs (mesh by mesh) and, through raw Assimp, on the
world-space bounding box and on node, bone, animation, normal, tangent and colour counts.

```
cargo run --release --bin realworld -- <out> <files...>      (NOVERIFY=1 disables the geometry guard)
```

| File (features) | FBX | FBX ASCII | OBJ | DAE |
|---|---|---|---|---|
| Lion (7.9k verts, vertex colours) | exact | UV err 5e-7 | exact | exact |
| UnityMaterialBall (11 meshes, 2 UV sets, **geometric pivots**) | **bbox off by 40 units → rejected by the guard** | **rejected** | exact geometry, 2nd UV set lost | exact |
| Curtain, Island (26k verts), LockOfHair, Spotlight (8 meshes) | exact | UV err ≤ 5e-7, bbox err ≤ 5e-5 | exact | exact (mesh order differs) |
| chest (4 meshes, 1 animation, textures) | exact, animation kept | ok | animation dropped | exact |
| Boat, pillar (tangents, UVs out of range), clover (diffuse+opacity) | exact geometry | ok | ok | ok |
| Chomper (150 bones) | exact, 150 bones | ok | no skin | **79 bones** (unweighted bones dropped) |
| Hoodie (100 bones + animation) | exact, bones+anim kept | ok | no skin | bones+anim kept |

"exact" means the world bbox is identical and the UV error is 0 on every mesh whose vertex count is unchanged. A few meshes re-import with slightly fewer vertices (Spotlight `Tripod_Base` 3652 → 3646), because `JoinIdenticalVertices` merges more vertices once some per-vertex data is gone or re-encoded: tangents and second UV sets are not written back, and the cause for Spotlight was not investigated further. UV and bbox checks still pass. The full logs are reproducible with the command above.

**Data-loss notes (all writers):**

* **Tangents/bitangents are never written** (pillar: tangents 1 → 0). Engines recompute them. Vertex counts after
  `JoinIdenticalVertices` can then drop, for example 1531 → 1277.
* **OBJ:** a single UV set, no node hierarchy (world transforms are baked), no skinning or animation, and meshes sharing a material
  get merged on re-import. Normal map → `bump`.
* **COLLADA:** mesh order and names change (`_1` suffix), and unweighted bones are dropped.
* **FBX:**
  * Geometric pivots (`GeometricTranslation`/`Rotation`, 3ds Max object offsets) are mishandled by Assimp's writer. The guard catches
    it (relative bbox error 0.209 > 1e-4).
  * Units: the writer emits `UnitScaleFactor 1` (cm). No scale drift was seen, because Assimp keeps the source units in the
    vertex/node data.
  * Material properties other than textures and colours are not round-tripped by Assimp in general. Merging materials keeps the
    first remapped material's colour properties.
* **Embedded textures:** reported through `Model::embedded_textures`, and `extract_embedded_textures` writes them (compressed
  payloads as-is, raw texels → PNG). None of the test files embed media, so **extraction is untested**.

**Import-setting findings:**

* `IMPORT_FBX_PRESERVE_PIVOTS = false` (to avoid `$AssimpFbx$` helper nodes) **drops a 90° rotation** on UnityMaterialBall. This was
  verified on the import alone with `NOPIVOTS=1 realworld --stats`. The module therefore keeps Assimp's default (pivots preserved).
* No triangulation is applied (quads survive OBJ, DAE and FBX).
* `JoinIdenticalVertices` is applied so meshes are indexed.

**glTF2 crash:**

```
rawexport.exe Lion.fbx glb2 out.glb    → process exit -1073740940 (0xC0000374 STATUS_HEAP_CORRUPTION)
rawexport.exe Lion.fbx gltf2 out.gltf  → aiReturn_FAILURE, then heap corruption
```

`rawexport` exports the scene exactly as Assimp imported it, without Rust copies, so this is an Assimp bug. glTF was removed from
`ExportFormat`.

## 6. UV conventions (documented in `mesh::uv_remap`)

* Atlas rects are image pixels with the origin at the **top-left** and y pointing down.
* Assimp returns UVs as stored in FBX/OBJ/DAE, with the origin at the **bottom-left** (OpenGL; Unity, Godot and Blender agree). Use
  `UvOrigin::BottomLeft`. `TopLeft` is for flipped data (`aiProcess_FlipUVs`, DirectX/Unreal conventions).
* `uv' = offset + uv * scale`:
  * `offset.u = (x + inset) / W`, `scale.u = (w − 2·inset) / W`
  * BottomLeft: `offset.v = 1 − (y + h − inset) / H`
  * TopLeft: `offset.v = (y + inset) / H`
  * `scale.v = (h − 2·inset) / H`
  * Default inset is half a texel, so UV 0 and 1 hit the centres of the edge texels.
* `wrapIntoTile` decides the tile **per face** (`floor(min corner + 1e-4)`), so a quad spanning `u ∈ [1,2]` keeps its right edge at
  1.0. A per-vertex `fract()` would collapse that edge to 0. Faces spanning more than one tile, and vertices shared across tiles,
  are counted and reported.
* The packer must not rotate rectangles used for meshes.

## 7. Fallback: "UV Remap Data" mode

* `mesh::remap_json::RemapFile` is written as `<model stem>.uvremap.json` next to the model (`sidecar_path`). It is shaped for
  Unity `JsonUtility`: no dictionaries, and vectors as arrays. Schema version 1:

  ```json
  { "version": 1, "generator": "texture-optimizer", "uvOrigin": "bottomLeft",
    "atlasPages": [ { "index": 0, "width": 2048, "height": 2048,
                      "textures": [ { "channel": "baseColor", "path": "atlas_baseColor.png" } ] } ],
    "models": [ { "model": "crate.fbx", "mergedMaterialName": "AtlasMaterial",
      "materials": [ { "materialIndex": 1, "materialName": "Crate", "skipped": false,
                       "atlasPage": 0, "uvChannel": 0, "offset": [0.0, 0.5], "scale": [0.5, 0.5],
                       "normalize": "none", "repeatOrigin": [0, 0], "repeatTiles": [1, 1],
                       "originalTextures": [ { "channel": "baseColor", "path": "tex/crate.png" } ] } ] } ] }
  ```

* `mesh::remap_json::UNITY_POSTPROCESSOR_CS` (file name `TextureOptimizerUvRemapPostprocessor.cs`) is an `AssetPostprocessor` to
  put in any `Editor/` folder.
* In `OnPreprocessModel` it registers the sidecar as an import dependency (Unity 2020.2+) and turns **Weld Vertices off**, so
  submeshes never share a vertex that would need two different UVs.
* In `OnPostprocessModel` it parses the sidecar with `JsonUtility` and picks the `models[]` entry whose `model` matches the asset
  file name (case-insensitive), or the only entry. For every `Renderer`, it matches submesh material names (minus ` (Instance)`)
  against `materialName`. On the submesh's vertices, in UV channel `uvChannel`, it applies `normalize` (none/clamp/repeat, or wrap
  with the tile decided per triangle) and then `offset + n * scale`. Each vertex is remapped once, and conflicts are logged.
* Unity UVs share Assimp's bottom-left origin, so no flip is needed.
* Assigning the atlas material is left to the user, or to Unity's material remapping by name.
* Verified: the script compiles with 0 errors and 0 warnings against Unity 6000.0.83f1's `UnityEngine.CoreModule`,
  `UnityEditor.CoreModule` and `JSONSerializeModule` (`dotnet build`, netstandard2.1).
* **Not verified:** running it inside a Unity project.

## 8. Open items / honest gaps

* Assimp-written FBX/DAE/OBJ files were validated only by **re-importing them with Assimp**. Opening them in Unity or Blender, the
  M1 "mở lại kết quả trong Unity/Blender" step, was not done. That should be a manual check in M6 before claiming FBX support to
  users.
* The macOS and Linux builds of the prebuilt static Assimp are untested here.
* The Unity postprocessor has only been compiled, not run in an editor.
* Unreal and Godot fallback importers are not written. Godot can use the DAE/OBJ output, or a small `EditorScenePostImport` later.

## 9. Files

* `crates/texopt-core/src/mesh/`:
  * `mod.rs`: API and codes
  * `model.rs`
  * `uv_remap.rs`: pure math with unit tests
  * `remap_json.rs`
  * `assimp.rs`: FFI backend, behind feature `assimp`
  * `fixtures.rs`
  * `unity_remap_postprocessor.cs`
* `crates/texopt-core/tests/mesh_{import,roundtrip,sampling,out_of_range}.rs`
* `spikes/assimp/russimp-try/`: the failed russimp build
* `spikes/assimp/asset-importer-try/`:
  * `main.rs`: exporter list and raw round trip
  * `bin/realworld.rs`: third-party FBX check
  * `bin/rawexport.rs`: crash isolation

  This crate has its own `[workspace]` and is not part of the app build.
