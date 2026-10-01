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

/// Trim and reject empty or already-redacted API keys (shared by
/// `build_api_key_account` impls). A `***` marker is not a real key and must
/// not be persisted as one — that produces nameless recycle-bin rows.
pub(crate) fn require_api_key(api_key: &str) -> Result<&str> {
    let key = api_key.trim();
    if key.is_empty() || crate::utils::redact::is_unusable_secret(key) {
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
    // Prefer explicit settings model over a stale default inside content
    // (custom OpenAI relays must not keep forced kimi-k2 when UI set grok-*).
    let settings_model = object
        .get("model")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let live = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let mut merged = merge_toml_provider_config(expected, &live, desired)?;
    if expected == AgentId::Kimi {
        if let Some(model) = settings_model {
            merged = apply_kimi_settings_model_override(&merged, &model)?;
        }
    }
    crate::utils::atomic::atomic_write(path, merged.as_bytes())
}

fn apply_kimi_settings_model_override(toml_text: &str, model: &str) -> Result<String> {
    use toml_edit::DocumentMut;
    let mut doc = toml_text.parse::<DocumentMut>().map_err(|error| {
        crate::error::AppError::InvalidArg(format!("Kimi TOML config is invalid: {error}"))
    })?;
    doc["default_model"] = toml_edit::value(model);
    crate::integrations::agents::kimi::managed::complete_kimi_live_toml(&mut doc)?;
    Ok(doc.to_string())
}

fn grok_document_has_api_key(doc: &toml_edit::DocumentMut) -> bool {
    let nonempty = |item: Option<&toml_edit::Item>| {
        item.and_then(toml_edit::Item::as_str)
            .is_some_and(|value| !value.trim().is_empty())
    };
    if nonempty(doc.get("api_key")) {
        return true;
    }
    doc.get("model")
        .and_then(toml_edit::Item::as_table)
        .is_some_and(|models| {
            models
                .iter()
                .any(|(_, item)| nonempty(item.as_table().and_then(|entry| entry.get("api_key"))))
        })
}

/// Copy desired auth keys onto the live table.
///
/// An API-key snapshot that predates `[auth]` still sets `preferred_method`
/// without deleting OIDC or the other login settings.
fn merge_grok_auth_preference(live: &mut toml_edit::DocumentMut, desired: &toml_edit::DocumentMut) {
    let desired_pairs: Vec<(String, toml_edit::Item)> = desired
        .get("auth")
        .and_then(toml_edit::Item::as_table)
        .map(|table| {
            table
                .iter()
                .map(|(key, item)| (key.to_string(), item.clone()))
                .collect()
        })
        .unwrap_or_default();
    let desired_has_auth = desired
        .get("auth")
        .and_then(toml_edit::Item::as_table)
        .is_some();
    let has_api_key = grok_document_has_api_key(desired);
    if !desired_has_auth && !has_api_key {
        return;
    }
    if live
        .get("auth")
        .and_then(toml_edit::Item::as_table)
        .is_none()
    {
        live.remove("auth");
        live["auth"] = toml_edit::table();
    }
    let Some(auth) = live.get_mut("auth").and_then(toml_edit::Item::as_table_mut) else {
        return;
    };
    if desired_has_auth {
        for (key, item) in desired_pairs {
            auth.insert(&key, item);
        }
        return;
    }
    auth.insert("preferred_method", toml_edit::value("api_key"));
}

/// Pin or clear `features.campaigns` without replacing other feature flags.
///
/// Bridge projections set `campaigns = false` so bare `grok -p` keeps
/// `models.default`. Non-bridge writes drop only that pin.
fn merge_grok_features_campaigns(
    live: &mut toml_edit::DocumentMut,
    desired: &toml_edit::DocumentMut,
) {
    let desired_campaigns_false = desired
        .get("features")
        .and_then(toml_edit::Item::as_table)
        .and_then(|table| table.get("campaigns"))
        .and_then(toml_edit::Item::as_bool)
        == Some(false);
    if desired_campaigns_false {
        if live
            .get("features")
            .and_then(toml_edit::Item::as_table)
            .is_none()
        {
            live.remove("features");
            live["features"] = toml_edit::table();
        }
        if let Some(features) = live
            .get_mut("features")
            .and_then(toml_edit::Item::as_table_mut)
        {
            features.insert("campaigns", toml_edit::value(false));
        }
        return;
    }
    let features_empty = {
        let Some(features) = live
            .get_mut("features")
            .and_then(toml_edit::Item::as_table_mut)
        else {
            return;
        };
        if features.get("campaigns").and_then(toml_edit::Item::as_bool) == Some(false) {
            features.remove("campaigns");
        }
        features.is_empty()
    };
    if features_empty {
        live.remove("features");
    }
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

    for key in crate::integrations::shared::toml_provider::managed_toml_provider_keys(expected)? {
        live_doc.as_table_mut().remove(key);
    }
    for (key, item) in desired_doc.iter() {
        if expected == AgentId::Grok && (key == "auth" || key == "features") {
            continue;
        }
        live_doc.as_table_mut().insert(key, item.clone());
    }
    if expected == AgentId::Grok {
        merge_grok_auth_preference(&mut live_doc, &desired_doc);
        merge_grok_features_campaigns(&mut live_doc, &desired_doc);
    }

    if expected == AgentId::Kimi {
        crate::integrations::agents::kimi::managed::complete_kimi_live_toml(&mut live_doc)?;
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
