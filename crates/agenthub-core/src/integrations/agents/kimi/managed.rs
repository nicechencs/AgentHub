//! Single source for Kimi provider-managed TOML keys.

use toml_edit::{DocumentMut, Item};

use crate::error::{AppError, Result};

pub const PROVIDER_TOML_KEYS: &[&str] =
    &["default_model", "default_provider", "providers", "models"];

/// Native TOML keys the projector writes. Must stay ⊆ [`PROVIDER_TOML_KEYS`].
// Referenced only from `tests.rs` in this crate; keep for test coverage.
#[allow(dead_code)]
pub const PROJECTOR_TOML_KEYS: &[&str] =
    &["default_model", "default_provider", "providers", "models"];

pub const DEFAULT_MODEL_ALIAS: &str = "kimi-k2";
pub const DEFAULT_MAX_CONTEXT_SIZE: i64 = 131_072;

/// Ensure `[providers.<slug>]` exists and has required `type` (openai-compatible).
pub(crate) fn ensure_kimi_provider_entry(doc: &mut DocumentMut, slug: &str) -> Result<()> {
    if doc.get("providers").is_none() {
        doc["providers"] = toml_edit::table();
    }
    let providers = doc["providers"]
        .as_table_mut()
        .ok_or_else(|| AppError::InvalidArg("Kimi providers must be a table".into()))?;
    if providers.get(slug).is_none() {
        providers.insert(slug, toml_edit::table());
    }
    let entry = providers
        .get_mut(slug)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| AppError::InvalidArg(format!("Kimi providers.{slug} must be a table")))?;
    let missing_type = entry
        .get("type")
        .and_then(Item::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none();
    if missing_type {
        entry["type"] = toml_edit::value("openai");
    }
    Ok(())
}

/// Ensure `[models."<alias>"]` exists with provider + model + max_context_size.
///
/// `default_model` must be a key in `[models]`. Alias equals `default_model`.
pub(crate) fn ensure_kimi_model_alias(
    doc: &mut DocumentMut,
    slug: &str,
    alias: &str,
) -> Result<()> {
    if doc.get("models").is_none() {
        doc["models"] = toml_edit::table();
    }
    let models = doc["models"]
        .as_table_mut()
        .ok_or_else(|| AppError::InvalidArg("Kimi models must be a table".into()))?;
    if models.get(alias).is_none() {
        models.insert(alias, toml_edit::table());
    }
    let entry = models
        .get_mut(alias)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| AppError::InvalidArg(format!("Kimi models.{alias} must be a table")))?;
    entry["provider"] = toml_edit::value(slug);
    let model_id = entry
        .get("model")
        .and_then(Item::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if model_id.is_none() {
        entry["model"] = toml_edit::value(alias);
    }
    let context_ok = entry
        .get("max_context_size")
        .and_then(Item::as_integer)
        .is_some_and(|n| n >= 1);
    if !context_ok {
        entry["max_context_size"] = toml_edit::value(DEFAULT_MAX_CONTEXT_SIZE);
    }
    Ok(())
}
