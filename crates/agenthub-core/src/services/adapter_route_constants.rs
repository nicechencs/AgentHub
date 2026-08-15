//! Shared constants and membership detection for implemented adapter write routes.
//!
//! Plan (`AdapterRouteService`) and apply (`AdapterApplyService` /
//! `AdapterSecretResolver`) import from here so endpoint, slot, env key,
//! and Kimi membership strings cannot drift.

use serde_json::Value;

use crate::models::AgentId;

/// Official Kimi coding Anthropic-compatible endpoint projected into Claude.
pub const KIMI_CLAUDE_BASE_URL: &str = "https://api.kimi.com/coding/";

/// Official GLM Coding Plan Anthropic-compatible endpoint projected into Claude.
pub const GLM_CLAUDE_BASE_URL: &str = "https://open.bigmodel.cn/api/anthropic";

/// Official DeepSeek Anthropic-compatible endpoint projected into Claude.
pub const DEEPSEEK_CLAUDE_BASE_URL: &str = "https://api.deepseek.com/anthropic";

/// Claude native_endpoint rule ids that write an Anthropic-compatible base URL.
pub const KIMI_CLAUDE_RULE_ID: &str = "kimi-membership-to-claude-v1";
pub const GLM_CLAUDE_RULE_ID: &str = "glm-coding-plan-to-claude-v1";
pub const DEEPSEEK_CLAUDE_RULE_ID: &str = "deepseek-api-to-claude-v1";

/// Substring that identifies the official Kimi Code membership HTTP host.
pub const KIMI_CODING_ENDPOINT_NEEDLE: &str = "api.kimi.com/coding";

/// Connections preset id for Kimi Code membership.
pub const KIMI_MEMBERSHIP_PRESET: &str = "kimi-code-membership";

/// Official Kimi coding OpenAI Chat Completions endpoint projected into Pi.
pub const KIMI_PI_BASE_URL: &str = "https://api.kimi.com/coding/v1";

/// Pi `models.json` provider slot for Kimi Code membership.
pub const KIMI_PI_PROVIDER_SLOT: &str = "kimi-for-coding";

/// Official GLM Coding Plan OpenAI Chat Completions endpoint projected into Pi.
pub const GLM_PI_BASE_URL: &str = "https://open.bigmodel.cn/api/coding/paas/v4";

/// Pi custom provider slot for GLM Coding Plan.
pub const GLM_PI_PROVIDER_SLOT: &str = "glm-coding-plan";

/// Pi custom provider slot for DeepSeek API.
pub const DEEPSEEK_PI_PROVIDER_SLOT: &str = "deepseek";

pub const GLM_PI_RULE_ID: &str = "glm-coding-plan-to-pi-v1";
pub const DEEPSEEK_PI_RULE_ID: &str = "deepseek-api-to-pi-v1";

/// Pi `models.json` provider slot for an explicit Anthropic API key.
pub const ANTHROPIC_PI_PROVIDER_SLOT: &str = "anthropic";

/// Pi `models.json` / `auth.json` API-key slot for OpenAI.
///
/// Verified 2026-08-15 against adapters/pi_auth.rs (`"openai": { "type": "api_key" }`)
/// and oauth/catalog.rs (alias `openai` → OAuth canonical `openai-codex`, a
/// different slot). Bind writes the API-key slot `openai`, not `openai-codex`.
pub const OPENAI_PI_PROVIDER_SLOT: &str = "openai";

/// Pi `models.json` / `auth.json` slot for xAI.
///
/// Verified 2026-08-15 against adapters/pi_auth.rs (top-level `"xai"`) and
/// oauth/catalog.rs (canonical `"xai"`, aliases: `xai`, `grok`).
pub const XAI_PI_PROVIDER_SLOT: &str = "xai";

/// Claude env key for the Anthropic-compatible base URL.
pub const ANTHROPIC_BASE_URL_ENV: &str = "ANTHROPIC_BASE_URL";

/// Claude env key that carries the membership API key (or connection marker).
pub const ANTHROPIC_AUTH_TOKEN_ENV: &str = "ANTHROPIC_AUTH_TOKEN";

/// Alternate Claude / Anthropic env key accepted when reading a source provider.
pub const ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";

/// Stored in generated reference providers instead of the source API key.
pub const CONNECTION_SECRET_MARKER: &str = "$AGENTHUB_CONNECTION_SECRET$";

/// Substring that identifies Anthropic's public API host.
pub const ANTHROPIC_API_ENDPOINT_NEEDLE: &str = "api.anthropic.com";

/// Official Anthropic Messages HTTP base (includes `/v1`).
pub const ANTHROPIC_MESSAGES_BASE_URL: &str = "https://api.anthropic.com/v1";

/// Required Anthropic request header value for Messages.
pub const ANTHROPIC_API_VERSION: &str = "2023-06-01";

/// Connections preset / extra.provider for an official OpenAI API Key.
pub const OPENAI_API_PRESET: &str = "openai";

/// Connections preset / extra.provider for an official xAI API Key.
pub const XAI_API_PRESET: &str = "xai";

/// Connections preset / extra.provider for GLM Coding Plan.
pub const GLM_CODING_PLAN_PRESET: &str = "glm-coding-plan";

/// Connections preset / extra.provider / ticket surface for DeepSeek API.
///
/// Classify also accepts the DSH-era alias `deepseek` (see
/// [`is_deepseek_api_marker`]). Do not upgrade from `agent_id=dsh` alone.
pub const DEEPSEEK_API_PRESET: &str = "deepseek-api";

/// Official DeepSeek Chat Completions host (do not append `/v1` here).
pub const DEEPSEEK_API_BASE_URL: &str = "https://api.deepseek.com";

/// Official OpenAI HTTP host. Custom OpenAI-compatible relays must not match.
pub const OPENAI_API_ENDPOINT_NEEDLE: &str = "api.openai.com";

/// Official xAI HTTP host. Custom relays must not match.
pub const XAI_API_ENDPOINT_NEEDLE: &str = "api.x.ai";

/// Official GLM Coding Plan Anthropic-compatible host path.
pub const GLM_CODING_ANTHROPIC_NEEDLE: &str = "open.bigmodel.cn/api/anthropic";

/// Official GLM Coding Plan Chat Completions host path.
pub const GLM_CODING_CHAT_NEEDLE: &str = "open.bigmodel.cn/api/coding";

/// Official DeepSeek HTTP host.
pub const DEEPSEEK_API_ENDPOINT_NEEDLE: &str = "api.deepseek.com";

/// OpenAI env key accepted when reading a source provider.
pub const OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";

/// xAI env key accepted when reading a source provider.
pub const XAI_API_KEY_ENV: &str = "XAI_API_KEY";

/// DeepSeek env key accepted when reading a source provider.
pub const DEEPSEEK_API_KEY_ENV: &str = "DEEPSEEK_API_KEY";

/// DSH official provider slot written by config_sync apply.
pub const DSH_DEEPSEEK_PROVIDER_SLOT: &str = "deepseek-official";

/// Default DSH model id when the source does not pin one.
pub const DSH_DEFAULT_MODEL: &str = "deepseek-v4-flash";

/// Env / credentials reference name written into the DSH home patch.
pub const DSH_API_KEY_ENV: &str = "DEEPSEEK_API_KEY";

/// Claude native_endpoint base URL for a writable rule. Unknown rules stay closed.
pub(crate) fn claude_native_base_url(rule_id: &str) -> Option<&'static str> {
    match rule_id {
        KIMI_CLAUDE_RULE_ID => Some(KIMI_CLAUDE_BASE_URL),
        GLM_CLAUDE_RULE_ID => Some(GLM_CLAUDE_BASE_URL),
        DEEPSEEK_CLAUDE_RULE_ID => Some(DEEPSEEK_CLAUDE_BASE_URL),
        _ => None,
    }
}

/// Membership = Kimi agent **and** (`kimi-code-membership` preset **or** official
/// coding endpoint). Never upgrade from `agent_id` alone (moonshot / custom stay closed).
pub(crate) fn is_kimi_code_membership_source(
    agent_id: AgentId,
    meta: &Value,
    settings: &Value,
) -> bool {
    agent_id == AgentId::Kimi
        && (meta_preset(meta) == Some(KIMI_MEMBERSHIP_PRESET)
            || settings_contain_kimi_coding_endpoint(settings))
}

/// True when config text/JSON contains the official Kimi Code coding host.
pub(crate) fn settings_contain_kimi_coding_endpoint(value: &Value) -> bool {
    value_contains_needle(value, KIMI_CODING_ENDPOINT_NEEDLE)
}

/// True when config points at Anthropic's public API host (not a third-party relay alone).
pub(crate) fn settings_contain_anthropic_api_endpoint(value: &Value) -> bool {
    value_contains_needle(value, ANTHROPIC_API_ENDPOINT_NEEDLE)
}

/// True when config points at OpenAI's public API host (not a custom relay).
pub(crate) fn settings_contain_openai_api_endpoint(value: &Value) -> bool {
    value_contains_needle(value, OPENAI_API_ENDPOINT_NEEDLE)
}

/// True when config points at xAI's public API host (not a custom relay).
pub(crate) fn settings_contain_xai_api_endpoint(value: &Value) -> bool {
    value_contains_needle(value, XAI_API_ENDPOINT_NEEDLE)
}

/// True when config points at an official GLM Coding Plan host path.
pub(crate) fn settings_contain_glm_coding_plan_endpoint(value: &Value) -> bool {
    value_contains_needle(value, GLM_CODING_ANTHROPIC_NEEDLE)
        || value_contains_needle(value, GLM_CODING_CHAT_NEEDLE)
}

/// True when config points at DeepSeek's public API host.
pub(crate) fn settings_contain_deepseek_api_endpoint(value: &Value) -> bool {
    value_contains_needle(value, DEEPSEEK_API_ENDPOINT_NEEDLE)
}

/// Explicit product tag from preset / extra.provider / credentials.provider.
pub(crate) fn explicit_provider_tag_matches(tag: Option<&str>, accepted: &[&str]) -> bool {
    tag.is_some_and(|value| {
        accepted
            .iter()
            .any(|accepted| value.eq_ignore_ascii_case(accepted))
    })
}

pub(crate) fn is_openai_api_marker(tag: Option<&str>, blob: &Value) -> bool {
    explicit_provider_tag_matches(tag, &[OPENAI_API_PRESET, "openai-api"])
        || settings_contain_openai_api_endpoint(blob)
}

pub(crate) fn is_xai_api_marker(tag: Option<&str>, blob: &Value) -> bool {
    explicit_provider_tag_matches(tag, &[XAI_API_PRESET, "xai-api"])
        || settings_contain_xai_api_endpoint(blob)
}

pub(crate) fn is_glm_coding_plan_marker(tag: Option<&str>, blob: &Value) -> bool {
    explicit_provider_tag_matches(tag, &[GLM_CODING_PLAN_PRESET])
        || settings_contain_glm_coding_plan_endpoint(blob)
}

/// DeepSeek API ticket = preset `deepseek-api` **or** DSH-era alias `deepseek`
/// **or** official API host. Never upgrade from `agent_id=dsh` alone.
pub(crate) fn is_deepseek_api_marker(tag: Option<&str>, blob: &Value) -> bool {
    explicit_provider_tag_matches(tag, &[DEEPSEEK_API_PRESET, "deepseek"])
        || settings_contain_deepseek_api_endpoint(blob)
}

fn meta_preset(meta: &Value) -> Option<&str> {
    meta.get("preset")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn value_contains_needle(value: &Value, needle: &str) -> bool {
    let needle = needle.to_ascii_lowercase();
    match value {
        Value::String(text) => text.to_ascii_lowercase().contains(&needle),
        Value::Array(items) => items
            .iter()
            .any(|item| value_contains_needle(item, &needle)),
        Value::Object(map) => map
            .values()
            .any(|item| value_contains_needle(item, &needle)),
        _ => false,
    }
}
