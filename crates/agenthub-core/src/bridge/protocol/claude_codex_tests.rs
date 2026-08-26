//! Claude subscription → Codex kernel fixtures.
//!
//! Same conversion as Anthropic API Key → Codex: Responses downstream ↔ IR ↔ Messages upstream.

use serde_json::Value;

use crate::bridge::types::{IrEvent, StopReason};

use super::{
    anthropic_messages::{
        anthropic_message_to_ir, to_anthropic_messages_request,
        translate_responses_to_anthropic_request, AnthropicStreamToIr,
    },
    chat::sse_frame,
    fixture_loader::fixture,
    responses::{encode_responses_from_ir, parse_responses_request, IrToResponsesSse},
};

fn assert_codex_completed_usage(usage: &Value) {
    #[derive(serde::Deserialize)]
    struct CodexCompletedUsage {
        input_tokens: i64,
        output_tokens: i64,
        total_tokens: i64,
        reasoning_tokens: i64,
        #[serde(default)]
        output_tokens_details: Option<CodexOutputDetails>,
    }
    #[derive(serde::Deserialize)]
    struct CodexOutputDetails {
        reasoning_tokens: i64,
    }
    let parsed: CodexCompletedUsage = serde_json::from_value(usage.clone())
        .expect("Codex ResponseCompleted usage must include reasoning_tokens");
    assert_eq!(
        parsed
            .output_tokens_details
            .as_ref()
            .map(|details| details.reasoning_tokens),
        Some(parsed.reasoning_tokens)
    );
}

#[test]
fn claude_codex_text_and_unicode_maps_to_anthropic_messages() {
    let request = fixture("claude_codex_responses_text");
    let (bridge, anthropic) =
        translate_responses_to_anthropic_request(&request).expect("translate");
    let via_helper = to_anthropic_messages_request(&bridge);
    assert_eq!(anthropic, via_helper);

    assert_eq!(bridge.model, "claude-sonnet-4-20250514");
    assert_eq!(anthropic["model"], "claude-sonnet-4-20250514");
    assert_eq!(anthropic["system"], "Answer precisely.");
    assert_eq!(anthropic["max_tokens"], 512);
    assert_eq!(anthropic["temperature"], 0.2);
    assert_eq!(anthropic["messages"][0]["role"], "user");
    assert_eq!(
        anthropic["messages"][0]["content"][0]["text"],
        "Hello, 你好世界"
    );
    assert_eq!(anthropic["tools"][0]["name"], "weather");
    assert_eq!(
        anthropic["tools"][0]["input_schema"]["properties"]["city"]["type"],
        "string"
    );
    assert_eq!(anthropic["tool_choice"]["type"], "auto");
}

#[test]
fn claude_codex_multiturn_preserves_assistant_history() {
    let (bridge, anthropic) =
        translate_responses_to_anthropic_request(&fixture("claude_codex_responses_multiturn"))
            .expect("translate");
    assert_eq!(bridge.input.len(), 3);

    let messages = anthropic["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 3, "multi-turn history must not drop turns");
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"][0]["text"], "你好");
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["content"][0]["type"], "text");
    assert_eq!(messages[1]["content"][0]["text"], "已收到你好。");
    assert_eq!(messages[2]["role"], "user");
    assert_eq!(messages[2]["content"][0]["text"], "世界");
}

#[test]
fn claude_codex_tool_call_and_result_preserve_ids() {
    let request = parse_responses_request(&fixture("claude_codex_responses_tools")).expect("parse");
    let anthropic = to_anthropic_messages_request(&request);
    let messages = anthropic["messages"].as_array().expect("messages");

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(
        messages[0]["content"][0]["text"],
        "Check weather and calendar. 你好"
    );

    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["content"][0]["type"], "tool_use");
    assert_eq!(messages[1]["content"][0]["id"], "call_weather");
    assert_eq!(messages[1]["content"][0]["name"], "weather");
    assert_eq!(messages[1]["content"][1]["id"], "call_calendar");
    assert_eq!(messages[1]["content"][1]["name"], "calendar");

    assert_eq!(messages[2]["role"], "user");
    assert_eq!(messages[2]["content"][0]["type"], "tool_result");
    assert_eq!(messages[2]["content"][0]["tool_use_id"], "call_weather");
    assert_eq!(messages[2]["content"][0]["content"], "Sunny 世界");
    assert_eq!(messages[2]["content"][1]["tool_use_id"], "call_calendar");
    assert_eq!(messages[2]["content"][1]["content"], "No meetings");
}

#[test]
fn claude_codex_usage_and_end_turn_become_responses_completed() {
    let ir = anthropic_message_to_ir(&fixture("claude_codex_anthropic_text_usage")).expect("to ir");
    assert!(matches!(ir[0], IrEvent::MessageStart { .. }));
    assert_eq!(
        ir.iter()
            .find_map(|event| match event {
                IrEvent::TextDelta { text } => Some(text.as_str()),
                _ => None,
            })
            .expect("text"),
        "你好，世界。"
    );
    assert!(matches!(
        ir.iter()
            .find(|event| matches!(event, IrEvent::Usage { .. })),
        Some(IrEvent::Usage {
            input_tokens: 8,
            output_tokens: 4,
            cached_input_tokens: Some(2)
        })
    ));
    assert!(matches!(
        ir.last(),
        Some(IrEvent::MessageEnd {
            stop_reason: StopReason::Stop
        })
    ));

    let response = encode_responses_from_ir(&ir, Some("resp_claude_codex")).expect("responses");
    assert_eq!(response["object"], "response");
    assert_eq!(response["status"], "completed");
    assert_eq!(response["output"][0]["content"][0]["text"], "你好，世界。");
    assert_eq!(response["usage"]["input_tokens"], 8);
    assert_eq!(response["usage"]["output_tokens"], 4);
    assert_eq!(response["usage"]["total_tokens"], 12);
    assert_eq!(
        response["usage"]["input_tokens_details"]["cached_tokens"],
        2
    );
    assert_codex_completed_usage(&response["usage"]);
}

#[test]
fn claude_codex_max_tokens_stop_becomes_responses_incomplete() {
    let ir =
        anthropic_message_to_ir(&fixture("claude_codex_anthropic_stop_max_tokens")).expect("ir");
    assert!(matches!(
        ir.last(),
        Some(IrEvent::MessageEnd {
            stop_reason: StopReason::Length
        })
    ));
    let response = encode_responses_from_ir(&ir, Some("resp_len")).expect("responses");
    assert_eq!(response["status"], "incomplete");
    assert_eq!(
        response["incomplete_details"]["reason"],
        "max_output_tokens"
    );
}

#[test]
fn claude_codex_error_is_generic_and_never_leaks_secret() {
    let planted = "sk-ant-claude-codex-secret";
    let ir = anthropic_message_to_ir(&fixture("claude_codex_anthropic_error")).expect("error ir");
    assert_eq!(ir.len(), 1);
    match &ir[0] {
        IrEvent::Error { code, message, .. } => {
            assert_eq!(code, "upstream_error");
            assert_eq!(message, "The upstream model provider returned an error.");
            assert!(!message.contains(planted));
            assert!(!message.contains("private input"));
        }
        other => panic!("expected Error, got {other:?}"),
    }

    let error = encode_responses_from_ir(&ir, Some("resp_err")).expect_err("error not encoded");
    assert_eq!(error.code, "upstream_error");
    assert_eq!(
        error.message,
        "The upstream model provider returned an error."
    );
    assert!(!error.message.contains(planted));
}

#[test]
fn claude_codex_sse_split_reassembles_unicode_deltas() {
    let chunks = fixture("claude_codex_anthropic_sse_split")
        .as_array()
        .cloned()
        .expect("array");
    let mut translator = AnthropicStreamToIr::new();
    let mut ir = chunks
        .iter()
        .flat_map(|chunk| translator.push_event(chunk).expect("event"))
        .collect::<Vec<_>>();
    assert!(translator.completed());
    ir.extend(translator.finish());

    let reassembled = ir
        .iter()
        .filter_map(|event| match event {
            IrEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(reassembled, "你好{\"k\":\"世界\"}");

    let mut encoder = IrToResponsesSse::new("resp_claude_codex_stream", "requested-model");
    let events = ir
        .iter()
        .flat_map(|event| encoder.push_event(event).expect("encode"))
        .collect::<Vec<_>>();
    let names = events
        .iter()
        .map(|event| event.event_name())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "response.created",
            "response.in_progress",
            "response.output_item.added",
            "response.content_part.added",
            "response.output_text.delta",
            "response.output_text.delta",
            "response.output_text.delta",
            "response.output_text.done",
            "response.content_part.done",
            "response.output_item.done",
            "response.completed",
        ]
    );

    let delta_text = events
        .iter()
        .filter(|event| event.event_name() == "response.output_text.delta")
        .map(|event| {
            event.data()["delta"]
                .as_str()
                .expect("string delta")
                .to_owned()
        })
        .collect::<String>();
    assert_eq!(delta_text, "你好{\"k\":\"世界\"}");

    let done = events
        .iter()
        .find(|event| event.event_name() == "response.output_text.done")
        .expect("text done");
    assert_eq!(done.data()["text"], "你好{\"k\":\"世界\"}");

    let joined = events.iter().map(sse_frame).collect::<String>();
    assert!(joined.contains("\"delta\":\"你好\""));
    assert!(joined.contains("世界"));
    assert!(joined.contains("\"input_tokens\":5"));
}

#[test]
fn claude_codex_thinking_fails_closed_without_forging_signature() {
    let error =
        anthropic_message_to_ir(&fixture("claude_codex_anthropic_thinking")).expect_err("thinking");
    assert_eq!(error.code, "unsupported_thinking");
    assert!(!error.message.contains("PLANTED_THINKING_SECRET"));
    assert!(!error.message.contains("private chain"));
    assert!(!error.message.contains("你好"));
    assert!(
        !error.message.contains("signature"),
        "kernel must not reconstruct a thinking signature"
    );
}

#[test]
fn claude_codex_unsupported_image_fails_closed() {
    let error =
        translate_responses_to_anthropic_request(&fixture("claude_codex_unsupported_image"))
            .expect_err("image rejected");
    assert_eq!(error.code, "unsupported_image_input");
    assert!(!error.message.contains("example.invalid"));
    assert!(!error.message.contains("claude-codex.png"));
}
