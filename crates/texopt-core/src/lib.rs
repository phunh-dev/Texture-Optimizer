//! Engine-agnostic texture optimization core.
//!
//! Every module here is free of UI/Tauri dependencies so it can be unit-tested
//! in isolation. User-facing text never originates here: failures are reported
//! as [`OpError`] values carrying a stable `code` plus parameters, which the
//! frontend translates through its locale files.

pub mod atlas;
pub mod error;
pub mod fixtures;
pub mod io;
pub mod mesh;
pub mod ops;
pub mod output;
pub mod rename;
pub mod staging;
pub mod thumbs;

pub use error::{OpError, OpResult};

/// All image processing works on 8-bit straight-alpha RGBA buffers.
pub type ImageBuf = image::RgbaImage;
