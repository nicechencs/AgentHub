use std::time::Instant;

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use futures_util::StreamExt;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::bridge::grok_cli::{
    apply_grok_cli_identity_with, grok_cli_request_identity, inject_prompt_cache_key,
    is_reasoning_decode_failure, normalize_grok_build_tools, strip_encrypted_reasoning,
    GrokCliRequestIdentity,
};
use crate::bridge::runtime::BridgeUpstreamProtocol;
use crate::bridge::types::{EmissionState, RetryClass, RetryGate};
use crate::utils::redact::redact_text;

use super::http::{error_response, stopping_response, ListenerState};
use super::stream::UpstreamBodyError;
use super::{ANTHROPIC_API_VERSION, UPSTREAM_BODY_IDLE_TIMEOUT, UPSTREAM_RESPONSE_HEADER_TIMEOUT};

const ACCESS_JWT_EXPIRY_SKEW_SECS: i64 = 60;
const UPSTREAM_ERROR_BODY_LIMIT_BYTES: usize = 8 * 1024;

fn apply_upstream_auth(
    builder: reqwest::RequestBuilder,
    protocol: BridgeUpstreamProtocol,
    token: &str,
    grok_identity: Option<&GrokCliRequestIdentity>,
) -> reqwest::RequestBuilder {
    match protocol {
        BridgeUpstreamProtocol::KimiChatCompletions
        | BridgeUpstreamProtocol::CodexResponsesOauth => builder.bearer_auth(token),
        BridgeUpstreamProtocol::XaiResponsesOauth => {
            apply_grok_cli_identity_with(builder.bearer_auth(token), grok_identity)
        }
        BridgeUpstreamProtocol::AnthropicMessages => builder
            .header("x-api-key", token)
            .header("anthropic-version", ANTHROPIC_API_VERSION),
    }
}

pub(super) fn grok_identity_for(
    protocol: BridgeUpstreamProtocol,
    request_id: &str,
    headers: &axum::http::HeaderMap,
    body: &Value,
    model: Option<&str>,
) -> Option<GrokCliRequestIdentity> {
    if protocol != BridgeUpstreamProtocol::XaiResponsesOauth {
        return None;
    }
    Some(grok_cli_request_identity(request_id, headers, body, model))
}

pub(super) fn prepare_grok_build_body(
    protocol: BridgeUpstreamProtocol,
    body: &mut Value,
    seed: Option<&str>,
) {
    if protocol != BridgeUpstreamProtocol::XaiResponsesOauth {
        return;
    }
    normalize_grok_build_tools(body);
    inject_prompt_cache_key(body, seed);
}

fn grok_replay_model(body: &Value, fallback: Option<&str>) -> String {
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

fn apply_grok_replay(state: &ListenerState, body: &mut Value, seed: Option<&str>) {
    if state.upstream.protocol != BridgeUpstreamProtocol::XaiResponsesOauth {
        return;
    }
    let model = grok_replay_model(body, state.upstream.model.as_deref());
    state.grok_replay.apply(body, &model, seed);
}

pub(super) fn capture_grok_completed(state: &ListenerState, seed: Option<&str>, completed: &Value) {
    if state.upstream.protocol != BridgeUpstreamProtocol::XaiResponsesOauth {
        return;
    }
    let model = grok_replay_model(completed, state.upstream.model.as_deref());
    state.grok_replay.store_completed(&model, seed, completed);
}

pub(super) fn capture_grok_sse(state: &ListenerState, seed: Option<&str>, sse: &str) {
    if state.upstream.protocol != BridgeUpstreamProtocol::XaiResponsesOauth {
        return;
    }
    let model = grok_replay_model(&Value::Null, state.upstream.model.as_deref());
    state.grok_replay.store_sse(&model, seed, sse);
}

fn map_upstream_http_error(
    state: &ListenerState,
    request_id: &str,
    started: Instant,
    status: StatusCode,
    retry_after: Option<HeaderValue>,
    upstream_detail: Option<&str>,
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

fn extract_upstream_error_detail(body: &[u8]) -> Option<String> {
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

fn oauth_subscription_protocol(protocol: BridgeUpstreamProtocol) -> bool {
    matches!(
        protocol,
        BridgeUpstreamProtocol::CodexResponsesOauth | BridgeUpstreamProtocol::XaiResponsesOauth
    )
}

fn access_jwt_near_expiry(token: &str) -> bool {
    let Some(claims) = crate::oauth::decode_jwt_payload(token) else {
        return false;
    };
    let Some(exp) = claims.get("exp").and_then(|value| value.as_i64()) else {
        return false;
    };
    let now = chrono::Utc::now().timestamp();
    exp <= now + ACCESS_JWT_EXPIRY_SKEW_SECS
}

fn try_reload_upstream_auth(state: &ListenerState) -> bool {
    let Some(reload) = state.reload_upstream_auth.as_ref() else {
        return false;
    };
    let current = state.upstream.auth.token();
    let Some(next) = reload() else {
        return false;
    };
    let next = next.trim();
    if next.is_empty() || next == current {
        return false;
    }
    state.upstream.auth.replace_token(next);
    true
}

async fn read_bounded_upstream_error(
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

async fn post_upstream(
    state: &ListenerState,
    url: &reqwest::Url,
    protocol: BridgeUpstreamProtocol,
    grok_identity: Option<&GrokCliRequestIdentity>,
    body: &Value,
) -> Result<reqwest::Response, Response> {
    let token = state.upstream.auth.token();
    let builder = apply_upstream_auth(
        state.client.post(url.clone()).json(body),
        protocol,
        &token,
        grok_identity,
    );
    let result = tokio::select! {
        _ = state.force_shutdown.cancelled() => return Err(stopping_response()),
        result = tokio::time::timeout(UPSTREAM_RESPONSE_HEADER_TIMEOUT, builder.send()) => match result {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, op = "upstream", code = "header_timeout", status = 504_u16, "bridge upstream response headers timed out");
                state.record_upstream_failure();
                return Err(error_response(
                    StatusCode::GATEWAY_TIMEOUT,
                    "upstream_timeout",
                    "The upstream model provider timed out.",
                    None,
                ));
            }
        },
    };
    match result {
        Ok(response) => Ok(response),
        Err(_) => {
            tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, op = "upstream", code = "unavailable", status = 502_u16, "bridge upstream unavailable");
            state.record_upstream_failure();
            Err(error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_unavailable",
                "The upstream model provider is unavailable.",
                None,
            ))
        }
    }
}

pub(super) fn join_upstream(state: &ListenerState, path: &str) -> Result<reqwest::Url, Response> {
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

pub(super) async fn send_upstream_with_grok_recovery(
    state: &ListenerState,
    url: reqwest::Url,
    protocol: BridgeUpstreamProtocol,
    request_id: &str,
    started: Instant,
    mut identity: Option<GrokCliRequestIdentity>,
    mut body: Value,
    cache_seed: Option<&str>,
) -> Result<reqwest::Response, Response> {
    if protocol == BridgeUpstreamProtocol::XaiResponsesOauth {
        apply_grok_replay(state, &mut body, cache_seed);
    }
    let retry_gate = RetryGate::default();
    let mut auth_reloaded = false;
    // Only consume the 401 retry slot when a follow/refresh actually swapped
    // the in-memory bearer. A no-op near-expiry reread must still allow one 401 retry.
    if oauth_subscription_protocol(protocol) && access_jwt_near_expiry(&state.upstream.auth.token())
    {
        auth_reloaded = try_reload_upstream_auth(state);
    }
    let mut attempt = 0u8;
    loop {
        let response = post_upstream(state, &url, protocol, identity.as_ref(), &body).await?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let retry_after = response.headers().get(header::RETRY_AFTER).cloned();
        let auth_attempts = if auth_reloaded { 1 } else { 0 };
        if status == StatusCode::UNAUTHORIZED
            && oauth_subscription_protocol(protocol)
            && retry_gate.can_retry(EmissionState::Idle, RetryClass::Transient, auth_attempts)
            && try_reload_upstream_auth(state)
        {
            auth_reloaded = true;
            tracing::info!(
                target: "core.adapter",
                profile_id = %state.profile_id,
                request_id = %request_id,
                "retrying upstream request after oauth access reload"
            );
            continue;
        }
        let can_recover = protocol == BridgeUpstreamProtocol::XaiResponsesOauth
            && status == StatusCode::BAD_REQUEST
            && attempt < 2;
        let error_body = match read_bounded_upstream_error(response, &state.force_shutdown).await {
            Ok(body) => body,
            Err(UpstreamBodyError::Stopping) => return Err(stopping_response()),
            Err(UpstreamBodyError::InvalidOrTooLarge) => Vec::new(),
        };
        if !can_recover {
            let detail = extract_upstream_error_detail(&error_body);
            return Err(map_upstream_http_error(
                state,
                request_id,
                started,
                status,
                retry_after,
                detail.as_deref(),
            ));
        }
        let err_text = String::from_utf8_lossy(&error_body);
        if !is_reasoning_decode_failure(&err_text) {
            let detail = extract_upstream_error_detail(&error_body);
            return Err(map_upstream_http_error(
                state,
                request_id,
                started,
                status,
                retry_after,
                detail.as_deref(),
            ));
        }
        let model = grok_replay_model(&body, state.upstream.model.as_deref());
        state.grok_replay.clear(&model, cache_seed);
        strip_encrypted_reasoning(&mut body);
        attempt += 1;
        if attempt >= 2 {
            if let Some(identity) = identity.as_mut() {
                identity.session_id = None;
            }
            if let Some(object) = body.as_object_mut() {
                object.remove("prompt_cache_key");
            }
        }
        tracing::info!(
            target: "core.adapter",
            profile_id = %state.profile_id,
            request_id = %request_id,
            attempt,
            "retrying Grok request after encrypted reasoning rejection"
        );
    }
}
