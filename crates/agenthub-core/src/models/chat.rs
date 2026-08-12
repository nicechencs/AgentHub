//! Chat conversation / message payloads + streaming events.

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
    Finished { turn: i64, ok: bool },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_role_parse() {
        assert_eq!(ChatRole::parse("user"), Some(ChatRole::User));
        assert_eq!(ChatRole::parse("AGENT"), Some(ChatRole::Agent));
        assert_eq!(ChatRole::parse("system"), None);
    }

    #[test]
    fn chat_status_parse() {
        assert_eq!(
            ChatMessageStatus::parse("cancelled"),
            Some(ChatMessageStatus::Cancelled)
        );
        assert_eq!(
            ChatMessageStatus::parse("canceled"),
            Some(ChatMessageStatus::Cancelled)
        );
        assert_eq!(
            ChatMessageStatus::parse("running"),
            Some(ChatMessageStatus::Running)
        );
    }

    #[test]
    fn chat_event_serde_tag() {
        let ev = ChatEvent::Error {
            message: "boom".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains(r#""type":"error""#));
        assert!(json.contains(r#""message":"boom""#));
    }

    #[test]
    fn chat_event_stream_variants_include_turn_camel_case() {
        let chunk = ChatEvent::AgentChunk {
            turn: 3,
            agent: AgentId::Claude,
            stream: OutputStream::Stdout,
            text: "hi".into(),
        };
        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains(r#""type":"agentChunk""#));
        assert!(json.contains(r#""turn":3"#));
        assert!(json.contains(r#""stream":"stdout""#));
        let back: ChatEvent = serde_json::from_str(&json).unwrap();
        match back {
            ChatEvent::AgentChunk { turn, text, .. } => {
                assert_eq!(turn, 3);
                assert_eq!(text, "hi");
            }
            other => panic!("unexpected: {other:?}"),
        }

        let started = ChatEvent::AgentStarted {
            turn: 1,
            agent: AgentId::Grok,
            command: "grok -p".into(),
        };
        let s = serde_json::to_string(&started).unwrap();
        assert!(s.contains(r#""type":"agentStarted""#));
        assert!(s.contains(r#""turn":1"#));

        let step = ChatEvent::AgentProcess {
            turn: 2,
            agent: AgentId::Codex,
            step: ProcessStep::Tool {
                id: Some("t1".into()),
                name: "shell".into(),
                input: Some(serde_json::json!({"cmd":"ls"})),
                status: "start".into(),
                result: None,
            },
        };
        let js = serde_json::to_string(&step).unwrap();
        assert!(js.contains(r#""type":"agentProcess""#));
        assert!(js.contains(r#""name":"shell""#));
        let back: ChatEvent = serde_json::from_str(&js).unwrap();
        match back {
            ChatEvent::AgentProcess { turn, step, .. } => {
                assert_eq!(turn, 2);
                assert_eq!(step.kind(), "tool");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn chat_message_serde_camel_case() {
        let msg = ChatMessage {
            id: "m1".into(),
            conversation_id: "c1".into(),
            turn: 2,
            role: ChatRole::Agent,
            agent_id: Some(AgentId::Codex),
            content: "body".into(),
            status: ChatMessageStatus::Ok,
            exit_code: Some(0),
            duration_ms: 10,
            error: None,
            created_at: "t".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""conversationId":"c1""#));
        assert!(json.contains(r#""agentId":"codex""#));
        assert!(json.contains(r#""durationMs":10"#));
        let back: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.turn, 2);
        assert_eq!(back.agent_id, Some(AgentId::Codex));
    }
}
