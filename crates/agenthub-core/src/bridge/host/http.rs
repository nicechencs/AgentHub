use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use reqwest::Url;
use serde_json::{json, Value};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::bridge::grok_cli::GrokReasoningReplay;
use crate::bridge::runtime::BridgeUpstreamStatus;
use crate::bridge::types::ProtocolError;

use super::dispatch::{
    handle_chat_completions, handle_messages, handle_responses, ProtocolSelector,
};
use super::{BODY_LIMIT_BYTES, REQUEST_BODY_TIMEOUT};

#[derive(Clone)]
pub(super) struct ListenerState {
    pub(super) profile_id: Arc<str>,
    pub(super) local_token: Arc<str>,
    pub(super) upstream: crate::bridge::runtime::BridgeUpstreamConfig,
    pub(super) upstream_url: Url,
    pub(super) client: reqwest::Client,
    pub(super) force_shutdown: CancellationToken,
    pub(super) admission: Arc<Semaphore>,
    pub(super) observed_upstream: Arc<Mutex<BridgeUpstreamStatus>>,
    pub(super) grok_replay: Arc<GrokReasoningReplay>,
    pub(super) listed_models: Arc<[String]>,
    pub(super) reload_upstream_auth: Option<crate::bridge::UpstreamAuthReload>,
}

impl ListenerState {
    pub(super) fn observed_upstream(&self) -> BridgeUpstreamStatus {
        self.observed_upstream
            .lock()
            .map(|status| *status)
            .unwrap_or(BridgeUpstreamStatus::Unavailable)
    }

    pub(super) fn record_upstream(&self, status: BridgeUpstreamStatus) {
        if let Ok(mut observed) = self.observed_upstream.lock() {
            *observed = status;
        }
    }

    pub(super) fn record_upstream_success(&self) {
        self.record_upstream(BridgeUpstreamStatus::Connected);
    }

    pub(super) fn record_upstream_failure(&self) {
        self.record_upstream(BridgeUpstreamStatus::Degraded);
    }
}

pub(super) fn router(state: ListenerState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(list_models))
        .route("/models", get(list_models))
        .route("/v1/responses", post(responses))
        .route("/v1/messages", post(messages))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/chat/completions", post(chat_completions))
        .layer(axum::extract::DefaultBodyLimit::max(BODY_LIMIT_BYTES))
        .with_state(state)
}

async fn health(State(state): State<ListenerState>, headers: HeaderMap) -> Response {
    if !has_valid_local_auth(&headers, &state.local_token) {
        tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, op = "health", code = "unauthorized", status = 401_u16, "bridge health request rejected");
        return error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "Invalid local bearer token.",
            None,
        );
    }
    // Local health is a listener liveness check. It reports the last stored
    // upstream outcome and never issues a new billable provider probe.
    let upstream_status = state.observed_upstream();
    tracing::debug!(target: "core.adapter", profile_id = %state.profile_id, op = "health", upstream_status = upstream_status.as_str(), "bridge health check");
    Json(json!({
        "ok": true,
        "service": "agenthub-bridge",
        "listener_status": "running",
        "upstream_status": upstream_status.as_str()
    }))
    .into_response()
}

async fn list_models(State(state): State<ListenerState>, headers: HeaderMap) -> Response {
    if !has_valid_local_auth(&headers, &state.local_token) {
        tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, op = "models", code = "unauthorized", status = 401_u16, "bridge models request rejected");
        return error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "Invalid local bearer token.",
            None,
        );
    }
    // Synthesized from the profile mapping table at start; never proxied.
    if state.listed_models.is_empty() {
        tracing::info!(
            target: "core.adapter",
            profile_id = %state.profile_id,
            op = "models",
            code = "empty_models",
            count = 0_usize,
            "bridge models list is empty"
        );
    }
    let data: Vec<Value> = state
        .listed_models
        .iter()
        .map(|id| json!({ "id": id, "object": "model" }))
        .collect();
    Json(json!({ "object": "list", "data": data })).into_response()
}

async fn responses(State(state): State<ListenerState>, request: Request) -> Response {
    if !ProtocolSelector::from_listener(&state).serves_responses() {
        return StatusCode::NOT_FOUND.into_response();
    }
    handle_responses(state, request).await
}

async fn messages(State(state): State<ListenerState>, request: Request) -> Response {
    if !ProtocolSelector::from_listener(&state).serves_messages() {
        return StatusCode::NOT_FOUND.into_response();
    }
    handle_messages(state, request).await
}

async fn chat_completions(State(state): State<ListenerState>, request: Request) -> Response {
    if !ProtocolSelector::from_listener(&state).serves_chat_completions() {
        return StatusCode::NOT_FOUND.into_response();
    }
    handle_chat_completions(state, request).await
}

pub(super) async fn read_request_json(request: Request) -> Result<Value, Response> {
    let body = match tokio::time::timeout(
        REQUEST_BODY_TIMEOUT,
        axum::body::to_bytes(request.into_body(), BODY_LIMIT_BYTES),
    )
    .await
    {
        Ok(Ok(body)) => body,
        Ok(Err(_)) => {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "The request body is invalid or too large.",
                None,
            ))
        }
        Err(_) => {
            return Err(error_response(
                StatusCode::REQUEST_TIMEOUT,
                "request_timeout",
                "The request body timed out.",
                None,
            ))
        }
    };
    serde_json::from_slice::<Value>(&body).map_err(|_| {
        error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "The request body must be valid JSON.",
            None,
        )
    })
}

#[cfg(test)]
pub fn sse_frame_end(buffer: &[u8]) -> Option<(usize, usize)> {
    let deque: std::collections::VecDeque<u8> = buffer.iter().copied().collect();
    sse_frame_end_deque(&deque)
}

pub fn sse_frame_end_deque(buffer: &std::collections::VecDeque<u8>) -> Option<(usize, usize)> {
    let mut crlf = None;
    let mut lf = None;
    for index in 0..buffer.len() {
        if crlf.is_none()
            && index + 4 <= buffer.len()
            && buffer
                .iter()
                .skip(index)
                .take(4)
                .copied()
                .eq(b"\r\n\r\n".iter().copied())
        {
            crlf = Some((index, 4));
        }
        if lf.is_none()
            && index + 2 <= buffer.len()
            && buffer
                .iter()
                .skip(index)
                .take(2)
                .copied()
                .eq(b"\n\n".iter().copied())
        {
            lf = Some((index, 2));
        }
        if crlf.is_some() && lf.is_some() {
            break;
        }
    }
    match (crlf, lf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(frame), None) | (None, Some(frame)) => Some(frame),
        (None, None) => None,
    }
}

pub(super) fn sse_data_payload(frame: &[u8]) -> Result<Option<String>, ()> {
    let frame = std::str::from_utf8(frame).map_err(|_| ())?;
    let payload = frame
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|line| line.strip_prefix(' ').unwrap_or(line))
        .collect::<Vec<_>>();
    if payload.is_empty() {
        Ok(None)
    } else {
        Ok(Some(payload.join("\n")))
    }
}

pub(super) fn stream_error_frame() -> axum::body::Bytes {
    axum::body::Bytes::from_static(b"event: error\ndata: {\"type\":\"error\",\"error\":{\"code\":\"upstream_error\",\"message\":\"The upstream model provider returned an invalid stream.\"}}\n\n")
}

pub(super) fn stopping_response() -> Response {
    error_response(
        StatusCode::SERVICE_UNAVAILABLE,
        "bridge_stopping",
        "The local bridge is stopping.",
        None,
    )
}

pub(super) fn has_valid_local_auth(headers: &HeaderMap, expected: &str) -> bool {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    let api_key = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok());
    bearer
        .or(api_key)
        .is_some_and(|token| constant_time_eq(token.as_bytes(), expected.as_bytes()))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut mismatch = left.len() ^ right.len();
    let width = left.len().max(right.len());
    for index in 0..width {
        mismatch |= usize::from(*left.get(index).unwrap_or(&0) ^ *right.get(index).unwrap_or(&0));
    }
    mismatch == 0
}

pub(super) fn protocol_error_response(error: ProtocolError) -> Response {
    error_response(StatusCode::BAD_REQUEST, error.code, &error.message, None)
}

pub(super) fn log_protocol_error(
    state: &ListenerState,
    request_id: &str,
    started: Instant,
    error: &ProtocolError,
) {
    tracing::warn!(target: "core.adapter.protocol", profile_id = %state.profile_id, request_id, op = "protocol", code = error.code, status = 400_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge protocol rejected request");
}

pub(super) fn error_response(
    status: StatusCode,
    code: &str,
    message: &str,
    retry_after: Option<HeaderValue>,
) -> Response {
    let mut response = (
        status,
        Json(json!({ "error": { "code": code, "message": message, "type": "invalid_request_error" } })),
    )
        .into_response();
    if let Some(retry_after) = retry_after {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, retry_after);
    }
    response
}
