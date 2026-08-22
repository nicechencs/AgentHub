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

use crate::bridge::protocol::anthropic_messages::{
    anthropic_message_to_ir, encode_anthropic_message, encode_anthropic_sse, AnthropicStreamToIr,
};
use crate::bridge::protocol::chat::{encode_chat_from_ir, encode_chat_sse, ChatStreamToIr};
use crate::bridge::protocol::responses::{
    encode_responses_from_ir, responses_output_to_ir, IrToResponsesSse, ResponsesStreamToIr,
};
use crate::bridge::types::{BridgeEvent, IrEvent, ProtocolError};

use super::http::{
    error_response, log_protocol_error, protocol_error_response, sse_data_payload,
    sse_frame_end_deque, stopping_response, stream_error_frame, EdgeState,
};
use super::transport::{UpstreamChannel, UpstreamDecode};
use super::upstream::{capture_grok_completed, capture_grok_sse};
use super::{
    BODY_LIMIT_BYTES, STREAM_LIMIT_BYTES, UPSTREAM_BODY_IDLE_TIMEOUT, UPSTREAM_STREAM_IDLE_TIMEOUT,
};

fn upstream_decode(state: &EdgeState) -> UpstreamDecode {
    UpstreamChannel::from_protocol(state.upstream.protocol).decode_kind()
}

pub(super) async fn messages_non_stream_response(
    state: EdgeState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    _permit: OwnedSemaphorePermit,
    replay_seed: Option<String>,
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
    capture_grok_completed(&state, replay_seed.as_deref(), &upstream_body);
    let translated = match upstream_decode(&state) {
        UpstreamDecode::ChatCompletions => crate::bridge::protocol::chat::translate_chat_response(
            &upstream_body,
            Some(&request_id),
        )
        .and_then(|responses| responses_output_to_ir(&responses))
        .and_then(|ir| encode_anthropic_message(&ir)),
        UpstreamDecode::OpenAiResponses => {
            responses_output_to_ir(&upstream_body).and_then(|ir| encode_anthropic_message(&ir))
        }
        UpstreamDecode::AnthropicMessages => {
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
    state: EdgeState,
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
    let translated = match upstream_decode(&state) {
        UpstreamDecode::OpenAiResponses => responses_output_to_ir(&upstream_body)
            .and_then(|ir| encode_chat_from_ir(&ir, Some(&request_id))),
        UpstreamDecode::ChatCompletions | UpstreamDecode::AnthropicMessages => {
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
    state: EdgeState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    _permit: OwnedSemaphorePermit,
    replay_seed: Option<String>,
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
    capture_grok_completed(&state, replay_seed.as_deref(), &upstream_body);
    let translated = match upstream_decode(&state) {
        UpstreamDecode::ChatCompletions => crate::bridge::protocol::chat::translate_chat_response(
            &upstream_body,
            Some(&request_id),
        ),
        UpstreamDecode::AnthropicMessages => anthropic_message_to_ir(&upstream_body)
            .and_then(|ir| encode_responses_from_ir(&ir, Some(&request_id))),
        UpstreamDecode::OpenAiResponses => Ok(upstream_body),
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

    fn treats_done_marker(&self) -> bool {
        matches!(self, Self::Kimi(_))
    }
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
) -> Response {
    let profile_id = state.profile_id.clone();
    let force_shutdown = state.force_shutdown.clone();
    let observed = state.clone();
    let bytes = response.bytes_stream();
    let output = stream! {
        let _permit = permit;
        let mut upstream_bytes = 0usize;
        let mut capture = Vec::new();
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
            if capture.len().saturating_add(chunk.len()) <= STREAM_LIMIT_BYTES {
                capture.extend_from_slice(&chunk);
            }
            yield Ok::<_, Infallible>(chunk);
        }
        if let Ok(sse) = std::str::from_utf8(&capture) {
            capture_grok_sse(&observed, replay_seed.as_deref(), sse);
        }
        observed.record_upstream_success();
        tracing::info!(target: "core.adapter.protocol", profile_id = %profile_id, request_id = %request_id, op = "responses_passthrough_stream", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge stream completed");
    };
    event_stream_response(output)
}

pub(super) fn stream_response(
    state: EdgeState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
) -> Response {
    let model = state.upstream.model.clone().unwrap_or_default();
    let profile_id = state.profile_id.clone();
    let force_shutdown = state.force_shutdown.clone();
    let observed = state.clone();
    let decode_kind = upstream_decode(&state);
    let bytes = response.bytes_stream();
    let output = stream! {
        let mut translator = StreamCodec::new(decode_kind, request_id.clone(), model);
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
    event_stream_response(output)
}

enum MessagesStreamCodec {
    Chat(ChatStreamToIr),
    Responses(ResponsesStreamToIr),
}

impl MessagesStreamCodec {
    fn new(kind: UpstreamDecode, request_id: String, model: String) -> Self {
        match kind {
            UpstreamDecode::ChatCompletions => Self::Chat(ChatStreamToIr::new(request_id, model)),
            UpstreamDecode::OpenAiResponses => Self::Responses(ResponsesStreamToIr::new()),
            UpstreamDecode::AnthropicMessages => {
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
    state: EdgeState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
    replay_seed: Option<String>,
) -> Response {
    let profile_id = state.profile_id.clone();
    let force_shutdown = state.force_shutdown.clone();
    let observed = state.clone();
    let bytes = response.bytes_stream();
    let output = stream! {
        let model = state.upstream.model.clone().unwrap_or_default();
        let decode_kind = upstream_decode(&state);
        let mut translator = MessagesStreamCodec::new(decode_kind, request_id.clone(), model);
        let mut ir_events: Vec<IrEvent> = Vec::new();
        let mut emitted_frames = 0usize;
        let mut buffer = std::collections::VecDeque::new();
        let mut upstream_bytes = 0usize;
        let mut output_bytes = 0usize;
        let mut capture = Vec::new();
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
            if capture.len().saturating_add(chunk.len()) <= STREAM_LIMIT_BYTES {
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
                        yield Ok::<_, Infallible>(stream_error_frame());
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
        if let Ok(sse) = std::str::from_utf8(&capture) {
            capture_grok_sse(&observed, replay_seed.as_deref(), sse);
        }
        observed.record_upstream_success();
        tracing::info!(target: "core.adapter.protocol", profile_id = %profile_id, request_id = %request_id, op = "messages_stream", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge stream completed");
    };
    event_stream_response(output)
}

pub(super) fn chat_stream_response(
    state: EdgeState,
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
        let decode_kind = upstream_decode(&state);
        let mut translator = MessagesStreamCodec::new(decode_kind, request_id.clone(), model);
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
    event_stream_response(output)
}
