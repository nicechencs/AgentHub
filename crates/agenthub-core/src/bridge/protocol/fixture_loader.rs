use serde_json::Value;

pub(super) fn fixture(name: &str) -> Value {
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
        "anthropic_upstream_text" => include_str!("fixtures/anthropic_upstream_text.json"),
        "anthropic_upstream_tool" => include_str!("fixtures/anthropic_upstream_tool.json"),
        "anthropic_upstream_stop_max_tokens" => {
            include_str!("fixtures/anthropic_upstream_stop_max_tokens.json")
        }
        "anthropic_upstream_usage" => include_str!("fixtures/anthropic_upstream_usage.json"),
        "anthropic_upstream_sse_text" => include_str!("fixtures/anthropic_upstream_sse_text.json"),
        "anthropic_upstream_sse_tool" => include_str!("fixtures/anthropic_upstream_sse_tool.json"),
        "anthropic_upstream_error" => include_str!("fixtures/anthropic_upstream_error.json"),
        "anthropic_upstream_thinking" => include_str!("fixtures/anthropic_upstream_thinking.json"),
        "anthropic_upstream_sse_truncated" => {
            include_str!("fixtures/anthropic_upstream_sse_truncated.json")
        }
        "anthropic_upstream_sse_orphan_delta" => {
            include_str!("fixtures/anthropic_upstream_sse_orphan_delta.json")
        }
        "anthropic_upstream_usage_malformed" => {
            include_str!("fixtures/anthropic_upstream_usage_malformed.json")
        }
        "anthropic_upstream_stop_unknown" => {
            include_str!("fixtures/anthropic_upstream_stop_unknown.json")
        }
        "claude_codex_responses_text" => include_str!("fixtures/claude_codex_responses_text.json"),
        "claude_codex_responses_multiturn" => {
            include_str!("fixtures/claude_codex_responses_multiturn.json")
        }
        "claude_codex_responses_tools" => {
            include_str!("fixtures/claude_codex_responses_tools.json")
        }
        "claude_codex_anthropic_text_usage" => {
            include_str!("fixtures/claude_codex_anthropic_text_usage.json")
        }
        "claude_codex_anthropic_stop_max_tokens" => {
            include_str!("fixtures/claude_codex_anthropic_stop_max_tokens.json")
        }
        "claude_codex_anthropic_error" => {
            include_str!("fixtures/claude_codex_anthropic_error.json")
        }
        "claude_codex_anthropic_sse_split" => {
            include_str!("fixtures/claude_codex_anthropic_sse_split.json")
        }
        "claude_codex_anthropic_thinking" => {
            include_str!("fixtures/claude_codex_anthropic_thinking.json")
        }
        "claude_codex_unsupported_image" => {
            include_str!("fixtures/claude_codex_unsupported_image.json")
        }
        "pair_codex_to_grok_request" => include_str!("fixtures/pair_codex_to_grok_request.json"),
        "pair_codex_to_grok_response" => include_str!("fixtures/pair_codex_to_grok_response.json"),
        "pair_codex_to_grok_sse" => include_str!("fixtures/pair_codex_to_grok_sse.json"),
        "pair_grok_to_codex_request" => include_str!("fixtures/pair_grok_to_codex_request.json"),
        "pair_grok_to_codex_response" => include_str!("fixtures/pair_grok_to_codex_response.json"),
        "pair_grok_to_codex_sse" => include_str!("fixtures/pair_grok_to_codex_sse.json"),
        "pair_parallel_tools" => include_str!("fixtures/pair_parallel_tools.json"),
        "pair_error_event" => include_str!("fixtures/pair_error_event.json"),
        _ => panic!("unknown fixture {name}"),
    };
    serde_json::from_str(source).expect("fixture is valid JSON")
}
