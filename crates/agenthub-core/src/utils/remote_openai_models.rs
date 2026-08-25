//! OpenAI-compatible `GET {base}/v1/models` helpers.
//!
//! URL normalize + JSON parse are mirrored in
//! `src/lib/provider-detect/remote-models.ts`. Live HTTP is desktop-only
//! (Tauri command); unit tests here do not touch the network.

use std::collections::HashSet;
use std::time::Duration;

use serde_json::Value;

use crate::error::{AppError, Result};
use crate::utils::redact::redact_text;

const HTTP_TIMEOUT: Duration = Duration::from_secs(8);

/// Build GET URL for `{base}/v1/models`, collapsing a trailing `/v1`.
pub fn openai_models_url(base_url: &str) -> String {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let stripped = trimmed.trim_end_matches('/');
    let ends_with_v1 = stripped
        .rsplit_once('/')
        .is_some_and(|(_, last)| last.eq_ignore_ascii_case("v1"));
    if ends_with_v1 {
        format!("{stripped}/models")
    } else {
        format!("{stripped}/v1/models")
    }
}

fn push_model_id(out: &mut Vec<String>, seen: &mut HashSet<String>, raw: Option<&str>) {
    let Some(raw) = raw else { return };
    let id = raw.trim();
    if id.is_empty() {
        return;
    }
    if seen.insert(id.to_string()) {
        out.push(id.to_string());
    }
}

fn push_from_array(out: &mut Vec<String>, seen: &mut HashSet<String>, items: &[Value]) {
    for item in items {
        match item {
            Value::String(s) => push_model_id(out, seen, Some(s.as_str())),
            Value::Object(map) => {
                let id = map.get("id").and_then(Value::as_str);
                push_model_id(out, seen, id);
            }
            _ => {}
        }
    }
}

/// Accept `{data:[{id}]}`, `data: string[]`, `{models:string[]}`,
/// `{models:[{id}]}`, or a top-level array. Dedupe, first-seen order.
pub fn parse_openai_model_list(input: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    match input {
        Value::Array(items) => push_from_array(&mut out, &mut seen, items),
        Value::Object(map) => {
            if let Some(Value::Array(items)) = map.get("data") {
                push_from_array(&mut out, &mut seen, items);
            }
            if let Some(Value::Array(items)) = map.get("models") {
                push_from_array(&mut out, &mut seen, items);
            }
        }
        _ => {}
    }
    out
}

fn looks_http_url(base: &str) -> bool {
    let lower = base.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// GET `{base}/v1/models` with `Authorization: Bearer`. No saved provider id.
pub fn list_remote_openai_models(base_url: &str, api_key: &str) -> Result<Vec<String>> {
    let base = base_url.trim();
    if base.is_empty() {
        return Err(AppError::InvalidArg("base URL is required".into()));
    }
    if !looks_http_url(base) {
        return Err(AppError::InvalidArg("base URL must be http(s)".into()));
    }
    let key = api_key.trim();
    if key.is_empty() {
        return Err(AppError::InvalidArg("API key is required".into()));
    }

    let url = openai_models_url(base);
    if url.is_empty() {
        return Err(AppError::InvalidArg("base URL is required".into()));
    }

    let response = ureq::get(&url)
        .set("Authorization", &format!("Bearer {key}"))
        .set("Accept", "application/json")
        .timeout(HTTP_TIMEOUT)
        .call()
        .map_err(|err| {
            AppError::message(
                "remote_openai_models.http",
                redact_text(&format!("GET {url} failed: {err}")),
            )
        })?;

    let body: Value = response.into_json().map_err(|err| {
        AppError::message(
            "remote_openai_models.json",
            redact_text(&format!("invalid JSON from {url}: {err}")),
        )
    })?;
    Ok(parse_openai_model_list(&body))
}

#[cfg(test)]
mod tests;
