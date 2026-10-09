//! Separable convolution resampler working in `f32`.
//!
//! Pipeline for every filter except `Nearest`:
//! 1. u8 -> f32 (optionally sRGB -> linear light for RGB, alpha stays linear);
//! 2. optionally premultiply RGB by alpha so fully transparent pixels (whose
//!    RGB is often black) do not bleed into visible edges;
//! 3. horizontal pass, then vertical pass, with normalized filter weights
//!    (support widened by the scale factor when downscaling);
//! 4. un-premultiply, clamp, back to sRGB if needed, round to u8.
//!
//! `Nearest` copies source pixels directly, so it never creates new colors
//! (the linear/premultiply options are irrelevant to it).

use rayon::prelude::*;

use super::ResampleFilter;
use crate::ImageBuf;

/// Options controlling [`resample`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResampleOptions {
    pub filter: ResampleFilter,
    /// Filter RGB in linear light instead of sRGB-encoded values.
    pub linear_space: bool,
    /// Premultiply RGB by alpha while filtering (prevents dark fringes).
    pub premultiply_alpha: bool,
}

impl Default for ResampleOptions {
    fn default() -> Self {
        Self {
            filter: ResampleFilter::Lanczos3,
            linear_space: false,
            premultiply_alpha: true,
        }
    }
}

/// sRGB-encoded value (0..1) to linear light (0..1).
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light (0..1) to sRGB-encoded value (0..1).
pub fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// Resample `img` to `w` x `h`. Same size returns an exact copy; a zero-sized
/// source yields a transparent image.
pub fn resample(img: &ImageBuf, w: u32, h: u32, opts: &ResampleOptions) -> ImageBuf {
    let (sw, sh) = img.dimensions();
    if (sw, sh) == (w, h) {
        return img.clone();
    }
    if w == 0 || h == 0 || sw == 0 || sh == 0 {
        return ImageBuf::new(w, h);
    }
    if opts.filter == ResampleFilter::Nearest {
        return nearest(img, w, h);
    }
    let src = to_float(img, opts);
    let tmp = if sw != w {
        horizontal(&src, sw as usize, w as usize, opts.filter)
    } else {
        src
    };
    let out = if sh != h {
        vertical(&tmp, w as usize, sh as usize, h as usize, opts.filter)
    } else {
        tmp
    };
    from_float(&out, w, h, opts)
}

fn nearest(img: &ImageBuf, w: u32, h: u32) -> ImageBuf {
    // Sample at destination pixel centers: src = floor((d + 0.5) * src / dst).
    let map = |dst: u32, src: u32| -> Vec<u32> {
        (0..dst as u64)
            .map(|d| (((2 * d + 1) * src as u64) / (2 * dst as u64)).min(src as u64 - 1) as u32)
            .collect()
    };
    let xs = map(w, img.width());
    let ys = map(h, img.height());
    ImageBuf::from_fn(w, h, |x, y| *img.get_pixel(xs[x as usize], ys[y as usize]))
}

type Px = [f32; 4];

fn to_float(img: &ImageBuf, opts: &ResampleOptions) -> Vec<Px> {
    let lut: [f32; 256] = std::array::from_fn(|i| {
        let v = i as f32 / 255.0;
        if opts.linear_space {
            srgb_to_linear(v)
        } else {
            v
        }
    });
    img.as_raw()
        .par_chunks_exact(4)
        .map(|p| {
            let a = p[3] as f32 / 255.0;
            let (r, g, b) = (lut[p[0] as usize], lut[p[1] as usize], lut[p[2] as usize]);
            if opts.premultiply_alpha {
                [r * a, g * a, b * a, a]
            } else {
                [r, g, b, a]
            }
        })
        .collect()
}

fn from_float(px: &[Px], w: u32, h: u32, opts: &ResampleOptions) -> ImageBuf {
    let encode = |v: f32| -> u8 {
        let v = v.clamp(0.0, 1.0);
        let v = if opts.linear_space {
            linear_to_srgb(v)
        } else {
            v
        };
        (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8
    };
    let raw: Vec<u8> = px
        .par_iter()
        .flat_map_iter(|&[r, g, b, a]| {
            let a = a.clamp(0.0, 1.0);
            let (r, g, b) = if opts.premultiply_alpha {
                if a > 0.0 {
                    (r / a, g / a, b / a)
                } else {
                    (0.0, 0.0, 0.0)
                }
            } else {
                (r, g, b)
            };
            [
                encode(r),
                encode(g),
                encode(b),
                (a * 255.0 + 0.5).clamp(0.0, 255.0) as u8,
            ]
        })
        .collect();
    ImageBuf::from_raw(w, h, raw).expect("buffer size matches dimensions")
}

/// Contribution of a contiguous run of source samples to one destination sample.
struct Contrib {
    start: usize,
    weights: Vec<f32>,
}

fn support(filter: ResampleFilter) -> f64 {
    match filter {
        ResampleFilter::Nearest => 0.5,
        ResampleFilter::Bilinear => 1.0,
        ResampleFilter::CatmullRom | ResampleFilter::Mitchell => 2.0,
        ResampleFilter::Lanczos3 => 3.0,
    }
}

fn kernel(filter: ResampleFilter, x: f64) -> f64 {
    let x = x.abs();
    match filter {
        ResampleFilter::Nearest => {
            if x <= 0.5 {
                1.0
            } else {
                0.0
            }
        }
        ResampleFilter::Bilinear => (1.0 - x).max(0.0),
        ResampleFilter::CatmullRom => bc_cubic(x, 0.0, 0.5),
        ResampleFilter::Mitchell => bc_cubic(x, 1.0 / 3.0, 1.0 / 3.0),
        ResampleFilter::Lanczos3 => {
            if x < 3.0 {
                sinc(x) * sinc(x / 3.0)
            } else {
                0.0
            }
        }
    }
}

/// Mitchell–Netravali family of cubics, `x >= 0`.
fn bc_cubic(x: f64, b: f64, c: f64) -> f64 {
    let (x2, x3) = (x * x, x * x * x);
    if x < 1.0 {
        ((12.0 - 9.0 * b - 6.0 * c) * x3 + (-18.0 + 12.0 * b + 6.0 * c) * x2 + (6.0 - 2.0 * b))
            / 6.0
    } else if x < 2.0 {
        ((-b - 6.0 * c) * x3
            + (6.0 * b + 30.0 * c) * x2
            + (-12.0 * b - 48.0 * c) * x
            + (8.0 * b + 24.0 * c))
            / 6.0
    } else {
        0.0
    }
}

fn sinc(x: f64) -> f64 {
    if x == 0.0 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

fn contributions(src: usize, dst: usize, filter: ResampleFilter) -> Vec<Contrib> {
    let scale = src as f64 / dst as f64;
    let fscale = scale.max(1.0);
    let radius = support(filter) * fscale;
    (0..dst)
        .map(|i| {
            let center = (i as f64 + 0.5) * scale;
            let left = ((center - radius).floor().max(0.0) as usize).min(src - 1);
            let right = ((center + radius).ceil() as usize).clamp(left + 1, src);
            let raw: Vec<f64> = (left..right)
                .map(|j| kernel(filter, (j as f64 + 0.5 - center) / fscale))
                .collect();
            let sum: f64 = raw.iter().sum();
            if sum.abs() < 1e-12 {
                let j = (center.floor() as usize).min(src - 1);
                return Contrib {
                    start: j,
                    weights: vec![1.0],
                };
            }
            Contrib {
                start: left,
                weights: raw.iter().map(|w| (w / sum) as f32).collect(),
            }
        })
        .collect()
}

fn horizontal(src: &[Px], sw: usize, dw: usize, filter: ResampleFilter) -> Vec<Px> {
    let contribs = contributions(sw, dw, filter);
    let rows = src.len() / sw;
    let mut out = vec![[0.0f32; 4]; dw * rows];
    out.par_chunks_mut(dw)
        .zip(src.par_chunks(sw))
        .for_each(|(row, srow)| {
            for (d, c) in row.iter_mut().zip(&contribs) {
                let mut acc = [0.0f32; 4];
                for (s, &wt) in srow[c.start..c.start + c.weights.len()]
                    .iter()
                    .zip(&c.weights)
                {
                    for k in 0..4 {
                        acc[k] += s[k] * wt;
                    }
                }
                *d = acc;
            }
        });
    out
}

fn vertical(src: &[Px], w: usize, sh: usize, dh: usize, filter: ResampleFilter) -> Vec<Px> {
    let contribs = contributions(sh, dh, filter);
    let mut out = vec![[0.0f32; 4]; w * dh];
    out.par_chunks_mut(w)
        .zip(contribs.par_iter())
        .for_each(|(row, c)| {
            for (j, &wt) in c.weights.iter().enumerate() {
                let srow = &src[(c.start + j) * w..(c.start + j + 1) * w];
                for (d, s) in row.iter_mut().zip(srow) {
                    for k in 0..4 {
                        d[k] += s[k] * wt;
                    }
                }
            }
        });
    out
}
