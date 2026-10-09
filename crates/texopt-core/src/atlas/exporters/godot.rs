//! Godot: one `AtlasTexture` resource (`.tres`) per sprite name.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use super::{page_file_name, page_frames};
use crate::atlas::incremental::{AtlasProject, ProjectSprite};
use crate::{OpError, OpResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum GodotVersion {
    Godot3,
    #[default]
    Godot4,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GodotOptions {
    pub version: GodotVersion,
    /// `res://` folder that will contain the exported page PNG(s).
    pub res_path: String,
    /// Sub-folder (relative to the export folder) for the `.tres` files; empty = next to the PNG.
    pub output_subfolder: String,
    /// Write `filter_clip = true` (prevents bleeding of neighbouring sprites when filtering).
    pub filter_clip: bool,
}

impl Default for GodotOptions {
    fn default() -> Self {
        Self {
            version: GodotVersion::Godot4,
            res_path: "res://".into(),
            output_subfolder: String::new(),
            filter_clip: false,
        }
    }
}

/// Characters not allowed in file names on Windows/macOS/Linux become `_`.
fn sanitize_file_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if "\\/:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    if s == "." || s == ".." {
        s.replace('.', "_")
    } else {
        s
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn join_res(res: &str, file: &str) -> String {
    if res.ends_with('/') {
        format!("{res}{file}")
    } else {
        format!("{res}/{file}")
    }
}

fn validate(o: &GodotOptions) -> OpResult<String> {
    if !o.res_path.starts_with("res://") {
        return Err(OpError::invalid_param("resPath", "mustStartWithRes"));
    }
    let sub = o.output_subfolder.replace('\\', "/");
    let sub = sub.trim_matches('/');
    if sub.split('/').any(|seg| seg == "..") || o.output_subfolder.contains(':') {
        return Err(OpError::invalid_param("outputSubfolder", "invalid"));
    }
    Ok(sub.to_string())
}

fn tres(o: &GodotOptions, texture_path: &str, s: &ProjectSprite) -> String {
    let f = &s.frame;
    let ss = &s.sprite_source_size;
    let margin = (ss.x, ss.y, s.source_size.w - ss.w, s.source_size.h - ss.h);
    let mut out = String::new();
    match o.version {
        GodotVersion::Godot4 => {
            out.push_str("[gd_resource type=\"AtlasTexture\" load_steps=2 format=3]\n\n");
            let _ = writeln!(
                out,
                "[ext_resource type=\"Texture2D\" path=\"{}\" id=\"1\"]\n",
                escape(texture_path)
            );
            out.push_str("[resource]\natlas = ExtResource(\"1\")\n");
            let _ = writeln!(out, "region = Rect2({}, {}, {}, {})", f.x, f.y, f.w, f.h);
            if s.trimmed {
                let _ = writeln!(
                    out,
                    "margin = Rect2({}, {}, {}, {})",
                    margin.0, margin.1, margin.2, margin.3
                );
            }
        }
        GodotVersion::Godot3 => {
            out.push_str("[gd_resource type=\"AtlasTexture\" load_steps=2 format=2]\n\n");
            let _ = writeln!(
                out,
                "[ext_resource path=\"{}\" type=\"Texture\" id=1]\n",
                escape(texture_path)
            );
            out.push_str("[resource]\natlas = ExtResource( 1 )\n");
            let _ = writeln!(out, "region = Rect2( {}, {}, {}, {} )", f.x, f.y, f.w, f.h);
            if s.trimmed {
                let _ = writeln!(
                    out,
                    "margin = Rect2( {}, {}, {}, {} )",
                    margin.0, margin.1, margin.2, margin.3
                );
            }
        }
    }
    if o.filter_clip {
        out.push_str("filter_clip = true\n");
    }
    out
}

pub(crate) fn export(
    project: &AtlasProject,
    base: &str,
    o: &GodotOptions,
) -> OpResult<Vec<(String, Vec<u8>)>> {
    let sub = validate(o)?;
    let n = project.pages.len();
    let mut used = BTreeSet::new();
    let mut files = Vec::new();
    for page in 0..n {
        let texture = join_res(&o.res_path, &page_file_name(base, page, n));
        for (name, s) in page_frames(project, page) {
            let stem = sanitize_file_name(name);
            let mut file = format!("{stem}.tres");
            let mut k = 2;
            while used.contains(&file.to_lowercase()) {
                file = format!("{stem}_{k}.tres");
                k += 1;
            }
            used.insert(file.to_lowercase());
            let path = if sub.is_empty() {
                file
            } else {
                format!("{sub}/{file}")
            };
            files.push((path, tres(o, &texture, s).into_bytes()));
        }
    }
    Ok(files)
}
