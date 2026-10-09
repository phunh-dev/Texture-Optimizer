//! Error type returned by every command: serializes as `{ code, params }`
//! (the TS `AppError`), so the frontend can translate it via `errors:<code>`.

use serde::Serialize;
use texopt_core::OpError;

/// Fallback code for unexpected failures (panics, join errors, ...).
pub const UNKNOWN: &str = "UNKNOWN";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct AppError(pub OpError);

impl AppError {
    pub fn unknown(detail: impl std::fmt::Display) -> Self {
        Self(OpError::new(UNKNOWN).with("detail", detail.to_string()))
    }
}

impl From<OpError> for AppError {
    fn from(e: OpError) -> Self {
        Self(e)
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for AppError {}

/// Deserialize a command argument ourselves so malformed input becomes an
/// `INVALID_PARAMS` AppError instead of Tauri's plain-string error.
pub fn parse_arg<T: serde::de::DeserializeOwned>(
    name: &str,
    value: serde_json::Value,
) -> Result<T, AppError> {
    serde_json::from_value(value)
        .map_err(|e| AppError(OpError::invalid_param(name, &e.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_code_and_params() {
        let e = AppError(OpError::new("JOB_NOT_FOUND").with("jobId", "job-1"));
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({ "code": "JOB_NOT_FOUND", "params": { "jobId": "job-1" } })
        );
        assert_eq!(AppError::unknown("x").0.code, UNKNOWN);
    }

    #[test]
    fn parse_arg_maps_to_invalid_params() {
        let err = parse_arg::<texopt_core::ops::OpRequest>(
            "request",
            serde_json::json!({ "kind": "nope" }),
        )
        .unwrap_err();
        assert_eq!(err.0.code, texopt_core::error::codes::INVALID_PARAMS);
        assert_eq!(err.0.params["param"], "request");
        let ok: texopt_core::ops::OpRequest = parse_arg(
            "request",
            serde_json::json!({ "kind": "resize", "params": {} }),
        )
        .unwrap();
        assert!(matches!(ok, texopt_core::ops::OpRequest::Resize(_)));
    }
}
