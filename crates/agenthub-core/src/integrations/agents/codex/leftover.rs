//! Strip leftover AgentHub 本机路由 keys from official Codex config.
//!
//! Official ChatGPT OAuth uses auth.json. AgentHub 本机路由 writes
//! `model_provider = agenthub_*_bridge` plus a 127.0.0.1 table. Switching
//! back to official login must drop those keys or Codex sends the ChatGPT
//! token at loopback and 401s.

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
    }
    leftover_slugs(&doc).next().is_some()
}

/// Remove AgentHub leftover keys. Returns whether the document changed.
pub fn strip_bridge_leftovers_in_doc(doc: &mut DocumentMut) -> bool {
    let slugs: Vec<String> = leftover_slugs(doc).collect();
    if slugs.is_empty() {
        return clear_apikey_auth_preference(doc);
    }

    let mut changed = false;
    let top = doc
        .get("model_provider")
        .and_then(|item| item.as_str())
        .map(str::to_string);
    if top.as_deref().is_some_and(is_leftover_slug) {
        doc.remove("model_provider");
        changed = true;
    }
    if clear_apikey_auth_preference(doc) {
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
            drop(providers);
            doc.remove("model_providers");
            changed = true;
        }
    }
    changed
}

/// Rewrite `config.toml` in place when leftover keys are present.
pub fn strip_bridge_leftovers_in_path(path: &Path) -> Result<bool> {
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
    if !strip_bridge_leftovers_in_doc(&mut doc) {
        return Ok(false);
    }
    atomic_write(path, doc.to_string().as_bytes())?;
    Ok(true)
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
        .is_some_and(toml_is_bridge_leftover)
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

fn clear_apikey_auth_preference(doc: &mut DocumentMut) -> bool {
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

fn content_has_agenthub_bridge_marker(content: &str) -> bool {
    content.contains("agenthub_") && content.contains("_bridge") && content.contains("127.0.0.1")
}

fn toml_file_is_leftover(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .is_some_and(|text| toml_is_bridge_leftover(&text))
}

#[cfg(test)]
mod tests;
