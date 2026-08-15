//! Shared constants and membership detection for implemented adapter write routes.
//!
//! Plan (`AdapterRouteService`) and apply (`AdapterApplyService` /
//! `AdapterSecretResolver`) import from here so endpoint, slot, env key,
//! and Kimi membership strings cannot drift.

use serde_json::Value;

use crate::models::AgentId;

/// Official Kimi coding Anthropic-compatible endpoint projected into Claude.
pub const KIMI_CLAUDE_BASE_URL: &str = "https://api.kimi.com/coding/";

/// Substring that identifies the official Kimi Code membership HTTP host.
pub const KIMI_CODING_ENDPOINT_NEEDLE: &str = "api.kimi.com/coding";

/// Connections preset id for Kimi Code membership.
pub const KIMI_MEMBERSHIP_PRESET: &str = "kimi-code-membership";

/// Official Kimi coding OpenAI Chat Completions endpoint projected into Pi.
pub const KIMI_PI_BASE_URL: &str = "https://api.kimi.com/coding/v1";

/// Pi `models.json` provider slot for Kimi Code membership.
pub const KIMI_PI_PROVIDER_SLOT: &str = "kimi-for-coding";

/// Pi `models.json` provider slot for an explicit Anthropic API key.
pub const ANTHROPIC_PI_PROVIDER_SLOT: &str = "anthropic";

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

/// Connections preset id for DeepSeek official API.
pub const DEEPSEEK_API_PRESET: &str = "deepseek";

/// Official DeepSeek API host (Chat Completions; do not append `/v1` here).
pub const DEEPSEEK_API_BASE_URL: &str = "https://api.deepseek.com";

/// Substring that identifies DeepSeek's public API host.
pub const DEEPSEEK_API_ENDPOINT_NEEDLE: &str = "api.deepseek.com";

/// DSH official provider slot written by config_sync apply.
pub const DSH_DEEPSEEK_PROVIDER_SLOT: &str = "deepseek-official";

/// Default DSH model id when the source does not pin one.
pub const DSH_DEFAULT_MODEL: &str = "deepseek-v4-flash";

/// Env / credentials reference name written into the DSH home patch.
pub const DSH_API_KEY_ENV: &str = "DEEPSEEK_API_KEY";

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

/// DeepSeek API ticket = explicit `deepseek` preset **or** official API host.
/// Never upgrade from `agent_id=dsh` alone.
pub(crate) fn is_deepseek_api_source(preset: Option<&str>, settings: &Value) -> bool {
    preset.is_some_and(|value| value.eq_ignore_ascii_case(DEEPSEEK_API_PRESET))
        || settings_contain_deepseek_api_endpoint(settings)
}

/// True when config text/JSON contains the official DeepSeek API host.
pub(crate) fn settings_contain_deepseek_api_endpoint(value: &Value) -> bool {
    value_contains_needle(value, DEEPSEEK_API_ENDPOINT_NEEDLE)
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
