//! Strip leftover AgentHub 本机路由 keys from official Codex config.
//!
//! Official ChatGPT OAuth uses auth.json. AgentHub 本机路由 writes
//! `model_provider = agenthub_*_bridge` plus a 127.0.0.1 table, and a Grok
//! `model` / `model_reasoning_effort`. Switching back to 官方登录 must drop
//! those keys or Codex sends the ChatGPT token at loopback (401) or rejects
//! leftover `grok-*` models (400). Do not invent a ChatGPT model name.
//!
//! Boundaries:
//! - `preferred_auth_method = "apikey"` is removed only when a leftover
//!   `agenthub_*_bridge` slug is present. Without a slug it stays
//!   (GLM / DeepSeek restore).
//! - A leftover `grok-*` / `claude-*` / `kimi-*` model is stripped even
//!   when bridge slugs are empty, unless `model_provider` is a non-leftover
//!   slug (e.g. `custom`). `deepseek-*` is not stripped here: GLM restore
//!   is a protection boundary.
//! - `model_reasoning_effort` is removed only when `model` is leftover.
//!   A missing `model` key does not drop effort.

use std::path::Path;

use toml_edit::DocumentMut;

use crate::error::{AppError, Result};
use crate::models::{AgentId, BackupRecord, Provider};
use crate::utils::atomic::atomic_write;
use crate::utils::paths::agent_home;

/// AgentHub-written Codex provider slugs (`agenthub_grok_bridge`, …).
pub fn is_agenthub_bridge_slug(slug: &str) -> bool {
    let slug = slug.trim();
    slug.starts_with("agenthub_") && slug.ends_with("_bridge")
}

/// True when this TOML still points Codex at an AgentHub 本机路由 leftover.
pub fn toml_is_bridge_leftover(content: &str) -> bool {
    let Ok(doc) = content.parse::<DocumentMut>() else {
        return content_has_agenthub_bridge_marker(content);
    };
    let leftover = leftover_slugs(&doc).next().is_some();
    leftover
}

/// True when the *active* `model_provider` is an AgentHub 本机路由 leftover.
///
/// A dead `agenthub_*_bridge` table next to a real `OpenAI` / `custom`
/// provider must not hide that login as leftover.
pub fn toml_active_provider_is_bridge_leftover(content: &str) -> bool {
    let Ok(doc) = content.parse::<DocumentMut>() else {
        return content_has_agenthub_bridge_marker(content);
    };
    if model_provider_is_non_leftover_slug(&doc) {
        return false;
    }
    let leftover = leftover_slugs(&doc).next().is_some();
    leftover
}

/// True when live `~/.codex/config.toml` still has leftover 本机路由 keys
/// that official apply would strip.
pub fn live_config_is_bridge_leftover() -> bool {
    agent_home(AgentId::Codex)
        .ok()
        .is_some_and(|home| toml_file_is_leftover(&home.join("config.toml")))
}

/// True when live Codex currently *uses* an AgentHub 本机路由 leftover.
pub fn live_active_provider_is_bridge_leftover() -> bool {
    agent_home(AgentId::Codex).ok().is_some_and(|home| {
        std::fs::read_to_string(home.join("config.toml"))
            .ok()
            .is_some_and(|text| toml_active_provider_is_bridge_leftover(&text))
    })
}

/// True when live Codex still has an API Key `model_provider` pointer.
///
/// Official ChatGPT OAuth uses `auth.json`. Any active `model_provider` is a
/// second live source and wins over the oauth grant (OpenRouter `env_key`,
/// custom relays, leftover 本机路由).
#[cfg_attr(not(test), allow(dead_code))]
pub fn live_oauth_has_competing_api_key_pointer() -> bool {
    agent_home(AgentId::Codex).ok().is_some_and(|home| {
        std::fs::read_to_string(home.join("config.toml"))
            .ok()
            .is_some_and(|text| toml_has_competing_api_key_pointer(&text))
    })
}

/// True when this TOML still points Codex at an API Key provider.
pub fn toml_has_competing_api_key_pointer(content: &str) -> bool {
    let Ok(doc) = content.parse::<DocumentMut>() else {
        return false;
    };
    doc.get("model_provider")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .is_some_and(|slug| !slug.is_empty())
}

/// Remove AgentHub leftover keys. Returns whether the document changed.
///
/// Bridge slugs may already be empty while a leftover `grok-*` / `claude-*` /
/// `kimi-*` model remains; 官方登录 still drops that model (and its effort)
/// unless `model_provider` is a non-leftover slug. `preferred_auth_method =
/// "apikey"` is cleared only when leftover slugs are present.
pub fn strip_bridge_leftovers_in_doc(doc: &mut DocumentMut) -> bool {
    let slugs: Vec<String> = leftover_slugs(doc).collect();
    let mut changed = false;

    if !slugs.is_empty() {
        let top = doc
            .get("model_provider")
            .and_then(|item| item.as_str())
            .map(str::to_string);
        if top.as_deref().is_some_and(is_leftover_slug) {
            doc.remove("model_provider");
            changed = true;
        }
        if let Some(providers) = doc
            .get_mut("model_providers")
            .and_then(|item| item.as_table_like_mut())
        {
            for slug in &slugs {
                if providers.remove(slug).is_some() {
                    changed = true;
                }
            }
            if providers.is_empty() {
                doc.remove("model_providers");
                changed = true;
            }
        }
    }

    if clear_apikey_auth_preference(doc, !slugs.is_empty()) {
        changed = true;
    }
    if clear_leftover_bridge_model_keys(doc) {
        changed = true;
    }
    changed
}

/// Rewrite `config.toml` in place when leftover keys are present.
pub fn strip_bridge_leftovers_in_path(path: &Path) -> Result<bool> {
    rewrite_codex_toml(path, |doc| {
        let mut changed = strip_bridge_leftovers_in_doc(doc);
        if strip_env_key_provider_leftovers_in_doc(doc) {
            changed = true;
        }
        changed
    })
}

fn rewrite_codex_toml(path: &Path, mutate: impl FnOnce(&mut DocumentMut) -> bool) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let live = std::fs::read_to_string(path)?;
    if live.trim().is_empty() {
        return Ok(false);
    }
    let mut doc = live
        .parse::<DocumentMut>()
        .map_err(|e| AppError::InvalidArg(format!("existing Codex config.toml is invalid: {e}")))?;
    if !mutate(&mut doc) {
        return Ok(false);
    }
    atomic_write(path, doc.to_string().as_bytes())?;
    Ok(true)
}

/// Official ChatGPT OAuth writes `auth.json`. A leftover `model_provider`
/// whose table requires `env_key` (OpenRouter, custom relays) still makes
/// Codex look up that environment variable and fail locally.
///
/// Deactivate the leftover pointer. Do not invent a ChatGPT model name: drop
/// OpenRouter-style leftover models (`provider/model`, `stealth/ox-*`) so
/// Codex uses its own default. Keep the unused provider table so switching
/// back to that API Key login can reuse it.
pub fn strip_env_key_provider_leftovers_in_doc(doc: &mut DocumentMut) -> bool {
    let slug = doc
        .get("model_provider")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|slug| !slug.is_empty())
        .map(str::to_string);
    let Some(slug) = slug else {
        return strip_openrouter_style_leftover_model(doc, false);
    };
    if is_leftover_slug(&slug) {
        return false;
    }
    let requires_env_key = doc
        .get("model_providers")
        .and_then(|item| item.as_table())
        .and_then(|providers| providers.get(slug.as_str()))
        .and_then(|item| item.as_table())
        .and_then(|table| table.get("env_key"))
        .and_then(|item| item.as_str())
        .map(str::trim)
        .is_some_and(|key| !key.is_empty());
    if !requires_env_key {
        return false;
    }
    doc.remove("model_provider");
    if doc
        .get("preferred_auth_method")
        .and_then(|item| item.as_str())
        == Some("apikey")
    {
        doc.remove("preferred_auth_method");
    }
    strip_openrouter_style_leftover_model(doc, true);
    true
}

fn strip_openrouter_style_leftover_model(doc: &mut DocumentMut, env_key_leftover: bool) -> bool {
    if !env_key_leftover && model_provider_is_non_leftover_slug(doc) {
        return false;
    }
    let model = doc
        .get("model")
        .and_then(|item| item.as_str())
        .map(str::to_string);
    let leftover = model
        .as_deref()
        .is_some_and(is_openrouter_style_leftover_model);
    if !leftover {
        return false;
    }
    doc.remove("model");
    if doc.get("model_reasoning_effort").is_some() {
        doc.remove("model_reasoning_effort");
    }
    if doc
        .get("review_model")
        .and_then(|item| item.as_str())
        .is_some_and(is_openrouter_style_leftover_model)
    {
        doc.remove("review_model");
    }
    true
}

fn is_openrouter_style_leftover_model(model: &str) -> bool {
    let model = model.trim();
    if model.is_empty() {
        return false;
    }
    model.contains('/')
        || model.starts_with("stealth/")
        || crate::models::is_openrouter_backup_model(model)
}

/// Strip leftover AgentHub keys from the live Codex config.toml.
pub fn strip_live_bridge_leftovers() -> Result<bool> {
    let path = agent_home(AgentId::Codex)?.join("config.toml");
    strip_bridge_leftovers_in_path(&path)
}

/// True when a pool row is itself an AgentHub 本机路由 leftover.
pub fn provider_is_bridge_leftover(provider: &Provider) -> bool {
    if provider.agent_id != AgentId::Codex {
        return false;
    }
    if provider
        .meta
        .get("generatedBy")
        .and_then(|value| value.as_str())
        == Some("adapter")
        && provider
            .meta
            .pointer("/adapterBridge/loopbackOnly")
            .and_then(|value| value.as_bool())
            == Some(true)
    {
        return true;
    }
    provider
        .settings_config
        .get("content")
        .and_then(|value| value.as_str())
        .is_some_and(toml_active_provider_is_bridge_leftover)
}

/// True when a live snapshot's Codex config.toml is a 本机路由 leftover.
pub fn backup_is_bridge_leftover(record: &BackupRecord) -> bool {
    if record.agent_id != Some(AgentId::Codex) {
        return false;
    }
    let root = Path::new(&record.path);
    if toml_file_is_leftover(&root.join("config.toml")) {
        return true;
    }
    record.files.iter().any(|name| {
        let lower = name.to_ascii_lowercase();
        (lower.ends_with(".toml") || lower.contains("config"))
            && toml_file_is_leftover(&root.join(name))
    })
}

fn leftover_slugs(doc: &DocumentMut) -> impl Iterator<Item = String> + '_ {
    let top = doc
        .get("model_provider")
        .and_then(|item| item.as_str())
        .map(str::to_string);
    let from_top = top.filter(|slug| is_leftover_slug(slug));
    let from_table = doc
        .get("model_providers")
        .and_then(|item| item.as_table())
        .into_iter()
        .flat_map(|table| {
            table.iter().filter_map(|(slug, item)| {
                if is_leftover_provider_table(slug, item.as_table()) {
                    Some(slug.to_string())
                } else {
                    None
                }
            })
        });
    from_top.into_iter().chain(from_table)
}

fn is_leftover_slug(slug: &str) -> bool {
    is_agenthub_bridge_slug(slug)
}

fn is_leftover_provider_table(slug: &str, _table: Option<&toml_edit::Table>) -> bool {
    is_agenthub_bridge_slug(slug)
}

fn clear_apikey_auth_preference(doc: &mut DocumentMut, has_leftover_slug: bool) -> bool {
    if !has_leftover_slug {
        return false;
    }
    let pref = doc
        .get("preferred_auth_method")
        .and_then(|item| item.as_str())
        .map(str::to_string);
    if pref.as_deref() != Some("apikey") {
        return false;
    }
    doc.remove("preferred_auth_method");
    true
}

fn is_leftover_bridge_model(model: &str) -> bool {
    let model = model.trim();
    model.starts_with("grok-") || model.starts_with("claude-") || model.starts_with("kimi-")
}

fn model_provider_is_non_leftover_slug(doc: &DocumentMut) -> bool {
    doc.get("model_provider")
        .and_then(|item| item.as_str())
        .is_some_and(|slug| !is_leftover_slug(slug))
}

/// Drop leftover bridge `model` (`grok-*` / `claude-*` / `kimi-*`) and its
/// `model_reasoning_effort`.
///
/// Keep official `gpt-*` models, `mcp_servers`, and `disable_response_storage`.
/// A non-leftover `model_provider` (e.g. `custom`) keeps leftover models and
/// effort. A missing `model` key does not drop effort (`None => false`).
fn clear_leftover_bridge_model_keys(doc: &mut DocumentMut) -> bool {
    if model_provider_is_non_leftover_slug(doc) {
        return false;
    }
    let mut changed = false;
    let model = doc
        .get("model")
        .and_then(|item| item.as_str())
        .map(str::to_string);
    if model.as_deref().is_some_and(is_leftover_bridge_model) {
        doc.remove("model");
        changed = true;
    }
    let leftover_reasoning = match model.as_deref() {
        Some(value) => is_leftover_bridge_model(value),
        None => false,
    };
    if leftover_reasoning && doc.get("model_reasoning_effort").is_some() {
        doc.remove("model_reasoning_effort");
        changed = true;
    }
    changed
}

fn content_has_agenthub_bridge_marker(content: &str) -> bool {
    content.contains("agenthub_") && content.contains("_bridge") && content.contains("127.0.0.1")
}

fn toml_file_is_leftover(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .is_some_and(|text| toml_is_bridge_leftover(&text))
}

#[cfg(test)]
pub(crate) fn lock_codex_home() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests;
