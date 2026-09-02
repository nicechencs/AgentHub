use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

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

    pub(super) fn endpoint_key(local: BridgeLocalSurface) -> &'static str {
        match local {
            BridgeLocalSurface::Responses => "responses",
            BridgeLocalSurface::Messages => "messages",
            BridgeLocalSurface::ChatCompletions => "chat_completions",
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

    pub(super) fn reject_if_unserved(
        self,
        state: &EdgeState,
        request_id: &str,
    ) -> Option<Response> {
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
            Some(surface_mismatch_response(self, state.upstream.local_surface))
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

/// Path clients should call for a bound local conversation surface.
pub(super) fn served_conversation_path(local: BridgeLocalSurface) -> &'static str {
    match local {
        BridgeLocalSurface::Responses => "/v1/responses",
        BridgeLocalSurface::Messages => "/v1/messages",
        BridgeLocalSurface::ChatCompletions => "/v1/chat/completions",
    }
}

pub(super) fn requested_conversation_path(surface: DownstreamSurface) -> &'static str {
    match surface {
        DownstreamSurface::Responses => "/v1/responses",
        DownstreamSurface::Messages => "/v1/messages",
        DownstreamSurface::ChatCompletions => "/v1/chat/completions",
        DownstreamSurface::Models => "/v1/models",
    }
}

/// Bilingual UX for surface_mismatch (empty 404 body was unhelpful).
pub(super) fn surface_mismatch_message(
    requested: DownstreamSurface,
    local: BridgeLocalSurface,
) -> String {
    let served = served_conversation_path(local);
    let asked = requested_conversation_path(requested);
    format!(
        "This route only serves {served}; try that path instead of {asked}. 本机路由只提供 {served}，请改打该路径，而不是 {asked}。"
    )
}

pub(super) fn surface_mismatch_response(
    requested: DownstreamSurface,
    local: BridgeLocalSurface,
) -> Response {
    let message = surface_mismatch_message(requested, local);
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": {
                "code": "surface_mismatch",
                "message": message,
                "type": "invalid_request_error",
            }
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests;
