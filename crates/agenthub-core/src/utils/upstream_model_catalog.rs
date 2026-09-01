//! Where a login's model list comes from. URLs stay hardcoded; live ids are
//! fetched at use time and not written to the DB.
//!
//! Official login catalogs:
//! - Codex ChatGPT: `GET https://chatgpt.com/backend-api/codex/models`
//! - Grok / Claude / Pi official: no public catalog we can call with that login
//! API Key / connection-pool settings: `{base}/v1/models` then `/models`.

use serde_json::Value;

use crate::utils::loopback::is_loopback_base_url;

const BASE_POINTERS: &[&str] = &[
    "/url",
    "/base_url",
    "/baseURL",
    "/baseUrl",
    "/env/ANTHROPIC_BASE_URL",
    "/env/OPENAI_BASE_URL",
    "/env/OPENAI_API_BASE",
    "/api_base",
    "/apiBase",
];

const KEY_POINTERS: &[&str] = &[
    "/api_key",
    "/apiKey",
    "/key",
    "/env/ANTHROPIC_AUTH_TOKEN",
    "/env/ANTHROPIC_API_KEY",
    "/env/OPENAI_API_KEY",
    "/env/XAI_API_KEY",
    "/env/DEEPSEEK_API_KEY",
    "/auth/OPENAI_API_KEY",
];

fn nonempty_str(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|item| !item.is_empty() && *item != "***")
        .map(str::to_owned)
}

fn first_pointer(blob: &Value, pointers: &[&str]) -> Option<String> {
    pointers
        .iter()
        .find_map(|pointer| nonempty_str(blob.pointer(pointer).and_then(Value::as_str)))
}

fn toml_first_string(doc: &toml_edit::DocumentMut, keys: &[&str]) -> Option<String> {
    if let Some(value) = keys
        .iter()
        .find_map(|key| doc.get(key).and_then(|item| item.as_str()))
        .and_then(|value| nonempty_str(Some(value)))
    {
        return Some(value);
    }
    for table_name in ["model_providers", "providers"] {
        let Some(table) = doc.get(table_name).and_then(|item| item.as_table()) else {
            continue;
        };
        for (_, provider) in table.iter() {
            let Some(provider) = provider.as_table() else {
                continue;
            };
            if let Some(value) = keys
                .iter()
                .find_map(|key| provider.get(key).and_then(|item| item.as_str()))
                .and_then(|value| nonempty_str(Some(value)))
            {
                return Some(value);
            }
        }
    }
    None
}

fn toml_endpoint(content: &str) -> (Option<String>, Option<String>) {
    let Ok(doc) = content.parse::<toml_edit::DocumentMut>() else {
        return (None, None);
    };
    (
        toml_first_string(&doc, &["base_url", "baseURL", "url"]),
        toml_first_string(&doc, &["api_key", "apiKey", "key"]),
    )
}

/// Upstream `(base_url, api_key)` for a live `/models` fetch. Loopback is skipped.
pub fn catalog_endpoint(blob: &Value) -> Option<(String, String)> {
    let mut base = first_pointer(blob, BASE_POINTERS);
    let mut key = first_pointer(blob, KEY_POINTERS);
    if blob
        .get("format")
        .and_then(Value::as_str)
        .is_some_and(|format| format.eq_ignore_ascii_case("toml"))
    {
        if let Some(content) = blob.get("content").and_then(Value::as_str) {
            let (toml_base, toml_key) = toml_endpoint(content);
            if base.is_none() {
                base = toml_base;
            }
            if key.is_none() {
                key = toml_key;
            }
        }
    }
    let base = base?;
    let key = key?;
    if is_loopback_base_url(&base) {
        return None;
    }
    Some((base, key))
}

fn push_id(out: &mut Vec<String>, seen: &mut std::collections::HashSet<String>, raw: &str) {
    let id = raw.trim();
    if id.is_empty() || !seen.insert(id.to_string()) {
        return;
    }
    out.push(id.to_string());
}

/// Model ids already stored on the login (not fetched). First-seen order.
pub fn embedded_listed_models(blob: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(items) = blob.get("listedModels").and_then(Value::as_array) {
        for item in items {
            if let Some(id) = item.as_str() {
                push_id(&mut out, &mut seen, id);
            }
        }
    }
    match blob.get("models") {
        Some(Value::Array(items)) => {
            for item in items {
                match item {
                    Value::String(id) => push_id(&mut out, &mut seen, id),
                    Value::Object(map) => {
                        if let Some(id) = map
                            .get("id")
                            .or_else(|| map.get("slug"))
                            .and_then(Value::as_str)
                        {
                            push_id(&mut out, &mut seen, id);
                        }
                    }
                    _ => {}
                }
            }
        }
        Some(Value::Object(map)) => {
            for key in map.keys() {
                push_id(&mut out, &mut seen, key);
            }
        }
        _ => {}
    }
    if let Some(id) = blob.get("model_id").and_then(Value::as_str) {
        push_id(&mut out, &mut seen, id);
    }
    if let Some(id) = blob.get("model").and_then(Value::as_str) {
        push_id(&mut out, &mut seen, id);
    }
    if let Some(id) = blob.pointer("/catalog_row/id").and_then(Value::as_str) {
        push_id(&mut out, &mut seen, id);
    }
    out
}

pub fn merge_model_ids(parts: impl IntoIterator<Item = Vec<String>>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for part in parts {
        for id in part {
            push_id(&mut out, &mut seen, &id);
        }
    }
    out
}

#[cfg(test)]
mod tests;
