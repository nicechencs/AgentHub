use std::time::Instant;

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::bridge::types::ProtocolError;

use super::dispatch::handle_conversation;
use super::gateway::{Gateway, GatewayAuthError};
use super::inbound::InboundRequestRecord;
use super::surface::DownstreamSurface;
use super::{BODY_LIMIT_BYTES, REQUEST_BODY_TIMEOUT};

pub(super) use super::gateway::EdgeState;

pub(super) fn router(gateway: Gateway) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(list_models))
        .route("/models", get(list_models))
        .route("/v1/responses", post(responses))
        .route("/v1/messages", post(messages))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/chat/completions", post(chat_completions))
        .layer(axum::extract::DefaultBodyLimit::max(BODY_LIMIT_BYTES))
        .layer(middleware::from_fn_with_state(
            gateway.clone(),
            record_inbound,
        ))
        .with_state(gateway)
}

async fn record_inbound(State(gateway): State<Gateway>, request: Request, next: Next) -> Response {
    let method = request.method().as_str().to_owned();
    let path = request.uri().path().to_owned();
    let profile_id = gateway
        .authenticate(request.headers())
        .ok()
        .map(|edge| edge.profile_id.to_string());
    let response = next.run(request).await;
    if let Some(profile_id) = profile_id {
        // Successful /health is a liveness probe — keep it out of the monitoring
        // feed so it does not crowd out real route traces (and has no port/conversion).
        let ok_health = method.eq_ignore_ascii_case("GET")
            && path == "/health"
            && response.status().is_success();
        if !ok_health {
            gateway.inbound.push(
                &profile_id,
                InboundRequestRecord::new(method, path, response.status().as_u16()),
            );
        }
    }
    response
}

fn edge_from_headers(
    gateway: &Gateway,
    headers: &HeaderMap,
    op: &'static str,
) -> Result<EdgeState, Response> {
    match gateway.authenticate(headers) {
        Ok(edge) => Ok(edge),
        Err(GatewayAuthError::Unauthorized) => Err(reject_invalid_local_auth(op, None)),
        Err(GatewayAuthError::Stopping | GatewayAuthError::Poisoned) => Err(stopping_response()),
    }
}

async fn health(State(gateway): State<Gateway>, headers: HeaderMap) -> Response {
    let state = match edge_from_headers(&gateway, &headers, "health") {
        Ok(state) => state,
        Err(response) => return response,
    };
    // Local health is a non-billable liveness check. It reports that edge's last
    // stored upstream outcome and never issues a new provider probe.
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

async fn list_models(State(gateway): State<Gateway>, headers: HeaderMap) -> Response {
    let state = match edge_from_headers(&gateway, &headers, DownstreamSurface::Models.op()) {
        Ok(state) => state,
        Err(response) => return response,
    };
    let listed = if let Some(index) = &state.route_index {
        state.models_after_denials(index.list_models(DownstreamSurface::endpoint_key(
            state.upstream.local_surface,
        )))
    } else {
        gateway.listed_models_with_backup(&state)
    };
    // Synthesized from the edge mapping table at start; never proxied.
    if listed.is_empty() {
        tracing::info!(
            target: "core.adapter",
            profile_id = %state.profile_id,
            op = "models",
            code = "empty_models",
            count = 0_usize,
            "bridge models list is empty"
        );
    }
    let data: Vec<Value> = listed
        .iter()
        .map(|id| json!({ "id": id, "object": "model" }))
        .collect();
    Json(json!({ "object": "list", "data": data })).into_response()
}

async fn responses(State(gateway): State<Gateway>, request: Request) -> Response {
    handle_conversation(DownstreamSurface::Responses, gateway, request).await
}

async fn messages(State(gateway): State<Gateway>, request: Request) -> Response {
    handle_conversation(DownstreamSurface::Messages, gateway, request).await
}

async fn chat_completions(State(gateway): State<Gateway>, request: Request) -> Response {
    handle_conversation(DownstreamSurface::ChatCompletions, gateway, request).await
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
            ));
        }
        Err(_) => {
            return Err(error_response(
                StatusCode::REQUEST_TIMEOUT,
                "request_timeout",
                "The request body timed out.",
                None,
            ));
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
    let mut previous_line_end = None;
    let mut index = 0;
    while index < buffer.len() {
        let line_end_len = match buffer.get(index) {
            Some(b'\r') if buffer.get(index + 1) == Some(&b'\n') => 2,
            Some(b'\r' | b'\n') => 1,
            _ => {
                index += 1;
                continue;
            }
        };
        if let Some((previous_start, previous_len)) = previous_line_end {
            if previous_start + previous_len == index {
                return Some((previous_start, index + line_end_len - previous_start));
            }
        }
        previous_line_end = Some((index, line_end_len));
        index += line_end_len;
    }
    None
}

pub(super) fn sse_data_payload(frame: &[u8]) -> Result<Option<String>, ()> {
    let frame = std::str::from_utf8(frame).map_err(|_| ())?;
    let normalized = frame.replace("\r\n", "\n").replace('\r', "\n");
    let payload = normalized
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

pub(super) fn overloaded_response() -> Response {
    error_response(
        StatusCode::TOO_MANY_REQUESTS,
        "bridge_overloaded",
        "The local bridge is temporarily busy.",
        Some(HeaderValue::from_static("1")),
    )
}

pub(super) fn reject_invalid_local_auth(
    op: &'static str,
    conversation: Option<(&str, Instant)>,
) -> Response {
    // Unauthenticated requests are not bound to an edge; do not log a profile_id.
    match conversation {
        Some((request_id, started)) => {
            tracing::warn!(target: "core.adapter", request_id = %request_id, op, code = "unauthorized", status = 401_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge request rejected");
        }
        None if op == "health" => {
            tracing::warn!(target: "core.adapter", op = "health", code = "unauthorized", status = 401_u16, "bridge health request rejected");
        }
        None => {
            tracing::warn!(target: "core.adapter", op = "models", code = "unauthorized", status = 401_u16, "bridge models request rejected");
        }
    }
    error_response(
        StatusCode::UNAUTHORIZED,
        "invalid_api_key",
        "Invalid local bearer token.",
        None,
    )
}

pub(super) fn protocol_error_response(error: ProtocolError) -> Response {
    error_response(StatusCode::BAD_REQUEST, error.code, &error.message, None)
}

pub(super) fn log_protocol_error(
    state: &EdgeState,
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
