use axum::http::{HeaderMap, HeaderName, HeaderValue};
use serde_json::json;
use uuid::Uuid;

use super::*;

fn header_map(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in pairs {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes()).expect("header name"),
            HeaderValue::from_str(value).expect("header value"),
        );
    }
    headers
}

fn header_str<'a>(headers: &'a reqwest::header::HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn built_headers(identity: Option<&GrokCliRequestIdentity>) -> reqwest::header::HeaderMap {
    apply_grok_cli_identity_with(
        reqwest::Client::new().get(GROK_CLI_PROXY_BASE_URL),
        identity,
    )
    .build()
    .expect("identity request")
    .headers()
    .clone()
}

#[test]
fn identity_headers_include_token_auth_version_identifier_mode() {
    let pairs = grok_cli_identity_header_pairs();
    let get = |name: &str| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(get("x-xai-token-auth"), Some(GROK_CLI_TOKEN_AUTH));
    assert_eq!(get("x-grok-client-version"), Some(GROK_CLI_VERSION));
    assert_eq!(get("x-grok-client-identifier"), Some(GROK_CLI_IDENTIFIER));
    assert_eq!(get("x-grok-client-mode"), Some(GROK_CLI_MODE));
    assert_eq!(get("x-authenticateresponse"), Some("authenticate-response"));
    assert_eq!(
        get("User-Agent").as_deref(),
        Some(grok_cli_user_agent().as_str())
    );

    let headers = apply_grok_cli_identity(reqwest::Client::new().get(GROK_CLI_PROXY_BASE_URL))
        .build()
        .expect("identity request")
        .headers()
        .clone();
    assert_eq!(
        header_str(&headers, "x-xai-token-auth"),
        Some(GROK_CLI_TOKEN_AUTH)
    );
    assert_eq!(
        header_str(&headers, "x-grok-client-version"),
        Some(GROK_CLI_VERSION)
    );
    assert_eq!(
        header_str(&headers, "x-grok-client-identifier"),
        Some(GROK_CLI_IDENTIFIER)
    );
    assert_eq!(
        header_str(&headers, "x-grok-client-mode"),
        Some(GROK_CLI_MODE)
    );
    assert_eq!(
        header_str(&headers, reqwest::header::USER_AGENT.as_str()),
        Some(grok_cli_user_agent().as_str())
    );
    assert!(headers.get("x-grok-turn-idx").is_none());
    assert!(headers.get("x-grok-req-id").is_none());
}

#[test]
fn grok_session_id_without_seed_is_none() {
    assert_eq!(grok_session_id(""), None);
    assert_eq!(grok_session_id("   "), None);
}

#[test]
fn grok_session_id_keeps_canonical_uuid() {
    let seed = "550E8400-E29B-41D4-A716-446655440000";
    assert_eq!(
        grok_session_id(seed).as_deref(),
        Some("550e8400-e29b-41d4-a716-446655440000")
    );
}

#[test]
fn grok_session_id_hashes_non_uuid_seed_stably() {
    let seed = "codex:window:abc";
    let expected = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("agenthub:grok-session:{seed}").as_bytes(),
    )
    .to_string();
    assert_eq!(grok_session_id(seed).as_deref(), Some(expected.as_str()));
    assert_eq!(grok_session_id(seed), grok_session_id(seed));
}

#[test]
fn grok_session_id_mixes_account_and_keeps_single_account_hash() {
    let seed = "codex:window:abc";
    let without = grok_session_id(seed).expect("base");
    let with_a = grok_session_id_for_account(seed, Some("acc-a")).expect("a");
    let with_b = grok_session_id_for_account(seed, Some("acc-b")).expect("b");
    assert_eq!(
        grok_session_id_for_account(seed, None).as_deref(),
        Some(without.as_str())
    );
    assert_ne!(without, with_a);
    assert_ne!(with_a, with_b);
}

#[test]
fn claude_session_header_maps_to_claude_agent_seed() {
    let body = json!({});
    let headers = header_map(&[("X-Claude-Code-Session-Id", "sess-1")]);
    assert_eq!(
        extract_prompt_cache_seed(&headers, &body).as_deref(),
        Some("claude:sess-1:agent:main")
    );

    let headers = header_map(&[
        ("X-Claude-Code-Session-Id", "sess-1"),
        ("X-Claude-Code-Agent-Id", "reviewer"),
    ]);
    assert_eq!(
        extract_prompt_cache_seed(&headers, &body).as_deref(),
        Some("claude:sess-1:agent:reviewer")
    );
}

#[test]
fn claude_title_request_returns_none() {
    let headers = header_map(&[("X-Claude-Code-Session-Id", "sess-title")]);
    let body = json!({
        "system": "Generate a concise title for this coding session."
    });
    assert_eq!(extract_prompt_cache_seed(&headers, &body), None);
}

#[test]
fn claude_title_request_in_user_message_returns_none() {
    let headers = header_map(&[("X-Claude-Code-Session-Id", "sess-title")]);
    let messages_body = json!({
        "messages": [{
            "role": "user",
            "content": "Generate a concise title for this coding session."
        }]
    });
    assert_eq!(extract_prompt_cache_seed(&headers, &messages_body), None);

    let input_body = json!({
        "input": [{
            "role": "user",
            "text": "Generate a concise title for this coding session."
        }]
    });
    assert_eq!(extract_prompt_cache_seed(&headers, &input_body), None);
}

#[test]
fn claude_ordinary_user_message_keeps_session_seed() {
    let headers = header_map(&[("X-Claude-Code-Session-Id", "sess-1")]);
    let body = json!({
        "messages": [{
            "role": "user",
            "content": "How do I add a login page?"
        }]
    });
    assert_eq!(
        extract_prompt_cache_seed(&headers, &body).as_deref(),
        Some("claude:sess-1:agent:main")
    );
}

#[test]
fn codex_turn_metadata_prompt_cache_key() {
    let headers = header_map(&[(
        "X-Codex-Turn-Metadata",
        r#"{"prompt_cache_key":"pc-from-header","window_id":"win-1"}"#,
    )]);
    assert_eq!(
        extract_prompt_cache_seed(&headers, &json!({})).as_deref(),
        Some("pc-from-header")
    );
}

#[test]
fn local_shell_upgrades_to_shell() {
    let mut body = json!({
        "tools": [
            { "type": "local_shell" },
            { "type": "web_search" }
        ]
    });
    normalize_grok_build_tools(&mut body);
    assert_eq!(
        body["tools"][0],
        json!({ "type": "shell", "environment": { "type": "local" } })
    );
    assert_eq!(body["tools"][1]["type"], "web_search");

    let mut body = json!({
        "tools": [
            { "type": "shell", "environment": { "type": "local" } },
            { "type": "local_shell" }
        ]
    });
    normalize_grok_build_tools(&mut body);
    assert_eq!(body["tools"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["tools"][0]["type"], "shell");
}

#[test]
fn apply_patch_becomes_function() {
    let mut body = json!({
        "tools": [
            { "type": "apply_patch" },
            { "type": "x_search" }
        ]
    });
    normalize_grok_build_tools(&mut body);
    let tool = &body["tools"][0];
    assert_eq!(tool["type"], "function");
    assert_eq!(tool["name"], "apply_patch");
    assert_eq!(tool["strict"], true);
    let description = tool["description"].as_str().unwrap_or("");
    assert!(description.contains("create_file"));
    assert!(description.contains("update_file"));
    assert!(description.contains("delete_file"));
    assert!(description.contains("empty string"));
    assert_eq!(tool["parameters"]["type"], "object");
    assert_eq!(tool["parameters"]["required"], json!(["operation"]));
    assert_eq!(tool["parameters"]["additionalProperties"], false);
    assert_eq!(
        tool["parameters"]["properties"]["operation"]["properties"]["type"]["enum"],
        json!(["create_file", "update_file", "delete_file"])
    );
    assert_eq!(
        tool["parameters"]["properties"]["operation"]["required"],
        json!(["type", "path", "diff"])
    );
    assert_eq!(
        tool["parameters"]["properties"]["operation"]["properties"]["path"]["minLength"],
        1
    );
    assert_eq!(body["tools"][1]["type"], "x_search");
}

#[test]
fn existing_prompt_cache_key_is_not_overwritten() {
    let mut body = json!({ "prompt_cache_key": "keep-me" });
    inject_prompt_cache_key(&mut body, Some("new-seed"));
    assert_eq!(body["prompt_cache_key"], "keep-me");

    let mut empty = json!({ "model": "grok-4.5" });
    inject_prompt_cache_key(&mut empty, Some("new-seed"));
    assert_eq!(empty["prompt_cache_key"], "new-seed");
}

#[test]
fn request_identity_adds_grok2api_headers_without_turn_idx() {
    let identity = GrokCliRequestIdentity {
        request_id: "req-1".into(),
        session_id: Some("sess-uuid".into()),
        model_override: Some("grok-4.5".into()),
    };
    let headers = built_headers(Some(&identity));
    assert_eq!(
        header_str(&headers, "x-authenticateresponse"),
        Some("authenticate-response")
    );
    assert!(header_str(&headers, "x-grok-agent-id").is_some_and(|id| Uuid::parse_str(id).is_ok()));
    assert_eq!(header_str(&headers, "x-grok-req-id"), Some("req-1"));
    assert_eq!(header_str(&headers, "x-grok-session-id"), Some("sess-uuid"));
    assert_eq!(header_str(&headers, "x-grok-conv-id"), Some("sess-uuid"));
    assert_eq!(
        header_str(&headers, "x-grok-model-override"),
        Some("grok-4.5")
    );
    let traceparent = header_str(&headers, "traceparent").unwrap_or("");
    assert!(
        traceparent.starts_with("00-")
            && traceparent.ends_with("-01")
            && traceparent.len() == "00-".len() + 32 + 1 + 16 + 3,
        "traceparent={traceparent}"
    );
    assert!(headers.get("x-grok-turn-idx").is_none());
}

#[test]
fn normalize_skips_body_without_tools() {
    let mut body = json!({ "model": "grok-4.5" });
    normalize_grok_build_tools(&mut body);
    assert_eq!(body, json!({ "model": "grok-4.5" }));
}

#[test]
fn reasoning_replay_injects_and_skips_when_already_present() {
    let replay = GrokReasoningReplay::new();
    let completed = json!({
        "output": [
            { "type": "reasoning", "encrypted_content": "enc-1" },
            { "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": "hi" }] }
        ]
    });
    replay.store_completed("grok-4.5", Some("sess"), &completed);

    let mut body = json!({ "model": "grok-4.5", "input": "hello" });
    replay.apply(&mut body, "grok-4.5", Some("sess"));
    let input = body["input"].as_array().expect("array");
    assert_eq!(input[0]["type"], "reasoning");
    assert_eq!(input[0]["encrypted_content"], "enc-1");
    assert_eq!(input[1]["role"], "user");

    let mut already = json!({
        "model": "grok-4.5",
        "input": [{ "type": "reasoning", "encrypted_content": "enc-1" }]
    });
    replay.apply(&mut already, "grok-4.5", Some("sess"));
    assert_eq!(already["input"].as_array().map(Vec::len), Some(1));

    assert!(is_reasoning_decode_failure(
        r#"{"error":{"message":"could not decrypt the provided encrypted_content"}}"#
    ));
    let mut strip = json!({
        "input": [
            { "type": "reasoning", "encrypted_content": "enc-1" },
            { "type": "message", "role": "user" }
        ]
    });
    assert!(strip_encrypted_reasoning(&mut strip));
    assert_eq!(strip["input"].as_array().map(Vec::len), Some(1));
    assert_eq!(strip["input"][0]["role"], "user");
}
