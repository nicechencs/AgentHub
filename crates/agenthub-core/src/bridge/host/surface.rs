use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::bridge::protocol::anthropic_messages::parse_messages_request;
use crate::bridge::protocol::chat::parse_chat_request;
use crate::bridge::protocol::responses::parse_responses_request;
use crate::bridge::runtime::{BridgeLocalSurface, BridgeUpstreamProtocol};
use crate::bridge::types::{BridgeRequest, ProtocolError};

use super::http::ListenerState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DownstreamSurface {
    Responses,
    Messages,
    ChatCompletions,
    Models,
}

impl DownstreamSurface {
    pub(super) fn from_local(local: BridgeLocalSurface) -> Self {
        match local {
            BridgeLocalSurface::Responses => Self::Responses,
            BridgeLocalSurface::Messages => Self::Messages,
            BridgeLocalSurface::ChatCompletions => Self::ChatCompletions,
        }
    }

    pub(super) fn op(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::Messages => "messages",
            Self::ChatCompletions => "chat",
            Self::Models => "models",
        }
    }

    /// Models always served. Conversation surfaces served iff they match the
    /// listener's BridgeLocalSurface. Wrong conversation surface → 404.
    pub(super) fn served_by(self, local: BridgeLocalSurface) -> bool {
        match self {
            Self::Models => true,
            conversation => conversation == Self::from_local(local),
        }
    }

    pub(super) fn reject_if_unserved(self, state: &ListenerState) -> Option<Response> {
        if self.served_by(state.upstream.local_surface) {
            None
        } else {
            Some(StatusCode::NOT_FOUND.into_response())
        }
    }

    pub(super) fn parse_request(self, body: &Value) -> Result<BridgeRequest, ProtocolError> {
        match self {
            Self::Responses => parse_responses_request(body),
            Self::Messages => parse_messages_request(body),
            Self::ChatCompletions => parse_chat_request(body),
            Self::Models => {
                unreachable!("models are synthesized locally and do not parse a conversation body")
            }
        }
    }
}

/// Centralizes local surface / upstream protocol so route handlers do not
/// sniff host or model names.
#[derive(Debug, Clone, Copy)]
pub(super) struct ProtocolSelector {
    protocol: BridgeUpstreamProtocol,
    local_surface: BridgeLocalSurface,
}

impl ProtocolSelector {
    pub(super) fn from_listener(state: &ListenerState) -> Self {
        Self {
            protocol: state.upstream.protocol,
            local_surface: state.upstream.local_surface,
        }
    }

    pub(super) fn serves_responses(self) -> bool {
        self.local_surface == BridgeLocalSurface::Responses
    }

    pub(super) fn serves_messages(self) -> bool {
        self.local_surface == BridgeLocalSurface::Messages
    }

    pub(super) fn serves_chat_completions(self) -> bool {
        self.local_surface == BridgeLocalSurface::ChatCompletions
    }

    pub(super) fn responses_passthrough(self) -> bool {
        self.serves_responses()
            && matches!(
                self.protocol,
                BridgeUpstreamProtocol::CodexResponsesOauth
                    | BridgeUpstreamProtocol::XaiResponsesOauth
            )
    }
}

#[cfg(test)]
mod tests;
