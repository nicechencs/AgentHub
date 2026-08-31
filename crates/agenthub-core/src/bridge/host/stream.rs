use std::convert::Infallible;
use std::time::Instant;

use async_stream::stream;
use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::OwnedSemaphorePermit;
use tokio_util::sync::CancellationToken;

use crate::bridge::account::PickedMember;
use crate::bridge::protocol::anthropic_messages::{
    anthropic_message_to_ir, encode_anthropic_message, AnthropicStreamToIr, IrToAnthropicSse,
};
use crate::bridge::protocol::chat::{encode_chat_from_ir, ChatStreamToIr, IrToChatSse};
use crate::bridge::protocol::pair::{
    sanitize_pair_response, sanitize_pair_sse_event, PairDirection,
};
use crate::bridge::protocol::responses::{
    encode_responses_from_ir, responses_output_to_ir, IrToResponsesSse, ResponsesStreamToIr,
};
use crate::bridge::types::{BridgeEvent, IrEvent, ProtocolError, Usage};
use crate::bridge::usage_capture::{
    emit, CaptureContext, GatewayUsageEvent, StreamCaptureGuard,
};

use super::http::{
    error_response, log_protocol_error, protocol_error_response, sse_data_payload,
    sse_frame_end_deque, stopping_response, stream_error_frame, EdgeState,
};
use super::surface::DownstreamSurface;
use super::transport::{UpstreamChannel, UpstreamDecode};
use super::upstream::{capture_grok_completed, capture_grok_sse, grok_upstream};
use super::{
    BODY_LIMIT_BYTES, STREAM_LIMIT_BYTES, UPSTREAM_BODY_IDLE_TIMEOUT, UPSTREAM_STREAM_IDLE_TIMEOUT,
};

fn upstream_decode(state: &EdgeState) -> UpstreamDecode {
    UpstreamChannel::from_protocol(state.upstream.protocol)
        .transport()
        .decode_kind()
}

fn partition_account<'a>(state: &'a EdgeState, member: &'a PickedMember) -> Option<&'a str> {
    state.account_picker.partition_account_id(member)
}

fn since_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Records the first forwarded non-empty payload instant (stream TTFT).
fn note_ttft(ttft_ms: &mut Option<u64>, started: Instant) {
    if ttft_ms.is_none() {
        *ttft_ms = Some(since_ms(started));
    }
}

/// Identity skeleton for gateway usage capture; outcome fields are set at the
/// completion sites below. Capture is best-effort and never fails a response.
fn usage_event(
    state: &EdgeState,
    request_id: &str,
    started: Instant,
    member: &PickedMember,
    capture: &CaptureContext,
) -> GatewayUsageEvent {
    GatewayUsageEvent::base(request_id, started, &state.profile_id)
        .with_member(member)
        .with_capture(capture)
        .with_upstream_model(state.upstream.model.as_deref())
}

/// Extract usage from a decoded non-stream upstream body, reusing the
/// protocol Usage IR decode helpers per upstream channel.
fn non_stream_upstream_usage(decode: UpstreamDecode, body: &Value) -> Option<Usage> {
    let usage = body.get("usage")?;
    match decode {
        UpstreamDecode::ChatCompletions => Usage::from_chat_usage(usage),
        UpstreamDecode::OpenAiResponses => Usage::from_responses_usage(usage),
        UpstreamDecode::AnthropicMessages => Usage::from_anthropic_usage(usage),
    }
}

pub(super) async fn messages_non_stream_response(
    state: EdgeState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    _permit: OwnedSemaphorePermit,
    replay_seed: Option<String>,
    member: PickedMember,
    capture: CaptureContext,
) -> Response {
    let status_code = response.status().as_u16();
    let upstream_body = match read_bounded_upstream_json(response, &state.force_shutdown).await {
        Ok(value) => value,
        Err(UpstreamBodyError::Stopping) => {
            emit(
                &state.usage_spool,
                usage_event(&state, &request_id, started, &member, &capture)
                    .failed(Some(status_code), "bridge_stopping"),
            );
            return stopping_response();
        }
        Err(UpstreamBodyError::InvalidOrTooLarge) => {
            state.record_upstream_failure();
            emit(
                &state.usage_spool,
                usage_event(&state, &request_id, started, &member, &capture)
                    .failed(Some(status_code), "upstream_invalid"),
            );
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider returned an invalid response.",
                None,
            );
        }
    };
    capture_grok_completed(
        &state,
        replay_seed.as_deref(),
        partition_account(&state, &member),
        &upstream_body,
    );
    let decoded = upstream_decode(&state);
    let translated = match decoded {
        UpstreamDecode::ChatCompletions => crate::bridge::protocol::chat::translate_chat_response(
            &upstream_body,
            Some(&request_id),
        )
        .and_then(|responses| responses_output_to_ir(&responses))
        .and_then(|ir| encode_anthropic_message(&ir)),
        UpstreamDecode::OpenAiResponses => {
            responses_output_to_ir(&upstream_body).and_then(|ir| encode_anthropic_message(&ir))
        }
        UpstreamDecode::AnthropicMessages => Ok(upstream_body.clone()),
    };
    match translated {
        Ok(value) => {
            state.record_upstream_success();
            tracing::info!(target: "core.adapter.protocol", profile_id = %state.profile_id, request_id = %request_id, account_id = %member.source_id, ticket_id = %member.ticket_id, op = "messages", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge response completed");
            let mut event = usage_event(&state, &request_id, started, &member, &capture)
                .ok(Some(status_code), None);
            if let Some(usage) = non_stream_upstream_usage(decoded, &upstream_body) {
                event = event.with_usage(&usage);
            }
            emit(&state.usage_spool, event);
            Json(value).into_response()
        }
        Err(error) => {
            state.record_upstream_failure();
            log_protocol_error(&state, &request_id, started, &error);
            emit(
                &state.usage_spool,
                usage_event(&state, &request_id, started, &member, &capture)
                    .failed(Some(400), error.code),
            );
            protocol_error_response(error)
        }
    }
}

pub(super) async fn chat_non_stream_response(
    state: EdgeState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    _permit: OwnedSemaphorePermit,
    replay_seed: Option<String>,
    member: PickedMember,
    capture: CaptureContext,
) -> Response {
    let status_code = response.status().as_u16();
    let upstream_body = match read_bounded_upstream_json(response, &state.force_shutdown).await {
        Ok(value) => value,
        Err(UpstreamBodyError::Stopping) => {
            emit(
                &state.usage_spool,
                usage_event(&state, &request_id, started, &member, &capture)
                    .failed(Some(status_code), "bridge_stopping"),
            );
            return stopping_response();
        }
        Err(UpstreamBodyError::InvalidOrTooLarge) => {
            state.record_upstream_failure();
            emit(
                &state.usage_spool,
                usage_event(&state, &request_id, started, &member, &capture)
                    .failed(Some(status_code), "upstream_invalid"),
            );
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider returned an invalid response.",
                None,
            );
        }
    };
    capture_grok_completed(
        &state,
        replay_seed.as_deref(),
        partition_account(&state, &member),
        &upstream_body,
    );
    let decoded = upstream_decode(&state);
    let translated = match decoded {
        UpstreamDecode::OpenAiResponses => responses_output_to_ir(&upstream_body)
            .and_then(|ir| encode_chat_from_ir(&ir, Some(&request_id))),
        UpstreamDecode::ChatCompletions => Ok(upstream_body.clone()),
        UpstreamDecode::AnthropicMessages => anthropic_message_to_ir(&upstream_body)
            .and_then(|ir| encode_chat_from_ir(&ir, Some(&request_id))),
    };
    match translated {
        Ok(value) => {
            state.record_upstream_success();
            tracing::info!(target: "core.adapter.protocol", profile_id = %state.profile_id, request_id = %request_id, account_id = %member.source_id, ticket_id = %member.ticket_id, op = "chat", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge response completed");
            let mut event = usage_event(&state, &request_id, started, &member, &capture)
                .ok(Some(status_code), None);
            if let Some(usage) = non_stream_upstream_usage(decoded, &upstream_body) {
                event = event.with_usage(&usage);
            }
            emit(&state.usage_spool, event);
            Json(value).into_response()
        }
        Err(error) => {
            state.record_upstream_failure();
            log_protocol_error(&state, &request_id, started, &error);
            emit(
                &state.usage_spool,
                usage_event(&state, &request_id, started, &member, &capture)
                    .failed(Some(400), error.code),
            );
            protocol_error_response(error)
        }
    }
}

pub(super) async fn non_stream_response(
    state: EdgeState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    _permit: OwnedSemaphorePermit,
    replay_seed: Option<String>,
    member: PickedMember,
    capture: CaptureContext,
) -> Response {
    let status_code = response.status().as_u16();
    let upstream_body = match read_bounded_upstream_json(response, &state.force_shutdown).await {
        Ok(value) => value,
        Err(UpstreamBodyError::Stopping) => {
            emit(
                &state.usage_spool,
                usage_event(&state, &request_id, started, &member, &capture)
                    .failed(Some(status_code), "bridge_stopping"),
            );
            return stopping_response();
        }
        Err(UpstreamBodyError::InvalidOrTooLarge) => {
            state.record_upstream_failure();
            emit(
                &state.usage_spool,
                usage_event(&state, &request_id, started, &member, &capture)
                    .failed(Some(status_code), "upstream_invalid"),
            );
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider returned an invalid response.",
                None,
            );
        }
    };
    capture_grok_completed(
        &state,
        replay_seed.as_deref(),
        partition_account(&state, &member),
        &upstream_body,
    );
    let decoded = upstream_decode(&state);
    let translated = match decoded {
        UpstreamDecode::ChatCompletions => crate::bridge::protocol::chat::translate_chat_response(
            &upstream_body,
            Some(&request_id),
        ),
        UpstreamDecode::AnthropicMessages => anthropic_message_to_ir(&upstream_body)
            .and_then(|ir| encode_responses_from_ir(&ir, Some(&request_id))),
        UpstreamDecode::OpenAiResponses => Ok(upstream_body.clone()),
    };
    match translated {
        Ok(value) => {
            state.record_upstream_success();
            tracing::info!(target: "core.adapter.protocol", profile_id = %state.profile_id, request_id = %request_id, account_id = %member.source_id, ticket_id = %member.ticket_id, op = "responses", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge response completed");
            let mut event = usage_event(&state, &request_id, started, &member, &capture)
                .ok(Some(status_code), None);
            if let Some(usage) = non_stream_upstream_usage(decoded, &upstream_body) {
                event = event.with_usage(&usage);
            }
            emit(&state.usage_spool, event);
            Json(value).into_response()
        }
        Err(error) => {
            state.record_upstream_failure();
            log_protocol_error(&state, &request_id, started, &error);
            emit(
                &state.usage_spool,
                usage_event(&state, &request_id, started, &member, &capture)
                    .failed(Some(400), error.code),
            );
            protocol_error_response(error)
        }
    }
}

pub(super) enum UpstreamBodyError {
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

async fn read_bounded_upstream_body(
    response: reqwest::Response,
    force_shutdown: &CancellationToken,
) -> Result<Vec<u8>, UpstreamBodyError> {
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
    Ok(body)
}

pub(super) async fn passthrough_json_response(
    state: EdgeState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    _permit: OwnedSemaphorePermit,
    replay_seed: Option<String>,
    member: PickedMember,
    pair: Option<PairDirection>,
    capture: CaptureContext,
) -> Response {
    let status = StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::OK);
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/json")
        .to_owned();
    let body = match read_bounded_upstream_body(response, &state.force_shutdown).await {
        Ok(body) => body,
        Err(UpstreamBodyError::Stopping) => {
            emit(
                &state.usage_spool,
                usage_event(&state, &request_id, started, &member, &capture)
                    .failed(Some(status.as_u16()), "bridge_stopping"),
            );
            return stopping_response();
        }
        Err(UpstreamBodyError::InvalidOrTooLarge) => {
            state.record_upstream_failure();
            emit(
                &state.usage_spool,
                usage_event(&state, &request_id, started, &member, &capture)
                    .failed(Some(status.as_u16()), "upstream_invalid"),
            );
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider returned an invalid response.",
                None,
            );
        }
    };
    let mut value = match serde_json::from_slice::<Value>(&body) {
        Ok(value) => value,
        Err(_) => {
            state.record_upstream_failure();
            emit(
                &state.usage_spool,
                usage_event(&state, &request_id, started, &member, &capture)
                    .failed(Some(status.as_u16()), "upstream_invalid"),
            );
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider returned an invalid response.",
                None,
            );
        }
    };
    capture_grok_completed(
        &state,
        replay_seed.as_deref(),
        partition_account(&state, &member),
        &value,
    );
    state
        .continuations
        .record_response(&value, replay_seed.as_deref(), &member.source_id);
    let body = if let Some(direction) = pair {
        sanitize_pair_response(direction, &mut value);
        match serde_json::to_vec(&value) {
            Ok(bytes) => bytes,
            Err(_) => {
                state.record_upstream_failure();
                emit(
                    &state.usage_spool,
                    usage_event(&state, &request_id, started, &member, &capture)
                        .failed(Some(status.as_u16()), "upstream_invalid"),
                );
                return error_response(
                    StatusCode::BAD_GATEWAY,
                    "upstream_error",
                    "The upstream model provider returned an invalid response.",
                    None,
                );
            }
        }
    } else {
        body
    };
    state.record_upstream_success();
    tracing::info!(target: "core.adapter.protocol", profile_id = %state.profile_id, request_id = %request_id, account_id = %member.source_id, ticket_id = %member.ticket_id, op = "passthrough", status = status.as_u16(), elapsed_ms = started.elapsed().as_millis() as u64, "bridge response completed");
    // Bytes are relayed without protocol decoding: capture records identity,
    // status, and latency only; token fields stay zero/NULL.
    emit(
        &state.usage_spool,
        usage_event(&state, &request_id, started, &member, &capture)
            .ok(Some(status.as_u16()), None),
    );
    let content_type = HeaderValue::from_str(&content_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/json"));
    (
        status,
        [(header::CONTENT_TYPE, content_type)],
        Body::from(body),
    )
        .into_response()
}

enum StreamCodec {
    Kimi(crate::bridge::protocol::chat::ResponsesSseTranslator),
    Anthropic {
        ir: AnthropicStreamToIr,
        out: IrToResponsesSse,
    },
}

impl StreamCodec {
    fn new(kind: UpstreamDecode, request_id: String, model: String) -> Self {
        match kind {
            UpstreamDecode::ChatCompletions => Self::Kimi(
                crate::bridge::protocol::chat::ResponsesSseTranslator::new(request_id, model),
            ),
            UpstreamDecode::AnthropicMessages => Self::Anthropic {
                ir: AnthropicStreamToIr::new(),
                out: IrToResponsesSse::new(request_id, model),
            },
            UpstreamDecode::OpenAiResponses => {
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

    /// Retained upstream usage (gateway usage capture); `None` until the
    /// upstream sent a usage object. Retention only — output frames stay
    /// byte-identical.
    fn captured_usage(&self) -> Option<Usage> {
        match self {
            Self::Kimi(translator) => translator.captured_usage(),
            Self::Anthropic { ir, .. } => ir.captured_usage(),
        }
    }

    fn treats_done_marker(&self) -> bool {
        matches!(self, Self::Kimi(_))
    }
}

fn warn_stream_fail(request_id: &str, code: &str) {
    tracing::warn!(
        target: "core.adapter.protocol",
        request_id = %request_id,
        op = "stream",
        code,
        "bridge stream translator or SSE failed"
    );
}

fn stream_fail_frame(surface: DownstreamSurface, sequence_number: u64) -> axum::body::Bytes {
    match surface {
        DownstreamSurface::ChatCompletions => axum::body::Bytes::from_static(
            b"data: {\"error\":{\"message\":\"The upstream model provider returned an invalid stream.\",\"type\":\"server_error\",\"code\":\"upstream_error\"}}\n\ndata: [DONE]\n\n",
        ),
        DownstreamSurface::Responses => {
            let data = serde_json::json!({
                "type": "error",
                "code": "upstream_error",
                "message": "The upstream model provider returned an invalid stream.",
                "param": Value::Null,
                "sequence_number": sequence_number,
            });
            let payload = serde_json::to_string(&data)
                .expect("Responses stream error frame must be serializable");
            axum::body::Bytes::from(format!("event: error\ndata: {payload}\n\n"))
        }
        DownstreamSurface::Messages | DownstreamSurface::Models => stream_error_frame(),
    }
}

fn advance_sequence_number(next_sequence_number: &mut u64, sequence_number: u64) {
    *next_sequence_number = (*next_sequence_number).max(sequence_number.saturating_add(1));
}

struct ResponsesSseFrameInfo {
    has_data: bool,
    error_like: bool,
    sequence_number: Option<u64>,
    terminal: bool,
}

fn parse_responses_sse_frame(frame: &[u8]) -> Result<ResponsesSseFrameInfo, ()> {
    let text = std::str::from_utf8(frame).map_err(|_| ())?;
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let event_names = normalized
        .lines()
        .filter_map(|line| {
            let (field, value) = line.split_once(':').unwrap_or((line, ""));
            (field == "event").then_some(value.strip_prefix(' ').unwrap_or(value))
        })
        .collect::<Vec<_>>();
    let payload = sse_data_payload(frame)?;
    let Some(payload) = payload else {
        return Ok(ResponsesSseFrameInfo {
            has_data: false,
            error_like: false,
            sequence_number: None,
            terminal: false,
        });
    };
    if payload.is_empty() {
        return Ok(ResponsesSseFrameInfo {
            has_data: false,
            error_like: false,
            sequence_number: None,
            terminal: false,
        });
    }
    if payload == "[DONE]" {
        return Err(());
    }
    if event_names.len() != 1 || event_names[0].is_empty() {
        return Err(());
    }

    let value = serde_json::from_str::<Value>(&payload).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    let data_type = object.get("type").and_then(Value::as_str).ok_or(())?;
    if event_names[0] != data_type || !is_known_responses_event_type(data_type) {
        return Err(());
    }
    let sequence_number = match object.get("sequence_number") {
        Some(sequence_number) => Some(sequence_number.as_u64().ok_or(())?),
        None => return Err(()),
    };
    let terminal = matches!(
        data_type,
        "response.completed" | "response.failed" | "response.incomplete" | "error"
    );

    Ok(ResponsesSseFrameInfo {
        has_data: true,
        error_like: matches!(data_type, "response.failed" | "error"),
        sequence_number,
        terminal,
    })
}

fn is_known_responses_event_type(kind: &str) -> bool {
    matches!(
        kind,
        "error"
            | "response.audio.delta"
            | "response.audio.done"
            | "response.audio.transcript.delta"
            | "response.audio.transcript.done"
            | "response.code_interpreter_call_code.delta"
            | "response.code_interpreter_call_code.done"
            | "response.code_interpreter_call.completed"
            | "response.code_interpreter_call.in_progress"
            | "response.code_interpreter_call.interpreting"
            | "response.completed"
            | "response.content_part.added"
            | "response.content_part.done"
            | "response.created"
            | "response.custom_tool_call_input.delta"
            | "response.custom_tool_call_input.done"
            | "response.failed"
            | "response.file_search_call.completed"
            | "response.file_search_call.in_progress"
            | "response.file_search_call.searching"
            | "response.function_call_arguments.delta"
            | "response.function_call_arguments.done"
            | "response.image_generation_call.completed"
            | "response.image_generation_call.generating"
            | "response.image_generation_call.in_progress"
            | "response.image_generation_call.partial_image"
            | "response.in_progress"
            | "response.incomplete"
            | "response.mcp_call_arguments.delta"
            | "response.mcp_call_arguments.done"
            | "response.mcp_call.completed"
            | "response.mcp_call.failed"
            | "response.mcp_call.in_progress"
            | "response.mcp_list_tools.completed"
            | "response.mcp_list_tools.failed"
            | "response.mcp_list_tools.in_progress"
            | "response.output_item.added"
            | "response.output_item.done"
            | "response.output_text.annotation.added"
            | "response.output_text.delta"
            | "response.output_text.done"
            | "response.queued"
            | "response.reasoning_summary_part.added"
            | "response.reasoning_summary_part.done"
            | "response.reasoning_summary_text.delta"
            | "response.reasoning_summary_text.done"
            | "response.reasoning_text.delta"
            | "response.reasoning_text.done"
            | "response.refusal.delta"
            | "response.refusal.done"
            | "response.shell_call.command.added"
            | "response.shell_call.command.delta"
            | "response.shell_call.command.done"
            | "response.shell_call.output_content.delta"
            | "response.shell_call.output_content.done"
            | "response.text.delta"
            | "response.text.done"
            | "response.tool_search_call.completed"
            | "response.tool_search_call.failed"
            | "response.tool_search_call.in_progress"
            | "response.web_search_call.completed"
            | "response.web_search_call.in_progress"
            | "response.web_search_call.searching"
    )
}

fn rewrite_pair_sse_frame(frame: Vec<u8>, pair: Option<PairDirection>) -> Result<Vec<u8>, ()> {
    let Some(direction) = pair else {
        return Ok(frame);
    };
    let Some(payload) = sse_data_payload(&frame)? else {
        return Ok(frame);
    };
    if payload.is_empty() {
        return Ok(frame);
    }
    let mut value = serde_json::from_str::<Value>(&payload).map_err(|_| ())?;
    sanitize_pair_sse_event(direction, &mut value);
    let event_type = value.get("type").and_then(Value::as_str).unwrap_or("error");
    let data = serde_json::to_string(&value).map_err(|_| ())?;
    Ok(format!("event: {event_type}\ndata: {data}\n\n").into_bytes())
}

fn event_stream_response(
    output: impl futures_util::Stream<Item = Result<axum::body::Bytes, Infallible>> + Send + 'static,
) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    (StatusCode::OK, headers, Body::from_stream(output)).into_response()
}

pub(super) fn passthrough_sse_response(
    state: EdgeState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
    replay_seed: Option<String>,
    member: PickedMember,
    surface: DownstreamSurface,
    pair: Option<PairDirection>,
    capture: CaptureContext,
) -> Response {
    let profile_id = state.profile_id.clone();
    let account_id = member.source_id.clone();
    let ticket_id = member.ticket_id.clone();
    let member_id = member.source_id.clone();
    let partition = partition_account(&state, &member).map(str::to_owned);
    let force_shutdown = state.force_shutdown.clone();
    let observed = state.clone();
    let upstream_status = response.status().as_u16();
    let bytes = response.bytes_stream();
    let op = match surface {
        DownstreamSurface::ChatCompletions => "chat_passthrough_stream",
        DownstreamSurface::Messages => "messages_passthrough_stream",
        _ => "responses_passthrough_stream",
    };
    let output = stream! {
        let _permit = permit;
        // Bytes are relayed without protocol decoding: capture records
        // identity, status, and latency only; token fields stay zero/NULL.
        let capture_guard = StreamCaptureGuard::new(
            &observed.usage_spool,
            usage_event(&observed, &request_id, started, &member, &capture),
            started,
            Some(upstream_status),
        );
        let mut ttft_ms: Option<u64> = None;
        let mut upstream_bytes = 0usize;
        let should_capture = grok_upstream(&observed);
        let mut capture = Vec::new();
        let mut responses_buffer = std::collections::VecDeque::new();
        let mut next_sequence_number = 0_u64;
        let mut last_responses_sequence_number = None;
        let mut saw_responses_terminal = false;
        let is_responses = surface == DownstreamSurface::Responses;
        futures_util::pin_mut!(bytes);
        'responses_upstream: loop {
            let next = tokio::select! {
                _ = force_shutdown.cancelled() => {
                    if is_responses {
                        observed.record_upstream_failure();
                    }
                    warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(surface, next_sequence_number));
                    return;
                }
                next = tokio::time::timeout(UPSTREAM_STREAM_IDLE_TIMEOUT, bytes.next()) => match next {
                    Ok(next) => next,
                    Err(_) => {
                        observed.record_upstream_failure();
                        warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(surface, next_sequence_number));
                        return;
                    }
                },
            };
            let Some(chunk) = next else { break; };
            let Ok(chunk) = chunk else {
                observed.record_upstream_failure();
                warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(surface, next_sequence_number));
                return;
            };
            if upstream_bytes.saturating_add(chunk.len()) > STREAM_LIMIT_BYTES {
                observed.record_upstream_failure();
                warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(surface, next_sequence_number));
                return;
            }
            upstream_bytes += chunk.len();
            if should_capture && capture.len().saturating_add(chunk.len()) <= STREAM_LIMIT_BYTES {
                capture.extend_from_slice(&chunk);
            }
            if !is_responses {
                note_ttft(&mut ttft_ms, started);
                yield Ok::<_, Infallible>(chunk);
                continue;
            }

            responses_buffer.extend(chunk.iter().copied());
            while let Some((frame_end, delimiter_len)) = sse_frame_end_deque(&responses_buffer) {
                let frame_len = frame_end + delimiter_len;
                let frame = responses_buffer.drain(..frame_len).collect::<Vec<_>>();
                let info = match parse_responses_sse_frame(&frame) {
                    Ok(info) => info,
                    Err(()) => {
                        observed.record_upstream_failure();
                        warn_stream_fail(&request_id, "stream_error");
                        yield Ok::<_, Infallible>(stream_fail_frame(
                            DownstreamSurface::Responses,
                            next_sequence_number,
                        ));
                        return;
                    }
                };
                if saw_responses_terminal && info.has_data {
                    observed.record_upstream_failure();
                    warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(
                        DownstreamSurface::Responses,
                        next_sequence_number,
                    ));
                    return;
                }
                if let Some(sequence_number) = info.sequence_number {
                    if last_responses_sequence_number
                        .is_some_and(|last| sequence_number <= last)
                    {
                        observed.record_upstream_failure();
                        warn_stream_fail(&request_id, "stream_error");
                        yield Ok::<_, Infallible>(stream_fail_frame(
                            DownstreamSurface::Responses,
                            next_sequence_number,
                        ));
                        return;
                    }
                    last_responses_sequence_number = Some(sequence_number);
                    advance_sequence_number(&mut next_sequence_number, sequence_number);
                }
                if info.error_like {
                    observed.record_upstream_failure();
                    warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(
                        DownstreamSurface::Responses,
                        info.sequence_number.expect("validated Responses sequence number"),
                    ));
                    return;
                }
                if let Ok(Some(payload)) = sse_data_payload(&frame) {
                    if let Ok(value) = serde_json::from_str::<Value>(&payload) {
                        observed.continuations.record_response(
                            &value,
                            replay_seed.as_deref(),
                            &member_id,
                        );
                    }
                }
                let terminal = info.terminal;
                saw_responses_terminal |= terminal;
                let trailing_bytes = responses_buffer.len();
                let outbound = match rewrite_pair_sse_frame(frame, pair) {
                    Ok(frame) => frame,
                    Err(()) => {
                        observed.record_upstream_failure();
                        warn_stream_fail(&request_id, "stream_error");
                        yield Ok::<_, Infallible>(stream_fail_frame(
                            DownstreamSurface::Responses,
                            next_sequence_number,
                        ));
                        return;
                    }
                };
                yield Ok::<_, Infallible>(axum::body::Bytes::from(outbound));
                note_ttft(&mut ttft_ms, started);
                if terminal {
                    responses_buffer.clear();
                    if trailing_bytes <= capture.len() {
                        capture.truncate(capture.len() - trailing_bytes);
                    }
                    break 'responses_upstream;
                }
            }
        }
        if is_responses && (!responses_buffer.is_empty() || !saw_responses_terminal) {
            observed.record_upstream_failure();
            warn_stream_fail(&request_id, "stream_error");
            yield Ok::<_, Infallible>(stream_fail_frame(
                DownstreamSurface::Responses,
                next_sequence_number,
            ));
            return;
        }
        if should_capture {
            if let Ok(sse) = std::str::from_utf8(&capture) {
                capture_grok_sse(&observed, replay_seed.as_deref(), partition.as_deref(), sse);
            }
        }
        observed.record_upstream_success();
        tracing::info!(target: "core.adapter.protocol", profile_id = %profile_id, request_id = %request_id, account_id = %account_id, ticket_id = %ticket_id, op, status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge stream completed");
        capture_guard.succeed(ttft_ms, None);
    };
    event_stream_response(output)
}

pub(super) fn stream_response(
    state: EdgeState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
    member: PickedMember,
    capture: CaptureContext,
) -> Response {
    let model = state.upstream.model.clone().unwrap_or_default();
    let profile_id = state.profile_id.clone();
    let account_id = member.source_id.clone();
    let ticket_id = member.ticket_id.clone();
    let force_shutdown = state.force_shutdown.clone();
    let observed = state.clone();
    let decode_kind = upstream_decode(&state);
    let upstream_status = response.status().as_u16();
    let bytes = response.bytes_stream();
    let output = stream! {
        let mut translator = StreamCodec::new(decode_kind, request_id.clone(), model);
        // Gateway usage capture: failure outcomes are recorded by the guard's
        // Drop (including a generator dropped on client disconnect); the
        // success tail disarms it explicitly.
        let capture_guard = StreamCaptureGuard::new(
            &observed.usage_spool,
            usage_event(&observed, &request_id, started, &member, &capture),
            started,
            Some(upstream_status),
        );
        let mut ttft_ms: Option<u64> = None;
        // `VecDeque` lets us consume complete SSE frames from the front without repeatedly
        // moving the unread tail. The cap counts all upstream bytes, not merely the current
        // partial frame, and the output cap protects a pathological translator expansion.
        let mut buffer = std::collections::VecDeque::new();
        let mut upstream_bytes = 0usize;
        let mut output_bytes = 0usize;
        let _permit = permit;
        let mut saw_done = false;
        let mut next_sequence_number = 0_u64;
        futures_util::pin_mut!(bytes);
        'upstream: loop {
            let next = tokio::select! {
                _ = force_shutdown.cancelled() => {
                    warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(DownstreamSurface::Responses, next_sequence_number));
                    return;
                }
                next = tokio::time::timeout(UPSTREAM_STREAM_IDLE_TIMEOUT, bytes.next()) => match next {
                    Ok(next) => next,
                    Err(_) => {
                        observed.record_upstream_failure();
                        warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(DownstreamSurface::Responses, next_sequence_number));
                        return;
                    }
                },
            };
            let Some(chunk) = next else { break; };
            let Ok(chunk) = chunk else {
                observed.record_upstream_failure();
                warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(DownstreamSurface::Responses, next_sequence_number));
                return;
            };
            if upstream_bytes.saturating_add(chunk.len()) > STREAM_LIMIT_BYTES {
                observed.record_upstream_failure();
                warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(DownstreamSurface::Responses, next_sequence_number));
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
                        warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(DownstreamSurface::Responses, next_sequence_number));
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
                    warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(DownstreamSurface::Responses, next_sequence_number));
                    return;
                };
                match translator.push(&value) {
                    Ok(events) => for event in events {
                        let frame = crate::bridge::protocol::chat::sse_frame(&event);
                        if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                            observed.record_upstream_failure();
                            warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(DownstreamSurface::Responses, next_sequence_number));
                            return;
                        }
                        output_bytes += frame.len();
                        advance_sequence_number(&mut next_sequence_number, event.sequence_number());
                        note_ttft(&mut ttft_ms, started);
                        yield Ok::<_, Infallible>(axum::body::Bytes::from(frame));
                    },
                    Err(_) => {
                        observed.record_upstream_failure();
                        warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(DownstreamSurface::Responses, next_sequence_number));
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
            warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(DownstreamSurface::Responses, next_sequence_number));
            return;
        }
        match translator.finish() {
            Ok(events) => {
                for event in events {
                    let frame = crate::bridge::protocol::chat::sse_frame(&event);
                    if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                        observed.record_upstream_failure();
                        warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(DownstreamSurface::Responses, next_sequence_number));
                        return;
                    }
                    output_bytes += frame.len();
                    advance_sequence_number(&mut next_sequence_number, event.sequence_number());
                    note_ttft(&mut ttft_ms, started);
                    yield Ok::<_, Infallible>(axum::body::Bytes::from(frame));
                }
            }
            Err(_) => {
                observed.record_upstream_failure();
                warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(stream_fail_frame(DownstreamSurface::Responses, next_sequence_number));
                return;
            }
        }
        observed.record_upstream_success();
        tracing::info!(target: "core.adapter.protocol", profile_id = %profile_id, request_id = %request_id, account_id = %account_id, ticket_id = %ticket_id, op = "stream", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge stream completed");
        capture_guard.succeed(ttft_ms, translator.captured_usage().as_ref());
    };
    event_stream_response(output)
}

enum MessagesStreamCodec {
    Chat(ChatStreamToIr),
    Anthropic(AnthropicStreamToIr),
    Responses(ResponsesStreamToIr),
}

impl MessagesStreamCodec {
    fn new(kind: UpstreamDecode, request_id: String, model: String) -> Self {
        match kind {
            UpstreamDecode::ChatCompletions => Self::Chat(ChatStreamToIr::new(request_id, model)),
            UpstreamDecode::AnthropicMessages => Self::Anthropic(AnthropicStreamToIr::new()),
            UpstreamDecode::OpenAiResponses => Self::Responses(ResponsesStreamToIr::new()),
        }
    }

    fn push(&mut self, value: &Value) -> Result<Vec<IrEvent>, ProtocolError> {
        match self {
            Self::Chat(translator) => translator.push_event(value),
            Self::Anthropic(translator) => translator.push_event(value),
            Self::Responses(translator) => translator.push_event(value),
        }
    }

    fn finish(&mut self) -> Vec<IrEvent> {
        match self {
            Self::Chat(translator) => translator.finish(),
            Self::Anthropic(translator) => translator.finish(),
            Self::Responses(translator) => translator.finish(),
        }
    }

    /// Retained upstream usage (gateway usage capture); `None` until the
    /// upstream sent a usage object. Retention only — output frames stay
    /// byte-identical.
    fn captured_usage(&self) -> Option<Usage> {
        match self {
            Self::Chat(translator) => translator.captured_usage(),
            Self::Anthropic(translator) => translator.captured_usage(),
            Self::Responses(translator) => translator.captured_usage(),
        }
    }
}

pub(super) fn messages_stream_response(
    state: EdgeState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
    replay_seed: Option<String>,
    member: PickedMember,
    capture: CaptureContext,
) -> Response {
    let profile_id = state.profile_id.clone();
    let account_id = member.source_id.clone();
    let ticket_id = member.ticket_id.clone();
    let partition = partition_account(&state, &member).map(str::to_owned);
    let force_shutdown = state.force_shutdown.clone();
    let observed = state.clone();
    let upstream_status = response.status().as_u16();
    let bytes = response.bytes_stream();
    let error_frame = stream_fail_frame(DownstreamSurface::Messages, 0);
    let output = stream! {
        let model = state.upstream.model.clone().unwrap_or_default();
        let decode_kind = upstream_decode(&state);
        let mut translator = MessagesStreamCodec::new(decode_kind, request_id.clone(), model);
        // Gateway usage capture: failure outcomes are recorded by the guard's
        // Drop (including a generator dropped on client disconnect); the
        // success tail disarms it explicitly.
        let capture_guard = StreamCaptureGuard::new(
            &observed.usage_spool,
            usage_event(&observed, &request_id, started, &member, &capture),
            started,
            Some(upstream_status),
        );
        let mut ttft_ms: Option<u64> = None;
        let mut encoder = IrToAnthropicSse::new();
        let mut buffer = std::collections::VecDeque::new();
        let mut upstream_bytes = 0usize;
        let mut output_bytes = 0usize;
        let should_capture = grok_upstream(&observed);
        let mut capture = Vec::new();
        let _permit = permit;
        let mut saw_done = false;
        futures_util::pin_mut!(bytes);
        'upstream: loop {
            let next = tokio::select! {
                _ = force_shutdown.cancelled() => {
                    warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                    return;
                }
                next = tokio::time::timeout(UPSTREAM_STREAM_IDLE_TIMEOUT, bytes.next()) => match next {
                    Ok(next) => next,
                    Err(_) => {
                        observed.record_upstream_failure();
                        warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                        return;
                    }
                },
            };
            let Some(chunk) = next else { break; };
            let Ok(chunk) = chunk else {
                observed.record_upstream_failure();
                warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                return;
            };
            if upstream_bytes.saturating_add(chunk.len()) > STREAM_LIMIT_BYTES {
                observed.record_upstream_failure();
                warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                return;
            }
            upstream_bytes += chunk.len();
            if should_capture && capture.len().saturating_add(chunk.len()) <= STREAM_LIMIT_BYTES {
                capture.extend_from_slice(&chunk);
            }
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
                        warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                        return;
                    }
                };
                let Some(payload) = payload else { continue; };
                if payload.is_empty() { continue; }
                if payload == "[DONE]" {
                    if decode_kind == UpstreamDecode::ChatCompletions {
                        saw_done = true;
                        break 'upstream;
                    }
                    continue;
                }
                let Ok(value) = serde_json::from_str::<Value>(&payload) else {
                    observed.record_upstream_failure();
                    warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                    return;
                };
                let events = match translator.push(&value) {
                    Ok(events) => events,
                    Err(_) => {
                        observed.record_upstream_failure();
                        warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                        return;
                    }
                };
                let completed = events
                    .iter()
                    .any(|event| matches!(event, IrEvent::MessageEnd { .. }));
                for event in &events {
                    let frames = match encoder.push_event(event) {
                        Ok(frames) => frames,
                        Err(_) => {
                            observed.record_upstream_failure();
                            warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                            return;
                        }
                    };
                    for frame in frames {
                        if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                            observed.record_upstream_failure();
                            warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                            return;
                        }
                        output_bytes += frame.len();
                        note_ttft(&mut ttft_ms, started);
                        yield Ok::<_, Infallible>(axum::body::Bytes::from(frame));
                    }
                }
                if completed {
                    saw_done = true;
                    break 'upstream;
                }
            }
        }
        if !saw_done || !buffer.is_empty() {
            observed.record_upstream_failure();
            warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
            return;
        }
        for event in translator.finish() {
            let frames = match encoder.push_event(&event) {
                Ok(frames) => frames,
                Err(_) => {
                    observed.record_upstream_failure();
                    warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                    return;
                }
            };
            for frame in frames {
                if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                    observed.record_upstream_failure();
                    warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                    return;
                }
                output_bytes += frame.len();
                note_ttft(&mut ttft_ms, started);
                yield Ok::<_, Infallible>(axum::body::Bytes::from(frame));
            }
        }
        if should_capture {
            if let Ok(sse) = std::str::from_utf8(&capture) {
                capture_grok_sse(&observed, replay_seed.as_deref(), partition.as_deref(), sse);
            }
        }
        observed.record_upstream_success();
        tracing::info!(target: "core.adapter.protocol", profile_id = %profile_id, request_id = %request_id, account_id = %account_id, ticket_id = %ticket_id, op = "messages_stream", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge stream completed");
        capture_guard.succeed(ttft_ms, translator.captured_usage().as_ref());
    };
    event_stream_response(output)
}

pub(super) fn chat_stream_response(
    state: EdgeState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
    replay_seed: Option<String>,
    member: PickedMember,
    capture: CaptureContext,
) -> Response {
    let profile_id = state.profile_id.clone();
    let account_id = member.source_id.clone();
    let ticket_id = member.ticket_id.clone();
    let partition = partition_account(&state, &member).map(str::to_owned);
    let force_shutdown = state.force_shutdown.clone();
    let observed = state.clone();
    let upstream_status = response.status().as_u16();
    let bytes = response.bytes_stream();
    let error_frame = stream_fail_frame(DownstreamSurface::ChatCompletions, 0);
    let output = stream! {
        let model = state.upstream.model.clone().unwrap_or_default();
        let decode_kind = upstream_decode(&state);
        let mut translator = MessagesStreamCodec::new(decode_kind, request_id.clone(), model);
        // Gateway usage capture: failure outcomes are recorded by the guard's
        // Drop (including a generator dropped on client disconnect); the
        // success tail disarms it explicitly.
        let capture_guard = StreamCaptureGuard::new(
            &observed.usage_spool,
            usage_event(&observed, &request_id, started, &member, &capture),
            started,
            Some(upstream_status),
        );
        let mut ttft_ms: Option<u64> = None;
        let mut encoder = IrToChatSse::new(Some(&request_id));
        let mut buffer = std::collections::VecDeque::new();
        let mut upstream_bytes = 0usize;
        let mut output_bytes = 0usize;
        let should_capture = grok_upstream(&observed);
        let mut capture = Vec::new();
        let _permit = permit;
        let mut saw_done = false;
        futures_util::pin_mut!(bytes);
        'upstream: loop {
            let next = tokio::select! {
                _ = force_shutdown.cancelled() => {
                    warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                    return;
                }
                next = tokio::time::timeout(UPSTREAM_STREAM_IDLE_TIMEOUT, bytes.next()) => match next {
                    Ok(next) => next,
                    Err(_) => {
                        observed.record_upstream_failure();
                        warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                        return;
                    }
                },
            };
            let Some(chunk) = next else { break; };
            let Ok(chunk) = chunk else {
                observed.record_upstream_failure();
                warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                return;
            };
            if upstream_bytes.saturating_add(chunk.len()) > STREAM_LIMIT_BYTES {
                observed.record_upstream_failure();
                warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                return;
            }
            upstream_bytes += chunk.len();
            if should_capture && capture.len().saturating_add(chunk.len()) <= STREAM_LIMIT_BYTES {
                capture.extend_from_slice(&chunk);
            }
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
                        warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
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
                    warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                    return;
                };
                let events = match translator.push(&value) {
                    Ok(events) => events,
                    Err(_) => {
                        observed.record_upstream_failure();
                        warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                        return;
                    }
                };
                let completed = events
                    .iter()
                    .any(|event| matches!(event, IrEvent::MessageEnd { .. }));
                for event in &events {
                    let frames = match encoder.push_event(event) {
                        Ok(frames) => frames,
                        Err(_) => {
                            observed.record_upstream_failure();
                            warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                            return;
                        }
                    };
                    for frame in frames {
                        if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                            observed.record_upstream_failure();
                            warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                            return;
                        }
                        output_bytes += frame.len();
                        note_ttft(&mut ttft_ms, started);
                        yield Ok::<_, Infallible>(axum::body::Bytes::from(frame));
                    }
                }
                if completed {
                    saw_done = true;
                    break 'upstream;
                }
            }
        }
        if !saw_done || !buffer.is_empty() {
            observed.record_upstream_failure();
            warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
            return;
        }
        for event in translator.finish() {
            let frames = match encoder.push_event(&event) {
                Ok(frames) => frames,
                Err(_) => {
                    observed.record_upstream_failure();
                    warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                    return;
                }
            };
            for frame in frames {
                if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                    observed.record_upstream_failure();
                    warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                    return;
                }
                output_bytes += frame.len();
                note_ttft(&mut ttft_ms, started);
                yield Ok::<_, Infallible>(axum::body::Bytes::from(frame));
            }
        }
        let frames = match encoder.finish() {
            Ok(frames) => frames,
            Err(_) => {
                observed.record_upstream_failure();
                warn_stream_fail(&request_id, "stream_error");
                yield Ok::<_, Infallible>(error_frame.clone());
                return;
            }
        };
        for frame in frames {
            if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                observed.record_upstream_failure();
                warn_stream_fail(&request_id, "stream_error");
                    yield Ok::<_, Infallible>(error_frame.clone());
                return;
            }
            output_bytes += frame.len();
            note_ttft(&mut ttft_ms, started);
            yield Ok::<_, Infallible>(axum::body::Bytes::from(frame));
        }
        if should_capture {
            if let Ok(sse) = std::str::from_utf8(&capture) {
                capture_grok_sse(&observed, replay_seed.as_deref(), partition.as_deref(), sse);
            }
        }
        observed.record_upstream_success();
        tracing::info!(target: "core.adapter.protocol", profile_id = %profile_id, request_id = %request_id, account_id = %account_id, ticket_id = %ticket_id, op = "chat_stream", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge stream completed");
        capture_guard.succeed(ttft_ms, translator.captured_usage().as_ref());
    };
    event_stream_response(output)
}
