//! Shared ACP `session/update` decoding (Grok JSON-RPC and Kiro stream-json).

use serde_json::Value;

use crate::models::ProcessStep;

/// Decode one ACP session-update payload (`params`, `data`, or the update object).
pub(crate) fn decode_session_update(v: &Value) -> Vec<ProcessStep> {
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
        "usage" | "token_usage" | "tokenUsage" | "tokens_used" | "turn_completed"
        | "turn_usage" | "response_completed" => usage_steps(update),
        "context_usage"
        | "auto_compact_started"
        | "auto_compact_completed"
        | "auto_compact"
        | "context_compact"
        | "compaction"
        | "config_option_update" => vec![],
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

pub(crate) fn acp_content_text(v: &Value) -> String {
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

pub(crate) fn extract_tool_result(v: &Value) -> Option<String> {
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

pub(crate) fn collect_text(value: &Value, depth: usize) -> String {
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

fn usage_steps(update: &Value) -> Vec<ProcessStep> {
    let usage = update
        .get("usage")
        .or_else(|| update.get("tokenUsage"))
        .or_else(|| update.get("token_usage"))
        .filter(|u| u.is_object())
        .unwrap_or(update);
    ProcessStep::from_usage_object(usage)
        .map(|step| vec![step])
        .unwrap_or_default()
}

pub(crate) fn first_str(v: &Value, keys: &[&str]) -> Option<String> {
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
