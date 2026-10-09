//! TexturePacker-compatible JSON (hash or array), one file per page.
//!
//! Follows TexturePacker conventions: `frame.w/h` are the sprite's unrotated
//! (trimmed) size; when `rotated` is true the pixels occupy `frame.h x frame.w`
//! in the page, turned 90 degrees clockwise.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Pivot, page_file_name, page_frames, page_stem};
use crate::atlas::incremental::{AtlasProject, ProjectSprite};
use crate::{OpError, OpResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum JsonFormat {
    /// `frames` is an object keyed by sprite name.
    #[default]
    Hash,
    /// `frames` is an array of objects with a `filename` field.
    Array,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GenericJsonOptions {
    pub format: JsonFormat,
    /// Prepended to the page file name in `meta.image` (e.g. `"textures/"`).
    pub image_path_prefix: String,
    /// Emit `trimmed`, `spriteSourceSize` and `sourceSize`.
    pub include_trim_info: bool,
    pub pretty: bool,
    /// Default pivot written for every frame (0,0 = top-left).
    pub pivot: Pivot,
}

impl Default for GenericJsonOptions {
    fn default() -> Self {
        Self {
            format: JsonFormat::Hash,
            image_path_prefix: String::new(),
            include_trim_info: true,
            pretty: true,
            pivot: Pivot::default(),
        }
    }
}

pub const GENERIC_APP: &str = "Texture Optimizer";
pub const TP_FORMAT_VERSION: &str = "1.0";

#[derive(Serialize)]
struct TpRect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

#[derive(Serialize)]
struct TpSize {
    w: u32,
    h: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TpFrame {
    frame: TpRect,
    rotated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    trimmed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sprite_source_size: Option<TpRect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_size: Option<TpSize>,
    pivot: Pivot,
}

#[derive(Serialize)]
struct TpNamed<'a> {
    filename: &'a str,
    #[serde(flatten)]
    frame: TpFrame,
}

#[derive(Serialize)]
struct TpMeta<'a> {
    app: &'a str,
    version: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<&'a str>,
    image: String,
    format: &'a str,
    size: TpSize,
    scale: &'a str,
}

#[derive(Serialize)]
struct TpDoc<'a, F: Serialize> {
    frames: F,
    meta: TpMeta<'a>,
}

/// Shared by the generic and Paper2D exporters.
pub(crate) struct TpSettings<'a> {
    pub app: &'a str,
    pub target: Option<&'a str>,
    pub format: JsonFormat,
    pub include_trim_info: bool,
    pub pretty: bool,
    pub pivot: Pivot,
    pub image_path_prefix: &'a str,
}

fn tp_frame(s: &ProjectSprite, cfg: &TpSettings) -> TpFrame {
    let ss = &s.sprite_source_size;
    let trim = cfg.include_trim_info;
    TpFrame {
        frame: TpRect {
            x: s.frame.x,
            y: s.frame.y,
            w: ss.w,
            h: ss.h,
        },
        rotated: s.rotated,
        trimmed: trim.then_some(s.trimmed),
        sprite_source_size: trim.then_some(TpRect {
            x: ss.x,
            y: ss.y,
            w: ss.w,
            h: ss.h,
        }),
        source_size: trim.then_some(TpSize {
            w: s.source_size.w,
            h: s.source_size.h,
        }),
        pivot: cfg.pivot,
    }
}

fn to_bytes<T: Serialize>(v: &T, pretty: bool) -> OpResult<Vec<u8>> {
    let r = if pretty {
        serde_json::to_vec_pretty(v)
    } else {
        serde_json::to_vec(v)
    };
    r.map_err(|e| {
        OpError::new(crate::error::codes::IMG_ENCODE_FAILED).with("detail", e.to_string())
    })
}

/// One TexturePacker JSON document for `page`.
pub(crate) fn tp_document(
    project: &AtlasProject,
    base: &str,
    page: usize,
    cfg: &TpSettings,
) -> OpResult<Vec<u8>> {
    let info = project.pages[page];
    let meta = TpMeta {
        app: cfg.app,
        version: TP_FORMAT_VERSION,
        target: cfg.target,
        image: format!(
            "{}{}",
            cfg.image_path_prefix,
            page_file_name(base, page, project.pages.len())
        ),
        format: "RGBA8888",
        size: TpSize {
            w: info.width,
            h: info.height,
        },
        scale: "1",
    };
    let frames = page_frames(project, page);
    match cfg.format {
        JsonFormat::Hash => {
            let map: BTreeMap<&str, TpFrame> =
                frames.iter().map(|(n, s)| (*n, tp_frame(s, cfg))).collect();
            to_bytes(&TpDoc { frames: map, meta }, cfg.pretty)
        }
        JsonFormat::Array => {
            let list: Vec<TpNamed> = frames
                .iter()
                .map(|(n, s)| TpNamed {
                    filename: n,
                    frame: tp_frame(s, cfg),
                })
                .collect();
            to_bytes(&TpDoc { frames: list, meta }, cfg.pretty)
        }
    }
}

pub(crate) fn export(
    project: &AtlasProject,
    base: &str,
    o: &GenericJsonOptions,
) -> OpResult<Vec<(String, Vec<u8>)>> {
    let cfg = TpSettings {
        app: GENERIC_APP,
        target: None,
        format: o.format,
        include_trim_info: o.include_trim_info,
        pretty: o.pretty,
        pivot: o.pivot,
        image_path_prefix: &o.image_path_prefix,
    };
    let n = project.pages.len();
    (0..n)
        .map(|i| {
            Ok((
                format!("{}.json", page_stem(base, i, n)),
                tp_document(project, base, i, &cfg)?,
            ))
        })
        .collect()
}
