//! Server-side Grok reasoning replay for the local bridge.
//!
//! Claude Code and Codex do not round-trip `encrypted_content`. Cache those
//! items per (model, session seed) and re-inject them on the next Responses
//! POST so Grok can continue the same chain. Never mint a session to hold this.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

const MAX_ENTRIES: usize = 64;
const MAX_CAPTURE_BYTES: usize = 8 << 20;

const DECODE_MARKERS: &[&str] = &[
    "could not decode the compaction blob",
    "could not decrypt the provided encrypted_content",
];

#[derive(Clone, Default)]
pub struct GrokReasoningReplay {
    entries: Arc<Mutex<HashMap<String, Vec<Value>>>>,
}

impl GrokReasoningReplay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&self, body: &mut Value, model: &str, session: Option<&str>) {
        let Some(session) = nonempty(session) else {
            return;
        };
        let model = model.trim();
        if model.is_empty() {
            return;
        }
        if has_previous_response_id(body) {
            return;
        }
        let items = {
            let Ok(guard) = self.entries.lock() else {
                return;
            };
            guard
                .get(&cache_key(model, session))
                .cloned()
                .unwrap_or_default()
        };
        if items.is_empty() {
            return;
        }
        inject_reasoning_items(body, &items);
    }

    pub fn store_completed(&self, model: &str, session: Option<&str>, completed: &Value) {
        let Some(session) = nonempty(session) else {
            return;
        };
        let model = model.trim();
        if model.is_empty() {
            return;
        }
        let items = extract_reasoning_items(completed);
        let Ok(mut guard) = self.entries.lock() else {
            return;
        };
        let key = cache_key(model, session);
        if items.is_empty() {
            guard.remove(&key);
            return;
        }
        if !guard.contains_key(&key) && guard.len() >= MAX_ENTRIES {
            if let Some(oldest) = guard.keys().next().cloned() {
                guard.remove(&oldest);
            }
        }
        guard.insert(key, items);
    }

    pub fn store_sse(&self, model: &str, session: Option<&str>, sse: &str) {
        if let Some(completed) = extract_completed_from_sse(sse) {
            self.store_completed(model, session, &completed);
        }
    }

    pub fn clear(&self, model: &str, session: Option<&str>) {
        let Some(session) = nonempty(session) else {
            return;
        };
        let model = model.trim();
        if model.is_empty() {
            return;
        }
        if let Ok(mut guard) = self.entries.lock() {
            guard.remove(&cache_key(model, session));
        }
    }
}

pub fn is_reasoning_decode_failure(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    DECODE_MARKERS.iter().any(|marker| lower.contains(marker))
}

/// Drop `reasoning` input items that carry encrypted_content. Returns whether the body changed.
pub fn strip_encrypted_reasoning(body: &mut Value) -> bool {
    let Some(Value::Array(input)) = body.get_mut("input") else {
        return false;
    };
    let before = input.len();
    input.retain(|item| !is_encrypted_reasoning_item(item));
    before != input.len()
}

fn inject_reasoning_items(body: &mut Value, items: &[Value]) {
    let Some(object) = body.as_object_mut() else {
        return;
    };
    ensure_input_array(object);
    let Some(Value::Array(input)) = object.get_mut("input") else {
        return;
    };
    if input.iter().any(is_encrypted_reasoning_item) {
        return;
    }
    let mut merged = Vec::with_capacity(items.len() + input.len());
    merged.extend(items.iter().cloned());
    merged.append(input);
    *input = merged;
}

fn ensure_input_array(object: &mut Map<String, Value>) {
    match object.get("input") {
        Some(Value::Array(_)) => {}
        Some(Value::String(text)) => {
            let wrapped = json!([{
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": text }]
            }]);
            object.insert("input".to_owned(), wrapped);
        }
        _ => {
            object.insert("input".to_owned(), Value::Array(Vec::new()));
        }
    }
}

fn extract_reasoning_items(completed: &Value) -> Vec<Value> {
    let output = completed.get("output").or_else(|| {
        completed
            .get("response")
            .and_then(|value| value.get("output"))
    });
    let Some(Value::Array(items)) = output else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|item| is_encrypted_reasoning_item(item))
        .cloned()
        .collect()
}

fn extract_completed_from_sse(sse: &str) -> Option<Value> {
    if sse.len() > MAX_CAPTURE_BYTES {
        return None;
    }
    let mut last: Option<Value> = None;
    for line in sse.lines() {
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        let event_type = value.get("type").and_then(Value::as_str).unwrap_or("");
        if event_type == "response.completed" {
            last = Some(value.get("response").cloned().unwrap_or(value));
        }
    }
    last
}

fn is_encrypted_reasoning_item(item: &Value) -> bool {
    item.get("type").and_then(Value::as_str) == Some("reasoning")
        && item
            .get("encrypted_content")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty())
}

fn has_previous_response_id(body: &Value) -> bool {
    body.get("previous_response_id")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
}

fn cache_key(model: &str, session: &str) -> String {
    format!("{model}\0{session}")
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
