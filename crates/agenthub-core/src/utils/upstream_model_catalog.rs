//! Where a login's model list comes from. Catalog URLs stay hardcoded.
//! Live ids are fetched once, then cached on the login row (`extra` /
//! `meta.modelCatalog`) until the URL, key, or official login identity changes.
//!
//! Official login catalogs:
//! - Codex ChatGPT: `GET https://chatgpt.com/backend-api/codex/models`
//! - Grok / Claude / Pi official: no public catalog we can call with that login
//! API Key / connection-pool settings: `{base}/v1/models` then `/models`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::utils::loopback::is_loopback_base_url;
use crate::utils::redact::{api_key_secret_hash, secret_sha256_hex};

pub const MODEL_CATALOG_KEY: &str = "modelCatalog";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredModelCatalog {
    pub fingerprint: String,
    pub source: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub extra_models: Vec<String>,
    #[serde(default)]
    pub attempted: bool,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceModelCatalog {
    pub models: Vec<String>,
    pub source: String,
    pub can_customize: bool,
}

impl SourceModelCatalog {
    pub fn from_stored(stored: &StoredModelCatalog) -> Self {
        let source = if stored.source == "custom" {
            "custom"
        } else if stored.models.is_empty() && stored.extra_models.is_empty() {
            "empty"
        } else if stored.source == "live" {
            "live"
        } else {
            "empty"
        };
        Self {
            models: merge_model_ids([stored.models.clone(), stored.extra_models.clone()]),
            source: source.to_owned(),
            can_customize: true,
        }
    }
}

/// Keep a live catalog and store only the ids the user added.
pub fn with_wanted_models(stored: StoredModelCatalog, wanted: Vec<String>) -> StoredModelCatalog {
    let wanted = merge_model_ids([wanted]);
    if stored.source == "live" && !stored.models.is_empty() {
        let live: std::collections::HashSet<_> = stored.models.iter().cloned().collect();
        let extra_models = wanted
            .into_iter()
            .filter(|id| !live.contains(id))
            .collect();
        StoredModelCatalog {
            extra_models,
            ..stored
        }
    } else {
        StoredModelCatalog {
            source: "custom".into(),
            models: wanted,
            extra_models: Vec::new(),
            attempted: true,
            ..stored
        }
    }
}

pub fn read_stored_catalog(blob: &Value) -> Option<StoredModelCatalog> {
    let value = blob.get(MODEL_CATALOG_KEY)?;
    serde_json::from_value(value.clone()).ok()
}

pub fn write_stored_catalog(blob: &mut Value, catalog: &StoredModelCatalog) {
    if !blob.is_object() {
        *blob = json!({});
    }
    if let Some(map) = blob.as_object_mut() {
        if let Ok(value) = serde_json::to_value(catalog) {
            map.insert(MODEL_CATALOG_KEY.into(), value);
        }
    }
}

pub fn fingerprint_oauth(agent: &str, identity: &str) -> String {
    secret_sha256_hex(&format!("oauth|{}|{}", agent.trim(), identity.trim()))
}

pub fn fingerprint_apikey(agent: &str, settings: &Value) -> String {
    let base = catalog_endpoint(settings)
        .map(|(base, _)| base)
        .unwrap_or_default();
    let hash = api_key_secret_hash(settings).unwrap_or_default();
    secret_sha256_hex(&format!(
        "apikey|{}|{}|{}",
        agent.trim(),
        base.trim(),
        hash
    ))
}

pub fn cache_is_current(stored: &StoredModelCatalog, fingerprint: &str) -> bool {
    stored.attempted && stored.fingerprint == fingerprint
}

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
