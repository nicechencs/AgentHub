//! Chat conversation / message payloads + streaming events.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::AgentId;

/// Message role in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Agent,
}

impl ChatRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "user" => Some(Self::User),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }
}

/// Per-message status (agent replies; user messages stay `ok`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatMessageStatus {
    Ok,
    Failed,
    Timeout,
    Skipped,
    Running,
    Cancelled,
}

impl ChatMessageStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::Timeout => "timeout",
            Self::Skipped => "skipped",
            Self::Running => "running",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ok" => Some(Self::Ok),
            "failed" => Some(Self::Failed),
            "timeout" => Some(Self::Timeout),
            "skipped" => Some(Self::Skipped),
            "running" => Some(Self::Running),
            "cancelled" | "canceled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// A chat conversation (1..N agents).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub agent_ids: Vec<AgentId>,
    pub cwd: Option<String>,
    pub allow_dangerous: bool,
    pub created_at: String,
    pub updated_at: String,
    /// Official CLI session id captured from the last print-mode run, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_session_id: Option<String>,
    /// Runtime projection: a live `status=running` message exists (not persisted).
    #[serde(default)]
    pub sending: bool,
    /// List projection: first user message body (not persisted). Recovers
    /// historically clipped titles on every rail row, not only the focused one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_user_content: Option<String>,
}

/// One imported transcript turn when opening a session by id.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistoryTurn {
    pub role: ChatRole,
    pub content: String,
}

/// One message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub conversation_id: String,
    /// Shared across the user message and all agent replies for one send.
    pub turn: i64,
    pub role: ChatRole,
    pub agent_id: Option<AgentId>,
    pub content: String,
    pub status: ChatMessageStatus,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub created_at: String,
}

/// Which pipe a streaming chunk came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

impl OutputStream {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

/// Normalized process step for Cursor-like process UI (wire format shared GUI/core).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ProcessStep {
    #[serde(rename_all = "camelCase")]
    Status {
        phase: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Thinking {
        text: String,
        #[serde(default)]
        done: bool,
    },
    #[serde(rename_all = "camelCase")]
    Tool {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input: Option<serde_json::Value>,
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Text { text: String },
    #[serde(rename_all = "camelCase")]
    Raw {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Error { message: String },
    /// Protocol token counts from the Agent. Absent fields were not sent.
    #[serde(rename_all = "camelCase")]
    Usage {
        /// `turn` = current turn (`last`); `session` = cumulative (`total`).
        /// Missing means current turn (Grok `turn_completed.usage`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_read: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_write: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_window: Option<u64>,
    },
}

impl ProcessStep {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Status { .. } => "status",
            Self::Thinking { .. } => "thinking",
            Self::Tool { .. } => "tool",
            Self::Text { .. } => "text",
            Self::Raw { .. } => "raw",
            Self::Error { .. } => "error",
            Self::Usage { .. } => "usage",
        }
    }

    /// Map a protocol usage object to a step. Skips all-zero / empty objects.
    ///
    /// Reads Codex `TokenUsageBreakdown` and Grok `turn_completed.usage`
    /// field names as sent. Does not subtract cache from input, convert cost,
    /// or invent a total.
    pub fn from_usage_object(usage: &serde_json::Value) -> Option<Self> {
        if let Some(step) = usage_from_fields(usage) {
            return Some(step);
        }
        let map = usage
            .get("modelUsage")
            .or_else(|| usage.get("model_usage"))
            .and_then(|v| v.as_object())?;
        if map.len() != 1 {
            return None;
        }
        usage_from_fields(map.values().next()?)
    }

    /// Codex `tokenUsage`: `last` is this turn, `total` is the thread cumulative.
    /// Does not invent a session total when only `last` is present.
    pub fn from_codex_token_usage(token_usage: &serde_json::Value) -> Vec<Self> {
        let window = token_u64(token_usage, &["modelContextWindow", "model_context_window"]);
        let mut out = Vec::new();
        if let Some(step) = token_usage.get("last").and_then(Self::from_usage_object) {
            out.push(step.with_usage_meta(Some("turn"), None));
        }
        if let Some(step) = token_usage.get("total").and_then(Self::from_usage_object) {
            out.push(step.with_usage_meta(Some("session"), window));
        }
        out
    }

    fn with_usage_meta(self, scope: Option<&str>, context_window: Option<u64>) -> Self {
        match self {
            Self::Usage {
                input,
                output,
                cache_read,
                cache_write,
                reasoning,
                total,
                ..
            } => Self::Usage {
                scope: scope.map(str::to_string),
                input,
                output,
                cache_read,
                cache_write,
                reasoning,
                total,
                context_window,
            },
            other => other,
        }
    }
}

fn usage_from_fields(usage: &serde_json::Value) -> Option<ProcessStep> {
    let input = token_u64(usage, &["inputTokens", "input_tokens"]);
    let output = token_u64(usage, &["outputTokens", "output_tokens"]);
    let cache_read = token_u64(
        usage,
        &[
            "cachedInputTokens",
            "cached_input_tokens",
            "cachedReadTokens",
            "cached_read_tokens",
        ],
    );
    let cache_write = token_u64(
        usage,
        &[
            "cacheWriteInputTokens",
            "cache_write_input_tokens",
            "cacheCreationTokens",
            "cache_creation_tokens",
        ],
    );
    let reasoning = token_u64(
        usage,
        &[
            "reasoningOutputTokens",
            "reasoning_output_tokens",
            "reasoningTokens",
            "reasoning_tokens",
        ],
    );
    let total = token_u64(usage, &["totalTokens", "total_tokens"]);
    if input.unwrap_or(0) == 0
        && output.unwrap_or(0) == 0
        && cache_read.unwrap_or(0) == 0
        && cache_write.unwrap_or(0) == 0
        && reasoning.unwrap_or(0) == 0
        && total.unwrap_or(0) == 0
    {
        return None;
    }
    Some(ProcessStep::Usage {
        scope: None,
        input,
        output,
        cache_read,
        cache_write,
        reasoning,
        total,
        context_window: None,
    })
}

fn token_u64(v: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        let Some(value) = v.get(*key) else {
            continue;
        };
        if let Some(n) = value.as_u64() {
            return Some(n);
        }
        if let Some(n) = value.as_i64().and_then(|n| u64::try_from(n).ok()) {
            return Some(n);
        }
        if let Some(n) = value.as_f64().and_then(|n| (n >= 0.0).then_some(n as u64)) {
            return Some(n);
        }
    }
    None
}

/// Streaming events for chat send (externally tagged; no Tauri types).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ChatEvent {
    #[serde(rename_all = "camelCase")]
    Started { turn: i64, agents: Vec<AgentId> },
    #[serde(rename_all = "camelCase")]
    AgentStarted {
        turn: i64,
        agent: AgentId,
        command: String,
    },
    #[serde(rename_all = "camelCase")]
    AgentChunk {
        turn: i64,
        agent: AgentId,
        stream: OutputStream,
        text: String,
    },
    /// Structured process step (tool / thinking / status). Phase 1+.
    #[serde(rename_all = "camelCase")]
    AgentProcess {
        turn: i64,
        agent: AgentId,
        step: ProcessStep,
    },
    #[serde(rename_all = "camelCase")]
    AgentFinished {
        turn: i64,
        agent: AgentId,
        message: ChatMessage,
    },
    #[serde(rename_all = "camelCase")]
    Finished {
        turn: i64,
        ok: bool,
        #[serde(default)]
        cancelled: bool,
    },
    #[serde(rename_all = "camelCase")]
    Error { message: String },
}

/// Process-level events used by `RunService::run_each` (no Tauri types).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum RunEvent {
    #[serde(rename_all = "camelCase")]
    Started { agent: AgentId, command: String },
    #[serde(rename_all = "camelCase")]
    Chunk {
        agent: AgentId,
        stream: OutputStream,
        text: String,
    },
    /// Decoded process step from structured CLI stdout.
    #[serde(rename_all = "camelCase")]
    Step { agent: AgentId, step: ProcessStep },
    #[serde(rename_all = "camelCase")]
    Finished { agent: AgentId },
}

/// Live Chat model chip + picker for an agent (Pi reads settings.json).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveChatModel {
    pub model: Option<String>,
    pub models: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub efforts: Vec<String>,
}

/// Local markdown file opened from a chat message link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownFilePreview {
    pub path: PathBuf,
    pub name: String,
    pub content: String,
    pub truncated: bool,
}

/// Persist a conversation title from the first user prompt.
/// Path tokens are dropped; the phrase is never clipped with an ellipsis.
pub fn conversation_title_from_prompt(prompt: &str) -> String {
    conversation_semantic_phrase(prompt)
}

fn conversation_semantic_phrase(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let without_ticks = strip_backtick_spans(trimmed);
    let without_paths = strip_path_tokens(&without_ticks);
    let collapsed = collapse_ws(&without_paths);
    let stripped = strip_weak_lead(&collapsed);
    if stripped.is_empty() || is_weak_only(&stripped) {
        String::new()
    } else {
        stripped
    }
}

/// Strip `` `code` `` spans. Mirrors the frontend's ``/`[^`]+`/g`` replace:
/// only closed pairs go away, and a lone backtick stays as written. The
/// adoption gate compares this string with the frontend's derivation, so both
/// implementations have to agree character for character.
fn strip_backtick_spans(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '`' {
            // `[^`]+` needs at least one non-backtick character inside.
            if let Some(close) = (i + 1..chars.len()).find(|&j| chars[j] == '`') {
                if close > i + 1 {
                    out.push(' ');
                    i = close + 1;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn strip_path_tokens(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if let Some(end) = match_path_token(&chars, i) {
            out.push(' ');
            i = end;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn match_path_token(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    if i + 1 < chars.len() && chars[i].is_ascii_alphabetic() && chars[i + 1] == ':' {
        i += 2;
    }
    let after_drive = i;
    let mut segments = 0;
    // Mirrors the frontend's `(?:[\\/][^\s\\/`'"]+)+`: a trailing separator is
    // not part of the token, so an empty segment ends the match at the last
    // complete one instead of discarding the whole path. The derived title has
    // to match the frontend character for character — see
    // `conversation_title_from_prompt` and its mirrored fixture.
    let mut end = after_drive;
    while i < chars.len() && (chars[i] == '/' || chars[i] == '\\') {
        i += 1;
        let seg_start = i;
        while i < chars.len() {
            let c = chars[i];
            if c.is_whitespace() || matches!(c, '`' | '\'' | '"' | '/' | '\\') {
                break;
            }
            i += 1;
        }
        if i == seg_start {
            break;
        }
        segments += 1;
        end = i;
    }
    if segments >= 1 && end > after_drive {
        Some(end)
    } else {
        None
    }
}

fn collapse_ws(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_weak_lead(input: &str) -> String {
    const LEADS: &[&str] = &["请帮我在", "请在", "请", "in", "at"];
    let lower = input.to_ascii_lowercase();
    for lead in LEADS {
        let lead_lower = lead.to_ascii_lowercase();
        if lower.starts_with(&lead_lower) {
            let rest = input.get(lead.len()..).unwrap_or("");
            if rest.starts_with(|c: char| c.is_whitespace()) {
                return rest.trim_start().to_string();
            }
        }
    }
    input.to_string()
}

fn is_weak_only(input: &str) -> bool {
    matches!(
        input.to_ascii_lowercase().as_str(),
        "请帮我在" | "请在" | "请" | "in" | "at" | "only"
    )
}

#[cfg(test)]
mod tests;
