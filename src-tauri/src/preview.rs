//! `preview_op` payload: the processed image as PNG plus its size and op
//! metadata, packed into one binary buffer so it reaches the webview as an
//! `ArrayBuffer` (no base64 / JSON number arrays).
//!
//! Layout (integers are little-endian u32):
//! `width | height | metaLen | meta (UTF-8 JSON, metaLen bytes; 0 = null) | png`
//! Decoded on the TS side by `decodePreviewPayload` in `src/lib/ipc/index.ts`.

use std::path::Path;

use texopt_core::error::codes;
use texopt_core::ops::OpOutput;
use texopt_core::output::{PngCompression, encode_png};
use texopt_core::{ImageBuf, OpError, OpResult};

use crate::session::SessionCache;

pub const PREVIEW_HEADER_LEN: usize = 12;

pub fn encode_preview(out: &OpOutput) -> OpResult<Vec<u8>> {
    let png = encode_png(&out.image, PngCompression::Fast).map_err(|e| {
        OpError::new(codes::IMG_ENCODE_FAILED)
            .with("path", "preview")
            .with("detail", e.to_string())
    })?;
    let meta = match &out.meta {
        Some(m) => serde_json::to_vec(m).unwrap_or_default(),
        None => Vec::new(),
    };
    let mut buf = Vec::with_capacity(PREVIEW_HEADER_LEN + meta.len() + png.len());
    buf.extend_from_slice(&out.image.width().to_le_bytes());
    buf.extend_from_slice(&out.image.height().to_le_bytes());
    buf.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    buf.extend_from_slice(&meta);
    buf.extend_from_slice(&png);
    Ok(buf)
}

/// Load `path` through the tab's session cache, run `op`, and pack the result.
pub fn render_preview(
    cache: &SessionCache,
    tab: &str,
    path: &Path,
    op: impl FnOnce(&ImageBuf) -> OpResult<OpOutput>,
) -> OpResult<Vec<u8>> {
    let img = cache.get_or_load(tab, path, texopt_core::io::load_image)?;
    encode_preview(&op(&img)?)
}

#[cfg(test)]
mod tests {
    use texopt_core::fixtures;

    use super::*;

    fn u32_at(buf: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(buf[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn packs_size_meta_and_png() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.png");
        fixtures::gradient(30, 20).save(&p).unwrap();
        let cache = SessionCache::default();
        let buf = render_preview(&cache, "tab", &p, |img| {
            let cropped = image::imageops::crop_imm(img, 0, 0, 10, 5).to_image();
            Ok(OpOutput {
                image: cropped,
                meta: Some(serde_json::json!({ "x": 1 })),
            })
        })
        .unwrap();
        assert_eq!((u32_at(&buf, 0), u32_at(&buf, 4)), (10, 5));
        let meta_len = u32_at(&buf, 8) as usize;
        let meta: serde_json::Value = serde_json::from_slice(&buf[12..12 + meta_len]).unwrap();
        assert_eq!(meta, serde_json::json!({ "x": 1 }));
        let png =
            image::load_from_memory_with_format(&buf[12 + meta_len..], image::ImageFormat::Png)
                .unwrap()
                .into_rgba8();
        assert_eq!(
            png,
            image::imageops::crop_imm(&fixtures::gradient(30, 20), 0, 0, 10, 5).to_image()
        );
        assert!(cache.contains("tab", &p));
    }

    #[test]
    fn no_meta_means_zero_length() {
        let buf = encode_preview(&OpOutput::image(fixtures::solid(2, 3, fixtures::RED))).unwrap();
        assert_eq!(
            (u32_at(&buf, 0), u32_at(&buf, 4), u32_at(&buf, 8)),
            (2, 3, 0)
        );
        assert!(image::load_from_memory(&buf[12..]).is_ok());
    }

    #[test]
    fn errors_propagate_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.png");
        fixtures::gradient(4, 4).save(&p).unwrap();
        let cache = SessionCache::default();
        let err = render_preview(&cache, "t", &p, |_| Err(OpError::new("BOOM").with("a", 1)))
            .unwrap_err();
        assert_eq!(err, OpError::new("BOOM").with("a", 1));
        let missing = render_preview(&cache, "t", &dir.path().join("x.png"), |i| {
            Ok(OpOutput::image(i.clone()))
        })
        .unwrap_err();
        assert_eq!(missing.code, codes::IO_READ_FAILED);
    }
}
