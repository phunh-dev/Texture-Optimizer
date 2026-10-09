//! Pixel work: resampling a channel texture to its block size, baking
//! repeats and drawing blocks (with extruded edges) onto atlas pages.

use image::Rgba;
use image::imageops::{self, FilterType};

use crate::ImageBuf;

/// Resample `img` to `size` (no-op when it already has that size).
pub(super) fn fit(img: &ImageBuf, size: [u32; 2]) -> ImageBuf {
    if img.dimensions() == (size[0], size[1]) {
        return img.clone();
    }
    imageops::resize(img, size[0], size[1], FilterType::Triangle)
}

/// `tile` repeated `tiles[0] x tiles[1]` times.
pub(super) fn repeat(tile: &ImageBuf, tiles: [u32; 2]) -> ImageBuf {
    if tiles == [1, 1] {
        return tile.clone();
    }
    let (w, h) = tile.dimensions();
    let mut out = ImageBuf::new(w * tiles[0], h * tiles[1]);
    for ty in 0..tiles[1] {
        for tx in 0..tiles[0] {
            imageops::replace(&mut out, tile, i64::from(tx * w), i64::from(ty * h));
        }
    }
    out
}

pub(super) fn solid(size: [u32; 2], color: Rgba<u8>) -> ImageBuf {
    ImageBuf::from_pixel(size[0], size[1], color)
}

/// Copy `src` to `(x, y)` and replicate its edge pixels `extrude` pixels
/// outward (same rule as the sprite atlas).
pub(super) fn blit_extruded(page: &mut ImageBuf, src: &ImageBuf, x: u32, y: u32, extrude: u32) {
    let (w, h) = src.dimensions();
    let e = i64::from(extrude);
    for dy in -e..i64::from(h) + e {
        let py = i64::from(y) + dy;
        if py < 0 || py >= i64::from(page.height()) {
            continue;
        }
        let sy = dy.clamp(0, i64::from(h) - 1) as u32;
        for dx in -e..i64::from(w) + e {
            let px = i64::from(x) + dx;
            if px < 0 || px >= i64::from(page.width()) {
                continue;
            }
            let sx = dx.clamp(0, i64::from(w) - 1) as u32;
            page.put_pixel(px as u32, py as u32, *src.get_pixel(sx, sy));
        }
    }
}

/// Downscale for previews so the longest edge is at most `max_edge`.
pub fn preview_downscale(img: &ImageBuf, max_edge: u32) -> ImageBuf {
    let (w, h) = img.dimensions();
    let longest = w.max(h);
    if max_edge == 0 || longest <= max_edge {
        return img.clone();
    }
    let k = f64::from(max_edge) / f64::from(longest);
    let nw = ((f64::from(w) * k).round() as u32).max(1);
    let nh = ((f64::from(h) * k).round() as u32).max(1);
    imageops::resize(img, nw, nh, FilterType::Triangle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{BLUE, RED, rect_on};

    #[test]
    fn repeat_and_fit() {
        let tile = rect_on(4, 2, RED, 0, 0, 2, 2, BLUE);
        let r = repeat(&tile, [3, 2]);
        assert_eq!(r.dimensions(), (12, 4));
        assert_eq!(*r.get_pixel(4, 3), BLUE);
        assert_eq!(*r.get_pixel(7, 0), RED);
        assert_eq!(fit(&solid([2, 2], RED), [5, 3]).dimensions(), (5, 3));
        assert_eq!(*fit(&solid([2, 2], RED), [5, 3]).get_pixel(4, 2), RED);
    }

    #[test]
    fn extrusion_replicates_edges_and_clips() {
        let mut page = ImageBuf::new(8, 8);
        let src = rect_on(2, 2, RED, 1, 0, 1, 2, BLUE);
        blit_extruded(&mut page, &src, 1, 1, 2);
        assert_eq!(*page.get_pixel(0, 0), RED); // extruded corner (clipped at -1)
        assert_eq!(*page.get_pixel(4, 4), BLUE); // extruded right edge
        assert_eq!(page.get_pixel(6, 6).0[3], 0); // untouched
    }

    #[test]
    fn preview_is_capped() {
        let img = solid([300, 100], RED);
        assert_eq!(preview_downscale(&img, 150).dimensions(), (150, 50));
        assert_eq!(preview_downscale(&img, 1000).dimensions(), (300, 100));
    }
}
