use std::time::Instant;

use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use futures_util::StreamExt;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::bridge::runtime::BridgeUpstreamProtocol;
use crate::utils::redact::redact_text;

use super::http::{error_response, stopping_response, EdgeState};
use super::stream::UpstreamBodyError;
use super::{UPSTREAM_BODY_IDLE_TIMEOUT, UPSTREAM_RESPONSE_HEADER_TIMEOUT};

const ACCESS_JWT_EXPIRY_SKEW_SECS: i64 = 60;
const UPSTREAM_ERROR_BODY_LIMIT_BYTES: usize = 8 * 1024;

fn grok_upstream(state: &EdgeState) -> bool {
    state.upstream.protocol == BridgeUpstreamProtocol::XaiResponsesOauth
}

pub(super) fn grok_replay_model(body: &Value, fallback: Option<&str>) -> String {
    body.get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            fallback
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_default()
}

pub(super) fn apply_grok_replay(
    state: &EdgeState,
    body: &mut Value,
    seed: Option<&str>,
    account_id: Option<&str>,
) {
    if !grok_upstream(state) {
        return;
    }
    let model = grok_replay_model(body, state.upstream.model.as_deref());
    state
        .grok_replay
        .apply(body, &model, replay_session(seed, account_id).as_deref());
}

pub(super) fn capture_grok_completed(
    state: &EdgeState,
    seed: Option<&str>,
    account_id: Option<&str>,
    completed: &Value,
) {
    if !grok_upstream(state) {
        return;
    }
    let model = grok_replay_model(completed, state.upstream.model.as_deref());
    state.grok_replay.store_completed(
        &model,
        replay_session(seed, account_id).as_deref(),
        completed,
    );
}

pub(super) fn capture_grok_sse(
    state: &EdgeState,
    seed: Option<&str>,
    account_id: Option<&str>,
    sse: &str,
) {
    if !grok_upstream(state) {
        return;
    }
    let model = grok_replay_model(&Value::Null, state.upstream.model.as_deref());
    state
        .grok_replay
        .store_sse(&model, replay_session(seed, account_id).as_deref(), sse);
}

pub(super) fn replay_session(seed: Option<&str>, account_id: Option<&str>) -> Option<String> {
    let seed = seed.map(str::trim).filter(|value| !value.is_empty())?;
    match account_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(account) => Some(format!("{account}\0{seed}")),
        None => Some(seed.to_string()),
    }
}

pub(super) fn map_upstream_http_error(
    state: &EdgeState,
    request_id: &str,
    started: Instant,
    status: StatusCode,
    retry_after: Option<HeaderValue>,
    upstream_detail: Option<&str>,
    member: Option<&crate::bridge::account::PickedMember>,
    failover_from: Option<&str>,
) -> Response {
    let local_status = if status == StatusCode::TOO_MANY_REQUESTS {
        StatusCode::TOO_MANY_REQUESTS
    } else {
        StatusCode::BAD_GATEWAY
    };
    tracing::warn!(
        target: "core.adapter",
        profile_id = %state.profile_id,
        request_id = %request_id,
        account_id = member.map(|m| m.source_id.as_str()).unwrap_or(""),
        ticket_id = member.map(|m| m.ticket_id.as_str()).unwrap_or(""),
        failover = failover_from.is_some(),
        failover_from = failover_from.unwrap_or(""),
        op = "upstream",
        code = "upstream_status",
        status = status.as_u16(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        upstream_detail = upstream_detail.unwrap_or(""),
        detail = upstream_detail.unwrap_or(""),
        "bridge upstream returned an error"
    );
    state.record_upstream_failure();
    error_response(
        local_status,
        "upstream_error",
        "The upstream model provider returned an error.",
        retry_after,
    )
}

pub(super) fn extract_upstream_error_detail(body: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<Value>(body).ok()?;
    let detail = value.get("detail").and_then(Value::as_str).or_else(|| {
        value
            .get("error")
            .and_then(Value::as_object)
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
    })?;
    let redacted = redact_text(detail);
    let flattened = redacted
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let flattened = flattened.split_whitespace().collect::<Vec<_>>().join(" ");
    let truncated = flattened.chars().take(512).collect::<String>();
    (!truncated.is_empty()).then_some(truncated)
}

pub(super) fn access_jwt_near_expiry(token: &str) -> bool {
    let Some(claims) = crate::oauth::decode_jwt_payload(token) else {
        return false;
    };
    let Some(exp) = claims.get("exp").and_then(|value| value.as_i64()) else {
        return false;
    };
    let now = chrono::Utc::now().timestamp();
    exp <= now + ACCESS_JWT_EXPIRY_SKEW_SECS
}

pub(super) fn try_reload_member_auth(member: &crate::bridge::account::PickedMember) -> bool {
    let Some(reload) = member.reload.as_ref() else {
        return false;
    };
    let current = member.auth.token();
    let Some(next) = reload() else {
        return false;
    };
    let next = next.trim();
    if next.is_empty() || next == current {
        return false;
    }
    member.auth.replace_token(next);
    true
}

pub(super) async fn read_bounded_upstream_error(
    response: reqwest::Response,
    force_shutdown: &CancellationToken,
) -> Result<Vec<u8>, UpstreamBodyError> {
    let mut body = Vec::with_capacity(UPSTREAM_ERROR_BODY_LIMIT_BYTES);
    let mut stream = response.bytes_stream();
    while let Some(chunk) = tokio::select! {
        _ = force_shutdown.cancelled() => return Err(UpstreamBodyError::Stopping),
        next = tokio::time::timeout(UPSTREAM_BODY_IDLE_TIMEOUT, stream.next()) => match next {
            Ok(next) => next,
            Err(_) => return Err(UpstreamBodyError::InvalidOrTooLarge),
        },
    } {
        let chunk = chunk.map_err(|_| UpstreamBodyError::InvalidOrTooLarge)?;
        let remaining = UPSTREAM_ERROR_BODY_LIMIT_BYTES.saturating_sub(body.len());
        if chunk.len() > remaining {
            body.extend_from_slice(&chunk[..remaining]);
            break;
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

pub(super) enum UpstreamConnectError {
    Stopping,
    Timeout,
    Unavailable,
}

pub(super) fn timeout_response() -> Response {
    error_response(
        StatusCode::GATEWAY_TIMEOUT,
        "upstream_timeout",
        "The upstream model provider timed out.",
        None,
    )
}

pub(super) fn unavailable_response() -> Response {
    error_response(
        StatusCode::BAD_GATEWAY,
        "upstream_unavailable",
        "The upstream model provider is unavailable.",
        None,
    )
}

pub(super) fn pool_exhausted_response(retry_after: Option<HeaderValue>) -> Response {
    error_response(
        StatusCode::SERVICE_UNAVAILABLE,
        "pool_exhausted",
        "No available connection can serve this request right now.",
        retry_after,
    )
}

pub(super) fn map_v2_request_error(
    status: StatusCode,
    retry_after: Option<HeaderValue>,
) -> Response {
    let status = match status.as_u16() {
        400 | 403 | 422 => status,
        _ => StatusCode::BAD_REQUEST,
    };
    error_response(
        status,
        "invalid_request",
        "The request was rejected.",
        retry_after,
    )
}

pub(super) async fn post_upstream_attempt(
    state: &EdgeState,
    builder: reqwest::RequestBuilder,
    request_id: &str,
) -> Result<reqwest::Response, UpstreamConnectError> {
    let result = tokio::select! {
        _ = state.force_shutdown.cancelled() => return Err(UpstreamConnectError::Stopping),
        result = tokio::time::timeout(UPSTREAM_RESPONSE_HEADER_TIMEOUT, builder.send()) => match result {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "header_timeout", status = 504_u16, "bridge upstream response headers timed out");
                state.record_upstream_failure();
                return Err(UpstreamConnectError::Timeout);
            }
        },
    };
    match result {
        Ok(response) => Ok(response),
        Err(_) => {
            tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "unavailable", status = 502_u16, "bridge upstream unavailable");
            state.record_upstream_failure();
            Err(UpstreamConnectError::Unavailable)
        }
    }
}

pub(super) async fn post_upstream(
    state: &EdgeState,
    builder: reqwest::RequestBuilder,
    request_id: &str,
) -> Result<reqwest::Response, Response> {
    match post_upstream_attempt(state, builder, request_id).await {
        Ok(response) => Ok(response),
        Err(UpstreamConnectError::Stopping) => Err(stopping_response()),
        Err(UpstreamConnectError::Timeout) => Err(timeout_response()),
        Err(UpstreamConnectError::Unavailable) => Err(unavailable_response()),
    }
}

pub(super) fn join_upstream(state: &EdgeState, path: &str) -> Result<reqwest::Url, Response> {
    match state.upstream_url.join(path) {
        Ok(url) => Ok(url),
        Err(_) => {
            state.record_upstream_failure();
            Err(error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider is unavailable.",
                None,
            ))
        }
    }
}
