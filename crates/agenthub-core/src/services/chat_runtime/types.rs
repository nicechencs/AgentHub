//! Durable, agent-neutral DTOs for the long-lived chat runtime.
//!
//! These types are deliberately kept next to the runtime service rather than
//! in the Codex transport.  The transport speaks JSON-RPC; the rest of the
//! application only sees these normalized values.

use serde::{Deserialize, Serialize};

use crate::models::{ChatEvent, ChatMessage};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeQuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    #[serde(default)]
    pub options: Vec<RuntimeQuestionOption>,
    #[serde(default)]
    pub is_other: bool,
    #[serde(default)]
    pub is_secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimePermissionOption {
    pub id: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeFileChange {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Protocol-provided snippet (`diff`, `patch`, `content`, or before/after).
    /// Absent when the payload only named a path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRequest {
    pub id: String,
    pub run_id: String,
    pub kind: RuntimeRequestKind,
    pub title: String,
    pub detail: String,
    #[serde(default)]
    pub questions: Vec<RuntimeQuestion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permission_options: Vec<RuntimePermissionOption>,
    /// File-edit rows copied from protocol fields. Empty when the request is
    /// not a file change, or when the payload had no path list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_changes: Vec<RuntimeFileChange>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeRequestKind {
    Command,
    File,
    Question,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEvent {
    pub sequence: i64,
    pub event: ChatEvent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimePhase {
    Idle,
    Starting,
    Running,
    Waiting,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl RuntimePhase {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Cancelling => "cancelling",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "idle" => Self::Idle,
            "starting" => Self::Starting,
            "running" => Self::Running,
            "waiting" => Self::Waiting,
            "cancelling" => Self::Cancelling,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" | "canceled" => Self::Cancelled,
            "interrupted" => Self::Interrupted,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSnapshot {
    pub conversation_id: String,
    pub enabled: bool,
    pub run_id: Option<String>,
    pub phase: RuntimePhase,
    pub last_sequence: i64,
    pub events: Vec<RuntimeEvent>,
    pub pending_requests: Vec<RuntimeRequest>,
    pub gap: bool,
    pub current_message: Option<ChatMessage>,
}

impl RuntimeSnapshot {
    pub(crate) fn disabled(conversation_id: impl Into<String>) -> Self {
        Self {
            conversation_id: conversation_id.into(),
            enabled: false,
            run_id: None,
            phase: RuntimePhase::Idle,
            last_sequence: 0,
            events: Vec::new(),
            pending_requests: Vec::new(),
            gap: false,
            current_message: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeReply {
    pub conversation_id: String,
    pub run_id: String,
    pub request_id: String,
    pub client_request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<RuntimeDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answers: Option<std::collections::BTreeMap<String, Vec<String>>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeDecision {
    Allow,
    Deny,
    #[serde(rename = "allow_always")]
    AllowAlways,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeTurnSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeModelOption {
    pub id: String,
    #[serde(default)]
    pub efforts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeLocalImage {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSkillRef {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeExtensionKind {
    Skill,
    Plugin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeExtensionItem {
    pub id: String,
    pub name: String,
    pub kind: RuntimeExtensionKind,
    pub installed: bool,
    pub enabled: bool,
    pub loaded: bool,
    pub callable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStartExtras {
    #[serde(default)]
    pub images: Vec<RuntimeLocalImage>,
    #[serde(default)]
    pub skills: Vec<RuntimeSkillRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOptions {
    pub conversation_id: String,
    pub settings: RuntimeTurnSettings,
    pub settings_frozen: bool,
    pub models: Vec<RuntimeModelOption>,
    pub extensions: Vec<RuntimeExtensionItem>,
    /// True when model/list was fetched from a live Codex process this session.
    pub models_from_codex: bool,
    #[serde(default)]
    pub image_input: bool,
    #[serde(default)]
    pub steer: bool,
}

impl RuntimeOptions {
    /// Read surface for a conversation that does not use continuous chat.
    /// Must not persist a runtime row or surface as a catalog failure.
    pub(crate) fn inactive(conversation_id: impl Into<String>) -> Self {
        Self {
            conversation_id: conversation_id.into(),
            settings: RuntimeTurnSettings::default(),
            settings_frozen: false,
            models: Vec::new(),
            extensions: Vec::new(),
            models_from_codex: false,
            image_input: false,
            steer: false,
        }
    }
}
