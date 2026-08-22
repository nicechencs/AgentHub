use std::time::Instant;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::Response;
use tokio::sync::OwnedSemaphorePermit;

use super::admission::{admit_conversation, AdmittedRequest};
use super::http::{error_response, stopping_response, ListenerState};
use super::stream::{
    chat_non_stream_response, chat_stream_response, messages_non_stream_response,
    messages_stream_response, non_stream_response, passthrough_sse_response, stream_response,
};
use super::surface::DownstreamSurface;
use super::transport::{send_upstream, UpstreamChannel, UpstreamPrepare};
use super::upstream::join_upstream;
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
    let channel = UpstreamChannel::from_protocol(admitted.state.upstream.protocol);
    let prepared = match channel.prepare(surface, &admitted) {
        Ok(prepared) => prepared,
        Err(response) => return response,
    };
    forward_upstream(surface, admitted, channel, prepared).await
}

async fn forward_upstream(
    surface: DownstreamSurface,
    admitted: AdmittedRequest,
    channel: UpstreamChannel,
    prepared: UpstreamPrepare,
) -> Response {
    let AdmittedRequest {
        state,
        request_id,
        started,
        permit,
        headers: _,
        body: _,
    } = admitted;
    let UpstreamPrepare {
        path,
        body,
        grok_identity,
        cache_seed,
        stream: stream_requested,
    } = prepared;
    let url = match join_upstream(&state, path) {
        Ok(url) => url,
        Err(response) => return response,
    };
    let response = match send_upstream(
        &state,
        url,
        channel,
        &request_id,
        started,
        grok_identity,
        body,
        cache_seed.as_deref(),
    )
    .await
    {
        Ok(response) => response,
        Err(response) => return response,
    };
    if stream_requested {
        return forward_stream(
            surface,
            channel,
            state,
            response,
            request_id,
            started,
            permit,
            cache_seed,
        );
    }
    let force_shutdown = state.force_shutdown.clone();
    tokio::select! {
        _ = force_shutdown.cancelled() => stopping_response(),
        result = tokio::time::timeout(
            UPSTREAM_NON_STREAM_TIMEOUT,
            forward_non_stream(surface, state.clone(), response, request_id, started, permit, cache_seed),
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

fn forward_stream(
    surface: DownstreamSurface,
    channel: UpstreamChannel,
    state: ListenerState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
    cache_seed: Option<String>,
) -> Response {
    match surface {
        DownstreamSurface::Responses if channel.passthrough() => {
            passthrough_sse_response(state, response, request_id, started, permit, cache_seed)
        }
        DownstreamSurface::Responses => {
            stream_response(state, response, request_id, started, permit)
        }
        DownstreamSurface::Messages => {
            messages_stream_response(state, response, request_id, started, permit, cache_seed)
        }
        DownstreamSurface::ChatCompletions => {
            chat_stream_response(state, response, request_id, started, permit)
        }
        DownstreamSurface::Models => {
            unreachable!("models are synthesized by list_models, not handle_conversation")
        }
    }
}

async fn forward_non_stream(
    surface: DownstreamSurface,
    state: ListenerState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
    cache_seed: Option<String>,
) -> Response {
    match surface {
        DownstreamSurface::Responses => {
            non_stream_response(state, response, request_id, started, permit, cache_seed).await
        }
        DownstreamSurface::Messages => {
            messages_non_stream_response(state, response, request_id, started, permit, cache_seed)
                .await
        }
        DownstreamSurface::ChatCompletions => {
            chat_non_stream_response(state, response, request_id, started, permit).await
        }
        DownstreamSurface::Models => {
            unreachable!("models are synthesized by list_models, not handle_conversation")
        }
    }
}
