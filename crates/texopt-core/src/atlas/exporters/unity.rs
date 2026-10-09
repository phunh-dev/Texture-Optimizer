//! Unity: a `TextureImporter` `.png.meta` per page with `spriteMode: 2`
//! (Multiple) and one sprite per name.
//!
//! IDs are kept stable so prefab/scene references survive re-exports:
//! the page `guid` and each sprite's `internalID`/`spriteID` are taken from
//! the existing `.meta` (if supplied), else from the exporter state saved in
//! the project, else freshly generated.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{ExistingFiles, Pivot, page_file_name, page_frames};
use crate::atlas::codes;
use crate::atlas::incremental::{AtlasProject, ProjectSprite};
use crate::{OpError, OpResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum UnityVersion {
    /// 2021.3 LTS (TextureImporter serializedVersion 11).
    Unity2021,
    /// 2022.3 LTS (serializedVersion 12).
    #[default]
    Unity2022,
    /// Unity 6 / 6000.x (serializedVersion 13).
    Unity6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum UnityFilterMode {
    Point,
    #[default]
    Bilinear,
    Trilinear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum UnityCompression {
    None,
    LowQuality,
    #[default]
    NormalQuality,
    HighQuality,
}

/// Sprite alignment; `custom` uses `customPivot`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum UnityPivot {
    #[default]
    Center,
    TopLeft,
    TopCenter,
    TopRight,
    LeftCenter,
    RightCenter,
    BottomLeft,
    BottomCenter,
    BottomRight,
    Custom,
}

impl UnityPivot {
    /// Unity `alignment` value and normalised pivot (0,0 = bottom-left).
    fn resolve(self, custom: Pivot) -> (u8, f64, f64) {
        match self {
            UnityPivot::Center => (0, 0.5, 0.5),
            UnityPivot::TopLeft => (1, 0.0, 1.0),
            UnityPivot::TopCenter => (2, 0.5, 1.0),
            UnityPivot::TopRight => (3, 1.0, 1.0),
            UnityPivot::LeftCenter => (4, 0.0, 0.5),
            UnityPivot::RightCenter => (5, 1.0, 0.5),
            UnityPivot::BottomLeft => (6, 0.0, 0.0),
            UnityPivot::BottomCenter => (7, 0.5, 0.0),
            UnityPivot::BottomRight => (8, 1.0, 0.0),
            UnityPivot::Custom => (9, custom.x, custom.y),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UnityOptions {
    pub unity_version: UnityVersion,
    pub pixels_per_unit: f64,
    pub filter_mode: UnityFilterMode,
    pub texture_compression: UnityCompression,
    /// 0 = smallest Unity size (32..16384) that holds the page; otherwise a power of two in that range.
    pub max_texture_size: u32,
    pub mipmaps: bool,
    pub pivot: UnityPivot,
    /// Used when `pivot` is `custom`; Unity convention (0,0 = bottom-left).
    pub custom_pivot: Pivot,
    /// For trimmed sprites, move the pivot so the sprite renders at the same
    /// place as the untrimmed original (alignment becomes Custom).
    pub preserve_pivot_on_trim: bool,
}

impl Default for UnityOptions {
    fn default() -> Self {
        Self {
            unity_version: UnityVersion::Unity2022,
            pixels_per_unit: 100.0,
            filter_mode: UnityFilterMode::Bilinear,
            texture_compression: UnityCompression::NormalQuality,
            max_texture_size: 0,
            mipmaps: false,
            pivot: UnityPivot::Center,
            custom_pivot: Pivot::default(),
            preserve_pivot_on_trim: true,
        }
    }
}

/// IDs of one sprite inside a texture meta.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpriteIds {
    pub internal_id: i64,
    pub sprite_id: Option<String>,
}

/// What [`parse_meta`] extracts from an existing `.meta`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnityMetaInfo {
    pub guid: Option<String>,
    pub sprites: BTreeMap<String, SpriteIds>,
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2 && v.starts_with('\'') && v.ends_with('\'') {
        return v[1..v.len() - 1].replace("''", "'");
    }
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        return v[1..v.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");
    }
    v.to_string()
}

/// Split `key: value` where the key may be quoted.
fn split_kv(t: &str) -> Option<(String, String)> {
    let t = t.trim();
    if let Some(q) = t.chars().next().filter(|c| *c == '\'' || *c == '"') {
        let mut i = 1;
        let bytes = t.as_bytes();
        while i < bytes.len() {
            if bytes[i] as char == q {
                if q == '\'' && bytes.get(i + 1) == Some(&b'\'') {
                    i += 2;
                    continue;
                }
                if q == '"' && bytes[i - 1] == b'\\' {
                    i += 1;
                    continue;
                }
                break;
            }
            i += 1;
        }
        let key = unquote(&t[..=i.min(t.len() - 1)]);
        let rest = t.get(i + 1..)?.trim_start().strip_prefix(':')?;
        return Some((key, rest.trim().to_string()));
    }
    let (k, v) = t.split_once(':')?;
    Some((k.trim().to_string(), v.trim().to_string()))
}

/// Extract the guid and the per-sprite IDs from a TextureImporter `.meta`.
pub fn parse_meta(text: &str) -> UnityMetaInfo {
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
    let mut info = UnityMetaInfo::default();
    for l in &lines {
        if let Some(v) = l.strip_prefix("guid:") {
            let g = v.trim().to_lowercase();
            if g.len() == 32 && g.chars().all(|c| c.is_ascii_hexdigit()) {
                info.guid = Some(g);
            }
            break;
        }
    }
    // spriteSheet.sprites list
    if let Some(start) = lines.iter().position(|l| l.trim() == "sprites:") {
        let key_indent = indent_of(lines[start]);
        let field_indent = key_indent + 2;
        let mut cur: Option<(Option<String>, SpriteIds)> = None;
        let flush = |cur: &mut Option<(Option<String>, SpriteIds)>, info: &mut UnityMetaInfo| {
            if let Some((Some(name), ids)) = cur.take() {
                info.sprites.insert(name, ids);
            }
        };
        for l in &lines[start + 1..] {
            if l.trim().is_empty() {
                continue;
            }
            let ind = indent_of(l);
            let t = l.trim();
            if ind < key_indent || (ind == key_indent && !t.starts_with("- ")) {
                break;
            }
            let field = if ind == key_indent {
                flush(&mut cur, &mut info);
                cur = Some((None, SpriteIds::default()));
                &t[2..]
            } else if ind == field_indent {
                t
            } else {
                continue;
            };
            let (Some(c), Some((k, v))) = (cur.as_mut(), split_kv(field)) else {
                continue;
            };
            match k.as_str() {
                "name" => c.0 = Some(unquote(&v)),
                "internalID" => c.1.internal_id = v.parse().unwrap_or(0),
                "spriteID" => {
                    let id = unquote(&v);
                    c.1.sprite_id = (!id.is_empty()).then_some(id);
                }
                _ => {}
            }
        }
        flush(&mut cur, &mut info);
    }
    // nameFileIdTable fills internalIDs of sprites missing from the list.
    if let Some(start) = lines.iter().position(|l| l.trim() == "nameFileIdTable:") {
        let key_indent = indent_of(lines[start]);
        for l in &lines[start + 1..] {
            if l.trim().is_empty() {
                continue;
            }
            if indent_of(l) <= key_indent {
                break;
            }
            if let Some((k, v)) = split_kv(l)
                && let Ok(id) = v.parse::<i64>()
            {
                info.sprites.entry(k).or_insert(SpriteIds {
                    internal_id: id,
                    sprite_id: None,
                });
            }
        }
    }
    info
}

/// YAML plain scalar when safe, single-quoted otherwise.
fn yaml_str(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let needs_quote = s.is_empty()
        || s.trim() != s
        || s.starts_with([
            '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%',
            '@', '`',
        ])
        || s.contains(": ")
        || s.contains(" #")
        || s.ends_with(':')
        || s.contains(['\n', '\r', '\t'])
        || matches!(
            lower.as_str(),
            "true" | "false" | "yes" | "no" | "on" | "off" | "null" | "~" | "y" | "n"
        )
        || s.parse::<f64>().is_ok();
    if needs_quote {
        format!("'{}'", s.replace('\'', "''"))
    } else {
        s.to_string()
    }
}

fn fmt_f(v: f64) -> String {
    if v == 0.0 { "0".into() } else { format!("{v}") }
}

fn new_guid() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn new_internal_id() -> i64 {
    loop {
        let b = uuid::Uuid::new_v4().into_bytes();
        let id = i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
        if id != 0 {
            return id;
        }
    }
}

fn auto_max_size(w: u32, h: u32) -> u32 {
    let m = w.max(h).max(32);
    m.next_power_of_two().min(16384)
}

struct SpriteEntry<'a> {
    name: &'a str,
    sprite: &'a ProjectSprite,
    ids: SpriteIds,
}

fn sprite_pivot(o: &UnityOptions, s: &ProjectSprite) -> (u8, f64, f64) {
    let (align, px, py) = o.pivot.resolve(o.custom_pivot);
    if !(s.trimmed && o.preserve_pivot_on_trim) {
        return (align, px, py);
    }
    let ss = &s.sprite_source_size;
    let (sw, sh) = (f64::from(s.source_size.w), f64::from(s.source_size.h));
    let ox = f64::from(ss.x);
    let oy = sh - f64::from(ss.y) - f64::from(ss.h);
    (
        9,
        (px * sw - ox) / f64::from(ss.w),
        (py * sh - oy) / f64::from(ss.h),
    )
}

fn write_meta(
    o: &UnityOptions,
    guid: &str,
    page_w: u32,
    page_h: u32,
    entries: &[SpriteEntry],
) -> String {
    let v = o.unity_version;
    let modern = v != UnityVersion::Unity2021;
    let serialized = match v {
        UnityVersion::Unity2021 => 11,
        UnityVersion::Unity2022 => 12,
        UnityVersion::Unity6 => 13,
    };
    let filter = match o.filter_mode {
        UnityFilterMode::Point => 0,
        UnityFilterMode::Bilinear => 1,
        UnityFilterMode::Trilinear => 2,
    };
    let compression = match o.texture_compression {
        UnityCompression::None => 0,
        UnityCompression::NormalQuality => 1,
        UnityCompression::HighQuality => 2,
        UnityCompression::LowQuality => 3,
    };
    let max_size = if o.max_texture_size == 0 {
        auto_max_size(page_w, page_h)
    } else {
        o.max_texture_size
    };
    let (align, px, py) = o.pivot.resolve(o.custom_pivot);
    let mut m = String::new();
    let w = &mut m;
    let _ = writeln!(w, "fileFormatVersion: 2");
    let _ = writeln!(w, "guid: {guid}");
    let _ = writeln!(w, "TextureImporter:");
    let _ = writeln!(w, "  internalIDToNameTable: []");
    let _ = writeln!(w, "  externalObjects: {{}}");
    let _ = writeln!(w, "  serializedVersion: {serialized}");
    let _ = writeln!(w, "  mipmaps:");
    let _ = writeln!(w, "    mipMapMode: 0");
    let _ = writeln!(w, "    enableMipMap: {}", u8::from(o.mipmaps));
    let _ = writeln!(w, "    sRGBTexture: 1");
    let _ = writeln!(w, "    linearTexture: 0");
    let _ = writeln!(w, "    fadeOut: 0");
    let _ = writeln!(w, "    borderMipMap: 0");
    let _ = writeln!(w, "    mipMapsPreserveCoverage: 0");
    let _ = writeln!(w, "    alphaTestReferenceValue: 0.5");
    let _ = writeln!(w, "    mipMapFadeDistanceStart: 1");
    let _ = writeln!(w, "    mipMapFadeDistanceEnd: 3");
    let _ = writeln!(w, "  bumpmap:");
    let _ = writeln!(w, "    convertToNormalMap: 0");
    let _ = writeln!(w, "    externalNormalMap: 0");
    let _ = writeln!(w, "    heightScale: 0.25");
    let _ = writeln!(w, "    normalMapFilter: 0");
    if modern {
        let _ = writeln!(w, "    flipGreenChannel: 0");
    }
    let _ = writeln!(w, "  isReadable: 0");
    let _ = writeln!(w, "  streamingMipmaps: 0");
    let _ = writeln!(w, "  streamingMipmapsPriority: 0");
    let _ = writeln!(w, "  vTOnly: 0");
    if modern {
        let _ = writeln!(w, "  ignoreMipmapLimit: 0");
    }
    let _ = writeln!(w, "  grayScaleToAlpha: 0");
    let _ = writeln!(w, "  generateCubemap: 6");
    let _ = writeln!(w, "  cubemapConvolution: 0");
    let _ = writeln!(w, "  seamlessCubemap: 0");
    let _ = writeln!(w, "  textureFormat: 1");
    let _ = writeln!(w, "  maxTextureSize: {max_size}");
    let _ = writeln!(w, "  textureSettings:");
    let _ = writeln!(w, "    serializedVersion: 2");
    let _ = writeln!(w, "    filterMode: {filter}");
    let _ = writeln!(w, "    aniso: 1");
    let _ = writeln!(w, "    mipBias: 0");
    let _ = writeln!(w, "    wrapU: 1");
    let _ = writeln!(w, "    wrapV: 1");
    let _ = writeln!(w, "    wrapW: 1");
    let _ = writeln!(w, "  nPOTScale: 0");
    let _ = writeln!(w, "  lightmap: 0");
    let _ = writeln!(w, "  compressionQuality: 50");
    let _ = writeln!(w, "  spriteMode: 2");
    let _ = writeln!(w, "  spriteExtrude: 1");
    let _ = writeln!(w, "  spriteMeshType: 1");
    let _ = writeln!(w, "  alignment: {align}");
    let _ = writeln!(w, "  spritePivot: {{x: {}, y: {}}}", fmt_f(px), fmt_f(py));
    let _ = writeln!(w, "  spritePixelsToUnits: {}", fmt_f(o.pixels_per_unit));
    let _ = writeln!(w, "  spriteBorder: {{x: 0, y: 0, z: 0, w: 0}}");
    let _ = writeln!(w, "  spriteGenerateFallbackPhysicsShape: 1");
    let _ = writeln!(w, "  alphaUsage: 1");
    let _ = writeln!(w, "  alphaIsTransparency: 1");
    let _ = writeln!(w, "  spriteTessellationDetail: -1");
    let _ = writeln!(w, "  textureType: 8");
    let _ = writeln!(w, "  textureShape: 1");
    let _ = writeln!(w, "  singleChannelComponent: 0");
    let _ = writeln!(w, "  flipbookRows: 1");
    let _ = writeln!(w, "  flipbookColumns: 1");
    let _ = writeln!(w, "  maxTextureSizeSet: 0");
    let _ = writeln!(w, "  compressionQualitySet: 0");
    let _ = writeln!(w, "  textureFormatSet: 0");
    let _ = writeln!(w, "  ignorePngGamma: 0");
    let _ = writeln!(w, "  applyGammaDecoding: 0");
    if modern {
        let _ = writeln!(w, "  swizzle: 50462976");
    }
    let _ = writeln!(w, "  cookieLightType: 0");
    let _ = writeln!(w, "  platformSettings:");
    let _ = writeln!(w, "  - serializedVersion: 3");
    let _ = writeln!(w, "    buildTarget: DefaultTexturePlatform");
    let _ = writeln!(w, "    maxTextureSize: {max_size}");
    let _ = writeln!(w, "    resizeAlgorithm: 0");
    let _ = writeln!(w, "    textureFormat: -1");
    let _ = writeln!(w, "    textureCompression: {compression}");
    let _ = writeln!(w, "    compressionQuality: 50");
    let _ = writeln!(w, "    crunchedCompression: 0");
    let _ = writeln!(w, "    allowsAlphaSplitting: 0");
    let _ = writeln!(w, "    overridden: 0");
    if modern {
        let _ = writeln!(w, "    ignorePlatformSupport: 0");
    }
    let _ = writeln!(w, "    androidETC2FallbackOverride: 0");
    let _ = writeln!(w, "    forceMaximumCompressionQuality_BC6H_BC7: 0");
    let _ = writeln!(w, "  spriteSheet:");
    let _ = writeln!(w, "    serializedVersion: 2");
    if entries.is_empty() {
        let _ = writeln!(w, "    sprites: []");
    } else {
        let _ = writeln!(w, "    sprites:");
    }
    for e in entries {
        let f = &e.sprite.frame;
        let (a, sx, sy) = sprite_pivot(o, e.sprite);
        let _ = writeln!(w, "    - serializedVersion: 2");
        let _ = writeln!(w, "      name: {}", yaml_str(e.name));
        let _ = writeln!(w, "      rect:");
        let _ = writeln!(w, "        serializedVersion: 2");
        let _ = writeln!(w, "        x: {}", f.x);
        let _ = writeln!(w, "        y: {}", page_h - f.y - f.h);
        let _ = writeln!(w, "        width: {}", f.w);
        let _ = writeln!(w, "        height: {}", f.h);
        let _ = writeln!(w, "      alignment: {a}");
        let _ = writeln!(w, "      pivot: {{x: {}, y: {}}}", fmt_f(sx), fmt_f(sy));
        let _ = writeln!(w, "      border: {{x: 0, y: 0, z: 0, w: 0}}");
        if modern {
            let _ = writeln!(w, "      customData:");
        }
        let _ = writeln!(w, "      outline: []");
        let _ = writeln!(w, "      physicsShape: []");
        let _ = writeln!(w, "      tessellationDetail: 0");
        let _ = writeln!(w, "      bones: []");
        let _ = writeln!(
            w,
            "      spriteID: {}",
            e.ids.sprite_id.as_deref().unwrap_or_default()
        );
        let _ = writeln!(w, "      internalID: {}", e.ids.internal_id);
        let _ = writeln!(w, "      vertices: []");
        let _ = writeln!(w, "      indices:");
        let _ = writeln!(w, "      edges: []");
        let _ = writeln!(w, "      weights: []");
    }
    let _ = writeln!(w, "    outline: []");
    if modern {
        let _ = writeln!(w, "    customData:");
    }
    let _ = writeln!(w, "    physicsShape: []");
    let _ = writeln!(w, "    bones: []");
    let _ = writeln!(w, "    spriteID:");
    let _ = writeln!(w, "    internalID: 0");
    let _ = writeln!(w, "    vertices: []");
    let _ = writeln!(w, "    indices:");
    let _ = writeln!(w, "    edges: []");
    let _ = writeln!(w, "    weights: []");
    let _ = writeln!(w, "    secondaryTextures: []");
    if modern {
        let _ = writeln!(w, "    spriteCustomMetadata:");
        let _ = writeln!(w, "      entries: []");
    }
    if entries.is_empty() {
        let _ = writeln!(w, "    nameFileIdTable: {{}}");
    } else {
        let _ = writeln!(w, "    nameFileIdTable:");
        for e in entries {
            let _ = writeln!(w, "      {}: {}", yaml_str(e.name), e.ids.internal_id);
        }
    }
    if modern {
        let _ = writeln!(w, "  mipmapLimitGroupName:");
    }
    let _ = writeln!(w, "  pSDRemoveMatte: 0");
    let _ = writeln!(w, "  userData:");
    let _ = writeln!(w, "  assetBundleName:");
    let _ = writeln!(w, "  assetBundleVariant:");
    m
}

pub(crate) struct UnityExport {
    pub files: Vec<(String, Vec<u8>)>,
    pub state: Value,
    pub warnings: Vec<OpError>,
}

fn state_ids(prev: &Value) -> (BTreeMap<String, String>, BTreeMap<String, SpriteIds>) {
    let mut pages = BTreeMap::new();
    let mut sprites = BTreeMap::new();
    let u = &prev["unity"];
    if let Some(p) = u["pages"].as_object() {
        for (k, v) in p {
            if let Some(g) = v.as_str() {
                pages.insert(k.clone(), g.to_string());
            }
        }
    }
    if let Some(s) = u["sprites"].as_object() {
        for (k, v) in s {
            if let Some(id) = v["internalID"].as_i64() {
                let sid = v["spriteID"].as_str().map(str::to_string);
                sprites.insert(
                    k.clone(),
                    SpriteIds {
                        internal_id: id,
                        sprite_id: sid,
                    },
                );
            }
        }
    }
    (pages, sprites)
}

fn valid_max_size(v: u32) -> bool {
    v == 0 || (v.is_power_of_two() && (32..=16384).contains(&v))
}

pub(crate) fn export(
    project: &AtlasProject,
    base: &str,
    o: &UnityOptions,
    existing: &ExistingFiles,
) -> OpResult<UnityExport> {
    if !valid_max_size(o.max_texture_size) {
        return Err(OpError::invalid_param("maxTextureSize", "notUnitySize"));
    }
    if !(o.pixels_per_unit.is_finite() && o.pixels_per_unit > 0.0) {
        return Err(OpError::invalid_param("pixelsPerUnit", "mustBePositive"));
    }
    let n = project.pages.len();
    let mut warnings = Vec::new();

    // Collect previous IDs: existing metas win over the saved exporter state.
    let (state_pages, state_sprites) = state_ids(&project.exporter_state);
    let mut meta_guids: BTreeMap<String, String> = BTreeMap::new();
    let mut known: BTreeMap<String, SpriteIds> = state_sprites;
    let mut from_meta: BTreeMap<String, SpriteIds> = BTreeMap::new();
    for i in 0..n {
        let path = format!("{}.meta", page_file_name(base, i, n));
        if let Some(bytes) = existing.get(&path) {
            let info = parse_meta(&String::from_utf8_lossy(bytes));
            match info.guid {
                Some(g) => {
                    meta_guids.insert(path.clone(), g);
                }
                None => warnings
                    .push(OpError::new(codes::ATLAS_META_UNREADABLE).with("path", path.clone())),
            }
            for (name, ids) in info.sprites {
                from_meta.entry(name).or_insert(ids);
            }
        }
    }
    for (name, ids) in from_meta {
        let merged = match known.get(&name) {
            Some(old) if ids.sprite_id.is_none() && old.internal_id == ids.internal_id => {
                old.clone()
            }
            _ => ids,
        };
        known.insert(name, merged);
    }

    let mut files = Vec::new();
    let mut st_pages = serde_json::Map::new();
    let mut st_sprites = serde_json::Map::new();
    let mut used_internal: BTreeSet<i64> = BTreeSet::new();
    let mut used_sprite_ids: BTreeSet<String> = BTreeSet::new();
    for page in 0..n {
        let png = page_file_name(base, page, n);
        let path = format!("{png}.meta");
        let guid = meta_guids
            .get(&path)
            .or_else(|| state_pages.get(&png))
            .cloned()
            .unwrap_or_else(new_guid);
        let mut entries = Vec::new();
        for (name, s) in page_frames(project, page) {
            let mut ids = known.get(name).cloned().unwrap_or_default();
            if ids.internal_id == 0 || used_internal.contains(&ids.internal_id) {
                ids.internal_id = loop {
                    let id = new_internal_id();
                    if !used_internal.contains(&id) {
                        break id;
                    }
                };
            }
            let sid_ok = ids.sprite_id.as_ref().is_some_and(|s| {
                s.len() == 32
                    && s.chars().all(|c| c.is_ascii_hexdigit())
                    && !used_sprite_ids.contains(s)
            });
            if !sid_ok {
                ids.sprite_id = Some(loop {
                    let id = new_guid();
                    if !used_sprite_ids.contains(&id) {
                        break id;
                    }
                });
            }
            used_internal.insert(ids.internal_id);
            used_sprite_ids.insert(ids.sprite_id.clone().unwrap_or_default());
            st_sprites.insert(
                name.to_string(),
                json!({ "internalID": ids.internal_id, "spriteID": ids.sprite_id.clone().unwrap_or_default() }),
            );
            entries.push(SpriteEntry {
                name,
                sprite: s,
                ids,
            });
        }
        let info = project.pages[page];
        files.push((
            path,
            write_meta(o, &guid, info.width, info.height, &entries).into_bytes(),
        ));
        st_pages.insert(png, Value::String(guid));
    }
    Ok(UnityExport {
        files,
        state: json!({ "pages": st_pages, "sprites": st_sprites }),
        warnings,
    })
}
