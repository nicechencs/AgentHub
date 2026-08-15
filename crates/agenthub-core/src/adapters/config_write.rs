//! Shared live-config and credential file writers.

use std::path::Path;

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{AccountKind, AgentConfig, AgentId, LiveAccount};
use crate::utils::atomic::atomic_write;
use crate::utils::redact::mask_secret_preview;

/// Pretty-print a JSON object, atomically write it, then re-read and verify.
///
/// Shared by account credential writers (Kimi / Grok / …) so verify logic cannot drift.
pub(crate) fn write_verified_json_object(path: &Path, body: &serde_json::Value) -> Result<()> {
    if !body.is_object() {
        return Err(AppError::InvalidArg(
            "credentials body must be a JSON object".into(),
        ));
    }
    let mut bytes = serde_json::to_vec_pretty(body)?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)?;
    let written = std::fs::read_to_string(path)?;
    let parsed: serde_json::Value = serde_json::from_str(&written)?;
    if &parsed != body {
        tracing::warn!(
            module = targets::ACCOUNT,
            op = "write_verified_json",
            path = %path.display(),
            "JSON verification failed after write"
        );
        return Err(AppError::message(
            "account.verify",
            "credentials file verification failed after write",
        ));
    }
    tracing::debug!(
        module = targets::ACCOUNT,
        op = "write_verified_json",
        path = %path.display(),
        "verified JSON write ok"
    );
    Ok(())
}

/// Trim and reject empty API keys (shared by `build_api_key_account` impls).
pub(crate) fn require_api_key(api_key: &str) -> Result<&str> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(AppError::InvalidArg("API key must not be empty".into()));
    }
    Ok(key)
}

/// Build a pool `LiveAccount` for an API key (caller supplies credentials + extras).
pub(crate) fn api_key_live_account(
    agent: AgentId,
    key: &str,
    credentials: serde_json::Value,
    label_kind: &str,
    extra: serde_json::Value,
) -> LiveAccount {
    LiveAccount {
        agent,
        kind: AccountKind::ApiKey,
        credentials,
        label_hint: Some(format!("{} ({label_kind})", mask_secret_preview(key))),
        extra,
    }
}

pub(crate) fn write_json_config(path: &Path, config: &AgentConfig) -> Result<()> {
    if config.agent != AgentId::Claude {
        return Err(crate::error::AppError::InvalidArg(format!(
            "config agent mismatch: expected claude, got {}",
            config.agent.as_str()
        )));
    }
    if !config.raw.is_object() {
        return Err(crate::error::AppError::InvalidArg(
            "Claude settings_config must be a JSON object".into(),
        ));
    }

    let mut bytes = serde_json::to_vec_pretty(&config.raw)?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)
}

pub(crate) fn write_toml_config(
    expected: AgentId,
    path: &Path,
    config: &AgentConfig,
) -> Result<()> {
    if config.agent != expected {
        return Err(crate::error::AppError::InvalidArg(format!(
            "config agent mismatch: expected {}, got {}",
            expected.as_str(),
            config.agent.as_str()
        )));
    }
    let object = config.raw.as_object().ok_or_else(|| {
        crate::error::AppError::InvalidArg("TOML settings_config must be a JSON object".into())
    })?;
    if object.get("format").and_then(|value| value.as_str()) != Some("toml") {
        return Err(crate::error::AppError::InvalidArg(
            "TOML settings_config.format must equal 'toml'".into(),
        ));
    }
    // AgentHub: `content`; dual-shape alias: `config`
    let desired = object
        .get("content")
        .or_else(|| object.get("config"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            crate::error::AppError::InvalidArg(
                "TOML settings_config.content (or config) must be a string".into(),
            )
        })?;

    let live = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let merged = merge_toml_provider_config(expected, &live, desired)?;
    crate::utils::atomic::atomic_write(path, merged.as_bytes())
}

fn merge_toml_provider_config(expected: AgentId, live: &str, desired: &str) -> Result<String> {
    use toml_edit::DocumentMut;

    let leading_trivia = leading_toml_trivia(live);
    let mut live_doc = if live.trim().is_empty() {
        DocumentMut::new()
    } else {
        live.parse::<DocumentMut>().map_err(|error| {
            crate::error::AppError::InvalidArg(format!(
                "existing {} TOML config is invalid: {error}",
                expected.as_str()
            ))
        })?
    };
    let desired_doc = desired.parse::<DocumentMut>().map_err(|error| {
        crate::error::AppError::InvalidArg(format!(
            "target {} TOML settings_config is invalid: {error}",
            expected.as_str()
        ))
    })?;

    for key in managed_toml_provider_keys(expected)? {
        live_doc.as_table_mut().remove(key);
    }
    for (key, item) in desired_doc.iter() {
        live_doc.as_table_mut().insert(key, item.clone());
    }

    let rendered = live_doc.to_string();
    if leading_trivia.is_empty() || rendered.starts_with(leading_trivia) {
        Ok(rendered)
    } else {
        Ok(format!("{leading_trivia}{rendered}"))
    }
}

fn leading_toml_trivia(input: &str) -> &str {
    let mut end = 0;
    for segment in input.split_inclusive('\n') {
        let line = segment.trim_end_matches(&['\r', '\n'][..]);
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            end += segment.len();
        } else {
            break;
        }
    }
    &input[..end]
}

fn managed_toml_provider_keys(agent: AgentId) -> Result<&'static [&'static str]> {
    match agent {
        AgentId::Codex => Ok(&[
            "model",
            "review_model",
            "model_provider",
            "model_reasoning_effort",
            "model_reasoning_summary",
            "model_verbosity",
            "model_providers",
            // provider / relay common top-level flags
            "disable_response_storage",
            "preferred_auth_method",
            "network_access",
            "windows_wsl_setup_acknowledged",
            // features.goals / responses_websockets_v2 等随供应商切换整表替换
            "features",
        ]),
        AgentId::Kimi => Ok(&["default_model", "default_provider", "providers"]),
        AgentId::Grok => Ok(&["models", "model", "base_url", "api_key", "env_key"]),
        AgentId::Claude | AgentId::Pi | AgentId::WorkBuddy | AgentId::Cursor | AgentId::Dsh => {
            Err(crate::error::AppError::InvalidArg(format!(
                "{} provider config is JSON, not TOML",
                agent.display_name()
            )))
        }
    }
}
