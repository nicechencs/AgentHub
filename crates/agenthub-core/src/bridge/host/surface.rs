use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::bridge::protocol::anthropic_messages::parse_messages_request;
use crate::bridge::protocol::chat::parse_chat_request;
use crate::bridge::protocol::responses::parse_responses_request;
use crate::bridge::runtime::BridgeLocalSurface;
use crate::bridge::types::{BridgeRequest, ProtocolError};

use super::http::EdgeState;

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
    /// edge's BridgeLocalSurface. Wrong conversation surface → 404 (after auth).
    pub(super) fn served_by(self, local: BridgeLocalSurface) -> bool {
        match self {
            Self::Models => true,
            conversation => conversation == Self::from_local(local),
        }
    }

    pub(super) fn reject_if_unserved(self, state: &EdgeState, request_id: &str) -> Option<Response> {
        if self.served_by(state.upstream.local_surface) {
            None
        } else {
            tracing::warn!(
                target: "core.adapter",
                profile_id = %state.profile_id,
                request_id = %request_id,
                op = "serve",
                code = "surface_mismatch",
                local_surface = ?state.upstream.local_surface,
                requested = self.op(),
                status = 404_u16,
                "local surface does not serve this path"
            );
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

#[cfg(test)]
mod tests;
