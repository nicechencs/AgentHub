//! Unified application error type for agenthub-core.
//! CLI/GUI map this to exit codes and user-facing messages.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    // code is exposed via `AppError::code()`; keep Display message-only to avoid double tags in CLI.
    #[error("{message}")]
    Message { code: &'static str, message: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("environment not ready: {0}")]
    EnvNotReady(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid argument: {0}")]
    InvalidArg(String),

    #[error("unsupported: {0}")]
    Unsupported(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Message { code, .. } => code,
            Self::Io(_) => "io",
            Self::Db(_) => "db",
            Self::Json(_) => "json",
            Self::EnvNotReady(_) => "env.not_ready",
            Self::NotFound(_) => "not_found",
            Self::InvalidArg(_) => "invalid_arg",
            Self::Unsupported(_) => "unsupported",
        }
    }

    pub fn message(code: &'static str, message: impl Into<String>) -> Self {
        Self::Message {
            code,
            message: message.into(),
        }
    }

    /// JSON `details` payload for CLI `--output json` (never includes raw secrets).
    pub fn details(&self) -> serde_json::Value {
        match self {
            Self::EnvNotReady(raw) => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
                    if value.is_object() {
                        return value;
                    }
                }
                serde_json::json!({ "message": crate::utils::redact::redact_text(raw) })
            }
            Self::InvalidArg(message)
            | Self::NotFound(message)
            | Self::Unsupported(message)
            | Self::Message { message, .. } => {
                serde_json::json!({ "message": crate::utils::redact::redact_text(message) })
            }
            other => {
                serde_json::json!({ "message": crate::utils::redact::redact_text(&other.to_string()) })
            }
        }
    }
}
