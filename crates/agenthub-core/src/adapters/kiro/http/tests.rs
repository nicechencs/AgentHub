use serde_json::json;

use super::client::{build_chat_body, parse_list_models_response};
use super::creds::{KiroAuthKind, KiroHttpCreds};
use super::eventstream::{collect_assistant_text, parse_event_stream};

fn frame(headers: &[(&str, &str)], payload: &[u8]) -> Vec<u8> {
    let mut header_bytes = Vec::new();
    for (name, value) in headers {
        header_bytes.push(name.len() as u8);
        header_bytes.extend_from_slice(name.as_bytes());
        header_bytes.push(7);
        let vb = value.as_bytes();
        header_bytes.extend_from_slice(&(vb.len() as u16).to_be_bytes());
        header_bytes.extend_from_slice(vb);
    }
    let headers_len = header_bytes.len() as u32;
    let total_len = 12 + header_bytes.len() + payload.len() + 4;
    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(&(total_len as u32).to_be_bytes());
    out.extend_from_slice(&headers_len.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&header_bytes);
    out.extend_from_slice(payload);
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

#[test]
fn event_stream_collects_text() {
    let mut bytes = frame(
        &[(":event-type", "assistantResponseEvent")],
        br#"{"content":"hello"}"#,
    );
    bytes.extend(frame(
        &[(":event-type", "assistantResponseEvent")],
        br#"{"content":" world"}"#,
    ));
    let events = parse_event_stream(&bytes);
    assert_eq!(events.len(), 2);
    let (text, _) = collect_assistant_text(&bytes);
    assert_eq!(text, "hello world");
}

#[test]
fn request_shaping_api_key_origin() {
    let body = build_chat_body("ping", "auto", "AI_EDITOR", None, None);
    assert!(body.get("profileArn").is_none());
    assert_eq!(
        body.pointer("/conversationState/currentMessage/userInputMessage/origin")
            .and_then(|v| v.as_str()),
        Some("AI_EDITOR")
    );
}

#[test]
fn list_models_parser_accepts_cli_shaped_json() {
    let v = json!({
        "models": [
            {"model_id": "auto"},
            {"modelId": "claude-haiku-4.5"}
        ],
        "default_model": "auto"
    });
    let parsed = parse_list_models_response(&v);
    assert_eq!(parsed.default_model.as_deref(), Some("auto"));
    assert!(parsed.models.iter().any(|m| m == "claude-haiku-4.5"));
}

#[test]
fn api_key_creds_surface_tokentype() {
    let creds = KiroHttpCreds {
        auth_kind: KiroAuthKind::ApiKey,
        access_token: "ksk_x".into(),
        refresh_token: None,
        expires_at: None,
        region: "us-east-1".into(),
        profile_arn: None,
        client_id: None,
        client_secret: None,
        origin: "AI_EDITOR".into(),
        sqlite_token_key: None,
        source: "env".into(),
    };
    assert_eq!(creds.token_type_header(), Some("API_KEY"));
}

#[test]
#[ignore = "live network + Builder ID login on this machine"]
fn live_list_models_and_chat_turn() {
    let listed = super::list_models_http().expect("list_models_http");
    assert!(!listed.models.is_empty(), "expected models from Kiro HTTP");
    let turn = super::chat_turn_http("Reply with exactly: pong", Some("claude-haiku-4.5"), None)
        .expect("chat_turn_http");
    assert!(
        turn.text.to_ascii_lowercase().contains("pong"),
        "unexpected text: {}",
        turn.text
    );
}

#[test]
fn try_http_skips_when_native_resume_set() {
    use crate::models::RunOptions;
    let mut opts = RunOptions::default();
    opts.native_session_id = Some("resume-me".into());
    assert!(
        super::try_http_run_result("hi", &opts).is_none(),
        "CLI --resume-id must stay on CLI path"
    );
}

#[test]
fn http_native_session_helpers_exported() {
    let encoded = super::http_native_session_id("cid-1");
    assert_eq!(encoded, "kiro-http:cid-1");
    assert_eq!(super::parse_http_native_session_id(&encoded), Some("cid-1"));
    assert_eq!(super::parse_http_native_session_id("cli-session"), None);
}

#[test]
#[ignore = "live network + Builder ID login on this machine"]
fn live_http_multi_turn_reuses_conversation_id() {
    let turn1 = super::chat_turn_http(
        "Reply with exactly: alpha",
        Some("claude-haiku-4.5"),
        None,
    )
    .expect("turn1");
    let cid = turn1
        .conversation_id
        .as_deref()
        .expect("turn1 conversationId");
    assert!(
        turn1.text.to_ascii_lowercase().contains("alpha"),
        "unexpected turn1: {}",
        turn1.text
    );
    let turn2 = super::chat_turn_http(
        "Reply with exactly: beta",
        Some("claude-haiku-4.5"),
        Some(cid),
    )
    .expect("turn2");
    assert!(
        turn2.text.to_ascii_lowercase().contains("beta"),
        "unexpected turn2: {}",
        turn2.text
    );
    if let Some(cid2) = turn2.conversation_id.as_deref() {
        assert_eq!(cid2, cid, "second turn should keep conversationId");
    }
}
