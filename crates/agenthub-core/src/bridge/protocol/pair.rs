//! Codex ↔ Grok Responses pair adapters.
//!
//! Both products speak Responses, but matching surface is not passthrough.
//! Transparent relay requires an explicit dialect-compatibility mark **and** a
//! passing sanitizer/validator. No Codex↔Grok pair is marked transparent.
//!
//! Direction-explicit adapters:
//! - Codex ingress → Grok upstream
//! - Grok ingress → Codex upstream
//!
//! Codex allowlist lives in [`super::responses::prepare_official_codex_request`].
//! Do not fork a second allowlist here.

use serde_json::{Map, Value};

use super::responses::{fold_official_codex_system_items, prepare_official_codex_request};

pub use crate::bridge::runtime::ResponsesDialect;

#[cfg(test)]
mod tests;

/// Explicit compatibility. Matching Responses surface is not a mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialectCompatibility {
    /// Proven transparent relay. Currently unused for Codex↔Grok.
    Transparent,
    /// Direction-explicit pair adapter required.
    Adapted,
}

/// Codex client talking to a Grok member, or Grok client talking to a Codex member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairDirection {
    CodexIngressGrokUpstream,
    GrokIngressCodexUpstream,
}

impl PairDirection {
    pub fn from_dialects(downstream: ResponsesDialect, upstream: ResponsesDialect) -> Option<Self> {
        match (downstream, upstream) {
            (ResponsesDialect::Codex, ResponsesDialect::Grok) => {
                Some(Self::CodexIngressGrokUpstream)
            }
            (ResponsesDialect::Grok, ResponsesDialect::Codex) => {
                Some(Self::GrokIngressCodexUpstream)
            }
            (ResponsesDialect::Codex, ResponsesDialect::Codex)
            | (ResponsesDialect::Grok, ResponsesDialect::Grok) => None,
        }
    }

    pub fn downstream(self) -> ResponsesDialect {
        match self {
            Self::CodexIngressGrokUpstream => ResponsesDialect::Codex,
            Self::GrokIngressCodexUpstream => ResponsesDialect::Grok,
        }
    }

    pub fn upstream(self) -> ResponsesDialect {
        match self {
            Self::CodexIngressGrokUpstream => ResponsesDialect::Grok,
            Self::GrokIngressCodexUpstream => ResponsesDialect::Codex,
        }
    }
}

/// Compatibility for a dialect pair. Never returns Transparent just because
/// both sides are Responses.
pub fn dialect_compatibility(
    downstream: ResponsesDialect,
    upstream: ResponsesDialect,
) -> DialectCompatibility {
    if explicit_transparent_relay(downstream, upstream) {
        DialectCompatibility::Transparent
    } else {
        DialectCompatibility::Adapted
    }
}

/// Explicit transparent-relay table. Empty: do not add a blanket Responses mark.
pub fn explicit_transparent_relay(
    downstream: ResponsesDialect,
    upstream: ResponsesDialect,
) -> bool {
    let _ = (downstream, upstream);
    false
}

/// Whether a body that already passed the pair sanitizer could be byte-relayed.
/// Always false until a pair is explicitly marked transparent.
pub fn sanitizer_allows_transparent(body: &Value) -> bool {
    let _ = body;
    false
}

/// Codex-shaped Responses body → Grok upstream. Does **not** run the official
/// Codex allowlist (that would drop Grok cache / reasoning / hosted tools).
pub fn adapt_codex_request_for_grok_upstream(body: &mut Value) {
    fold_official_codex_system_items(body);
    if let Some(object) = body.as_object_mut() {
        object.remove("store");
        for key in CODEX_ONLY_REQUEST_KEYS {
            object.remove(*key);
        }
    }
}

/// Grok-shaped Responses body → official Codex upstream. Reuses the existing
/// allowlist, `store:false`, `stream:true`, and system folding.
pub fn adapt_grok_request_for_codex_upstream(
    body: &mut Value,
    incoming_model: &str,
    configured_model: Option<&str>,
) {
    prepare_official_codex_request(body, incoming_model, configured_model);
}

/// Strip Grok identity / session fields before a Codex client sees the body.
pub fn sanitize_grok_response_for_codex(body: &mut Value) -> bool {
    sanitize_object_tree(body, grok_only_key);
    true
}

/// Strip Codex-only fields before a Grok client sees the body.
pub fn sanitize_codex_response_for_grok(body: &mut Value) -> bool {
    sanitize_object_tree(body, codex_only_key);
    true
}

pub fn sanitize_pair_response(direction: PairDirection, body: &mut Value) -> bool {
    match direction {
        PairDirection::CodexIngressGrokUpstream => sanitize_grok_response_for_codex(body),
        PairDirection::GrokIngressCodexUpstream => sanitize_codex_response_for_grok(body),
    }
}

/// Sanitize one Responses SSE JSON payload (the `data:` object).
pub fn sanitize_pair_sse_event(direction: PairDirection, event: &mut Value) -> bool {
    sanitize_pair_response(direction, event)
}

pub fn is_stateful_continuation(body: &Value) -> bool {
    nonempty_str(body.get("previous_response_id")).is_some()
        || nonempty_str(body.get("prompt_cache_key")).is_some()
}

pub fn previous_response_id(body: &Value) -> Option<&str> {
    nonempty_str(body.get("previous_response_id"))
}

pub fn prompt_cache_key(body: &Value) -> Option<&str> {
    nonempty_str(body.get("prompt_cache_key"))
}

pub fn completed_response_id(body: &Value) -> Option<&str> {
    nonempty_str(body.get("id")).or_else(|| {
        body.get("response")
            .and_then(|response| nonempty_str(response.get("id")))
    })
}

/// Chat Completions leftovers and Codex-only request keys that must not hit
/// Grok raw. Intentionally excludes Grok cache / reasoning / include / tools.
const CODEX_ONLY_REQUEST_KEYS: &[&str] = &[
    "metadata",
    "max_tokens",
    "service_tier",
    "text",
    "truncation",
    "user",
];

const GROK_ONLY_RESPONSE_KEYS: &[&str] = &[
    "prompt_cache_key",
    "session_id",
    "grok_session_id",
    "x_grok_session_id",
    "x_grok_conv_id",
    "x_grok_req_id",
    "x_grok_agent_id",
    "x_grok_client_version",
    "x_grok_model_override",
    "conv_id",
    "conversation_id",
    "server_side_session",
];

const CODEX_ONLY_RESPONSE_KEYS: &[&str] = &["store", "service_tier", "metadata"];

fn grok_only_key(key: &str) -> bool {
    GROK_ONLY_RESPONSE_KEYS.contains(&key)
        || key.starts_with("x_grok_")
        || key.starts_with("x-grok-")
}

fn codex_only_key(key: &str) -> bool {
    CODEX_ONLY_RESPONSE_KEYS.contains(&key)
}

fn sanitize_object_tree(value: &mut Value, drop_key: fn(&str) -> bool) {
    match value {
        Value::Object(object) => sanitize_map(object, drop_key),
        Value::Array(items) => {
            for item in items {
                sanitize_object_tree(item, drop_key);
            }
        }
        _ => {}
    }
}

fn sanitize_map(object: &mut Map<String, Value>, drop_key: fn(&str) -> bool) {
    object.retain(|key, _| !drop_key(key));
    for child in object.values_mut() {
        sanitize_object_tree(child, drop_key);
    }
}

fn nonempty_str(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
