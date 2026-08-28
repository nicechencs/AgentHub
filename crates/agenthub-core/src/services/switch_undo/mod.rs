//! Session-style switch undo slots in the `settings` table.
//!
//! Keys: `{prefix}.{agent_id}` → JSON `{"fromId","toId"}`.
//! Used by provider/account live switch so GUI toast "撤销" can re-apply the
//! previous connection without a full backup restore.

use serde_json::json;

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::storage::Database;

pub const PROVIDER_UNDO_PREFIX: &str = "provider.undo";
pub const ACCOUNT_UNDO_PREFIX: &str = "account.undo";

fn setting_key(prefix: &str, agent: AgentId) -> String {
    format!("{prefix}.{}", agent.as_str())
}

pub fn record_switch_undo(
    db: &Database,
    prefix: &str,
    agent: AgentId,
    from_id: &str,
    to_id: &str,
) -> Result<()> {
    let value = json!({ "fromId": from_id, "toId": to_id }).to_string();
    db.set_setting(&setting_key(prefix, agent), &value)
}

pub fn clear_switch_undo(db: &Database, prefix: &str, agent: AgentId) -> Result<()> {
    // Empty value keeps schema simple (no delete API on settings).
    db.set_setting(&setting_key(prefix, agent), "")
}

/// Read the undo slot without clearing. Returns previous connection id (`fromId`).
pub fn peek_switch_undo(db: &Database, prefix: &str, agent: AgentId) -> Result<Option<String>> {
    let key = setting_key(prefix, agent);
    let raw = match db.get_setting(&key)? {
        Some(value) if !value.trim().is_empty() => value,
        _ => return Ok(None),
    };
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|error| {
        AppError::message(
            "switch.undo",
            format!("invalid undo slot for {}: {error}", agent.as_str()),
        )
    })?;
    let from_id = parsed
        .get("fromId")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    Ok(from_id)
}

/// Walk provider/account JSON for a probeable HTTP(S) base URL.
pub fn extract_probe_url(settings: &serde_json::Value) -> Option<String> {
    fn from_str(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            Some(trimmed.to_owned())
        } else {
            None
        }
    }

    fn is_schema_or_secret_key(key: &str) -> bool {
        matches!(
            key,
            "$schema" | "$id" | "schema" | "schemaUrl" | "$comment" | "$defs"
        ) || key.eq_ignore_ascii_case("api_key")
            || key.eq_ignore_ascii_case("apiKey")
            || key.eq_ignore_ascii_case("token")
            || key.eq_ignore_ascii_case("authorization")
    }

    fn is_endpoint_url(raw: &str) -> bool {
        let Some(url) = from_str(raw) else {
            return false;
        };
        let lower = url.to_ascii_lowercase();
        !lower.contains("json.schemastore.org") && !lower.contains("/schema")
    }

    fn walk(value: &serde_json::Value, out: &mut Option<String>) {
        if out.is_some() {
            return;
        }
        match value {
            serde_json::Value::String(text) => {
                if is_endpoint_url(text) {
                    *out = from_str(text);
                }
            }
            serde_json::Value::Object(map) => {
                const KEYS: &[&str] = &[
                    "base_url",
                    "baseUrl",
                    "ANTHROPIC_BASE_URL",
                    "OPENAI_BASE_URL",
                    "url",
                ];
                for key in KEYS {
                    if let Some(serde_json::Value::String(text)) = map.get(*key) {
                        if is_endpoint_url(text) {
                            *out = from_str(text);
                            return;
                        }
                    }
                }
                if let Some(env) = map.get("env") {
                    walk(env, out);
                    if out.is_some() {
                        return;
                    }
                }
                for (key, child) in map {
                    if is_schema_or_secret_key(key) || key == "env" {
                        continue;
                    }
                    walk(child, out);
                    if out.is_some() {
                        return;
                    }
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    walk(item, out);
                    if out.is_some() {
                        return;
                    }
                }
            }
            _ => {}
        }
    }

    let mut found = None;
    walk(settings, &mut found);
    found
}

/// HTTP GET probe; any response status is success. Timeout → error.
pub fn probe_url_latency_ms(url: &str) -> Result<u64> {
    use std::time::{Duration, Instant};

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            AppError::message("provider.latency", format!("runtime start failed: {error}"))
        })?;

    runtime.block_on(async {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::limited(3))
            .build()
            .map_err(|error| {
                AppError::message("provider.latency", format!("client build failed: {error}"))
            })?;
        let started = Instant::now();
        match client.get(url).send().await {
            Ok(_response) => Ok(started.elapsed().as_millis() as u64),
            Err(error) => Err(AppError::message(
                "provider.latency",
                format!("probe failed: {error}"),
            )),
        }
    })
}

#[cfg(test)]
mod tests;
