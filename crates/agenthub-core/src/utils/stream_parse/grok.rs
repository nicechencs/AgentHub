//! Parser for `grok -p --output-format streaming-json`.
//!
//! Grok Build ≥ 0.2.117 emits ACP-shaped NDJSON (JSON-RPC `session/update`),
//! matching the host decoder in RongleCat/grok-app. Older CLIs still use
//! `{type, data}` thought/text/usage/end lines. Both shapes are accepted.
//!
//! Unknown ACP kinds return an empty step list so the line is not dumped as
//! raw JSON into the Chat process timeline.

use serde_json::Value;

use super::acp::{
    acp_content_text, collect_text, decode_session_update, extract_tool_result, first_str,
};
use crate::models::ProcessStep;

pub fn parse_line(line: &str) -> Option<Vec<ProcessStep>> {
    let v: Value = serde_json::from_str(line).ok()?;

    if let Some(method) = v.get("method").and_then(|m| m.as_str()) {
        return parse_rpc_method(method, &v);
    }

    if v.get("sessionUpdate").is_some()
        || v.get("session_update").is_some()
        || v.get("update")
            .and_then(|u| u.get("sessionUpdate").or_else(|| u.get("session_update")))
            .is_some()
    {
        return Some(decode_session_update(&v));
    }

    parse_legacy_type(&v)
}

fn parse_rpc_method(method: &str, v: &Value) -> Option<Vec<ProcessStep>> {
    match method {
        "session/update"
        | "session_update"
        | "_x.ai/session/update"
        | "_x.ai/session_notification" => {
            let payload = v.get("params").unwrap_or(v);
            Some(decode_session_update(payload))
        }
        "_x.ai/session/prompt_complete" => {
            let reason = v
                .pointer("/params/stopReason")
                .or_else(|| v.pointer("/params/stop_reason"))
                .and_then(|s| s.as_str())
                .unwrap_or("end_turn");
            Some(vec![ProcessStep::Status {
                phase: "result".into(),
                detail: Some(reason.into()),
            }])
        }
        // Reverse-RPC and other JSON-RPC traffic is not actionable in `-p`
        // mode. Swallow so Chat does not render the envelope as raw JSON.
        _ => Some(vec![]),
    }
}

fn parse_legacy_type(v: &Value) -> Option<Vec<ProcessStep>> {
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match ty {
        "available_commands" | "session" => Some(vec![]),
        "start" => Some(vec![ProcessStep::Status {
            phase: "starting".into(),
            detail: Some(ty.into()),
        }]),
        "end" | "done" | "result" => Some(vec![ProcessStep::Status {
            phase: "result".into(),
            detail: Some(ty.into()),
        }]),
        "usage" => {
            let obj = v.get("data").filter(|d| d.is_object()).unwrap_or(v);
            Some(
                ProcessStep::from_usage_object(obj)
                    .map(|step| vec![step])
                    .unwrap_or_default(),
            )
        }
        "thought" | "thinking" | "reasoning" => {
            let text = extract_data_text(v);
            if text.is_empty() {
                Some(vec![])
            } else {
                Some(vec![ProcessStep::Thinking { text, done: false }])
            }
        }
        "text" | "message" | "assistant" => Some(text_steps(v)),
        "tool" | "tool_call" | "tool_use" | "function_call" => Some(vec![ProcessStep::Tool {
            id: first_str(v, &["id", "call_id", "toolCallId"]),
            name: first_str(v, &["name", "tool", "title", "kind"]).unwrap_or_else(|| "tool".into()),
            input: v
                .get("input")
                .or_else(|| v.get("arguments"))
                .or_else(|| v.get("rawInput"))
                .or_else(|| v.get("data"))
                .cloned(),
            status: "start".into(),
            result: None,
        }]),
        "tool_result" | "tool_end" => Some(vec![ProcessStep::Tool {
            id: first_str(v, &["id", "call_id", "toolCallId"]),
            name: first_str(v, &["name", "tool", "title", "kind"]).unwrap_or_else(|| "tool".into()),
            input: None,
            status: "end".into(),
            result: extract_tool_result(v),
        }]),
        "error" => {
            let message = v
                .get("error")
                .and_then(|e| e.as_str())
                .or_else(|| v.get("message").and_then(|m| m.as_str()))
                .or_else(|| v.get("data").and_then(|d| d.as_str()))
                .unwrap_or("error")
                .to_string();
            Some(vec![ProcessStep::Error { message }])
        }
        "session/update" | "session_update" => Some(decode_session_update(v)),
        _ => None,
    }
}

fn text_steps(v: &Value) -> Vec<ProcessStep> {
    let text = extract_data_text(v);
    if !text.is_empty() {
        return vec![ProcessStep::Text { text }];
    }
    if let Some(arr) = v.get("content").and_then(|c| c.as_array()) {
        let mut steps = Vec::new();
        for block in arr {
            if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                if !t.is_empty() {
                    steps.push(ProcessStep::Text { text: t.into() });
                }
            }
        }
        return steps;
    }
    let nested = acp_content_text(v);
    if nested.is_empty() {
        vec![]
    } else {
        vec![ProcessStep::Text { text: nested }]
    }
}

fn extract_data_text(v: &Value) -> String {
    if let Some(data) = v.get("data") {
        let text = collect_text(data, 0);
        if !text.is_empty() {
            return text;
        }
    }
    v.get("text")
        .and_then(|t| t.as_str())
        .or_else(|| v.get("content").and_then(|c| c.as_str()))
        .or_else(|| v.pointer("/content/text").and_then(|t| t.as_str()))
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests;
