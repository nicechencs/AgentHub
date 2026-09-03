//! Aggregate a complete Responses JSON object from an official SSE body.
//!
//! Official ChatGPT / Codex Responses requires `stream: true`. When the
//! downstream client asked for a non-stream body, the host consumes the SSE
//! and rebuilds one Responses object so existing dialect encoders can run.

use serde_json::Value;

use crate::bridge::types::{ProtocolError, ProtocolResult, UPSTREAM_STREAM_REQUIRED_ZH};

use super::{encode_responses_from_ir, ResponsesStreamToIr};

/// True when the buffered upstream body is SSE rather than a JSON object.
pub fn looks_like_sse_body(body: &[u8]) -> bool {
    let trimmed = trim_ascii_start(body);
    trimmed.starts_with(b"data:")
        || trimmed.starts_with(b"event:")
        || trimmed.starts_with(b"id:")
        || trimmed.starts_with(b"retry:")
}

/// Rebuild a complete Responses JSON object from a finished SSE buffer.
///
/// Prefers the `response` object on `response.completed` / `response.incomplete`
/// so usage and `id` match the upstream. Falls back to IR encoding when the
/// terminal event has no embedded object.
pub fn aggregate_responses_sse_to_json(body: &[u8]) -> ProtocolResult<Value> {
    let text =
        std::str::from_utf8(body).map_err(|_| ProtocolError::upstream_stream_incomplete())?;
    let mut translator = ResponsesStreamToIr::new();
    let mut events = Vec::new();
    let mut completed_response = None;
    let mut saw_terminal = false;
    let mut saw_error = false;

    for payload in sse_data_payloads(text) {
        if payload == "[DONE]" {
            saw_terminal = true;
            continue;
        }
        let value = serde_json::from_str::<Value>(&payload)
            .map_err(|_| ProtocolError::upstream_stream_incomplete())?;
        let event_type = value.get("type").and_then(Value::as_str).unwrap_or("");
        match event_type {
            "response.completed" | "response.incomplete" => {
                if let Some(response) = value.get("response") {
                    completed_response = Some(response.clone());
                }
                saw_terminal = true;
            }
            "response.failed" | "error" => {
                saw_error = true;
                saw_terminal = true;
            }
            _ => {}
        }
        events.extend(translator.push_event(&value)?);
    }

    if saw_error && completed_response.is_none() {
        return Err(ProtocolError::upstream());
    }
    if !saw_terminal && completed_response.is_none() {
        return Err(ProtocolError::upstream_stream_incomplete());
    }
    events.extend(translator.finish());
    if let Some(response) = completed_response {
        if response_has_output(&response) {
            return Ok(response);
        }
        // Official streams often send `output: []` on completed and put text
        // only in deltas. Fold IR, then keep the upstream id / usage.
        let mut encoded =
            encode_responses_from_ir(&events, response.get("id").and_then(Value::as_str))?;
        if let Some(id) = response.get("id") {
            encoded["id"] = id.clone();
        }
        if let Some(model) = response.get("model") {
            encoded["model"] = model.clone();
        }
        if let Some(usage) = response.get("usage") {
            encoded["usage"] = usage.clone();
        }
        return Ok(encoded);
    }
    encode_responses_from_ir(&events, None)
}

fn response_has_output(response: &Value) -> bool {
    response
        .get("output")
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty())
}

/// Client-facing message when a redacted upstream detail is the stream-only contract.
pub fn client_message_for_upstream_detail(detail: Option<&str>) -> Option<&'static str> {
    let detail = detail?;
    if !upstream_detail_requires_stream(detail) {
        return None;
    }
    Some(UPSTREAM_STREAM_REQUIRED_ZH)
}

/// True when the upstream 400 is the official Codex stream-only contract.
pub fn upstream_detail_requires_stream(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    if lower.contains("stream must be set to true") {
        return true;
    }
    if lower.contains("stream is required") {
        return true;
    }
    lower.contains("stream")
        && lower.contains("true")
        && (lower.contains("must") || lower.contains("require"))
}

fn trim_ascii_start(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    &bytes[start..]
}

fn sse_data_payloads(body: &str) -> Vec<String> {
    let normalized = body.replace("\r\n", "\n").replace('\r', "\n");
    normalized
        .split("\n\n")
        .filter_map(block_data_payload)
        .collect()
}

fn block_data_payload(block: &str) -> Option<String> {
    let mut lines = Vec::new();
    for line in block.lines() {
        let Some(rest) = line.strip_prefix("data:") else {
            continue;
        };
        lines.push(rest.strip_prefix(' ').unwrap_or(rest));
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}
