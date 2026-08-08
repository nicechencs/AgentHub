//! Parser for `pi -p --mode json` session event stream (measured 2026-08).
//!
//! Observed event types include: `session`, `agent_start`, `turn_start`,
//! `message_start` / `message_update` / `message_end`, `turn_end`, `agent_end`,
//! `agent_settled`, plus tool execution events when tools run.

use serde_json::Value;

use crate::models::ProcessStep;

pub fn parse_line(line: &str) -> Option<Vec<ProcessStep>> {
    let v: Value = serde_json::from_str(line).ok()?;
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match ty {
        "session" | "agent_start" | "turn_start" => Some(vec![ProcessStep::Status {
            phase: "starting".into(),
            detail: Some(ty.into()),
        }]),
        "agent_end" | "agent_settled" | "turn_end" => Some(vec![ProcessStep::Status {
            phase: "result".into(),
            detail: Some(ty.into()),
        }]),
        "message_update" => parse_message_update(&v),
        "message_start" | "message_end" => {
            // Prefer deltas from message_update to avoid double-counting full text.
            // Still surface tools if present on the message.
            let msg = v.get("message")?;
            if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
                return Some(vec![]);
            }
            if ty == "message_end" {
                // Only extract tool blocks; text already streamed via deltas.
                return Some(tools_from_message(msg));
            }
            Some(vec![])
        }
        "tool_execution_start" | "tool_start" | "tool_call" => Some(vec![tool_event(&v, "start")]),
        "tool_execution_end" | "tool_end" | "tool_result" => Some(vec![tool_event(&v, "end")]),
        "bash_execution_update" | "tool_execution_update" => Some(vec![tool_event(&v, "update")]),
        "error" => {
            let message = v
                .get("error")
                .and_then(|e| e.as_str())
                .or_else(|| v.get("message").and_then(|m| m.as_str()))
                .unwrap_or("error")
                .to_string();
            Some(vec![ProcessStep::Error { message }])
        }
        _ => None,
    }
}

fn parse_message_update(v: &Value) -> Option<Vec<ProcessStep>> {
    let ev = v.get("assistantMessageEvent")?;
    let ety = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match ety {
        "text_delta" => {
            let t = ev.get("delta").and_then(|d| d.as_str()).unwrap_or("");
            if t.is_empty() {
                Some(vec![])
            } else {
                Some(vec![ProcessStep::Text { text: t.into() }])
            }
        }
        "text_start" | "text_end" => Some(vec![]),
        "thinking_delta" => {
            let t = ev.get("delta").and_then(|d| d.as_str()).unwrap_or("");
            if t.is_empty() {
                Some(vec![])
            } else {
                Some(vec![ProcessStep::Thinking {
                    text: t.into(),
                    done: false,
                }])
            }
        }
        "thinking_start" => Some(vec![ProcessStep::Status {
            phase: "thinking".into(),
            detail: Some("start".into()),
        }]),
        "thinking_end" => Some(vec![ProcessStep::Thinking {
            text: String::new(),
            done: true,
        }]),
        "toolcall_start" | "tool_call_start" => Some(vec![tool_event(ev, "start")]),
        "toolcall_end" | "tool_call_end" => Some(vec![tool_event(ev, "end")]),
        _ => None,
    }
}

fn tools_from_message(msg: &Value) -> Vec<ProcessStep> {
    let mut steps = Vec::new();
    if let Some(arr) = msg.get("content").and_then(|c| c.as_array()) {
        for block in arr {
            let bty = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if matches!(bty, "toolCall" | "tool_use" | "tool_call" | "functionCall") {
                steps.push(tool_event(block, "end"));
            }
        }
    }
    steps
}

fn tool_event(v: &Value, status: &str) -> ProcessStep {
    let name = v
        .get("toolName")
        .or_else(|| v.get("name"))
        .or_else(|| v.get("tool"))
        .and_then(|n| n.as_str())
        .unwrap_or("tool")
        .to_string();
    let id = v
        .get("id")
        .or_else(|| v.get("toolCallId"))
        .or_else(|| v.get("call_id"))
        .and_then(|i| i.as_str())
        .map(|s| s.to_string());
    let input = v
        .get("args")
        .or_else(|| v.get("input"))
        .or_else(|| v.get("arguments"))
        .cloned();
    let result = v
        .get("result")
        .or_else(|| v.get("output"))
        .map(|o| match o {
            Value::String(s) => truncate(s, 800),
            other => truncate(&other.to_string(), 800),
        });
    ProcessStep::Tool {
        id,
        name,
        input,
        status: status.into(),
        result,
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
mod tests {
    use super::*;

    #[test]
    fn parses_text_delta() {
        let line = r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"ok"}}"#;
        let steps = parse_line(line).unwrap();
        assert_eq!(steps, vec![ProcessStep::Text { text: "ok".into() }]);
    }

    #[test]
    fn parses_thinking_delta() {
        let line = r#"{"type":"message_update","assistantMessageEvent":{"type":"thinking_delta","delta":"plan"}}"#;
        let steps = parse_line(line).unwrap();
        assert!(matches!(
            &steps[0],
            ProcessStep::Thinking { text, done: false } if text == "plan"
        ));
    }

    #[test]
    fn parses_agent_start() {
        let steps = parse_line(r#"{"type":"agent_start"}"#).unwrap();
        assert!(matches!(
            &steps[0],
            ProcessStep::Status { phase, .. } if phase == "starting"
        ));
    }
}
