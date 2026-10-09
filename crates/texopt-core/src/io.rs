//! Image loading/saving and output path resolution.

use std::path::Path;

use crate::error::codes;
use crate::{ImageBuf, OpError, OpResult};

/// File extensions the app can import (lowercase, without dot).
pub const SUPPORTED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "tga", "bmp", "webp"];

pub fn load_image(path: &Path) -> OpResult<ImageBuf> {
    let img = image::open(path).map_err(|e| {
        OpError::new(codes::IMG_DECODE_FAILED)
            .with("path", path.display().to_string())
            .with("detail", e.to_string())
    })?;
    Ok(img.into_rgba8())
}
