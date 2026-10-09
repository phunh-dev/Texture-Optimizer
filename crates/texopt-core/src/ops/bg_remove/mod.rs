//! Background remover: white / checkerboard / picked color / auto-detected,
//! flood-filled from the edges or removed globally, with feather and defringe.

mod color;
mod detect;
mod edt;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

use self::color::{Point, dist, threshold, to_point};
use self::detect::Checker;
use super::OpOutput;
use super::common::Color;
use crate::{ImageBuf, OpError, OpResult};

/// Checker mode was requested (or a cell size forced) but the border does not
/// contain a two-color checkerboard.
pub const BG_CHECKER_NOT_DETECTED: &str = "BG_CHECKER_NOT_DETECTED";

/// Edge pixels within this distance (px) of removed background are defringed.
const DEFRINGE_BAND: f64 = 2.0;
/// Search radius (px) for an interior pixel used as the pure foreground color.
const DEFRINGE_SEARCH: i64 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BgMode {
    /// Detect from the border: checkerboard if found, else the dominant color.
    #[default]
    Auto,
    White,
    Checker,
    /// Uses [`BgRemoveParams::color`].
    Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum FillMode {
    /// Only background connected to the image border (4-connectivity).
    #[default]
    FloodFromEdges,
    /// Every matching pixel.
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ColorMetric {
    /// Euclidean RGB; tolerance is a percentage of the black↔white distance.
    #[default]
    Rgb,
    /// CIE76 ΔE in L*a*b*; tolerance is in ΔE units.
    Lab,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BgRemoveParams {
    pub mode: BgMode,
    /// Background color for [`BgMode::Color`] (alpha ignored).
    pub color: Color,
    /// Checker cell size in px; `None` auto-detects it.
    pub checker_cell_size: Option<u32>,
    pub fill: FillMode,
    /// 0–100 match threshold, see [`ColorMetric`].
    pub tolerance: f32,
    pub metric: ColorMetric,
    /// Width (px) of the soft alpha ramp inside the foreground boundary; 0 = hard edge.
    pub feather: u32,
    /// Un-blend the background color out of edge pixels (halo removal).
    pub defringe: bool,
    /// 0–100 blend between original (0) and fully un-blended (100) edge pixels.
    pub defringe_strength: f32,
}

impl Default for BgRemoveParams {
    fn default() -> Self {
        Self {
            mode: BgMode::Auto,
            color: Color([255, 255, 255, 255]),
            checker_cell_size: None,
            fill: FillMode::FloodFromEdges,
            tolerance: 10.0,
            metric: ColorMetric::Rgb,
            feather: 0,
            defringe: false,
            defringe_strength: 100.0,
        }
    }
}

/// What the pixels are compared against.
#[derive(Debug, Clone, Copy)]
enum Background {
    /// Nothing to remove (e.g. auto mode on an already transparent border).
    None,
    Solid([u8; 3]),
    Checker(Checker),
}

impl Background {
    /// Background color expected behind pixel (x, y).
    fn expected(&self, x: u32, y: u32) -> Option<[u8; 3]> {
        match self {
            Background::None => None,
            Background::Solid(c) => Some(*c),
            Background::Checker(ch) => Some(ch.expected(x, y)),
        }
    }

    fn colors(&self) -> Vec<[u8; 3]> {
        match self {
            Background::None => vec![],
            Background::Solid(c) => vec![*c],
            Background::Checker(ch) => ch.colors.to_vec(),
        }
    }
}

fn validate(p: &BgRemoveParams) -> OpResult<()> {
    let in_range = |v: f32| v.is_finite() && (0.0..=100.0).contains(&v);
    if !in_range(p.tolerance) {
        return Err(OpError::invalid_param("tolerance", "outOfRange"));
    }
    if !in_range(p.defringe_strength) {
        return Err(OpError::invalid_param("defringeStrength", "outOfRange"));
    }
    if p.checker_cell_size == Some(0) {
        return Err(OpError::invalid_param("checkerCellSize", "outOfRange"));
    }
    Ok(())
}

fn resolve_background(img: &ImageBuf, p: &BgRemoveParams) -> OpResult<(Background, &'static str)> {
    let rgb = |c: Color| [c.0[0], c.0[1], c.0[2]];
    Ok(match p.mode {
        BgMode::White => (Background::Solid([255; 3]), "white"),
        BgMode::Color => (Background::Solid(rgb(p.color)), "color"),
        BgMode::Checker => match detect::detect_checker(img, p.checker_cell_size) {
            Some(ch) => (Background::Checker(ch), "checker"),
            None => return Err(OpError::new(BG_CHECKER_NOT_DETECTED)),
        },
        BgMode::Auto => {
            if let Some(ch) = detect::detect_checker(img, p.checker_cell_size) {
                (Background::Checker(ch), "checker")
            } else if let Some(c) = detect::dominant_border_color(img) {
                let near_white = color::rgb_dist(c, [255; 3]) <= 24.0;
                (
                    Background::Solid(c),
                    if near_white { "white" } else { "color" },
                )
            } else {
                (Background::None, "none")
            }
        }
    })
}

pub fn apply(img: &ImageBuf, params: &BgRemoveParams) -> OpResult<OpOutput> {
    validate(params)?;
    let (w, h) = img.dimensions();
    let (bg, detected_mode) = if w == 0 || h == 0 {
        (Background::None, "none")
    } else {
        resolve_background(img, params)?
    };

    let mut out = img.clone();
    let removed = background_mask(img, &bg, params);
    let removed_count = removed.iter().filter(|&&r| r).count();

    if removed_count > 0 {
        let needs_edt = params.feather > 0 || (params.defringe && params.defringe_strength > 0.0);
        let dist_sq = needs_edt.then(|| edt::squared_distance(w as usize, h as usize, &removed));
        if params.defringe
            && params.defringe_strength > 0.0
            && let Some(d) = &dist_sq
        {
            defringe(
                img,
                &mut out,
                &bg,
                &removed,
                d,
                params.defringe_strength / 100.0,
            );
        }
        if params.feather > 0
            && let Some(d) = &dist_sq
        {
            feather(&mut out, &removed, d, params.feather);
        }
        for (px, &r) in out.pixels_mut().zip(&removed) {
            if r {
                px.0 = [0, 0, 0, 0];
            }
        }
    }

    let meta = json!({
        "removedPixels": removed_count,
        "detectedMode": detected_mode,
        "checkerCellSize": match bg { Background::Checker(ch) => Some(ch.cell), _ => None },
        "bgColors": bg.colors().iter().map(|c| [c[0], c[1], c[2], 255]).collect::<Vec<_>>(),
    });
    Ok(OpOutput {
        image: out,
        meta: Some(meta),
    })
}

/// Pixels that become transparent (already-transparent pixels are never
/// counted, but flood fill may pass through them).
fn background_mask(img: &ImageBuf, bg: &Background, p: &BgRemoveParams) -> Vec<bool> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if matches!(bg, Background::None) {
        return vec![false; w * h];
    }
    let thr = threshold(p.metric, p.tolerance) + 1e-3;
    let bg_points: Vec<Point> = bg.colors().iter().map(|&c| to_point(p.metric, c)).collect();
    let raw = img.as_raw();

    // passable[i]: pixel matches the background (or is already fully transparent).
    let mut passable = vec![false; w * h];
    passable.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, slot) in row.iter_mut().enumerate() {
            let i = (y * w + x) * 4;
            let px = &raw[i..i + 4];
            if px[3] == 0 {
                *slot = true;
                continue;
            }
            let c = to_point(p.metric, [px[0], px[1], px[2]]);
            *slot = match (bg, p.fill) {
                (Background::Checker(ch), FillMode::Global) => {
                    dist(c, bg_points[ch.parity(x as u32, y as u32)]) <= thr
                }
                _ => bg_points.iter().any(|&b| dist(c, b) <= thr),
            };
        }
    });

    let region = match p.fill {
        FillMode::Global => passable,
        FillMode::FloodFromEdges => flood_from_edges(w, h, &passable),
    };
    region
        .iter()
        .zip(img.pixels())
        .map(|(&r, px)| r && px[3] > 0)
        .collect()
}

fn flood_from_edges(w: usize, h: usize, passable: &[bool]) -> Vec<bool> {
    let mut seen = vec![false; w * h];
    let mut stack: Vec<usize> = Vec::new();
    let push = |i: usize, seen: &mut Vec<bool>, stack: &mut Vec<usize>| {
        if passable[i] && !seen[i] {
            seen[i] = true;
            stack.push(i);
        }
    };
    for x in 0..w {
        push(x, &mut seen, &mut stack);
        push((h - 1) * w + x, &mut seen, &mut stack);
    }
    for y in 0..h {
        push(y * w, &mut seen, &mut stack);
        push(y * w + w - 1, &mut seen, &mut stack);
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        if x > 0 {
            push(i - 1, &mut seen, &mut stack);
        }
        if x + 1 < w {
            push(i + 1, &mut seen, &mut stack);
        }
        if y > 0 {
            push(i - w, &mut seen, &mut stack);
        }
        if y + 1 < h {
            push(i + w, &mut seen, &mut stack);
        }
    }
    seen
}

/// Linear alpha ramp over the first `width` px of foreground next to removed background.
fn feather(out: &mut ImageBuf, removed: &[bool], dist_sq: &[f64], width: u32) {
    let width = width as f64;
    for ((px, &r), &d2) in out.pixels_mut().zip(removed).zip(dist_sq) {
        if r || px[3] == 0 {
            continue;
        }
        let d = d2.sqrt();
        if d <= width {
            px[3] = (px[3] as f64 * d / (width + 1.0)).round() as u8;
        }
    }
}

/// Un-blends the background from edge pixels: estimates coverage `a` and the
/// pure foreground color `F = (C - (1 - a) * B) / a`.
fn defringe(
    src: &ImageBuf,
    out: &mut ImageBuf,
    bg: &Background,
    removed: &[bool],
    dist_sq: &[f64],
    strength: f32,
) {
    let (w, h) = (src.width() as i64, src.height() as i64);
    let band_sq = DEFRINGE_BAND * DEFRINGE_BAND;
    let idx = |x: i64, y: i64| (y * w + x) as usize;
    let rgb = |x: i64, y: i64| {
        let p = src.get_pixel(x as u32, y as u32);
        [p[0] as f32, p[1] as f32, p[2] as f32]
    };
    let is_interior = |x: i64, y: i64| {
        let i = idx(x, y);
        !removed[i] && src.get_pixel(x as u32, y as u32)[3] > 0 && dist_sq[i] > band_sq
    };

    for y in 0..h {
        for x in 0..w {
            let i = idx(x, y);
            if removed[i] || dist_sq[i] > band_sq || src.get_pixel(x as u32, y as u32)[3] == 0 {
                continue;
            }
            let Some(b) = bg.expected(x as u32, y as u32) else {
                continue;
            };
            let b = b.map(|v| v as f32);
            let c = rgb(x, y);

            // Nearest interior pixel gives the pure foreground color.
            let mut best: Option<(i64, [f32; 3])> = None;
            for dy in -DEFRINGE_SEARCH..=DEFRINGE_SEARCH {
                for dx in -DEFRINGE_SEARCH..=DEFRINGE_SEARCH {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w || ny >= h || !is_interior(nx, ny) {
                        continue;
                    }
                    let d = dx * dx + dy * dy;
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, rgb(nx, ny)));
                    }
                }
            }
            let a = match best {
                Some((_, f)) => {
                    let v = [f[0] - b[0], f[1] - b[1], f[2] - b[2]];
                    let len_sq = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
                    if len_sq < 1.0 {
                        continue; // foreground indistinguishable from background
                    }
                    let proj = (c[0] - b[0]) * v[0] + (c[1] - b[1]) * v[1] + (c[2] - b[2]) * v[2];
                    (proj / len_sq).clamp(0.0, 1.0)
                }
                None => color_to_alpha(c, b),
            };

            let px = out.get_pixel_mut(x as u32, y as u32);
            let orig_alpha = px[3] as f32;
            let unblended: [f32; 3] = if a > 1.0 / 255.0 {
                std::array::from_fn(|k| ((c[k] - (1.0 - a) * b[k]) / a).clamp(0.0, 255.0))
            } else {
                c
            };
            for k in 0..3 {
                px[k] = (c[k] + strength * (unblended[k] - c[k])).round() as u8;
            }
            px[3] = (orig_alpha * (1.0 - strength + strength * a)).round() as u8;
        }
    }
}

/// Smallest coverage that can explain `c` as some color blended over `b`
/// (GIMP "color to alpha"), used when no interior reference pixel is nearby.
fn color_to_alpha(c: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3)
        .map(|k| {
            let d = c[k] - b[k];
            if d > 0.0 {
                d / (255.0 - b[k]).max(1.0)
            } else if d < 0.0 {
                -d / b[k].max(1.0)
            } else {
                0.0
            }
        })
        .fold(0.0, f32::max)
        .clamp(0.0, 1.0)
}
