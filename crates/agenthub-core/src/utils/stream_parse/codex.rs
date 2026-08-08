//! Best-effort parser for `codex exec --json` JSONL events.
//!
//! Event types include `thread.*`, `turn.*`, `item.*`, and nested `item` payloads
//! (`agent_message`, `command_execution`, `file_change`, …). Schema is version-
//! tolerant: unknown types become [`None`] (session may emit Raw).

use serde_json::Value;

use crate::models::ProcessStep;

pub fn parse_line(line: &str) -> Option<Vec<ProcessStep>> {
    let v: Value = serde_json::from_str(line).ok()?;
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match ty {
        "thread.started" | "turn.started" => Some(vec![ProcessStep::Status {
            phase: "starting".into(),
            detail: Some(ty.into()),
        }]),
        "turn.completed" => Some(vec![ProcessStep::Status {
            phase: "result".into(),
            detail: Some(ty.into()),
        }]),
        "turn.failed" | "error" => {
            let message = v
                .get("error")
                .and_then(|e| {
                    e.as_str().map(|s| s.to_string()).or_else(|| {
                        e.get("message")
                            .and_then(|m| m.as_str())
                            .map(|s| s.to_string())
                    })
                })
                .or_else(|| {
                    v.get("message")
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| ty.to_string());
            Some(vec![ProcessStep::Error { message }])
        }
        "item.started" | "item.updated" | "item.completed" => {
            let item = v.get("item").unwrap_or(&v);
            let status = if ty.ends_with("started") {
                "start"
            } else if ty.ends_with("completed") {
                "end"
            } else {
                "update"
            };
            // agent_message text only on completed — avoid duplicating full text
            // across started/updated/completed events from some Codex builds.
            let only_completed_text = ty.ends_with("completed");
            Some(parse_item(item, status, only_completed_text))
        }
        "agent_message" => {
            let text = v
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            if text.is_empty() {
                Some(vec![])
            } else {
                Some(vec![ProcessStep::Text { text }])
            }
        }
        _ => {
            // Some builds nest under "item" without item.* type prefix.
            if let Some(item) = v.get("item") {
                return Some(parse_item(item, "end", true));
            }
            None
        }
    }
}

fn parse_item(item: &Value, status: &str, allow_message_text: bool) -> Vec<ProcessStep> {
    let ity = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match ity {
        "agent_message" | "message" => {
            if !allow_message_text {
                return vec![];
            }
            let text = item
                .get("text")
                .and_then(|t| t.as_str())
                .or_else(|| item.pointer("/content/0/text").and_then(|t| t.as_str()))
                .unwrap_or("");
            if text.is_empty() {
                vec![]
            } else {
                vec![ProcessStep::Text {
                    text: text.to_string(),
                }]
            }
        }
        "reasoning" | "thought" => {
            let text = item
                .get("text")
                .or_else(|| item.get("content"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            if text.is_empty() {
                vec![]
            } else {
                vec![ProcessStep::Thinking {
                    text: text.to_string(),
                    done: status == "end",
                }]
            }
        }
        "command_execution" | "shell" | "function_call" | "tool_call" => {
            let name = item
                .get("name")
                .and_then(|n| n.as_str())
                .or(Some(ity))
                .unwrap_or("tool")
                .to_string();
            let id = item
                .get("id")
                .or_else(|| item.get("call_id"))
                .and_then(|i| i.as_str())
                .map(|s| s.to_string());
            let input = item
                .get("command")
                .cloned()
                .or_else(|| item.get("arguments").cloned())
                .or_else(|| item.get("input").cloned());
            let result = item
                .get("aggregated_output")
                .or_else(|| item.get("output"))
                .or_else(|| item.get("result"))
                .map(|o| match o {
                    Value::String(s) => truncate(s, 800),
                    other => truncate(&other.to_string(), 800),
                });
            vec![ProcessStep::Tool {
                id,
                name,
                input,
                status: status.into(),
                result,
            }]
        }
        "file_change" | "apply_patch" | "patch" => {
            let path = item
                .get("path")
                .or_else(|| item.pointer("/changes/0/path"))
                .and_then(|p| p.as_str())
                .unwrap_or("file");
            vec![ProcessStep::Tool {
                id: item
                    .get("id")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string()),
                name: ity.to_string(),
                input: Some(serde_json::json!({ "path": path })),
                status: status.into(),
                result: None,
            }]
        }
        _ => {
            if !ity.is_empty() {
                vec![ProcessStep::Status {
                    phase: status.into(),
                    detail: Some(format!("item:{ity}")),
                }]
            } else {
                vec![]
            }
        }
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
    fn parses_command_execution() {
        let line = r#"{"type":"item.started","item":{"type":"command_execution","command":"ls","id":"1"}}"#;
        let steps = parse_line(line).unwrap();
        assert!(matches!(
            &steps[0],
            ProcessStep::Tool { name, status, .. } if name == "command_execution" && status == "start"
        ));
    }

    #[test]
    fn parses_agent_message_completed() {
        let line =
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"hello world"}}"#;
        let steps = parse_line(line).unwrap();
        assert_eq!(
            steps,
            vec![ProcessStep::Text {
                text: "hello world".into()
            }]
        );
    }

    #[test]
    fn agent_message_updated_does_not_emit_text() {
        let line = r#"{"type":"item.updated","item":{"type":"agent_message","text":"partial"}}"#;
        let steps = parse_line(line).unwrap();
        assert!(steps.is_empty());
    }
}
