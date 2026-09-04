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

#[test]
fn continuation_evicts_when_over_cap() {
    let bindings = ContinuationBindings::new();
    for i in 0..CONTINUATION_MAX_ENTRIES {
        bindings.record_response(&json!({ "id": format!("resp_{i}") }), None, "member-a");
    }
    assert_eq!(bindings.len(), CONTINUATION_MAX_ENTRIES);
    let before = bindings.keys();
    bindings.record_response(&json!({ "id": "resp_new" }), None, "member-b");
    assert_eq!(bindings.len(), CONTINUATION_MAX_ENTRIES);
    let after = bindings.keys();
    assert!(after.iter().any(|key| key == "response:resp_new"));
    assert_eq!(before.iter().filter(|key| !after.contains(key)).count(), 1);
}

impl ContinuationBindings {
    fn len(&self) -> usize {
        self.inner.lock().map(|guard| guard.len()).unwrap_or(0)
    }

    fn keys(&self) -> Vec<String> {
        self.inner
            .lock()
            .map(|guard| guard.keys())
            .unwrap_or_default()
    }
}
