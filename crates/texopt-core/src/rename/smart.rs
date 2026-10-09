//! Texture type detection from file names and engine naming conventions.

use std::collections::BTreeMap;
use std::ops::Range;

use serde::{Deserialize, Serialize};

use super::case::{CaseMode, apply_case, word_spans};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextureType {
    BaseColor,
    Normal,
    Roughness,
    Metallic,
    Ao,
    Emissive,
    Height,
    Mask,
    Orm,
    Opacity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum EnginePreset {
    /// `T_<Name>_<D|N|R|M|AO|E|H|M|ORM|O>`
    #[default]
    Unreal,
    /// `<Name>_<Albedo|Normal|Roughness|Metallic|Occlusion|Emission|Height|Mask|ORM|Opacity>`
    Unity,
    /// snake_case `<name>_<albedo|normal|roughness|metallic|ao|emission|height|mask|orm|opacity>`
    Godot,
    /// `<Name>_<customMap[type]>`; unmapped types are left alone.
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SmartParams {
    pub enabled: bool,
    pub preset: EnginePreset,
    pub custom_map: BTreeMap<TextureType, String>,
}

fn keyword(word: &str) -> Option<TextureType> {
    use TextureType::*;
    Some(match word {
        "albedo" | "diffuse" | "basecolor" | "basecolour" | "color" | "colour" | "col" => BaseColor,
        "normal" | "nrm" | "nor" | "norm" => Normal,
        "roughness" | "rough" | "rgh" => Roughness,
        "metallic" | "metal" | "mtl" => Metallic,
        "ao" | "occlusion" | "ambientocclusion" => Ao,
        "emissive" | "emission" | "emit" => Emissive,
        "height" | "disp" | "displacement" => Height,
        "mask" | "msk" => Mask,
        "orm" => Orm,
        "opacity" | "alpha" => Opacity,
        _ => return None,
    })
}

/// Detected type plus the byte range of the keyword inside the stem.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Detection {
    pub ty: TextureType,
    pub span: Range<usize>,
}

/// Last keyword in the name wins; two-word keywords (`base_color`,
/// `BaseColor`) are matched before single words.
pub(crate) fn detect(stem: &str) -> Option<Detection> {
    let spans = word_spans(stem);
    let lower: Vec<String> = spans
        .iter()
        .map(|r| stem[r.clone()].to_lowercase())
        .collect();
    for i in (0..spans.len()).rev() {
        if i > 0 {
            let pair = format!("{}{}", lower[i - 1], lower[i]);
            if matches!(
                pair.as_str(),
                "basecolor" | "basecolour" | "ambientocclusion"
            ) {
                return Some(Detection {
                    ty: keyword(&pair)?,
                    span: spans[i - 1].start..spans[i].end,
                });
            }
        }
        if let Some(ty) = keyword(&lower[i]) {
            return Some(Detection {
                ty,
                span: spans[i].clone(),
            });
        }
    }
    None
}

pub fn detect_texture_type(stem: &str) -> Option<TextureType> {
    detect(stem).map(|d| d.ty)
}

/// Removes the keyword and one adjacent separator; keeps the stem if nothing would remain.
pub(crate) fn strip_keyword(stem: &str, span: &Range<usize>) -> String {
    let is_sep = |c: char| !c.is_alphanumeric();
    let (mut start, mut end) = (span.start, span.end);
    if let Some(c) = stem[..start].chars().next_back().filter(|&c| is_sep(c)) {
        start -= c.len_utf8();
    } else if let Some(c) = stem[end..].chars().next().filter(|&c| is_sep(c)) {
        end += c.len_utf8();
    }
    let out = format!("{}{}", &stem[..start], &stem[end..]);
    if out.is_empty() { stem.to_owned() } else { out }
}

/// Engine suffix for a type; `None` for custom types missing from the map.
pub(crate) fn suffix(smart: &SmartParams, ty: TextureType) -> Option<String> {
    use TextureType::*;
    let s = match smart.preset {
        EnginePreset::Unreal => match ty {
            BaseColor => "D",
            Normal => "N",
            Roughness => "R",
            Metallic => "M",
            Ao => "AO",
            Emissive => "E",
            Height => "H",
            Mask => "M",
            Orm => "ORM",
            Opacity => "O",
        },
        EnginePreset::Unity => match ty {
            BaseColor => "Albedo",
            Normal => "Normal",
            Roughness => "Roughness",
            Metallic => "Metallic",
            Ao => "Occlusion",
            Emissive => "Emission",
            Height => "Height",
            Mask => "Mask",
            Orm => "ORM",
            Opacity => "Opacity",
        },
        EnginePreset::Godot => match ty {
            BaseColor => "albedo",
            Normal => "normal",
            Roughness => "roughness",
            Metallic => "metallic",
            Ao => "ao",
            Emissive => "emission",
            Height => "height",
            Mask => "mask",
            Orm => "orm",
            Opacity => "opacity",
        },
        EnginePreset::Custom => return smart.custom_map.get(&ty).cloned(),
    };
    Some(s.to_owned())
}

/// Applies the engine convention to an already composed name body.
/// `suffix` is `None` when it must not be appended (no type detected, or the
/// template placed `{type}` itself).
pub(crate) fn wrap(preset: EnginePreset, body: &str, suffix: Option<&str>) -> String {
    let join = |b: &str, s: Option<&str>| match s {
        Some(s) if !b.is_empty() => format!("{b}_{s}"),
        Some(s) => s.to_owned(),
        None => b.to_owned(),
    };
    match preset {
        EnginePreset::Unreal => {
            let b = if body.starts_with("T_") {
                body.to_owned()
            } else {
                format!("T_{body}")
            };
            join(&b, suffix)
        }
        EnginePreset::Godot => join(&apply_case(body, CaseMode::Snake), suffix),
        EnginePreset::Unity | EnginePreset::Custom => join(body, suffix),
    }
}
