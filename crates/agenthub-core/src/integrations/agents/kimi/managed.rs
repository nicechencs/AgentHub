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

#[allow(dead_code)]
pub const DEFAULT_MODEL_ALIAS: &str = "kimi-k2";
pub const DEFAULT_MAX_CONTEXT_SIZE: i64 = 131_072;

/// Kimi Code `providers.<slug>.type` for an API root.
///
/// `anthropic` → Messages, `openai_responses` → Responses, `kimi` → official
/// Moonshot/Kimi platform, `openai` → Chat Completions (custom relays / loopback).
pub(crate) fn kimi_provider_type_for_url(url: Option<&str>) -> &'static str {
    let Some(url) = url.map(str::trim).filter(|s| !s.is_empty()) else {
        return "openai";
    };
    let lower = url.to_ascii_lowercase();
    if lower.contains("/anthropic")
        || lower.contains("/v1/messages")
        || lower.ends_with("/messages")
    {
        return "anthropic";
    }
    if lower.contains("/v1/responses") || lower.contains("/responses") {
        return "openai_responses";
    }
    if is_official_kimi_platform_url(&lower) {
        return "kimi";
    }
    "openai"
}

fn is_official_kimi_platform_url(lower: &str) -> bool {
    if lower.contains("/coding") {
        return false;
    }
    lower.contains("api.moonshot.") || lower.contains("api.kimi.com")
}

fn provider_type_missing(entry: &toml_edit::Table) -> bool {
    entry
        .get("type")
        .and_then(Item::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
}

fn fill_missing_type(entry: &mut toml_edit::Table) {
    if !provider_type_missing(entry) {
        return;
    }
    let url = entry.get("base_url").and_then(|v| v.as_str());
    entry["type"] = toml_edit::value(kimi_provider_type_for_url(url));
}

/// Ensure `[providers.<slug>]` exists. Call [`fill_missing_kimi_provider_type`]
/// after `base_url` is known so type is inferred from the final address.
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
    providers
        .get_mut(slug)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| AppError::InvalidArg(format!("Kimi providers.{slug} must be a table")))?;
    Ok(())
}

/// Fill `type` after `base_url` is known. Does not overwrite an existing type.
pub(crate) fn fill_missing_kimi_provider_type(doc: &mut DocumentMut, slug: &str) -> Result<()> {
    let Some(entry) = doc
        .get_mut("providers")
        .and_then(Item::as_table_mut)
        .and_then(|t| t.get_mut(slug))
        .and_then(Item::as_table_mut)
    else {
        return Ok(());
    };
    fill_missing_type(entry);
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

/// Backfill provider `type` and `[models."<alias>"]` after a pool/live merge.
///
/// Switch writes `settings_config` through [`write_toml_config`] and does not
/// run the projector. Old logins with only `default_model` + `[providers.*]`
/// must still land a complete file. A membership-only doc with no providers
/// is left unchanged.
pub(crate) fn complete_kimi_live_toml(doc: &mut DocumentMut) -> Result<()> {
    let slug = doc
        .get("default_provider")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            doc.get("providers")
                .and_then(|p| p.as_table())
                .and_then(|t| t.iter().next().map(|(k, _)| k.to_string()))
        });
    let Some(slug) = slug else {
        return Ok(());
    };
    if doc
        .get("default_provider")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        doc["default_provider"] = toml_edit::value(slug.as_str());
    }
    ensure_kimi_provider_entry(doc, slug.as_str())?;
    fill_missing_kimi_provider_type(doc, slug.as_str())?;
    let stored = doc
        .get("default_model")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    // Keep the account's model. Rewriting grok-* to kimi-k2 made custom
    // relays 404: the key's group never had that alias.
    let Some(alias) = stored else {
        return Ok(());
    };
    ensure_kimi_model_alias(doc, slug.as_str(), &alias)
}

#[cfg(test)]
mod tests;
