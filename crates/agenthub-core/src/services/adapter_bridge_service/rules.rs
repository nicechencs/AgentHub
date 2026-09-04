use super::*;
use crate::bridge::BridgeLocalSurface;

#[derive(Clone, Copy, PartialEq, Eq)]
enum BridgeProjection {
    ClaudeEnv,
    GrokToml,
    KimiToml,
    DshJson,
    CodexToml,
}

fn projection_of(agent: AgentId) -> BridgeProjection {
    match agent {
        AgentId::Claude => BridgeProjection::ClaudeEnv,
        AgentId::Grok => BridgeProjection::GrokToml,
        AgentId::Kimi => BridgeProjection::KimiToml,
        AgentId::Dsh => BridgeProjection::DshJson,
        _ => BridgeProjection::CodexToml,
    }
}

pub(super) fn endpoint_target(agent: AgentId) -> &'static str {
    match agent {
        AgentId::Claude => "claude",
        AgentId::Codex => "codex",
        AgentId::Grok => "grok",
        _ => "",
    }
}

fn preset_of(agent: AgentId) -> &'static str {
    match projection_of(agent) {
        BridgeProjection::ClaudeEnv => "anthropic",
        BridgeProjection::GrokToml | BridgeProjection::KimiToml => "openai-chat",
        BridgeProjection::DshJson => "deepseek",
        BridgeProjection::CodexToml => "openai-compatible",
    }
}

pub(super) fn projected_provider_input(
    profile: &AdapterProfile,
    local_bearer: &str,
    port: u16,
    model: &str,
    context_window_tokens: Option<u32>,
) -> Result<ProviderInput> {
    validate_bound_port(port)?;
    let rule = rule_for_id(&profile.rule_id).ok_or_else(|| {
        AppError::InvalidArg("这条本机路由已失效，无法启动。请删除后重建。".into())
    })?;
    let provider_id = profile.generated_provider_id.as_deref().ok_or_else(|| {
        AppError::message(
            "adapter.provider_conflict",
            "bridge profile has no generated provider id",
        )
    })?;
    let local_bearer = local_bearer.trim();
    if local_bearer.is_empty() {
        return Err(AppError::message(
            "adapter.local_bearer",
            "bridge local bearer is unavailable",
        ));
    }
    if projection_of(rule.target_agent) == BridgeProjection::ClaudeEnv {
        let mut env = serde_json::Map::new();
        env.insert(
            "ANTHROPIC_BASE_URL".into(),
            json!(format!("http://127.0.0.1:{port}")),
        );
        env.insert("ANTHROPIC_AUTH_TOKEN".into(), json!(local_bearer));
        crate::models::apply_claude_live_model_env(&mut env, model, context_window_tokens);
        let mut settings = json!({ "env": env });
        let id = crate::models::strip_claude_context_marker(model);
        if !id.is_empty() {
            settings
                .as_object_mut()
                .expect("object")
                .insert("model".into(), json!(id));
        }
        return Ok(ProviderInput {
            id: provider_id.into(),
            agent_id: AgentId::Claude,
            name: format!(
                "{} ({})",
                rule.provider_name,
                safe_label(&profile.source_id)
            ),
            settings_config: settings,
            meta: generated_provider_meta(profile, &rule),
            is_current: false,
        });
    }
    if projection_of(rule.target_agent) == BridgeProjection::GrokToml {
        return Ok(ProviderInput {
            id: provider_id.into(),
            agent_id: AgentId::Grok,
            name: format!(
                "{} ({})",
                rule.provider_name,
                safe_label(&profile.source_id)
            ),
            settings_config: json!({
                "format": "toml",
                "content": grok_bridge_toml(&rule, port, local_bearer),
                "auth": { "OPENAI_API_KEY": local_bearer },
            }),
            meta: generated_provider_meta(profile, &rule),
            is_current: false,
        });
    }
    if projection_of(rule.target_agent) == BridgeProjection::KimiToml {
        return Ok(ProviderInput {
            id: provider_id.into(),
            agent_id: AgentId::Kimi,
            name: format!(
                "{} ({})",
                rule.provider_name,
                safe_label(&profile.source_id)
            ),
            settings_config: json!({
                "format": "toml",
                "content": kimi_bridge_toml(&rule, port, local_bearer),
                "auth": { "OPENAI_API_KEY": local_bearer },
            }),
            meta: generated_provider_meta(profile, &rule),
            is_current: false,
        });
    }
    if projection_of(rule.target_agent) == BridgeProjection::DshJson {
        return Ok(ProviderInput {
            id: provider_id.into(),
            agent_id: AgentId::Dsh,
            name: format!(
                "{} ({})",
                rule.provider_name,
                safe_label(&profile.source_id)
            ),
            settings_config: json!({
                "baseURL": format!("http://127.0.0.1:{port}"),
                "apiKeyEnv": crate::services::adapter_route_constants::DSH_API_KEY_ENV,
                "api_key": local_bearer,
            }),
            meta: generated_provider_meta(profile, &rule),
            is_current: false,
        });
    }
    Ok(ProviderInput {
        id: provider_id.into(),
        agent_id: rule.target_agent,
        name: format!(
            "{} ({})",
            rule.provider_name,
            safe_label(&profile.source_id)
        ),
        settings_config: json!({
            "format": "toml",
            "content": codex_bridge_toml(&rule, port),
            "auth": { "OPENAI_API_KEY": local_bearer },
        }),
        meta: generated_provider_meta(profile, &rule),
        is_current: false,
    })
}

pub(super) fn generated_provider_meta(profile: &AdapterProfile, rule: &CodexBridgeRule) -> Value {
    json!({
        "preset": preset_of(rule.target_agent),
        "generatedBy": GENERATED_BY,
        "adapterRuleId": rule.rule_id,
        "adapterRuleVersion": 1,
        "adapterSecretMode": "local_token",
        "adapterProfileId": profile.id,
        "adapterSourceRef": {"kind": profile.source_kind.as_str(), "id": profile.source_id},
        "adapterBridge": {
            "kind": rule.bridge_kind,
            "loopbackOnly": true,
        },
    })
}

pub(super) fn codex_bridge_toml(rule: &CodexBridgeRule, port: u16) -> String {
    format!(
        "model_provider = \"{slug}\"\nmodel = \"{model}\"\nmodel_reasoning_effort = \"high\"\ndisable_response_storage = true\npreferred_auth_method = \"apikey\"\n\n[model_providers.{slug}]\nname = \"{name}\"\nbase_url = \"http://127.0.0.1:{port}/v1\"\nwire_api = \"responses\"\n",
        slug = rule.provider_slug,
        model = rule.default_model,
        name = rule.toml_name,
    )
}

/// Grok config.toml for Codex official login. Local surface is Responses.
/// No ChatGPT model name, no leftover `grok-*`.
pub(super) fn grok_bridge_toml(rule: &CodexBridgeRule, port: u16, local_bearer: &str) -> String {
    format!(
        "[models]\ndefault = \"{slug}\"\n\n[model.\"{slug}\"]\nbase_url = \"http://127.0.0.1:{port}/v1\"\napi_key = \"{token}\"\napi_backend = \"responses\"\n",
        slug = rule.provider_slug,
        token = local_bearer,
    )
}

/// Pre-159e8cd Grok TOML: same as [`grok_bridge_toml`] except `chat_completions`.
pub(super) fn legacy_grok_bridge_toml(
    rule: &CodexBridgeRule,
    port: u16,
    local_bearer: &str,
) -> String {
    format!(
        "[models]\ndefault = \"{slug}\"\n\n[model.\"{slug}\"]\nbase_url = \"http://127.0.0.1:{port}/v1\"\napi_key = \"{token}\"\napi_backend = \"chat_completions\"\n",
        slug = rule.provider_slug,
        token = local_bearer,
    )
}

/// Kimi config.toml for a local-bridge write. `type` follows the local surface.
pub(super) fn kimi_bridge_toml(rule: &CodexBridgeRule, port: u16, local_bearer: &str) -> String {
    let (ty, base) = match rule.local_surface {
        BridgeLocalSurface::Messages => ("anthropic", format!("http://127.0.0.1:{port}")),
        BridgeLocalSurface::Responses => {
            ("openai_responses", format!("http://127.0.0.1:{port}/v1"))
        }
        BridgeLocalSurface::ChatCompletions => ("openai", format!("http://127.0.0.1:{port}/v1")),
    };
    format!(
        "default_provider = \"{slug}\"\n\n[providers.{slug}]\nname = \"{name}\"\ntype = \"{ty}\"\nbase_url = \"{base}\"\napi_key = \"{token}\"\n",
        slug = rule.provider_slug,
        name = rule.toml_name,
        token = local_bearer,
    )
}

/// Pre-type-field Kimi TOML: same as [`kimi_bridge_toml`] without `type = …`.
/// Older projections omitted the type line; restore must still accept them so
/// `needs_reprojection` can rewrite to the current template.
pub(super) fn legacy_kimi_bridge_toml(
    rule: &CodexBridgeRule,
    port: u16,
    local_bearer: &str,
) -> String {
    let base = match rule.local_surface {
        BridgeLocalSurface::Messages => format!("http://127.0.0.1:{port}"),
        BridgeLocalSurface::Responses | BridgeLocalSurface::ChatCompletions => {
            format!("http://127.0.0.1:{port}/v1")
        }
    };
    format!(
        "default_provider = \"{slug}\"\n\n[providers.{slug}]\nname = \"{name}\"\nbase_url = \"{base}\"\napi_key = \"{token}\"\n",
        slug = rule.provider_slug,
        name = rule.toml_name,
        token = local_bearer,
    )
}

pub(super) fn validate_generated_provider(
    provider: &Provider,
    profile: &AdapterProfile,
    expected_port: Option<u16>,
) -> Result<()> {
    if !provider_owned_by(provider, profile) {
        return Err(AppError::message(
            "adapter.provider_conflict",
            "generated provider does not belong to adapter bridge profile",
        ));
    }
    let rule = rule_for_id(&profile.rule_id).ok_or_else(invalid_projection)?;
    let local_bearer = local_bearer_from_provider(provider)?;
    if projection_of(rule.target_agent) == BridgeProjection::ClaudeEnv {
        let env = provider
            .settings_config
            .get("env")
            .and_then(Value::as_object)
            .ok_or_else(invalid_projection)?;
        let base_url = env
            .get("ANTHROPIC_BASE_URL")
            .and_then(Value::as_str)
            .ok_or_else(invalid_projection)?;
        if !base_url.starts_with("http://127.0.0.1:")
            || env
                .get("ANTHROPIC_AUTH_TOKEN")
                .and_then(Value::as_str)
                .is_none_or(|token| token.trim().is_empty())
        {
            return Err(invalid_projection());
        }
        if let Some(port) = expected_port {
            if base_url != format!("http://127.0.0.1:{port}") {
                return Err(AppError::message(
                    "adapter.provider_conflict",
                    "generated bridge provider does not match the bound port",
                ));
            }
        }
        return Ok(());
    }

    if projection_of(rule.target_agent) == BridgeProjection::DshJson {
        let base_url = provider
            .settings_config
            .get("baseURL")
            .and_then(Value::as_str)
            .ok_or_else(invalid_projection)?;
        if !base_url.starts_with("http://127.0.0.1:") {
            return Err(invalid_projection());
        }
        if let Some(port) = expected_port {
            if base_url != format!("http://127.0.0.1:{port}") {
                return Err(AppError::message(
                    "adapter.provider_conflict",
                    "generated bridge provider does not match the bound port",
                ));
            }
        }
        return Ok(());
    }

    if matches!(
        projection_of(rule.target_agent),
        BridgeProjection::GrokToml | BridgeProjection::KimiToml
    ) {
        let content = provider
            .settings_config
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(invalid_projection)?;
        if !content.contains("127.0.0.1") || !content.contains("base_url") {
            return Err(invalid_projection());
        }
        if let Some(port) = expected_port {
            let matches_current = if projection_of(rule.target_agent) == BridgeProjection::GrokToml
            {
                content == grok_bridge_toml(&rule, port, &local_bearer)
                    || content == legacy_grok_bridge_toml(&rule, port, &local_bearer)
            } else {
                content == kimi_bridge_toml(&rule, port, &local_bearer)
                    || content == legacy_kimi_bridge_toml(&rule, port, &local_bearer)
            };
            if !matches_current {
                return Err(AppError::message(
                    "adapter.provider_conflict",
                    "generated bridge provider does not match the bound port",
                ));
            }
        }
        return Ok(());
    }

    let content = provider
        .settings_config
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(invalid_projection)?;
    let document = content
        .parse::<DocumentMut>()
        .map_err(|_| invalid_projection())?;
    let codex_provider = document
        .get("model_providers")
        .and_then(|item| item.as_table())
        .and_then(|providers| providers.get(rule.provider_slug))
        .and_then(|item| item.as_table())
        .ok_or_else(invalid_projection)?;
    if document
        .get("model_provider")
        .and_then(|item| item.as_str())
        != Some(rule.provider_slug)
        || codex_provider
            .get("wire_api")
            .and_then(|item| item.as_str())
            != Some("responses")
        || codex_provider
            .get("base_url")
            .and_then(|item| item.as_str())
            .is_none()
    {
        return Err(invalid_projection());
    }
    if let Some(port) = expected_port {
        if content != codex_bridge_toml(&rule, port) {
            return Err(AppError::message(
                "adapter.provider_conflict",
                "generated bridge provider does not match the bound port",
            ));
        }
    }
    Ok(())
}

pub(super) fn local_bearer_from_provider(provider: &Provider) -> Result<String> {
    if projection_of(provider.agent_id) == BridgeProjection::ClaudeEnv {
        return provider
            .settings_config
            .get("env")
            .and_then(Value::as_object)
            .and_then(|env| env.get("ANTHROPIC_AUTH_TOKEN"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != "***")
            .map(str::to_owned)
            .ok_or_else(invalid_projection);
    }
    if projection_of(provider.agent_id) == BridgeProjection::DshJson {
        return provider
            .settings_config
            .get("api_key")
            .or_else(|| provider.settings_config.get("apiKey"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != "***")
            .map(str::to_owned)
            .ok_or_else(invalid_projection);
    }
    if provider
        .settings_config
        .get("format")
        .and_then(Value::as_str)
        != Some("toml")
    {
        return Err(invalid_projection());
    }
    let local_bearer = provider
        .settings_config
        .get("auth")
        .and_then(Value::as_object)
        .and_then(|auth| auth.get("OPENAI_API_KEY"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "***")
        .ok_or_else(invalid_projection)?;
    Ok(local_bearer.into())
}

pub(super) fn provider_owned_by(provider: &Provider, profile: &AdapterProfile) -> bool {
    let Some(rule) = rule_for_id(&profile.rule_id) else {
        return false;
    };
    provider.id == stable_id(rule.provider_prefix, &profile.source_id)
        && provider.agent_id == rule.target_agent
        && provider.meta.get("preset").and_then(Value::as_str) == Some(preset_of(rule.target_agent))
        && provider.meta.get("generatedBy").and_then(Value::as_str) == Some(GENERATED_BY)
        && provider.meta.get("adapterRuleId").and_then(Value::as_str) == Some(rule.rule_id)
        && provider
            .meta
            .get("adapterRuleVersion")
            .and_then(Value::as_u64)
            == Some(1)
        && provider
            .meta
            .get("adapterSecretMode")
            .and_then(Value::as_str)
            == Some("local_token")
        && provider
            .meta
            .get("adapterProfileId")
            .and_then(Value::as_str)
            == Some(profile.id.as_str())
        && provider
            .meta
            .get("adapterSourceRef")
            .and_then(|value| value.get("kind"))
            .and_then(Value::as_str)
            == Some(profile.source_kind.as_str())
        && provider
            .meta
            .get("adapterSourceRef")
            .and_then(|value| value.get("id"))
            .and_then(Value::as_str)
            == Some(profile.source_id.as_str())
        && adapter_bridge_kind_matches(provider, &rule)
        && provider
            .meta
            .get("adapterBridge")
            .and_then(|value| value.get("loopbackOnly"))
            .and_then(Value::as_bool)
            == Some(true)
}

fn adapter_bridge_kind_matches(provider: &Provider, rule: &CodexBridgeRule) -> bool {
    let Some(kind) = provider
        .meta
        .get("adapterBridge")
        .and_then(|value| value.get("kind"))
        .and_then(Value::as_str)
    else {
        return false;
    };
    kind == rule.bridge_kind || rule.legacy_bridge_kinds.contains(&kind)
}

/// Compare generated provider `settings_config` and `meta` to the current
/// projection. Display `name` is not part of the contract. Missing port is
/// never current.
pub(super) fn provider_matches_current_projection(
    provider: &Provider,
    profile: &AdapterProfile,
    port: Option<u16>,
    model: &str,
    context_window_tokens: Option<u32>,
) -> bool {
    let Some(port) = port else {
        return false;
    };
    let Ok(local_bearer) = local_bearer_from_provider(provider) else {
        return false;
    };
    let Ok(projected) =
        projected_provider_input(profile, &local_bearer, port, model, context_window_tokens)
    else {
        return false;
    };
    provider.settings_config == projected.settings_config && provider.meta == projected.meta
}

/// 幂等判定：已有桥投影是否已是当前规则的完整契约。
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

pub(super) fn validate_bound_port(port: u16) -> Result<()> {
    if port == 0 {
        return Err(AppError::InvalidArg(
            "adapter bridge bound port must be between 1 and 65535".into(),
        ));
    }
    Ok(())
}

pub(super) fn generate_local_bearer() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|error| {
        AppError::message("adapter.local_bearer", format!("random failed: {error}"))
    })?;
    Ok(format!("ahb_{}", URL_SAFE_NO_PAD.encode(bytes)))
}

pub(super) fn invalid_projection() -> AppError {
    AppError::message(
        "adapter.provider_conflict",
        "这条本机路由的配置不完整，无法启动。请点重试，或删除后重建。",
    )
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
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
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
