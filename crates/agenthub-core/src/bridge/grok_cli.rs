//! Grok CLI session identity for subscription Responses upstream.
//!
//! SuperGrok / Grok Build OAuth talks to `cli-chat-proxy.grok.com`, not the
//! public `api.x.ai` Chat Completions surface. The proxy 426s without these
//! client headers. Quota probes reuse the same identity pairs.
//!
//! Session IDs are hashed from a client cache seed; never invent a random UUID
//! per request (that zeroes prompt cache).

mod replay;
mod session;
mod tools;

pub use replay::{
    is_reasoning_decode_failure, strip_encrypted_reasoning, GrokReasoningReplay,
};
pub use session::{extract_prompt_cache_seed, grok_session_id, grok_session_id_for_account};
pub use tools::{inject_prompt_cache_key, normalize_grok_build_tools};

use std::sync::OnceLock;

pub const GROK_CLI_VERSION: &str = "0.2.114";
pub const GROK_CLI_PROXY_BASE_URL: &str = "https://cli-chat-proxy.grok.com/v1";
pub const GROK_CLI_DEFAULT_MODEL: &str = "grok-4.5";
pub const GROK_CLI_TOKEN_AUTH: &str = "xai-grok-cli";
pub const GROK_CLI_IDENTIFIER: &str = "grok-shell";
pub const GROK_CLI_MODE: &str = "headless";

#[derive(Debug, Clone)]
pub struct GrokCliRequestIdentity {
    pub request_id: String,
    pub session_id: Option<String>,
    pub model_override: Option<String>,
}

pub fn grok_cli_user_agent() -> String {
    format!("grok-pager/{GROK_CLI_VERSION} grok-shell/{GROK_CLI_VERSION}")
}

/// Static Grok CLI identity headers for non-reqwest clients (quota probe).
pub fn grok_cli_identity_header_pairs() -> Vec<(&'static str, String)> {
    vec![
        ("x-xai-token-auth", GROK_CLI_TOKEN_AUTH.to_string()),
        ("x-grok-client-version", GROK_CLI_VERSION.to_string()),
        ("x-grok-client-identifier", GROK_CLI_IDENTIFIER.to_string()),
        ("x-grok-client-mode", GROK_CLI_MODE.to_string()),
        ("x-authenticateresponse", "authenticate-response".to_string()),
        ("User-Agent", grok_cli_user_agent()),
    ]
}

pub fn grok_cli_request_identity(
    request_id: impl Into<String>,
    headers: &axum::http::HeaderMap,
    body: &serde_json::Value,
    model_override: Option<&str>,
) -> GrokCliRequestIdentity {
    grok_cli_request_identity_for_account(request_id, headers, body, model_override, None)
}

pub fn grok_cli_request_identity_for_account(
    request_id: impl Into<String>,
    headers: &axum::http::HeaderMap,
    body: &serde_json::Value,
    model_override: Option<&str>,
    account_id: Option<&str>,
) -> GrokCliRequestIdentity {
    let seed = extract_prompt_cache_seed(headers, body);
    GrokCliRequestIdentity {
        request_id: request_id.into(),
        session_id: seed
            .as_deref()
            .and_then(|seed| grok_session_id_for_account(seed, account_id)),
        model_override: model_override
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_owned),
    }
}

pub fn apply_grok_cli_identity(builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    apply_grok_cli_identity_with(builder, None)
}

pub fn apply_grok_cli_identity_with(
    builder: reqwest::RequestBuilder,
    identity: Option<&GrokCliRequestIdentity>,
) -> reqwest::RequestBuilder {
    let mut builder = builder;
    for (name, value) in grok_cli_identity_header_pairs() {
        builder = builder.header(name, value);
    }

    let Some(identity) = identity else {
        return builder;
    };

    builder = builder
        .header("x-grok-agent-id", grok_cli_agent_id())
        .header("x-grok-req-id", identity.request_id.as_str());

    if let Some(session_id) = identity.session_id.as_deref() {
        builder = builder
            .header("x-grok-session-id", session_id)
            .header("x-grok-conv-id", session_id);
    }

    if let Some(model) = identity
        .model_override
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
    {
        builder = builder.header("x-grok-model-override", model);
    }

    if let Some(traceparent) = grok_cli_traceparent() {
        builder = builder.header("traceparent", traceparent);
    }

    builder
}

fn grok_cli_agent_id() -> &'static str {
    static AGENT_ID: OnceLock<String> = OnceLock::new();
    AGENT_ID
        .get_or_init(|| uuid::Uuid::new_v4().to_string())
        .as_str()
}

fn grok_cli_traceparent() -> Option<String> {
    let mut bytes = [0u8; 24];
    getrandom::getrandom(&mut bytes).ok()?;
    if bytes[..16].iter().all(|&b| b == 0) {
        bytes[0] = 1;
    }
    if bytes[16..].iter().all(|&b| b == 0) {
        bytes[16] = 1;
    }
    Some(format!(
        "00-{}-{}-01",
        hex_lower(&bytes[..16]),
        hex_lower(&bytes[16..])
    ))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests;
