//! Best-effort parser for `claude -p --output-format stream-json` NDJSON lines.
//!
//! Schema varies by CLI version; unknown fields are ignored. Unrecognized JSON
//! objects become a single [`ProcessStep::Raw`].
//!
//! Claude emits both `assistant` message text and a final `result` string with
//! the same answer. Prefer streamed assistant / delta text; only use `result`
//! text as a fallback when no assistant text was seen in this session (same
//! pattern as Pi `message_end` / Kiro `runFinished.finalText`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;

use crate::models::{AgentId, ProcessStep};
use crate::platform::stream::StreamParser;
use crate::platform::AgentKey;

/// Stateful Claude NDJSON decoder. Flags are per session (see [`StreamParser::for_session`]).
pub struct ClaudeStreamParser {
    saw_text: AtomicBool,
}

impl ClaudeStreamParser {
    pub fn new() -> Self {
        Self {
            saw_text: AtomicBool::new(false),
        }
    }

    fn saw_text(&self) -> bool {
        self.saw_text.load(Ordering::Relaxed)
    }

    fn mark_text(&self) {
        self.saw_text.store(true, Ordering::Relaxed);
    }

    fn parse(&self, line: &str) -> Option<Vec<ProcessStep>> {
        let v: Value = serde_json::from_str(line).ok()?;
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match ty {
            "system" => {
                let subtype = v
                    .get("subtype")
                    .and_then(|s| s.as_str())
                    .unwrap_or("system");
                Some(vec![ProcessStep::Status {
                    phase: "starting".into(),
                    detail: Some(subtype.into()),
                }])
            }
            "assistant" => Some(self.parse_assistant_message(&v)),
            "user" => Some(parse_user_tool_results(&v)),
            "result" => Some(self.parse_result(&v)),
            "content_block_delta" | "stream_event" => self.parse_deltaish(&v),
            "tool_use" => Some(vec![tool_from_obj(&v, "start")]),
            "tool_result" => Some(vec![tool_result_from_obj(&v)]),
            "error" => {
                let message = v
                    .get("error")
                    .and_then(|e| e.as_str())
                    .or_else(|| v.get("message").and_then(|m| m.as_str()))
                    .unwrap_or("error")
                    .to_string();
                Some(vec![ProcessStep::Error { message }])
            }
            _ => {
                // Nested message content still worth trying.
                if v.get("message").is_some() {
                    let steps = self.parse_assistant_message(&v);
                    if !steps.is_empty() {
                        return Some(steps);
                    }
                }
                None
            }
        }
    }

    fn parse_result(&self, v: &Value) -> Vec<ProcessStep> {
        let subtype = v
            .get("subtype")
            .and_then(|s| s.as_str())
            .unwrap_or("result");
        let is_err = subtype.contains("error")
            || v.get("is_error").and_then(|b| b.as_bool()).unwrap_or(false);
        if is_err {
            let msg = v
                .get("error")
                .and_then(|e| e.as_str())
                .or_else(|| v.get("result").and_then(|r| r.as_str()))
                .unwrap_or(subtype)
                .to_string();
            return vec![ProcessStep::Error { message: msg }];
        }
        let mut steps = vec![ProcessStep::Status {
            phase: "result".into(),
            detail: Some(subtype.into()),
        }];
        // Fallback final answer when stream omitted assistant text blocks.
        if !self.saw_text() {
            if let Some(t) = v.get("result").and_then(|r| r.as_str()) {
                if !t.is_empty() {
                    self.mark_text();
                    steps.push(ProcessStep::Text { text: t.into() });
                }
            }
        }
        steps
    }

    fn parse_assistant_message(&self, v: &Value) -> Vec<ProcessStep> {
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
                                self.mark_text();
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
                    "tool_use" => steps.push(tool_from_obj(block, "start")),
                    _ => {}
                }
            }
        } else if let Some(t) = v.get("text").and_then(|t| t.as_str()) {
            if !t.is_empty() {
                self.mark_text();
                steps.push(ProcessStep::Text { text: t.into() });
            }
        }
        steps
    }

    fn parse_deltaish(&self, v: &Value) -> Option<Vec<ProcessStep>> {
        // content_block_delta: { delta: { type: text_delta, text } }
        if let Some(delta) = v.get("delta") {
            let dty = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if dty == "text_delta" || dty == "text" {
                if let Some(t) = delta.get("text").and_then(|t| t.as_str()) {
                    if !t.is_empty() {
                        self.mark_text();
                        return Some(vec![ProcessStep::Text { text: t.into() }]);
                    }
                }
            }
            if dty.contains("thinking") {
                if let Some(t) = delta
                    .get("thinking")
                    .or_else(|| delta.get("text"))
                    .and_then(|t| t.as_str())
                {
                    if !t.is_empty() {
                        return Some(vec![ProcessStep::Thinking {
                            text: t.into(),
                            done: false,
                        }]);
                    }
                }
            }
        }
        // stream_event may wrap event
        if let Some(ev) = v.get("event") {
            return self.parse(&ev.to_string());
        }
        None
    }
}

impl Default for ClaudeStreamParser {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamParser for ClaudeStreamParser {
    fn agent_key(&self) -> AgentKey {
        AgentKey::from_agent_id(AgentId::Claude)
    }

    fn parse_line(&self, line: &str) -> Option<Vec<ProcessStep>> {
        self.parse(line)
    }

    fn for_session(&self) -> Option<Arc<dyn StreamParser>> {
        Some(Arc::new(ClaudeStreamParser::new()))
    }
}

/// Stateless convenience for unit tests / ChatRuntime one-shot decode.
///
/// Each call uses a fresh parser, so `result` text is always emitted here.
/// ChatRuntime already treats `result` text as fallback when the bubble is
/// non-empty; print-path Chat uses [`ClaudeStreamParser`] via `for_session`.
#[allow(dead_code)]
pub fn parse_line(line: &str) -> Option<Vec<ProcessStep>> {
    ClaudeStreamParser::new().parse(line)
}

fn parse_user_tool_results(v: &Value) -> Vec<ProcessStep> {
    let mut steps = Vec::new();
    let content = v
        .pointer("/message/content")
        .or_else(|| v.get("content"))
        .cloned()
        .unwrap_or(Value::Null);
    if let Some(arr) = content.as_array() {
        for block in arr {
            if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                steps.push(tool_result_from_obj(block));
            }
        }
    }
    steps
}

fn tool_from_obj(v: &Value, status: &str) -> ProcessStep {
    let id = v
        .get("id")
        .or_else(|| v.get("tool_use_id"))
        .and_then(|i| i.as_str())
        .map(|s| s.to_string());
    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("tool")
        .to_string();
    let input = v.get("input").cloned();
    ProcessStep::Tool {
        id,
        name,
        input,
        status: status.into(),
        result: None,
    }
}

fn tool_result_from_obj(v: &Value) -> ProcessStep {
    let id = v
        .get("tool_use_id")
        .or_else(|| v.get("id"))
        .and_then(|i| i.as_str())
        .map(|s| s.to_string());
    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("tool")
        .to_string();
    let result = v
        .get("content")
        .map(|c| match c {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .or_else(|| {
            v.get("output")
                .and_then(|o| o.as_str())
                .map(|s| s.to_string())
        });
    let is_err = v.get("is_error").and_then(|b| b.as_bool()).unwrap_or(false);
    ProcessStep::Tool {
        id,
        name,
        input: None,
        status: if is_err { "error".into() } else { "end".into() },
        result: result.map(|s| {
            if s.chars().count() > 800 {
                let head: String = s.chars().take(800).collect();
                format!("{head}…")
            } else {
                s
            }
        }),
    }
}

#[cfg(test)]
mod tests;
