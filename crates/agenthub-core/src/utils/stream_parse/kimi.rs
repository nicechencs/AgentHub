//! Best-effort parser for `kimi -p --output-format stream-json` NDJSON.
//!
//! Schema is version-tolerant and deliberately overlaps Claude-style
//! assistant/tool blocks plus generic message/event envelopes.

use serde_json::Value;

use crate::models::ProcessStep;

pub fn parse_line(line: &str) -> Option<Vec<ProcessStep>> {
    let v: Value = serde_json::from_str(line).ok()?;
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match ty {
        "system" | "status" | "session" | "init" => Some(vec![ProcessStep::Status {
            phase: "starting".into(),
            detail: Some(ty.into()),
        }]),
        "assistant" | "message" => Some(parse_contentful(&v)),
        "user" => Some(parse_tool_results(&v)),
        "result" | "final" | "done" => Some(vec![ProcessStep::Status {
            phase: "result".into(),
            detail: Some(ty.into()),
        }]),
        "error" => {
            let message = v
                .get("error")
                .and_then(|e| e.as_str())
                .or_else(|| v.get("message").and_then(|m| m.as_str()))
                .unwrap_or("error")
                .to_string();
            Some(vec![ProcessStep::Error { message }])
        }
        "tool_use" | "tool_call" => Some(vec![tool_from(&v, "start")]),
        "tool_result" => Some(vec![tool_result_from(&v)]),
        "thinking" | "reasoning" => {
            let text = v
                .get("text")
                .or_else(|| v.get("thinking"))
                .or_else(|| v.get("content"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            if text.is_empty() {
                Some(vec![])
            } else {
                Some(vec![ProcessStep::Thinking {
                    text: text.into(),
                    done: false,
                }])
            }
        }
        "content_block_delta" => {
            let delta = v.get("delta")?;
            let dty = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if dty.contains("text") {
                let t = delta.get("text").and_then(|t| t.as_str()).unwrap_or("");
                if t.is_empty() {
                    Some(vec![])
                } else {
                    Some(vec![ProcessStep::Text { text: t.into() }])
                }
            } else if dty.contains("thinking") {
                let t = delta
                    .get("thinking")
                    .or_else(|| delta.get("text"))
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
            } else {
                None
            }
        }
        _ => {
            if v.get("message").is_some() || v.get("content").is_some() {
                let steps = parse_contentful(&v);
                if steps.is_empty() {
                    None
                } else {
                    Some(steps)
                }
            } else {
                None
            }
        }
    }
}

fn parse_contentful(v: &Value) -> Vec<ProcessStep> {
    let mut steps = Vec::new();
    let content = v
        .pointer("/message/content")
        .or_else(|| v.get("content"))
        .cloned()
        .unwrap_or(Value::Null);

    if let Some(arr) = content.as_array() {
        for block in arr {
            let bty = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match bty {
                "text" => {
                    if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                        if !t.is_empty() {
                            steps.push(ProcessStep::Text { text: t.into() });
                        }
                    }
                }
                "thinking" | "reasoning" => {
                    let t = block
                        .get("thinking")
                        .or_else(|| block.get("text"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("");
                    if !t.is_empty() {
                        steps.push(ProcessStep::Thinking {
                            text: t.into(),
                            done: false,
                        });
                    }
                }
                "tool_use" | "tool_call" => steps.push(tool_from(block, "start")),
                "tool_result" => steps.push(tool_result_from(block)),
                _ => {}
            }
        }
    } else if let Some(t) = v.get("text").and_then(|t| t.as_str()) {
        if !t.is_empty() {
            steps.push(ProcessStep::Text { text: t.into() });
        }
    } else if let Some(t) = content.as_str() {
        if !t.is_empty() {
            steps.push(ProcessStep::Text { text: t.into() });
        }
    }
    steps
}

fn parse_tool_results(v: &Value) -> Vec<ProcessStep> {
    let mut steps = Vec::new();
    let content = v
        .pointer("/message/content")
        .or_else(|| v.get("content"))
        .cloned()
        .unwrap_or(Value::Null);
    if let Some(arr) = content.as_array() {
        for block in arr {
            if matches!(
                block.get("type").and_then(|t| t.as_str()),
                Some("tool_result")
            ) {
                steps.push(tool_result_from(block));
            }
        }
    }
    steps
}

fn tool_from(v: &Value, status: &str) -> ProcessStep {
    ProcessStep::Tool {
        id: v
            .get("id")
            .or_else(|| v.get("tool_use_id"))
            .and_then(|i| i.as_str())
            .map(|s| s.to_string()),
        name: v
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("tool")
            .to_string(),
        input: v.get("input").or_else(|| v.get("arguments")).cloned(),
        status: status.into(),
        result: None,
    }
}

fn tool_result_from(v: &Value) -> ProcessStep {
    let result = v.get("content").map(|c| match c {
        Value::String(s) => truncate(s, 800),
        other => truncate(&other.to_string(), 800),
    });
    let is_err = v.get("is_error").and_then(|b| b.as_bool()).unwrap_or(false);
    ProcessStep::Tool {
        id: v
            .get("tool_use_id")
            .or_else(|| v.get("id"))
            .and_then(|i| i.as_str())
            .map(|s| s.to_string()),
        name: v
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("tool")
            .to_string(),
        input: None,
        status: if is_err { "error".into() } else { "end".into() },
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
    fn parses_assistant_text_and_tool() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"},{"type":"tool_use","id":"1","name":"Bash","input":{"cmd":"ls"}}]}}"#;
        let steps = parse_line(line).unwrap();
        assert!(steps
            .iter()
            .any(|s| matches!(s, ProcessStep::Text { text } if text == "hi")));
        assert!(steps
            .iter()
            .any(|s| matches!(s, ProcessStep::Tool { name, .. } if name == "Bash")));
    }
}
