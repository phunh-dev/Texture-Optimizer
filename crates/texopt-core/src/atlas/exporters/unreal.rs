//! Unreal Paper2D: TexturePacker JSON (hash) as read by Paper2D's sprite sheet
//! importer (`meta.app` must be the TexturePacker URL, `meta.target` = "paper2d").

use serde::{Deserialize, Serialize};

use super::generic::{JsonFormat, TpSettings, tp_document};
use super::{Pivot, page_stem};
use crate::OpResult;
use crate::atlas::incremental::AtlasProject;

pub const PAPER2D_APP: &str = "http://www.codeandweb.com/texturepacker";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Paper2dExtension {
    /// `.paper2dsprites` (what TexturePacker writes for Paper2D).
    #[default]
    Paper2dsprites,
    Json,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct UnrealOptions {
    /// Pivot written for every frame (0,0 = top-left, default center).
    pub pivot: Pivot,
    pub file_extension: Paper2dExtension,
}

pub(crate) fn export(
    project: &AtlasProject,
    base: &str,
    o: &UnrealOptions,
) -> OpResult<Vec<(String, Vec<u8>)>> {
    let cfg = TpSettings {
        app: PAPER2D_APP,
        target: Some("paper2d"),
        format: JsonFormat::Hash,
        include_trim_info: true,
        pretty: true,
        pivot: o.pivot,
        image_path_prefix: "",
    };
    let ext = match o.file_extension {
        Paper2dExtension::Paper2dsprites => "paper2dsprites",
        Paper2dExtension::Json => "json",
    };
    let n = project.pages.len();
    (0..n)
        .map(|i| {
            Ok((
                format!("{}.{ext}", page_stem(base, i, n)),
                tp_document(project, base, i, &cfg)?,
            ))
        })
        .collect()
}
