use std::time::Instant;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::Response;
use tokio::sync::OwnedSemaphorePermit;

use super::admission::{admit_conversation, AdmittedRequest};
use super::gateway::{Gateway, GatewayAuthError, ModelSwitchOutcome};
use super::http::{error_response, reject_invalid_local_auth, stopping_response, EdgeState};
use super::stream::{
    chat_non_stream_response, chat_stream_response, messages_non_stream_response,
    messages_stream_response, non_stream_response, passthrough_json_response,
    passthrough_sse_response, stream_response,
};
use super::surface::DownstreamSurface;
use super::transport::{send_upstream, UpstreamChannel, UpstreamPrepare, UpstreamSendOutcome};
use super::upstream::join_upstream;
use super::UPSTREAM_NON_STREAM_TIMEOUT;
use crate::bridge::account::PickedMember;

pub(super) async fn handle_conversation(
    surface: DownstreamSurface,
    gateway: Gateway,
    request: Request,
) -> Response {
    let request_id = uuid::Uuid::new_v4().to_string();
    let started = Instant::now();
    // Bearer is the only middleware: invalid/missing token is always 401, even
    // when this path would later 404 for the bound edge.
    let state = match gateway.authenticate(request.headers()) {
        Ok(state) => state,
        Err(GatewayAuthError::Unauthorized) => {
            return reject_invalid_local_auth(surface.op(), Some((&request_id, started)));
        }
        Err(GatewayAuthError::Stopping | GatewayAuthError::Poisoned) => {
            return stopping_response();
        }
    };
    if let Some(response) = surface.reject_if_unserved(&state, &request_id) {
        return response;
    }
    tracing::debug!(
        target: "core.adapter",
        profile_id = %state.profile_id,
        request_id = %request_id,
        op = surface.op(),
        "request started"
    );
    let mut admitted = match admit_conversation(state, request, surface, request_id, started).await
    {
        Ok(admitted) => admitted,
        Err(response) => return response,
    };
    if let Some(serde_json::Value::String(model)) = admitted.body.get_mut("model") {
        let stripped = crate::models::strip_claude_context_marker(model).to_owned();
        if stripped != *model {
            *model = stripped;
        }
    }
    let model = admitted
        .body
        .get("model")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let mut resolver_candidates = None;
    if let Some(index) = &admitted.state.route_index {
        let endpoint = DownstreamSurface::endpoint_key(admitted.state.upstream.local_surface);
        match index.resolve(endpoint, model) {
            Ok(candidates) if !candidates.is_empty() => {
                resolver_candidates = Some(candidates);
            }
            Ok(_) | Err(_) => {
                tracing::warn!(
                    target: "core.adapter",
                    profile_id = %admitted.state.profile_id,
                    request_id = %admitted.request_id,
                    model,
                    op = "upstream",
                    code = "model_unavailable",
                    status = 400_u16,
                    "v2 route index failed closed before any upstream attempt"
                );
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "model_unavailable",
                    "No running route can serve this model.",
                    None,
                );
            }
        }
    } else {
        match gateway.switch_edge_for_model(&admitted.state, model) {
            ModelSwitchOutcome::Stay => {}
            ModelSwitchOutcome::Switched(mut switched) => {
                tracing::info!(
                    target: "core.adapter",
                    request_id = %admitted.request_id,
                    lead_profile_id = %admitted.state.profile_id,
                    switch_profile_id = %switched.profile_id,
                    model,
                    "request-scoped model switch after lead mapping miss"
                );
                if !model.is_empty() {
                    switched.upstream.model = Some(model.to_owned());
                }
                admitted.state = switched;
            }
            ModelSwitchOutcome::Unavailable => {
                let listed_hit = admitted
                    .state
                    .listed_models
                    .iter()
                    .any(|item| crate::models::listed_model_matches(item, model));
                let listed_restricted = !admitted.state.listed_models.is_empty();
                let code = if listed_restricted && !listed_hit && !model.is_empty() {
                    "listed_models_reject"
                } else {
                    "model_unavailable"
                };
                tracing::warn!(
                    target: "core.adapter",
                    profile_id = %admitted.state.profile_id,
                    request_id = %admitted.request_id,
                    model,
                    listed = listed_restricted,
                    op = "upstream",
                    code,
                    status = 400_u16,
                    "lead mapping missed and no running alternate can serve this model"
                );
                return error_response(
                    StatusCode::BAD_REQUEST,
                    code,
                    "No running route can serve this model.",
                    None,
                );
            }
        }
    }
    // Re-prepare from this admitted body after the pick. Do not reuse a
    // sibling member's mutated headers/body.
    let original_body = admitted.body.clone();
    let Some(member) = (match &resolver_candidates {
        Some(candidates) => {
            admitted
                .state
                .account_picker
                .pick_from_candidates(candidates, None, &[])
        }
        None => admitted.state.account_picker.pick_new(),
    }) else {
        return no_eligible_member(&admitted.state, &admitted.request_id, admitted.started);
    };
    admitted.body = original_body;
    admitted.member = Some(member);
    let channel = UpstreamChannel::from_protocol(admitted.state.upstream.protocol);
    let prepared = match channel.prepare(surface, &admitted) {
        Ok(prepared) => prepared,
        Err(response) => return response,
    };
    tracing::debug!(
        target: "core.adapter.protocol",
        request_id = %admitted.request_id,
        profile_id = %admitted.state.profile_id,
        from = surface.op(),
        to = prepared.path,
        "downstream surface converted to upstream path"
    );
    forward_upstream(surface, admitted, channel, prepared).await
}

fn no_eligible_member(state: &EdgeState, request_id: &str, started: Instant) -> Response {
    tracing::warn!(
        target: "core.adapter",
        profile_id = %state.profile_id,
        request_id = %request_id,
        op = "upstream",
        code = "upstream_unavailable",
        status = 502_u16,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "bridge has no eligible upstream account"
    );
    state.record_upstream_failure();
    error_response(
        StatusCode::BAD_GATEWAY,
        "upstream_error",
        "The upstream model provider returned an error.",
        None,
    )
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
        member,
    } = admitted;
    let member = member.expect("handle_conversation always picks before forward");
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
    let UpstreamSendOutcome { response, member } = match send_upstream(
        &state,
        url,
        channel,
        &request_id,
        started,
        grok_identity,
        body,
        cache_seed.as_deref(),
        member,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(response) => return response,
    };
    if stream_requested {
        return forward_stream(
            surface, channel, state, response, request_id, started, permit, cache_seed, member,
        );
    }
    let force_shutdown = state.force_shutdown.clone();
    tokio::select! {
        _ = force_shutdown.cancelled() => stopping_response(),
        result = tokio::time::timeout(
            UPSTREAM_NON_STREAM_TIMEOUT,
            forward_non_stream(surface, channel, state.clone(), response, request_id, started, permit, cache_seed, member),
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
    state: EdgeState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
    cache_seed: Option<String>,
    member: PickedMember,
) -> Response {
    if channel.passthrough_for(surface) {
        return passthrough_sse_response(
            state, response, request_id, started, permit, cache_seed, member, surface,
        );
    }
    match surface {
        DownstreamSurface::Responses => {
            stream_response(state, response, request_id, started, permit, member)
        }
        DownstreamSurface::Messages => messages_stream_response(
            state, response, request_id, started, permit, cache_seed, member,
        ),
        DownstreamSurface::ChatCompletions => chat_stream_response(
            state, response, request_id, started, permit, cache_seed, member,
        ),
        DownstreamSurface::Models => {
            unreachable!("models are synthesized by list_models, not handle_conversation")
        }
    }
}

async fn forward_non_stream(
    surface: DownstreamSurface,
    channel: UpstreamChannel,
    state: EdgeState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
    cache_seed: Option<String>,
    member: PickedMember,
) -> Response {
    if channel.passthrough_for(surface) {
        return passthrough_json_response(
            state, response, request_id, started, permit, cache_seed, member,
        )
        .await;
    }
    match surface {
        DownstreamSurface::Responses => {
            non_stream_response(
                state, response, request_id, started, permit, cache_seed, member,
            )
            .await
        }
        DownstreamSurface::Messages => {
            messages_non_stream_response(
                state, response, request_id, started, permit, cache_seed, member,
            )
            .await
        }
        DownstreamSurface::ChatCompletions => {
            chat_non_stream_response(
                state, response, request_id, started, permit, cache_seed, member,
            )
            .await
        }
        DownstreamSurface::Models => {
            unreachable!("models are synthesized by list_models, not handle_conversation")
        }
    }
}
