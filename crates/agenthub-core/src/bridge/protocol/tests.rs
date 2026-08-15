use serde_json::{json, Value};

use crate::bridge::types::{
    BridgeEvent, EmissionState, IrEvent, RetryClass, RetryGate, StopReason,
};

use super::{
    anthropic_messages::{encode_anthropic_message, encode_anthropic_sse, parse_messages_request},
    chat::{sse_frame, translate_chat_response, ResponsesSseTranslator},
    responses::{
        parse_responses_request, responses_output_to_ir, to_kimi_chat_request,
        to_responses_request, translate_responses_request, ResponsesStreamToIr,
    },
};

fn fixture(name: &str) -> Value {
    let source = match name {
        "responses_text" => include_str!("fixtures/responses_text.json"),
        "unsupported_input" => include_str!("fixtures/unsupported_input.json"),
        "unsupported_web_search" => include_str!("fixtures/unsupported_web_search.json"),
        "chat_text" => include_str!("fixtures/chat_text.json"),
        "chat_tool" => include_str!("fixtures/chat_tool.json"),
        "sse_text_split" => include_str!("fixtures/sse_text_split.json"),
        "sse_text_usage_tail" => include_str!("fixtures/sse_text_usage_tail.json"),
        "sse_length_stop" => include_str!("fixtures/sse_length_stop.json"),
        "sse_content_filter_stop" => include_str!("fixtures/sse_content_filter_stop.json"),
        "sse_tool_call" => include_str!("fixtures/sse_tool_call.json"),
        "responses_parallel_tool_history" => {
            include_str!("fixtures/responses_parallel_tool_history.json")
        }
        "responses_reasoning" => include_str!("fixtures/responses_reasoning.json"),
        "responses_function_output_text" => {
            include_str!("fixtures/responses_function_output_text.json")
        }
        "responses_function_output_unsupported" => {
            include_str!("fixtures/responses_function_output_unsupported.json")
        }
        "usage_stop" => include_str!("fixtures/usage_stop.json"),
        "upstream_error" => include_str!("fixtures/upstream_error.json"),
        "anthropic_messages_text" => include_str!("fixtures/anthropic_messages_text.json"),
        "anthropic_messages_tools" => include_str!("fixtures/anthropic_messages_tools.json"),
        "anthropic_messages_image" => include_str!("fixtures/anthropic_messages_image.json"),
        "responses_upstream_text" => include_str!("fixtures/responses_upstream_text.json"),
        "responses_upstream_tool" => include_str!("fixtures/responses_upstream_tool.json"),
        "responses_upstream_sse_text" => include_str!("fixtures/responses_upstream_sse_text.json"),
        "responses_upstream_sse_tool" => include_str!("fixtures/responses_upstream_sse_tool.json"),
        _ => panic!("unknown fixture"),
    };
    serde_json::from_str(source).expect("fixture is valid JSON")
}

#[test]
fn responses_request_maps_text_tools_options_and_unicode() {
    let request = fixture("responses_text");
    let (bridge, kimi) = translate_responses_request(&request).expect("request translates");

    assert_eq!(bridge.model, "kimi-k2.5");
    assert_eq!(bridge.input[0].content.len(), 1);
    assert_eq!(bridge.passthrough["temperature"], json!(0.2));
    assert_eq!(kimi["messages"][0]["role"], "system");
    assert_eq!(kimi["messages"][1]["content"], "Hello, 世界");
    assert_eq!(kimi["tools"][0]["function"]["name"], "weather");
    assert_eq!(kimi["max_tokens"], 512);
    assert_eq!(kimi["temperature"], json!(0.2));
    assert!(kimi.get("stream_options").is_none());
}

#[test]
fn streaming_request_opt_in_preserves_final_usage_chunk() {
    let mut request = fixture("responses_text");
    request["stream"] = Value::Bool(true);
    let bridge = parse_responses_request(&request).expect("request parses");
    let kimi = to_kimi_chat_request(&bridge);
    assert_eq!(kimi["stream_options"]["include_usage"], true);
}

#[test]
fn reasoning_configuration_fails_closed_without_a_verified_kimi_mapping() {
    let error = parse_responses_request(&fixture("responses_reasoning"))
        .expect_err("reasoning must not be silently dropped");

    assert_eq!(error.code, "unsupported_reasoning");
    assert_eq!(
        error.message,
        "Reasoning configuration is not supported by this bridge."
    );
}

#[test]
fn historical_parallel_function_calls_become_one_assistant_tool_call_message() {
    let request = fixture("responses_parallel_tool_history");
    let (_, kimi) = translate_responses_request(&request).expect("history translates");
    let messages = kimi["messages"].as_array().expect("chat messages");

    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[1]["role"], "assistant");
    assert!(messages[1]["content"].is_null());
    assert_eq!(messages[1]["tool_calls"].as_array().unwrap().len(), 2);
    assert_eq!(messages[1]["tool_calls"][0]["id"], "call_weather");
    assert_eq!(messages[1]["tool_calls"][1]["id"], "call_calendar");
    assert_eq!(messages[2]["role"], "tool");
    assert_eq!(messages[2]["tool_call_id"], "call_weather");
    assert_eq!(messages[3]["tool_call_id"], "call_calendar");
}

#[test]
fn structured_function_call_output_preserves_only_text_content() {
    let (_, kimi) = translate_responses_request(&fixture("responses_function_output_text"))
        .expect("text output translates");
    assert_eq!(kimi["messages"][1]["role"], "tool");
    assert_eq!(kimi["messages"][1]["tool_call_id"], "call_text");
    assert_eq!(kimi["messages"][1]["content"], "first second");
}

#[test]
fn unsupported_or_non_text_function_call_output_fails_closed() {
    let request = fixture("responses_function_output_unsupported");
    for kind in ["input_image", "input_file", "input_audio", "file", "audio"] {
        let mut request = request.clone();
        request["input"][1]["output"][0]["type"] = Value::String(kind.to_owned());
        let error = parse_responses_request(&request).expect_err("media output rejected");
        assert_eq!(error.code, "unsupported_function_output_content", "{kind}");
    }

    let mut request = fixture("responses_function_output_text");
    request["input"][1]["output"] = json!({ "nested": "result" });
    let error = parse_responses_request(&request).expect_err("object output is not stringified");
    assert_eq!(error.code, "invalid_request");
    assert_eq!(
        error.message,
        "Function call output must be a string or an array of text content."
    );
}

#[test]
fn unsupported_multimodal_and_hosted_tools_fail_closed() {
    let error = parse_responses_request(&fixture("unsupported_input")).expect_err("image rejected");
    assert_eq!(error.code, "unsupported_image_input");
    assert!(!error.message.contains("example.invalid"));

    let error = parse_responses_request(&fixture("unsupported_web_search"))
        .expect_err("web search rejected");
    assert_eq!(error.code, "unsupported_web_search");
}

#[test]
fn non_streaming_text_response_has_responses_shape_and_usage() {
    let response = translate_chat_response(&fixture("chat_text"), Some("resp_text"))
        .expect("response translates");

    assert_eq!(response["id"], "resp_text");
    assert_eq!(response["object"], "response");
    assert_eq!(response["status"], "completed");
    assert_eq!(response["output"][0]["content"][0]["text"], "你好，世界。");
    assert_eq!(response["usage"]["input_tokens"], 8);
    assert_eq!(response["usage"]["output_tokens"], 4);
}

#[test]
fn non_streaming_tool_call_becomes_function_call_item() {
    let response = translate_chat_response(&fixture("chat_tool"), Some("resp_tool"))
        .expect("response translates");
    assert_eq!(response["output"][0]["type"], "function_call");
    assert_eq!(response["output"][0]["call_id"], "call_weather_1");
    assert_eq!(response["output"][0]["name"], "weather");
    assert_eq!(response["output"][0]["arguments"], "{\"city\":\"Taipei\"}");
}

#[test]
fn stream_text_split_emits_required_responses_event_sequence() {
    let chunks = fixture("sse_text_usage_tail")
        .as_array()
        .cloned()
        .expect("array fixture");
    let mut translator = ResponsesSseTranslator::new("resp_stream", "requested-model");
    let mut events = chunks
        .iter()
        .flat_map(|chunk| translator.push_chunk(chunk).expect("chunk translates"))
        .collect::<Vec<_>>();
    assert!(
        events
            .iter()
            .all(|event| event.event_name() != "response.completed"),
        "a finish_reason must wait for the upstream [DONE] marker so a trailing usage chunk is retained"
    );
    events.extend(translator.finish());
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
            "response.output_text.done",
            "response.content_part.done",
            "response.output_item.done",
            "response.completed",
        ]
    );
    assert_eq!(
        events.last().expect("completed").data()["response"]["usage"]["total_tokens"],
        9
    );
    assert!(
        translator.finish().is_empty(),
        "completion is idempotent after the explicit terminal [DONE] handling"
    );
    assert_eq!(
        events
            .iter()
            .map(BridgeEvent::sequence_number)
            .collect::<Vec<_>>(),
        (0..events.len() as u64).collect::<Vec<_>>(),
        "Responses stream sequence numbers are monotonically increasing"
    );
}

#[test]
fn stream_tool_call_preserves_index_id_name_and_argument_deltas() {
    let chunks = fixture("sse_tool_call")
        .as_array()
        .cloned()
        .expect("array fixture");
    let mut translator = ResponsesSseTranslator::new("resp_stream_tool", "requested-model");
    let mut events = chunks
        .iter()
        .flat_map(|chunk| translator.push_chunk(chunk).expect("chunk translates"))
        .collect::<Vec<_>>();
    assert!(events
        .iter()
        .all(|event| event.event_name() != "response.completed"));
    events.extend(translator.finish());
    let arguments = events
        .iter()
        .filter(|event| event.event_name() == "response.function_call_arguments.delta")
        .map(|event| {
            event.data()["delta"]
                .as_str()
                .expect("string delta")
                .to_owned()
        })
        .collect::<String>();
    assert_eq!(arguments, "{\"city\":\"Taipei\"}");
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
            "response.function_call_arguments.delta",
            "response.function_call_arguments.delta",
            "response.function_call_arguments.done",
            "response.output_item.done",
            "response.completed",
        ]
    );
    let arguments_done = events
        .iter()
        .find(|event| event.event_name() == "response.function_call_arguments.done")
        .expect("arguments done")
        .data();
    assert_eq!(arguments_done["arguments"], "{\"city\":\"Taipei\"}");
    let completed = events.last().expect("completed").data();
    assert_eq!(completed["response"]["output"][0]["call_id"], "call_9");
    assert_eq!(completed["response"]["output"][0]["name"], "weather");
}

#[test]
fn stream_length_and_content_filter_stop_with_incomplete_terminal_events() {
    for (fixture_name, expected_reason) in [
        ("sse_length_stop", "max_output_tokens"),
        ("sse_content_filter_stop", "content_filter"),
    ] {
        let chunks = fixture(fixture_name)
            .as_array()
            .cloned()
            .expect("array fixture");
        let mut translator = ResponsesSseTranslator::new("resp_incomplete", "requested-model");
        let mut events = chunks
            .iter()
            .flat_map(|chunk| translator.push_chunk(chunk).expect("chunk translates"))
            .collect::<Vec<_>>();
        events.extend(translator.finish());

        assert!(
            events
                .iter()
                .all(|event| event.event_name() != "response.completed"),
            "{fixture_name} must not emit a completed terminal event"
        );
        let terminal = events.last().expect("terminal event");
        assert_eq!(terminal.event_name(), "response.incomplete");
        assert_eq!(terminal.data()["response"]["status"], "incomplete");
        assert_eq!(
            terminal.data()["response"]["incomplete_details"]["reason"],
            expected_reason
        );
        assert_eq!(
            events
                .iter()
                .map(BridgeEvent::sequence_number)
                .collect::<Vec<_>>(),
            (0..events.len() as u64).collect::<Vec<_>>(),
            "{fixture_name} sequence numbers remain monotonic"
        );
    }
}

#[test]
fn usage_and_length_stop_reason_become_incomplete_response() {
    let response = translate_chat_response(&fixture("usage_stop"), Some("resp_length"))
        .expect("response translates");
    assert_eq!(response["status"], "incomplete");
    assert_eq!(
        response["incomplete_details"]["reason"],
        "max_output_tokens"
    );
    assert_eq!(
        response["usage"]["input_tokens_details"]["cached_tokens"],
        3
    );
}

#[test]
fn content_filter_stop_reason_is_incomplete_and_explicit() {
    let response = translate_chat_response(
        &json!({
            "id": "chatcmpl_filter",
            "model": "kimi-k2.5",
            "choices": [{
                "message": {"role": "assistant", "content": ""},
                "finish_reason": "content_filter"
            }],
            "usage": {"prompt_tokens": 2, "completion_tokens": 0, "total_tokens": 2}
        }),
        Some("resp_filter"),
    )
    .expect("response translates");

    assert_eq!(response["status"], "incomplete");
    assert_eq!(response["incomplete_details"]["reason"], "content_filter");
}

#[test]
fn upstream_error_is_generic_and_never_leaks_sensitive_upstream_data() {
    let error = translate_chat_response(&fixture("upstream_error"), None).expect_err("error maps");
    assert_eq!(error.code, "upstream_error");
    assert!(!error.message.contains("sk-secret-key"));
    assert!(!error.message.contains("private input"));
}

#[test]
fn sse_frame_uses_event_and_data_lines() {
    let mut translator = ResponsesSseTranslator::new("resp_frame", "kimi-k2.5");
    let event = translator
        .push_chunk(&json!({"choices": []}))
        .expect("chunk translates")
        .remove(0);
    let frame = sse_frame(&event);
    assert!(frame.starts_with("event: response.created\ndata: {"));
    assert!(frame.ends_with("\n\n"));
}

#[test]
fn anthropic_messages_request_maps_to_responses_request_with_unicode() {
    let request = parse_messages_request(&fixture("anthropic_messages_text")).expect("parse");
    assert_eq!(request.model, "gpt-5");
    assert_eq!(request.instructions.as_deref(), Some("Answer precisely."));
    assert_eq!(request.input.len(), 1);
    assert_eq!(request.tools[0].name, "weather");
    assert_eq!(request.passthrough["max_output_tokens"], 256);
    assert_eq!(request.passthrough["temperature"], json!(0.2));

    let responses = to_responses_request(&request);
    assert_eq!(responses["model"], "gpt-5");
    assert_eq!(responses["instructions"], "Answer precisely.");
    assert_eq!(responses["input"][0]["content"][0]["text"], "Hello, 世界");
    assert_eq!(responses["tools"][0]["name"], "weather");
    assert_eq!(responses["max_output_tokens"], 256);
    assert_eq!(responses["tool_choice"], "auto");
}

#[test]
fn anthropic_tool_history_becomes_responses_function_calls_and_outputs() {
    let request = parse_messages_request(&fixture("anthropic_messages_tools")).expect("parse");
    assert_eq!(request.input.len(), 4);
    assert!(matches!(
        request.input[1].content[0],
        crate::bridge::types::BridgeContent::ToolCall { .. }
    ));
    assert!(matches!(
        request.input[2].role,
        crate::bridge::types::MessageRole::Tool
    ));

    let responses = to_responses_request(&request);
    let input = responses["input"].as_array().expect("input array");
    assert_eq!(input[0]["role"], "user");
    assert_eq!(input[1]["type"], "function_call");
    assert_eq!(input[1]["call_id"], "call_weather");
    assert_eq!(input[2]["type"], "function_call");
    assert_eq!(input[2]["call_id"], "call_calendar");
    assert_eq!(input.len(), 5);
    assert_eq!(input[3]["type"], "function_call_output");
    assert_eq!(input[3]["call_id"], "call_weather");
    assert_eq!(input[3]["output"], "sunny");
    assert_eq!(input[4]["type"], "function_call_output");
    assert_eq!(input[4]["call_id"], "call_calendar");
}

#[test]
fn anthropic_mixed_user_text_and_tool_result_keeps_text_in_responses_input() {
    let request = parse_messages_request(&json!({
        "model": "gpt-5",
        "max_tokens": 128,
        "messages": [
            {
                "role": "user",
                "content": [
                    { "type": "text", "text": "Also consider this note." },
                    {
                        "type": "tool_result",
                        "tool_use_id": "call_weather",
                        "content": "sunny"
                    }
                ]
            }
        ]
    }))
    .expect("parse mixed user message");

    assert_eq!(request.input.len(), 1);
    assert!(matches!(
        request.input[0].role,
        crate::bridge::types::MessageRole::User
    ));
    assert!(
        request.input[0]
            .content
            .iter()
            .any(|part| matches!(part, crate::bridge::types::BridgeContent::Text { .. }))
    );
    assert!(
        request.input[0]
            .content
            .iter()
            .any(|part| matches!(part, crate::bridge::types::BridgeContent::ToolResult { .. }))
    );

    let responses = to_responses_request(&request);
    let input = responses["input"].as_array().expect("input array");
    assert_eq!(input.len(), 2);
    assert_eq!(input[0]["type"], "function_call_output");
    assert_eq!(input[0]["call_id"], "call_weather");
    assert_eq!(input[0]["output"], "sunny");
    assert_eq!(input[1]["type"], "message");
    assert_eq!(input[1]["role"], "user");
    assert_eq!(input[1]["content"][0]["type"], "input_text");
    assert_eq!(input[1]["content"][0]["text"], "Also consider this note.");
}

#[test]
fn anthropic_image_input_fails_closed() {
    let error =
        parse_messages_request(&fixture("anthropic_messages_image")).expect_err("image rejected");
    assert_eq!(error.code, "unsupported_image_input");
    assert!(!error.message.contains("example.invalid"));
}

#[test]
fn responses_output_to_ir_and_anthropic_message_round_trip_text() {
    let ir = responses_output_to_ir(&fixture("responses_upstream_text")).expect("to ir");
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
        ir.last(),
        Some(IrEvent::MessageEnd {
            stop_reason: StopReason::Stop
        })
    ));

    let message = encode_anthropic_message(&ir).expect("anthropic message");
    assert_eq!(message["role"], "assistant");
    assert_eq!(message["content"][0]["text"], "你好，世界。");
    assert_eq!(message["stop_reason"], "end_turn");
    assert_eq!(message["usage"]["input_tokens"], 8);
    assert_eq!(message["usage"]["cache_read_input_tokens"], 2);
}

#[test]
fn responses_output_to_ir_tool_call_becomes_anthropic_tool_use() {
    let ir = responses_output_to_ir(&fixture("responses_upstream_tool")).expect("to ir");
    let message = encode_anthropic_message(&ir).expect("anthropic message");
    assert_eq!(message["stop_reason"], "tool_use");
    assert_eq!(message["content"][0]["type"], "tool_use");
    assert_eq!(message["content"][0]["id"], "call_weather");
    assert_eq!(message["content"][0]["name"], "weather");
    assert_eq!(message["content"][0]["input"]["city"], "Tokyo");
}

#[test]
fn responses_sse_to_ir_to_anthropic_sse_text_and_unicode_chunks() {
    let chunks = fixture("responses_upstream_sse_text")
        .as_array()
        .cloned()
        .expect("array");
    let mut translator = ResponsesStreamToIr::new();
    let mut ir = chunks
        .iter()
        .flat_map(|chunk| translator.push_event(chunk).expect("event"))
        .collect::<Vec<_>>();
    ir.extend(translator.finish());

    let frames = encode_anthropic_sse(&ir).expect("sse frames");
    let joined = frames.join("");
    assert!(joined.contains("event: message_start\n"));
    assert!(joined.contains("event: content_block_delta\n"));
    assert!(joined.contains("\"text\":\"你\""));
    assert!(joined.contains("\"text\":\"好\""));
    assert!(joined.contains("event: message_delta\n"));
    assert!(joined.contains("event: message_stop\n"));
    assert!(joined.contains("\"stop_reason\":\"end_turn\""));
}

#[test]
fn responses_sse_to_ir_to_anthropic_sse_tool_call_deltas() {
    let chunks = fixture("responses_upstream_sse_tool")
        .as_array()
        .cloned()
        .expect("array");
    let mut translator = ResponsesStreamToIr::new();
    let mut ir = chunks
        .iter()
        .flat_map(|chunk| translator.push_event(chunk).expect("event"))
        .collect::<Vec<_>>();
    ir.extend(translator.finish());

    assert!(ir.iter().any(|event| matches!(
        event,
        IrEvent::ToolCallStart {
            id,
            name
        } if id == "call_weather" && name == "weather"
    )));
    let args = ir
        .iter()
        .filter_map(|event| match event {
            IrEvent::ToolCallDelta {
                id,
                arguments_delta,
            } if id == "call_weather" => Some(arguments_delta.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(args, "{\"city\":\"Tokyo\"}");

    let frames = encode_anthropic_sse(&ir).expect("sse frames");
    let joined = frames.join("");
    assert!(joined.contains("\"type\":\"tool_use\""));
    assert!(joined.contains("\"partial_json\":\"{\\\"city\\\":"));
    assert!(joined.contains("\"stop_reason\":\"tool_use\""));
}

#[test]
fn anthropic_thinking_configuration_fails_closed() {
    let error = parse_messages_request(&json!({
        "model": "gpt-5",
        "max_tokens": 32,
        "messages": [{ "role": "user", "content": "hi" }],
        "thinking": { "type": "enabled", "budget_tokens": 1024 }
    }))
    .expect_err("thinking must not be silently dropped");
    assert_eq!(error.code, "unsupported_thinking");
}

#[test]
fn responses_stream_error_is_generic_and_never_leaks_upstream_body() {
    let mut translator = ResponsesStreamToIr::new();
    let events = translator
        .push_event(&json!({
            "type": "error",
            "error": {
                "message": "Authorization Bearer sk-secret-key saw private input"
            }
        }))
        .expect("error event maps");
    assert_eq!(events.len(), 1);
    match &events[0] {
        IrEvent::Error {
            code,
            message,
            retryable,
        } => {
            assert_eq!(code, "upstream_error");
            assert!(!retryable);
            assert!(!message.contains("sk-secret-key"));
            assert!(!message.contains("private input"));
            assert_eq!(message, "The upstream model provider returned an error.");
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

#[test]
fn retry_gate_allows_transient_only_before_first_effective_event() {
    let gate = RetryGate::new(2);
    assert!(gate.can_retry(EmissionState::Idle, RetryClass::Transient, 0));
    assert!(!gate.can_retry(EmissionState::Idle, RetryClass::Transient, 1));
    assert!(!gate.can_retry(EmissionState::Idle, RetryClass::Permanent, 0));

    let mut state = EmissionState::Idle;
    state = state.observe(&IrEvent::MessageStart {
        id: "msg_1".into(),
        model: "gpt-5".into(),
    });
    assert_eq!(state, EmissionState::Idle);
    assert!(gate.can_retry(state, RetryClass::Transient, 0));

    // Usage alone is not client-visible content for retry purposes.
    state = state.observe(&IrEvent::Usage {
        input_tokens: 1,
        output_tokens: 0,
        cached_input_tokens: None,
    });
    assert_eq!(state, EmissionState::Idle);

    state = state.observe(&IrEvent::TextDelta { text: "hi".into() });
    assert_eq!(state, EmissionState::Emitted);
    assert!(
        !gate.can_retry(state, RetryClass::Transient, 0),
        "replay is forbidden after any effective client-visible content"
    );

    // Tool-call starts also commit the stream (prevents dual tool execution on replay).
    let after_tool = EmissionState::Idle.observe(&IrEvent::ToolCallStart {
        id: "call_1".into(),
        name: "weather".into(),
    });
    assert_eq!(after_tool, EmissionState::Emitted);
    assert!(!gate.can_retry(after_tool, RetryClass::Transient, 0));
}
