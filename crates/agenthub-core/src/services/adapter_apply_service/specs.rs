use chrono::Utc;
use serde_json::json;

use crate::error::{AppError, Result};
use crate::models::{
    map_adapter_model, AdapterGateKind, AdapterProfile, AdapterProfileMode, AdapterProfileStatus,
    AdapterRoute, AdapterSourceKind, AdapterSourceProduct, AdapterSupport, AgentId, Provider,
    ProviderInput,
};
use crate::services::adapter_route_constants::{
    claude_native_base_url, ANTHROPIC_AUTH_TOKEN_ENV, ANTHROPIC_BASE_URL_ENV,
    ANTHROPIC_PI_PROVIDER_SLOT, CONNECTION_SECRET_MARKER, DEEPSEEK_API_BASE_URL,
    DEEPSEEK_CLAUDE_DEFAULT_MODEL, DEEPSEEK_CLAUDE_RULE_ID, DEEPSEEK_CODEX_BASE_URL,
    DEEPSEEK_CODEX_DEFAULT_MODEL,
    DEEPSEEK_CODEX_PROVIDER_PREFIX, DEEPSEEK_CODEX_PROVIDER_SLUG, DEEPSEEK_CODEX_RULE_ID,
    DEEPSEEK_PI_PROVIDER_SLOT, DEEPSEEK_PI_RULE_ID, DSH_API_KEY_ENV, DSH_DEEPSEEK_PROVIDER_SLOT,
    DSH_DEFAULT_MODEL, GLM_CLAUDE_DEFAULT_MODEL, GLM_CLAUDE_RULE_ID, GLM_CODEX_BASE_URL,
    GLM_CODEX_DEFAULT_MODEL,
    GLM_CODEX_PROVIDER_PREFIX, GLM_CODEX_PROVIDER_SLUG, GLM_CODEX_RULE_ID, GLM_PI_BASE_URL,
    GLM_PI_PROVIDER_SLOT, GLM_PI_RULE_ID, KIMI_CLAUDE_DEFAULT_MODEL, KIMI_CLAUDE_RULE_ID,
    KIMI_GROK_BASE_URL,
    KIMI_GROK_DEFAULT_MODEL, KIMI_PI_BASE_URL, KIMI_PI_PROVIDER_SLOT, OPENAI_GROK_BASE_URL,
    OPENAI_GROK_DEFAULT_MODEL, OPENAI_PI_PROVIDER_SLOT, XAI_PI_PROVIDER_SLOT,
};

use super::GeneratedApplySpec;

pub(super) const RULE_ID: &str = KIMI_CLAUDE_RULE_ID;
pub(super) const KIMI_PI_RULE_ID: &str = "kimi-membership-to-pi-v1";
pub(super) const ANTHROPIC_PI_RULE_ID: &str = "anthropic-api-to-pi-v1";
pub(super) const OPENAI_PI_RULE_ID: &str = "openai-api-to-pi-v1";
pub(super) const XAI_PI_RULE_ID: &str = "xai-api-to-pi-v1";
pub(super) const CLAUDE_SUBSCRIPTION_PI_RULE_ID: &str = "claude-subscription-to-pi-v1";
pub(super) const CODEX_SUBSCRIPTION_PI_RULE_ID: &str = "codex-subscription-to-pi-v1";
pub(super) const GROK_SUBSCRIPTION_PI_RULE_ID: &str = "grok-subscription-to-pi-v1";
pub(super) const KIMI_GROK_RULE_ID: &str = "kimi-membership-to-grok-v1";
pub(super) const OPENAI_GROK_RULE_ID: &str = "openai-api-to-grok-v1";
pub(super) const DEEPSEEK_DSH_RULE_ID: &str = "deepseek-api-to-dsh-v1";
pub(super) const RULE_VERSION: &str = "1";
pub(super) const CLAUDE_PROVIDER_PREFIX: &str = "claude-kimi-adapter";
pub(super) const CLAUDE_GLM_PROVIDER_PREFIX: &str = "claude-glm-adapter";
pub(super) const CLAUDE_DEEPSEEK_PROVIDER_PREFIX: &str = "claude-deepseek-adapter";
pub(super) const CODEX_GLM_PROFILE_PREFIX: &str = "adapter-glm-codex";
pub(super) const CODEX_DEEPSEEK_PROFILE_PREFIX: &str = "adapter-deepseek-codex";
pub(super) const PI_KIMI_PROVIDER_PREFIX: &str = "pi-kimi-adapter";
pub(super) const PI_ANTHROPIC_PROVIDER_PREFIX: &str = "pi-anthropic-adapter";
pub(super) const PI_OPENAI_PROVIDER_PREFIX: &str = "pi-openai-adapter";
pub(super) const PI_XAI_PROVIDER_PREFIX: &str = "pi-xai-adapter";
pub(super) const PI_GLM_PROVIDER_PREFIX: &str = "pi-glm-adapter";
pub(super) const PI_DEEPSEEK_PROVIDER_PREFIX: &str = "pi-deepseek-adapter";
pub(super) const PI_CLAUDE_OAUTH_PROVIDER_PREFIX: &str = "pi-claude-oauth-adapter";
pub(super) const PI_CODEX_OAUTH_PROVIDER_PREFIX: &str = "pi-codex-oauth-adapter";
pub(super) const PI_GROK_OAUTH_PROVIDER_PREFIX: &str = "pi-grok-oauth-adapter";
pub(super) const DSH_DEEPSEEK_PROVIDER_PREFIX: &str = "dsh-deepseek-adapter";
pub(super) const GROK_KIMI_PROVIDER_PREFIX: &str = "grok-kimi-adapter";
pub(super) const GROK_OPENAI_PROVIDER_PREFIX: &str = "grok-openai-adapter";
pub(super) const CLAUDE_PROFILE_PREFIX: &str = "adapter-kimi-claude";
pub(super) const CLAUDE_GLM_PROFILE_PREFIX: &str = "adapter-glm-claude";
pub(super) const CLAUDE_DEEPSEEK_PROFILE_PREFIX: &str = "adapter-deepseek-claude";
pub(super) const PI_KIMI_PROFILE_PREFIX: &str = "adapter-kimi-pi";
pub(super) const PI_ANTHROPIC_PROFILE_PREFIX: &str = "adapter-anthropic-pi";
pub(super) const PI_OPENAI_PROFILE_PREFIX: &str = "adapter-openai-pi";
pub(super) const PI_XAI_PROFILE_PREFIX: &str = "adapter-xai-pi";
pub(super) const PI_GLM_PROFILE_PREFIX: &str = "adapter-glm-pi";
pub(super) const PI_DEEPSEEK_PROFILE_PREFIX: &str = "adapter-deepseek-pi";
pub(super) const DSH_DEEPSEEK_PROFILE_PREFIX: &str = "adapter-deepseek-dsh";
pub(super) const GROK_KIMI_PROFILE_PREFIX: &str = "adapter-kimi-grok";
pub(super) const GROK_OPENAI_PROFILE_PREFIX: &str = "adapter-openai-grok";
pub(super) const PREVIOUS_CURRENT_ID: &str = "previousCurrentId";
pub(super) const PREVIOUS_BACKUP_ID: &str = "previousBackupId";

/// 幂等判定：已有投影是否已是当前规则的完整契约。
/// 不比较 `name`：展示名随票 display 变化，不是契约。
pub(super) fn same_profile_contract(existing: &AdapterProfile, proposed: &AdapterProfile) -> bool {
    existing.id == proposed.id
        && existing.source_kind == proposed.source_kind
        && existing.source_id == proposed.source_id
        && existing.target_agent_id == proposed.target_agent_id
        && existing.route == proposed.route
        && existing.mode == proposed.mode
        && existing.rule_id == proposed.rule_id
        && existing.rule_version == proposed.rule_version
        && existing.generated_provider_id == proposed.generated_provider_id
}

pub(super) fn owns_apply_profile(profile: &AdapterProfile) -> bool {
    matches!(
        (profile.target_agent_id, profile.route),
        (AgentId::Claude, AdapterRoute::NativeEndpoint)
            | (AgentId::Codex, AdapterRoute::NativeEndpoint)
            | (AgentId::Grok, AdapterRoute::NativeEndpoint)
            | (AgentId::Pi, AdapterRoute::ConfigSync)
            | (AgentId::Dsh, AdapterRoute::ConfigSync)
    )
}

pub(super) fn generated_provider_prefix(profile: &AdapterProfile) -> Option<&'static str> {
    match (
        profile.target_agent_id,
        profile.route,
        profile.rule_id.as_str(),
    ) {
        (AgentId::Claude, AdapterRoute::NativeEndpoint, RULE_ID) => Some(CLAUDE_PROVIDER_PREFIX),
        (AgentId::Claude, AdapterRoute::NativeEndpoint, GLM_CLAUDE_RULE_ID) => {
            Some(CLAUDE_GLM_PROVIDER_PREFIX)
        }
        (AgentId::Claude, AdapterRoute::NativeEndpoint, DEEPSEEK_CLAUDE_RULE_ID) => {
            Some(CLAUDE_DEEPSEEK_PROVIDER_PREFIX)
        }
        (AgentId::Codex, AdapterRoute::NativeEndpoint, GLM_CODEX_RULE_ID) => {
            Some(GLM_CODEX_PROVIDER_PREFIX)
        }
        (AgentId::Codex, AdapterRoute::NativeEndpoint, DEEPSEEK_CODEX_RULE_ID) => {
            Some(DEEPSEEK_CODEX_PROVIDER_PREFIX)
        }
        (AgentId::Grok, AdapterRoute::NativeEndpoint, KIMI_GROK_RULE_ID) => {
            Some(GROK_KIMI_PROVIDER_PREFIX)
        }
        (AgentId::Grok, AdapterRoute::NativeEndpoint, OPENAI_GROK_RULE_ID) => {
            Some(GROK_OPENAI_PROVIDER_PREFIX)
        }
        (AgentId::Pi, AdapterRoute::ConfigSync, KIMI_PI_RULE_ID) => Some(PI_KIMI_PROVIDER_PREFIX),
        (AgentId::Pi, AdapterRoute::ConfigSync, ANTHROPIC_PI_RULE_ID) => {
            Some(PI_ANTHROPIC_PROVIDER_PREFIX)
        }
        (AgentId::Pi, AdapterRoute::ConfigSync, OPENAI_PI_RULE_ID) => {
            Some(PI_OPENAI_PROVIDER_PREFIX)
        }
        (AgentId::Pi, AdapterRoute::ConfigSync, XAI_PI_RULE_ID) => Some(PI_XAI_PROVIDER_PREFIX),
        (AgentId::Pi, AdapterRoute::ConfigSync, GLM_PI_RULE_ID) => Some(PI_GLM_PROVIDER_PREFIX),
        (AgentId::Pi, AdapterRoute::ConfigSync, DEEPSEEK_PI_RULE_ID) => {
            Some(PI_DEEPSEEK_PROVIDER_PREFIX)
        }
        (AgentId::Pi, AdapterRoute::ConfigSync, CLAUDE_SUBSCRIPTION_PI_RULE_ID) => {
            Some(PI_CLAUDE_OAUTH_PROVIDER_PREFIX)
        }
        (AgentId::Pi, AdapterRoute::ConfigSync, CODEX_SUBSCRIPTION_PI_RULE_ID) => {
            Some(PI_CODEX_OAUTH_PROVIDER_PREFIX)
        }
        (AgentId::Pi, AdapterRoute::ConfigSync, GROK_SUBSCRIPTION_PI_RULE_ID) => {
            Some(PI_GROK_OAUTH_PROVIDER_PREFIX)
        }
        (AgentId::Dsh, AdapterRoute::ConfigSync, DEEPSEEK_DSH_RULE_ID) => {
            Some(DSH_DEEPSEEK_PROVIDER_PREFIX)
        }
        _ => None,
    }
}

pub(super) fn provider_owned_by(
    provider: &crate::models::Provider,
    profile: &AdapterProfile,
) -> bool {
    let Some(prefix) = generated_provider_prefix(profile) else {
        return false;
    };
    provider.id == stable_id(prefix, &profile.source_id)
        && provider.agent_id == profile.target_agent_id
        && provider
            .meta
            .get("generatedBy")
            .and_then(serde_json::Value::as_str)
            == Some("adapter")
        && provider
            .meta
            .get("adapterRuleId")
            .and_then(serde_json::Value::as_str)
            == Some(profile.rule_id.as_str())
        && provider
            .meta
            .get("adapterRuleVersion")
            .and_then(serde_json::Value::as_u64)
            == Some(1)
        && provider
            .meta
            .get("adapterSecretMode")
            .and_then(serde_json::Value::as_str)
            == Some("source_reference")
        && provider
            .meta
            .get("adapterProfileId")
            .and_then(serde_json::Value::as_str)
            == Some(profile.id.as_str())
        && provider
            .meta
            .get("adapterSourceRef")
            .and_then(|v| v.get("kind"))
            .and_then(serde_json::Value::as_str)
            == Some(profile.source_kind.as_str())
        && provider
            .meta
            .get("adapterSourceRef")
            .and_then(|v| v.get("id"))
            .and_then(serde_json::Value::as_str)
            == Some(profile.source_id.as_str())
}

/// Same gate as `ensure_supported` — used by matrix consistency tests.
pub(crate) fn apply_request_supported(
    source_kind: AdapterSourceKind,
    target: AgentId,
    route: AdapterRoute,
    rule_id: Option<&str>,
    support: AdapterSupport,
    gate_kind: AdapterGateKind,
) -> bool {
    if gate_kind != AdapterGateKind::None {
        return false;
    }
    match (source_kind, target, route) {
        (source_kind, AgentId::Claude, AdapterRoute::NativeEndpoint) => match (rule_id, support) {
            (Some(RULE_ID), AdapterSupport::Stable) => is_api_source_kind(source_kind),
            (Some(rule), AdapterSupport::Experimental) if is_claude_native_explicit_rule(rule) => {
                true
            }
            _ => false,
        },
        (source_kind, AgentId::Codex, AdapterRoute::NativeEndpoint) => {
            support == AdapterSupport::Experimental
                && rule_id.is_some_and(|rule| {
                    is_codex_native_rule(rule) && is_api_source_kind(source_kind)
                })
        }
        (source_kind, AgentId::Grok, AdapterRoute::NativeEndpoint) => {
            support == AdapterSupport::Experimental
                && rule_id.is_some_and(|rule| {
                    is_grok_native_rule(rule) && is_api_source_kind(source_kind)
                })
        }
        (AdapterSourceKind::Provider, AgentId::Pi, AdapterRoute::ConfigSync) => {
            (support == AdapterSupport::Stable && rule_id == Some(KIMI_PI_RULE_ID))
                || ((support == AdapterSupport::Stable || support == AdapterSupport::Experimental)
                    && rule_id.is_some_and(is_explicit_api_to_pi_rule))
        }
        (AdapterSourceKind::Account, AgentId::Pi, AdapterRoute::ConfigSync) => {
            (support == AdapterSupport::Stable && rule_id == Some(KIMI_PI_RULE_ID))
                || ((support == AdapterSupport::Stable || support == AdapterSupport::Experimental)
                    && rule_id.is_some_and(is_explicit_api_to_pi_rule))
                || (support == AdapterSupport::Experimental
                    && rule_id.is_some_and(is_subscription_pi_rule))
        }
        (AdapterSourceKind::Provider, AgentId::Dsh, AdapterRoute::ConfigSync) => {
            support == AdapterSupport::Stable && rule_id == Some(DEEPSEEK_DSH_RULE_ID)
        }
        _ => false,
    }
}

pub(super) fn is_claude_native_explicit_rule(rule_id: &str) -> bool {
    matches!(rule_id, GLM_CLAUDE_RULE_ID | DEEPSEEK_CLAUDE_RULE_ID)
}

pub(super) fn is_codex_native_rule(rule_id: &str) -> bool {
    matches!(rule_id, GLM_CODEX_RULE_ID | DEEPSEEK_CODEX_RULE_ID)
}

pub(super) fn is_grok_native_rule(rule_id: &str) -> bool {
    matches!(rule_id, KIMI_GROK_RULE_ID | OPENAI_GROK_RULE_ID)
}

pub(super) fn is_api_source_kind(source_kind: AdapterSourceKind) -> bool {
    matches!(
        source_kind,
        AdapterSourceKind::Provider | AdapterSourceKind::Account
    )
}

pub(super) fn claude_native_layout(
    rule_id: &str,
) -> Result<(&'static str, &'static str, &'static str, &'static str)> {
    let base_url = claude_native_base_url(rule_id).ok_or_else(|| {
        AppError::Unsupported(
            "adapter apply currently supports Kimi / GLM / DeepSeek -> Claude".into(),
        )
    })?;
    match rule_id {
        RULE_ID => Ok((
            CLAUDE_PROFILE_PREFIX,
            CLAUDE_PROVIDER_PREFIX,
            "Kimi Code",
            base_url,
        )),
        GLM_CLAUDE_RULE_ID => Ok((
            CLAUDE_GLM_PROFILE_PREFIX,
            CLAUDE_GLM_PROVIDER_PREFIX,
            "GLM",
            base_url,
        )),
        DEEPSEEK_CLAUDE_RULE_ID => Ok((
            CLAUDE_DEEPSEEK_PROFILE_PREFIX,
            CLAUDE_DEEPSEEK_PROVIDER_PREFIX,
            "DeepSeek",
            base_url,
        )),
        _ => Err(AppError::Unsupported(
            "adapter apply currently supports Kimi / GLM / DeepSeek -> Claude".into(),
        )),
    }
}

fn claude_native_default_model(rule_id: &str) -> Option<&'static str> {
    match rule_id {
        RULE_ID => map_adapter_model(
            AdapterSourceProduct::KimiCodeMembership,
            AgentId::Claude,
            "",
        )
        .or(Some(KIMI_CLAUDE_DEFAULT_MODEL)),
        GLM_CLAUDE_RULE_ID => Some(GLM_CLAUDE_DEFAULT_MODEL),
        DEEPSEEK_CLAUDE_RULE_ID => Some(DEEPSEEK_CLAUDE_DEFAULT_MODEL),
        _ => None,
    }
}

pub(super) fn claude_native_settings_config(rule_id: &str, base_url: &str) -> serde_json::Value {
    let mut env = serde_json::Map::new();
    env.insert(ANTHROPIC_BASE_URL_ENV.into(), json!(base_url));
    env.insert(
        ANTHROPIC_AUTH_TOKEN_ENV.into(),
        json!(CONNECTION_SECRET_MARKER),
    );
    if let Some(model) = claude_native_default_model(rule_id) {
        crate::models::apply_claude_live_model_env(&mut env, model, None);
    }
    let mut settings = json!({ "env": env });
    if let Some(model) = claude_native_default_model(rule_id) {
        settings
            .as_object_mut()
            .expect("object")
            .insert("model".into(), json!(model));
    }
    settings
}

pub(super) fn claude_native_spec(
    source_kind: AdapterSourceKind,
    source_id: &str,
    rule_id: &str,
) -> Result<GeneratedApplySpec> {
    let (profile_prefix, provider_prefix, display, base_url) = claude_native_layout(rule_id)?;
    let profile_id = stable_id(profile_prefix, source_id);
    let provider_id = stable_id(provider_prefix, source_id);
    let created_at = now();
    Ok(GeneratedApplySpec {
        target_agent: AgentId::Claude,
        provider_id: provider_id.clone(),
        proposed: AdapterProfile {
            id: profile_id.clone(),
            name: format!("{display} → Claude ({})", safe_label(source_id)),
            source_kind,
            source_id: source_id.into(),
            target_agent_id: AgentId::Claude,
            route: AdapterRoute::NativeEndpoint,
            mode: AdapterProfileMode::Api,
            status: AdapterProfileStatus::Applying,
            rule_id: rule_id.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        },
        provider: ProviderInput {
            id: provider_id,
            agent_id: AgentId::Claude,
            name: format!("{display} ({})", safe_label(source_id)),
            settings_config: claude_native_settings_config(rule_id, base_url),
            meta: generated_meta(
                rule_id,
                &profile_id,
                source_kind,
                source_id,
                Some("anthropic-compatible"),
            ),
            is_current: false,
        },
    })
}

pub(super) fn codex_native_spec(
    source_kind: AdapterSourceKind,
    source_id: &str,
    rule_id: &str,
) -> Result<GeneratedApplySpec> {
    let (profile_prefix, provider_prefix, display, base_url, model, slug) = match rule_id {
        GLM_CODEX_RULE_ID => (
            CODEX_GLM_PROFILE_PREFIX,
            GLM_CODEX_PROVIDER_PREFIX,
            "GLM Coding Plan",
            GLM_CODEX_BASE_URL,
            GLM_CODEX_DEFAULT_MODEL,
            GLM_CODEX_PROVIDER_SLUG,
        ),
        DEEPSEEK_CODEX_RULE_ID => (
            CODEX_DEEPSEEK_PROFILE_PREFIX,
            DEEPSEEK_CODEX_PROVIDER_PREFIX,
            "DeepSeek",
            DEEPSEEK_CODEX_BASE_URL,
            DEEPSEEK_CODEX_DEFAULT_MODEL,
            DEEPSEEK_CODEX_PROVIDER_SLUG,
        ),
        _ => {
            return Err(AppError::Unsupported(
                "adapter apply currently supports GLM / DeepSeek API -> Codex".into(),
            ))
        }
    };
    let profile_id = stable_id(profile_prefix, source_id);
    let provider_id = stable_id(provider_prefix, source_id);
    let created_at = now();
    let content = format!(
        "model_provider = \"{slug}\"\n\
         model = \"{model}\"\n\
         model_reasoning_effort = \"high\"\n\
         preferred_auth_method = \"apikey\"\n\
         \n\
         [model_providers.{slug}]\n\
         name = \"{display}\"\n\
         base_url = \"{base_url}\"\n\
         wire_api = \"responses\"\n\
         experimental_bearer_token = \"{marker}\"\n",
        marker = CONNECTION_SECRET_MARKER,
    );
    Ok(GeneratedApplySpec {
        target_agent: AgentId::Codex,
        provider_id: provider_id.clone(),
        proposed: AdapterProfile {
            id: profile_id.clone(),
            name: format!("{display} → Codex ({})", safe_label(source_id)),
            source_kind,
            source_id: source_id.into(),
            target_agent_id: AgentId::Codex,
            route: AdapterRoute::NativeEndpoint,
            mode: AdapterProfileMode::Api,
            status: AdapterProfileStatus::Applying,
            rule_id: rule_id.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        },
        provider: ProviderInput {
            id: provider_id,
            agent_id: AgentId::Codex,
            name: format!("{display} ({})", safe_label(source_id)),
            settings_config: json!({
                "format": "toml",
                "content": content,
                "auth": { "OPENAI_API_KEY": CONNECTION_SECRET_MARKER },
            }),
            meta: generated_meta(rule_id, &profile_id, source_kind, source_id, None),
            is_current: false,
        },
    })
}

pub(super) fn grok_native_spec(
    source_kind: AdapterSourceKind,
    source_id: &str,
    rule_id: &str,
) -> Result<GeneratedApplySpec> {
    let (profile_prefix, provider_prefix, display, base_url, model) = match rule_id {
        KIMI_GROK_RULE_ID => (
            GROK_KIMI_PROFILE_PREFIX,
            GROK_KIMI_PROVIDER_PREFIX,
            "Kimi Code",
            KIMI_GROK_BASE_URL,
            KIMI_GROK_DEFAULT_MODEL,
        ),
        OPENAI_GROK_RULE_ID => (
            GROK_OPENAI_PROFILE_PREFIX,
            GROK_OPENAI_PROVIDER_PREFIX,
            "OpenAI",
            OPENAI_GROK_BASE_URL,
            OPENAI_GROK_DEFAULT_MODEL,
        ),
        _ => {
            return Err(AppError::Unsupported(
                "adapter apply currently supports Kimi / OpenAI API -> Grok".into(),
            ))
        }
    };
    let profile_id = stable_id(profile_prefix, source_id);
    let provider_id = stable_id(provider_prefix, source_id);
    let created_at = now();
    let alias = if rule_id == KIMI_GROK_RULE_ID {
        "agenthub_kimi"
    } else {
        "agenthub_openai"
    };
    let content = format!(
        "[models]\ndefault = \"{alias}\"\n\n[model.\"{alias}\"]\nmodel = \"{model}\"\nbase_url = \"{base_url}\"\napi_key = \"{marker}\"\napi_backend = \"chat_completions\"\n",
        marker = CONNECTION_SECRET_MARKER,
    );
    Ok(GeneratedApplySpec {
        target_agent: AgentId::Grok,
        provider_id: provider_id.clone(),
        proposed: AdapterProfile {
            id: profile_id.clone(),
            name: format!("{display} → Grok ({})", safe_label(source_id)),
            source_kind,
            source_id: source_id.into(),
            target_agent_id: AgentId::Grok,
            route: AdapterRoute::NativeEndpoint,
            mode: AdapterProfileMode::Api,
            status: AdapterProfileStatus::Applying,
            rule_id: rule_id.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        },
        provider: ProviderInput {
            id: provider_id,
            agent_id: AgentId::Grok,
            name: format!("{display} ({})", safe_label(source_id)),
            settings_config: json!({"format": "toml", "content": content}),
            meta: generated_meta(
                rule_id,
                &profile_id,
                source_kind,
                source_id,
                Some("openai-chat"),
            ),
            is_current: false,
        },
    })
}

pub(super) fn pi_kimi_spec(source_kind: AdapterSourceKind, source_id: &str) -> GeneratedApplySpec {
    let profile_id = stable_id(PI_KIMI_PROFILE_PREFIX, source_id);
    let provider_id = stable_id(PI_KIMI_PROVIDER_PREFIX, source_id);
    let created_at = now();
    let model = map_adapter_model(AdapterSourceProduct::KimiCodeMembership, AgentId::Pi, "")
        .unwrap_or("kimi-k2.5");
    GeneratedApplySpec {
        target_agent: AgentId::Pi,
        provider_id: provider_id.clone(),
        proposed: AdapterProfile {
            id: profile_id.clone(),
            name: format!("Kimi → Pi ({})", safe_label(source_id)),
            source_kind,
            source_id: source_id.into(),
            target_agent_id: AgentId::Pi,
            route: AdapterRoute::ConfigSync,
            mode: AdapterProfileMode::Api,
            status: AdapterProfileStatus::Applying,
            rule_id: KIMI_PI_RULE_ID.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        },
        provider: ProviderInput {
            id: provider_id,
            agent_id: AgentId::Pi,
            name: format!("Kimi Code ({})", safe_label(source_id)),
            settings_config: json!({
                "models": {
                    "providers": {
                        KIMI_PI_PROVIDER_SLOT: {
                            "baseUrl": KIMI_PI_BASE_URL,
                            "apiKey": CONNECTION_SECRET_MARKER,
                            "api": "openai-completions",
                            "models": [{ "id": model }],
                        }
                    }
                },
                "settings": { "defaultProvider": KIMI_PI_PROVIDER_SLOT },
            }),
            meta: generated_meta(KIMI_PI_RULE_ID, &profile_id, source_kind, source_id, None),
            is_current: false,
        },
    }
}

pub(super) fn is_explicit_api_to_pi_rule(rule_id: &str) -> bool {
    matches!(
        rule_id,
        ANTHROPIC_PI_RULE_ID
            | OPENAI_PI_RULE_ID
            | XAI_PI_RULE_ID
            | GLM_PI_RULE_ID
            | DEEPSEEK_PI_RULE_ID
    )
}

pub(super) fn is_subscription_pi_rule(rule_id: &str) -> bool {
    matches!(
        rule_id,
        CLAUDE_SUBSCRIPTION_PI_RULE_ID
            | CODEX_SUBSCRIPTION_PI_RULE_ID
            | GROK_SUBSCRIPTION_PI_RULE_ID
    )
}

pub(super) fn pi_subscription_layout(
    rule_id: &str,
) -> Result<(&'static str, &'static str, &'static str)> {
    match rule_id {
        CLAUDE_SUBSCRIPTION_PI_RULE_ID => Ok((
            PI_CLAUDE_OAUTH_PROVIDER_PREFIX,
            "Claude",
            ANTHROPIC_PI_PROVIDER_SLOT,
        )),
        CODEX_SUBSCRIPTION_PI_RULE_ID => Ok((
            PI_CODEX_OAUTH_PROVIDER_PREFIX,
            "Codex / ChatGPT",
            "openai-codex",
        )),
        GROK_SUBSCRIPTION_PI_RULE_ID => Ok((
            PI_GROK_OAUTH_PROVIDER_PREFIX,
            "Grok / xAI",
            XAI_PI_PROVIDER_SLOT,
        )),
        _ => Err(AppError::Unsupported(
            "adapter apply currently supports Claude / Codex / Grok subscription -> Pi".into(),
        )),
    }
}

pub(super) fn pi_subscription_spec(source_id: &str, rule_id: &str) -> Result<GeneratedApplySpec> {
    let (provider_prefix, display, slot) = pi_subscription_layout(rule_id)?;
    let profile_id = stable_id(&format!("adapter-{provider_prefix}"), source_id);
    let provider_id = stable_id(provider_prefix, source_id);
    let created_at = now();
    Ok(GeneratedApplySpec {
        target_agent: AgentId::Pi,
        provider_id: provider_id.clone(),
        proposed: AdapterProfile {
            id: profile_id.clone(),
            name: format!("{display} 订阅 → Pi ({})", safe_label(source_id)),
            source_kind: AdapterSourceKind::Account,
            source_id: source_id.into(),
            target_agent_id: AgentId::Pi,
            route: AdapterRoute::ConfigSync,
            mode: AdapterProfileMode::Oauth,
            status: AdapterProfileStatus::Applying,
            rule_id: rule_id.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        },
        provider: ProviderInput {
            id: provider_id,
            agent_id: AgentId::Pi,
            name: format!("{display} 订阅 ({})", safe_label(source_id)),
            settings_config: json!({
                "auth": {
                    (slot): {
                        "type": "oauth",
                        "access": CONNECTION_SECRET_MARKER,
                        "refresh": CONNECTION_SECRET_MARKER,
                    }
                },
                "settings": { "defaultProvider": slot },
            }),
            meta: generated_meta(
                rule_id,
                &profile_id,
                AdapterSourceKind::Account,
                source_id,
                None,
            ),
            is_current: false,
        },
    })
}

pub(super) fn pi_explicit_api_layout(
    rule_id: &str,
) -> Result<(&'static str, &'static str, &'static str, &'static str)> {
    match rule_id {
        ANTHROPIC_PI_RULE_ID => Ok((
            PI_ANTHROPIC_PROFILE_PREFIX,
            PI_ANTHROPIC_PROVIDER_PREFIX,
            "Anthropic",
            ANTHROPIC_PI_PROVIDER_SLOT,
        )),
        OPENAI_PI_RULE_ID => Ok((
            PI_OPENAI_PROFILE_PREFIX,
            PI_OPENAI_PROVIDER_PREFIX,
            "OpenAI",
            OPENAI_PI_PROVIDER_SLOT,
        )),
        XAI_PI_RULE_ID => Ok((
            PI_XAI_PROFILE_PREFIX,
            PI_XAI_PROVIDER_PREFIX,
            "xAI",
            XAI_PI_PROVIDER_SLOT,
        )),
        GLM_PI_RULE_ID => Ok((
            PI_GLM_PROFILE_PREFIX,
            PI_GLM_PROVIDER_PREFIX,
            "GLM Coding Plan",
            GLM_PI_PROVIDER_SLOT,
        )),
        DEEPSEEK_PI_RULE_ID => Ok((
            PI_DEEPSEEK_PROFILE_PREFIX,
            PI_DEEPSEEK_PROVIDER_PREFIX,
            "DeepSeek",
            DEEPSEEK_PI_PROVIDER_SLOT,
        )),
        _ => Err(AppError::Unsupported(
            "adapter apply currently supports Anthropic / OpenAI / xAI / GLM / DeepSeek API -> Pi"
                .into(),
        )),
    }
}

pub(super) fn pi_explicit_api_spec(
    source_kind: AdapterSourceKind,
    source_id: &str,
    rule_id: &str,
) -> Result<GeneratedApplySpec> {
    let (profile_prefix, provider_prefix, display, slot) = pi_explicit_api_layout(rule_id)?;
    let (base_url, model) = match rule_id {
        GLM_PI_RULE_ID => (GLM_PI_BASE_URL, "glm-4.6"),
        DEEPSEEK_PI_RULE_ID => (DEEPSEEK_API_BASE_URL, "deepseek-chat"),
        _ => ("", ""),
    };
    let mut pi_provider = json!({"apiKey": CONNECTION_SECRET_MARKER});
    if !base_url.is_empty() {
        pi_provider["baseUrl"] = json!(base_url);
        pi_provider["api"] = json!("openai-completions");
        pi_provider["models"] = json!([{ "id": model }]);
    }
    let profile_id = stable_id(profile_prefix, source_id);
    let provider_id = stable_id(provider_prefix, source_id);
    let created_at = now();
    Ok(GeneratedApplySpec {
        target_agent: AgentId::Pi,
        provider_id: provider_id.clone(),
        proposed: AdapterProfile {
            id: profile_id.clone(),
            name: format!("{display} → Pi ({})", safe_label(source_id)),
            source_kind,
            source_id: source_id.into(),
            target_agent_id: AgentId::Pi,
            route: AdapterRoute::ConfigSync,
            mode: AdapterProfileMode::Api,
            status: AdapterProfileStatus::Applying,
            rule_id: rule_id.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        },
        provider: ProviderInput {
            id: provider_id,
            agent_id: AgentId::Pi,
            name: format!("{display} ({})", safe_label(source_id)),
            settings_config: json!({
                "models": {
                    "providers": {
                        (slot): pi_provider
                    }
                },
                "settings": { "defaultProvider": slot },
            }),
            meta: generated_meta(rule_id, &profile_id, source_kind, source_id, None),
            is_current: false,
        },
    })
}

pub(super) fn dsh_deepseek_spec(source_id: &str) -> GeneratedApplySpec {
    let profile_id = stable_id(DSH_DEEPSEEK_PROFILE_PREFIX, source_id);
    let provider_id = stable_id(DSH_DEEPSEEK_PROVIDER_PREFIX, source_id);
    let created_at = now();
    let model = map_adapter_model(AdapterSourceProduct::DeepseekApi, AgentId::Dsh, "")
        .unwrap_or(DSH_DEFAULT_MODEL);
    GeneratedApplySpec {
        target_agent: AgentId::Dsh,
        provider_id: provider_id.clone(),
        proposed: AdapterProfile {
            id: profile_id.clone(),
            name: format!("DeepSeek → DSH ({})", safe_label(source_id)),
            source_kind: AdapterSourceKind::Provider,
            source_id: source_id.into(),
            target_agent_id: AgentId::Dsh,
            route: AdapterRoute::ConfigSync,
            mode: AdapterProfileMode::Api,
            status: AdapterProfileStatus::Applying,
            rule_id: DEEPSEEK_DSH_RULE_ID.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        },
        provider: ProviderInput {
            id: provider_id,
            agent_id: AgentId::Dsh,
            name: format!("DeepSeek API ({})", safe_label(source_id)),
            settings_config: json!({
                "provider": DSH_DEEPSEEK_PROVIDER_SLOT,
                "model": model,
                "apiKeyEnv": DSH_API_KEY_ENV,
                "baseURL": DEEPSEEK_API_BASE_URL,
                "api_key": CONNECTION_SECRET_MARKER,
            }),
            meta: generated_meta(
                DEEPSEEK_DSH_RULE_ID,
                &profile_id,
                AdapterSourceKind::Provider,
                source_id,
                Some("deepseek"),
            ),
            is_current: false,
        },
    }
}

pub(super) fn generated_meta(
    rule_id: &str,
    profile_id: &str,
    source_kind: AdapterSourceKind,
    source_id: &str,
    preset: Option<&str>,
) -> serde_json::Value {
    let mut meta = serde_json::Map::new();
    if let Some(preset) = preset {
        meta.insert("preset".into(), json!(preset));
    }
    meta.insert("generatedBy".into(), json!("adapter"));
    meta.insert("adapterRuleId".into(), json!(rule_id));
    meta.insert("adapterRuleVersion".into(), json!(1));
    meta.insert("adapterSecretMode".into(), json!("source_reference"));
    meta.insert("adapterProfileId".into(), json!(profile_id));
    meta.insert(
        "adapterSourceRef".into(),
        json!({"kind": source_kind.as_str(), "id": source_id}),
    );
    serde_json::Value::Object(meta)
}

/// 投影契约：id / agent / settings / 合同 meta。
/// 不比较 `name`：展示名随票 display 变化，重 bind 不得因此重写 live。
pub(super) fn provider_matches_projection(
    provider: &crate::models::Provider,
    projection: &ProviderInput,
) -> bool {
    provider.id == projection.id
        && provider.agent_id == projection.agent_id
        && provider.settings_config == projection.settings_config
        && projection_contract_meta(&provider.meta) == projection_contract_meta(&projection.meta)
}

pub(super) fn projection_contract_meta(meta: &serde_json::Value) -> serde_json::Value {
    let mut cloned = meta.clone();
    if let Some(object) = cloned.as_object_mut() {
        object.remove(PREVIOUS_CURRENT_ID);
        object.remove(PREVIOUS_BACKUP_ID);
    }
    cloned
}

pub(super) fn stamp_previous_restore_meta(
    meta: &mut serde_json::Value,
    previous_current: Option<&Provider>,
    generated_id: &str,
    existing: Option<&Provider>,
) {
    let Some(object) = meta.as_object_mut() else {
        return;
    };
    let previous_id = previous_current
        .map(|provider| provider.id.as_str())
        .filter(|id| *id != generated_id);
    match previous_id {
        Some(id) => {
            object.insert(PREVIOUS_CURRENT_ID.into(), json!(id));
        }
        None => {
            if let Some(existing_id) = existing
                .and_then(|provider| provider.meta.get(PREVIOUS_CURRENT_ID))
                .cloned()
            {
                object.insert(PREVIOUS_CURRENT_ID.into(), existing_id);
            }
        }
    }
    if let Some(existing_backup) = existing
        .and_then(|provider| provider.meta.get(PREVIOUS_BACKUP_ID))
        .cloned()
    {
        object.insert(PREVIOUS_BACKUP_ID.into(), existing_backup);
    }
}

pub(super) fn provider_input(provider: &Provider) -> ProviderInput {
    ProviderInput {
        id: provider.id.clone(),
        agent_id: provider.agent_id,
        name: provider.name.clone(),
        settings_config: provider.settings_config.clone(),
        meta: provider.meta.clone(),
        is_current: provider.is_current,
    }
}

pub(super) fn now() -> String {
    Utc::now().to_rfc3339()
}

pub(super) fn stable_id(prefix: &str, source_id: &str) -> String {
    format!(
        "{prefix}-{}-{:016x}",
        safe_label(source_id),
        fnv1a(source_id.as_bytes())
    )
}

pub(super) fn safe_label(value: &str) -> String {
    let label: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let label = label.trim_matches('-');
    if label.is_empty() {
        "source".into()
    } else {
        label.chars().take(40).collect()
    }
}

pub(super) fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
