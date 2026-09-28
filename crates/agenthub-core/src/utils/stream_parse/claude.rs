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
            "tool_use" => Some(if is_claude_plan_tool_name(tool_name(&v)) {
                vec![]
            } else {
                vec![tool_from_obj(&v, "start")]
            }),
            "tool_result" => Some(if is_claude_plan_result(&v) {
                vec![]
            } else {
                vec![tool_result_from_obj(&v)]
            }),
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
                    "tool_use" => {
                        if !is_claude_plan_tool_name(tool_name(block)) {
                            steps.push(tool_from_obj(block, "start"));
                        }
                    }
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
    let parent_plan_result = looks_like_plan_output(v);
    let content = v
        .pointer("/message/content")
        .or_else(|| v.get("content"))
        .cloned()
        .unwrap_or(Value::Null);
    if let Some(arr) = content.as_array() {
        for block in arr {
            if block.get("type").and_then(|t| t.as_str()) != Some("tool_result") {
                continue;
            }
            if is_claude_plan_result(block) || (parent_plan_result && tool_name(block).is_empty()) {
                continue;
            }
            steps.push(tool_result_from_obj(block));
        }
    }
    steps
}

fn tool_name(v: &Value) -> &str {
    v.get("name").and_then(|name| name.as_str()).unwrap_or("")
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

/// One Claude todo / Task tool row. Not a `ProcessStep`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClaudePlanEntry {
    pub content: String,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub id: Option<String>,
}

/// Mutations from TodoWrite / Task* tool_use and matching results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClaudePlanOp {
    Replace(Vec<ClaudePlanEntry>),
    Create {
        tool_use_id: Option<String>,
        content: String,
        status: Option<String>,
        id: Option<String>,
        priority: Option<String>,
    },
    Update {
        id: String,
        status: Option<String>,
        content: Option<String>,
        priority: Option<String>,
    },
    BindId {
        tool_use_id: String,
        id: String,
    },
}

/// TodoWrite replaces the list; TaskCreate/TaskUpdate patch it. Not a process row.
pub(crate) fn extract_todo_plan(v: &Value) -> Vec<ClaudePlanOp> {
    if let Some(event) = v.get("event") {
        let nested = extract_todo_plan(event);
        if !nested.is_empty() {
            return nested;
        }
    }
    let mut ops = Vec::new();
    for_each_tool_use(v, |block| {
        if let Some(op) = plan_op_from_tool_use(block) {
            ops.push(op);
        }
    });
    ops.extend(plan_ops_from_results(v));
    ops
}

pub(crate) fn is_claude_plan_tool_name(name: &str) -> bool {
    plan_tool_kind(name).is_some()
}

pub(crate) fn plan_tool_use_ids(v: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    for_each_tool_use(v, |block| {
        if !is_claude_plan_tool_name(tool_name(block)) {
            return;
        }
        if let Some(id) = first_str(block, &["id", "tool_use_id"]) {
            ids.push(id);
        }
    });
    ids
}

fn is_claude_plan_result(v: &Value) -> bool {
    is_claude_plan_tool_name(tool_name(v)) || looks_like_plan_output(v)
}

fn plan_tool_kind(name: &str) -> Option<&'static str> {
    let compact = name
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '-', ' '], "");
    match compact.as_str() {
        "todowrite" => Some("write"),
        "todoread" => Some("read"),
        "taskcreate" => Some("create"),
        "taskupdate" => Some("update"),
        "taskget" => Some("get"),
        "tasklist" => Some("list"),
        _ => None,
    }
}

fn for_each_tool_use(v: &Value, mut visit: impl FnMut(&Value)) {
    let ty = v.get("type").and_then(Value::as_str).unwrap_or("");
    if ty == "tool_use" {
        visit(v);
        return;
    }
    if let Some(block) = v.get("content_block") {
        if block.get("type").and_then(Value::as_str) == Some("tool_use") {
            visit(block);
        }
    }
    let content = v.pointer("/message/content").or_else(|| v.get("content"));
    if let Some(items) = content.and_then(Value::as_array) {
        for block in items {
            if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                visit(block);
            }
        }
    }
}

fn plan_op_from_tool_use(block: &Value) -> Option<ClaudePlanOp> {
    let kind = plan_tool_kind(tool_name(block))?;
    let input = block.get("input").unwrap_or(&Value::Null);
    match kind {
        "write" => {
            let entries = input
                .get("todos")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(parse_plan_entry)
                .collect::<Vec<_>>();
            Some(ClaudePlanOp::Replace(entries))
        }
        "create" => {
            let content = first_str(input, &["subject", "content", "text", "title"])?;
            Some(ClaudePlanOp::Create {
                tool_use_id: first_str(block, &["id", "tool_use_id"]),
                content,
                status: first_str(input, &["status"]).or_else(|| Some("pending".into())),
                id: first_str(input, &["taskId", "id", "task_id"]),
                priority: first_str(input, &["priority"]),
            })
        }
        "update" => {
            let id = first_str(input, &["taskId", "id", "task_id"])?;
            Some(ClaudePlanOp::Update {
                id,
                status: first_str(input, &["status"]),
                content: first_str(input, &["subject", "content", "text", "title"]),
                priority: first_str(input, &["priority"]),
            })
        }
        _ => None,
    }
}

fn plan_ops_from_results(v: &Value) -> Vec<ClaudePlanOp> {
    let mut ops = Vec::new();
    let parent_result = v
        .get("tool_use_result")
        .or_else(|| v.pointer("/message/tool_use_result"));
    let content = v.pointer("/message/content").or_else(|| v.get("content"));
    if let Some(items) = content.and_then(Value::as_array) {
        for block in items {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            collect_result_ops(block, parent_result, &mut ops);
        }
    } else if v.get("type").and_then(Value::as_str) == Some("tool_result") {
        collect_result_ops(v, parent_result, &mut ops);
    }
    ops
}

fn collect_result_ops(block: &Value, parent_result: Option<&Value>, ops: &mut Vec<ClaudePlanOp>) {
    if let Some(tool_use_id) = first_str(block, &["tool_use_id"]) {
        if let Some(id) = task_id_from(parent_result).or_else(|| task_id_from(Some(block))) {
            ops.push(ClaudePlanOp::BindId { tool_use_id, id });
        }
    }
    if let Some(entries) = tasks_from_payload(parent_result).or_else(|| tasks_from_value(block)) {
        if !entries.is_empty() {
            ops.push(ClaudePlanOp::Replace(entries));
        }
    }
}

fn task_id_from(payload: Option<&Value>) -> Option<String> {
    payload
        .and_then(structured_task_output)?
        .pointer("/task/id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn tasks_from_payload(payload: Option<&Value>) -> Option<Vec<ClaudePlanEntry>> {
    let parsed = payload.and_then(structured_task_output)?;
    let items = parsed.get("tasks").and_then(Value::as_array)?;
    let entries = items
        .iter()
        .filter_map(parse_plan_entry)
        .collect::<Vec<_>>();
    (!entries.is_empty()).then_some(entries)
}

fn looks_like_plan_output(v: &Value) -> bool {
    payload_has_plan_shape(v)
        || v.get("tool_use_result").is_some_and(payload_has_plan_shape)
        || v.pointer("/message/tool_use_result")
            .is_some_and(payload_has_plan_shape)
        || v.get("content")
            .and_then(parse_jsonish)
            .is_some_and(|value| payload_has_plan_shape(&value))
        || v.get("output")
            .and_then(parse_jsonish)
            .is_some_and(|value| payload_has_plan_shape(&value))
}

fn payload_has_plan_shape(v: &Value) -> bool {
    v.get("task").is_some()
        || v.get("tasks").is_some()
        || v.get("oldTodos").is_some()
        || v.get("newTodos").is_some()
        || v.get("old_todos").is_some()
        || v.get("new_todos").is_some()
        || v.get("updatedFields").is_some()
        || v.get("updated_fields").is_some()
        || ((v.get("taskId").is_some() || v.get("task_id").is_some())
            && (v.get("success").is_some() || v.get("status").is_some()))
}

fn structured_task_output(v: &Value) -> Option<Value> {
    if payload_has_plan_shape(v) {
        return Some(v.clone());
    }
    if let Some(direct) = v.get("tool_use_result") {
        if payload_has_plan_shape(direct) {
            return Some(direct.clone());
        }
    }
    let content = v.get("content").or_else(|| v.get("output"))?;
    parse_jsonish(content).filter(payload_has_plan_shape)
}

fn tasks_from_value(v: &Value) -> Option<Vec<ClaudePlanEntry>> {
    let parsed = structured_task_output(v)?;
    let items = parsed.get("tasks").and_then(Value::as_array)?;
    let entries = items
        .iter()
        .filter_map(parse_plan_entry)
        .collect::<Vec<_>>();
    (!entries.is_empty()).then_some(entries)
}

fn parse_plan_entry(value: &Value) -> Option<ClaudePlanEntry> {
    let content = first_str(value, &["content", "subject", "text", "title"])?;
    Some(ClaudePlanEntry {
        content,
        status: first_str(value, &["status"]),
        priority: first_str(value, &["priority"]),
        id: first_str(value, &["id", "taskId", "task_id"]),
    })
}

fn parse_jsonish(content: &Value) -> Option<Value> {
    match content {
        Value::Object(_) => Some(content.clone()),
        Value::String(text) => serde_json::from_str(text).ok(),
        Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(|item| {
                    if item.get("type").and_then(Value::as_str) == Some("text") {
                        item.get("text").and_then(Value::as_str)
                    } else {
                        None
                    }
                })
                .collect::<String>();
            if text.trim().is_empty() {
                None
            } else {
                serde_json::from_str(&text).ok()
            }
        }
        _ => None,
    }
}

fn first_str(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            return Some(text.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// `--include-partial-messages` support (verified against Claude Code 2.1.283).
//
// With partial messages on, stdout carries raw Anthropic stream events wrapped as
// `{"type":"stream_event","event":{...},"parent_tool_use_id":null,...}`:
// `message_start` (carries `message.id`) → `content_block_start {index, content_block}`
// → `content_block_delta {index, delta:{type:"text_delta",text} | {type:"thinking_delta",
// thinking} | {type:"input_json_delta",partial_json} | {type:"signature_delta"}}` →
// `content_block_stop` → `message_delta` → `message_stop`.
//
// Claude Code still emits the complete `assistant` line, one per finished content block
// (each carries a single block and the same `message.id`), right before that block's
// `content_block_stop`. On interrupt it emits a snapshot `assistant` line holding the text
// streamed so far. Callers that render deltas must therefore strip already-streamed text
// from those `assistant` lines ([`ClaudePartialDedup`]).
// ---------------------------------------------------------------------------

/// Inner Anthropic event of a `stream_event` line.
pub fn partial_event(value: &Value) -> Option<&Value> {
    if value.get("type").and_then(Value::as_str) != Some("stream_event") {
        return None;
    }
    value.get("event")
}

fn partial_delta<'a>(value: &'a Value, delta_type: &str) -> Option<&'a Value> {
    let event = partial_event(value)?;
    if event.get("type").and_then(Value::as_str) != Some("content_block_delta") {
        return None;
    }
    let delta = event.get("delta")?;
    (delta.get("type").and_then(Value::as_str) == Some(delta_type)).then_some(delta)
}

/// Non-empty text of a `stream_event` / `content_block_delta` / `text_delta` line.
pub fn partial_text_delta(value: &Value) -> Option<String> {
    partial_delta(value, "text_delta")?
        .get("text")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// Non-empty thinking of a `stream_event` / `content_block_delta` / `thinking_delta` line.
///
/// Claude Code may send empty `thinking` (display mode `updates` / redacted summaries);
/// those return `None`.
pub fn partial_thinking_delta(value: &Value) -> Option<String> {
    partial_delta(value, "thinking_delta")?
        .get("thinking")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// `message.id` announced by a `stream_event` / `message_start` line.
pub fn partial_message_start_id(value: &Value) -> Option<String> {
    let event = partial_event(value)?;
    if event.get("type").and_then(Value::as_str) != Some("message_start") {
        return None;
    }
    event
        .pointer("/message/id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// `message.id` of a complete `assistant` line.
pub fn assistant_message_id(value: &Value) -> Option<String> {
    if value.get("type").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    value
        .pointer("/message/id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Line belongs to a sub-agent (Task tool) rather than the main conversation.
pub fn is_subagent_line(value: &Value) -> bool {
    value
        .get("parent_tool_use_id")
        .is_some_and(|p| !p.is_null())
}

/// `result` produced by a `control_request` `interrupt`.
///
/// Claude Code reports it as `subtype:"error_during_execution"`, `is_error:true`,
/// `terminal_reason:"aborted_streaming"` (no `result` text). Callers should treat it as
/// cancelled, not failed, once they have sent an interrupt.
pub fn is_interrupted_result(value: &Value) -> bool {
    value.get("type").and_then(Value::as_str) == Some("result")
        && value
            .get("terminal_reason")
            .and_then(Value::as_str)
            .is_some_and(|r| r.starts_with("aborted"))
}

/// NDJSON line (no trailing newline) that asks a stream-json Claude process to stop the
/// current turn. The process answers with
/// `{"type":"control_response","response":{"subtype":"success","request_id":..,...}}`,
/// then a snapshot `assistant` line, a `user` "[Request interrupted by user]" line and an
/// interrupted `result`; it stays alive for the next user message.
pub fn interrupt_request_line(request_id: &str) -> String {
    serde_json::json!({
        "type": "control_request",
        "request_id": request_id,
        "request": { "subtype": "interrupt" },
    })
    .to_string()
}

/// `request_id` of a successful `control_response` line.
#[allow(dead_code)] // the transport skips control_response lines; kept for diagnostics and tests
pub fn control_response_success_id(value: &Value) -> Option<String> {
    if value.get("type").and_then(Value::as_str) != Some("control_response") {
        return None;
    }
    let response = value.get("response")?;
    if response.get("subtype").and_then(Value::as_str) != Some("success") {
        return None;
    }
    response
        .get("request_id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartialBlockKind {
    Text,
    Thinking,
}

impl PartialBlockKind {
    fn from_block_type(ty: &str) -> Option<Self> {
        match ty {
            "text" => Some(Self::Text),
            "thinking" => Some(Self::Thinking),
            _ => None,
        }
    }

    fn field(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Thinking => "thinking",
        }
    }
}

#[derive(Debug)]
struct PartialBlock {
    index: u64,
    kind: PartialBlockKind,
    streamed: String,
    consumed: bool,
}

/// Per-conversation dedup between partial deltas and complete `assistant` lines.
///
/// Feed every stdout JSON value in order to [`ClaudePartialDedup::filter`] and hand the
/// returned value (if any) to [`parse_line`]. Without partial events it is a pass-through.
#[derive(Debug, Default)]
pub struct ClaudePartialDedup {
    message_id: Option<String>,
    blocks: Vec<PartialBlock>,
}

impl ClaudePartialDedup {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget streamed state (new turn, cancel, process restart).
    pub fn reset(&mut self) {
        self.message_id = None;
        self.blocks.clear();
    }

    /// Returns what to pass to [`parse_line`], or `None` to drop the line.
    ///
    /// - `stream_event` text / thinking deltas: recorded and passed through (non-empty only;
    ///   `parse_line` already renders them as `Text` / `Thinking`).
    /// - other `stream_event` lines (`message_start`, `content_block_start/stop`,
    ///   `input_json_delta`, `signature_delta`, `message_delta`, `message_stop`): recorded
    ///   where relevant and dropped.
    /// - `assistant` with the streamed `message.id`: text / thinking blocks lose the prefix
    ///   already streamed (empty blocks are removed; tool_use blocks are kept). `None` when
    ///   nothing is left.
    /// - `result`: clears state and passes through.
    /// - anything else: passed through unchanged.
    pub fn filter(&mut self, value: &Value) -> Option<Value> {
        if let Some(event) = partial_event(value) {
            return self.observe_stream_event(value, event);
        }
        match value.get("type").and_then(Value::as_str) {
            Some("assistant") => self.strip_assistant(value),
            Some("result") => {
                self.reset();
                Some(value.clone())
            }
            _ => Some(value.clone()),
        }
    }

    fn observe_stream_event(&mut self, line: &Value, event: &Value) -> Option<Value> {
        match event.get("type").and_then(Value::as_str).unwrap_or("") {
            "message_start" => {
                self.reset();
                self.message_id = partial_message_start_id(line);
                None
            }
            "content_block_start" => {
                let index = event.get("index").and_then(Value::as_u64);
                let kind = event
                    .pointer("/content_block/type")
                    .and_then(Value::as_str)
                    .and_then(PartialBlockKind::from_block_type);
                if let (Some(index), Some(kind)) = (index, kind) {
                    self.blocks.retain(|b| b.index != index);
                    self.blocks.push(PartialBlock {
                        index,
                        kind,
                        streamed: String::new(),
                        consumed: false,
                    });
                }
                None
            }
            "content_block_delta" => {
                let (kind, text) = partial_text_delta(line)
                    .map(|t| (PartialBlockKind::Text, t))
                    .or_else(|| {
                        partial_thinking_delta(line).map(|t| (PartialBlockKind::Thinking, t))
                    })?;
                let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);
                match self.blocks.iter_mut().find(|b| b.index == index) {
                    Some(block) if block.kind == kind => block.streamed.push_str(&text),
                    Some(_) => {}
                    None => self.blocks.push(PartialBlock {
                        index,
                        kind,
                        streamed: text,
                        consumed: false,
                    }),
                }
                Some(line.clone())
            }
            _ => None,
        }
    }

    fn strip_assistant(&mut self, value: &Value) -> Option<Value> {
        let Some(id) = assistant_message_id(value) else {
            return Some(value.clone());
        };
        if self.message_id.as_deref() != Some(id.as_str()) || self.blocks.is_empty() {
            return Some(value.clone());
        }
        let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else {
            return Some(value.clone());
        };
        let mut kept = Vec::with_capacity(content.len());
        for block in content {
            let kind = block
                .get("type")
                .and_then(Value::as_str)
                .and_then(PartialBlockKind::from_block_type);
            let Some(kind) = kind else {
                kept.push(block.clone());
                continue;
            };
            let full = block
                .get(kind.field())
                .and_then(Value::as_str)
                .unwrap_or("");
            let Some(streamed) = self
                .blocks
                .iter_mut()
                .find(|b| b.kind == kind && !b.consumed)
            else {
                if !full.is_empty() {
                    kept.push(block.clone());
                }
                continue;
            };
            streamed.consumed = true;
            // Streamed text is authoritative; only the unseen tail (if any) is new.
            let rest = full.strip_prefix(streamed.streamed.as_str()).unwrap_or("");
            if !rest.is_empty() {
                let mut tail = block.clone();
                tail[kind.field()] = Value::String(rest.to_string());
                kept.push(tail);
            }
        }
        if kept.is_empty() {
            return None;
        }
        let mut out = value.clone();
        out["message"]["content"] = Value::Array(kept);
        Some(out)
    }
}

#[cfg(test)]
mod tests;
