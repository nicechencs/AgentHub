//! Codex official-login catalog (aligned with sub2api).
//!
//! These ChatGPT URLs are protocol facts, not user settings and not DB rows:
//! - conversation: `POST https://chatgpt.com/backend-api/codex/responses`
//! - quota: that same `POST` (headers), then `GET …/wham/usage`
//! - models: `GET https://chatgpt.com/backend-api/codex/models`
//!
//! ChatGPT OAuth must not call `https://api.openai.com/v1/models`.
//! Live model ids are fetched at use time; they are not persisted.

use std::time::Duration;

use serde_json::Value;

use crate::error::{AppError, Result};
use crate::utils::redact::redact_text;

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const CODEX_MODELS_CLIENT_VERSION: &str = "0.146.0";
const CODEX_PROBE_ORIGINATOR: &str = "codex-tui";

/// ChatGPT Codex model-list path. Conversation stays on `…/codex/responses`.
pub const CHATGPT_CODEX_MODELS_URL: &str = "https://chatgpt.com/backend-api/codex/models";

/// Parse `{models:[{slug}]}` (and `id` / top-level arrays). First-seen order.
pub fn parse_chatgpt_codex_models(input: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let items = match input {
        Value::Array(items) => items.as_slice(),
        Value::Object(map) => map
            .get("models")
            .or_else(|| map.get("data"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        _ => &[],
    };
    for item in items {
        let id = match item {
            Value::String(s) => s.as_str(),
            Value::Object(map) => map
                .get("slug")
                .or_else(|| map.get("id"))
                .or_else(|| map.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(""),
            _ => "",
        };
        let id = id.trim();
        if id.is_empty() || !seen.insert(id.to_string()) {
            continue;
        }
        out.push(id.to_string());
    }
    out
}

/// GET ChatGPT Codex models with the official-login bearer. Never returns the token.
pub fn list_chatgpt_codex_models(access_token: &str, account_id: &str) -> Result<Vec<String>> {
    let access = access_token.trim();
    let account_id = account_id.trim();
    if access.is_empty() {
        return Err(AppError::InvalidArg("access token is required".into()));
    }
    if account_id.is_empty() {
        return Err(AppError::InvalidArg("chatgpt account id is required".into()));
    }
    let url = format!("{CHATGPT_CODEX_MODELS_URL}?client_version={CODEX_MODELS_CLIENT_VERSION}");
    let ua = format!("{CODEX_PROBE_ORIGINATOR}/{CODEX_MODELS_CLIENT_VERSION}");
    let response = ureq::get(&url)
        .set("Authorization", &format!("Bearer {access}"))
        .set("chatgpt-account-id", account_id)
        .set("Accept", "application/json")
        .set("Originator", CODEX_PROBE_ORIGINATOR)
        .set("Version", CODEX_MODELS_CLIENT_VERSION)
        .set("User-Agent", &ua)
        .timeout(HTTP_TIMEOUT)
        .call()
        .map_err(|err| {
            AppError::message(
                "chatgpt_codex_models.http",
                redact_text(&format!("GET {CHATGPT_CODEX_MODELS_URL} failed: {err}")),
            )
        })?;
    let body: Value = response.into_json().map_err(|err| {
        AppError::message(
            "chatgpt_codex_models.json",
            redact_text(&format!("invalid JSON from {CHATGPT_CODEX_MODELS_URL}: {err}")),
        )
    })?;
    Ok(parse_chatgpt_codex_models(&body))
}

#[cfg(test)]
mod tests;
