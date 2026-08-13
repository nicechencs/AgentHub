//! Shared Grok Build `config.toml` model-registry shape helpers.
//!
//! Grok stores providers under `[models]` and `[model."<alias>"]`. The legacy
//! top-level shape is still read and migrated so existing installations remain
//! usable. Adapter account writers and the config projector both use this module
//! so migration rules cannot drift.

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
        doc.get("api_key")
            .and_then(Item::as_str)
            .map(str::to_owned)
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
