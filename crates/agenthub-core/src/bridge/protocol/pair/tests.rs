use serde_json::Value;

use crate::models::{feature_flag_enabled, FEATURE_CODEX_INGRESS_GROK_UPSTREAM};

use super::super::fixture_loader::fixture;

use super::{
    adapt_codex_request_for_grok_upstream, adapt_grok_request_for_codex_upstream,
    dialect_compatibility, explicit_transparent_relay, is_stateful_continuation,
    sanitize_pair_response, sanitize_pair_sse_event, sanitizer_allows_transparent,
    DialectCompatibility, PairDirection, ResponsesDialect,
};

fn assert_no_system_or_developer_items(body: &Value) {
    let Some(items) = body.get("input").and_then(Value::as_array) else {
        return;
    };
    for item in items {
        let role = item.get("role").and_then(Value::as_str);
        assert_ne!(role, Some("system"), "{item}");
        assert_ne!(role, Some("developer"), "{item}");
    }
}

#[test]
fn matching_responses_surface_is_not_transparent() {
    assert!(!explicit_transparent_relay(
        ResponsesDialect::Codex,
        ResponsesDialect::Grok
    ));
    assert!(!explicit_transparent_relay(
        ResponsesDialect::Grok,
        ResponsesDialect::Codex
    ));
    assert!(!explicit_transparent_relay(
        ResponsesDialect::Codex,
        ResponsesDialect::Codex
    ));
    assert_eq!(
        dialect_compatibility(ResponsesDialect::Codex, ResponsesDialect::Grok),
        DialectCompatibility::Adapted
    );
    assert!(!sanitizer_allows_transparent(&fixture(
        "pair_codex_to_grok_response"
    )));
}

#[test]
fn same_dialect_does_not_select_a_pair_adapter() {
    assert_eq!(
        PairDirection::from_dialects(ResponsesDialect::Codex, ResponsesDialect::Codex),
        None
    );
    assert_eq!(
        PairDirection::from_dialects(ResponsesDialect::Grok, ResponsesDialect::Grok),
        None
    );
    assert_eq!(
        PairDirection::from_dialects(ResponsesDialect::Codex, ResponsesDialect::Grok),
        Some(PairDirection::CodexIngressGrokUpstream)
    );
    assert_eq!(
        PairDirection::from_dialects(ResponsesDialect::Grok, ResponsesDialect::Codex),
        Some(PairDirection::GrokIngressCodexUpstream)
    );
}

#[test]
fn flag_parser_matches_route_pool_v2() {
    assert!(!feature_flag_enabled(None));
    assert!(!feature_flag_enabled(Some("off")));
    assert!(feature_flag_enabled(Some("on")));
    assert!(feature_flag_enabled(Some("YES")));
    assert_eq!(
        FEATURE_CODEX_INGRESS_GROK_UPSTREAM,
        "feature.codex_ingress_grok_upstream"
    );
}

#[test]
fn codex_to_grok_request_strips_store_system_and_allowlist_rejects() {
    let mut body = fixture("pair_codex_to_grok_request");
    adapt_codex_request_for_grok_upstream(&mut body);
    assert!(body.get("store").is_none(), "{body}");
    assert!(body.get("metadata").is_none(), "{body}");
    assert!(body.get("max_output_tokens").is_none(), "{body}");
    assert_no_system_or_developer_items(&body);
    let instructions = body["instructions"].as_str().expect("instructions");
    assert!(instructions.contains("Be brief."), "{instructions}");
    assert!(
        instructions.contains("You are a coding agent."),
        "{instructions}"
    );
    assert_eq!(body["temperature"], 0.2);
    assert_eq!(body["tools"][0]["type"], "local_shell");
    assert_eq!(body["tools"][1]["name"], "lookup");
    assert_eq!(body["input"][0]["content"][0]["text"], "hello");
}

#[test]
fn grok_to_codex_request_uses_official_allowlist() {
    let mut body = fixture("pair_grok_to_codex_request");
    adapt_grok_request_for_codex_upstream(&mut body, "gpt-5.4", Some("gpt-5.4"));
    assert_eq!(body["store"], false);
    assert_eq!(body["model"], "gpt-5.4");
    assert!(body.get("prompt_cache_key").is_none(), "{body}");
    assert!(body.get("previous_response_id").is_none(), "{body}");
    assert!(body.get("reasoning").is_none(), "{body}");
    assert!(body.get("include").is_none(), "{body}");
    assert!(body.get("metadata").is_none(), "{body}");
    assert!(body.get("max_output_tokens").is_none(), "{body}");
    assert_no_system_or_developer_items(&body);
    assert_eq!(body["tools"][0]["name"], "lookup");
}

#[test]
fn grok_response_does_not_leak_identity_to_codex() {
    let mut body = fixture("pair_codex_to_grok_response");
    assert!(sanitize_pair_response(
        PairDirection::CodexIngressGrokUpstream,
        &mut body
    ));
    assert!(body.get("prompt_cache_key").is_none(), "{body}");
    assert!(body.get("session_id").is_none(), "{body}");
    assert!(body.get("grok_session_id").is_none(), "{body}");
    assert!(body.get("x_grok_req_id").is_none(), "{body}");
    assert!(body.get("conversation_id").is_none(), "{body}");
    assert_eq!(body["id"], "resp_grok_pair");
    assert_eq!(body["output"][0]["encrypted_content"], "enc-blob");
    assert_eq!(body["output"][1]["content"][0]["text"], "hello from grok");
    assert_eq!(body["usage"]["total_tokens"], 10);
    assert_eq!(body["usage"]["reasoning_tokens"], 2);
}

#[test]
fn codex_response_does_not_leak_store_to_grok() {
    let mut body = fixture("pair_grok_to_codex_response");
    assert!(sanitize_pair_response(
        PairDirection::GrokIngressCodexUpstream,
        &mut body
    ));
    assert!(body.get("store").is_none(), "{body}");
    assert!(body.get("service_tier").is_none(), "{body}");
    assert!(body.get("metadata").is_none(), "{body}");
    assert_eq!(body["output"][0]["call_id"], "call_lookup");
    assert_eq!(body["output"][0]["arguments"], "{\"q\":\"weather\"}");
    assert_eq!(body["usage"]["total_tokens"], 12);
}

#[test]
fn grok_sse_strips_session_fields_and_keeps_text_delta() {
    let mut events = fixture("pair_codex_to_grok_sse");
    for event in events.as_array_mut().expect("sse array") {
        assert!(sanitize_pair_sse_event(
            PairDirection::CodexIngressGrokUpstream,
            event
        ));
        if let Some(response) = event.get("response") {
            assert!(response.get("prompt_cache_key").is_none(), "{response}");
            assert!(response.get("session_id").is_none(), "{response}");
            assert!(response.get("x_grok_req_id").is_none(), "{response}");
        }
    }
    assert_eq!(events[1]["delta"], "hi");
    assert_eq!(events[2]["type"], "response.completed");
    assert_eq!(
        events[2]["response"]["output"][0]["content"][0]["text"],
        "hi"
    );
}

#[test]
fn codex_sse_strips_store_and_keeps_tool_and_error_events() {
    let mut events = fixture("pair_grok_to_codex_sse");
    for event in events.as_array_mut().expect("sse array") {
        assert!(sanitize_pair_sse_event(
            PairDirection::GrokIngressCodexUpstream,
            event
        ));
        if let Some(response) = event.get("response") {
            assert!(response.get("store").is_none(), "{response}");
            assert!(response.get("service_tier").is_none(), "{response}");
        }
        assert!(event.get("store").is_none(), "{event}");
        assert!(event.get("metadata").is_none(), "{event}");
    }
    assert_eq!(events[1]["item"]["call_id"], "call_lookup");
    assert_eq!(events[2]["arguments"], "{\"q\":\"weather\"}");
    assert_eq!(events[3]["type"], "error");
    assert_eq!(events[3]["code"], "upstream_error");
}

#[test]
fn parallel_tool_history_survives_codex_to_grok_request_adapt() {
    let mut body = fixture("pair_parallel_tools");
    adapt_codex_request_for_grok_upstream(&mut body);
    let input = body["input"].as_array().expect("input");
    assert_eq!(input.len(), 5);
    assert_eq!(input[1]["call_id"], "call_weather");
    assert_eq!(input[2]["call_id"], "call_calendar");
    assert_eq!(input[3]["type"], "function_call_output");
    assert_eq!(input[4]["output"], "No meetings");
    assert_eq!(body["tools"].as_array().expect("tools").len(), 2);
}

#[test]
fn error_event_drops_provider_identity_fields() {
    let mut event = fixture("pair_error_event");
    assert!(sanitize_pair_sse_event(
        PairDirection::CodexIngressGrokUpstream,
        &mut event
    ));
    assert_eq!(event["type"], "error");
    assert_eq!(event["code"], "upstream_error");
    assert!(event.get("prompt_cache_key").is_none(), "{event}");
    assert!(event.get("session_id").is_none(), "{event}");
}

#[test]
fn previous_response_id_and_prompt_cache_are_stateful() {
    assert!(is_stateful_continuation(&fixture(
        "pair_grok_to_codex_request"
    )));
    assert!(!is_stateful_continuation(&fixture(
        "pair_codex_to_grok_request"
    )));
}
