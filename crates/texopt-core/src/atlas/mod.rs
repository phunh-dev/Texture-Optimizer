//! Smart atlas generation: packing, incremental project files and
//! engine-specific exporters (generic JSON, Unity, Godot, Unreal Paper2D,
//! or the page image only).
//!
//! Flow used by the app:
//! 1. optionally [`exporters::adapt_params`] to auto-disable features the
//!    chosen exporter cannot represent (e.g. rotation for Unity/Godot);
//! 2. [`build`] packs the sprites (optionally reusing a previous
//!    [`AtlasProject`]) and returns page images + the new project;
//! 3. [`exporters::export`] produces the files to write (page PNGs + engine
//!    metadata); the app stores `ExportOutput.exporter_state` into
//!    `project.exporter_state` and saves `<name>.texatlas.json`.

pub mod exporters;
pub mod incremental;
mod layout;
mod packer;
pub mod params;
mod sprites;
pub mod workflow;

pub use exporters::{ExistingFiles, ExportOutput, ExporterConfig, export};
pub use incremental::{
    AtlasProject, Frame, IncrementalMode, PROJECT_VERSION, PageInfo, ProjectSprite, Size,
};
pub use params::{AtlasParams, MAX_ATLAS_SIZE, PackAlgorithm, PackHeuristic, SizeMode, SortBy};

use crate::{ImageBuf, OpError, OpResult};

/// Error / warning codes introduced by the atlas module.
pub mod codes {
    /// A sprite (or, without `multiPage`, the whole set) does not fit the max
    /// page. Params: `name`, `width`, `height`, `maxWidth`, `maxHeight`.
    pub const ATLAS_DOES_NOT_FIT: &str = "ATLAS_DOES_NOT_FIT";
    /// No sprites were given.
    pub const ATLAS_EMPTY: &str = "ATLAS_EMPTY";
    /// The exporter cannot represent a feature used by the atlas.
    /// Params: `feature` (`rotation` | `multiPage` | `trim`), `exporter`.
    pub const ATLAS_EXPORTER_UNSUPPORTED: &str = "ATLAS_EXPORTER_UNSUPPORTED";
    /// The previous project file is unreadable or inconsistent. Params: `reason`
    /// (+ `name`/`version`/`detail` depending on the reason).
    pub const ATLAS_PROJECT_INVALID: &str = "ATLAS_PROJECT_INVALID";
    /// Warning: two inputs had the same name; the later one was used. Params: `name`.
    pub const ATLAS_DUPLICATE_NAME: &str = "ATLAS_DUPLICATE_NAME";
    /// Warning: `keepPositions` could not reuse the previous layout and packed
    /// from scratch. Params: `reason` (`spacingChanged` | `maxSizeReduced` | `notPot` | `notSquare`).
    pub const ATLAS_LAYOUT_RESET: &str = "ATLAS_LAYOUT_RESET";
    /// Warning from `adapt_params`: a feature was switched off for the exporter.
    /// Params: `feature`, `exporter`.
    pub const ATLAS_FEATURE_DISABLED: &str = "ATLAS_FEATURE_DISABLED";
    /// Warning from the Unity exporter: an existing `.meta` had no readable
    /// guid, so a new one was generated. Params: `path`.
    pub const ATLAS_META_UNREADABLE: &str = "ATLAS_META_UNREADABLE";
    /// Warning from the incremental workflow: a sprite of the previous atlas
    /// could not be recovered from its page image and was dropped.
    /// Params: `name`, `path` (page file), `reason` (error code or `pageMismatch`).
    pub const ATLAS_SPRITE_NOT_RECOVERED: &str = "ATLAS_SPRITE_NOT_RECOVERED";
    /// Warning from the incremental workflow: a stale file of the previous
    /// export could not be deleted. Params: `path`, `detail`.
    pub const ATLAS_STALE_DELETE_FAILED: &str = "ATLAS_STALE_DELETE_FAILED";
    /// A sprite has zero width or height. Params: `name`.
    pub const ATLAS_SPRITE_EMPTY: &str = "ATLAS_SPRITE_EMPTY";
    /// Warning from the workflow with the image-only exporter: an atlas
    /// project exists at the target but is neither merged nor updated (and
    /// none of its files is deleted). Params: `path` (project file).
    pub const ATLAS_PROJECT_IGNORED: &str = "ATLAS_PROJECT_IGNORED";
}

/// One input image. `name` is the sprite name used by exporters (usually the
/// file stem) and must be unique; a duplicate name replaces the earlier sprite.
#[derive(Debug, Clone)]
pub struct SpriteInput {
    pub name: String,
    pub image: ImageBuf,
}

impl SpriteInput {
    pub fn new(name: impl Into<String>, image: ImageBuf) -> Self {
        Self {
            name: name.into(),
            image,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AtlasResult {
    /// Page images, index = `ProjectSprite.page`.
    pub pages: Vec<ImageBuf>,
    pub project: AtlasProject,
    pub warnings: Vec<OpError>,
}

/// Pack `sprites` into one or more pages.
///
/// `previous` is the last saved project (if any); with
/// [`IncrementalMode::KeepPositions`] its layout is reused, with
/// [`IncrementalMode::RepackOptimal`] only its exporter state is carried over.
/// The output is fully deterministic for a given input order.
pub fn build(
    sprites: Vec<SpriteInput>,
    params: &AtlasParams,
    previous: Option<&AtlasProject>,
    mode: IncrementalMode,
) -> OpResult<AtlasResult> {
    params.validate()?;
    if sprites.is_empty() {
        return Err(OpError::new(codes::ATLAS_EMPTY));
    }
    if let Some(prev) = previous {
        prev.validate()?;
    }
    let (prepared, mut warnings) = sprites::prepare(sprites, params)?;
    let pages = match (previous, mode) {
        (Some(prev), IncrementalMode::KeepPositions) => {
            incremental::keep_positions(&prepared, params, prev, &mut warnings)?
        }
        _ => layout::layout_all(&prepared, params)?,
    };

    let images: Vec<ImageBuf> = {
        use rayon::prelude::*;
        pages
            .par_iter()
            .map(|p| layout::compose(p, &prepared, params))
            .collect()
    };

    let mut out_sprites = Vec::with_capacity(prepared.len());
    for (page_idx, page) in pages.iter().enumerate() {
        for pl in &page.placements {
            let p = &prepared[pl.idx];
            out_sprites.push(ProjectSprite {
                name: p.name.clone(),
                hash: p.hash.clone(),
                page: page_idx,
                frame: Frame {
                    x: pl.frame.x,
                    y: pl.frame.y,
                    w: pl.frame.w,
                    h: pl.frame.h,
                },
                rotated: pl.rotated,
                trimmed: p.trimmed(),
                source_size: Size {
                    w: p.source_w,
                    h: p.source_h,
                },
                sprite_source_size: Frame {
                    x: p.trim.x,
                    y: p.trim.y,
                    w: p.trim.w,
                    h: p.trim.h,
                },
                aliases: p.aliases.clone(),
            });
        }
    }
    out_sprites.sort_by(|a, b| a.name.cmp(&b.name));

    let project = AtlasProject {
        version: PROJECT_VERSION,
        params: params.clone(),
        pages: pages
            .iter()
            .map(|p| PageInfo {
                width: p.width,
                height: p.height,
            })
            .collect(),
        sprites: out_sprites,
        exporter_state: previous
            .map(|p| p.exporter_state.clone())
            .unwrap_or(serde_json::Value::Null),
    };
    Ok(AtlasResult {
        pages: images,
        project,
        warnings,
    })
}

/// One page of [`layout_sizes`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizeLayoutPage {
    pub width: u32,
    pub height: u32,
    /// `(input index, frame)` of every rect on this page (top-left origin).
    pub rects: Vec<(usize, Frame)>,
}

/// Pack plain sizes (no pixels, no trimming or dedupe) with the same packer
/// and page sizing as [`build`]; rotation is never used. The 3D texture
/// packer uses this to compute one layout shared by several channel images.
/// Sizes are packed largest area first (ties: input order). Fails with
/// `ATLAS_DOES_NOT_FIT` (param `name` = input index) like [`build`].
pub fn layout_sizes(sizes: &[(u32, u32)], params: &AtlasParams) -> OpResult<Vec<SizeLayoutPage>> {
    let mut params = params.clone();
    params.allow_rotation = false;
    params.trim = false;
    params.validate()?;
    if sizes.is_empty() {
        return Err(OpError::new(codes::ATLAS_EMPTY));
    }
    let mut prepared: Vec<sprites::Prepared> = Vec::with_capacity(sizes.len());
    for (i, &(w, h)) in sizes.iter().enumerate() {
        if w == 0 || h == 0 {
            return Err(OpError::new(codes::ATLAS_SPRITE_EMPTY).with("name", i.to_string()));
        }
        prepared.push(sprites::Prepared {
            name: i.to_string(),
            aliases: Vec::new(),
            hash: String::new(),
            image: ImageBuf::new(1, 1),
            source_w: w,
            source_h: h,
            trim: packer::Rect::new(0, 0, w, h),
            order: i,
        });
    }
    prepared.sort_by_key(|p| {
        (
            std::cmp::Reverse(u64::from(p.w()) * u64::from(p.h())),
            p.order,
        )
    });
    let pages = layout::layout_all(&prepared, &params)?;
    Ok(pages
        .into_iter()
        .map(|p| SizeLayoutPage {
            width: p.width,
            height: p.height,
            rects: p
                .placements
                .iter()
                .map(|pl| {
                    (
                        prepared[pl.idx].order,
                        Frame {
                            x: pl.frame.x,
                            y: pl.frame.y,
                            w: pl.frame.w,
                            h: pl.frame.h,
                        },
                    )
                })
                .collect(),
        })
        .collect())
}

#[cfg(test)]
mod layout_sizes_tests {
    use super::*;

    #[test]
    fn packs_sizes_without_rotation_into_pot_pages() {
        let params = AtlasParams {
            padding: 4,
            extrude: 4,
            allow_rotation: true,
            max_width: 256,
            max_height: 256,
            multi_page: true,
            ..Default::default()
        };
        let sizes = [(100, 20), (20, 100), (64, 64), (200, 200)];
        let pages = layout_sizes(&sizes, &params).unwrap();
        let mut seen = vec![false; sizes.len()];
        for page in &pages {
            assert!(page.width.is_power_of_two() && page.height.is_power_of_two());
            for (i, f) in &page.rects {
                assert_eq!((f.w, f.h), sizes[*i], "never rotated");
                assert!(
                    f.x >= 4
                        && f.y >= 4
                        && f.x + f.w + 4 <= page.width
                        && f.y + f.h + 4 <= page.height
                );
                seen[*i] = true;
            }
            for (a, fa) in &page.rects {
                for (b, fb) in &page.rects {
                    if a < b {
                        let sep = fa.x + fa.w + 12 <= fb.x
                            || fb.x + fb.w + 12 <= fa.x
                            || fa.y + fa.h + 12 <= fb.y
                            || fb.y + fb.h + 12 <= fa.y;
                        assert!(sep, "rects {a} and {b} overlap or are too close");
                    }
                }
            }
        }
        assert!(seen.iter().all(|&s| s));
        assert!(pages.len() >= 2);
        let err = layout_sizes(&[(300, 10)], &params).unwrap_err();
        assert_eq!(err.code, codes::ATLAS_DOES_NOT_FIT);
        assert_eq!(err.params["name"], "0");
    }
}
