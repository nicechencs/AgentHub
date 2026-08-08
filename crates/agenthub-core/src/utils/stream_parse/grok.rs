//! Parser for `grok -p --output-format streaming-json` (measured 2026-08).
//!
//! Observed types: `available_commands`, `thought` (+`data` deltas), `text`
//! (+`data` deltas), `usage`, `end`. Tool-related types are accepted when present
//! (`tool`, `tool_call`, `tool_result`, ACP-style updates).

use serde_json::Value;

use crate::models::ProcessStep;

pub fn parse_line(line: &str) -> Option<Vec<ProcessStep>> {
    let v: Value = serde_json::from_str(line).ok()?;
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match ty {
        "available_commands" | "session" | "start" => Some(vec![ProcessStep::Status {
            phase: "starting".into(),
            detail: Some(ty.into()),
        }]),
        "end" | "done" | "result" => Some(vec![ProcessStep::Status {
            phase: "result".into(),
            detail: Some(ty.into()),
        }]),
        "usage" => Some(vec![]),
        "thought" | "thinking" | "reasoning" => {
            let text = extract_data_text(&v);
            if text.is_empty() {
                Some(vec![])
            } else {
                Some(vec![ProcessStep::Thinking { text, done: false }])
            }
        }
        "text" | "message" | "assistant" => {
            let text = extract_data_text(&v);
            if text.is_empty() {
                // Nested content blocks
                if let Some(arr) = v.get("content").and_then(|c| c.as_array()) {
                    let mut steps = Vec::new();
                    for block in arr {
                        if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                            if !t.is_empty() {
                                steps.push(ProcessStep::Text { text: t.into() });
                            }
                        }
                    }
                    return Some(steps);
                }
                Some(vec![])
            } else {
                Some(vec![ProcessStep::Text { text }])
            }
        }
        "tool" | "tool_call" | "tool_use" | "function_call" => Some(vec![ProcessStep::Tool {
            id: v
                .get("id")
                .or_else(|| v.get("call_id"))
                .and_then(|i| i.as_str())
                .map(|s| s.to_string()),
            name: v
                .get("name")
                .or_else(|| v.get("tool"))
                .and_then(|n| n.as_str())
                .unwrap_or("tool")
                .to_string(),
            input: v
                .get("input")
                .or_else(|| v.get("arguments"))
                .or_else(|| v.get("data"))
                .cloned(),
            status: "start".into(),
            result: None,
        }]),
        "tool_result" | "tool_end" => Some(vec![ProcessStep::Tool {
            id: v
                .get("id")
                .or_else(|| v.get("call_id"))
                .and_then(|i| i.as_str())
                .map(|s| s.to_string()),
            name: v
                .get("name")
                .or_else(|| v.get("tool"))
                .and_then(|n| n.as_str())
                .unwrap_or("tool")
                .to_string(),
            input: None,
            status: "end".into(),
            result: v
                .get("data")
                .or_else(|| v.get("result"))
                .or_else(|| v.get("output"))
                .map(|o| match o {
                    Value::String(s) => truncate(s, 800),
                    other => truncate(&other.to_string(), 800),
                }),
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
        // ACP session/update style (future-proof)
        "session/update" | "session_update" => parse_acp_update(&v),
        _ => None,
    }
}

fn extract_data_text(v: &Value) -> String {
    v.get("data")
        .and_then(|d| d.as_str())
        .or_else(|| v.get("text").and_then(|t| t.as_str()))
        .or_else(|| v.get("content").and_then(|c| c.as_str()))
        .unwrap_or("")
        .to_string()
}

fn parse_acp_update(v: &Value) -> Option<Vec<ProcessStep>> {
    let update = v.get("update").or_else(|| v.get("data"))?;
    let uty = update
        .get("sessionUpdate")
        .or_else(|| update.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    match uty {
        "agent_message_chunk" | "message" => {
            let t = update
                .pointer("/content/text")
                .or_else(|| update.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            if t.is_empty() {
                Some(vec![])
            } else {
                Some(vec![ProcessStep::Text { text: t.into() }])
            }
        }
        "agent_thought_chunk" | "thought" => {
            let t = update
                .pointer("/content/text")
                .or_else(|| update.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            if t.is_empty() {
                Some(vec![])
            } else {
                Some(vec![ProcessStep::Thinking {
                    text: t.into(),
                    done: false,
                }])
            }
        }
        "tool_call" | "tool_call_update" => {
            let status = if uty.ends_with("update") {
                "update"
            } else {
                "start"
            };
            Some(vec![ProcessStep::Tool {
                id: update
                    .get("toolCallId")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string()),
                name: update
                    .get("title")
                    .or_else(|| update.get("kind"))
                    .or_else(|| update.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("tool")
                    .to_string(),
                input: update.get("rawInput").cloned(),
                status: status.into(),
                result: None,
            }])
        }
        _ => None,
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
    fn parses_thought_and_text_data() {
        let t = parse_line(r#"{"type":"thought","data":"plan"}"#).unwrap();
        assert!(matches!(&t[0], ProcessStep::Thinking { text, .. } if text == "plan"));
        let x = parse_line(r#"{"type":"text","data":"hi"}"#).unwrap();
        assert_eq!(x, vec![ProcessStep::Text { text: "hi".into() }]);
    }

    #[test]
    fn parses_end_status() {
        let s = parse_line(r#"{"type":"end"}"#).unwrap();
        assert!(matches!(
            &s[0],
            ProcessStep::Status { phase, .. } if phase == "result"
        ));
    }
}
