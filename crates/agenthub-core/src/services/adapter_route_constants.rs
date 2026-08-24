//! Shared constants and membership detection for implemented adapter write routes.
//!
//! Plan (`AdapterRouteService`) and apply (`AdapterApplyService` /
//! `AdapterSecretResolver`) import from here so endpoint, slot, env key,
//! and Kimi membership strings cannot drift.

use serde_json::Value;

use crate::models::{AgentId, Provider};
use crate::utils::loopback::is_loopback_base_url;

/// Official Kimi coding Anthropic-compatible endpoint projected into Claude.
pub const KIMI_CLAUDE_BASE_URL: &str = "https://api.kimi.com/coding/";

/// Official GLM Coding Plan Anthropic-compatible endpoint projected into Claude.
pub const GLM_CLAUDE_BASE_URL: &str = "https://open.bigmodel.cn/api/anthropic";

/// Official DeepSeek Anthropic-compatible endpoint projected into Claude.
pub const DEEPSEEK_CLAUDE_BASE_URL: &str = "https://api.deepseek.com/anthropic";

/// Official GLM Coding Plan OpenAI Responses endpoint projected into Codex.
pub const GLM_CODEX_BASE_URL: &str = "https://open.bigmodel.cn/api/v1";

/// Official DeepSeek OpenAI Responses endpoint projected into Codex.
pub const DEEPSEEK_CODEX_BASE_URL: &str = DEEPSEEK_API_BASE_URL;

/// Claude native_endpoint rule ids that write an Anthropic-compatible base URL.
pub const KIMI_CLAUDE_RULE_ID: &str = "kimi-membership-to-claude-v1";
pub const GLM_CLAUDE_RULE_ID: &str = "glm-coding-plan-to-claude-v1";
pub const DEEPSEEK_CLAUDE_RULE_ID: &str = "deepseek-api-to-claude-v1";
pub const GLM_CODEX_RULE_ID: &str = "glm-coding-plan-to-codex-v1";
pub const DEEPSEEK_CODEX_RULE_ID: &str = "deepseek-api-to-codex-v1";
pub const KIMI_GROK_RULE_ID: &str = "kimi-membership-to-grok-v1";
pub const OPENAI_GROK_RULE_ID: &str = "openai-api-to-grok-v1";
pub const GROK_CLAUDE_RULE_ID: &str = "grok-subscription-to-claude-v1";
pub const GROK_CODEX_RULE_ID: &str = "grok-subscription-to-codex-v1";
pub const CODEX_GROK_RULE_ID: &str = "codex-subscription-to-grok-v1";
pub const CODEX_KIMI_RULE_ID: &str = "codex-subscription-to-kimi-v1";
pub const CODEX_DSH_RULE_ID: &str = "codex-subscription-to-dsh-v1";

pub const GLM_CODEX_DEFAULT_MODEL: &str = "glm-5.3";
pub const DEEPSEEK_CODEX_DEFAULT_MODEL: &str = "deepseek-v4-flash";
pub const GLM_CODEX_PROVIDER_PREFIX: &str = "codex-glm-adapter";
pub const DEEPSEEK_CODEX_PROVIDER_PREFIX: &str = "codex-deepseek-adapter";
pub const GLM_CODEX_PROVIDER_SLUG: &str = "agenthub_glm";
pub const DEEPSEEK_CODEX_PROVIDER_SLUG: &str = "agenthub_deepseek";
pub const KIMI_GROK_BASE_URL: &str = KIMI_PI_BASE_URL;
pub const OPENAI_GROK_BASE_URL: &str = "https://api.openai.com/v1";
pub const KIMI_GROK_DEFAULT_MODEL: &str = "kimi-k2.5";
pub const OPENAI_GROK_DEFAULT_MODEL: &str = "gpt-4o";

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

/// OpenRouter OpenAI-compat host. Classifies as [`crate::models::AdapterSourceProduct::OpenaiApi`].
pub const OPENROUTER_API_ENDPOINT_NEEDLE: &str = "openrouter.ai";

const BASE_URL_POINTERS: &[&str] = &[
    "/baseURL",
    "/baseUrl",
    "/base_url",
    "/env/OPENAI_BASE_URL",
    "/env/OPENAI_API_BASE",
    "/api_base",
    "/apiBase",
];

/// Preset / extra.provider for a custom OpenAI-compatible endpoint (incl. OpenRouter).
/// Catalog id is `openai-compatible`; `openai-compat` is accepted as an alias.
pub const OPENAI_COMPAT_PRESET: &str = "openai-compatible";

/// Alternate tag for OpenRouter stored as an OpenAI-compat login.
pub const OPENROUTER_PRESET: &str = "openrouter";

/// OpenAI-compat Chat Completions / OpenRouter → Claude local-bridge.
pub const OPENAI_CLAUDE_RULE_ID: &str = "openai-api-to-claude-v1";

/// OpenAI-compat Chat Completions / OpenRouter → Grok local-bridge.
/// Distinct from NativeEndpoint [`OPENAI_GROK_RULE_ID`].
pub const OPENAI_GROK_BRIDGE_RULE_ID: &str = "openai-api-to-grok-bridge-v1";

/// OpenAI-compat Chat Completions / OpenRouter → Codex local-bridge.
pub const OPENAI_CODEX_RULE_ID: &str = "openai-api-to-codex-v1";

/// Env key for a custom OpenAI-compat base URL.
pub const OPENAI_BASE_URL_ENV: &str = "OPENAI_BASE_URL";

/// Official xAI HTTP host. Custom relays must not match.
pub const XAI_API_ENDPOINT_NEEDLE: &str = "api.x.ai";

/// Official GLM Coding Plan Anthropic-compatible host path.
pub const GLM_CODING_ANTHROPIC_NEEDLE: &str = "open.bigmodel.cn/api/anthropic";

/// Official GLM Coding Plan Chat Completions host path.
pub const GLM_CODING_CHAT_NEEDLE: &str = "open.bigmodel.cn/api/coding";

/// Official GLM Coding Plan Responses host path.
pub const GLM_CODING_RESPONSES_NEEDLE: &str = "open.bigmodel.cn/api/v1";

/// Official DeepSeek HTTP host.
pub const DEEPSEEK_API_ENDPOINT_NEEDLE: &str = "api.deepseek.com";

/// Rule ids defined in this module. Append when adding a `*_RULE_ID`.
/// Secret-resolver matchers must cover each applyable projection.
pub const PUBLISHED_ROUTE_RULE_IDS: &[&str] = &[
    KIMI_CLAUDE_RULE_ID,
    GLM_CLAUDE_RULE_ID,
    DEEPSEEK_CLAUDE_RULE_ID,
    GLM_CODEX_RULE_ID,
    DEEPSEEK_CODEX_RULE_ID,
    KIMI_GROK_RULE_ID,
    OPENAI_GROK_RULE_ID,
    OPENAI_CLAUDE_RULE_ID,
    OPENAI_GROK_BRIDGE_RULE_ID,
    OPENAI_CODEX_RULE_ID,
    GROK_CLAUDE_RULE_ID,
    GROK_CODEX_RULE_ID,
    CODEX_GROK_RULE_ID,
    CODEX_KIMI_RULE_ID,
    CODEX_DSH_RULE_ID,
    GLM_PI_RULE_ID,
    DEEPSEEK_PI_RULE_ID,
];

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

/// Account membership = Kimi API Key **and** (`extra.provider` /
/// `extra.preset` / `credentials.provider` is the membership tag **or** the
/// credentials/extra blob contains the official coding endpoint). Managed Kimi
/// OAuth must never be upgraded to a membership API Key.
pub(crate) fn is_kimi_code_membership_account(
    agent_id: AgentId,
    extra: &Value,
    credentials: &Value,
) -> bool {
    agent_id == AgentId::Kimi
        && (account_provider_tag_matches(extra, credentials)
            || settings_contain_kimi_coding_endpoint(extra)
            || settings_contain_kimi_coding_endpoint(credentials))
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
    openai_compat_base_url(value)
        .and_then(|value| normalized_http_host(&value))
        .as_deref()
        == Some(OPENAI_API_ENDPOINT_NEEDLE)
}

/// True when config points at OpenRouter's public API host.
pub(crate) fn settings_contain_openrouter_endpoint(value: &Value) -> bool {
    openai_compat_base_url(value)
        .and_then(|value| normalized_http_host(&value))
        .as_deref()
        == Some(OPENROUTER_API_ENDPOINT_NEEDLE)
}

/// True when config points at xAI's public API host (not a custom relay).
pub(crate) fn settings_contain_xai_api_endpoint(value: &Value) -> bool {
    value_contains_needle(value, XAI_API_ENDPOINT_NEEDLE)
}

/// True when config points at an official GLM Coding Plan host path.
pub(crate) fn settings_contain_glm_coding_plan_endpoint(value: &Value) -> bool {
    value_contains_needle(value, GLM_CODING_ANTHROPIC_NEEDLE)
        || value_contains_needle(value, GLM_CODING_CHAT_NEEDLE)
        || value_contains_needle(value, GLM_CODING_RESPONSES_NEEDLE)
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

/// Known provider identities that must never be inferred as OpenAI from a URL.
///
/// These tags are product evidence for another vendor. A base URL is still
/// useful to their own marker helpers, but it cannot override the explicit
/// product identity in the OpenAI branch.
pub(crate) fn is_non_openai_provider_tag(tag: Option<&str>) -> bool {
    tag.is_some_and(|value| {
        [
            "anthropic",
            "anthropic-api",
            "anthropic-compatible",
            "deepseek",
            "deepseek-api",
            "glm-coding-plan",
            "kimi",
            "kimi-api",
            KIMI_MEMBERSHIP_PRESET,
            "moonshot",
            XAI_API_PRESET,
            "xai-api",
        ]
        .iter()
        .any(|accepted| value.eq_ignore_ascii_case(accepted))
    })
}

/// Official OpenAI/OpenRouter host, or a real OpenAI-compatible remote.
///
/// A generic compatibility marker is only evidence when an active base URL is
/// present. Official `openai` / `openrouter` markers are also checked against
/// that active URL, so a stale marker cannot authorize a different relay.
pub(crate) fn is_openai_api_marker(tag: Option<&str>, blob: &Value) -> bool {
    let tag = tag.map(str::trim).filter(|value| !value.is_empty());
    if is_non_openai_provider_tag(tag) {
        return false;
    }
    let host = openai_compat_base_url(blob).and_then(|url| normalized_http_host(&url));
    let has_active_base_url = has_active_base_url(blob);

    match tag {
        Some(value)
            if value.eq_ignore_ascii_case(OPENAI_API_PRESET)
                || value.eq_ignore_ascii_case("openai-api") =>
        {
            (!has_active_base_url && host.is_none())
                || host.as_deref() == Some(OPENAI_API_ENDPOINT_NEEDLE)
        }
        Some(value) if value.eq_ignore_ascii_case(OPENROUTER_PRESET) => {
            host.as_deref() == Some(OPENROUTER_API_ENDPOINT_NEEDLE)
        }
        Some(value)
            if value.eq_ignore_ascii_case("openai-compat")
                || value.eq_ignore_ascii_case(OPENAI_COMPAT_PRESET) =>
        {
            host.is_some()
        }
        _ => host.as_deref().is_some_and(is_openai_compat_host),
    }
}

/// Official OpenAI host is not custom. OpenRouter host and explicit
/// `openai-compat` / `openrouter` tags are. Opaque leftover
/// `openai-compatible` fixtures are not.
// Referenced only from `tests.rs` in this crate; keep for test coverage.
#[allow(dead_code)]
pub(crate) fn is_custom_openai_compat(tag: Option<&str>, blob: &Value) -> bool {
    if !is_openai_api_marker(tag, blob) {
        return false;
    }
    if settings_contain_openai_api_endpoint(blob) {
        return false;
    }
    explicit_provider_tag_matches(tag, &["openai-compat", OPENROUTER_PRESET])
        || settings_contain_openrouter_endpoint(blob)
        || openai_compat_custom_base_url(blob).is_some()
}

/// True only for OpenRouter. Official Grok / ChatGPT / other hosts must not
/// inherit stealth/ox-alpha listing from "any non-OpenAI URL".
pub fn is_custom_openai_compat_url(url: &str) -> bool {
    normalized_http_host(url).as_deref() == Some(OPENROUTER_API_ENDPOINT_NEEDLE)
}

/// True when a provider carries evidence for the official OpenAI/OpenRouter
/// product, rather than only a generic OpenAI-compatible relay marker.
pub(crate) fn provider_has_official_openai_api_evidence(provider: &Provider) -> bool {
    let tag = [
        provider.meta.get("preset").and_then(Value::as_str),
        provider.meta.get("provider").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|tag| !tag.is_empty());
    let host = openai_compat_base_url(&provider.settings_config)
        .and_then(|url| normalized_http_host(&url));
    let has_active_base_url = has_active_base_url(&provider.settings_config);

    if is_non_openai_provider_tag(tag) || has_ambiguous_active_base_url(&provider.settings_config) {
        return false;
    }

    match tag {
        Some(value)
            if value.eq_ignore_ascii_case(OPENAI_API_PRESET)
                || value.eq_ignore_ascii_case("openai-api") =>
        {
            (!has_active_base_url && host.is_none())
                || host.as_deref() == Some(OPENAI_API_ENDPOINT_NEEDLE)
        }
        Some(value) if value.eq_ignore_ascii_case(OPENROUTER_PRESET) => {
            host.as_deref() == Some(OPENROUTER_API_ENDPOINT_NEEDLE)
        }
        _ => host.as_deref().is_some_and(|value| {
            value == OPENAI_API_ENDPOINT_NEEDLE || value == OPENROUTER_API_ENDPOINT_NEEDLE
        }),
    }
}

/// True when a provider points at an untrusted custom OpenAI-compatible relay.
///
/// Adapter route planning may recognize arbitrary OpenAI-compatible remotes,
/// but bind must keep those rows closed. This is the single shared guard for
/// both core bind paths.
pub(crate) fn is_unknown_custom_relay_provider(provider: &Provider) -> bool {
    let tag = provider
        .meta
        .get("preset")
        .or_else(|| provider.meta.get("provider"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|tag| !tag.is_empty());

    let is_non_openai_tag = is_non_openai_provider_tag(tag);
    let has_active_base_url = has_active_base_url(&provider.settings_config);
    let host = openai_compat_base_url(&provider.settings_config)
        .and_then(|url| normalized_http_host(&url));

    if has_ambiguous_active_base_url(&provider.settings_config) {
        return true;
    }

    if has_active_base_url {
        if tag.is_some_and(|tag| {
            (explicit_provider_tag_matches(Some(tag), &[OPENAI_API_PRESET, "openai-api"])
                && host.as_deref() != Some(OPENAI_API_ENDPOINT_NEEDLE))
                || (explicit_provider_tag_matches(Some(tag), &[OPENROUTER_PRESET])
                    && host.as_deref() != Some(OPENROUTER_API_ENDPOINT_NEEDLE))
                || (is_non_openai_tag
                    && host.as_deref().is_some_and(|host| {
                        host == OPENAI_API_ENDPOINT_NEEDLE || host == OPENROUTER_API_ENDPOINT_NEEDLE
                    }))
        }) {
            return true;
        }

        if host.is_none() {
            return true;
        }
    }

    if host.as_deref().is_some_and(|host| {
        host == OPENAI_API_ENDPOINT_NEEDLE || host == OPENROUTER_API_ENDPOINT_NEEDLE
    }) {
        return false;
    }

    settings_contain_custom_openai_compat_remote(&provider.settings_config)
}

/// First usable OpenAI-compat base URL in a settings / credentials blob.
pub(crate) fn openai_compat_base_url(blob: &Value) -> Option<String> {
    first_http_url(blob, BASE_URL_POINTERS)
}

/// Custom (non-official, non-other-vendor) OpenAI-compat URL, if any.
pub(crate) fn openai_compat_custom_base_url(blob: &Value) -> Option<String> {
    let url = openai_compat_base_url(blob)?;
    if !looks_like_openai_compat_base_url(&url) {
        return None;
    }
    Some(url)
}

// Referenced only from `tests.rs` in this crate; keep for test coverage.
#[allow(dead_code)]
fn has_openai_shaped_secret(blob: &Value) -> bool {
    blob.pointer("/env/OPENAI_API_KEY")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
        || blob
            .get("apiKey")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
        || blob
            .get("api_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
}

#[cfg(test)]
mod tests;

fn looks_like_openai_compat_base_url(url: &str) -> bool {
    let Some(host) = normalized_http_host(url) else {
        return false;
    };
    is_openai_compat_host(&host)
}

/// Normalize a single HTTP(S) URL's host without treating arbitrary text as a URL.
///
/// Host comparisons intentionally happen after parsing the authority, so
/// `api.openai.com.evil.example`, comments, and strings containing multiple URLs
/// cannot prove an official product host.
pub(crate) fn normalized_http_host(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    let parsed = reqwest::Url::parse(trimmed).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    parsed.host_str().map(|host| host.to_ascii_lowercase())
}

fn is_openai_compat_host(host: &str) -> bool {
    const OTHER_VENDOR_HOSTS: &[&str] = &[
        "api.kimi.com",
        "api.anthropic.com",
        "api.x.ai",
        "open.bigmodel.cn",
        "api.deepseek.com",
    ];
    host == OPENAI_API_ENDPOINT_NEEDLE
        || host == OPENROUTER_API_ENDPOINT_NEEDLE
        || !OTHER_VENDOR_HOSTS.contains(&host)
}

fn first_http_url(value: &Value, pointers: &[&str]) -> Option<String> {
    let is_toml = value
        .get("format")
        .and_then(Value::as_str)
        .is_some_and(|format| format.eq_ignore_ascii_case("toml"));
    if is_toml {
        if let Some(content) = value.get("content").and_then(Value::as_str) {
            return first_toml_http_url(content);
        }
    }

    if let Some(url) = pointers.iter().find_map(|pointer| {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|candidate| normalized_http_host(candidate).is_some())
            .map(str::to_owned)
    }) {
        return Some(url);
    }
    None
}

fn first_toml_http_url(content: &str) -> Option<String> {
    let doc = content.parse::<toml_edit::DocumentMut>().ok()?;
    let slug = toml_active_provider_slug(&doc)?;
    let provider_url = doc
        .get("model_providers")
        .and_then(|item| item.as_table())
        .and_then(|providers| providers.get(&slug))
        .and_then(|provider| provider.get("base_url"))
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    provider_url
        .or_else(|| {
            doc.get("base_url")
                .and_then(|item| item.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
        .filter(|value| normalized_http_host(value).is_some())
}

fn toml_active_provider_slug(doc: &toml_edit::DocumentMut) -> Option<String> {
    let top = doc
        .get("model_provider")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(top) = top {
        return Some(top.to_owned());
    }

    let providers = doc.get("model_providers")?.as_table()?;
    let mut entries = providers.iter();
    let (slug, _) = entries.next()?;
    if entries.next().is_none() {
        Some(slug.to_string())
    } else {
        None
    }
}

fn has_active_base_url(blob: &Value) -> bool {
    let is_toml = blob
        .get("format")
        .and_then(Value::as_str)
        .is_some_and(|format| format.eq_ignore_ascii_case("toml"));
    if is_toml {
        if let Some(content) = blob.get("content").and_then(Value::as_str) {
            let Ok(doc) = content.parse::<toml_edit::DocumentMut>() else {
                return false;
            };
            let Some(slug) = toml_active_provider_slug(&doc) else {
                return doc
                    .get("model_providers")
                    .and_then(|item| item.as_table())
                    .is_some_and(|providers| {
                        providers.iter().any(|(_, provider)| {
                            provider
                                .get("base_url")
                                .and_then(|item| item.as_str())
                                .is_some_and(|value| !value.trim().is_empty())
                        })
                    });
            };
            return doc
                .get("model_providers")
                .and_then(|item| item.as_table())
                .and_then(|providers| providers.get(&slug))
                .and_then(|provider| provider.get("base_url"))
                .and_then(|item| item.as_str())
                .is_some_and(|value| !value.trim().is_empty())
                || doc
                    .get("base_url")
                    .and_then(|item| item.as_str())
                    .is_some_and(|value| !value.trim().is_empty());
        }
    }

    BASE_URL_POINTERS.iter().any(|pointer| {
        blob.pointer(pointer)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
    })
}

/// Multiple distinct active base URL fields are ambiguous and must not prove
/// an official product. Repeated aliases containing the same URL are allowed;
/// this preserves provider snapshots that duplicate `baseURL`/`baseUrl`.
fn has_ambiguous_active_base_url(blob: &Value) -> bool {
    let is_toml = blob
        .get("format")
        .and_then(Value::as_str)
        .is_some_and(|format| format.eq_ignore_ascii_case("toml"));
    if is_toml {
        let Some(content) = blob.get("content").and_then(Value::as_str) else {
            return false;
        };
        let Ok(doc) = content.parse::<toml_edit::DocumentMut>() else {
            return false;
        };
        let Some(slug) = toml_active_provider_slug(&doc) else {
            return false;
        };
        let mut values = Vec::new();
        if let Some(value) = doc
            .get("model_providers")
            .and_then(|item| item.as_table())
            .and_then(|providers| providers.get(&slug))
            .and_then(|provider| provider.get("base_url"))
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            values.push(value.to_owned());
        }
        if let Some(value) = doc
            .get("base_url")
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            values.push(value.to_owned());
        }
        return has_multiple_distinct_base_urls(values);
    }

    let values = BASE_URL_POINTERS
        .iter()
        .filter_map(|pointer| blob.pointer(pointer))
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    has_multiple_distinct_base_urls(values)
}

fn has_multiple_distinct_base_urls(values: Vec<String>) -> bool {
    let mut distinct = Vec::new();
    for value in values {
        if !distinct
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&value))
        {
            distinct.push(value);
        }
    }
    distinct.len() > 1
}

/// Optional model id pinned on a custom OpenAI-compat provider.
pub(crate) fn openai_compat_listed_models(blob: &Value) -> Vec<String> {
    let mut listed = Vec::new();
    if let Some(items) = blob.get("listedModels").and_then(Value::as_array) {
        for item in items {
            if let Some(model) = item
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                if !listed
                    .iter()
                    .any(|existing: &String| existing.eq_ignore_ascii_case(model))
                {
                    listed.push(model.to_owned());
                }
            }
        }
    }
    listed
}

pub(crate) fn openai_compat_endpoint_url(blob: &Value, target: &str) -> Option<String> {
    let rows = blob.get("endpoints").and_then(Value::as_array)?;
    for row in rows {
        let target_ok = row.get("target").and_then(Value::as_str) == Some(target);
        let enabled = row.get("enabled").and_then(Value::as_bool).unwrap_or(true);
        if !target_ok || !enabled {
            continue;
        }
        if let Some(url) = row
            .get("url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| normalized_http_host(value).is_some())
        {
            return Some(url.to_owned());
        }
    }
    None
}

pub(crate) fn looks_like_anthropic_messages_url(url: &str) -> bool {
    let Some(host) = normalized_http_host(url) else {
        return false;
    };
    if host == ANTHROPIC_API_ENDPOINT_NEEDLE {
        return true;
    }
    reqwest::Url::parse(url.trim()).ok().is_some_and(|parsed| {
        parsed
            .path()
            .split('/')
            .any(|segment| segment.eq_ignore_ascii_case("anthropic"))
    })
}

pub(crate) fn openai_compat_pinned_model(blob: &Value) -> Option<String> {
    if let Some(listed) = blob.get("listedModels").and_then(Value::as_array) {
        let first = listed.iter().find_map(|item| {
            item.as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        });
        if first.is_some() {
            return first;
        }
    }
    if blob
        .get("format")
        .and_then(Value::as_str)
        .is_some_and(|format| format.eq_ignore_ascii_case("toml"))
    {
        if let Some(content) = blob.get("content").and_then(Value::as_str) {
            if let Ok(doc) = content.parse::<toml_edit::DocumentMut>() {
                if let Some(model) = doc
                    .get("model")
                    .and_then(|item| item.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    return Some(model.to_owned());
                }
            }
        }
    }

    ["model", "default_model", "defaultModel"]
        .iter()
        .find_map(|key| {
            blob.get(*key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
}

/// True when settings/TOML contain a remote (non-loopback) OpenAI-compat URL.
/// Catalog `openai-compatible` leftovers with only a loopback 本机路由 stay unknown.
pub(crate) fn settings_contain_custom_openai_compat_remote(blob: &Value) -> bool {
    let Some(url) = openai_compat_base_url(blob) else {
        return false;
    };
    looks_like_openai_compat_base_url(&url) && !is_loopback_base_url(&url)
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

fn account_provider_tag_matches(extra: &Value, credentials: &Value) -> bool {
    [
        extra.get("provider").and_then(Value::as_str),
        extra.get("preset").and_then(Value::as_str),
        credentials.get("provider").and_then(Value::as_str),
    ]
    .into_iter()
    .any(|tag| explicit_provider_tag_matches(tag, &[KIMI_MEMBERSHIP_PRESET]))
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
