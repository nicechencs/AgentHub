//! In-memory inbound request log for local-route HTTP.
//!
//! Credential-free: time, method, path (no query), status, ok/fail only.
//! Never stores Authorization, bodies, tokens, or API keys.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

pub const INBOUND_LOG_CAP: usize = 20;

/// One observed local-route request. Fields are a closed allow-list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundRequestRecord {
    pub at_unix_ms: u128,
    pub method: String,
    pub path: String,
    pub status: u16,
    pub ok: bool,
}

impl InboundRequestRecord {
    pub fn new(method: impl AsRef<str>, path: impl AsRef<str>, status: u16) -> Self {
        Self {
            at_unix_ms: now_unix_ms(),
            method: sanitize_method(method.as_ref()),
            path: sanitize_path(path.as_ref()),
            status,
            ok: (200..400).contains(&status),
        }
    }
}

/// Per-profile ring of the last [`INBOUND_LOG_CAP`] inbound requests, newest first.
#[derive(Clone, Default)]
pub struct InboundRequestLog {
    inner: Arc<Mutex<HashMap<String, VecDeque<InboundRequestRecord>>>>,
}

impl InboundRequestLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, profile_id: &str, record: InboundRequestRecord) {
        if profile_id.is_empty() {
            return;
        }
        let Ok(mut map) = self.inner.lock() else {
            return;
        };
        let deque = map.entry(profile_id.to_owned()).or_default();
        deque.push_front(record);
        deque.truncate(INBOUND_LOG_CAP);
    }

    pub fn recent(&self, profile_id: &str) -> Vec<InboundRequestRecord> {
        self.inner
            .lock()
            .ok()
            .and_then(|map| map.get(profile_id).cloned())
            .map(|deque| deque.into_iter().collect())
            .unwrap_or_default()
    }
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0)
}

fn sanitize_method(method: &str) -> String {
    let cleaned: String = method
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .take(16)
        .collect();
    let upper = cleaned.to_ascii_uppercase();
    match upper.as_str() {
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" => upper,
        _ => "OTHER".into(),
    }
}

/// Path only: drop query/fragment and characters that could smuggle secrets.
fn sanitize_path(path: &str) -> String {
    let base = path.split(['?', '#']).next().unwrap_or("/");
    let mut out = String::new();
    for ch in base.chars() {
        if out.len() >= 128 {
            break;
        }
        if ch.is_ascii_alphanumeric() || matches!(ch, '/' | '-' | '_' | '.') {
            out.push(ch);
        }
    }
    if !out.starts_with('/') {
        out.insert(0, '/');
    }
    out.truncate(128);
    if out.is_empty() {
        "/".into()
    } else {
        out
    }
}

#[cfg(test)]
mod tests;
