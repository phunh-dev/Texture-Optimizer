//! Color distance helpers: plain RGB Euclidean distance or CIE76 ΔE in Lab.

use std::sync::OnceLock;

use super::ColorMetric;

/// Largest possible RGB Euclidean distance (black ↔ white).
pub const MAX_RGB_DIST: f32 = 441.672_96;

/// Color converted into the space a [`ColorMetric`] measures distances in.
pub type Point = [f32; 3];

fn srgb_lut() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut lut = [0f32; 256];
        for (i, v) in lut.iter_mut().enumerate() {
            let c = i as f32 / 255.0;
            *v = if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            };
        }
        lut
    })
}

/// sRGB (D65) → CIE L*a*b*.
pub fn rgb_to_lab(c: [u8; 3]) -> Point {
    let lut = srgb_lut();
    let (r, g, b) = (lut[c[0] as usize], lut[c[1] as usize], lut[c[2] as usize]);
    let x = (0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b) / 0.950_47;
    let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b;
    let z = (0.019_333_9 * r + 0.119_192 * g + 0.950_304_1 * b) / 1.088_83;
    let f = |t: f32| {
        const D: f32 = 6.0 / 29.0;
        if t > D * D * D {
            t.cbrt()
        } else {
            t / (3.0 * D * D) + 4.0 / 29.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

pub fn to_point(metric: ColorMetric, c: [u8; 3]) -> Point {
    match metric {
        ColorMetric::Rgb => [c[0] as f32, c[1] as f32, c[2] as f32],
        ColorMetric::Lab => rgb_to_lab(c),
    }
}

pub fn dist(a: Point, b: Point) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// Maps the 0–100 tolerance slider to a distance threshold:
/// a percentage of the max RGB distance, or ΔE units for Lab.
pub fn threshold(metric: ColorMetric, tolerance: f32) -> f32 {
    match metric {
        ColorMetric::Rgb => tolerance / 100.0 * MAX_RGB_DIST,
        ColorMetric::Lab => tolerance,
    }
}

pub fn rgb_dist(a: [u8; 3], b: [u8; 3]) -> f32 {
    dist(to_point(ColorMetric::Rgb, a), to_point(ColorMetric::Rgb, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lab_reference_values() {
        let w = rgb_to_lab([255, 255, 255]);
        assert!(
            (w[0] - 100.0).abs() < 0.01 && w[1].abs() < 0.01 && w[2].abs() < 0.01,
            "{w:?}"
        );
        let k = rgb_to_lab([0, 0, 0]);
        assert!(k[0].abs() < 0.01, "{k:?}");
        // sRGB red ≈ (53.24, 80.09, 67.20)
        let r = rgb_to_lab([255, 0, 0]);
        assert!(
            (r[0] - 53.24).abs() < 0.1 && (r[1] - 80.09).abs() < 0.2 && (r[2] - 67.2).abs() < 0.2,
            "{r:?}"
        );
    }
}
