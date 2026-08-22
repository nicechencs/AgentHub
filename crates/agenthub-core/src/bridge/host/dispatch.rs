use std::future::Future;
use std::time::Instant;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::Response;
use serde_json::Value;
use tokio::sync::OwnedSemaphorePermit;

use crate::bridge::grok_cli::{
    extract_prompt_cache_seed, inject_prompt_cache_key, GrokCliRequestIdentity,
};
use crate::bridge::protocol::anthropic_messages::to_anthropic_messages_request;
use crate::bridge::protocol::responses::{
    prepare_official_codex_request, to_grok_responses_request, to_kimi_chat_request,
    to_responses_request,
};
use crate::bridge::runtime::BridgeUpstreamProtocol;

use super::admission::{admit_conversation, AdmittedRequest};
use super::http::{
    error_response, log_protocol_error, protocol_error_response, stopping_response, ListenerState,
};
use super::stream::{
    chat_non_stream_response, chat_stream_response, messages_non_stream_response,
    messages_stream_response, non_stream_response, passthrough_sse_response, stream_response,
};
use super::surface::{DownstreamSurface, ProtocolSelector};
use super::upstream::{
    grok_identity_for, join_upstream, prepare_grok_build_body, send_upstream_with_grok_recovery,
};
use super::UPSTREAM_NON_STREAM_TIMEOUT;

pub(super) async fn handle_conversation(
    surface: DownstreamSurface,
    state: ListenerState,
    request: Request,
) -> Response {
    // Wrong-surface 404 is decided before local auth (401).
    if let Some(response) = surface.reject_if_unserved(&state) {
        return response;
    }
    let admitted = match admit_conversation(state, request, surface).await {
        Ok(admitted) => admitted,
        Err(response) => return response,
    };
    match surface {
        DownstreamSurface::Responses => dispatch_responses(admitted).await,
        DownstreamSurface::Messages => dispatch_messages(admitted).await,
        DownstreamSurface::ChatCompletions => dispatch_chat_completions(admitted).await,
        DownstreamSurface::Models => {
            unreachable!("models are synthesized by list_models, not handle_conversation")
        }
    }
}

async fn dispatch_responses(admitted: AdmittedRequest) -> Response {
    let AdmittedRequest {
        state,
        request_id,
        started,
        permit,
        headers,
        body,
    } = admitted;
    let selector = ProtocolSelector::from_listener(&state);
    let protocol = state.upstream.protocol;
    let grok_identity = grok_identity_for(
        protocol,
        &request_id,
        &headers,
        &body,
        state.upstream.model.as_deref(),
    );
    let cache_seed = grok_identity
        .as_ref()
        .and_then(|_| extract_prompt_cache_seed(&headers, &body));
    let passthrough = selector.responses_passthrough();
    let (mut upstream_body, stream_requested) = if passthrough {
        match passthrough_responses_body(body, &state) {
            Ok(pair) => pair,
            Err(response) => return response,
        }
    } else {
        let request = match DownstreamSurface::Responses.parse_request(&body) {
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
    forward_upstream(
        state,
        path,
        request_id,
        started,
        permit,
        grok_identity,
        cache_seed,
        upstream_body,
        stream_requested,
        |state, response, request_id, started, permit, cache_seed| {
            if passthrough {
                passthrough_sse_response(state, response, request_id, started, permit, cache_seed)
            } else {
                stream_response(state, response, request_id, started, permit)
            }
        },
        |state, response, request_id, started, permit, cache_seed| {
            non_stream_response(state, response, request_id, started, permit, cache_seed)
        },
    )
    .await
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
            prepare_official_codex_request(
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

async fn dispatch_messages(admitted: AdmittedRequest) -> Response {
    let AdmittedRequest {
        state,
        request_id,
        started,
        permit,
        headers,
        body,
    } = admitted;
    let grok_identity = grok_identity_for(
        state.upstream.protocol,
        &request_id,
        &headers,
        &body,
        state.upstream.model.as_deref(),
    );
    let cache_seed = grok_identity
        .as_ref()
        .and_then(|_| extract_prompt_cache_seed(&headers, &body));
    let request = match DownstreamSurface::Messages.parse_request(&body) {
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
            prepare_official_codex_request(
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
    forward_upstream(
        state,
        path,
        request_id,
        started,
        permit,
        grok_identity,
        cache_seed,
        upstream_body,
        stream_requested,
        |state, response, request_id, started, permit, cache_seed| {
            messages_stream_response(state, response, request_id, started, permit, cache_seed)
        },
        |state, response, request_id, started, permit, cache_seed| {
            messages_non_stream_response(state, response, request_id, started, permit, cache_seed)
        },
    )
    .await
}

async fn dispatch_chat_completions(admitted: AdmittedRequest) -> Response {
    let AdmittedRequest {
        state,
        request_id,
        started,
        permit,
        headers: _,
        body,
    } = admitted;
    let request = match DownstreamSurface::ChatCompletions.parse_request(&body) {
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
    prepare_official_codex_request(
        &mut upstream_body,
        &request.model,
        state.upstream.model.as_deref(),
    );
    forward_upstream(
        state,
        "responses",
        request_id,
        started,
        permit,
        None,
        None,
        upstream_body,
        stream_requested,
        |state, response, request_id, started, permit, _cache_seed| {
            chat_stream_response(state, response, request_id, started, permit)
        },
        |state, response, request_id, started, permit, _cache_seed| {
            chat_non_stream_response(state, response, request_id, started, permit)
        },
    )
    .await
}

async fn forward_upstream<S, N, NFut>(
    state: ListenerState,
    path: &'static str,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
    grok_identity: Option<GrokCliRequestIdentity>,
    cache_seed: Option<String>,
    upstream_body: Value,
    stream_requested: bool,
    on_stream: S,
    on_non_stream: N,
) -> Response
where
    S: FnOnce(
        ListenerState,
        reqwest::Response,
        String,
        Instant,
        OwnedSemaphorePermit,
        Option<String>,
    ) -> Response,
    N: FnOnce(
        ListenerState,
        reqwest::Response,
        String,
        Instant,
        OwnedSemaphorePermit,
        Option<String>,
    ) -> NFut,
    NFut: Future<Output = Response>,
{
    let protocol = state.upstream.protocol;
    let url = match join_upstream(&state, path) {
        Ok(url) => url,
        Err(response) => return response,
    };
    let response = match send_upstream_with_grok_recovery(
        &state,
        url,
        protocol,
        &request_id,
        started,
        grok_identity,
        upstream_body,
        cache_seed.as_deref(),
    )
    .await
    {
        Ok(response) => response,
        Err(response) => return response,
    };
    if stream_requested {
        on_stream(state, response, request_id, started, permit, cache_seed)
    } else {
        let force_shutdown = state.force_shutdown.clone();
        tokio::select! {
            _ = force_shutdown.cancelled() => stopping_response(),
            result = tokio::time::timeout(
                UPSTREAM_NON_STREAM_TIMEOUT,
                on_non_stream(state.clone(), response, request_id, started, permit, cache_seed),
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
