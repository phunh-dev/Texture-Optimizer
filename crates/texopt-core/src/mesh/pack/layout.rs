//! Placing the per-material texture blocks on atlas pages.
//!
//! Every page is POT (the packer always runs with `forcePot`), rects are never
//! rotated (UVs could not follow) and never trimmed. When everything does not
//! fit one page, models are kept together on a page whenever possible
//! (first-fit decreasing over models) because a rewritten model file can only
//! reference one atlas set; a model too large for one page is spread over
//! pages of its own.

use std::collections::BTreeMap;

use crate::atlas::{self, AtlasParams, PackAlgorithm, PackHeuristic, SizeMode, SortBy};
use crate::mesh::uv_remap::AtlasRect;
use crate::{OpError, OpResult};

use super::PackOptions;

/// One block to place: all channel images of a material (or of materials
/// sharing the same textures) use this rect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Item {
    pub model: usize,
    pub w: u32,
    pub h: u32,
    /// Shown in `ATLAS_DOES_NOT_FIT` (`model file / material`).
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PageSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Layout {
    pub pages: Vec<PageSize>,
    /// Per item: `(page, rect)`.
    pub placed: Vec<(usize, AtlasRect)>,
}

pub(super) fn atlas_params(options: &PackOptions, multi_page: bool) -> AtlasParams {
    AtlasParams {
        algorithm: PackAlgorithm::MaxRects,
        heuristic: PackHeuristic::BestShortSideFit,
        max_width: options.max_size,
        max_height: options.max_size,
        force_pot: true,
        force_square: options.force_square,
        padding: options.padding,
        extrude: options.extrude,
        border: 0,
        allow_rotation: false,
        trim: false,
        trim_threshold: 0,
        dedupe: false,
        multi_page,
        sort_by: SortBy::Area,
        premultiply_alpha: false,
        size_mode: SizeMode::ShrinkToFit,
    }
}

type PagePlacements = Vec<(PageSize, Vec<(usize, AtlasRect)>)>;

/// Pack `subset` (indices into `items`); `multi` allows several pages.
fn pack_subset(
    items: &[Item],
    subset: &[usize],
    options: &PackOptions,
    multi: bool,
) -> OpResult<PagePlacements> {
    let sizes: Vec<(u32, u32)> = subset.iter().map(|&i| (items[i].w, items[i].h)).collect();
    let pages = atlas::layout_sizes(&sizes, &atlas_params(options, multi)).map_err(|mut e| {
        if let Some(i) = e
            .params
            .get("name")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<usize>().ok())
            .and_then(|k| subset.get(k))
        {
            e.params
                .insert("name".into(), items[*i].label.clone().into());
        }
        e
    })?;
    Ok(pages
        .into_iter()
        .map(|p| {
            (
                PageSize {
                    width: p.width,
                    height: p.height,
                },
                p.rects
                    .into_iter()
                    .map(|(k, f)| {
                        (
                            subset[k],
                            AtlasRect {
                                x: f.x,
                                y: f.y,
                                width: f.w,
                                height: f.h,
                            },
                        )
                    })
                    .collect(),
            )
        })
        .collect())
}

fn is_does_not_fit(e: &OpError) -> bool {
    e.code == atlas::codes::ATLAS_DOES_NOT_FIT
}

/// Everything on one page.
pub(super) fn single_page(items: &[Item], options: &PackOptions) -> OpResult<Layout> {
    let all: Vec<usize> = (0..items.len()).collect();
    let pages = pack_subset(items, &all, options, false)?;
    Ok(assemble(items.len(), pages))
}

fn assemble(n: usize, pages: PagePlacements) -> Layout {
    let mut placed = vec![
        (
            0,
            AtlasRect {
                x: 0,
                y: 0,
                width: 0,
                height: 0
            }
        );
        n
    ];
    let mut sizes = Vec::with_capacity(pages.len());
    for (pi, (size, rects)) in pages.into_iter().enumerate() {
        sizes.push(size);
        for (i, r) in rects {
            placed[i] = (pi, r);
        }
    }
    Layout {
        pages: sizes,
        placed,
    }
}

/// Several pages, keeping each model on one page when it fits alone.
pub(super) fn multi_page(items: &[Item], options: &PackOptions) -> OpResult<Layout> {
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, it) in items.iter().enumerate() {
        groups.entry(it.model).or_default().push(i);
    }
    let area = |g: &[usize]| -> u64 {
        g.iter()
            .map(|&i| u64::from(items[i].w) * u64::from(items[i].h))
            .sum()
    };
    let mut order: Vec<(usize, Vec<usize>)> = groups.into_iter().collect();
    order.sort_by_key(|(m, g)| (std::cmp::Reverse(area(g)), *m));

    let mut shared: Vec<Vec<usize>> = Vec::new();
    let mut exclusive: PagePlacements = Vec::new();
    for (_, group) in order {
        let mut placed = false;
        for page in shared.iter_mut() {
            let mut candidate = page.clone();
            candidate.extend_from_slice(&group);
            match pack_subset(items, &candidate, options, false) {
                Ok(_) => {
                    *page = candidate;
                    placed = true;
                    break;
                }
                Err(e) if is_does_not_fit(&e) => {}
                Err(e) => return Err(e),
            }
        }
        if placed {
            continue;
        }
        match pack_subset(items, &group, options, false) {
            Ok(_) => shared.push(group),
            Err(e) if is_does_not_fit(&e) => {
                exclusive.extend(pack_subset(items, &group, options, true)?);
            }
            Err(e) => return Err(e),
        }
    }
    let mut pages: PagePlacements = Vec::new();
    for page in &shared {
        pages.extend(pack_subset(items, page, options, false)?);
    }
    pages.extend(exclusive);
    Ok(assemble(items.len(), pages))
}

/// Result of [`plan_layout`].
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Planned {
    pub layout: Layout,
    pub items: Vec<Item>,
    /// Divisor applied to the requested texture scale by `scaleToFit` (1 = none).
    pub divisor: u32,
}

/// Lay out the items produced by `make(scale)` following the options:
/// one page at the requested scale; else `scaleToFit` halves the scale (down
/// to 1/8); else `multiPage` spreads models over pages at the requested scale.
pub(super) fn plan_layout(
    options: &PackOptions,
    make: &dyn Fn(f64) -> Vec<Item>,
) -> OpResult<Planned> {
    let base = f64::from(options.texture_scale) / 100.0;
    let items = make(base);
    let first_err = match single_page(&items, options) {
        Ok(layout) => {
            return Ok(Planned {
                layout,
                items,
                divisor: 1,
            });
        }
        Err(e) if is_does_not_fit(&e) => e,
        Err(e) => return Err(e),
    };
    if options.scale_to_fit {
        let mut divisor = 2;
        while divisor <= super::options::MIN_FIT_DIVISOR {
            let scaled = make(base / f64::from(divisor));
            match single_page(&scaled, options) {
                Ok(layout) => {
                    return Ok(Planned {
                        layout,
                        items: scaled,
                        divisor,
                    });
                }
                Err(e) if is_does_not_fit(&e) => {}
                Err(e) => return Err(e),
            }
            divisor *= 2;
        }
    }
    if !options.multi_page {
        return Err(first_err);
    }
    let layout = multi_page(&items, options)?;
    Ok(Planned {
        layout,
        items,
        divisor: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(model: usize, w: u32, h: u32) -> Item {
        Item {
            model,
            w,
            h,
            label: format!("m{model}/{w}x{h}"),
        }
    }

    fn opts(max: u32) -> PackOptions {
        PackOptions {
            max_size: max,
            padding: 2,
            extrude: 2,
            ..Default::default()
        }
    }

    #[test]
    fn one_page_when_it_fits() {
        let items = vec![item(0, 60, 60), item(1, 30, 30)];
        let p = plan_layout(&opts(256), &|_| items.clone()).unwrap();
        assert_eq!(p.layout.pages.len(), 1);
        assert!(p.layout.pages[0].width.is_power_of_two());
        assert_eq!(p.divisor, 1);
    }

    #[test]
    fn multi_page_keeps_models_together() {
        // Each model needs most of a 128 page; model 0 has two items.
        let items = vec![
            item(0, 60, 100),
            item(0, 50, 100),
            item(1, 100, 100),
            item(2, 40, 40),
        ];
        let o = opts(128);
        let p = plan_layout(&o, &|_| items.clone()).unwrap();
        assert!(p.layout.pages.len() >= 2);
        let page_of = |i: usize| p.layout.placed[i].0;
        assert_eq!(page_of(0), page_of(1), "model 0 split across pages");
        assert_ne!(page_of(0), page_of(2));
        for (i, (page, r)) in p.layout.placed.iter().enumerate() {
            let size = p.layout.pages[*page];
            assert_eq!((r.width, r.height), (items[i].w, items[i].h));
            assert!(r.x + r.width <= size.width && r.y + r.height <= size.height);
        }

        let no_multi = PackOptions {
            multi_page: false,
            ..o.clone()
        };
        let err = plan_layout(&no_multi, &|_| items.clone()).unwrap_err();
        assert_eq!(err.code, atlas::codes::ATLAS_DOES_NOT_FIT);
        assert!(err.params["name"].as_str().unwrap().starts_with("m"));
    }

    #[test]
    fn model_larger_than_a_page_gets_exclusive_pages() {
        let items = vec![item(0, 100, 100), item(0, 100, 100), item(1, 20, 20)];
        let p = plan_layout(&opts(128), &|_| items.clone()).unwrap();
        assert_eq!(p.layout.pages.len(), 3);
        assert_ne!(p.layout.placed[0].0, p.layout.placed[1].0);
    }

    #[test]
    fn scale_to_fit_halves_until_one_page() {
        let o = PackOptions {
            scale_to_fit: true,
            ..opts(128)
        };
        let make = |s: f64| vec![item(0, (200.0 * s) as u32, (200.0 * s) as u32)];
        let p = plan_layout(&o, &make).unwrap();
        assert_eq!(p.divisor, 2);
        assert_eq!(p.items[0].w, 100);
        assert_eq!(p.layout.pages.len(), 1);
    }
}
