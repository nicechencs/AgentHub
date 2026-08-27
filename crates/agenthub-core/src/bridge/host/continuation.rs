//! Stateful continuation bindings for Codex↔Grok pair adapters.
//!
//! `previous_response_id`, prompt cache, and Grok session seed stay on the
//! member that produced them. They are not portable across Provider/member
//! unless an adapter explicitly proves otherwise (default: not portable).

use std::collections::HashMap;
use std::sync::Mutex;

use axum::http::HeaderMap;
use serde_json::Value;

use crate::bridge::grok_cli::extract_prompt_cache_seed;
use crate::bridge::protocol::pair::{completed_response_id, is_stateful_continuation};

#[derive(Default)]
pub(super) struct ContinuationBindings {
    inner: Mutex<HashMap<String, String>>,
}

impl ContinuationBindings {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn required_member(&self, body: &Value, headers: &HeaderMap) -> Option<String> {
        let keys = continuation_keys(body, headers);
        if keys.is_empty() {
            return None;
        }
        let Ok(guard) = self.inner.lock() else {
            return None;
        };
        for key in keys {
            if let Some(member_id) = guard.get(&key) {
                return Some(member_id.clone());
            }
        }
        None
    }

    pub(super) fn record_response(
        &self,
        response_body: &Value,
        cache_seed: Option<&str>,
        member_id: &str,
    ) {
        if member_id.trim().is_empty() {
            return;
        }
        let mut keys = Vec::new();
        if let Some(id) = completed_response_id(response_body) {
            keys.push(format!("response:{id}"));
        }
        if let Some(seed) = cache_seed.map(str::trim).filter(|value| !value.is_empty()) {
            keys.push(format!("cache:{seed}"));
        }
        if keys.is_empty() {
            return;
        }
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        for key in keys {
            guard.insert(key, member_id.to_owned());
        }
    }

    pub(super) fn has_stateful_fields(body: &Value, headers: &HeaderMap) -> bool {
        is_stateful_continuation(body) || extract_prompt_cache_seed(headers, body).is_some()
    }
}

/// Session identifier for route-scoped sticky. Reuses continuation /
/// prompt-cache / conversation / client session extractors. Never used as a
/// raw global map key.
///
/// Prefer the stable cache/session seed. `previous_response_id` changes every
/// turn, so it is only a fallback when no seed exists.
pub(super) fn session_identifier(body: &Value, headers: &HeaderMap) -> Option<String> {
    if let Some(seed) = extract_prompt_cache_seed(headers, body) {
        return Some(seed);
    }
    crate::bridge::protocol::pair::previous_response_id(body).map(str::to_owned)
}

fn continuation_keys(body: &Value, headers: &HeaderMap) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(id) = body
        .get("previous_response_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        keys.push(format!("response:{id}"));
    }
    if let Some(seed) = extract_prompt_cache_seed(headers, body) {
        keys.push(format!("cache:{seed}"));
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use serde_json::json;

    #[test]
    fn session_identifier_prefers_cache_seed_over_previous_response_id() {
        let body = json!({
            "previous_response_id": "resp_turn_2",
            "prompt_cache_key": "cache-stable"
        });
        let id = session_identifier(&body, &HeaderMap::new()).expect("seed");
        assert_eq!(id, "cache-stable");
        assert!(!id.contains("resp_turn_2"));
    }

    #[test]
    fn session_identifier_prefers_session_header_over_previous_response_id() {
        let mut headers = HeaderMap::new();
        headers.insert("x-session-id", HeaderValue::from_static("stable-session"));
        let body = json!({ "previous_response_id": "resp_turn_2" });
        let id = session_identifier(&body, &headers).expect("header");
        assert_eq!(id, "stable-session");
    }

    #[test]
    fn session_identifier_falls_back_to_previous_response_id() {
        let body = json!({ "previous_response_id": "resp_only" });
        assert_eq!(
            session_identifier(&body, &HeaderMap::new()).as_deref(),
            Some("resp_only")
        );
    }
}
