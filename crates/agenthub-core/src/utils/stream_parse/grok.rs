//! Parser for `grok -p --output-format streaming-json`.
//!
//! Grok Build ≥ 0.2.117 emits ACP-shaped NDJSON (JSON-RPC `session/update`),
//! matching the host decoder in RongleCat/grok-app. Older CLIs still use
//! `{type, data}` thought/text/usage/end lines. Both shapes are accepted.
//!
//! Unknown ACP kinds return an empty step list so the line is not dumped as
//! raw JSON into the Chat process timeline.

use serde_json::Value;

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
        "usage" => Some(vec![]),
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

fn decode_session_update(v: &Value) -> Vec<ProcessStep> {
    let update = v.get("update").or_else(|| v.get("data")).unwrap_or(v);
    let uty = update
        .get("sessionUpdate")
        .or_else(|| update.get("session_update"))
        .or_else(|| update.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    match uty {
        "agent_message_chunk" | "message" | "agent_message" => {
            let text = acp_content_text(update);
            if text.is_empty() {
                vec![]
            } else {
                vec![ProcessStep::Text { text }]
            }
        }
        "agent_thought_chunk" | "thought" => {
            let text = acp_content_text(update);
            if text.is_empty() {
                vec![]
            } else {
                vec![ProcessStep::Thinking { text, done: false }]
            }
        }
        "tool_call" | "tool_call_update" => {
            let is_update = uty.ends_with("update");
            let raw_status = update.get("status").and_then(|s| s.as_str()).unwrap_or("");
            vec![ProcessStep::Tool {
                id: first_str(update, &["toolCallId", "tool_call_id", "id"]),
                name: first_str(update, &["title", "kind", "name"])
                    .unwrap_or_else(|| "tool".into()),
                input: update
                    .get("rawInput")
                    .or_else(|| update.get("raw_input"))
                    .or_else(|| update.get("input"))
                    .cloned(),
                status: map_tool_status(raw_status, is_update),
                result: extract_tool_result(update),
            }]
        }
        "retry_state" => {
            let attempt = update.get("attempt").and_then(|n| n.as_u64()).unwrap_or(0);
            let max = update
                .get("max_retries")
                .or_else(|| update.get("maxRetries"))
                .and_then(|n| n.as_u64())
                .unwrap_or(0);
            let reason = update
                .get("reason")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .trim();
            let detail = if reason.is_empty() {
                format!("retry {attempt}/{max}")
            } else {
                format!("retry {attempt}/{max}: {reason}")
            };
            vec![ProcessStep::Status {
                phase: "running".into(),
                detail: Some(detail),
            }]
        }
        "plan" => {
            let body = update
                .get("planContent")
                .or_else(|| update.get("plan_content"))
                .and_then(|s| s.as_str())
                .or_else(|| update.get("content").and_then(|s| s.as_str()))
                .unwrap_or("");
            vec![ProcessStep::Status {
                phase: "running".into(),
                detail: Some(if body.is_empty() {
                    "plan".into()
                } else {
                    truncate(body, 240)
                }),
            }]
        }
        "available_commands" | "available_commands_update" => vec![],
        "usage"
        | "token_usage"
        | "tokenUsage"
        | "context_usage"
        | "tokens_used"
        | "turn_completed"
        | "turn_usage"
        | "response_completed"
        | "auto_compact_started"
        | "auto_compact_completed"
        | "auto_compact"
        | "context_compact"
        | "compaction" => vec![],
        "error" => {
            let message = update
                .get("message")
                .or_else(|| update.get("error"))
                .and_then(|s| s.as_str())
                .unwrap_or("error")
                .to_string();
            vec![ProcessStep::Error { message }]
        }
        // Recognized envelope, unknown kind — do not fall back to raw JSON.
        _ => vec![],
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
    v.get("data")
        .and_then(|d| d.as_str())
        .or_else(|| v.get("text").and_then(|t| t.as_str()))
        .or_else(|| v.get("content").and_then(|c| c.as_str()))
        .or_else(|| v.pointer("/content/text").and_then(|t| t.as_str()))
        .unwrap_or("")
        .to_string()
}

fn acp_content_text(v: &Value) -> String {
    if let Some(t) = v
        .pointer("/content/text")
        .or_else(|| v.get("text"))
        .or_else(|| v.get("delta"))
        .or_else(|| v.pointer("/content/delta"))
        .and_then(|t| t.as_str())
    {
        if !t.is_empty() {
            return t.to_string();
        }
    }
    collect_text(v.get("content").unwrap_or(&Value::Null), 0)
}

fn extract_tool_result(v: &Value) -> Option<String> {
    if let Some(s) = v
        .get("data")
        .or_else(|| v.get("result"))
        .or_else(|| v.get("output"))
        .and_then(|o| match o {
            Value::String(s) => Some(s.clone()),
            other if !other.is_null() => Some(other.to_string()),
            _ => None,
        })
    {
        return Some(truncate(&s, 800));
    }
    let from_content = collect_text(v.get("content").unwrap_or(&Value::Null), 0);
    if from_content.is_empty() {
        None
    } else {
        Some(truncate(&from_content, 800))
    }
}

fn collect_text(value: &Value, depth: usize) -> String {
    if depth > 4 {
        return String::new();
    }
    match value {
        Value::String(s) => s.clone(),
        Value::Array(items) => {
            let mut out = String::new();
            for item in items {
                let piece = collect_text(item, depth + 1);
                if !piece.is_empty() {
                    out.push_str(&piece);
                }
            }
            out
        }
        Value::Object(map) => {
            if let Some(t) = map.get("text").and_then(|t| t.as_str()) {
                return t.to_string();
            }
            if let Some(c) = map.get("content") {
                return collect_text(c, depth + 1);
            }
            String::new()
        }
        _ => String::new(),
    }
}

fn first_str(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_str()))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn map_tool_status(raw: &str, is_update: bool) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "completed" | "complete" | "success" | "ok" | "done" => "end".into(),
        "failed" | "error" | "rejected" | "denied" | "cancelled" | "canceled" => "end".into(),
        "pending" | "in_progress" | "running" | "start" => {
            if is_update {
                "update".into()
            } else {
                "start".into()
            }
        }
        "" => {
            if is_update {
                "update".into()
            } else {
                "start".into()
            }
        }
        other => other.to_string(),
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let head: String = s.chars().take(max_chars).collect();
        format!("{head}…")
    }
}

#[cfg(test)]
mod tests;
