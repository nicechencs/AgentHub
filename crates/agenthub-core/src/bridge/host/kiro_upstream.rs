//! In-process Kiro upstream: downstream surface → GenerateAssistantResponse.
//!
//! Does not POST OpenAI/Anthropic; uses the AgentHub-owned Kiro HTTP client.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use tokio::task::spawn_blocking;

use super::admission::AdmittedRequest;
use super::http::error_response;
use super::surface::DownstreamSurface;
use crate::adapters::kiro::http::chat_turn_with_access_token;
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

    let result = spawn_blocking(move || {
        chat_turn_with_access_token(&token, &prompt, model.as_deref())
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
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider returned an error.",
                None,
            );
        }
        Err(_) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider returned an error.",
                None,
            );
        }
    };

    let model_id = admitted
        .state
        .upstream
        .model
        .clone()
        .unwrap_or_else(|| "auto".into());
    let ir = kiro_ir(&request_id, &model_id, &turn.text);
    let encoded = match encode_surface(surface, &ir, &request_id) {
        Ok(value) => value,
        Err(_) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider returned an invalid response.",
                None,
            );
        }
    };

    let _ = (started, capture, stream);
    Json(encoded).into_response()
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
            crate::bridge::protocol::anthropic_messages::encode_anthropic_message(ir).map_err(|_| ())
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
mod tests {
    use super::*;

    #[test]
    fn prompt_from_messages_joins_roles() {
        let body = json!({
            "messages": [
                {"role": "system", "content": "be brief"},
                {"role": "user", "content": "hi"}
            ]
        });
        let prompt = prompt_from_body(DownstreamSurface::Messages, &body);
        assert!(prompt.contains("System: be brief"));
        assert!(prompt.contains("hi"));
    }

    #[test]
    fn prompt_from_responses_string_input() {
        let body = json!({ "input": "hello kiro" });
        assert_eq!(
            prompt_from_body(DownstreamSurface::Responses, &body),
            "hello kiro"
        );
    }

    #[test]
    fn encode_messages_has_text() {
        let ir = kiro_ir("abc", "auto", "pong");
        let value = encode_surface(DownstreamSurface::Messages, &ir, "abc").unwrap();
        let dump = value.to_string();
        assert!(dump.contains("pong"));
    }
}
