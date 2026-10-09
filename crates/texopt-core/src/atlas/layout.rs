//! Turning prepared sprites into page layouts (full repack) and page images.

use super::codes;
use super::packer::{
    Geometry, PackCfg, PackItem, Placed, Rect, fits_max_page, layout_page, pack_greedy,
};
use super::params::AtlasParams;
use super::sprites::{Prepared, blit_extruded, oriented, premultiply};
use crate::{ImageBuf, OpError, OpResult};

/// One sprite on a page. `frame` is in page pixels (top-left origin) and is
/// the area actually occupied, i.e. `h x w` of the trimmed sprite when rotated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Placement {
    pub idx: usize,
    pub frame: Rect,
    pub rotated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PageLayout {
    pub width: u32,
    pub height: u32,
    pub placements: Vec<Placement>,
}

pub(crate) fn does_not_fit(p: &Prepared, params: &AtlasParams) -> OpError {
    let (mw, mh) = params.effective_max();
    OpError::new(codes::ATLAS_DOES_NOT_FIT)
        .with("name", p.name.clone())
        .with("width", p.w())
        .with("height", p.h())
        .with("maxWidth", mw)
        .with("maxHeight", mh)
}

pub(crate) fn placement(geom: &Geometry, idx: usize, p: &Prepared, pl: &Placed) -> Placement {
    let (fx, fy) = geom.frame_pos(pl.x, pl.y);
    let (w, h) = if pl.rotated {
        (p.h(), p.w())
    } else {
        (p.w(), p.h())
    };
    Placement {
        idx,
        frame: Rect::new(fx, fy, w, h),
        rotated: pl.rotated,
    }
}

pub(crate) fn check_fits(prepared: &[Prepared], params: &AtlasParams) -> OpResult<()> {
    let geom = Geometry::from_params(params);
    for p in prepared {
        if !fits_max_page(geom.cell(p.w(), p.h()), params) {
            return Err(does_not_fit(p, params));
        }
    }
    Ok(())
}

/// Pack everything from scratch (also used by `repackOptimal`).
pub(crate) fn layout_all(prepared: &[Prepared], params: &AtlasParams) -> OpResult<Vec<PageLayout>> {
    check_fits(prepared, params)?;
    let geom = Geometry::from_params(params);
    let cfg = PackCfg::from_params(params);
    let items: Vec<PackItem> = prepared.iter().map(|p| geom.cell(p.w(), p.h())).collect();
    let (mw, mh) = params.effective_max();
    let (bw, bh) = geom
        .bin_size(mw, mh)
        .ok_or_else(|| OpError::invalid_param("border", "tooLarge"))?;

    let to_page = |w: u32, h: u32, idxs: &[usize], placed: &[Placed]| PageLayout {
        width: w,
        height: h,
        placements: idxs
            .iter()
            .zip(placed)
            .map(|(&i, pl)| placement(&geom, i, &prepared[i], pl))
            .collect(),
    };

    let mut pages = Vec::new();
    let mut remaining: Vec<usize> = (0..prepared.len()).collect();
    while !remaining.is_empty() {
        let sub: Vec<PackItem> = remaining.iter().map(|&i| items[i]).collect();
        if let Some((w, h, placed)) = layout_page(&sub, params) {
            pages.push(to_page(w, h, &remaining, &placed));
            break;
        }
        let greedy = pack_greedy(&sub, bw, bh, &cfg);
        let mut fit_idx = Vec::new();
        let mut fit_pl = Vec::new();
        let mut rest = Vec::new();
        for (&i, g) in remaining.iter().zip(&greedy) {
            match g {
                Some(pl) => {
                    fit_idx.push(i);
                    fit_pl.push(*pl);
                }
                None => rest.push(i),
            }
        }
        if let Some(&first) = rest.first()
            && !params.multi_page
        {
            return Err(does_not_fit(&prepared[first], params));
        }
        if fit_idx.is_empty() {
            // Unreachable: every item fits an empty max-size page (checked above).
            return Err(does_not_fit(&prepared[remaining[0]], params));
        }
        let fit_items: Vec<PackItem> = fit_idx.iter().map(|&i| items[i]).collect();
        match layout_page(&fit_items, params) {
            Some((w, h, placed)) => pages.push(to_page(w, h, &fit_idx, &placed)),
            None => pages.push(to_page(mw, mh, &fit_idx, &fit_pl)),
        }
        remaining = rest;
    }
    Ok(pages)
}

pub(crate) fn compose(page: &PageLayout, prepared: &[Prepared], params: &AtlasParams) -> ImageBuf {
    let mut img = ImageBuf::new(page.width, page.height);
    for pl in &page.placements {
        let src = oriented(&prepared[pl.idx].image, pl.rotated);
        blit_extruded(&mut img, &src, pl.frame.x, pl.frame.y, params.extrude);
    }
    if params.premultiply_alpha {
        premultiply(&mut img);
    }
    img
}
