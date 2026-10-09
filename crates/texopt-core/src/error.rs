use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stable error/warning codes. The frontend maps each to `errors:<CODE>` in
/// the locale files, so codes must never be renamed once shipped.
/// Modules may add their own codes (keep them UPPER_SNAKE_CASE and add the
/// matching key to `src/locales/{en,vi}/errors.json`).
pub mod codes {
    pub const NOT_IMPLEMENTED: &str = "NOT_IMPLEMENTED";
    pub const INVALID_PARAMS: &str = "INVALID_PARAMS";
    pub const IO_READ_FAILED: &str = "IO_READ_FAILED";
    pub const IO_WRITE_FAILED: &str = "IO_WRITE_FAILED";
    pub const IMG_DECODE_FAILED: &str = "IMG_DECODE_FAILED";
    pub const IMG_ENCODE_FAILED: &str = "IMG_ENCODE_FAILED";
    pub const IMG_UNSUPPORTED_FORMAT: &str = "IMG_UNSUPPORTED_FORMAT";
    pub const IMG_TOO_LARGE: &str = "IMG_TOO_LARGE";
    pub const IMG_EMPTY: &str = "IMG_EMPTY";
    pub const CANCELLED: &str = "CANCELLED";
}

/// A translatable failure: `code` selects the message, `params` fill its
/// placeholders (e.g. `{{path}}`, `{{max}}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[error("{code} {params:?}")]
pub struct OpError {
    pub code: String,
    #[serde(default)]
    pub params: BTreeMap<String, Value>,
}

impl OpError {
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            params: BTreeMap::new(),
        }
    }

    pub fn with(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    pub fn invalid_param(name: &str, reason: &str) -> Self {
        Self::new(codes::INVALID_PARAMS)
            .with("param", name)
            .with("reason", reason)
    }

    pub fn not_implemented(what: &str) -> Self {
        Self::new(codes::NOT_IMPLEMENTED).with("what", what)
    }
}

pub type OpResult<T> = Result<T, OpError>;
