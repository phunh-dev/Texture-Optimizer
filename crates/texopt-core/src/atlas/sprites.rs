//! Sprite preparation (name clean-up, hashing, dedupe, trimming, sorting) and
//! page composition (blit, rotation, extrusion, premultiplication).

use std::collections::BTreeMap;

use image::imageops;
use rayon::prelude::*;

use super::packer::Rect;
use super::params::{AtlasParams, SortBy};
use super::{SpriteInput, codes};
use crate::error::codes as core_codes;
use crate::{ImageBuf, OpError, OpResult};

/// A unique sprite ready for packing.
#[derive(Debug, Clone)]
pub(crate) struct Prepared {
    pub name: String,
    pub aliases: Vec<String>,
    pub hash: String,
    /// Trimmed, unrotated pixels.
    pub image: ImageBuf,
    pub source_w: u32,
    pub source_h: u32,
    /// Trimmed rect inside the source image (`spriteSourceSize`).
    pub trim: Rect,
    /// Input position of the primary name (used by `sortBy: none`).
    pub order: usize,
}

impl Prepared {
    pub fn w(&self) -> u32 {
        self.trim.w
    }
    pub fn h(&self) -> u32 {
        self.trim.h
    }
    pub fn trimmed(&self) -> bool {
        self.trim.w != self.source_w || self.trim.h != self.source_h
    }
    pub fn names(&self) -> impl Iterator<Item = &String> {
        std::iter::once(&self.name).chain(self.aliases.iter())
    }
}

/// blake3 over dimensions + raw RGBA bytes, as lowercase hex.
pub(crate) fn hash_image(img: &ImageBuf) -> String {
    let mut h = blake3::Hasher::new();
    h.update(&img.width().to_le_bytes());
    h.update(&img.height().to_le_bytes());
    h.update(img.as_raw());
    h.finalize().to_hex().to_string()
}

/// Bounding box of pixels with `alpha > threshold`; a fully transparent image
/// trims to its top-left pixel (1x1).
pub(crate) fn trim_rect(img: &ImageBuf, threshold: u8) -> Rect {
    let (w, h) = img.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for (x, y, p) in img.enumerate_pixels() {
        if p.0[3] > threshold {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    if x0 == u32::MAX {
        return Rect::new(0, 0, 1.min(w), 1.min(h));
    }
    Rect::new(x0, y0, x1 - x0 + 1, y1 - y0 + 1)
}

pub(crate) fn prepare(
    sprites: Vec<SpriteInput>,
    params: &AtlasParams,
) -> OpResult<(Vec<Prepared>, Vec<OpError>)> {
    let mut warnings = Vec::new();
    // 1. Unique names: a later sprite with the same name replaces the earlier one.
    let mut unique: Vec<SpriteInput> = Vec::with_capacity(sprites.len());
    let mut pos: BTreeMap<String, usize> = BTreeMap::new();
    for s in sprites {
        if s.name.is_empty() {
            return Err(OpError::invalid_param("name", "empty"));
        }
        if s.image.width() == 0 || s.image.height() == 0 {
            return Err(OpError::new(core_codes::IMG_EMPTY).with("name", s.name.clone()));
        }
        match pos.get(&s.name) {
            Some(&i) => {
                warnings
                    .push(OpError::new(codes::ATLAS_DUPLICATE_NAME).with("name", s.name.clone()));
                unique[i] = s;
            }
            None => {
                pos.insert(s.name.clone(), unique.len());
                unique.push(s);
            }
        }
    }
    if unique.is_empty() {
        return Err(OpError::new(codes::ATLAS_EMPTY));
    }

    // 2. Hash every source image.
    let hashes: Vec<String> = unique.par_iter().map(|s| hash_image(&s.image)).collect();

    // 3. Group identical images (or keep every sprite on its own).
    let mut groups: Vec<Vec<usize>> = Vec::new();
    if params.dedupe {
        let mut by_hash: BTreeMap<&str, usize> = BTreeMap::new();
        for (i, h) in hashes.iter().enumerate() {
            match by_hash.get(h.as_str()) {
                Some(&g) => groups[g].push(i),
                None => {
                    by_hash.insert(h, groups.len());
                    groups.push(vec![i]);
                }
            }
        }
    } else {
        groups = (0..unique.len()).map(|i| vec![i]).collect();
    }

    // 4. Trim the primary of each group.
    let mut prepared: Vec<Prepared> = groups
        .par_iter()
        .map(|g| {
            let mut members: Vec<usize> = g.clone();
            members.sort_by(|&a, &b| unique[a].name.cmp(&unique[b].name));
            let primary = members[0];
            let src = &unique[primary].image;
            let trim = if params.trim {
                trim_rect(src, params.trim_threshold)
            } else {
                Rect::new(0, 0, src.width(), src.height())
            };
            let image = if trim.w == src.width() && trim.h == src.height() {
                src.clone()
            } else {
                imageops::crop_imm(src, trim.x, trim.y, trim.w, trim.h).to_image()
            };
            Prepared {
                name: unique[primary].name.clone(),
                aliases: members[1..]
                    .iter()
                    .map(|&i| unique[i].name.clone())
                    .collect(),
                hash: hashes[primary].clone(),
                image,
                source_w: src.width(),
                source_h: src.height(),
                trim,
                order: *g.iter().min().unwrap_or(&primary),
            }
        })
        .collect();

    // 5. Sort for packing.
    sort_prepared(&mut prepared, params.sort_by);
    Ok((prepared, warnings))
}

fn sort_prepared(v: &mut [Prepared], by: SortBy) {
    use std::cmp::Reverse;
    match by {
        SortBy::Area => v.sort_by(|a, b| {
            (Reverse(u64::from(a.w()) * u64::from(a.h())), &a.name)
                .cmp(&(Reverse(u64::from(b.w()) * u64::from(b.h())), &b.name))
        }),
        SortBy::MaxSide => v.sort_by(|a, b| {
            (
                Reverse(a.w().max(a.h())),
                Reverse(a.w().min(a.h())),
                &a.name,
            )
                .cmp(&(
                    Reverse(b.w().max(b.h())),
                    Reverse(b.w().min(b.h())),
                    &b.name,
                ))
        }),
        SortBy::Height => v.sort_by(|a, b| {
            (Reverse(a.h()), Reverse(a.w()), &a.name).cmp(&(
                Reverse(b.h()),
                Reverse(b.w()),
                &b.name,
            ))
        }),
        SortBy::Width => v.sort_by(|a, b| {
            (Reverse(a.w()), Reverse(a.h()), &a.name).cmp(&(
                Reverse(b.w()),
                Reverse(b.h()),
                &b.name,
            ))
        }),
        SortBy::Name => v.sort_by(|a, b| a.name.cmp(&b.name)),
        SortBy::None => v.sort_by_key(|p| p.order),
    }
}

/// Rotate 90 degrees clockwise (the convention stored as `rotated: true`).
pub(crate) fn oriented(img: &ImageBuf, rotated: bool) -> ImageBuf {
    if rotated {
        imageops::rotate90(img)
    } else {
        img.clone()
    }
}

/// Copy `src` to `(fx, fy)` and replicate its edge pixels `extrude` pixels outward.
pub(crate) fn blit_extruded(page: &mut ImageBuf, src: &ImageBuf, fx: u32, fy: u32, extrude: u32) {
    let (w, h) = src.dimensions();
    let e = i64::from(extrude);
    for dy in -e..i64::from(h) + e {
        let sy = dy.clamp(0, i64::from(h) - 1) as u32;
        let py = i64::from(fy) + dy;
        if py < 0 || py >= i64::from(page.height()) {
            continue;
        }
        for dx in -e..i64::from(w) + e {
            let px = i64::from(fx) + dx;
            if px < 0 || px >= i64::from(page.width()) {
                continue;
            }
            let sx = dx.clamp(0, i64::from(w) - 1) as u32;
            page.put_pixel(px as u32, py as u32, *src.get_pixel(sx, sy));
        }
    }
}

pub(crate) fn premultiply(img: &mut ImageBuf) {
    for p in img.pixels_mut() {
        let a = u32::from(p.0[3]);
        for c in &mut p.0[..3] {
            *c = ((u32::from(*c) * a + 127) / 255) as u8;
        }
    }
}
