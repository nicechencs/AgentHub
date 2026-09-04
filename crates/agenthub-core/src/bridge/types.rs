//! Provider-neutral types used by bridge protocol kernels.
//!
//! The existing Kimi path uses OpenAI Responses wire objects and Chat Completions.
//! The Codex subscription → Claude Code candidate uses the same request IR
//! ([`BridgeRequest`]) plus a surface-neutral stream IR ([`IrEvent`]).
//!
//! These types stay separate from the HTTP host so translation is deterministic and
//! transport errors cannot accidentally serialise prompts, tool arguments, or credentials.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

pub use super::usage::Usage;

pub type ProtocolResult<T> = Result<T, ProtocolError>;

/// A safe-to-expose protocol error.
///
/// `message` must stay generic: callers may display it or put it on a public HTTP
/// response.  Do not add request data, upstream bodies, credentials, or tool arguments.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct ProtocolError {
    pub code: &'static str,
    pub message: String,
}

impl ProtocolError {
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_request",
            message: message.into(),
        }
    }

    pub fn unsupported(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn upstream() -> Self {
        Self {
            code: "upstream_error",
            message: "The upstream model provider returned an error.".to_owned(),
        }
    }

    /// Official Codex / ChatGPT Responses rejects non-stream requests.
    /// The host forces `stream: true` and aggregates; this is the client-facing
    /// message when the upstream still refuses streaming.
    pub fn upstream_stream_required() -> Self {
        Self {
            code: "upstream_error",
            message: UPSTREAM_STREAM_REQUIRED_ZH.to_owned(),
        }
    }

    pub fn upstream_stream_incomplete() -> Self {
        Self {
            code: "upstream_error",
            message: UPSTREAM_STREAM_INCOMPLETE_ZH.to_owned(),
        }
    }
}

/// Shown on the local route and in logs when the upstream requires SSE.
pub const UPSTREAM_STREAM_REQUIRED_ZH: &str =
    "上游要求使用流式请求。本机路由会自动改成流式并拼成完整回复；若仍失败，请点重试。";

/// Shown when a forced-stream upstream closes before a terminal event.
pub const UPSTREAM_STREAM_INCOMPLETE_ZH: &str = "上游流式回复不完整或已中断，请点重试。";

/// ChatGPT Codex subscription rejected the requested model id.
pub const CHATGPT_CODEX_MODEL_UNSUPPORTED_ZH: &str = "当前 ChatGPT 登录不支持该模型";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeRequest {
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    pub input: Vec<BridgeMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<BridgeTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default)]
    pub stream: bool,
    /// Parsed request fields that have no provider-neutral meaning yet.
    ///
    /// Keeping them here gives a future adapter a deliberate compatibility path rather
    /// than silently losing fields while translating a request.  Only documented,
    /// provider-compatible fields are forwarded by `to_kimi_chat_request`.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub passthrough: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeMessage {
    pub role: MessageRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<BridgeContent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    Developer,
    User,
    Assistant,
    Tool,
}

impl MessageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Developer => "developer",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BridgeContent {
    Text {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    ToolResult {
        call_id: String,
        output: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    None,
    Required,
    Function { name: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    #[default]
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    Unknown,
}

impl StopReason {
    pub fn from_chat_finish_reason(value: Option<&str>) -> Self {
        match value {
            Some("stop") => Self::Stop,
            Some("length") => Self::Length,
            Some("tool_calls") | Some("function_call") => Self::ToolCalls,
            Some("content_filter") => Self::ContentFilter,
            _ => Self::Unknown,
        }
    }
}

/// Provider-neutral incremental output.  HTTP/SSE code can serialise an event with
/// `BridgeEvent::event_name` and `BridgeEvent::data` without knowing Kimi's schema.
#[derive(Debug, Clone, PartialEq)]
pub enum BridgeEvent {
    ResponseStarted {
        response: Value,
        sequence_number: u64,
    },
    ResponseInProgress {
        response: Value,
        sequence_number: u64,
    },
    OutputItemAdded {
        output_index: usize,
        item: Value,
        sequence_number: u64,
    },
    ContentPartAdded {
        output_index: usize,
        item_id: String,
        content_index: usize,
        part: Value,
        sequence_number: u64,
    },
    TextDelta {
        output_index: usize,
        item_id: String,
        content_index: usize,
        delta: String,
        sequence_number: u64,
    },
    TextDone {
        output_index: usize,
        item_id: String,
        content_index: usize,
        text: String,
        sequence_number: u64,
    },
    ContentPartDone {
        output_index: usize,
        item_id: String,
        content_index: usize,
        part: Value,
        sequence_number: u64,
    },
    FunctionCallArgumentsDelta {
        output_index: usize,
        item_id: String,
        call_id: String,
        delta: String,
        sequence_number: u64,
    },
    FunctionCallArgumentsDone {
        output_index: usize,
        item_id: String,
        call_id: String,
        arguments: String,
        sequence_number: u64,
    },
    OutputItemDone {
        output_index: usize,
        item: Value,
        sequence_number: u64,
    },
    Completed {
        response: Value,
        sequence_number: u64,
    },
    Incomplete {
        response: Value,
        sequence_number: u64,
    },
    Error {
        code: &'static str,
        message: String,
        sequence_number: u64,
    },
}

impl BridgeEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::ResponseStarted { .. } => "response.created",
            Self::ResponseInProgress { .. } => "response.in_progress",
            Self::OutputItemAdded { .. } => "response.output_item.added",
            Self::ContentPartAdded { .. } => "response.content_part.added",
            Self::TextDelta { .. } => "response.output_text.delta",
            Self::TextDone { .. } => "response.output_text.done",
            Self::ContentPartDone { .. } => "response.content_part.done",
            Self::FunctionCallArgumentsDelta { .. } => "response.function_call_arguments.delta",
            Self::FunctionCallArgumentsDone { .. } => "response.function_call_arguments.done",
            Self::OutputItemDone { .. } => "response.output_item.done",
            Self::Completed { .. } => "response.completed",
            Self::Incomplete { .. } => "response.incomplete",
            Self::Error { .. } => "error",
        }
    }

    pub fn sequence_number(&self) -> u64 {
        match self {
            Self::ResponseStarted {
                sequence_number, ..
            }
            | Self::ResponseInProgress {
                sequence_number, ..
            }
            | Self::OutputItemAdded {
                sequence_number, ..
            }
            | Self::ContentPartAdded {
                sequence_number, ..
            }
            | Self::TextDelta {
                sequence_number, ..
            }
            | Self::TextDone {
                sequence_number, ..
            }
            | Self::ContentPartDone {
                sequence_number, ..
            }
            | Self::FunctionCallArgumentsDelta {
                sequence_number, ..
            }
            | Self::FunctionCallArgumentsDone {
                sequence_number, ..
            }
            | Self::OutputItemDone {
                sequence_number, ..
            }
            | Self::Completed {
                sequence_number, ..
            }
            | Self::Incomplete {
                sequence_number, ..
            }
            | Self::Error {
                sequence_number, ..
            } => *sequence_number,
        }
    }

    pub fn data(&self) -> Value {
        match self {
            Self::ResponseStarted { response, .. } | Self::ResponseInProgress { response, .. } => {
                serde_json::json!({
                    "type": self.event_name(),
                    "sequence_number": self.sequence_number(),
                    "response": response,
                })
            }
            Self::OutputItemAdded {
                output_index, item, ..
            }
            | Self::OutputItemDone {
                output_index, item, ..
            } => serde_json::json!({
                "type": self.event_name(),
                "sequence_number": self.sequence_number(),
                "output_index": output_index,
                "item": item,
            }),
            Self::ContentPartAdded {
                output_index,
                item_id,
                content_index,
                part,
                ..
            } => serde_json::json!({
                "type": self.event_name(),
                "sequence_number": self.sequence_number(),
                "output_index": output_index,
                "item_id": item_id,
                "content_index": content_index,
                "part": part,
            }),
            Self::TextDelta {
                output_index,
                item_id,
                content_index,
                delta,
                ..
            } => serde_json::json!({
                "type": self.event_name(),
                "sequence_number": self.sequence_number(),
                "output_index": output_index,
                "item_id": item_id,
                "content_index": content_index,
                "delta": delta,
            }),
            Self::TextDone {
                output_index,
                item_id,
                content_index,
                text,
                ..
            } => serde_json::json!({
                "type": self.event_name(),
                "sequence_number": self.sequence_number(),
                "output_index": output_index,
                "item_id": item_id,
                "content_index": content_index,
                "text": text,
            }),
            Self::ContentPartDone {
                output_index,
                item_id,
                content_index,
                part,
                ..
            } => serde_json::json!({
                "type": self.event_name(),
                "sequence_number": self.sequence_number(),
                "output_index": output_index,
                "item_id": item_id,
                "content_index": content_index,
                "part": part,
            }),
            Self::FunctionCallArgumentsDelta {
                output_index,
                item_id,
                call_id,
                delta,
                ..
            } => serde_json::json!({
                "type": self.event_name(),
                "sequence_number": self.sequence_number(),
                "output_index": output_index,
                "item_id": item_id,
                "call_id": call_id,
                "delta": delta,
            }),
            Self::FunctionCallArgumentsDone {
                output_index,
                item_id,
                call_id,
                arguments,
                ..
            } => serde_json::json!({
                "type": self.event_name(),
                "sequence_number": self.sequence_number(),
                "output_index": output_index,
                "item_id": item_id,
                "call_id": call_id,
                "arguments": arguments,
            }),
            Self::Completed { response, .. } | Self::Incomplete { response, .. } => {
                serde_json::json!({
                    "type": self.event_name(),
                    "sequence_number": self.sequence_number(),
                    "response": response,
                })
            }
            Self::Error { code, message, .. } => serde_json::json!({
                "type": self.event_name(),
                "sequence_number": self.sequence_number(),
                "error": { "code": code, "message": message },
            }),
        }
    }
}

/// Stable insertion-ordered state for upstream Kimi tool-call `index` values.
pub(crate) type ToolCallMap<T> = BTreeMap<usize, T>;

/// Surface-neutral stream events for Messages ↔ Responses protocol kernels.
///
/// Wire encoders (Anthropic SSE, Responses SSE) own event names and JSON envelopes.
/// Runtime hosts must not invent tool side-effects from these events alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrEvent {
    MessageStart {
        id: String,
        model: String,
    },
    TextDelta {
        text: String,
    },
    ToolCallStart {
        id: String,
        name: String,
    },
    ToolCallDelta {
        id: String,
        arguments_delta: String,
    },
    ToolCallEnd {
        id: String,
    },
    Usage {
        input_tokens: u64,
        output_tokens: u64,
        cached_input_tokens: Option<u64>,
    },
    MessageEnd {
        stop_reason: StopReason,
    },
    Error {
        code: String,
        message: String,
        retryable: bool,
    },
}

impl IrEvent {
    /// Whether emitting this event commits the stream so upstream retries become unsafe.
    pub fn commits_output(&self) -> bool {
        match self {
            Self::TextDelta { .. }
            | Self::ToolCallStart { .. }
            | Self::ToolCallDelta { .. }
            | Self::ToolCallEnd { .. }
            | Self::MessageEnd { .. } => true,
            // MessageStart / Usage alone do not expose model content to the client surface
            // in a way that would double-execute tools if the upstream turn is retried.
            Self::MessageStart { .. } | Self::Usage { .. } | Self::Error { .. } => false,
        }
    }
}

/// Whether any effective client-visible content has already been emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmissionState {
    #[default]
    Idle,
    Emitted,
}

impl EmissionState {
    pub fn observe(self, event: &IrEvent) -> Self {
        if self == Self::Emitted || event.commits_output() {
            Self::Emitted
        } else {
            Self::Idle
        }
    }
}

/// Classification used by the first-event retry gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryClass {
    /// Transient upstream/transport failure that may be retried before output.
    Transient,
    /// Permanent failure (auth, invalid request, unsupported, etc.).
    Permanent,
}

/// Pure retry policy for subscription bridges.
///
/// Safe retries are allowed only before the first effective client-visible event.
/// After any such event, replay is forbidden (no account switch, no tool re-run).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryGate {
    pub max_attempts: u32,
}

impl Default for RetryGate {
    fn default() -> Self {
        Self { max_attempts: 2 }
    }
}

impl RetryGate {
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
        }
    }

    /// `attempts_already` is the number of upstream tries that have already finished.
    /// The first try is `0`.
    pub fn can_retry(
        &self,
        state: EmissionState,
        class: RetryClass,
        attempts_already: u32,
    ) -> bool {
        if state != EmissionState::Idle {
            return false;
        }
        if class != RetryClass::Transient {
            return false;
        }
        attempts_already.saturating_add(1) < self.max_attempts
    }
}

impl StopReason {
    pub fn to_anthropic_stop_reason(&self) -> &'static str {
        match self {
            Self::Stop => "end_turn",
            Self::Length => "max_tokens",
            Self::ToolCalls => "tool_use",
            Self::ContentFilter => "refusal",
            Self::Unknown => "end_turn",
        }
    }

    pub fn from_anthropic_stop_reason(value: Option<&str>) -> Self {
        match value {
            Some("end_turn") | Some("stop_sequence") => Self::Stop,
            Some("max_tokens") => Self::Length,
            Some("tool_use") => Self::ToolCalls,
            Some("refusal") => Self::ContentFilter,
            _ => Self::Unknown,
        }
    }

    pub fn to_responses_status(&self) -> &'static str {
        match self {
            Self::Length | Self::ContentFilter => "incomplete",
            _ => "completed",
        }
    }
}
