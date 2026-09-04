//! Stateful continuation bindings for Codex↔Grok pair adapters.
//!
//! `previous_response_id`, prompt cache, and Grok session seed stay on the
//! member that produced them. They are not portable across Provider/member
//! unless an adapter explicitly proves otherwise (default: not portable).

use std::sync::Mutex;
use std::time::Duration;

use axum::http::HeaderMap;
use serde_json::Value;

use crate::bridge::bounded_ttl::BoundedTtlMap;
use crate::bridge::grok_cli::extract_prompt_cache_seed;
use crate::bridge::protocol::pair::completed_response_id;

const CONTINUATION_MAX_ENTRIES: usize = 8192;
const CONTINUATION_IDLE_TTL: Duration = Duration::from_secs(2 * 60 * 60);

pub(super) struct ContinuationBindings {
    inner: Mutex<BoundedTtlMap<String, String>>,
}

impl ContinuationBindings {
    pub(super) fn new() -> Self {
        Self {
            inner: Mutex::new(BoundedTtlMap::new(
                CONTINUATION_MAX_ENTRIES,
                CONTINUATION_IDLE_TTL,
            )),
        }
    }

    pub(super) fn required_member(&self, body: &Value, headers: &HeaderMap) -> Option<String> {
        let keys = continuation_keys(body, headers);
        if keys.is_empty() {
            return None;
        }
        let Ok(mut guard) = self.inner.lock() else {
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
mod tests;
