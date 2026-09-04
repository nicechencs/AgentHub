//! Best-effort usage observation for identity-relay (passthrough) responses.
//!
//! Passthrough forwards upstream bytes unchanged. Monitoring still needs the
//! token counts from those payloads, so this observer parses usage objects
//! without rewriting the wire.

use std::collections::VecDeque;

use serde_json::{Map, Value};

use crate::bridge::usage::Usage;

use super::http::{sse_data_payload, sse_frame_end_deque};
use super::transport::UpstreamDecode;

const SSE_OBSERVE_BUF_LIMIT: usize = 256 * 1024;

#[derive(Debug)]
pub(super) struct PassthroughUsageObserver {
    decode: UpstreamDecode,
    usage: Option<Usage>,
    sse_buf: VecDeque<u8>,
}

impl PassthroughUsageObserver {
    pub(super) fn new(decode: UpstreamDecode) -> Self {
        Self {
            decode,
            usage: None,
            sse_buf: VecDeque::new(),
        }
    }

    pub(super) fn observe_json(&mut self, value: &Value) {
        let decode = self.decode;
        match decode {
            UpstreamDecode::ChatCompletions => {
                if let Some(usage) = value.get("usage").and_then(Usage::from_chat_usage) {
                    self.usage = Some(usage);
                }
            }
            UpstreamDecode::OpenAiResponses => {
                if let Some(usage) = responses_usage(value) {
                    self.usage = Some(usage);
                }
            }
            UpstreamDecode::AnthropicMessages => merge_anthropic_usage(&mut self.usage, value),
        }
    }

    /// Observe raw SSE bytes. Incomplete frames stay buffered; parse failures
    /// are ignored so observation cannot affect the forwarded stream.
    pub(super) fn observe_sse_bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.sse_buf.extend(bytes.iter().copied());
        while let Some((frame_end, delimiter_len)) = sse_frame_end_deque(&self.sse_buf) {
            let frame_len = frame_end + delimiter_len;
            let frame = self.sse_buf.drain(..frame_len).collect::<Vec<_>>();
            let Ok(Some(payload)) = sse_data_payload(&frame) else {
                continue;
            };
            if payload.is_empty() || payload == "[DONE]" {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(&payload) {
                self.observe_json(&value);
            }
        }
        if self.sse_buf.len() > SSE_OBSERVE_BUF_LIMIT {
            self.sse_buf.clear();
        }
    }

    pub(super) fn captured(&self) -> Option<&Usage> {
        self.usage.as_ref()
    }
}

fn responses_usage(value: &Value) -> Option<Usage> {
    value
        .get("usage")
        .and_then(Usage::from_responses_usage)
        .or_else(|| {
            value
                .get("response")
                .and_then(|response| response.get("usage"))
                .and_then(Usage::from_responses_usage)
        })
}

fn merge_anthropic_usage(slot: &mut Option<Usage>, value: &Value) {
    let Some(object) = anthropic_usage_object(value) else {
        return;
    };
    let input = object.get("input_tokens").and_then(Value::as_u64);
    let output = object.get("output_tokens").and_then(Value::as_u64);
    let cached = object
        .get("cache_read_input_tokens")
        .and_then(Value::as_u64)
        .or_else(|| {
            object
                .get("cache_creation_input_tokens")
                .and_then(Value::as_u64)
        });
    if input.is_none() && output.is_none() && cached.is_none() {
        return;
    }
    let current = slot.get_or_insert_with(Usage::default);
    if let Some(input) = input {
        current.input_tokens = input;
    }
    if let Some(output) = output {
        current.output_tokens = output;
    }
    if let Some(cached) = cached {
        current.cached_input_tokens = Some(cached);
    }
    current.total_tokens = current.input_tokens.saturating_add(current.output_tokens);
}

fn anthropic_usage_object(value: &Value) -> Option<&Map<String, Value>> {
    value.get("usage").and_then(Value::as_object).or_else(|| {
        value
            .get("message")
            .and_then(Value::as_object)
            .and_then(|message| message.get("usage"))
            .and_then(Value::as_object)
    })
}

#[cfg(test)]
mod tests;
