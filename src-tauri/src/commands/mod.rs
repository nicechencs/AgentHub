//! Tauri command modules — thin wrappers over agenthub-core.

pub mod account;
pub mod agent_catalog;
pub mod backup;
pub mod chat;
pub mod configuration;
pub mod doctor;
pub mod install;
pub mod oauth;
pub mod project;
pub mod provider;
pub mod settings;
pub mod skill;
pub mod usage;

use std::sync::Arc;
use std::time::Instant;

use agenthub_core::error::AppError;
use agenthub_core::logging::{self, targets};
use agenthub_core::models::AgentId;
use agenthub_core::AgentHub;
use serde::Serialize;

/// Structured GUI error (keeps `code` for diagnostics).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiError {
    pub code: String,
    pub message: String,
}

impl GuiError {
    pub fn from_app(err: &AppError) -> Self {
        Self {
            code: err.code().to_string(),
            message: agenthub_core::utils::redact::redact_text(&err.to_string()),
        }
    }
}

impl From<AppError> for GuiError {
    fn from(err: AppError) -> Self {
        Self::from_app(&err)
    }
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
