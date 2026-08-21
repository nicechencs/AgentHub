use std::convert::Infallible;
use std::time::Instant;

use async_stream::stream;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::OwnedSemaphorePermit;
use tokio_util::sync::CancellationToken;

use crate::bridge::grok_cli::{
    apply_grok_cli_identity_with, extract_prompt_cache_seed, grok_cli_request_identity,
    inject_prompt_cache_key, normalize_grok_build_tools, GrokCliRequestIdentity,
};
use crate::bridge::protocol::anthropic_messages::{
    anthropic_message_to_ir, encode_anthropic_message, encode_anthropic_sse,
    parse_messages_request, to_anthropic_messages_request, AnthropicStreamToIr,
};
use crate::bridge::protocol::chat::{
    encode_chat_from_ir, encode_chat_sse, parse_chat_request, ChatStreamToIr,
};
use crate::bridge::protocol::responses::{
    apply_official_codex_model, encode_responses_from_ir, parse_responses_request,
    responses_output_to_ir, to_grok_responses_request, to_kimi_chat_request, to_responses_request,
    IrToResponsesSse, ResponsesStreamToIr,
};
use crate::bridge::runtime::{BridgeLocalSurface, BridgeUpstreamProtocol};
use crate::bridge::types::{BridgeEvent, BridgeRequest, IrEvent, ProtocolError};

use super::http::{
    error_response, has_valid_local_auth, log_protocol_error, protocol_error_response,
    read_request_json, sse_data_payload, sse_frame_end_deque, stopping_response,
    stream_error_frame, ListenerState,
};
use super::{
    ANTHROPIC_API_VERSION, BODY_LIMIT_BYTES, STREAM_LIMIT_BYTES, UPSTREAM_BODY_IDLE_TIMEOUT,
    UPSTREAM_NON_STREAM_TIMEOUT, UPSTREAM_RESPONSE_HEADER_TIMEOUT, UPSTREAM_STREAM_IDLE_TIMEOUT,
};

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

    fn responses_passthrough(self) -> bool {
        self.serves_responses()
            && matches!(
                self.protocol,
                BridgeUpstreamProtocol::CodexResponsesOauth
                    | BridgeUpstreamProtocol::XaiResponsesOauth
            )
    }
}

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

fn grok_identity_for(
    protocol: BridgeUpstreamProtocol,
    request_id: &str,
    headers: &HeaderMap,
    body: &Value,
    model: Option<&str>,
) -> Option<GrokCliRequestIdentity> {
    if protocol != BridgeUpstreamProtocol::XaiResponsesOauth {
        return None;
    }
    Some(grok_cli_request_identity(request_id, headers, body, model))
}

fn prepare_grok_build_body(protocol: BridgeUpstreamProtocol, body: &mut Value, seed: Option<&str>) {
    if protocol != BridgeUpstreamProtocol::XaiResponsesOauth {
        return;
    }
    normalize_grok_build_tools(body);
    inject_prompt_cache_key(body, seed);
}

pub(super) async fn handle_responses(state: ListenerState, request: Request) -> Response {
    let request_id = uuid::Uuid::new_v4().to_string();
    let started = Instant::now();
    // Do this before extracting JSON. Axum's Json extractor would otherwise read a potentially
    // slow or oversized body for an unauthenticated peer.
    if !has_valid_local_auth(request.headers(), &state.local_token) {
        tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "responses", code = "unauthorized", status = 401_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge request rejected");
        return error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "Invalid local bearer token.",
            None,
        );
    }
    if state.force_shutdown.is_cancelled() {
        return stopping_response();
    }
    let permit = match state.admission.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "responses", code = "overloaded", status = 429_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge profile is at request capacity");
            return error_response(
                StatusCode::TOO_MANY_REQUESTS,
                "bridge_overloaded",
                "The local bridge is temporarily busy.",
                None,
            );
        }
    };
    let incoming_headers = request.headers().clone();
    let body = match read_request_json(request).await {
        Ok(body) => body,
        Err(response) => return response,
    };

    let selector = ProtocolSelector::from_listener(&state);
    let protocol = state.upstream.protocol;
    let grok_identity = grok_identity_for(
        protocol,
        &request_id,
        &incoming_headers,
        &body,
        state.upstream.model.as_deref(),
    );
    let cache_seed = grok_identity
        .as_ref()
        .and_then(|_| extract_prompt_cache_seed(&incoming_headers, &body));
    let (mut upstream_body, stream_requested) = if selector.responses_passthrough() {
        match passthrough_responses_body(body, &state) {
            Ok(pair) => pair,
            Err(response) => return response,
        }
    } else {
        let request = match parse_responses_request(&body) {
            Ok(request) => request,
            Err(error) => {
                log_protocol_error(&state, &request_id, started, &error);
                return protocol_error_response(error);
            }
        };
        let stream_requested = request.stream;
        let mut upstream_body = match protocol {
            BridgeUpstreamProtocol::KimiChatCompletions => to_kimi_chat_request(&request),
            BridgeUpstreamProtocol::AnthropicMessages => to_anthropic_messages_request(&request),
            BridgeUpstreamProtocol::CodexResponsesOauth
            | BridgeUpstreamProtocol::XaiResponsesOauth => {
                unreachable!("Responses passthrough owns this protocol")
            }
        };
        if let Some(model) = &state.upstream.model {
            upstream_body["model"] = Value::String(model.clone());
        }
        (upstream_body, stream_requested)
    };
    prepare_grok_build_body(protocol, &mut upstream_body, cache_seed.as_deref());
    let path = match protocol {
        BridgeUpstreamProtocol::KimiChatCompletions => "chat/completions",
        BridgeUpstreamProtocol::AnthropicMessages => "messages",
        BridgeUpstreamProtocol::CodexResponsesOauth | BridgeUpstreamProtocol::XaiResponsesOauth => {
            "responses"
        }
    };
    let url = match state.upstream_url.join(path) {
        Ok(url) => url,
        Err(_) => {
            state.record_upstream_failure();
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider is unavailable.",
                None,
            );
        }
    };
    let builder = apply_upstream_auth(
        state.client.post(url).json(&upstream_body),
        protocol,
        state.upstream.auth.token(),
        grok_identity.as_ref(),
    );
    let upstream_request = builder.send();
    let upstream = tokio::select! {
        _ = state.force_shutdown.cancelled() => return stopping_response(),
        result = tokio::time::timeout(UPSTREAM_RESPONSE_HEADER_TIMEOUT, upstream_request) => match result {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "header_timeout", status = 504_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge upstream response headers timed out");
                state.record_upstream_failure();
                return error_response(
                    StatusCode::GATEWAY_TIMEOUT,
                    "upstream_timeout",
                    "The upstream model provider timed out.",
                    None,
                );
            }
        },
    };
    let response = match upstream {
        Ok(response) => response,
        Err(_) => {
            tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "unavailable", status = 502_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge upstream unavailable");
            state.record_upstream_failure();
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_unavailable",
                "The upstream model provider is unavailable.",
                None,
            );
        }
    };
    if !response.status().is_success() {
        let status = response.status();
        let retry_after = response.headers().get(header::RETRY_AFTER).cloned();
        let local_status = if status == StatusCode::TOO_MANY_REQUESTS {
            StatusCode::TOO_MANY_REQUESTS
        } else {
            StatusCode::BAD_GATEWAY
        };
        tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "upstream_status", status = status.as_u16(), elapsed_ms = started.elapsed().as_millis() as u64, "bridge upstream returned an error");
        state.record_upstream_failure();
        return error_response(
            local_status,
            "upstream_error",
            "The upstream model provider returned an error.",
            retry_after,
        );
    }
    if stream_requested {
        if selector.responses_passthrough() {
            passthrough_sse_response(state, response, request_id, started, permit)
        } else {
            stream_response(state, response, request_id, started, permit)
        }
    } else {
        let force_shutdown = state.force_shutdown.clone();
        tokio::select! {
            _ = force_shutdown.cancelled() => stopping_response(),
            result = tokio::time::timeout(
                UPSTREAM_NON_STREAM_TIMEOUT,
                non_stream_response(state.clone(), response, request_id, started, permit),
            ) => match result {
                Ok(response) => response,
                Err(_) => {
                    state.record_upstream_failure();
                    error_response(
                        StatusCode::GATEWAY_TIMEOUT,
                        "upstream_timeout",
                        "The upstream model provider timed out.",
                        None,
                    )
                }
            },
        }
    }
}

fn passthrough_responses_body(
    body: Value,
    state: &ListenerState,
) -> Result<(Value, bool), Response> {
    if !body.is_object() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "The request body must be valid JSON.",
            None,
        ));
    }
    let stream_requested = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let incoming_model = body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let mut upstream_body = body;
    match state.upstream.protocol {
        BridgeUpstreamProtocol::CodexResponsesOauth => {
            apply_official_codex_model(
                &mut upstream_body,
                &incoming_model,
                state.upstream.model.as_deref(),
            );
        }
        BridgeUpstreamProtocol::XaiResponsesOauth => {
            if let Some(model) = &state.upstream.model {
                if !model.trim().is_empty() {
                    upstream_body["model"] = Value::String(model.clone());
                }
            }
        }
        BridgeUpstreamProtocol::KimiChatCompletions | BridgeUpstreamProtocol::AnthropicMessages => {
            unreachable!("passthrough is Responses-to-Responses only")
        }
    }
    Ok((upstream_body, stream_requested))
}

pub(super) async fn handle_messages(state: ListenerState, request: Request) -> Response {
    let request_id = uuid::Uuid::new_v4().to_string();
    let started = Instant::now();
    if !has_valid_local_auth(request.headers(), &state.local_token) {
        tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "messages", code = "unauthorized", status = 401_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge request rejected");
        return error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "Invalid local bearer token.",
            None,
        );
    }
    if state.force_shutdown.is_cancelled() {
        return stopping_response();
    }
    let permit = match state.admission.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "messages", code = "overloaded", status = 429_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge profile is at request capacity");
            return error_response(
                StatusCode::TOO_MANY_REQUESTS,
                "bridge_overloaded",
                "The local bridge is temporarily busy.",
                None,
            );
        }
    };
    let incoming_headers = request.headers().clone();
    let body = match read_request_json(request).await {
        Ok(body) => body,
        Err(response) => return response,
    };
    let grok_identity = grok_identity_for(
        state.upstream.protocol,
        &request_id,
        &incoming_headers,
        &body,
        state.upstream.model.as_deref(),
    );
    let cache_seed = grok_identity
        .as_ref()
        .and_then(|_| extract_prompt_cache_seed(&incoming_headers, &body));
    let request = match parse_messages_request(&body) {
        Ok(request) => request,
        Err(error) => {
            log_protocol_error(&state, &request_id, started, &error);
            return protocol_error_response(error);
        }
    };
    let stream_requested = request.stream;
    let protocol = state.upstream.protocol;
    let mut upstream_body = match protocol {
        BridgeUpstreamProtocol::KimiChatCompletions => to_kimi_chat_request(&request),
        BridgeUpstreamProtocol::CodexResponsesOauth => to_responses_request(&request),
        BridgeUpstreamProtocol::XaiResponsesOauth => to_grok_responses_request(&request),
        BridgeUpstreamProtocol::AnthropicMessages => {
            unreachable!("messages handler does not accept Anthropic upstream")
        }
    };
    if protocol == BridgeUpstreamProtocol::XaiResponsesOauth {
        inject_prompt_cache_key(&mut upstream_body, cache_seed.as_deref());
    }
    match protocol {
        BridgeUpstreamProtocol::CodexResponsesOauth => {
            apply_official_codex_model(
                &mut upstream_body,
                &request.model,
                state.upstream.model.as_deref(),
            );
        }
        BridgeUpstreamProtocol::KimiChatCompletions | BridgeUpstreamProtocol::XaiResponsesOauth => {
            if let Some(model) = &state.upstream.model {
                upstream_body["model"] = Value::String(model.clone());
            }
        }
        BridgeUpstreamProtocol::AnthropicMessages => {
            unreachable!("messages handler does not accept Anthropic upstream")
        }
    }
    let path = match protocol {
        BridgeUpstreamProtocol::KimiChatCompletions => "chat/completions",
        BridgeUpstreamProtocol::CodexResponsesOauth | BridgeUpstreamProtocol::XaiResponsesOauth => {
            "responses"
        }
        BridgeUpstreamProtocol::AnthropicMessages => {
            unreachable!("messages handler does not accept Anthropic upstream")
        }
    };
    let url = match state.upstream_url.join(path) {
        Ok(url) => url,
        Err(_) => {
            state.record_upstream_failure();
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider is unavailable.",
                None,
            );
        }
    };
    let builder = apply_upstream_auth(
        state.client.post(url).json(&upstream_body),
        protocol,
        state.upstream.auth.token(),
        grok_identity.as_ref(),
    );
    let upstream = tokio::select! {
        _ = state.force_shutdown.cancelled() => return stopping_response(),
        result = tokio::time::timeout(
            UPSTREAM_RESPONSE_HEADER_TIMEOUT,
            builder.send(),
        ) => match result {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "header_timeout", status = 504_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge upstream response headers timed out");
                state.record_upstream_failure();
                return error_response(
                    StatusCode::GATEWAY_TIMEOUT,
                    "upstream_timeout",
                    "The upstream model provider timed out.",
                    None,
                );
            }
        },
    };
    let response = match upstream {
        Ok(response) => response,
        Err(_) => {
            tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "unavailable", status = 502_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge upstream unavailable");
            state.record_upstream_failure();
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_unavailable",
                "The upstream model provider is unavailable.",
                None,
            );
        }
    };
    if !response.status().is_success() {
        let status = response.status();
        let retry_after = response.headers().get(header::RETRY_AFTER).cloned();
        let local_status = if status == StatusCode::TOO_MANY_REQUESTS {
            StatusCode::TOO_MANY_REQUESTS
        } else {
            StatusCode::BAD_GATEWAY
        };
        tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "upstream_status", status = status.as_u16(), elapsed_ms = started.elapsed().as_millis() as u64, "bridge upstream returned an error");
        state.record_upstream_failure();
        return error_response(
            local_status,
            "upstream_error",
            "The upstream model provider returned an error.",
            retry_after,
        );
    }
    if stream_requested {
        messages_stream_response(state, response, request_id, started, permit)
    } else {
        let force_shutdown = state.force_shutdown.clone();
        tokio::select! {
            _ = force_shutdown.cancelled() => stopping_response(),
            result = tokio::time::timeout(
                UPSTREAM_NON_STREAM_TIMEOUT,
                messages_non_stream_response(state.clone(), response, request_id, started, permit),
            ) => match result {
                Ok(response) => response,
                Err(_) => {
                    state.record_upstream_failure();
                    error_response(
                        StatusCode::GATEWAY_TIMEOUT,
                        "upstream_timeout",
                        "The upstream model provider timed out.",
                        None,
                    )
                }
            },
        }
    }
}

pub(super) async fn handle_chat_completions(state: ListenerState, request: Request) -> Response {
    let request_id = uuid::Uuid::new_v4().to_string();
    let started = Instant::now();
    if !has_valid_local_auth(request.headers(), &state.local_token) {
        tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "chat", code = "unauthorized", status = 401_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge request rejected");
        return error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "Invalid local bearer token.",
            None,
        );
    }
    if state.force_shutdown.is_cancelled() {
        return stopping_response();
    }
    let permit = match state.admission.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "chat", code = "overloaded", status = 429_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge profile is at request capacity");
            return error_response(
                StatusCode::TOO_MANY_REQUESTS,
                "bridge_overloaded",
                "The local bridge is temporarily busy.",
                None,
            );
        }
    };
    let body = match read_request_json(request).await {
        Ok(body) => body,
        Err(response) => return response,
    };
    let request = match parse_chat_request(&body) {
        Ok(request) => request,
        Err(error) => {
            log_protocol_error(&state, &request_id, started, &error);
            return protocol_error_response(error);
        }
    };
    let stream_requested = request.stream;
    let protocol = state.upstream.protocol;
    let mut upstream_body = match protocol {
        BridgeUpstreamProtocol::CodexResponsesOauth => to_responses_request(&request),
        BridgeUpstreamProtocol::KimiChatCompletions
        | BridgeUpstreamProtocol::AnthropicMessages
        | BridgeUpstreamProtocol::XaiResponsesOauth => {
            unreachable!("chat completions handler owns Codex Responses OAuth")
        }
    };
    apply_official_codex_model(
        &mut upstream_body,
        &request.model,
        state.upstream.model.as_deref(),
    );
    let url = match state.upstream_url.join("responses") {
        Ok(url) => url,
        Err(_) => {
            state.record_upstream_failure();
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider is unavailable.",
                None,
            );
        }
    };
    let builder = state
        .client
        .post(url)
        .json(&upstream_body)
        .bearer_auth(state.upstream.auth.token());
    let upstream = tokio::select! {
        _ = state.force_shutdown.cancelled() => return stopping_response(),
        result = tokio::time::timeout(
            UPSTREAM_RESPONSE_HEADER_TIMEOUT,
            builder.send(),
        ) => match result {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "header_timeout", status = 504_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge upstream response headers timed out");
                state.record_upstream_failure();
                return error_response(
                    StatusCode::GATEWAY_TIMEOUT,
                    "upstream_timeout",
                    "The upstream model provider timed out.",
                    None,
                );
            }
        },
    };
    let response = match upstream {
        Ok(response) => response,
        Err(_) => {
            tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "unavailable", status = 502_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge upstream unavailable");
            state.record_upstream_failure();
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_unavailable",
                "The upstream model provider is unavailable.",
                None,
            );
        }
    };
    if !response.status().is_success() {
        let status = response.status();
        let retry_after = response.headers().get(header::RETRY_AFTER).cloned();
        let local_status = if status == StatusCode::TOO_MANY_REQUESTS {
            StatusCode::TOO_MANY_REQUESTS
        } else {
            StatusCode::BAD_GATEWAY
        };
        tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "upstream_status", status = status.as_u16(), elapsed_ms = started.elapsed().as_millis() as u64, "bridge upstream returned an error");
        state.record_upstream_failure();
        return error_response(
            local_status,
            "upstream_error",
            "The upstream model provider returned an error.",
            retry_after,
        );
    }
    if stream_requested {
        chat_stream_response(state, response, request_id, started, permit)
    } else {
        let force_shutdown = state.force_shutdown.clone();
        tokio::select! {
            _ = force_shutdown.cancelled() => stopping_response(),
            result = tokio::time::timeout(
                UPSTREAM_NON_STREAM_TIMEOUT,
                chat_non_stream_response(state.clone(), response, request_id, started, permit),
            ) => match result {
                Ok(response) => response,
                Err(_) => {
                    state.record_upstream_failure();
                    error_response(
                        StatusCode::GATEWAY_TIMEOUT,
                        "upstream_timeout",
                        "The upstream model provider timed out.",
                        None,
                    )
                }
            },
        }
    }
}

pub(super) async fn messages_non_stream_response(
    state: ListenerState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    _permit: OwnedSemaphorePermit,
) -> Response {
    let upstream_body = match read_bounded_upstream_json(response, &state.force_shutdown).await {
        Ok(value) => value,
        Err(UpstreamBodyError::Stopping) => return stopping_response(),
        Err(UpstreamBodyError::InvalidOrTooLarge) => {
            state.record_upstream_failure();
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider returned an invalid response.",
                None,
            );
        }
    };
    let translated = match state.upstream.protocol {
        BridgeUpstreamProtocol::KimiChatCompletions => {
            crate::bridge::protocol::chat::translate_chat_response(
                &upstream_body,
                Some(&request_id),
            )
            .and_then(|responses| responses_output_to_ir(&responses))
            .and_then(|ir| encode_anthropic_message(&ir))
        }
        BridgeUpstreamProtocol::CodexResponsesOauth | BridgeUpstreamProtocol::XaiResponsesOauth => {
            responses_output_to_ir(&upstream_body).and_then(|ir| encode_anthropic_message(&ir))
        }
        BridgeUpstreamProtocol::AnthropicMessages => {
            unreachable!("messages handler does not accept Anthropic upstream")
        }
    };
    match translated {
        Ok(value) => {
            state.record_upstream_success();
            tracing::info!(target: "core.adapter.protocol", profile_id = %state.profile_id, request_id = %request_id, op = "messages", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge response completed");
            Json(value).into_response()
        }
        Err(error) => {
            state.record_upstream_failure();
            log_protocol_error(&state, &request_id, started, &error);
            protocol_error_response(error)
        }
    }
}

pub(super) async fn chat_non_stream_response(
    state: ListenerState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    _permit: OwnedSemaphorePermit,
) -> Response {
    let upstream_body = match read_bounded_upstream_json(response, &state.force_shutdown).await {
        Ok(value) => value,
        Err(UpstreamBodyError::Stopping) => return stopping_response(),
        Err(UpstreamBodyError::InvalidOrTooLarge) => {
            state.record_upstream_failure();
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider returned an invalid response.",
                None,
            );
        }
    };
    let translated = match state.upstream.protocol {
        BridgeUpstreamProtocol::CodexResponsesOauth => responses_output_to_ir(&upstream_body)
            .and_then(|ir| encode_chat_from_ir(&ir, Some(&request_id))),
        BridgeUpstreamProtocol::KimiChatCompletions
        | BridgeUpstreamProtocol::AnthropicMessages
        | BridgeUpstreamProtocol::XaiResponsesOauth => {
            unreachable!("chat completions handler owns Codex Responses OAuth")
        }
    };
    match translated {
        Ok(value) => {
            state.record_upstream_success();
            tracing::info!(target: "core.adapter.protocol", profile_id = %state.profile_id, request_id = %request_id, op = "chat", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge response completed");
            Json(value).into_response()
        }
        Err(error) => {
            state.record_upstream_failure();
            log_protocol_error(&state, &request_id, started, &error);
            protocol_error_response(error)
        }
    }
}

pub(super) async fn non_stream_response(
    state: ListenerState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    _permit: OwnedSemaphorePermit,
) -> Response {
    let upstream_body = match read_bounded_upstream_json(response, &state.force_shutdown).await {
        Ok(value) => value,
        Err(UpstreamBodyError::Stopping) => return stopping_response(),
        Err(UpstreamBodyError::InvalidOrTooLarge) => {
            state.record_upstream_failure();
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider returned an invalid response.",
                None,
            );
        }
    };
    let translated = match state.upstream.protocol {
        BridgeUpstreamProtocol::KimiChatCompletions => {
            crate::bridge::protocol::chat::translate_chat_response(
                &upstream_body,
                Some(&request_id),
            )
        }
        BridgeUpstreamProtocol::AnthropicMessages => anthropic_message_to_ir(&upstream_body)
            .and_then(|ir| encode_responses_from_ir(&ir, Some(&request_id))),
        BridgeUpstreamProtocol::CodexResponsesOauth | BridgeUpstreamProtocol::XaiResponsesOauth => {
            Ok(upstream_body)
        }
    };
    match translated {
        Ok(value) => {
            state.record_upstream_success();
            tracing::info!(target: "core.adapter.protocol", profile_id = %state.profile_id, request_id = %request_id, op = "responses", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge response completed");
            Json(value).into_response()
        }
        Err(error) => {
            state.record_upstream_failure();
            log_protocol_error(&state, &request_id, started, &error);
            protocol_error_response(error)
        }
    }
}

enum UpstreamBodyError {
    Stopping,
    InvalidOrTooLarge,
}

async fn read_bounded_upstream_json(
    response: reqwest::Response,
    force_shutdown: &CancellationToken,
) -> Result<Value, UpstreamBodyError> {
    if response
        .content_length()
        .is_some_and(|length| length > BODY_LIMIT_BYTES as u64)
    {
        return Err(UpstreamBodyError::InvalidOrTooLarge);
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = tokio::select! {
        _ = force_shutdown.cancelled() => return Err(UpstreamBodyError::Stopping),
        next = tokio::time::timeout(UPSTREAM_BODY_IDLE_TIMEOUT, stream.next()) => match next {
            Ok(next) => next,
            Err(_) => return Err(UpstreamBodyError::InvalidOrTooLarge),
        },
    } {
        let chunk = chunk.map_err(|_| UpstreamBodyError::InvalidOrTooLarge)?;
        if body.len().saturating_add(chunk.len()) > BODY_LIMIT_BYTES {
            return Err(UpstreamBodyError::InvalidOrTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| UpstreamBodyError::InvalidOrTooLarge)
}

enum StreamCodec {
    Kimi(crate::bridge::protocol::chat::ResponsesSseTranslator),
    Anthropic {
        ir: AnthropicStreamToIr,
        out: IrToResponsesSse,
    },
}

impl StreamCodec {
    fn new(protocol: BridgeUpstreamProtocol, request_id: String, model: String) -> Self {
        match protocol {
            BridgeUpstreamProtocol::KimiChatCompletions => Self::Kimi(
                crate::bridge::protocol::chat::ResponsesSseTranslator::new(request_id, model),
            ),
            BridgeUpstreamProtocol::AnthropicMessages => Self::Anthropic {
                ir: AnthropicStreamToIr::new(),
                out: IrToResponsesSse::new(request_id, model),
            },
            BridgeUpstreamProtocol::CodexResponsesOauth
            | BridgeUpstreamProtocol::XaiResponsesOauth => {
                unreachable!("Responses passthrough owns this protocol")
            }
        }
    }

    fn push(&mut self, value: &Value) -> Result<Vec<BridgeEvent>, ProtocolError> {
        match self {
            Self::Kimi(translator) => translator.push_chunk(value),
            Self::Anthropic { ir, out } => {
                let events = ir.push_event(value)?;
                let mut frames = Vec::new();
                for event in events {
                    frames.extend(out.push_event(&event)?);
                }
                Ok(frames)
            }
        }
    }

    fn finish(&mut self) -> Result<Vec<BridgeEvent>, ProtocolError> {
        match self {
            Self::Kimi(translator) => Ok(translator.finish()),
            Self::Anthropic { ir, out } => {
                let events = ir.finish();
                let mut frames = Vec::new();
                for event in events {
                    frames.extend(out.push_event(&event)?);
                }
                frames.extend(out.finish());
                Ok(frames)
            }
        }
    }

    fn completed(&self) -> bool {
        match self {
            Self::Kimi(_) => false,
            Self::Anthropic { ir, .. } => ir.completed(),
        }
    }

    fn treats_done_marker(&self) -> bool {
        matches!(self, Self::Kimi(_))
    }
}

fn passthrough_sse_response(
    state: ListenerState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
) -> Response {
    let profile_id = state.profile_id.clone();
    let force_shutdown = state.force_shutdown.clone();
    let observed = state.clone();
    let bytes = response.bytes_stream();
    let output = stream! {
        let _permit = permit;
        let mut upstream_bytes = 0usize;
        futures_util::pin_mut!(bytes);
        loop {
            let next = tokio::select! {
                _ = force_shutdown.cancelled() => {
                    yield Ok::<_, Infallible>(stream_error_frame());
                    return;
                }
                next = tokio::time::timeout(UPSTREAM_STREAM_IDLE_TIMEOUT, bytes.next()) => match next {
                    Ok(next) => next,
                    Err(_) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                },
            };
            let Some(chunk) = next else { break; };
            let Ok(chunk) = chunk else {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            };
            if upstream_bytes.saturating_add(chunk.len()) > STREAM_LIMIT_BYTES {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            }
            upstream_bytes += chunk.len();
            yield Ok::<_, Infallible>(chunk);
        }
        observed.record_upstream_success();
        tracing::info!(target: "core.adapter.protocol", profile_id = %profile_id, request_id = %request_id, op = "responses_passthrough_stream", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge stream completed");
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    (StatusCode::OK, headers, Body::from_stream(output)).into_response()
}

pub(super) fn stream_response(
    state: ListenerState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
) -> Response {
    let model = state.upstream.model.clone().unwrap_or_default();
    let profile_id = state.profile_id.clone();
    let force_shutdown = state.force_shutdown.clone();
    let observed = state.clone();
    let protocol = state.upstream.protocol;
    let bytes = response.bytes_stream();
    let output = stream! {
        let mut translator = StreamCodec::new(protocol, request_id.clone(), model);
        // `VecDeque` lets us consume complete SSE frames from the front without repeatedly
        // moving the unread tail. The cap counts all upstream bytes, not merely the current
        // partial frame, and the output cap protects a pathological translator expansion.
        let mut buffer = std::collections::VecDeque::new();
        let mut upstream_bytes = 0usize;
        let mut output_bytes = 0usize;
        let _permit = permit;
        let mut saw_done = false;
        futures_util::pin_mut!(bytes);
        'upstream: loop {
            let next = tokio::select! {
                _ = force_shutdown.cancelled() => {
                    yield Ok::<_, Infallible>(stream_error_frame());
                    return;
                }
                next = tokio::time::timeout(UPSTREAM_STREAM_IDLE_TIMEOUT, bytes.next()) => match next {
                    Ok(next) => next,
                    Err(_) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                },
            };
            let Some(chunk) = next else { break; };
            let Ok(chunk) = chunk else {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            };
            if upstream_bytes.saturating_add(chunk.len()) > STREAM_LIMIT_BYTES {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            }
            upstream_bytes += chunk.len();
            buffer.extend(chunk.iter().copied());
            while let Some((frame_end, delimiter_len)) = sse_frame_end_deque(&buffer) {
                let frame = buffer.drain(..frame_end).collect::<Vec<_>>();
                for _ in 0..delimiter_len {
                    let _ = buffer.pop_front();
                }
                let payload = match sse_data_payload(&frame) {
                    Ok(payload) => payload,
                    Err(()) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                };
                let Some(payload) = payload else { continue; };
                if payload.is_empty() { continue; }
                if payload == "[DONE]" {
                    if translator.treats_done_marker() {
                        saw_done = true;
                        break 'upstream;
                    }
                    continue;
                }
                let Ok(value) = serde_json::from_str::<Value>(&payload) else {
                    observed.record_upstream_failure();
                    yield Ok::<_, Infallible>(stream_error_frame());
                    return;
                };
                match translator.push(&value) {
                    Ok(events) => for event in events {
                        let frame = crate::bridge::protocol::chat::sse_frame(&event);
                        if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                            observed.record_upstream_failure();
                            yield Ok::<_, Infallible>(stream_error_frame());
                            return;
                        }
                        output_bytes += frame.len();
                        yield Ok::<_, Infallible>(axum::body::Bytes::from(frame));
                    },
                    Err(_) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                }
                if translator.completed() {
                    saw_done = true;
                    break 'upstream;
                }
            }
        }
        // A clean EOF without the provider's terminal marker is not a completed response. This
        // distinction matters to response clients, which otherwise persist a truncated answer.
        if !saw_done || !buffer.is_empty() {
            observed.record_upstream_failure();
            yield Ok::<_, Infallible>(stream_error_frame());
            return;
        }
        match translator.finish() {
            Ok(events) => {
                for event in events {
                    let frame = crate::bridge::protocol::chat::sse_frame(&event);
                    if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                    output_bytes += frame.len();
                    yield Ok::<_, Infallible>(axum::body::Bytes::from(frame));
                }
            }
            Err(_) => {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            }
        }
        observed.record_upstream_success();
        tracing::info!(target: "core.adapter.protocol", profile_id = %profile_id, request_id = %request_id, op = "stream", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge stream completed");
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    (StatusCode::OK, headers, Body::from_stream(output)).into_response()
}

enum MessagesStreamCodec {
    Chat(ChatStreamToIr),
    Responses(ResponsesStreamToIr),
}

impl MessagesStreamCodec {
    fn new(protocol: BridgeUpstreamProtocol, request_id: String, model: String) -> Self {
        match protocol {
            BridgeUpstreamProtocol::KimiChatCompletions => {
                Self::Chat(ChatStreamToIr::new(request_id, model))
            }
            BridgeUpstreamProtocol::CodexResponsesOauth
            | BridgeUpstreamProtocol::XaiResponsesOauth => {
                Self::Responses(ResponsesStreamToIr::new())
            }
            BridgeUpstreamProtocol::AnthropicMessages => {
                unreachable!("messages handler does not accept Anthropic upstream")
            }
        }
    }

    fn push(&mut self, value: &Value) -> Result<Vec<IrEvent>, ProtocolError> {
        match self {
            Self::Chat(translator) => translator.push_event(value),
            Self::Responses(translator) => translator.push_event(value),
        }
    }

    fn finish(&mut self) -> Vec<IrEvent> {
        match self {
            Self::Chat(translator) => translator.finish(),
            Self::Responses(translator) => translator.finish(),
        }
    }
}

pub(super) fn messages_stream_response(
    state: ListenerState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
) -> Response {
    let profile_id = state.profile_id.clone();
    let force_shutdown = state.force_shutdown.clone();
    let observed = state.clone();
    let bytes = response.bytes_stream();
    let output = stream! {
        let model = state.upstream.model.clone().unwrap_or_default();
        let protocol = state.upstream.protocol;
        let mut translator = MessagesStreamCodec::new(protocol, request_id.clone(), model);
        let mut ir_events: Vec<IrEvent> = Vec::new();
        let mut emitted_frames = 0usize;
        let mut buffer = std::collections::VecDeque::new();
        let mut upstream_bytes = 0usize;
        let mut output_bytes = 0usize;
        let _permit = permit;
        let mut saw_done = false;
        futures_util::pin_mut!(bytes);
        'upstream: loop {
            let next = tokio::select! {
                _ = force_shutdown.cancelled() => {
                    yield Ok::<_, Infallible>(stream_error_frame());
                    return;
                }
                next = tokio::time::timeout(UPSTREAM_STREAM_IDLE_TIMEOUT, bytes.next()) => match next {
                    Ok(next) => next,
                    Err(_) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                },
            };
            let Some(chunk) = next else { break; };
            let Ok(chunk) = chunk else {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            };
            if upstream_bytes.saturating_add(chunk.len()) > STREAM_LIMIT_BYTES {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            }
            upstream_bytes += chunk.len();
            buffer.extend(chunk.iter().copied());
            while let Some((frame_end, delimiter_len)) = sse_frame_end_deque(&buffer) {
                let frame = buffer.drain(..frame_end).collect::<Vec<_>>();
                for _ in 0..delimiter_len {
                    let _ = buffer.pop_front();
                }
                let payload = match sse_data_payload(&frame) {
                    Ok(payload) => payload,
                    Err(()) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                };
                let Some(payload) = payload else { continue; };
                if payload.is_empty() { continue; }
                if payload == "[DONE]" {
                    if protocol == BridgeUpstreamProtocol::KimiChatCompletions {
                        saw_done = true;
                        break 'upstream;
                    }
                    continue;
                }
                let Ok(value) = serde_json::from_str::<Value>(&payload) else {
                    observed.record_upstream_failure();
                    yield Ok::<_, Infallible>(stream_error_frame());
                    return;
                };
                let events = match translator.push(&value) {
                    Ok(events) => events,
                    Err(_) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                };
                let completed = events
                    .iter()
                    .any(|event| matches!(event, IrEvent::MessageEnd { .. }));
                ir_events.extend(events);
                let frames = match encode_anthropic_sse(&ir_events) {
                    Ok(frames) => frames,
                    Err(_) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                };
                for frame in frames.iter().skip(emitted_frames) {
                    if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                    output_bytes += frame.len();
                    yield Ok::<_, Infallible>(axum::body::Bytes::from(frame.clone()));
                }
                emitted_frames = frames.len();
                if completed {
                    saw_done = true;
                    break 'upstream;
                }
            }
        }
        if !saw_done || !buffer.is_empty() {
            observed.record_upstream_failure();
            yield Ok::<_, Infallible>(stream_error_frame());
            return;
        }
        ir_events.extend(translator.finish());
        let frames = match encode_anthropic_sse(&ir_events) {
            Ok(frames) => frames,
            Err(_) => {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            }
        };
        for frame in frames.iter().skip(emitted_frames) {
            if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            }
            output_bytes += frame.len();
            yield Ok::<_, Infallible>(axum::body::Bytes::from(frame.clone()));
        }
        observed.record_upstream_success();
        tracing::info!(target: "core.adapter.protocol", profile_id = %profile_id, request_id = %request_id, op = "messages_stream", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge stream completed");
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    (StatusCode::OK, headers, Body::from_stream(output)).into_response()
}

pub(super) fn chat_stream_response(
    state: ListenerState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
) -> Response {
    let profile_id = state.profile_id.clone();
    let force_shutdown = state.force_shutdown.clone();
    let observed = state.clone();
    let bytes = response.bytes_stream();
    let output = stream! {
        let model = state.upstream.model.clone().unwrap_or_default();
        let protocol = state.upstream.protocol;
        let mut translator = MessagesStreamCodec::new(protocol, request_id.clone(), model);
        let mut ir_events: Vec<IrEvent> = Vec::new();
        let mut emitted_frames = 0usize;
        let mut buffer = std::collections::VecDeque::new();
        let mut upstream_bytes = 0usize;
        let mut output_bytes = 0usize;
        let _permit = permit;
        let mut saw_done = false;
        futures_util::pin_mut!(bytes);
        'upstream: loop {
            let next = tokio::select! {
                _ = force_shutdown.cancelled() => {
                    yield Ok::<_, Infallible>(stream_error_frame());
                    return;
                }
                next = tokio::time::timeout(UPSTREAM_STREAM_IDLE_TIMEOUT, bytes.next()) => match next {
                    Ok(next) => next,
                    Err(_) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                },
            };
            let Some(chunk) = next else { break; };
            let Ok(chunk) = chunk else {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            };
            if upstream_bytes.saturating_add(chunk.len()) > STREAM_LIMIT_BYTES {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            }
            upstream_bytes += chunk.len();
            buffer.extend(chunk.iter().copied());
            while let Some((frame_end, delimiter_len)) = sse_frame_end_deque(&buffer) {
                let frame = buffer.drain(..frame_end).collect::<Vec<_>>();
                for _ in 0..delimiter_len {
                    let _ = buffer.pop_front();
                }
                let payload = match sse_data_payload(&frame) {
                    Ok(payload) => payload,
                    Err(()) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                };
                let Some(payload) = payload else { continue; };
                if payload.is_empty() { continue; };
                if payload == "[DONE]" {
                    continue;
                }
                let Ok(value) = serde_json::from_str::<Value>(&payload) else {
                    observed.record_upstream_failure();
                    yield Ok::<_, Infallible>(stream_error_frame());
                    return;
                };
                let events = match translator.push(&value) {
                    Ok(events) => events,
                    Err(_) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                };
                let completed = events
                    .iter()
                    .any(|event| matches!(event, IrEvent::MessageEnd { .. }));
                ir_events.extend(events);
                let frames = match encode_chat_sse(&ir_events, Some(&request_id)) {
                    Ok(frames) => frames,
                    Err(_) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                };
                for frame in frames.iter().skip(emitted_frames) {
                    if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                    output_bytes += frame.len();
                    yield Ok::<_, Infallible>(axum::body::Bytes::from(frame.clone()));
                }
                emitted_frames = frames.len();
                if completed {
                    saw_done = true;
                    break 'upstream;
                }
            }
        }
        if !saw_done || !buffer.is_empty() {
            observed.record_upstream_failure();
            yield Ok::<_, Infallible>(stream_error_frame());
            return;
        }
        ir_events.extend(translator.finish());
        let frames = match encode_chat_sse(&ir_events, Some(&request_id)) {
            Ok(frames) => frames,
            Err(_) => {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            }
        };
        for frame in frames.iter().skip(emitted_frames) {
            if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            }
            output_bytes += frame.len();
            yield Ok::<_, Infallible>(axum::body::Bytes::from(frame.clone()));
        }
        observed.record_upstream_success();
        tracing::info!(target: "core.adapter.protocol", profile_id = %profile_id, request_id = %request_id, op = "chat_stream", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge stream completed");
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    (StatusCode::OK, headers, Body::from_stream(output)).into_response()
}
