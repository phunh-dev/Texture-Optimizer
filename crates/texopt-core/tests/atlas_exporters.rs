mod atlas_support;

use std::collections::BTreeMap;

use atlas_support::*;
use serde_json::{Value, json};
use texopt_core::ImageBuf;
use texopt_core::atlas::codes::*;
use texopt_core::atlas::exporters::unity::parse_meta;
use texopt_core::atlas::exporters::{
    GenericJsonOptions, GodotOptions, GodotVersion, JsonFormat, Paper2dExtension, UnityOptions,
    UnityPivot, UnityVersion, UnrealOptions, adapt_params, files_to_read, page_file_name,
};
use texopt_core::atlas::{
    AtlasParams, AtlasProject, AtlasResult, ExistingFiles, ExporterConfig, Frame, IncrementalMode,
    PROJECT_VERSION, PageInfo, ProjectSprite, Size, build, export,
};

fn sprite(
    name: &str,
    frame: [u32; 4],
    source: [u32; 2],
    sss: [u32; 4],
    rotated: bool,
    aliases: &[&str],
) -> ProjectSprite {
    ProjectSprite {
        name: name.into(),
        hash: format!("hash-{name}"),
        page: 0,
        frame: Frame {
            x: frame[0],
            y: frame[1],
            w: frame[2],
            h: frame[3],
        },
        rotated,
        trimmed: (sss[2], sss[3]) != (source[0], source[1]),
        source_size: Size {
            w: source[0],
            h: source[1],
        },
        sprite_source_size: Frame {
            x: sss[0],
            y: sss[1],
            w: sss[2],
            h: sss[3],
        },
        aliases: aliases.iter().map(|s| s.to_string()).collect(),
    }
}

/// 64x32 page: `hero` 16x24 untrimmed at (0,0); `coin` trimmed 8x8 from a
/// 12x10 source at (18,0) with alias `coin_copy`.
fn sample(exporter_state: Value) -> AtlasResult {
    let project = AtlasProject {
        version: PROJECT_VERSION,
        params: AtlasParams::default(),
        pages: vec![PageInfo {
            width: 64,
            height: 32,
        }],
        sprites: vec![
            sprite(
                "coin",
                [18, 0, 8, 8],
                [12, 10],
                [1, 1, 8, 8],
                false,
                &["coin_copy"],
            ),
            sprite("hero", [0, 0, 16, 24], [16, 24], [0, 0, 16, 24], false, &[]),
        ],
        exporter_state,
    };
    project.validate().unwrap();
    AtlasResult {
        pages: vec![ImageBuf::new(64, 32)],
        project,
        warnings: vec![],
    }
}

fn rotated_sample() -> AtlasResult {
    let mut r = sample(Value::Null);
    r.project.sprites.push(sprite(
        "rot",
        [30, 0, 20, 10],
        [10, 20],
        [0, 0, 10, 20],
        true,
        &[],
    ));
    r.project.validate().unwrap();
    r
}

fn files_map(out: &texopt_core::atlas::ExportOutput) -> BTreeMap<String, String> {
    out.files
        .iter()
        .filter(|(p, _)| !p.ends_with(".png"))
        .map(|(p, b)| (p.clone(), String::from_utf8(b.clone()).unwrap()))
        .collect()
}

fn paths(out: &texopt_core::atlas::ExportOutput) -> Vec<&str> {
    out.files.iter().map(|(p, _)| p.as_str()).collect()
}

// ------------------------------------------------------------ Generic JSON

const COIN_TP: &str = r#"{"frame":{"x":18,"y":0,"w":8,"h":8},"rotated":false,"trimmed":true,"spriteSourceSize":{"x":1,"y":1,"w":8,"h":8},"sourceSize":{"w":12,"h":10},"pivot":{"x":0.5,"y":0.5}}"#;
const HERO_TP: &str = r#"{"frame":{"x":0,"y":0,"w":16,"h":24},"rotated":false,"trimmed":false,"spriteSourceSize":{"x":0,"y":0,"w":16,"h":24},"sourceSize":{"w":16,"h":24},"pivot":{"x":0.5,"y":0.5}}"#;
const META_TP: &str = r#"{"app":"Texture Optimizer","version":"1.0","image":"img/sheet.png","format":"RGBA8888","size":{"w":64,"h":32},"scale":"1"}"#;

fn generic(format: JsonFormat, pretty: bool) -> ExporterConfig {
    ExporterConfig::GenericJson(GenericJsonOptions {
        format,
        image_path_prefix: "img/".into(),
        include_trim_info: true,
        pretty,
        ..GenericJsonOptions::default()
    })
}

#[test]
fn generic_json_hash_snapshot() {
    let out = export(
        &sample(Value::Null),
        "sheet",
        &generic(JsonFormat::Hash, false),
        &ExistingFiles::new(),
    )
    .unwrap();
    assert_eq!(paths(&out), vec!["sheet.png", "sheet.json"]);
    let text = &files_map(&out)["sheet.json"];
    let expected = format!(
        r#"{{"frames":{{"coin":{COIN_TP},"coin_copy":{COIN_TP},"hero":{HERO_TP}}},"meta":{META_TP}}}"#
    );
    assert_eq!(text, &expected);
    let v: Value = serde_json::from_str(text).unwrap();
    assert_eq!(v["frames"]["coin_copy"]["spriteSourceSize"]["x"], 1);
}

#[test]
fn generic_json_array_snapshot() {
    let out = export(
        &sample(Value::Null),
        "sheet",
        &generic(JsonFormat::Array, false),
        &ExistingFiles::new(),
    )
    .unwrap();
    let text = &files_map(&out)["sheet.json"];
    let named = |n: &str, body: &str| format!(r#"{{"filename":"{n}",{}"#, &body[1..]);
    let expected = format!(
        r#"{{"frames":[{},{},{}],"meta":{META_TP}}}"#,
        named("coin", COIN_TP),
        named("coin_copy", COIN_TP),
        named("hero", HERO_TP)
    );
    assert_eq!(text, &expected);
    let v: Value = serde_json::from_str(text).unwrap();
    assert_eq!(v["frames"].as_array().unwrap().len(), 3);
}

#[test]
fn generic_json_pretty_parses_to_the_same_value() {
    let compact = export(
        &sample(Value::Null),
        "sheet",
        &generic(JsonFormat::Hash, false),
        &ExistingFiles::new(),
    )
    .unwrap();
    let pretty = export(
        &sample(Value::Null),
        "sheet",
        &generic(JsonFormat::Hash, true),
        &ExistingFiles::new(),
    )
    .unwrap();
    let a: Value = serde_json::from_str(&files_map(&compact)["sheet.json"]).unwrap();
    let pretty_text = &files_map(&pretty)["sheet.json"];
    assert!(pretty_text.contains("\n  \"frames\": {"));
    let b: Value = serde_json::from_str(pretty_text).unwrap();
    assert_eq!(a, b);
}

#[test]
fn generic_json_without_trim_info_and_with_rotation() {
    let cfg = ExporterConfig::GenericJson(GenericJsonOptions {
        include_trim_info: false,
        pretty: false,
        ..GenericJsonOptions::default()
    });
    let out = export(&rotated_sample(), "s", &cfg, &ExistingFiles::new()).unwrap();
    let v: Value = serde_json::from_str(&files_map(&out)["s.json"]).unwrap();
    assert!(v["frames"]["coin"].get("trimmed").is_none());
    assert!(v["frames"]["coin"].get("sourceSize").is_none());
    // TexturePacker convention: frame w/h are the unrotated size.
    assert_eq!(
        v["frames"]["rot"]["frame"],
        json!({"x": 30, "y": 0, "w": 10, "h": 20})
    );
    assert_eq!(v["frames"]["rot"]["rotated"], true);
    assert_eq!(v["meta"]["image"], "s.png");
}

// ----------------------------------------------------------------- Paper2D

#[test]
fn paper2d_json_snapshot() {
    let cfg = ExporterConfig::Unreal(UnrealOptions::default());
    let out = export(&rotated_sample(), "chars", &cfg, &ExistingFiles::new()).unwrap();
    assert_eq!(paths(&out), vec!["chars.png", "chars.paper2dsprites"]);
    let v: Value = serde_json::from_str(&files_map(&out)["chars.paper2dsprites"]).unwrap();
    let coin = json!({"frame": {"x": 18, "y": 0, "w": 8, "h": 8}, "rotated": false, "trimmed": true,
        "spriteSourceSize": {"x": 1, "y": 1, "w": 8, "h": 8}, "sourceSize": {"w": 12, "h": 10},
        "pivot": {"x": 0.5, "y": 0.5}});
    let expected = json!({
        "frames": {
            "coin": coin,
            "coin_copy": coin,
            "hero": {"frame": {"x": 0, "y": 0, "w": 16, "h": 24}, "rotated": false, "trimmed": false,
                "spriteSourceSize": {"x": 0, "y": 0, "w": 16, "h": 24}, "sourceSize": {"w": 16, "h": 24},
                "pivot": {"x": 0.5, "y": 0.5}},
            "rot": {"frame": {"x": 30, "y": 0, "w": 10, "h": 20}, "rotated": true, "trimmed": false,
                "spriteSourceSize": {"x": 0, "y": 0, "w": 10, "h": 20}, "sourceSize": {"w": 10, "h": 20},
                "pivot": {"x": 0.5, "y": 0.5}}
        },
        "meta": {"app": "http://www.codeandweb.com/texturepacker", "version": "1.0", "target": "paper2d",
            "image": "chars.png", "format": "RGBA8888", "size": {"w": 64, "h": 32}, "scale": "1"}
    });
    assert_eq!(v, expected);

    let cfg = ExporterConfig::Unreal(UnrealOptions {
        file_extension: Paper2dExtension::Json,
        pivot: texopt_core::atlas::exporters::Pivot { x: 0.5, y: 1.0 },
    });
    let out = export(&sample(Value::Null), "chars", &cfg, &ExistingFiles::new()).unwrap();
    let v: Value = serde_json::from_str(&files_map(&out)["chars.json"]).unwrap();
    assert_eq!(v["frames"]["hero"]["pivot"], json!({"x": 0.5, "y": 1.0}));
}

// ------------------------------------------------------------------- Godot

fn godot(version: GodotVersion) -> ExporterConfig {
    ExporterConfig::Godot(GodotOptions {
        version,
        res_path: "res://atlases".into(),
        output_subfolder: "sprites".into(),
        filter_clip: true,
    })
}

#[test]
fn godot4_tres_snapshot() {
    let out = export(
        &sample(Value::Null),
        "sheet",
        &godot(GodotVersion::Godot4),
        &ExistingFiles::new(),
    )
    .unwrap();
    assert_eq!(
        paths(&out),
        vec![
            "sheet.png",
            "sprites/coin.tres",
            "sprites/coin_copy.tres",
            "sprites/hero.tres"
        ]
    );
    let f = files_map(&out);
    assert_eq!(
        f["sprites/coin.tres"],
        "[gd_resource type=\"AtlasTexture\" load_steps=2 format=3]\n\n\
         [ext_resource type=\"Texture2D\" path=\"res://atlases/sheet.png\" id=\"1\"]\n\n\
         [resource]\n\
         atlas = ExtResource(\"1\")\n\
         region = Rect2(18, 0, 8, 8)\n\
         margin = Rect2(1, 1, 4, 2)\n\
         filter_clip = true\n"
    );
    assert_eq!(
        f["sprites/hero.tres"],
        "[gd_resource type=\"AtlasTexture\" load_steps=2 format=3]\n\n\
         [ext_resource type=\"Texture2D\" path=\"res://atlases/sheet.png\" id=\"1\"]\n\n\
         [resource]\n\
         atlas = ExtResource(\"1\")\n\
         region = Rect2(0, 0, 16, 24)\n\
         filter_clip = true\n"
    );
    assert_eq!(f["sprites/coin_copy.tres"], f["sprites/coin.tres"]);
}

#[test]
fn godot3_tres_snapshot() {
    let cfg = ExporterConfig::Godot(GodotOptions {
        version: GodotVersion::Godot3,
        ..GodotOptions::default()
    });
    let out = export(&sample(Value::Null), "sheet", &cfg, &ExistingFiles::new()).unwrap();
    assert_eq!(
        paths(&out),
        vec!["sheet.png", "coin.tres", "coin_copy.tres", "hero.tres"]
    );
    let f = files_map(&out);
    assert_eq!(
        f["coin.tres"],
        "[gd_resource type=\"AtlasTexture\" load_steps=2 format=2]\n\n\
         [ext_resource path=\"res://sheet.png\" type=\"Texture\" id=1]\n\n\
         [resource]\n\
         atlas = ExtResource( 1 )\n\
         region = Rect2( 18, 0, 8, 8 )\n\
         margin = Rect2( 1, 1, 4, 2 )\n"
    );
    assert_eq!(
        f["hero.tres"],
        "[gd_resource type=\"AtlasTexture\" load_steps=2 format=2]\n\n\
         [ext_resource path=\"res://sheet.png\" type=\"Texture\" id=1]\n\n\
         [resource]\n\
         atlas = ExtResource( 1 )\n\
         region = Rect2( 0, 0, 16, 24 )\n"
    );
}

#[test]
fn godot_validates_paths() {
    let bad = ExporterConfig::Godot(GodotOptions {
        res_path: "C:/game".into(),
        ..GodotOptions::default()
    });
    assert_eq!(
        export(&sample(Value::Null), "s", &bad, &ExistingFiles::new())
            .unwrap_err()
            .params["param"],
        "resPath"
    );
    let bad = ExporterConfig::Godot(GodotOptions {
        output_subfolder: "../x".into(),
        ..GodotOptions::default()
    });
    assert_eq!(
        export(&sample(Value::Null), "s", &bad, &ExistingFiles::new())
            .unwrap_err()
            .params["param"],
        "outputSubfolder"
    );
}

// ------------------------------------------------------------------- Unity

const GUID: &str = "0123456789abcdef0123456789abcdef";

fn unity_state() -> Value {
    json!({"unity": {
        "pages": {"sheet.png": GUID},
        "sprites": {
            "coin": {"internalID": 21300000, "spriteID": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
            "coin_copy": {"internalID": -5, "spriteID": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
            "hero": {"internalID": 7766554433221100_i64, "spriteID": "cccccccccccccccccccccccccccccccc"}
        }
    }})
}

const UNITY_2022_SNAPSHOT: &str = "fileFormatVersion: 2
guid: 0123456789abcdef0123456789abcdef
TextureImporter:
  internalIDToNameTable: []
  externalObjects: {}
  serializedVersion: 12
  mipmaps:
    mipMapMode: 0
    enableMipMap: 0
    sRGBTexture: 1
    linearTexture: 0
    fadeOut: 0
    borderMipMap: 0
    mipMapsPreserveCoverage: 0
    alphaTestReferenceValue: 0.5
    mipMapFadeDistanceStart: 1
    mipMapFadeDistanceEnd: 3
  bumpmap:
    convertToNormalMap: 0
    externalNormalMap: 0
    heightScale: 0.25
    normalMapFilter: 0
    flipGreenChannel: 0
  isReadable: 0
  streamingMipmaps: 0
  streamingMipmapsPriority: 0
  vTOnly: 0
  ignoreMipmapLimit: 0
  grayScaleToAlpha: 0
  generateCubemap: 6
  cubemapConvolution: 0
  seamlessCubemap: 0
  textureFormat: 1
  maxTextureSize: 64
  textureSettings:
    serializedVersion: 2
    filterMode: 0
    aniso: 1
    mipBias: 0
    wrapU: 1
    wrapV: 1
    wrapW: 1
  nPOTScale: 0
  lightmap: 0
  compressionQuality: 50
  spriteMode: 2
  spriteExtrude: 1
  spriteMeshType: 1
  alignment: 7
  spritePivot: {x: 0.5, y: 0}
  spritePixelsToUnits: 32
  spriteBorder: {x: 0, y: 0, z: 0, w: 0}
  spriteGenerateFallbackPhysicsShape: 1
  alphaUsage: 1
  alphaIsTransparency: 1
  spriteTessellationDetail: -1
  textureType: 8
  textureShape: 1
  singleChannelComponent: 0
  flipbookRows: 1
  flipbookColumns: 1
  maxTextureSizeSet: 0
  compressionQualitySet: 0
  textureFormatSet: 0
  ignorePngGamma: 0
  applyGammaDecoding: 0
  swizzle: 50462976
  cookieLightType: 0
  platformSettings:
  - serializedVersion: 3
    buildTarget: DefaultTexturePlatform
    maxTextureSize: 64
    resizeAlgorithm: 0
    textureFormat: -1
    textureCompression: 0
    compressionQuality: 50
    crunchedCompression: 0
    allowsAlphaSplitting: 0
    overridden: 0
    ignorePlatformSupport: 0
    androidETC2FallbackOverride: 0
    forceMaximumCompressionQuality_BC6H_BC7: 0
  spriteSheet:
    serializedVersion: 2
    sprites:
    - serializedVersion: 2
      name: coin
      rect:
        serializedVersion: 2
        x: 18
        y: 24
        width: 8
        height: 8
      alignment: 9
      pivot: {x: 0.625, y: -0.125}
      border: {x: 0, y: 0, z: 0, w: 0}
      customData:
      outline: []
      physicsShape: []
      tessellationDetail: 0
      bones: []
      spriteID: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
      internalID: 21300000
      vertices: []
      indices:
      edges: []
      weights: []
    - serializedVersion: 2
      name: coin_copy
      rect:
        serializedVersion: 2
        x: 18
        y: 24
        width: 8
        height: 8
      alignment: 9
      pivot: {x: 0.625, y: -0.125}
      border: {x: 0, y: 0, z: 0, w: 0}
      customData:
      outline: []
      physicsShape: []
      tessellationDetail: 0
      bones: []
      spriteID: bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
      internalID: -5
      vertices: []
      indices:
      edges: []
      weights: []
    - serializedVersion: 2
      name: hero
      rect:
        serializedVersion: 2
        x: 0
        y: 8
        width: 16
        height: 24
      alignment: 7
      pivot: {x: 0.5, y: 0}
      border: {x: 0, y: 0, z: 0, w: 0}
      customData:
      outline: []
      physicsShape: []
      tessellationDetail: 0
      bones: []
      spriteID: cccccccccccccccccccccccccccccccc
      internalID: 7766554433221100
      vertices: []
      indices:
      edges: []
      weights: []
    outline: []
    customData:
    physicsShape: []
    bones: []
    spriteID:
    internalID: 0
    vertices: []
    indices:
    edges: []
    weights: []
    secondaryTextures: []
    spriteCustomMetadata:
      entries: []
    nameFileIdTable:
      coin: 21300000
      coin_copy: -5
      hero: 7766554433221100
  mipmapLimitGroupName:
  pSDRemoveMatte: 0
  userData:
  assetBundleName:
  assetBundleVariant:
";

fn unity_opts() -> UnityOptions {
    UnityOptions {
        unity_version: UnityVersion::Unity2022,
        pixels_per_unit: 32.0,
        filter_mode: texopt_core::atlas::exporters::UnityFilterMode::Point,
        texture_compression: texopt_core::atlas::exporters::UnityCompression::None,
        pivot: UnityPivot::BottomCenter,
        ..UnityOptions::default()
    }
}

#[test]
fn unity_meta_snapshot_with_y_flip_and_preserved_ids() {
    let cfg = ExporterConfig::Unity(unity_opts());
    let out = export(&sample(unity_state()), "sheet", &cfg, &ExistingFiles::new()).unwrap();
    assert_eq!(paths(&out), vec!["sheet.png", "sheet.png.meta"]);
    let meta = &files_map(&out)["sheet.png.meta"];
    assert_eq!(meta, UNITY_2022_SNAPSHOT);
    assert_eq!(out.exporter_state, unity_state());
}

fn table(meta: &str) -> BTreeMap<String, i64> {
    let mut out = BTreeMap::new();
    let mut lines = meta.lines().skip_while(|l| l.trim() != "nameFileIdTable:");
    lines.next();
    for l in lines {
        if !l.starts_with("      ") {
            break;
        }
        let (k, v) = l.trim().split_once(": ").unwrap();
        out.insert(k.to_string(), v.parse().unwrap());
    }
    out
}

fn list_ids(meta: &str) -> BTreeMap<String, (i64, String)> {
    let mut out = BTreeMap::new();
    let mut name = None;
    let mut sid = String::new();
    for l in meta.lines() {
        if let Some(n) = l.strip_prefix("      name: ") {
            name = Some(n.to_string());
        } else if let Some(s) = l.strip_prefix("      spriteID: ") {
            sid = s.to_string();
        } else if let Some(i) = l.strip_prefix("      internalID: ") {
            out.insert(name.take().unwrap(), (i.parse().unwrap(), sid.clone()));
        }
    }
    out
}

fn is_hex32(s: &str) -> bool {
    s.len() == 32
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

#[test]
fn unity_generates_new_ids_without_existing_meta() {
    let cfg = ExporterConfig::Unity(UnityOptions::default());
    let a = export(&sample(Value::Null), "sheet", &cfg, &ExistingFiles::new()).unwrap();
    let b = export(&sample(Value::Null), "sheet", &cfg, &ExistingFiles::new()).unwrap();
    let ma = &files_map(&a)["sheet.png.meta"];
    let mb = &files_map(&b)["sheet.png.meta"];
    let ga = parse_meta(ma).guid.unwrap();
    assert!(is_hex32(&ga));
    assert_ne!(ga, parse_meta(mb).guid.unwrap());

    let ids = list_ids(ma);
    assert_eq!(
        ids.keys().cloned().collect::<Vec<_>>(),
        vec!["coin", "coin_copy", "hero"]
    );
    let mut seen = std::collections::BTreeSet::new();
    for (internal, sid) in ids.values() {
        assert_ne!(*internal, 0);
        assert!(seen.insert(*internal), "duplicate internalID");
        assert!(is_hex32(sid));
    }
    // nameFileIdTable matches the sprite list.
    let t = table(ma);
    assert_eq!(
        t,
        ids.iter()
            .map(|(k, v)| (k.clone(), v.0))
            .collect::<BTreeMap<_, _>>()
    );
    // State records the new IDs.
    assert_eq!(a.exporter_state["unity"]["pages"]["sheet.png"], ga);
    assert_eq!(
        a.exporter_state["unity"]["sprites"]["hero"]["internalID"],
        ids["hero"].0
    );
}

/// A meta as Unity writes it (abridged), with ids that must survive.
const EXISTING_META: &str = "fileFormatVersion: 2
guid: fedcba9876543210fedcba9876543210
TextureImporter:
  internalIDToNameTable: []
  serializedVersion: 12
  spriteMode: 2
  spriteSheet:
    serializedVersion: 2
    sprites:
    - serializedVersion: 2
      name: hero
      rect:
        serializedVersion: 2
        x: 0
        y: 0
        width: 10
        height: 10
      alignment: 0
      pivot: {x: 0.5, y: 0.5}
      outline:
      - - {x: 1, y: 2}
      spriteID: 1111111111111111aaaaaaaaaaaaaaaa
      internalID: -987654321012345678
      vertices: []
    - serializedVersion: 2
      name: 'removed: sprite'
      rect:
        serializedVersion: 2
        x: 0
        y: 0
        width: 1
        height: 1
      spriteID: 22222222222222222222222222222222
      internalID: 42
    outline: []
    spriteID:
    internalID: 0
    nameFileIdTable:
      hero: -987654321012345678
      'removed: sprite': 42
      coin: 1234
  userData:
";

#[test]
fn unity_preserves_guid_and_ids_from_existing_meta() {
    let info = parse_meta(EXISTING_META);
    assert_eq!(
        info.guid.as_deref(),
        Some("fedcba9876543210fedcba9876543210")
    );
    assert_eq!(info.sprites["hero"].internal_id, -987654321012345678);
    assert_eq!(info.sprites["removed: sprite"].internal_id, 42);
    assert_eq!(info.sprites["coin"].internal_id, 1234);

    let cfg = ExporterConfig::Unity(UnityOptions::default());
    assert_eq!(files_to_read("sheet", 1, &cfg), vec!["sheet.png.meta"]);
    let existing = ExistingFiles::new().with("sheet.png.meta", EXISTING_META);
    // Existing meta wins over stale exporter state.
    let out = export(&sample(unity_state()), "sheet", &cfg, &existing).unwrap();
    let meta = &files_map(&out)["sheet.png.meta"];
    assert_eq!(
        parse_meta(meta).guid.as_deref(),
        Some("fedcba9876543210fedcba9876543210")
    );
    let ids = list_ids(meta);
    assert_eq!(
        ids["hero"],
        (
            -987654321012345678,
            "1111111111111111aaaaaaaaaaaaaaaa".to_string()
        )
    );
    // coin: internalID from the table; spriteID unknown in the meta -> taken from state only if the id matches,
    // otherwise freshly generated.
    assert_eq!(ids["coin"].0, 1234);
    assert!(is_hex32(&ids["coin"].1));
    // coin_copy is not in the meta -> falls back to the exporter state.
    assert_eq!(
        ids["coin_copy"],
        (-5, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string())
    );
    assert!(!meta.contains("removed"));
    assert_eq!(
        table(meta),
        ids.iter()
            .map(|(k, v)| (k.clone(), v.0))
            .collect::<BTreeMap<_, _>>()
    );

    // Re-exporting with the produced meta is byte-identical.
    let again = export(
        &sample(Value::Null),
        "sheet",
        &cfg,
        &ExistingFiles::new().with("sheet.png.meta", meta.clone()),
    )
    .unwrap();
    assert_eq!(&files_map(&again)["sheet.png.meta"], meta);
}

#[test]
fn unity_ids_survive_repack_optimal_through_exporter_state() {
    let params = AtlasParams {
        padding: 1,
        ..AtlasParams::default()
    };
    let inputs: Vec<(String, ImageBuf)> = vec![
        ("a".into(), pattern(10, 20, 1)),
        ("b".into(), pattern(30, 8, 2)),
        ("c".into(), pattern(5, 5, 3)),
    ];
    let v1 = build(
        to_inputs(&inputs),
        &params,
        None,
        IncrementalMode::KeepPositions,
    )
    .unwrap();
    let cfg = ExporterConfig::Unity(UnityOptions::default());
    let out1 = export(&v1, "atlas", &cfg, &ExistingFiles::new()).unwrap();
    let mut project = v1.project.clone();
    project.exporter_state = out1.exporter_state.clone();
    // Save/load the project like the app does.
    let project = AtlasProject::from_json(&project.to_json()).unwrap();

    let mut changed = inputs.clone();
    changed[2].1 = pattern(40, 40, 9);
    let v2 = build(
        to_inputs(&changed),
        &params,
        Some(&project),
        IncrementalMode::RepackOptimal,
    )
    .unwrap();
    // No existing meta supplied: ids come from the saved state.
    let out2 = export(&v2, "atlas", &cfg, &ExistingFiles::new()).unwrap();
    let m1 = &files_map(&out1)["atlas.png.meta"];
    let m2 = &files_map(&out2)["atlas.png.meta"];
    assert_eq!(parse_meta(m1).guid, parse_meta(m2).guid);
    assert_eq!(list_ids(m1), list_ids(m2));
}

#[test]
fn unity_multi_page_writes_one_meta_per_page() {
    let mut r = sample(Value::Null);
    r.project.pages.push(PageInfo {
        width: 32,
        height: 32,
    });
    r.project.sprites.push(ProjectSprite {
        page: 1,
        ..sprite("p1", [0, 0, 4, 4], [4, 4], [0, 0, 4, 4], false, &[])
    });
    r.pages.push(ImageBuf::new(32, 32));
    r.project.validate().unwrap();
    let cfg = ExporterConfig::Unity(UnityOptions::default());
    let out = export(&r, "sheet", &cfg, &ExistingFiles::new()).unwrap();
    assert_eq!(
        paths(&out),
        vec![
            "sheet_0.png",
            "sheet_1.png",
            "sheet_0.png.meta",
            "sheet_1.png.meta"
        ]
    );
    assert_eq!(page_file_name("sheet", 1, 2), "sheet_1.png");
    let f = files_map(&out);
    let p1 = list_ids(&f["sheet_1.png.meta"]);
    assert_eq!(p1.keys().cloned().collect::<Vec<_>>(), vec!["p1"]);
    assert!(f["sheet_1.png.meta"].contains("        y: 28\n"));
    assert_ne!(
        parse_meta(&f["sheet_0.png.meta"]).guid,
        parse_meta(&f["sheet_1.png.meta"]).guid
    );

    // JSON exporter: one document per page.
    let out = export(
        &r,
        "sheet",
        &generic(JsonFormat::Hash, false),
        &ExistingFiles::new(),
    )
    .unwrap();
    assert_eq!(
        paths(&out),
        vec!["sheet_0.png", "sheet_1.png", "sheet_0.json", "sheet_1.json"]
    );
}

#[test]
fn unity_versions_and_options() {
    let r = sample(unity_state());
    let opts = |v| {
        ExporterConfig::Unity(UnityOptions {
            unity_version: v,
            mipmaps: true,
            ..UnityOptions::default()
        })
    };
    let m2021 = files_map(
        &export(
            &r,
            "sheet",
            &opts(UnityVersion::Unity2021),
            &ExistingFiles::new(),
        )
        .unwrap(),
    )
    .remove("sheet.png.meta")
    .unwrap();
    assert!(m2021.contains("  serializedVersion: 11\n"));
    assert!(!m2021.contains("swizzle"));
    let m6 = files_map(
        &export(
            &r,
            "sheet",
            &opts(UnityVersion::Unity6),
            &ExistingFiles::new(),
        )
        .unwrap(),
    )
    .remove("sheet.png.meta")
    .unwrap();
    assert!(m6.contains("  serializedVersion: 13\n"));
    assert!(m6.contains("    enableMipMap: 1\n"));
    assert!(m6.contains("  spritePixelsToUnits: 100\n"));
    assert!(m6.contains("    filterMode: 1\n"));
    assert!(m6.contains("    textureCompression: 1\n"));
    // Untrimmed sprite keeps the preset alignment, trimmed one gets a compensated custom pivot.
    let custom = ExporterConfig::Unity(UnityOptions {
        pivot: UnityPivot::Custom,
        custom_pivot: texopt_core::atlas::exporters::Pivot { x: 0.25, y: 0.75 },
        preserve_pivot_on_trim: false,
        ..UnityOptions::default()
    });
    let m = files_map(&export(&r, "sheet", &custom, &ExistingFiles::new()).unwrap())
        .remove("sheet.png.meta")
        .unwrap();
    assert!(m.contains("      alignment: 9\n      pivot: {x: 0.25, y: 0.75}\n"));

    let bad = ExporterConfig::Unity(UnityOptions {
        max_texture_size: 100,
        ..UnityOptions::default()
    });
    assert_eq!(
        export(&r, "sheet", &bad, &ExistingFiles::new())
            .unwrap_err()
            .params["param"],
        "maxTextureSize"
    );
}

#[test]
fn unity_quotes_unsafe_names() {
    let mut r = sample(Value::Null);
    r.project.sprites[1].name = "hero: idle #1".into();
    r.project.sprites.sort_by(|a, b| a.name.cmp(&b.name));
    let cfg = ExporterConfig::Unity(UnityOptions::default());
    let out = export(&r, "sheet", &cfg, &ExistingFiles::new()).unwrap();
    let meta = &files_map(&out)["sheet.png.meta"];
    assert!(meta.contains("      name: 'hero: idle #1'\n"));
    let info = parse_meta(meta);
    assert!(info.sprites.contains_key("hero: idle #1"));
}

// -------------------------------------------------------- capabilities

#[test]
fn rotation_is_rejected_by_unity_and_godot() {
    let r = rotated_sample();
    for cfg in [
        ExporterConfig::Unity(UnityOptions::default()),
        ExporterConfig::Godot(GodotOptions::default()),
    ] {
        assert!(!cfg.supports_rotation());
        let err = export(&r, "s", &cfg, &ExistingFiles::new()).unwrap_err();
        assert_eq!(err.code, ATLAS_EXPORTER_UNSUPPORTED);
        assert_eq!(err.params["feature"], "rotation");
        assert_eq!(err.params["exporter"], cfg.id());
    }
    for cfg in [
        ExporterConfig::GenericJson(GenericJsonOptions::default()),
        ExporterConfig::Unreal(UnrealOptions::default()),
    ] {
        assert!(cfg.supports_rotation() && cfg.supports_multipage() && cfg.supports_trim());
        export(&r, "s", &cfg, &ExistingFiles::new()).unwrap();
    }
}

#[test]
fn adapt_params_disables_rotation_for_unity() {
    let params = AtlasParams {
        allow_rotation: true,
        ..AtlasParams::default()
    };
    let (p, warnings) = adapt_params(&params, &ExporterConfig::Unity(UnityOptions::default()));
    assert!(!p.allow_rotation);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].code, ATLAS_FEATURE_DISABLED);
    assert_eq!(warnings[0].params["feature"], "rotation");
    let (p, warnings) = adapt_params(&params, &ExporterConfig::Unreal(UnrealOptions::default()));
    assert!(p.allow_rotation && warnings.is_empty());

    // End to end: adapted params never produce rotated sprites, so Godot export works.
    let inputs = vec![("tall".to_string(), pattern(4, 60, 1))];
    let small = AtlasParams {
        max_width: 64,
        max_height: 64,
        allow_rotation: true,
        ..AtlasParams::default()
    };
    let cfg = ExporterConfig::Godot(GodotOptions::default());
    let (adapted, _) = adapt_params(&small, &cfg);
    let res = build(
        to_inputs(&inputs),
        &adapted,
        None,
        IncrementalMode::KeepPositions,
    )
    .unwrap();
    export(&res, "s", &cfg, &ExistingFiles::new()).unwrap();
}

#[test]
fn exporter_config_json_shape() {
    let cfg: ExporterConfig = serde_json::from_str(
        r#"{"kind":"unity","options":{"pixelsPerUnit":64,"pivot":"bottomCenter"}}"#,
    )
    .unwrap();
    match &cfg {
        ExporterConfig::Unity(o) => {
            assert_eq!(o.pixels_per_unit, 64.0);
            assert_eq!(o.pivot, UnityPivot::BottomCenter);
            assert_eq!(o.max_texture_size, 0);
        }
        _ => panic!("wrong kind"),
    }
    let v =
        serde_json::to_value(ExporterConfig::GenericJson(GenericJsonOptions::default())).unwrap();
    assert_eq!(
        v,
        json!({"kind": "genericJson", "options": {"format": "hash", "imagePathPrefix": "", "includeTrimInfo": true,
            "pretty": true, "pivot": {"x": 0.5, "y": 0.5}}})
    );
    let v = serde_json::to_value(ExporterConfig::Godot(GodotOptions::default())).unwrap();
    assert_eq!(
        v,
        json!({"kind": "godot", "options": {"version": "godot4", "resPath": "res://", "outputSubfolder": "",
            "filterClip": false}})
    );
    let v = serde_json::to_value(ExporterConfig::Unreal(UnrealOptions::default())).unwrap();
    assert_eq!(
        v,
        json!({"kind": "unreal", "options": {"pivot": {"x": 0.5, "y": 0.5}, "fileExtension": "paper2dsprites"}})
    );
}

#[test]
fn export_rejects_bad_base_name_and_writes_valid_png() {
    let cfg = ExporterConfig::GenericJson(GenericJsonOptions::default());
    let err = export(&sample(Value::Null), "a/b", &cfg, &ExistingFiles::new()).unwrap_err();
    assert_eq!(err.params["param"], "baseName");
    let out = export(&sample(Value::Null), "sheet", &cfg, &ExistingFiles::new()).unwrap();
    let png = image::load_from_memory(&out.files[0].1).unwrap();
    assert_eq!((png.width(), png.height()), (64, 32));
}
