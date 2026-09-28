//! Shared Grok Build `config.toml` model-registry shape helpers.
//!
//! Grok stores providers under `[models]` and `[model."<alias>"]`. The legacy
//! top-level shape is still read and migrated so existing installations remain
//! usable. Adapter account writers and the config projector both use this module
//! so migration rules cannot drift.

use serde_json::{json, Map, Value};
use toml_edit::{DocumentMut, Item, Table};

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

/// Return the active model entry selected by `models.default`.
pub fn active_model_entry<'a>(doc: &'a DocumentMut) -> Option<&'a Table> {
    let alias = active_model_alias(doc);
    doc.get("model")
        .and_then(Item::as_table)
        .and_then(|models| models.get(&alias))
        .and_then(Item::as_table)
}

/// Resolve the provider used by the active model alias.
pub fn active_model_provider(doc: &DocumentMut) -> Option<String> {
    active_model_entry(doc).and_then(|entry| nonempty_str(entry.get("model_provider")))
}

/// Return the provider table selected by the active model alias.
pub fn active_model_provider_table<'a>(doc: &'a DocumentMut) -> Option<&'a Table> {
    let provider = active_model_provider(doc)?;
    doc.get("model_providers")
        .and_then(Item::as_table)
        .and_then(|providers| providers.get(&provider))
        .and_then(Item::as_table)
}

/// Return the mutable provider table selected by the active model alias.
pub fn active_model_provider_table_mut<'a>(doc: &'a mut DocumentMut) -> Option<&'a mut Table> {
    let provider = active_model_provider(doc)?;
    doc.get_mut("model_providers")
        .and_then(Item::as_table_mut)
        .and_then(|providers| providers.get_mut(&provider))
        .and_then(Item::as_table_mut)
}

/// Read a string from the active provider table.
pub fn active_provider_field(doc: &DocumentMut, key: &str) -> Option<String> {
    active_model_provider_table(doc).and_then(|provider| nonempty_str(provider.get(key)))
}

/// Ensure and return a provider table.
pub fn ensure_grok_provider_table<'a>(
    doc: &'a mut DocumentMut,
    provider: &str,
) -> Result<&'a mut Table> {
    let provider = provider.trim();
    if provider.is_empty() {
        return Err(AppError::InvalidArg(
            "Grok model_provider must not be empty".into(),
        ));
    }
    if doc.get("model_providers").is_none() {
        doc["model_providers"] = toml_edit::table();
    }
    let providers = doc["model_providers"]
        .as_table_mut()
        .ok_or_else(|| AppError::InvalidArg("Grok model_providers must be a table".into()))?;
    if providers.get(provider).is_none() {
        providers.insert(provider, toml_edit::table());
    }
    providers
        .get_mut(provider)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| {
            AppError::InvalidArg(format!("Grok model_providers.{provider} must be a table"))
        })
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
    let legacy_api_backend = doc
        .get("api_backend")
        .and_then(Item::as_str)
        .map(str::to_owned);
    let legacy_env_key = doc.get("env_key").and_then(Item::as_str).map(str::to_owned);
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
            if let Some(key) = legacy_key.as_deref() {
                entry["api_key"] = toml_edit::value(key);
            }
            entry["model_provider"] = toml_edit::value("proxy");
            if let Some(env_key) = legacy_env_key.as_deref() {
                entry["env_key"] = toml_edit::value(env_key);
            }
            model_root.insert(alias, entry);
        } else if options.migrate_legacy_api_key {
            if let (Some(entry), Some(key)) = (
                model_root.get_mut(alias).and_then(Item::as_table_mut),
                legacy_key.as_deref(),
            ) {
                if entry.get("api_key").is_none() {
                    entry["api_key"] = toml_edit::value(key);
                }
            }
        }
        if model_root.get(alias).and_then(Item::as_table).is_none() {
            return Err(AppError::InvalidArg(format!(
                "Grok model.{alias} must be a table"
            )));
        }
    }

    // Complete the active model's provider link, then migrate old inline
    // endpoint fields into that provider. Existing model metadata and other
    // aliases are deliberately left untouched.
    let provider = doc
        .get("model")
        .and_then(Item::as_table)
        .and_then(|models| models.get(alias))
        .and_then(Item::as_table)
        .and_then(|entry| nonempty_str(entry.get("model_provider")))
        .unwrap_or_else(|| "proxy".to_owned());
    if let Some(entry) = doc
        .get_mut("model")
        .and_then(Item::as_table_mut)
        .and_then(|models| models.get_mut(alias))
        .and_then(Item::as_table_mut)
    {
        if nonempty_str(entry.get("model_provider")).is_none() {
            entry["model_provider"] = toml_edit::value(provider.as_str());
        }
    }
    let inline_base_url = doc
        .get("model")
        .and_then(Item::as_table)
        .and_then(|models| models.get(alias))
        .and_then(Item::as_table)
        .and_then(|entry| nonempty_str(entry.get("base_url")))
        .or(legacy_base_url);
    let inline_api_backend = doc
        .get("model")
        .and_then(Item::as_table)
        .and_then(|models| models.get(alias))
        .and_then(Item::as_table)
        .and_then(|entry| nonempty_str(entry.get("api_backend")))
        .or(legacy_api_backend);
    {
        let provider_table = ensure_grok_provider_table(doc, &provider)?;
        if provider_table.get("base_url").is_none() {
            if let Some(base_url) = inline_base_url {
                provider_table["base_url"] = toml_edit::value(base_url);
            }
        }
        if provider_table.get("api_backend").is_none() {
            if let Some(api_backend) = inline_api_backend {
                provider_table["api_backend"] = toml_edit::value(api_backend);
            }
        }
    }
    if let Some(entry) = doc
        .get_mut("model")
        .and_then(Item::as_table_mut)
        .and_then(|models| models.get_mut(alias))
        .and_then(Item::as_table_mut)
    {
        // Model-level copies override the provider. Grok would then ignore
        // the endpoint and backend written on `[model_providers.*]`.
        entry.remove("base_url");
        entry.remove("api_backend");
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
    pub model_provider: Option<String>,
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
        model_provider: active_model_provider(doc),
        model: nonempty_str(entry.and_then(|table| table.get("model")))
            .or_else(|| nonempty_str(doc.get("model"))),
        base_url: active_provider_field(doc, "base_url")
            .or_else(|| nonempty_str(entry.and_then(|table| table.get("base_url"))))
            .or_else(|| nonempty_str(doc.get("base_url"))),
        api_key: nonempty_str(entry.and_then(|table| table.get("api_key")))
            .or_else(|| nonempty_str(doc.get("api_key"))),
        env_key: nonempty_str(entry.and_then(|table| table.get("env_key")))
            .or_else(|| nonempty_str(doc.get("env_key"))),
        api_backend: active_provider_field(doc, "api_backend")
            .or_else(|| nonempty_str(entry.and_then(|table| table.get("api_backend")))),
        context_window: entry
            .and_then(|table| table.get("context_window"))
            .and_then(Item::as_integer)
            .filter(|n| *n > 0),
    }
}

/// Read overlay fields from one alias, not from `models.default`.
///
/// Saved accounts can name an alias that is not the file's current default.
/// Recovering fields from the default would apply the other model's endpoint.
pub fn extract_api_key_overlay_for_alias(doc: &DocumentMut, alias: &str) -> GrokApiKeyOverlay {
    let alias = alias.trim();
    if alias.is_empty() {
        return extract_api_key_overlay(doc);
    }
    let entry = doc
        .get("model")
        .and_then(Item::as_table)
        .and_then(|models| models.get(alias))
        .and_then(Item::as_table);
    let provider = entry.and_then(|table| nonempty_str(table.get("model_provider")));
    let provider_table = provider.as_deref().and_then(|name| {
        doc.get("model_providers")
            .and_then(Item::as_table)
            .and_then(|providers| providers.get(name))
            .and_then(Item::as_table)
    });
    let provider_value = |key: &str| provider_table.and_then(|table| nonempty_str(table.get(key)));
    GrokApiKeyOverlay {
        alias: alias.to_string(),
        model_provider: provider,
        model: nonempty_str(entry.and_then(|table| table.get("model"))),
        base_url: provider_value("base_url")
            .or_else(|| nonempty_str(entry.and_then(|table| table.get("base_url")))),
        api_key: nonempty_str(entry.and_then(|table| table.get("api_key"))),
        env_key: nonempty_str(entry.and_then(|table| table.get("env_key"))),
        api_backend: provider_value("api_backend")
            .or_else(|| nonempty_str(entry.and_then(|table| table.get("api_backend")))),
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
    ensure_grok_model_shape(
        doc,
        &alias,
        EnsureGrokModelShapeOptions {
            migrate_legacy_api_key: false,
            strip_root_env_key: true,
        },
    )?;
    if let Some(model_provider) = overlay
        .model_provider
        .as_deref()
        .map(str::trim)
        .filter(|provider| !provider.is_empty())
    {
        let entry = doc
            .get_mut("model")
            .and_then(Item::as_table_mut)
            .and_then(|models| models.get_mut(&alias))
            .and_then(Item::as_table_mut)
            .ok_or_else(|| AppError::InvalidArg(format!("Grok model.{alias} must be a table")))?;
        entry["model_provider"] = toml_edit::value(model_provider);
    }
    if let Some(models) = doc.get_mut("models").and_then(Item::as_table_mut) {
        // A saved account may belong to a non-default alias. Selecting that
        // account must make the same alias active before verification or the
        // next CLI invocation.
        models["default"] = toml_edit::value(alias.as_str());
        models["web_search"] = toml_edit::value(alias.as_str());
    }
    let provider = doc
        .get("model")
        .and_then(Item::as_table)
        .and_then(|models| models.get(&alias))
        .and_then(Item::as_table)
        .and_then(|entry| nonempty_str(entry.get("model_provider")))
        .unwrap_or_else(|| "proxy".to_owned());
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
        let entry = doc
            .get_mut("model")
            .and_then(Item::as_table_mut)
            .and_then(|models| models.get_mut(&alias))
            .and_then(Item::as_table_mut)
            .ok_or_else(|| AppError::InvalidArg(format!("Grok model.{alias} must be a table")))?;
        set_or_remove(entry, "model", overlay.model.as_deref());
    }
    {
        let provider_table = ensure_grok_provider_table(doc, &provider)?;
        if overlay.base_url.is_some() {
            set_or_remove(provider_table, "base_url", overlay.base_url.as_deref());
        }
        if overlay.api_backend.is_some() {
            set_or_remove(
                provider_table,
                "api_backend",
                overlay.api_backend.as_deref(),
            );
        }
    }
    {
        let entry = doc
            .get_mut("model")
            .and_then(Item::as_table_mut)
            .and_then(|models| models.get_mut(&alias))
            .and_then(Item::as_table_mut)
            .ok_or_else(|| AppError::InvalidArg(format!("Grok model.{alias} must be a table")))?;
        if overlay.api_key.is_some() {
            set_or_remove(entry, "api_key", overlay.api_key.as_deref());
        }
        if let Some(window) = overlay.context_window {
            if window > 0 {
                entry["context_window"] = toml_edit::value(window);
            }
        }
        // Model-level copies override the provider. Grok would then ignore
        // the endpoint and backend written on `[model_providers.*]`.
        entry.remove("base_url");
        entry.remove("api_backend");
        entry.remove("env_key");
    }
    if let Some(key) = overlay
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        let Some(models) = doc.get_mut("model").and_then(Item::as_table_mut) else {
            return Err(AppError::InvalidArg("Grok model must be a table".into()));
        };
        for (_, item) in models.iter_mut() {
            let Some(entry) = item.as_table_mut() else {
                continue;
            };
            if entry
                .get("model_provider")
                .and_then(Item::as_str)
                .map(str::trim)
                == Some(provider.as_str())
            {
                entry["api_key"] = toml_edit::value(key);
                entry.remove("env_key");
            }
        }
    }
    // API key accounts use the active model's key and the provider's endpoint.
    // A stale root endpoint or environment pointer can shadow that pair.
    doc.remove("base_url");
    doc.remove("env_key");
    if doc.get("auth").and_then(Item::as_table).is_none() {
        doc.remove("auth");
        doc["auth"] = toml_edit::table();
    }
    doc["auth"]["preferred_method"] = toml_edit::value("api_key");
    Ok(())
}

/// Flatten overlay + optional full toml snapshot onto an `api_key` credentials object.
pub fn overlay_into_credentials(map: &mut Map<String, Value>, overlay: &GrokApiKeyOverlay) {
    if !overlay.alias.trim().is_empty() {
        map.insert("alias".into(), json!(overlay.alias));
    }
    if let Some(provider) = overlay.model_provider.as_deref().filter(|s| !s.is_empty()) {
        map.insert("model_provider".into(), json!(provider));
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
        model_provider: str_field("model_provider"),
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
