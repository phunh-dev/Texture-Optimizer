//! Deterministic image generators for tests (no binary fixture files).

use image::Rgba;

use crate::ImageBuf;

pub const TRANSPARENT: Rgba<u8> = Rgba([0, 0, 0, 0]);
pub const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);
pub const BLACK: Rgba<u8> = Rgba([0, 0, 0, 255]);
pub const RED: Rgba<u8> = Rgba([255, 0, 0, 255]);
pub const GREEN: Rgba<u8> = Rgba([0, 255, 0, 255]);
pub const BLUE: Rgba<u8> = Rgba([0, 0, 255, 255]);

pub fn solid(w: u32, h: u32, color: Rgba<u8>) -> ImageBuf {
    ImageBuf::from_pixel(w, h, color)
}

/// `bg` canvas with an `fg` rectangle at (x, y, rw, rh), clipped to the canvas.
#[allow(clippy::too_many_arguments)]
pub fn rect_on(
    w: u32,
    h: u32,
    bg: Rgba<u8>,
    x: u32,
    y: u32,
    rw: u32,
    rh: u32,
    fg: Rgba<u8>,
) -> ImageBuf {
    let mut img = solid(w, h, bg);
    for py in y..(y + rh).min(h) {
        for px in x..(x + rw).min(w) {
            img.put_pixel(px, py, fg);
        }
    }
    img
}

/// Sprite with transparent margins: `fg` content inside a transparent canvas.
pub fn sprite(w: u32, h: u32, x: u32, y: u32, rw: u32, rh: u32, fg: Rgba<u8>) -> ImageBuf {
    rect_on(w, h, TRANSPARENT, x, y, rw, rh, fg)
}

/// Checkerboard of two colors with square cells of `cell` pixels.
pub fn checker(w: u32, h: u32, cell: u32, a: Rgba<u8>, b: Rgba<u8>) -> ImageBuf {
    ImageBuf::from_fn(w, h, |x, y| {
        if ((x / cell) + (y / cell)).is_multiple_of(2) {
            a
        } else {
            b
        }
    })
}

/// Opaque gradient: red varies with x, green with y.
pub fn gradient(w: u32, h: u32) -> ImageBuf {
    let dx = w.saturating_sub(1).max(1);
    let dy = h.saturating_sub(1).max(1);
    ImageBuf::from_fn(w, h, |x, y| {
        Rgba([((x * 255) / dx) as u8, ((y * 255) / dy) as u8, 128, 255])
    })
}
