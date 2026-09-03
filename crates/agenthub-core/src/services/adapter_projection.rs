//! Classify live credentials as a user login vs AgentHub's own projection.
//!
//! Local routing writes a generated provider and live files. That material is
//! not a login. Matching uses ownership (generated provider / local bearer /
//! AgentHub bridge slugs / profile port), not "host is 127.0.0.1" alone.

use serde_json::Value;

use crate::integrations::agents::codex::leftover;
use crate::models::{AccountKind, AdapterProfile, AdapterRoute, AgentId, Provider};
use crate::services::switch_undo::extract_probe_url;
use crate::utils::loopback::is_loopback_base_url;

const ADAPTER_GENERATED_BY: &str = "adapter";
const LOCAL_BEARER_PREFIX: &str = "ahb_";

/// Probe overlay kind for GUI import gates (`AuthState.also_present`).
pub const ADAPTER_PROJECTION_KIND: &str = "adapter_projection";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveOrigin {
    /// A real user grant (official login or a Key the user owns).
    UserGrant,
    /// Current live is the generated provider AgentHub just applied.
    ActiveProjection,
    /// Live still looks like something we wrote, but it is not the current
    /// generated provider (unbind leftover, orphan files).
    LeftoverProjection,
}

impl LiveOrigin {
    pub fn is_projection(self) -> bool {
        !matches!(self, Self::UserGrant)
    }
}

pub fn leftover_live_flag(agent: AgentId) -> bool {
    agent == AgentId::Codex && leftover::live_active_provider_is_bridge_leftover()
}

pub fn generated_provider_is_adapter_owned(provider: &Provider) -> bool {
    provider
        .meta
        .get("generatedBy")
        .and_then(|value| value.as_str())
        == Some(ADAPTER_GENERATED_BY)
}

/// Classify one live *account* snapshot for import.
pub fn classify_account_live(
    agent: AgentId,
    kind: AccountKind,
    credentials: &Value,
    profiles: &[AdapterProfile],
    providers: &[Provider],
    leftover_live: bool,
) -> LiveOrigin {
    if let Some(current) = current_generated_provider(agent, providers) {
        if live_matches_generated(credentials, current)
            || live_matches_our_projection(agent, credentials, profiles, providers)
        {
            return LiveOrigin::ActiveProjection;
        }
    }
    if live_matches_our_projection(agent, credentials, profiles, providers) {
        return LiveOrigin::LeftoverProjection;
    }
    if leftover_live && kind != AccountKind::Oauth {
        return LiveOrigin::LeftoverProjection;
    }
    LiveOrigin::UserGrant
}

pub fn should_skip_live_reconcile(
    agent: AgentId,
    kind: AccountKind,
    credentials: &Value,
    profiles: &[AdapterProfile],
    providers: &[Provider],
    leftover_live: bool,
) -> bool {
    classify_account_live(agent, kind, credentials, profiles, providers, leftover_live)
        .is_projection()
}

/// Classify a live *provider* config snapshot (the agent's config file).
pub fn classify_provider_config(
    agent: AgentId,
    raw: &Value,
    profiles: &[AdapterProfile],
    providers: &[Provider],
    leftover_live: bool,
) -> LiveOrigin {
    if let Some(current) = current_generated_provider(agent, providers) {
        if live_matches_generated(raw, current)
            || live_matches_our_projection(agent, raw, profiles, providers)
        {
            return LiveOrigin::ActiveProjection;
        }
    }
    if leftover_live || live_matches_our_projection(agent, raw, profiles, providers) {
        return LiveOrigin::LeftoverProjection;
    }
    LiveOrigin::UserGrant
}

pub fn projection_import_error() -> crate::error::AppError {
    crate::error::AppError::message(
        "account.import_projection",
        "当前是本机路由写进去的配置，不是一份新登录",
    )
}

fn current_generated_provider(agent: AgentId, providers: &[Provider]) -> Option<&Provider> {
    providers.iter().find(|provider| {
        provider.agent_id == agent
            && provider.is_current
            && generated_provider_is_adapter_owned(provider)
    })
}

fn live_matches_generated(credentials: &Value, generated: &Provider) -> bool {
    live_matches_our_projection(
        generated.agent_id,
        credentials,
        &[],
        std::slice::from_ref(generated),
    )
}

fn live_matches_our_projection(
    agent: AgentId,
    credentials: &Value,
    profiles: &[AdapterProfile],
    providers: &[Provider],
) -> bool {
    if credentials_have_local_bearer(credentials)
        || credentials_have_agenthub_bridge_marker(credentials)
    {
        return true;
    }
    let Some(url) = extract_probe_url(credentials) else {
        return false;
    };
    if !is_loopback_base_url(&url) {
        return false;
    }
    let port = loopback_url_port(&url);
    if port.is_some_and(|port| {
        profiles.iter().any(|profile| {
            profile.target_agent_id == agent
                && profile.route == AdapterRoute::LocalBridge
                && profile.local_port == Some(port)
        })
    }) {
        return true;
    }
    providers.iter().any(|provider| {
        provider.agent_id == agent
            && generated_provider_is_adapter_owned(provider)
            && extract_probe_url(&provider.settings_config).as_deref() == Some(url.as_str())
    })
}

fn loopback_url_port(raw: &str) -> Option<u16> {
    reqwest::Url::parse(raw.trim())
        .ok()
        .and_then(|url| url.port())
}

fn credentials_have_local_bearer(value: &Value) -> bool {
    match value {
        Value::String(text) => text.trim().starts_with(LOCAL_BEARER_PREFIX),
        Value::Array(items) => items.iter().any(credentials_have_local_bearer),
        Value::Object(map) => map.values().any(credentials_have_local_bearer),
        _ => false,
    }
}

fn credentials_have_agenthub_bridge_marker(value: &Value) -> bool {
    if let Some(content) = value.get("content").and_then(Value::as_str) {
        let is_toml = value
            .get("format")
            .and_then(Value::as_str)
            .is_some_and(|format| format.eq_ignore_ascii_case("toml"))
            || content.contains("model_provider")
            || content.contains("[model_providers.");
        if is_toml {
            return leftover::toml_active_provider_is_bridge_leftover(content);
        }
    }
    match value {
        Value::String(text) => text_contains_bridge_slug(text),
        Value::Array(items) => items.iter().any(credentials_have_agenthub_bridge_marker),
        Value::Object(map) => map
            .iter()
            .filter(|(key, _)| *key != "content")
            .any(|(_, child)| credentials_have_agenthub_bridge_marker(child)),
        _ => false,
    }
}

fn text_contains_bridge_slug(text: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .any(leftover::is_agenthub_bridge_slug)
}

/// Find the unique adapter-generated provider whose loopback URL + local bearer
/// exactly match live config. Used to heal `is_current` / active binding when
/// live settings still point at one bridge while the binding stuck on another
/// (e.g. OpenAI @40661 vs Codex @44227).
pub fn exact_generated_provider_for_live<'a>(
    agent: AgentId,
    live: &Value,
    providers: &'a [Provider],
) -> Option<&'a Provider> {
    let live_url = extract_probe_url(live)?;
    let live_bearer = extract_local_bearer_token(live)?;
    let mut matched: Option<&Provider> = None;
    for provider in providers {
        if provider.agent_id != agent || !generated_provider_is_adapter_owned(provider) {
            continue;
        }
        let Some(url) = extract_probe_url(&provider.settings_config) else {
            continue;
        };
        let Some(bearer) = extract_local_bearer_token(&provider.settings_config) else {
            continue;
        };
        if url != live_url || bearer != live_bearer {
            continue;
        }
        if matched.is_some() {
            return None;
        }
        matched = Some(provider);
    }
    matched
}

fn extract_local_bearer_token(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.starts_with(LOCAL_BEARER_PREFIX) && trimmed.len() > LOCAL_BEARER_PREFIX.len()
            {
                Some(trimmed.to_owned())
            } else {
                None
            }
        }
        Value::Array(items) => items.iter().find_map(extract_local_bearer_token),
        Value::Object(map) => {
            for key in [
                "ANTHROPIC_AUTH_TOKEN",
                "api_key",
                "apiKey",
                "token",
                "local_token",
                "localToken",
            ] {
                if let Some(found) = map.get(key).and_then(extract_local_bearer_token) {
                    return Some(found);
                }
            }
            map.values().find_map(extract_local_bearer_token)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
