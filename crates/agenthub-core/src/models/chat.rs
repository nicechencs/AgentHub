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
        }
    }
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

#[cfg(test)]
mod tests;
