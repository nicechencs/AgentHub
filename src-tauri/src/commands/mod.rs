//! Tauri command modules — thin wrappers over agenthub-core.

pub mod account;
pub mod adapter;
pub mod agent_catalog;
pub mod agent_visibility;
pub mod backup;
pub mod chat;
pub mod configuration;
pub mod doctor;
pub mod install;
pub mod lifecycle;
pub mod mcp;
pub mod oauth;
pub mod plugins;
pub mod project;
pub mod provider;
pub mod settings;
pub mod shell_icon;
pub mod skill;
pub mod sub2api;
pub mod sub2api_remembered_vault;
pub mod trash;
pub mod usage;

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use agenthub_core::error::AppError;
use agenthub_core::logging::{self, targets};
use agenthub_core::models::AgentId;
use agenthub_core::AgentHub;
use serde::{Deserialize, Serialize};

/// Structured GUI error (keeps `code` for diagnostics).
///
/// Adapter commands return this type so the frontend can keep `code`,
/// `message`, optional `details`, and `retryable`. Non-adapter commands
/// still use [`map_err_string`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    pub retryable: bool,
}

impl GuiError {
    pub fn from_app(err: &AppError) -> Self {
        Self::adapter(
            err.code(),
            agenthub_core::utils::redact::redact_text(&err.to_string()),
            None,
        )
    }

    pub(crate) fn adapter(
        code: impl Into<String>,
        message: impl Into<String>,
        details: Option<String>,
    ) -> Self {
        let code = code.into();
        let retryable = is_adapter_error_retryable(&code);
        Self {
            code,
            message: message.into(),
            details,
            retryable,
        }
    }
}

impl From<AppError> for GuiError {
    fn from(err: AppError) -> Self {
        Self::from_app(&err)
    }
}

const RETRYABLE_ERROR_CONTRACT_JSON: &str =
    include_str!("../../../src/lib/backend/contracts/retryable-error-contract.json");

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RetryableErrorContract {
    retryable_exact: Vec<String>,
    retryable_prefixes: Vec<String>,
}

fn retryable_error_contract() -> &'static RetryableErrorContract {
    static CONTRACT: OnceLock<RetryableErrorContract> = OnceLock::new();
    CONTRACT.get_or_init(|| {
        serde_json::from_str(RETRYABLE_ERROR_CONTRACT_JSON).expect("retryable-error-contract.json")
    })
}

/// Retryable Adapter command codes from `retryable-error-contract.json`.
/// Keep in lockstep with `isAdapterErrorCodeRetryable` on the frontend.
///
/// The UI still prefers the structured `retryable` field on the error.
pub(crate) fn is_adapter_error_retryable(code: &str) -> bool {
    let contract = retryable_error_contract();
    contract.retryable_exact.iter().any(|item| item == code)
        || contract
            .retryable_prefixes
            .iter()
            .any(|prefix| code.starts_with(prefix.as_str()))
}

fn is_structured_error_code(code: &str) -> bool {
    !code.is_empty()
        && !code.contains(' ')
        && (code.contains('.')
            || code.starts_with("retryable:")
            || matches!(
                code,
                "needs_attention"
                    | "not_found"
                    | "invalid_arg"
                    | "unsupported"
                    | "io"
                    | "db"
                    | "json"
            ))
}

/// Recover a structured Adapter error from a controller / legacy `String`
/// without dropping a bracketed or bare error code.
pub(crate) fn adapter_error_from_string(raw: String) -> GuiError {
    let trimmed = raw.trim();
    if is_structured_error_code(trimmed) {
        return GuiError::adapter(trimmed, trimmed, None);
    }
    if let Some((code, message, details)) = split_legacy_error(trimmed) {
        return GuiError::adapter(code, message, details);
    }
    GuiError::adapter("adapter.command", raw, None)
}

fn split_legacy_error(raw: &str) -> Option<(String, String, Option<String>)> {
    let start = raw.rfind('[')?;
    let end = raw[start + 1..].find(']')? + start + 1;
    let code = &raw[start + 1..end];
    if !is_structured_error_code(code) {
        return None;
    }
    let before = raw[..start].trim_end();
    let after = raw[end + 1..].trim();
    let after = after.strip_prefix(':').map(str::trim).unwrap_or(after);
    let message = if before.is_empty() {
        raw.to_string()
    } else {
        before.to_string()
    };
    let details = if after.is_empty() {
        None
    } else {
        Some(after.to_string())
    };
    Some((code.to_string(), message, details))
}

/// Map core error to String while logging (legacy commands still use String).
///
/// Format: `{message} [{code}]` — readable in toasts; code stays greppable in UI/logs.
///
/// Business modules own ERROR-level failure logs; the GUI shell only records a
/// debug breadcrumb so `module=gui` does not double-emit the same AppError.
pub(crate) fn map_err_string(op: &str, err: AppError) -> String {
    logging::log_debug(
        targets::GUI,
        op,
        &format!("command error code={}", err.code()),
    );
    let ge = GuiError::from_app(&err);
    format!("{} [{}]", ge.message, ge.code)
}

/// Parse a required agent id string (from [`AgentId::ALL`]).
pub(crate) fn parse_agent(agent: &str) -> Result<AgentId, String> {
    AgentId::parse_required(agent).map_err(|err| {
        // Keep GUI string errors as the InvalidArg payload (no `invalid argument:` prefix).
        let msg = match err {
            AppError::InvalidArg(m) => m,
            other => other.to_string(),
        };
        tracing::warn!(target: targets::GUI, op = "parse_agent", "{msg}");
        msg
    })
}

/// Parse an optional agent id filter.
pub(crate) fn parse_agent_opt(agent: Option<&str>) -> Result<Option<AgentId>, String> {
    AgentId::parse_optional(agent).map_err(|err| {
        let msg = match err {
            AppError::InvalidArg(m) => m,
            other => other.to_string(),
        };
        tracing::warn!(target: targets::GUI, op = "parse_agent", "{msg}");
        msg
    })
}

/// Run blocking hub work on the async runtime's blocking pool.
///
/// Tauri v2 executes non-`async` commands on the main thread; I/O and subprocess
/// work must move off it. `State<'_>` cannot cross `.await`, so callers take
/// `hub_arc()` first and move the `Arc` into the closure.
pub(crate) async fn with_hub_blocking<T, F>(hub: Arc<AgentHub>, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&AgentHub) -> Result<T, String> + Send + 'static,
{
    let started = Instant::now();
    let result = tauri::async_runtime::spawn_blocking(move || f(&hub))
        .await
        .map_err(|e| {
            let msg = format!("command join error: {e}");
            logging::log_warn(targets::GUI, "join", &msg);
            msg
        })?;
    let elapsed_ms = started.elapsed().as_millis();
    if let Err(ref e) = result {
        tracing::debug!(
            target: targets::GUI,
            elapsed_ms = u64::try_from(elapsed_ms).unwrap_or(u64::MAX),
            error = %e,
            "hub command failed"
        );
    } else {
        tracing::debug!(
            target: targets::GUI,
            elapsed_ms = u64::try_from(elapsed_ms).unwrap_or(u64::MAX),
            "hub command ok"
        );
    }
    result
}
