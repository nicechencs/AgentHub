use serde_json::{json, Value};
use toml_edit::DocumentMut;

use super::*;
use crate::adapters::pi_auth::pi_oauth_entry_from_tokens;
use crate::bridge::ResolvedAuth;
use crate::error::{AppError, Result};
use crate::models::{AdapterSourceKind, AgentId, Provider};
use crate::services::adapter_route_constants::*;
use crate::storage::{AccountRepo, Database, ProviderRepo};

/// Generated-provider allowlists. Every applyable matrix cell and live
/// bridge rule must match one of these; the coverage test in `tests.rs`
/// fails CI when a published rule id is omitted.
pub(super) fn is_claude_source_reference(provider: &Provider) -> bool {
    provider.agent_id == AgentId::Claude
        && matches!(
            adapter_rule_id(provider),
            Some(KIMI_TO_CLAUDE_RULE) | Some(GLM_TO_CLAUDE_RULE) | Some(DEEPSEEK_TO_CLAUDE_RULE)
        )
        && provider
            .meta
            .get("adapterRuleVersion")
            .and_then(Value::as_u64)
            == Some(1)
        && provider
            .meta
            .get("adapterSecretMode")
            .and_then(Value::as_str)
            == Some(SOURCE_REFERENCE_MODE)
}

pub(super) fn is_codex_source_reference(provider: &Provider) -> bool {
    provider.agent_id == AgentId::Codex
        && matches!(
            adapter_rule_id(provider),
            Some(GLM_TO_CODEX_RULE) | Some(DEEPSEEK_TO_CODEX_RULE)
        )
        && provider
            .meta
            .get("adapterRuleVersion")
            .and_then(Value::as_u64)
            == Some(1)
        && provider
            .meta
            .get("adapterSecretMode")
            .and_then(Value::as_str)
            == Some(SOURCE_REFERENCE_MODE)
}

pub(super) fn is_dsh_source_reference(provider: &Provider) -> bool {
    provider.agent_id == AgentId::Dsh
        && adapter_rule_id(provider) == Some(DEEPSEEK_TO_DSH_RULE)
        && provider
            .meta
            .get("adapterRuleVersion")
            .and_then(Value::as_u64)
            == Some(1)
        && provider
            .meta
            .get("adapterSecretMode")
            .and_then(Value::as_str)
            == Some(SOURCE_REFERENCE_MODE)
}

pub(super) fn is_grok_source_reference(provider: &Provider) -> bool {
    provider.agent_id == AgentId::Grok
        && matches!(
            adapter_rule_id(provider),
            Some(KIMI_TO_GROK_RULE) | Some(OPENAI_TO_GROK_RULE)
        )
        && provider
            .meta
            .get("adapterRuleVersion")
            .and_then(Value::as_u64)
            == Some(1)
        && provider
            .meta
            .get("adapterSecretMode")
            .and_then(Value::as_str)
            == Some(SOURCE_REFERENCE_MODE)
}

pub(super) fn is_pi_source_reference(provider: &Provider) -> bool {
    provider.agent_id == AgentId::Pi
        && matches!(
            adapter_rule_id(provider),
            Some(KIMI_TO_PI_RULE)
                | Some(ANTHROPIC_TO_PI_RULE)
                | Some(OPENAI_TO_PI_RULE)
                | Some(XAI_TO_PI_RULE)
                | Some(GLM_TO_PI_RULE)
                | Some(DEEPSEEK_TO_PI_RULE)
                | Some(CLAUDE_SUBSCRIPTION_PI_RULE)
                | Some(CODEX_SUBSCRIPTION_PI_RULE)
                | Some(GROK_SUBSCRIPTION_PI_RULE)
        )
        && provider
            .meta
            .get("adapterRuleVersion")
            .and_then(Value::as_u64)
            == Some(1)
        && provider
            .meta
            .get("adapterSecretMode")
            .and_then(Value::as_str)
            == Some(SOURCE_REFERENCE_MODE)
}

pub(super) fn is_pi_subscription_reference(provider: &Provider) -> bool {
    matches!(
        adapter_rule_id(provider),
        Some(CLAUDE_SUBSCRIPTION_PI_RULE)
            | Some(CODEX_SUBSCRIPTION_PI_RULE)
            | Some(GROK_SUBSCRIPTION_PI_RULE)
    )
}

pub(super) fn is_subscription_pi_rule(rule_id: &str) -> bool {
    matches!(
        rule_id,
        CLAUDE_SUBSCRIPTION_PI_RULE | CODEX_SUBSCRIPTION_PI_RULE | GROK_SUBSCRIPTION_PI_RULE
    )
}

pub(super) fn adapter_rule_id(provider: &Provider) -> Option<&str> {
    provider.meta.get("adapterRuleId").and_then(Value::as_str)
}

pub(super) fn codex_contract(rule_id: &str) -> Result<(&'static str, &'static str)> {
    match rule_id {
        GLM_TO_CODEX_RULE => Ok((GLM_CODEX_BASE_URL, GLM_CODEX_PROVIDER_SLUG)),
        DEEPSEEK_TO_CODEX_RULE => Ok((DEEPSEEK_CODEX_BASE_URL, DEEPSEEK_CODEX_PROVIDER_SLUG)),
        _ => Err(invalid_reference()),
    }
}

pub(super) fn codex_provider_slug(rule_id: &str) -> Result<&'static str> {
    codex_contract(rule_id).map(|(_, slug)| slug)
}

pub(super) fn grok_contract(rule_id: &str) -> Result<(&'static str, &'static str, &'static str)> {
    match rule_id {
        KIMI_TO_GROK_RULE => Ok((KIMI_GROK_BASE_URL, "kimi-k2.5", "agenthub_kimi")),
        OPENAI_TO_GROK_RULE => Ok((OPENAI_GROK_BASE_URL, "gpt-4o", "agenthub_openai")),
        _ => Err(invalid_reference()),
    }
}

pub(super) fn pi_slot_name(provider: &Provider) -> Result<&'static str> {
    match adapter_rule_id(provider) {
        Some(KIMI_TO_PI_RULE) => Ok(KIMI_PI_PROVIDER_SLOT),
        Some(ANTHROPIC_TO_PI_RULE) => Ok(ANTHROPIC_PI_PROVIDER_SLOT),
        Some(OPENAI_TO_PI_RULE) => Ok(OPENAI_PI_PROVIDER_SLOT),
        Some(XAI_TO_PI_RULE) => Ok(XAI_PI_PROVIDER_SLOT),
        Some(GLM_TO_PI_RULE) => Ok(GLM_PI_PROVIDER_SLOT),
        Some(DEEPSEEK_TO_PI_RULE) => Ok(DEEPSEEK_PI_PROVIDER_SLOT),
        Some(CLAUDE_SUBSCRIPTION_PI_RULE) => Ok(ANTHROPIC_PI_PROVIDER_SLOT),
        Some(CODEX_SUBSCRIPTION_PI_RULE) => Ok("openai-codex"),
        Some(GROK_SUBSCRIPTION_PI_RULE) => Ok(XAI_PI_PROVIDER_SLOT),
        _ => Err(invalid_reference()),
    }
}

pub(super) fn pi_base_url_for_rule(rule_id: &str) -> Option<&'static str> {
    match rule_id {
        KIMI_TO_PI_RULE => Some(KIMI_PI_BASE_URL),
        GLM_TO_PI_RULE => Some(GLM_PI_BASE_URL),
        DEEPSEEK_TO_PI_RULE => Some(DEEPSEEK_API_BASE_URL),
        _ => None,
    }
}

pub(super) fn pi_slot_object<'a>(settings: &'a Value, slot: &str) -> Option<&'a Value> {
    settings
        .get("models")
        .and_then(|models| models.get("providers"))
        .and_then(|providers| providers.get(slot))
        .filter(|value| value.is_object())
}

pub(super) fn pi_auth_slot_object<'a>(settings: &'a Value, slot: &str) -> Option<&'a Value> {
    settings
        .get("auth")
        .and_then(Value::as_object)
        .and_then(|auth| auth.get(slot))
        .filter(|value| value.is_object())
}

pub(super) fn set_pi_slot_api_key(settings: &mut Value, slot: &str, api_key: &str) -> Result<()> {
    let provider = settings
        .get_mut("models")
        .and_then(Value::as_object_mut)
        .and_then(|models| models.get_mut("providers"))
        .and_then(Value::as_object_mut)
        .and_then(|providers| providers.get_mut(slot))
        .and_then(Value::as_object_mut)
        .ok_or_else(invalid_reference)?;
    provider.insert("apiKey".into(), Value::String(api_key.into()));
    Ok(())
}

/// Live Pi envelopes now include `auth.json`. Never persist those tokens into a
/// provider row; saga snapshots keep the in-memory original for rollback.
pub(super) fn strip_pi_auth_for_persist(provider: &Provider, live_raw: &Value) -> Value {
    if provider.agent_id != AgentId::Pi {
        return live_raw.clone();
    }
    let mut scrubbed = live_raw.clone();
    if let Some(object) = scrubbed.as_object_mut() {
        object.remove("auth");
    }
    scrubbed
}

pub(super) fn set_pi_slot_oauth(
    settings: &mut Value,
    slot: &str,
    access: &str,
    refresh: Option<&str>,
    expires_at: Option<&str>,
) -> Result<()> {
    let auth = settings
        .get_mut("auth")
        .and_then(Value::as_object_mut)
        .ok_or_else(invalid_reference)?;
    let entry = pi_oauth_entry_from_tokens(access, refresh, expires_at, None);
    auth.insert(slot.into(), entry);
    Ok(())
}

pub(super) fn provider_explicit_tag(source: &Provider) -> Option<&str> {
    source
        .meta
        .get("preset")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            source
                .meta
                .get("provider")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
}

pub(super) fn is_anthropic_api_source(source: &Provider) -> bool {
    source.agent_id == AgentId::Claude
        && (source.meta.get("preset").and_then(Value::as_str) == Some(ANTHROPIC_PRESET)
            || settings_contain_anthropic_api_endpoint(&source.settings_config))
}

pub(super) fn is_openai_api_source(source: &Provider) -> bool {
    is_openai_api_marker(provider_explicit_tag(source), &source.settings_config)
}

pub(super) fn is_xai_api_source(source: &Provider) -> bool {
    is_xai_api_marker(provider_explicit_tag(source), &source.settings_config)
}

pub(super) fn is_glm_coding_plan_source(source: &Provider) -> bool {
    is_glm_coding_plan_marker(provider_explicit_tag(source), &source.settings_config)
}

pub(super) fn is_deepseek_api_source(source: &Provider) -> bool {
    is_deepseek_api_marker(provider_explicit_tag(source), &source.settings_config)
}

pub(super) fn provider_matches_explicit_api_rule(rule_id: &str, source: &Provider) -> bool {
    match rule_id {
        ANTHROPIC_TO_PI_RULE => is_anthropic_api_source(source),
        OPENAI_TO_PI_RULE | OPENAI_TO_GROK_RULE => is_openai_api_source(source),
        XAI_TO_PI_RULE => is_xai_api_source(source),
        GLM_TO_CLAUDE_RULE | GLM_TO_PI_RULE | GLM_TO_CODEX_RULE => {
            is_glm_coding_plan_source(source)
        }
        DEEPSEEK_TO_CLAUDE_RULE | DEEPSEEK_TO_PI_RULE | DEEPSEEK_TO_CODEX_RULE => {
            is_deepseek_api_source(source)
        }
        _ => false,
    }
}

pub(super) fn extract_account_api_key(credentials: &Value) -> Result<String> {
    let format = credentials
        .get("format")
        .and_then(Value::as_str)
        .map(str::trim);
    if format != Some(ACCOUNT_API_KEY_FORMAT) {
        return Err(invalid_reference());
    }
    credentials
        .get("api_key")
        .and_then(Value::as_str)
        .and_then(usable_secret)
        .map(str::to_owned)
        .ok_or_else(invalid_reference)
}

pub(super) fn extract_explicit_provider_api_key(rule_id: &str, settings: &Value) -> Result<String> {
    let env = settings.get("env");
    let env_keys: &[&str] = match rule_id {
        ANTHROPIC_TO_PI_RULE | GLM_TO_CLAUDE_RULE | GLM_TO_PI_RULE | GLM_TO_CODEX_RULE => {
            &[ANTHROPIC_AUTH_TOKEN_ENV, ANTHROPIC_API_KEY_ENV]
        }
        OPENAI_TO_PI_RULE | OPENAI_TO_GROK_RULE => &[OPENAI_API_KEY_ENV],
        XAI_TO_PI_RULE => &[XAI_API_KEY_ENV],
        DEEPSEEK_TO_CLAUDE_RULE | DEEPSEEK_TO_PI_RULE | DEEPSEEK_TO_CODEX_RULE => &[
            ANTHROPIC_AUTH_TOKEN_ENV,
            ANTHROPIC_API_KEY_ENV,
            DEEPSEEK_API_KEY_ENV,
        ],
        _ => return Err(invalid_reference()),
    };
    let mut candidates = Vec::new();
    for key in env_keys {
        if let Some(value) = env.and_then(|env| env.get(*key)).and_then(Value::as_str) {
            candidates.push(value);
        }
    }
    if let Some(value) = settings.get("apiKey").and_then(Value::as_str) {
        candidates.push(value);
    }
    if let Some(value) = settings.get("api_key").and_then(Value::as_str) {
        candidates.push(value);
    }
    for candidate in candidates {
        if let Some(key) = usable_secret(candidate) {
            return Ok(key.to_owned());
        }
    }
    Err(invalid_reference())
}

pub(super) fn extract_deepseek_api_key(settings: &Value) -> Result<String> {
    let env = settings.get("env");
    for candidate in [
        settings.get("api_key").and_then(Value::as_str),
        settings.get("apiKey").and_then(Value::as_str),
        env.and_then(|value| value.get(DSH_API_KEY_ENV))
            .and_then(Value::as_str),
        settings
            .get("credentials")
            .and_then(|value| value.get("api_key"))
            .and_then(Value::as_str),
        env.and_then(|value| value.get(ANTHROPIC_AUTH_TOKEN_ENV))
            .and_then(Value::as_str),
        env.and_then(|value| value.get(ANTHROPIC_API_KEY_ENV))
            .and_then(Value::as_str),
        env.and_then(|value| value.get(DEEPSEEK_API_KEY_ENV))
            .and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(key) = usable_secret(candidate) {
            return Ok(key.to_owned());
        }
    }
    Err(invalid_reference())
}

pub(super) fn usable_secret(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "***" || trimmed == CONNECTION_SECRET_MARKER {
        None
    } else {
        Some(trimmed)
    }
}

pub(super) fn first_usable_string(value: &Value, pointers: &[&str]) -> Option<String> {
    pointers
        .iter()
        .filter_map(|pointer| value.pointer(pointer).and_then(Value::as_str))
        .find_map(|candidate| usable_secret(candidate).map(str::to_owned))
}

pub(super) fn is_codex_local_token(provider: &Provider) -> bool {
    matches!(
        (
            provider.agent_id,
            provider.meta.get("adapterRuleId").and_then(Value::as_str)
        ),
        (
            AgentId::Codex,
            Some(KIMI_TO_CODEX_BRIDGE_RULE | ANTHROPIC_TO_CODEX_BRIDGE_RULE)
        ) | (
            AgentId::Claude,
            Some(CODEX_TO_CLAUDE_BRIDGE_RULE | GROK_CLAUDE_RULE_ID)
        )
    ) && provider
        .meta
        .get("adapterRuleVersion")
        .and_then(Value::as_u64)
        == Some(1)
        && provider
            .meta
            .get("adapterSecretMode")
            .and_then(Value::as_str)
            == Some(LOCAL_TOKEN_MODE)
}

pub(super) fn valid_local_token_projection(provider: &Provider) -> bool {
    if provider.agent_id == AgentId::Claude {
        let Some(env) = provider
            .settings_config
            .get("env")
            .and_then(Value::as_object)
        else {
            return false;
        };
        return env
            .get(ANTHROPIC_BASE_URL_ENV)
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with("http://127.0.0.1:"))
            && env
                .get(ANTHROPIC_AUTH_TOKEN_ENV)
                .and_then(Value::as_str)
                .is_some_and(|token| usable_secret(token).is_some());
    }

    provider
        .settings_config
        .get("format")
        .and_then(Value::as_str)
        == Some("toml")
        && provider
            .settings_config
            .get("auth")
            .and_then(Value::as_object)
            .and_then(|auth| auth.get("OPENAI_API_KEY"))
            .and_then(Value::as_str)
            .is_some_and(|token| usable_secret(token).is_some())
}

pub(super) fn extract_kimi_api_key(settings: &Value) -> Result<String> {
    let value = if let Some(api_key) = settings.get("apiKey").and_then(Value::as_str) {
        api_key.to_owned()
    } else if settings.get("format").and_then(Value::as_str) == Some("toml") {
        let content = settings
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(invalid_reference)?;
        let document = content
            .parse::<DocumentMut>()
            .map_err(|_| invalid_reference())?;
        // Prefer Kimi's selected provider, then the first configured provider
        // with a non-empty key, and finally the legacy top-level key. These
        // are the only accepted TOML paths; do not recursively search
        // arbitrary user content for something key-shaped.
        document
            .get("default_provider")
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|provider| !provider.is_empty())
            .and_then(|provider| {
                document
                    .get("providers")
                    .and_then(|item| item.as_table())
                    .and_then(|providers| providers.get(provider))
                    .and_then(toml_provider_api_key)
            })
            .or_else(|| {
                document
                    .get("providers")
                    .and_then(|item| item.as_table())
                    .and_then(|providers| {
                        providers
                            .iter()
                            .find_map(|(_, provider)| toml_provider_api_key(provider))
                    })
            })
            .or_else(|| toml_non_empty(document.get("api_key").and_then(|item| item.as_str())))
            .unwrap_or_default()
            .to_owned()
    } else {
        return Err(invalid_reference());
    };

    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "***" || trimmed == CONNECTION_SECRET_MARKER {
        return Err(invalid_reference());
    }
    Ok(trimmed.to_owned())
}

pub(super) fn toml_provider_api_key(provider: &toml_edit::Item) -> Option<&str> {
    toml_non_empty(provider.get("api_key").and_then(|item| item.as_str()))
}

pub(super) fn toml_non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub(super) fn invalid_reference() -> AppError {
    AppError::InvalidArg("invalid adapter secret reference".into())
}
