//! Provider identity: `sha256(secret) + normalized_base_url + agent_id`.
//!
//! Persist the hash in `meta.secretHash`. Never persist the raw secret as the
//! identity key. Last4 / label are display-only and must not merge rows.

use serde_json::{json, Value};

use crate::models::{AdapterProfile, AdapterSourceKind, AgentId, Provider};
use crate::services::switch_undo::extract_probe_url;
use crate::utils::redact::api_key_secret_hash;

pub const META_SECRET_HASH: &str = "secretHash";

/// Opaque identity for same-agent provider merge.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProviderIdentity {
    pub agent_id: AgentId,
    pub secret_hash: String,
    pub base_url: String,
}

pub fn provider_identity(provider: &Provider) -> Option<ProviderIdentity> {
    if is_generated_adapter_provider(provider) {
        return None;
    }
    identity_from_parts(
        provider.agent_id,
        &provider.settings_config,
        provider.meta.get(META_SECRET_HASH).and_then(Value::as_str),
    )
}

pub fn identity_from_parts(
    agent_id: AgentId,
    settings: &Value,
    persisted_hash: Option<&str>,
) -> Option<ProviderIdentity> {
    let secret_hash = api_key_secret_hash(settings).or_else(|| {
        persisted_hash
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })?;
    let base_url = normalize_provider_base_url(settings)?;
    Some(ProviderIdentity {
        agent_id,
        secret_hash,
        base_url,
    })
}

pub fn stamp_secret_hash(meta: &mut Value, settings: &Value) {
    let Some(hash) = api_key_secret_hash(settings) else {
        return;
    };
    if let Value::Object(map) = meta {
        map.insert(META_SECRET_HASH.into(), json!(hash));
        return;
    }
    *meta = json!({ META_SECRET_HASH: hash });
}

pub fn normalize_provider_base_url(settings: &Value) -> Option<String> {
    if let Some(url) = extract_probe_url(settings) {
        return Some(normalize_base_url(&url));
    }
    let content = settings
        .get("content")
        .or_else(|| settings.get("config"))
        .and_then(Value::as_str)?;
    extract_toml_base_url(content).map(|url| normalize_base_url(&url))
}

pub fn normalize_base_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    let without_v1 = trimmed
        .strip_suffix("/v1")
        .or_else(|| trimmed.strip_suffix("/V1"))
        .unwrap_or(trimmed)
        .trim_end_matches('/');
    without_v1.to_string()
}

pub fn looks_like_uuid_provider_id(id: &str) -> bool {
    id.split('-')
        .any(|part| part.len() == 12 && part.chars().all(|c| c.is_ascii_hexdigit()))
        && id.chars().filter(|c| *c == '-').count() >= 4
}

pub fn is_backup_slug_id(id: &str) -> bool {
    id.rsplit('-')
        .next()
        .is_some_and(|part| part.eq_ignore_ascii_case("backup"))
}

pub fn is_generated_adapter_provider(provider: &Provider) -> bool {
    provider.meta.get("generatedBy").and_then(Value::as_str) == Some("adapter")
}

/// Pick the row to keep when several share an identity.
///
/// Prefer the UUID that already has adapter projections / bindings; drop
/// leftover `*-backup` slugs.
pub fn pick_identity_keeper<'a>(
    rows: &'a [Provider],
    profiles: &[AdapterProfile],
) -> Option<&'a Provider> {
    if rows.is_empty() {
        return None;
    }
    let mut ranked: Vec<(&Provider, i32, &str)> = rows
        .iter()
        .map(|row| (row, keeper_score(row, profiles), row.updated_at.as_str()))
        .collect();
    ranked.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| right.2.cmp(left.2))
            .then_with(|| right.0.id.cmp(&left.0.id))
    });
    ranked.into_iter().next().map(|(row, _, _)| row)
}

fn keeper_score(row: &Provider, profiles: &[AdapterProfile]) -> i32 {
    let mut score = 0;
    if profiles.iter().any(|profile| {
        profile.source_kind == AdapterSourceKind::Provider && profile.source_id == row.id
    }) {
        score += 100;
    }
    if profiles
        .iter()
        .any(|profile| profile.generated_provider_id.as_deref() == Some(row.id.as_str()))
    {
        score += 40;
    }
    if looks_like_uuid_provider_id(&row.id) {
        score += 20;
    }
    if is_backup_slug_id(&row.id) {
        score -= 30;
    }
    if row.is_current {
        score += 50;
    }
    score
}

pub fn retarget_profiles_from_loser(
    profiles: &mut [AdapterProfile],
    loser_id: &str,
    keeper_id: &str,
) -> Vec<usize> {
    let mut changed = Vec::new();
    for (index, profile) in profiles.iter_mut().enumerate() {
        let mut dirty = false;
        if profile.source_kind == AdapterSourceKind::Provider && profile.source_id == loser_id {
            profile.source_id = keeper_id.to_owned();
            dirty = true;
        }
        if profile.generated_provider_id.as_deref() == Some(loser_id) {
            profile.generated_provider_id = Some(keeper_id.to_owned());
            dirty = true;
        }
        if dirty {
            changed.push(index);
        }
    }
    changed
}

fn extract_toml_base_url(content: &str) -> Option<String> {
    let doc: toml_edit::DocumentMut = content.parse().ok()?;
    let configured_slug = doc
        .get("model_provider")
        .or_else(|| doc.get("default_provider"))
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|slug| !slug.is_empty());
    let table_url = |table_name: &str, slug: Option<&str>| {
        let table = doc.get(table_name)?.as_table()?;
        let item = match slug {
            Some(slug) => table.get(slug),
            None => table.iter().next().map(|(_, item)| item),
        }?;
        item.get("base_url")
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(str::to_owned)
    };

    // Codex writes `[model_providers.<name>]`; Kimi writes
    // `[providers.<name>]` with `default_provider`. An explicit selector must
    // resolve exactly; falling back could pair a saved key with another URL.
    if let Some(slug) = configured_slug {
        return table_url("model_providers", Some(slug))
            .or_else(|| table_url("providers", Some(slug)));
    }

    // Legacy files without a selector are safe only when exactly one provider
    // entry exists across both supported table shapes.
    let provider_count = ["model_providers", "providers"]
        .into_iter()
        .filter_map(|name| doc.get(name).and_then(|item| item.as_table()))
        .map(toml_edit::Table::len)
        .sum::<usize>();
    if provider_count > 0 {
        return (provider_count == 1)
            .then(|| table_url("model_providers", None).or_else(|| table_url("providers", None)))
            .flatten();
    }

    doc.get("base_url")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests;
