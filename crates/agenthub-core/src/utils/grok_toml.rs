//! Shared Grok Build `config.toml` model-registry shape helpers.
//!
//! Grok stores providers under `[models]` and `[model."<alias>"]`. The legacy
//! top-level shape is still read and migrated so existing installations remain
//! usable. Adapter account writers and the config projector both use this module
//! so migration rules cannot drift.

use serde_json::{json, Map, Value};
use toml_edit::{DocumentMut, Item};

use crate::error::{AppError, Result};

/// Default model alias when neither `models.default` nor a nested entry exists.
pub const DEFAULT_ALIAS: &str = "grok";

/// Options that preserve intentional differences between call sites.
#[derive(Debug, Clone, Copy)]
pub struct EnsureGrokModelShapeOptions {
    /// When creating `model.<alias>`, copy a legacy top-level `api_key` into the entry.
    ///
    /// The config projector needs this so apply of non-secret fields preserves the
    /// key. The account writer always sets `api_key` immediately after ensure, so
    /// it can leave this off.
    pub migrate_legacy_api_key: bool,
    /// Strip a leftover top-level `env_key` after migration.
    ///
    /// Account writers clear root credential pointers so OAuth/API-key writes do
    /// not leave a shadowing env reference. The projector only migrates known
    /// schema fields and leaves unknown root keys alone.
    pub strip_root_env_key: bool,
}

impl Default for EnsureGrokModelShapeOptions {
    fn default() -> Self {
        Self {
            migrate_legacy_api_key: true,
            strip_root_env_key: false,
        }
    }
}

/// Resolve the active model alias (`models.default`, else first `model.*` key).
pub fn active_model_alias(doc: &DocumentMut) -> String {
    doc.get("models")
        .and_then(Item::as_table)
        .and_then(|models| models.get("default"))
        .and_then(Item::as_str)
        .map(str::trim)
        .filter(|alias| !alias.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            doc.get("model")
                .and_then(Item::as_table)
                .and_then(|models| models.iter().next().map(|(key, _)| key.to_string()))
        })
        .unwrap_or_else(|| DEFAULT_ALIAS.to_owned())
}

/// Ensure `[models]` + `[model."<alias>"]` exist, migrating legacy top-level keys.
///
/// Returns a mutable reference to the alias entry table.
pub fn ensure_grok_model_shape<'a>(
    doc: &'a mut DocumentMut,
    alias: &str,
    options: EnsureGrokModelShapeOptions,
) -> Result<&'a mut toml_edit::Table> {
    let legacy_model = doc.get("model").and_then(Item::as_str).map(str::to_owned);
    let legacy_base_url = doc
        .get("base_url")
        .and_then(Item::as_str)
        .map(str::to_owned);
    let legacy_key = if options.migrate_legacy_api_key {
        doc.get("api_key").and_then(Item::as_str).map(str::to_owned)
    } else {
        None
    };

    if doc.get("models").is_none() {
        doc["models"] = toml_edit::table();
    }
    {
        let models = doc["models"]
            .as_table_mut()
            .ok_or_else(|| AppError::InvalidArg("Grok models must be a table".into()))?;
        if models.get("default").is_none() {
            models["default"] = toml_edit::value(alias);
        }
        if models.get("web_search").is_none() {
            models["web_search"] = toml_edit::value(alias);
        }
    }

    if doc.get("model").and_then(Item::as_table).is_none() {
        doc.remove("model");
        doc["model"] = toml_edit::table();
    }
    {
        let model_root = doc["model"]
            .as_table_mut()
            .ok_or_else(|| AppError::InvalidArg("Grok model must be a table".into()))?;
        if model_root.get(alias).is_none() {
            let mut entry = toml_edit::table();
            if let Some(model) = legacy_model {
                entry["model"] = toml_edit::value(model);
            }
            if let Some(base_url) = legacy_base_url {
                entry["base_url"] = toml_edit::value(base_url);
            }
            if let Some(key) = legacy_key {
                entry["api_key"] = toml_edit::value(key);
            }
            model_root.insert(alias, entry);
        }
        if model_root.get(alias).and_then(Item::as_table).is_none() {
            return Err(AppError::InvalidArg(format!(
                "Grok model.{alias} must be a table"
            )));
        }
    }

    // Once migrated, the legacy root keys must not shadow the registry.
    doc.remove("base_url");
    doc.remove("api_key");
    if options.strip_root_env_key {
        doc.remove("env_key");
    }

    doc["model"]
        .as_table_mut()
        .and_then(|models| models.get_mut(alias))
        .and_then(Item::as_table_mut)
        .ok_or_else(|| AppError::InvalidArg(format!("Grok model.{alias} must be a table")))
}

/// Authorization overlay extracted from the active `[model."<alias>"]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrokApiKeyOverlay {
    pub alias: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub env_key: Option<String>,
    pub api_backend: Option<String>,
    pub context_window: Option<i64>,
}

fn nonempty_str(item: Option<&Item>) -> Option<String> {
    item.and_then(Item::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Read overlay fields from a parsed Grok `config.toml`.
pub fn extract_api_key_overlay(doc: &DocumentMut) -> GrokApiKeyOverlay {
    let alias = active_model_alias(doc);
    let entry = doc
        .get("model")
        .and_then(Item::as_table)
        .and_then(|models| models.get(&alias))
        .and_then(Item::as_table);
    GrokApiKeyOverlay {
        alias: alias.clone(),
        model: nonempty_str(entry.and_then(|table| table.get("model")))
            .or_else(|| nonempty_str(doc.get("model"))),
        base_url: nonempty_str(entry.and_then(|table| table.get("base_url")))
            .or_else(|| nonempty_str(doc.get("base_url"))),
        api_key: nonempty_str(entry.and_then(|table| table.get("api_key")))
            .or_else(|| nonempty_str(doc.get("api_key"))),
        env_key: nonempty_str(entry.and_then(|table| table.get("env_key")))
            .or_else(|| nonempty_str(doc.get("env_key"))),
        api_backend: nonempty_str(entry.and_then(|table| table.get("api_backend"))),
        context_window: entry
            .and_then(|table| table.get("context_window"))
            .and_then(Item::as_integer)
            .filter(|n| *n > 0),
    }
}

/// Merge overlay authorization fields into the active model table.
///
/// Unknown tables (MCP, extra models) and unknown keys on the active entry
/// stay. Empty overlay strings clear the corresponding field.
pub fn merge_api_key_overlay(doc: &mut DocumentMut, overlay: &GrokApiKeyOverlay) -> Result<()> {
    let alias = overlay
        .alias
        .trim()
        .is_empty()
        .then(|| active_model_alias(doc))
        .unwrap_or_else(|| overlay.alias.trim().to_string());
    let entry = ensure_grok_model_shape(
        doc,
        &alias,
        EnsureGrokModelShapeOptions {
            migrate_legacy_api_key: false,
            strip_root_env_key: true,
        },
    )?;
    let set_or_remove = |table: &mut toml_edit::Table, key: &str, value: Option<&str>| match value
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(s) => table[key] = toml_edit::value(s),
        None => {
            table.remove(key);
        }
    };
    if overlay.model.is_some() {
        set_or_remove(entry, "model", overlay.model.as_deref());
    }
    if overlay.base_url.is_some() {
        set_or_remove(entry, "base_url", overlay.base_url.as_deref());
    }
    if overlay.api_key.is_some() {
        set_or_remove(entry, "api_key", overlay.api_key.as_deref());
    }
    if overlay.env_key.is_some() {
        set_or_remove(entry, "env_key", overlay.env_key.as_deref());
    }
    if overlay.api_backend.is_some() {
        set_or_remove(entry, "api_backend", overlay.api_backend.as_deref());
    }
    if let Some(window) = overlay.context_window {
        if window > 0 {
            entry["context_window"] = toml_edit::value(window);
        }
    }
    Ok(())
}

/// Flatten overlay + optional full toml snapshot onto an `api_key` credentials object.
pub fn overlay_into_credentials(map: &mut Map<String, Value>, overlay: &GrokApiKeyOverlay) {
    if !overlay.alias.trim().is_empty() {
        map.insert("alias".into(), json!(overlay.alias));
    }
    if let Some(model) = overlay.model.as_deref().filter(|s| !s.is_empty()) {
        map.insert("model".into(), json!(model));
    }
    if let Some(url) = overlay.base_url.as_deref().filter(|s| !s.is_empty()) {
        map.insert("base_url".into(), json!(url));
    }
    if let Some(key) = overlay.env_key.as_deref().filter(|s| !s.is_empty()) {
        map.insert("env_key".into(), json!(key));
    }
    if let Some(backend) = overlay.api_backend.as_deref().filter(|s| !s.is_empty()) {
        map.insert("api_backend".into(), json!(backend));
    }
    if let Some(window) = overlay.context_window.filter(|n| *n > 0) {
        map.insert("context_window".into(), json!(window));
    }
}

/// Overlay from a stored credentials JSON object.
pub fn overlay_from_credentials(credentials: &Value) -> GrokApiKeyOverlay {
    let str_field = |key: &str| {
        credentials
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    GrokApiKeyOverlay {
        alias: str_field("alias").unwrap_or_default(),
        model: str_field("model"),
        base_url: str_field("base_url"),
        api_key: str_field("api_key"),
        env_key: str_field("env_key"),
        api_backend: str_field("api_backend"),
        context_window: credentials
            .get("context_window")
            .and_then(Value::as_i64)
            .filter(|n| *n > 0),
    }
}
