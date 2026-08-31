use std::time::Instant;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::Response;
use tokio::sync::OwnedSemaphorePermit;

use super::admission::{admit_conversation, AdmittedRequest};
use super::gateway::{Gateway, GatewayAuthError, ModelSwitchOutcome};
use super::http::{error_response, reject_invalid_local_auth, stopping_response, EdgeState};
use super::pair_policy::{
    identity_relay, pair_adapter_active, pair_adapter_rejected, pair_direction, pair_model_servable,
};
use super::stream::{
    chat_non_stream_response, chat_stream_response, messages_non_stream_response,
    messages_stream_response, non_stream_response, passthrough_json_response,
    passthrough_sse_response, stream_response,
};
use super::surface::DownstreamSurface;
use super::transport::{
    send_upstream, send_upstream_v2, UpstreamChannel, UpstreamPrepare, UpstreamSendOutcome,
};
use super::upstream::{join_upstream, pool_exhausted_response};
use super::UPSTREAM_NON_STREAM_TIMEOUT;
use crate::bridge::account::PickedMember;
use crate::bridge::route_index::DispatchCandidate;
use crate::bridge::usage_capture::CaptureContext;

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
    let initial_channel = UpstreamChannel::from_protocol(admitted.state.upstream.protocol);
    if surface == DownstreamSurface::Responses
        && pair_adapter_rejected(&admitted.state, initial_channel)
    {
        return pair_adapter_rejected_response(
            &admitted.state,
            &admitted.request_id,
            admitted.started,
        );
    }
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
        .unwrap_or("")
        .to_owned();
    let mut resolver_candidates = None;
    if let Some(index) = &admitted.state.route_index {
        let endpoint = DownstreamSurface::endpoint_key(admitted.state.upstream.local_surface);
        match index.resolve(endpoint, &model) {
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
        match gateway.switch_edge_for_model(&admitted.state, &model) {
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
                    .any(|item| crate::models::listed_model_matches(item, &model));
                let listed_restricted = !admitted.state.listed_models.is_empty();
                let pair_active = pair_adapter_active(
                    &admitted.state,
                    UpstreamChannel::from_protocol(admitted.state.upstream.protocol),
                );
                let code = if pair_active {
                    "model_unavailable"
                } else if listed_restricted && !listed_hit && !model.is_empty() {
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
    let channel = UpstreamChannel::from_protocol(admitted.state.upstream.protocol);
    if surface == DownstreamSurface::Responses && pair_adapter_rejected(&admitted.state, channel) {
        return pair_adapter_rejected_response(
            &admitted.state,
            &admitted.request_id,
            admitted.started,
        );
    }
    if pair_adapter_active(&admitted.state, channel)
        && !pair_model_servable(&admitted.state, &model)
    {
        tracing::warn!(
            target: "core.adapter",
            profile_id = %admitted.state.profile_id,
            request_id = %admitted.request_id,
            model,
            op = "upstream",
            code = "model_unavailable",
            status = 400_u16,
            "pair adapter has no upstream mapping for this model"
        );
        return error_response(
            StatusCode::BAD_REQUEST,
            "model_unavailable",
            "No running route can serve this model.",
            None,
        );
    }
    let pair_active = pair_adapter_active(&admitted.state, channel);
    admitted.affinity_key = admitted
        .state
        .affinity_key_for(&admitted.body, &admitted.headers);
    let required_member = if pair_active {
        admitted
            .state
            .continuations
            .required_member(&admitted.body, &admitted.headers)
    } else {
        None
    };
    if pair_active
        && crate::bridge::protocol::pair::previous_response_id(&admitted.body).is_some()
        && required_member.is_none()
    {
        return continuation_unavailable(&admitted.state, &admitted.request_id, admitted.started);
    }
    let Some(member) = (if let Some(required_id) = required_member.as_deref() {
        match pick_bound_member(
            &admitted.state,
            resolver_candidates.as_deref(),
            required_id,
            &model,
        ) {
            Some(member) => Some(member),
            None => {
                return continuation_unavailable(
                    &admitted.state,
                    &admitted.request_id,
                    admitted.started,
                );
            }
        }
    } else {
        match &resolver_candidates {
            // v2: never fall back to pick_new() — that ignores cooldown,
            // auth isolation, and the resolver candidate set.
            Some(candidates) => {
                admitted
                    .state
                    .pick_v2(candidates, &model, &[], admitted.affinity_key.as_deref())
            }
            None => admitted.state.account_picker.pick_new(),
        }
    }) else {
        return no_eligible_member(
            &admitted.state,
            &admitted.request_id,
            admitted.started,
            &model,
        );
    };
    let continuation_locked = pair_active && required_member.is_some();
    admitted.member = Some(member);
    // Gateway usage capture identity: only fields the dispatch path already
    // knows. The session id reuses the affinity extractor, so capture adds no
    // new body parsing.
    let capture = CaptureContext {
        surface: surface.op(),
        model: model.clone(),
        session_id: super::continuation::session_identifier(&admitted.body, &admitted.headers),
        channel: None,
    };
    if admitted.state.route_index.is_some() {
        return forward_upstream_v2(
            surface,
            admitted,
            resolver_candidates,
            model,
            continuation_locked,
            capture,
        )
        .await;
    }
    let transport = channel.transport();
    let prepared = match transport.prepare(surface, &admitted) {
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
    forward_upstream(surface, admitted, channel, prepared, continuation_locked, capture).await
}

fn pair_adapter_rejected_response(
    state: &EdgeState,
    request_id: &str,
    started: Instant,
) -> Response {
    tracing::warn!(
        target: "core.adapter",
        profile_id = %state.profile_id,
        request_id = %request_id,
        op = "upstream",
        code = "route_unavailable",
        status = 503_u16,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "explicit Responses profile is not authorized for this route"
    );
    error_response(
        StatusCode::SERVICE_UNAVAILABLE,
        "route_unavailable",
        "This route cannot serve the requested Responses format.",
        None,
    )
}

fn pick_bound_member(
    state: &EdgeState,
    candidates: Option<&[DispatchCandidate]>,
    member_id: &str,
    model: &str,
) -> Option<crate::bridge::account::PickedMember> {
    let _ = model;
    let matches_id = |member: &crate::bridge::account::PickedMember| {
        member.source_id == member_id || member.ticket_id == member_id || member.label == member_id
    };
    state
        .account_picker
        .members()
        .iter()
        .find(|member| {
            if !matches_id(member) || !member.is_eligible() {
                return false;
            }
            if state
                .auth_reload
                .is_isolated(&member.authorization_fingerprint())
            {
                return false;
            }
            match candidates {
                Some(candidates) if !candidates.is_empty() => candidates.iter().any(|candidate| {
                    member.source_id == candidate.member_id
                        || member.ticket_id == candidate.member_id
                        || member.label == candidate.member_id
                }),
                _ => true,
            }
        })
        .cloned()
}

fn continuation_unavailable(state: &EdgeState, request_id: &str, started: Instant) -> Response {
    tracing::warn!(
        target: "core.adapter",
        profile_id = %state.profile_id,
        request_id = %request_id,
        op = "upstream",
        code = "continuation_unavailable",
        status = 400_u16,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "stateful continuation cannot keep the original login"
    );
    error_response(
        StatusCode::BAD_REQUEST,
        "continuation_unavailable",
        "This conversation cannot continue because the original login is no longer available.",
        None,
    )
}

fn no_eligible_member(
    state: &EdgeState,
    request_id: &str,
    started: Instant,
    model: &str,
) -> Response {
    if state.route_index.is_some() {
        tracing::warn!(
            target: "core.adapter",
            profile_id = %state.profile_id,
            request_id = %request_id,
            op = "upstream",
            code = "pool_exhausted",
            status = 503_u16,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "v2 route pool has no eligible member"
        );
        state.record_upstream_failure();
        return pool_exhausted_response(state.account_picker.soonest_retry_after(model));
    }
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

async fn forward_upstream_v2(
    surface: DownstreamSurface,
    admitted: AdmittedRequest,
    candidates: Option<Vec<DispatchCandidate>>,
    public_model: String,
    continuation_locked: bool,
    mut capture: CaptureContext,
) -> Response {
    let AdmittedRequest {
        state,
        request_id,
        started,
        permit,
        headers,
        body,
        member,
        affinity_key,
    } = admitted;
    let member = member.expect("handle_conversation always picks before forward");
    let UpstreamSendOutcome {
        response,
        member,
        channel,
        cache_seed,
        stream,
    } = match send_upstream_v2(
        &state,
        surface,
        &request_id,
        started,
        &headers,
        &body,
        member,
        candidates.as_deref().unwrap_or(&[]),
        &public_model,
        continuation_locked,
        affinity_key.as_deref(),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(response) => return response,
    };
    capture.channel = Some(channel.name());
    finish_upstream(
        surface, channel, state, response, request_id, started, permit, cache_seed, member, stream,
        capture,
    )
    .await
}

async fn forward_upstream(
    surface: DownstreamSurface,
    admitted: AdmittedRequest,
    channel: UpstreamChannel,
    prepared: UpstreamPrepare,
    continuation_locked: bool,
    mut capture: CaptureContext,
) -> Response {
    let AdmittedRequest {
        state,
        request_id,
        started,
        permit,
        headers: _,
        body: _,
        member,
        affinity_key: _,
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
    let UpstreamSendOutcome {
        response, member, ..
    } = match send_upstream(
        &state,
        url,
        channel,
        &request_id,
        started,
        grok_identity,
        body,
        cache_seed.as_deref(),
        member,
        continuation_locked,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(response) => return response,
    };
    capture.channel = Some(channel.name());
    finish_upstream(
        surface,
        channel,
        state,
        response,
        request_id,
        started,
        permit,
        cache_seed,
        member,
        stream_requested,
        capture,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn finish_upstream(
    surface: DownstreamSurface,
    channel: UpstreamChannel,
    state: EdgeState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
    cache_seed: Option<String>,
    member: PickedMember,
    stream_requested: bool,
    capture: CaptureContext,
) -> Response {
    if stream_requested {
        return forward_stream(
            surface, channel, state, response, request_id, started, permit, cache_seed, member,
            capture,
        );
    }
    let force_shutdown = state.force_shutdown.clone();
    tokio::select! {
        _ = force_shutdown.cancelled() => stopping_response(),
        result = tokio::time::timeout(
            UPSTREAM_NON_STREAM_TIMEOUT,
            forward_non_stream(surface, channel, state.clone(), response, request_id, started, permit, cache_seed, member, capture),
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

#[allow(clippy::too_many_arguments)]
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
    capture: CaptureContext,
) -> Response {
    if identity_relay(channel, surface, &state) {
        return passthrough_sse_response(
            state, response, request_id, started, permit, cache_seed, member, surface, None,
            capture,
        );
    }
    if surface == DownstreamSurface::Responses {
        if let Some(direction) = pair_direction(&state, channel) {
            if pair_adapter_active(&state, channel) {
                return passthrough_sse_response(
                    state,
                    response,
                    request_id,
                    started,
                    permit,
                    cache_seed,
                    member,
                    surface,
                    Some(direction),
                    capture,
                );
            }
        }
    }
    match surface {
        DownstreamSurface::Responses => {
            stream_response(state, response, request_id, started, permit, member, capture)
        }
        DownstreamSurface::Messages => messages_stream_response(
            state, response, request_id, started, permit, cache_seed, member, capture,
        ),
        DownstreamSurface::ChatCompletions => chat_stream_response(
            state, response, request_id, started, permit, cache_seed, member, capture,
        ),
        DownstreamSurface::Models => {
            unreachable!("models are synthesized by list_models, not handle_conversation")
        }
    }
}

#[allow(clippy::too_many_arguments)]
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
    capture: CaptureContext,
) -> Response {
    if identity_relay(channel, surface, &state) {
        return passthrough_json_response(
            state, response, request_id, started, permit, cache_seed, member, None, capture,
        )
        .await;
    }
    if surface == DownstreamSurface::Responses {
        if let Some(direction) = pair_direction(&state, channel) {
            if pair_adapter_active(&state, channel) {
                return passthrough_json_response(
                    state,
                    response,
                    request_id,
                    started,
                    permit,
                    cache_seed,
                    member,
                    Some(direction),
                    capture,
                )
                .await;
            }
        }
    }
    match surface {
        DownstreamSurface::Responses => {
            non_stream_response(
                state, response, request_id, started, permit, cache_seed, member, capture,
            )
            .await
        }
        DownstreamSurface::Messages => {
            messages_non_stream_response(
                state, response, request_id, started, permit, cache_seed, member, capture,
            )
            .await
        }
        DownstreamSurface::ChatCompletions => {
            chat_non_stream_response(
                state, response, request_id, started, permit, cache_seed, member, capture,
            )
            .await
        }
        DownstreamSurface::Models => {
            unreachable!("models are synthesized by list_models, not handle_conversation")
        }
    }
}
