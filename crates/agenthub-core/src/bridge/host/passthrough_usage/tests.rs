use serde_json::json;

use super::super::transport::UpstreamDecode;
use super::PassthroughUsageObserver;

fn chat() -> PassthroughUsageObserver {
    PassthroughUsageObserver::new(UpstreamDecode::ChatCompletions)
}

fn responses() -> PassthroughUsageObserver {
    PassthroughUsageObserver::new(UpstreamDecode::OpenAiResponses)
}

fn anthropic() -> PassthroughUsageObserver {
    PassthroughUsageObserver::new(UpstreamDecode::AnthropicMessages)
}

#[test]
fn chat_json_reads_prompt_and_completion_tokens() {
    let mut observer = chat();
    observer.observe_json(&json!({
        "usage": {
            "prompt_tokens": 11,
            "completion_tokens": 4,
            "total_tokens": 15,
            "prompt_tokens_details": { "cached_tokens": 3 }
        }
    }));
    let usage = observer.captured().expect("usage");
    assert_eq!(usage.input_tokens, 11);
    assert_eq!(usage.output_tokens, 4);
    assert_eq!(usage.cached_input_tokens, Some(3));
}

#[test]
fn responses_json_and_completed_event_both_count() {
    let mut observer = responses();
    observer.observe_json(&json!({
        "usage": {
            "input_tokens": 8,
            "output_tokens": 2,
            "input_tokens_details": { "cached_tokens": 1 }
        }
    }));
    assert_eq!(observer.captured().expect("body").input_tokens, 8);

    observer.observe_json(&json!({
        "type": "response.completed",
        "response": {
            "usage": {
                "input_tokens": 11,
                "output_tokens": 4,
                "input_tokens_details": { "cached_tokens": 3 }
            }
        }
    }));
    let usage = observer.captured().expect("completed");
    assert_eq!(usage.input_tokens, 11);
    assert_eq!(usage.output_tokens, 4);
    assert_eq!(usage.cached_input_tokens, Some(3));
}

#[test]
fn anthropic_merges_message_start_input_with_delta_output() {
    let mut observer = anthropic();
    observer.observe_json(&json!({
        "type": "message_start",
        "message": {
            "usage": {
                "input_tokens": 11,
                "output_tokens": 0,
                "cache_read_input_tokens": 2
            }
        }
    }));
    observer.observe_json(&json!({
        "type": "message_delta",
        "usage": { "output_tokens": 4 }
    }));
    let usage = observer.captured().expect("merged");
    assert_eq!(usage.input_tokens, 11);
    assert_eq!(usage.output_tokens, 4);
    assert_eq!(usage.cached_input_tokens, Some(2));
}

#[test]
fn missing_usage_stays_empty() {
    let mut observer = chat();
    observer.observe_json(&json!({ "choices": [] }));
    observer.observe_sse_bytes(b"data: [DONE]\n\n");
    assert!(observer.captured().is_none());
}

#[test]
fn sse_bytes_survive_split_frames() {
    let mut observer = chat();
    observer.observe_sse_bytes(b"data: {\"choices\":[{\"delta\":{\"content\":\"h\"}}]}\n\n");
    observer.observe_sse_bytes(b"data: {\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":4}");
    assert!(
        observer.captured().is_none(),
        "incomplete frame is buffered"
    );
    observer.observe_sse_bytes(b"}\n\ndata: [DONE]\n\n");
    let usage = observer.captured().expect("joined frame");
    assert_eq!(usage.input_tokens, 11);
    assert_eq!(usage.output_tokens, 4);
}

#[test]
fn anthropic_sse_event_lines_are_ignored_for_payload() {
    let mut observer = anthropic();
    observer.observe_sse_bytes(
        b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":9,\"output_tokens\":0}}}\n\n",
    );
    observer.observe_sse_bytes(
        b"event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":3}}\n\n",
    );
    let usage = observer.captured().expect("sse");
    assert_eq!(usage.input_tokens, 9);
    assert_eq!(usage.output_tokens, 3);
}
