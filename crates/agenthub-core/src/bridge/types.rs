//! Provider-neutral types used by the OpenAI Responses <-> Chat Completions bridge.
//!
//! The bridge intentionally keeps these types separate from the HTTP host.  That makes
//! protocol translation deterministic and, importantly, prevents transport errors from
//! accidentally serialising prompts, tool arguments, or credentials.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

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
}

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
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
}

impl Usage {
    pub fn from_chat_usage(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let input_tokens = object
            .get("prompt_tokens")
            .or_else(|| object.get("input_tokens"))?
            .as_u64()?;
        let output_tokens = object
            .get("completion_tokens")
            .or_else(|| object.get("output_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let total_tokens = object
            .get("total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(input_tokens.saturating_add(output_tokens));
        let cached_input_tokens = object
            .get("prompt_tokens_details")
            .and_then(Value::as_object)
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_u64);

        Some(Self {
            input_tokens,
            output_tokens,
            total_tokens,
            cached_input_tokens,
        })
    }

    pub fn to_responses_json(&self) -> Value {
        let mut input_tokens_details = Map::new();
        if let Some(cached_tokens) = self.cached_input_tokens {
            input_tokens_details.insert("cached_tokens".to_owned(), Value::from(cached_tokens));
        }
        serde_json::json!({
            "input_tokens": self.input_tokens,
            "input_tokens_details": Value::Object(input_tokens_details),
            "output_tokens": self.output_tokens,
            "output_tokens_details": {},
            "total_tokens": self.total_tokens,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
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
