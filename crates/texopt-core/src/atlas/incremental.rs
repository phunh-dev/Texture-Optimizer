//! Atlas project file (`<name>.texatlas.json`) and incremental re-packing.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::codes;
use super::layout::{PageLayout, Placement, check_fits, does_not_fit, layout_all, placement};
use super::packer::{Geometry, MaxRectsBin, PackItem, Rect, grow_step, layout_page};
use super::params::{AtlasParams, PackAlgorithm, PackHeuristic, SizeMode};
use super::sprites::Prepared;
use crate::{OpError, OpResult};

/// Current `AtlasProject.version`.
pub const PROJECT_VERSION: u32 = 1;

/// How a new build treats the layout of a previous project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum IncrementalMode {
    /// Unchanged sprites (same name and same packed size) keep their exact
    /// rect; changed/new sprites go into free space, growing pages (and then
    /// adding pages when `multiPage`) only when needed.
    #[default]
    KeepPositions,
    /// Pack from scratch; only the exporter state (Unity GUID/IDs) is kept.
    RepackOptimal,
}

/// Rectangle in page pixels, top-left origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Frame {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PageInfo {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSprite {
    pub name: String,
    /// blake3 of the source image (dimensions + RGBA bytes).
    pub hash: String,
    pub page: usize,
    /// Area occupied in the page. When `rotated` the sprite is stored turned
    /// 90 degrees clockwise, so `frame.w == spriteSourceSize.h`.
    pub frame: Frame,
    pub rotated: bool,
    pub trimmed: bool,
    /// Untrimmed source image size.
    pub source_size: Size,
    /// Trimmed rect inside the source image (unrotated).
    pub sprite_source_size: Frame,
    /// Other names sharing this rect because their pixels are identical (dedupe).
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtlasProject {
    pub version: u32,
    pub params: AtlasParams,
    pub pages: Vec<PageInfo>,
    /// Sorted by name.
    pub sprites: Vec<ProjectSprite>,
    /// Opaque per-exporter state (e.g. `{"unity": {...}}`) replaced by
    /// `ExportOutput.exporter_state` after each export.
    #[serde(default)]
    pub exporter_state: Value,
}

fn invalid(reason: &str) -> OpError {
    OpError::new(codes::ATLAS_PROJECT_INVALID).with("reason", reason)
}

impl AtlasProject {
    pub fn from_json(text: &str) -> OpResult<Self> {
        let p: AtlasProject = serde_json::from_str(text)
            .map_err(|e| invalid("parse").with("detail", e.to_string()))?;
        p.validate()?;
        Ok(p)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Find a sprite by its name or one of its aliases.
    pub fn sprite(&self, name: &str) -> Option<&ProjectSprite> {
        self.sprites
            .iter()
            .find(|s| s.name == name || s.aliases.iter().any(|a| a == name))
    }

    /// Every sprite name (primary names and aliases), sorted.
    pub fn all_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .sprites
            .iter()
            .flat_map(|s| std::iter::once(s.name.clone()).chain(s.aliases.clone()))
            .collect();
        v.sort();
        v
    }

    pub fn validate(&self) -> OpResult<()> {
        if self.version != PROJECT_VERSION {
            return Err(invalid("unsupportedVersion").with("version", self.version));
        }
        let mut names = BTreeSet::new();
        for s in &self.sprites {
            let page = self
                .pages
                .get(s.page)
                .ok_or_else(|| invalid("pageIndex").with("name", s.name.clone()))?;
            let f = &s.frame;
            if f.w == 0
                || f.h == 0
                || u64::from(f.x) + u64::from(f.w) > u64::from(page.width)
                || u64::from(f.y) + u64::from(f.h) > u64::from(page.height)
            {
                return Err(invalid("frameOutOfBounds").with("name", s.name.clone()));
            }
            let ss = &s.sprite_source_size;
            let (ew, eh) = if s.rotated {
                (ss.h, ss.w)
            } else {
                (ss.w, ss.h)
            };
            if (f.w, f.h) != (ew, eh) {
                return Err(invalid("frameSize").with("name", s.name.clone()));
            }
            for n in std::iter::once(&s.name).chain(&s.aliases) {
                if !names.insert(n.as_str()) {
                    return Err(invalid("duplicateName").with("name", n.clone()));
                }
            }
        }
        Ok(())
    }
}

/// Why a previous layout cannot be reused with the new params, if at all.
fn incompatibility(prev: &AtlasProject, params: &AtlasParams) -> Option<&'static str> {
    let pp = &prev.params;
    if pp.padding != params.padding || pp.extrude != params.extrude || pp.border != params.border {
        return Some("spacingChanged");
    }
    let (mw, mh) = params.effective_max();
    for p in &prev.pages {
        if p.width > mw || p.height > mh {
            return Some("maxSizeReduced");
        }
        if params.force_pot && !(p.width.is_power_of_two() && p.height.is_power_of_two()) {
            return Some("notPot");
        }
        if params.force_square && p.width != p.height {
            return Some("notSquare");
        }
    }
    None
}

struct PageState {
    w: u32,
    h: u32,
    cells: Vec<Rect>,
    placements: Vec<Placement>,
    bin: MaxRectsBin,
}

fn rebuild(
    geom: &Geometry,
    w: u32,
    h: u32,
    cells: &[Rect],
    heuristic: PackHeuristic,
) -> Option<MaxRectsBin> {
    let (bw, bh) = geom.bin_size(w, h)?;
    let mut bin = MaxRectsBin::new(bw, bh, heuristic);
    for c in cells {
        bin.place(*c);
    }
    Some(bin)
}

pub(crate) fn keep_positions(
    prepared: &[Prepared],
    params: &AtlasParams,
    prev: &AtlasProject,
    warnings: &mut Vec<OpError>,
) -> OpResult<Vec<PageLayout>> {
    if let Some(reason) = incompatibility(prev, params) {
        warnings.push(OpError::new(codes::ATLAS_LAYOUT_RESET).with("reason", reason));
        return layout_all(prepared, params);
    }
    check_fits(prepared, params)?;
    let geom = Geometry::from_params(params);
    let (mw, mh) = params.effective_max();
    // Skyline cannot start from arbitrary occupied rects, so free space is
    // always searched with MaxRects.
    let heuristic = match params.algorithm {
        PackAlgorithm::MaxRects => params.heuristic,
        PackAlgorithm::Skyline => PackHeuristic::BestShortSideFit,
    };
    let fixed = params.size_mode == SizeMode::Fixed;

    let mut pages: Vec<PageState> = Vec::with_capacity(prev.pages.len());
    for p in &prev.pages {
        let (w, h) = if fixed { (mw, mh) } else { (p.width, p.height) };
        let bin =
            rebuild(&geom, w, h, &[], heuristic).ok_or_else(|| invalid("frameOutOfBounds"))?;
        pages.push(PageState {
            w,
            h,
            cells: Vec::new(),
            placements: Vec::new(),
            bin,
        });
    }

    let mut by_name: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, s) in prev.sprites.iter().enumerate() {
        for n in std::iter::once(&s.name).chain(&s.aliases) {
            by_name.insert(n.as_str(), i);
        }
    }
    let mut claimed = vec![false; prev.sprites.len()];
    let mut pending = Vec::new();
    for (idx, p) in prepared.iter().enumerate() {
        let mut kept = false;
        for n in p.names() {
            let Some(&si) = by_name.get(n.as_str()) else {
                continue;
            };
            if claimed[si] {
                continue;
            }
            let s = &prev.sprites[si];
            let (fw, fh) = if s.rotated {
                (p.h(), p.w())
            } else {
                (p.w(), p.h())
            };
            if s.frame.w != fw || s.frame.h != fh || (s.rotated && !params.allow_rotation) {
                continue;
            }
            let (cx, cy) = geom
                .cell_pos(s.frame.x, s.frame.y)
                .ok_or_else(|| invalid("frameOutOfBounds"))?;
            let cell = geom.cell(fw, fh);
            let page = &mut pages[s.page];
            let (bw, bh) = geom
                .bin_size(page.w, page.h)
                .ok_or_else(|| invalid("frameOutOfBounds"))?;
            if cx + cell.w > bw || cy + cell.h > bh {
                return Err(invalid("frameOutOfBounds").with("name", s.name.clone()));
            }
            claimed[si] = true;
            page.cells.push(Rect::new(cx, cy, cell.w, cell.h));
            page.placements.push(Placement {
                idx,
                frame: Rect::new(s.frame.x, s.frame.y, fw, fh),
                rotated: s.rotated,
            });
            kept = true;
            break;
        }
        if !kept {
            pending.push(idx);
        }
    }
    for page in &mut pages {
        page.bin = rebuild(&geom, page.w, page.h, &page.cells, heuristic)
            .ok_or_else(|| invalid("frameOutOfBounds"))?;
    }

    let record =
        |page: &mut PageState, idx: usize, item: PackItem, pl: &crate::atlas::packer::Placed| {
            let (w, h) = item.dims(pl.rotated);
            page.cells.push(Rect::new(pl.x, pl.y, w, h));
            page.placements
                .push(placement(&geom, idx, &prepared[idx], pl));
        };

    'items: for idx in pending {
        let p = &prepared[idx];
        let item = geom.cell(p.w(), p.h());
        // 1. Free space of existing pages.
        for page in pages.iter_mut() {
            if let Some(pl) = page.bin.insert(item, params.allow_rotation) {
                record(page, idx, item, &pl);
                continue 'items;
            }
        }
        // 2. Grow an existing page (positions are top-left based, so they stay valid).
        for page in pages.iter_mut() {
            let (mut w, mut h) = (page.w, page.h);
            while let Some((nw, nh)) = grow_step(w, h, params) {
                (w, h) = (nw, nh);
                let Some(mut bin) = rebuild(&geom, w, h, &page.cells, heuristic) else {
                    continue;
                };
                if let Some(pl) = bin.insert(item, params.allow_rotation) {
                    page.w = w;
                    page.h = h;
                    page.bin = bin;
                    record(page, idx, item, &pl);
                    continue 'items;
                }
            }
        }
        // 3. A new page.
        if !params.multi_page && !pages.is_empty() {
            return Err(does_not_fit(p, params));
        }
        let (w, h, placed) = layout_page(&[item], params).ok_or_else(|| does_not_fit(p, params))?;
        let mut page = PageState {
            w,
            h,
            cells: Vec::new(),
            placements: Vec::new(),
            bin: rebuild(&geom, w, h, &[], heuristic).ok_or_else(|| does_not_fit(p, params))?,
        };
        let (cw, ch) = item.dims(placed[0].rotated);
        page.bin.place(Rect::new(placed[0].x, placed[0].y, cw, ch));
        record(&mut page, idx, item, &placed[0]);
        pages.push(page);
    }

    while pages.len() > 1 && pages.last().is_some_and(|p| p.placements.is_empty()) {
        pages.pop();
    }
    Ok(pages
        .into_iter()
        .map(|p| PageLayout {
            width: p.w,
            height: p.h,
            placements: p.placements,
        })
        .collect())
}
