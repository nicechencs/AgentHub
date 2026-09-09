//! In-process Kiro upstream: downstream surface → GenerateAssistantResponse.
//!
//! Does not POST OpenAI/Anthropic; uses the AgentHub-owned Kiro HTTP client.

use axum::body::{Body, Bytes};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::stream;
use serde_json::{json, Value};
use std::convert::Infallible;
use std::thread;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tokio::task::spawn_blocking;

use super::admission::AdmittedRequest;
use super::http::error_response;
use super::surface::DownstreamSurface;
use crate::adapters::kiro::http::{
    chat_turn_with_access_token, stream_chat_turn_with_access_token,
};
use crate::bridge::protocol::anthropic_messages::IrToAnthropicSse;
use crate::bridge::protocol::chat::IrToChatSse;
use crate::bridge::protocol::responses::IrToResponsesSse;
use crate::bridge::types::{IrEvent, StopReason};
use crate::bridge::usage_capture::CaptureContext;
use crate::utils::redact::redact_text;

pub(super) async fn handle_kiro_conversation(
    surface: DownstreamSurface,
    admitted: AdmittedRequest,
    capture: CaptureContext,
) -> Response {
    let member = admitted
        .member
        .expect("handle_conversation always picks before Kiro upstream");
    let token = member.auth.token();
    let kiro_http = member.kiro_http.clone();
    if token.trim().is_empty() {
        return error_response(
            StatusCode::BAD_GATEWAY,
            "upstream_error",
            "The upstream model provider returned an error.",
            None,
        );
    }
    let prompt = prompt_from_body(surface, &admitted.body);
    if prompt.trim().is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "This request has no user text to send.",
            None,
        );
    }
    let model = admitted
        .body
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or(admitted.state.upstream.model.as_deref())
        .map(str::to_owned);
    let request_id = admitted.request_id.clone();
    let started = admitted.started;
    let stream = admitted
        .body
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let _ = (started, capture);
    if stream {
        return stream_kiro_conversation(
            surface,
            token,
            prompt,
            model,
            kiro_http,
            request_id,
            admitted.state.profile_id.to_string(),
        )
        .await;
    }

    let result = spawn_blocking(move || {
        chat_turn_with_access_token(&token, &prompt, model.as_deref(), kiro_http.as_ref())
    })
    .await;

    let turn = match result {
        Ok(Ok(turn)) => turn,
        Ok(Err(e)) => {
            tracing::debug!(
                target: "core.adapter",
                profile_id = %admitted.state.profile_id,
                request_id = %request_id,
                error = %redact_text(&e.to_string()),
                "Kiro HTTP upstream failed"
            );
            return kiro_upstream_error();
        }
        Err(_) => return kiro_upstream_error(),
    };

    let turn_view = KiroTurnView {
        text: &turn.text,
        model_id: &turn.model_id,
    };
    match encode_kiro_response(surface, false, &request_id, &turn_view) {
        Ok(response) => response,
        Err(_) => kiro_upstream_error(),
    }
}

fn kiro_upstream_error() -> Response {
    error_response(
        StatusCode::BAD_GATEWAY,
        "upstream_error",
        "The upstream model provider returned an error.",
        None,
    )
}

#[derive(Debug)]
enum KiroStreamItem {
    Text(String),
    Done,
}

async fn stream_kiro_conversation(
    surface: DownstreamSurface,
    token: String,
    prompt: String,
    model: Option<String>,
    kiro_http: Option<crate::adapters::kiro::http::KiroHttpRouteParams>,
    request_id: String,
    profile_id: String,
) -> Response {
    let (tx, rx) = unbounded_channel();
    let request_id_for_worker = request_id.clone();
    let model_for_worker = model.clone();
    thread::spawn(move || {
        let result = stream_chat_turn_with_access_token(
            &token,
            &prompt,
            model_for_worker.as_deref(),
            kiro_http.as_ref(),
            |delta| {
                let _ = tx.send(Ok(KiroStreamItem::Text(delta.to_owned())));
            },
        );
        match result {
            Ok(_) => {
                let _ = tx.send(Ok(KiroStreamItem::Done));
            }
            Err(e) => {
                tracing::debug!(
                    target: "core.adapter",
                    profile_id = %profile_id,
                    request_id = %request_id_for_worker,
                    error = %redact_text(&e.to_string()),
                    "Kiro HTTP upstream failed"
                );
                let _ = tx.send(Err(e.to_string()));
            }
        }
    });
    response_from_kiro_stream(surface, request_id, model, rx).await
}

async fn response_from_kiro_stream(
    surface: DownstreamSurface,
    request_id: String,
    model: Option<String>,
    mut rx: UnboundedReceiver<Result<KiroStreamItem, String>>,
) -> Response {
    let first = rx.recv().await;
    let first_text = match first {
        Some(Ok(KiroStreamItem::Text(text))) => text,
        Some(Ok(KiroStreamItem::Done)) | Some(Err(_)) | None => {
            return kiro_upstream_error();
        }
    };
    let model_id = model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("auto")
        .to_owned();
    let output = kiro_sse_byte_stream(surface, request_id, model_id, rx, first_text);
    buffered_sse_response(output)
}

fn kiro_sse_byte_stream(
    surface: DownstreamSurface,
    request_id: String,
    model_id: String,
    mut rx: UnboundedReceiver<Result<KiroStreamItem, String>>,
    first_text: String,
) -> impl futures_util::Stream<Item = Result<Bytes, Infallible>> + Send + 'static {
    async_stream::stream! {
        let Ok(mut encoder) = KiroSseEncoder::new(surface, &request_id, &model_id) else {
            return;
        };
        let start = IrEvent::MessageStart {
            id: format!("msg_{request_id}"),
            model: model_id.clone(),
        };
        match encoder.push_ir(&start) {
            Ok(frames) => {
                for frame in frames {
                    yield Ok(Bytes::from(frame));
                }
            }
            Err(()) => return,
        }
        match encoder.push_ir(&IrEvent::TextDelta {
            text: first_text,
        }) {
            Ok(frames) => {
                for frame in frames {
                    yield Ok(Bytes::from(frame));
                }
            }
            Err(()) => return,
        }
        while let Some(item) = rx.recv().await {
            match item {
                Ok(KiroStreamItem::Text(text)) => {
                    match encoder.push_ir(&IrEvent::TextDelta { text }) {
                        Ok(frames) => {
                            for frame in frames {
                                yield Ok(Bytes::from(frame));
                            }
                        }
                        Err(()) => return,
                    }
                }
                Ok(KiroStreamItem::Done) | Err(_) => break,
            }
        }
        match encoder.close() {
            Ok(frames) => {
                for frame in frames {
                    yield Ok(Bytes::from(frame));
                }
            }
            Err(()) => {}
        }
    }
}

enum KiroSseEncoder {
    Chat(IrToChatSse),
    Messages(IrToAnthropicSse),
    Responses(IrToResponsesSse),
}

impl KiroSseEncoder {
    fn new(surface: DownstreamSurface, request_id: &str, model: &str) -> Result<Self, ()> {
        Ok(match surface {
            DownstreamSurface::ChatCompletions => Self::Chat(IrToChatSse::new(Some(request_id))),
            DownstreamSurface::Messages => Self::Messages(IrToAnthropicSse::new()),
            DownstreamSurface::Responses => {
                Self::Responses(IrToResponsesSse::new(request_id, model))
            }
            DownstreamSurface::Models => return Err(()),
        })
    }

    fn push_ir(&mut self, event: &IrEvent) -> Result<Vec<String>, ()> {
        match self {
            Self::Chat(encoder) => encoder.push_event(event).map_err(|_| ()),
            Self::Messages(encoder) => encoder.push_event(event).map_err(|_| ()),
            Self::Responses(encoder) => {
                let events = encoder.push_event(event).map_err(|_| ())?;
                events.iter().map(responses_sse_frame).collect()
            }
        }
    }

    fn close(&mut self) -> Result<Vec<String>, ()> {
        let mut frames = self.push_ir(&IrEvent::MessageEnd {
            stop_reason: StopReason::Stop,
        })?;
        match self {
            Self::Chat(encoder) => frames.extend(encoder.finish().map_err(|_| ())?),
            Self::Messages(_) => {}
            Self::Responses(encoder) => {
                for event in encoder.finish() {
                    frames.push(responses_sse_frame(&event)?);
                }
            }
        }
        Ok(frames)
    }
}

struct KiroTurnView<'a> {
    text: &'a str,
    model_id: &'a str,
}

fn encode_kiro_response(
    surface: DownstreamSurface,
    stream_requested: bool,
    request_id: &str,
    turn: &KiroTurnView<'_>,
) -> Result<Response, ()> {
    // Kiro may resolve `auto` (or another requested alias) to a concrete model.
    // Keep the model returned by the completed turn in every downstream envelope.
    let model_id = turn.model_id.trim().to_owned();
    let ir = kiro_ir(request_id, &model_id, &turn.text);
    if stream_requested {
        let frames = encode_surface_sse(surface, &ir, request_id, &model_id)?;
        return Ok(buffered_sse_response(stream::iter(
            frames
                .into_iter()
                .map(|frame| Ok::<Bytes, Infallible>(Bytes::from(frame))),
        )));
    }
    let encoded = encode_surface(surface, &ir, request_id)?;
    Ok(Json(encoded).into_response())
}

fn buffered_sse_response(
    output: impl futures_util::Stream<Item = Result<Bytes, Infallible>> + Send + 'static,
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

fn kiro_ir(request_id: &str, model: &str, text: &str) -> Vec<IrEvent> {
    vec![
        IrEvent::MessageStart {
            id: format!("msg_{request_id}"),
            model: model.to_string(),
        },
        IrEvent::TextDelta {
            text: text.to_string(),
        },
        IrEvent::MessageEnd {
            stop_reason: StopReason::Stop,
        },
    ]
}

fn encode_surface(
    surface: DownstreamSurface,
    ir: &[IrEvent],
    request_id: &str,
) -> Result<Value, ()> {
    match surface {
        DownstreamSurface::Messages => {
            crate::bridge::protocol::anthropic_messages::encode_anthropic_message(ir)
                .map_err(|_| ())
        }
        DownstreamSurface::ChatCompletions => {
            crate::bridge::protocol::chat::encode_chat_from_ir(ir, Some(request_id)).map_err(|_| ())
        }
        DownstreamSurface::Responses => {
            crate::bridge::protocol::responses::encode_responses_from_ir(ir, Some(request_id))
                .map_err(|_| ())
        }
        DownstreamSurface::Models => Ok(json!({ "object": "list", "data": [] })),
    }
}

fn encode_surface_sse(
    surface: DownstreamSurface,
    ir: &[IrEvent],
    request_id: &str,
    model: &str,
) -> Result<Vec<String>, ()> {
    match surface {
        DownstreamSurface::Messages => {
            crate::bridge::protocol::anthropic_messages::encode_anthropic_sse(ir).map_err(|_| ())
        }
        DownstreamSurface::ChatCompletions => {
            crate::bridge::protocol::chat::encode_chat_sse(ir, Some(request_id)).map_err(|_| ())
        }
        DownstreamSurface::Responses => {
            let mut encoder =
                crate::bridge::protocol::responses::IrToResponsesSse::new(request_id, model);
            let mut frames = Vec::new();
            for event in ir {
                for response_event in encoder.push_event(event).map_err(|_| ())? {
                    frames.push(responses_sse_frame(&response_event)?);
                }
            }
            for response_event in encoder.finish() {
                frames.push(responses_sse_frame(&response_event)?);
            }
            Ok(frames)
        }
        DownstreamSurface::Models => Err(()),
    }
}

fn responses_sse_frame(event: &crate::bridge::types::BridgeEvent) -> Result<String, ()> {
    let data = serde_json::to_string(&event.data()).map_err(|_| ())?;
    Ok(format!("event: {}\ndata: {data}\n\n", event.event_name()))
}

pub(super) fn prompt_from_body(surface: DownstreamSurface, body: &Value) -> String {
    match surface {
        DownstreamSurface::Responses => flatten_responses_input(body.get("input")),
        DownstreamSurface::Messages | DownstreamSurface::ChatCompletions => {
            flatten_messages(body.get("messages"))
        }
        DownstreamSurface::Models => String::new(),
    }
}

fn flatten_messages(messages: Option<&Value>) -> String {
    let Some(arr) = messages.and_then(Value::as_array) else {
        return String::new();
    };
    let mut parts = Vec::new();
    for msg in arr {
        let role = msg.get("role").and_then(Value::as_str).unwrap_or("user");
        let text = flatten_content(msg.get("content"));
        if text.trim().is_empty() {
            continue;
        }
        if role.eq_ignore_ascii_case("system") {
            parts.push(format!("System: {text}"));
        } else if role.eq_ignore_ascii_case("assistant") {
            parts.push(format!("Assistant: {text}"));
        } else {
            parts.push(text);
        }
    }
    parts.join("\n\n")
}

fn flatten_responses_input(input: Option<&Value>) -> String {
    match input {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => {
            let mut parts = Vec::new();
            for item in items {
                if let Some(s) = item.as_str() {
                    parts.push(s.to_string());
                    continue;
                }
                let text = flatten_content(item.get("content").or(Some(item)));
                if !text.trim().is_empty() {
                    parts.push(text);
                }
            }
            parts.join("\n\n")
        }
        Some(other) => flatten_content(Some(other)),
        None => String::new(),
    }
}

fn flatten_content(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => {
            let mut out = String::new();
            for part in parts {
                if let Some(s) = part.as_str() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(s);
                    continue;
                }
                if part.get("type").and_then(Value::as_str) == Some("text")
                    || part.get("type").and_then(Value::as_str) == Some("input_text")
                    || part.get("type").and_then(Value::as_str) == Some("output_text")
                {
                    if let Some(s) = part.get("text").and_then(Value::as_str) {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(s);
                    }
                }
            }
            out
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests;
