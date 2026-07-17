//! RPC error codes and structured JSON-RPC-style errors.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Wire-format JSON-RPC error object carried inside [`crate::frame::ResponseFrame`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RpcError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Stable daemon/extension error codes (§4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    UnknownMethod,
    Unsupported,
    InvalidParams,
    NotFound,
    PermissionDenied,
    Timeout,
    CdpFailed,
    ProtocolError,
    Cancelled,
    UserAborted,
    VersionTooOld,
    MultipleBrowsersOnline,
    NoBrowserConnected,
}

impl ErrorCode {
    /// Stable wire name used in structured logs and JSON responses.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::UnknownMethod => "unknown_method",
            ErrorCode::Unsupported => "unsupported",
            ErrorCode::InvalidParams => "invalid_params",
            ErrorCode::NotFound => "not_found",
            ErrorCode::PermissionDenied => "permission_denied",
            ErrorCode::Timeout => "timeout",
            ErrorCode::CdpFailed => "cdp_failed",
            ErrorCode::ProtocolError => "protocol_error",
            ErrorCode::Cancelled => "cancelled",
            ErrorCode::UserAborted => "user_aborted",
            ErrorCode::VersionTooOld => "version_too_old",
            ErrorCode::MultipleBrowsersOnline => "multiple_browsers_online",
            ErrorCode::NoBrowserConnected => "no_browser_connected",
        }
    }
}

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("ambiguous response frame: expected exactly one of result or error")]
    AmbiguousResponse,
    #[error("invalid protocol frame: {0}")]
    InvalidFrame(String),
}

#[cfg(test)]
mod user_aborted_tests {
    use super::*;

    #[test]
    fn user_aborted_serialises_as_snake_case() {
        let v = serde_json::to_value(ErrorCode::UserAborted).unwrap();
        assert_eq!(v, serde_json::json!("user_aborted"));
        assert_eq!(ErrorCode::UserAborted.as_str(), "user_aborted");
    }

    #[test]
    fn user_aborted_round_trips() {
        let parsed: ErrorCode = serde_json::from_value(serde_json::json!("user_aborted")).unwrap();
        assert_eq!(parsed, ErrorCode::UserAborted);
    }

    #[test]
    fn user_aborted_is_distinct_from_cancelled() {
        assert_ne!(ErrorCode::UserAborted, ErrorCode::Cancelled);
    }
}
