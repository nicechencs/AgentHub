//! v2 indexed failover: typed errors, candidate exclusion, cooldown, pool_exhausted.
//!
//! Only used when `EdgeState.route_index` is Some. Each attempt re-prepares
//! from the original admitted request using that candidate's transport. v1
//! `send_upstream` stays byte-for-byte on edges without an index.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use serde_json::Value;
use tokio::sync::Semaphore;

use crate::bridge::account::PickedMember;
use crate::bridge::grok_cli::{is_reasoning_decode_failure, strip_encrypted_reasoning};
use crate::bridge::route_index::DispatchCandidate;
use crate::bridge::upstream_class::{
    classify_http, cooldown_from_retry_after, FailoverDecision, UpstreamErrorClass,
};

use super::super::admission::AdmittedRequest;
use super::super::http::{error_response, stopping_response, EdgeState};
use super::super::stream::UpstreamBodyError;
use super::super::surface::DownstreamSurface;
use super::super::upstream::{
    extract_upstream_error_detail, grok_replay_model, join_upstream, map_upstream_http_error,
    map_v2_request_error, pool_exhausted_response, post_upstream_attempt,
    read_bounded_upstream_error, replay_session, timeout_response, upstream_header_timeout,
    UpstreamConnectError,
};
use super::{
    identity_for_member, log_serving_account, UpstreamChannel, UpstreamPrepare, UpstreamSendOutcome,
};

const V2_MAX_ATTEMPTS: usize = 8;

pub async fn send_upstream_v2(
    state: &EdgeState,
    surface: DownstreamSurface,
    request_id: &str,
    started: Instant,
    headers: &HeaderMap,
    original_body: &Value,
    member: PickedMember,
    candidates: &[DispatchCandidate],
    public_model: &str,
    continuation_locked: bool,
    affinity_key: Option<&str>,
) -> Result<UpstreamSendOutcome, Response> {
    let mut member = member;
    let mut excluded: Vec<String> = Vec::new();
    let mut reloaded: HashSet<String> = HashSet::new();
    let mut failover_from: Option<String> = None;
    let mut grok_strip_attempt = 0u8;
    let max_attempts = candidates.len().clamp(1, V2_MAX_ATTEMPTS);
    let mut attempts = 0usize;
    let mut last_fail: Option<(
        UpstreamErrorClass,
        StatusCode,
        Option<HeaderValue>,
        Option<String>,
    )> = None;

    loop {
        attempts += 1;
        if attempts > max_attempts {
            return Err(exhausted_or_last_fail(
                state,
                request_id,
                started,
                public_model,
                failover_from.as_deref(),
                &member,
                last_fail.as_ref(),
            ));
        }
        let candidate = candidate_for_member(candidates, &member);
        let Some(_member_permit) = member.try_acquire() else {
            exclude_member(&mut excluded, &member);
            if continuation_locked {
                return Err(exhausted(
                    state,
                    request_id,
                    started,
                    public_model,
                    failover_from.as_deref(),
                ));
            }
            let Some(next) = state.pick_v2_in_lane(
                candidates,
                public_model,
                &excluded,
                Some(member.source_id.as_str()),
                affinity_key,
            ) else {
                return Err(exhausted(
                    state,
                    request_id,
                    started,
                    public_model,
                    failover_from.as_deref(),
                ));
            };
            failover_from = Some(member.source_id.clone());
            member = next;
            continue;
        };
        let (attempt_channel, attempt_url, prepared) = prepare_candidate_attempt(
            state,
            surface,
            headers,
            original_body,
            request_id,
            started,
            &member,
            candidate,
            public_model,
        )?;
        let transport = attempt_channel.transport();
        let recovery = transport.recovery();
        let fingerprint = member.authorization_fingerprint();
        let account_id = state
            .account_picker
            .partition_account_id(&member)
            .map(str::to_owned);
        let account_id = account_id.as_deref();
        let cache_seed = prepared.cache_seed.clone();
        let stream = prepared.stream;
        let mut identity =
            identity_for_member(&prepared.grok_identity, cache_seed.as_deref(), account_id);
        let mut body = prepared.body;
        if recovery.strips_grok_reasoning() {
            let model = grok_replay_model(&body, Some(public_model));
            state.grok_replay.apply(
                &mut body,
                &model,
                replay_session(cache_seed.as_deref(), account_id).as_deref(),
            );
        }

        tracing::info!(
            target: "core.adapter",
            profile_id = %state.profile_id,
            request_id = %request_id,
            route_id = state.route_index.as_ref().map(|index| index.route_id.as_str()).unwrap_or(""),
            member_id = %member.source_id,
            ?attempt_channel,
            model = %public_model,
            attempt = attempts,
            retry_reason = failover_from.as_deref().unwrap_or(""),
            "v2 upstream attempt"
        );

        loop {
            let token = member.auth.token();
            let builder = transport.apply_auth(
                state.client.post(attempt_url.clone()).json(&body),
                &token,
                identity.as_ref(),
            );
            let response = match post_upstream_attempt(
                state,
                builder,
                request_id,
                upstream_header_timeout(stream || attempt_channel.forces_upstream_stream()),
            )
            .await
            {
                Ok(response) => response,
                Err(UpstreamConnectError::Stopping) => return Err(stopping_response()),
                Err(UpstreamConnectError::Timeout) => {
                    // Headers timed out after the request was sent; upstream may
                    // already be generating/billing. Do not replay onto another member.
                    return Err(timeout_response());
                }
                Err(UpstreamConnectError::Unavailable) => {
                    // Transport failed before a usable response — safe to try another member.
                    exclude_member(&mut excluded, &member);
                    break;
                }
            };
            if response.status().is_success() {
                log_serving_account(
                    state,
                    request_id,
                    &member,
                    failover_from.is_some(),
                    failover_from.as_deref(),
                );
                return Ok(UpstreamSendOutcome {
                    response,
                    member,
                    channel: attempt_channel,
                    cache_seed,
                    stream,
                });
            }

            let status = response.status();
            let retry_after = response.headers().get(header::RETRY_AFTER).cloned();
            let error_body =
                match read_bounded_upstream_error(response, &state.force_shutdown).await {
                    Ok(body) => body,
                    Err(UpstreamBodyError::Stopping) => return Err(stopping_response()),
                    Err(UpstreamBodyError::InvalidOrTooLarge | UpstreamBodyError::IncompleteStream) => {
                        Vec::new()
                    }
                };
            let detail = extract_upstream_error_detail(&error_body);
            let err_text = String::from_utf8_lossy(&error_body);
            let grok_recoverable = recovery.strips_grok_reasoning()
                && status == StatusCode::BAD_REQUEST
                && grok_strip_attempt < 2
                && is_reasoning_decode_failure(&err_text);
            let class = classify_http(status, Some(err_text.as_ref()), grok_recoverable);
            match class.decision(false) {
                FailoverDecision::RetrySameMember => {
                    let replay_seed = replay_session(cache_seed.as_deref(), account_id);
                    let model = grok_replay_model(&body, state.upstream.model.as_deref());
                    state.grok_replay.clear(&model, replay_seed.as_deref());
                    strip_encrypted_reasoning(&mut body);
                    grok_strip_attempt += 1;
                    if grok_strip_attempt >= 2 {
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
                        account_id = %member.source_id,
                        attempt = grok_strip_attempt,
                        "retrying Grok request after encrypted reasoning rejection"
                    );
                    continue;
                }
                FailoverDecision::ReturnToClient => {
                    return Err(map_request_or_upstream(
                        state,
                        request_id,
                        started,
                        class,
                        status,
                        retry_after,
                        detail.as_deref(),
                        &member,
                        failover_from.as_deref(),
                    ));
                }
                FailoverDecision::ReloadThenFailover => {
                    if reloaded.insert(fingerprint.clone()) {
                        let outcome = state.auth_reload.reload_member(&member).await;
                        if outcome.should_retry() {
                            tracing::info!(
                                target: "core.adapter",
                                profile_id = %state.profile_id,
                                request_id = %request_id,
                                account_id = %member.source_id,
                                "retrying upstream request after oauth access reload"
                            );
                            continue;
                        }
                    }
                    last_fail = Some((class, status, retry_after, detail));
                    state.isolate_authorization(&member);
                    exclude_member(&mut excluded, &member);
                    break;
                }
                FailoverDecision::ExcludeMemberModel => {
                    last_fail = Some((class, status, retry_after, detail));
                    state.deny_member_model(&member.source_id, public_model);
                    exclude_member(&mut excluded, &member);
                    break;
                }
                FailoverDecision::CooldownAndFailover => {
                    let duration = cooldown_from_retry_after(retry_after.as_ref());
                    let model = match class {
                        UpstreamErrorClass::QuotaModel => Some(public_model),
                        _ => None,
                    };
                    state
                        .account_picker
                        .set_cooldown(&member.source_id, model, duration);
                    last_fail = Some((class, status, retry_after, detail));
                    exclude_member(&mut excluded, &member);
                    break;
                }
                FailoverDecision::FailoverIfUncommitted => {
                    last_fail = Some((class, status, retry_after, detail));
                    exclude_member(&mut excluded, &member);
                    break;
                }
            }
        }

        if continuation_locked {
            if let Some((class, status, retry_after, detail)) = last_fail {
                return Err(map_request_or_upstream(
                    state,
                    request_id,
                    started,
                    class,
                    status,
                    retry_after,
                    detail.as_deref(),
                    &member,
                    failover_from.as_deref(),
                ));
            }
            return Err(exhausted(
                state,
                request_id,
                started,
                public_model,
                failover_from.as_deref(),
            ));
        }
        let Some(next) = state.pick_v2_in_lane(
            candidates,
            public_model,
            &excluded,
            Some(member.source_id.as_str()),
            affinity_key,
        ) else {
            return Err(exhausted_or_last_fail(
                state,
                request_id,
                started,
                public_model,
                failover_from.as_deref(),
                &member,
                last_fail.as_ref(),
            ));
        };
        tracing::info!(
            target: "core.adapter",
            profile_id = %state.profile_id,
            request_id = %request_id,
            account_id = %next.source_id,
            failover = true,
            failover_from = %member.source_id,
            "switching upstream account before first stream event"
        );
        if failover_from.is_none() {
            failover_from = Some(member.source_id.clone());
        }
        member = next;
        grok_strip_attempt = 0;
    }
}

fn candidate_for_member<'a>(
    candidates: &'a [DispatchCandidate],
    member: &PickedMember,
) -> Option<&'a DispatchCandidate> {
    candidates.iter().find(|candidate| {
        candidate.member_id == member.source_id
            || candidate.member_id == member.ticket_id
            || candidate.member_id == member.label
            || member
                .ticket_id
                .ends_with(&format!(":{}", candidate.member_id))
            || member
                .source_id
                .ends_with(&format!(":{}", candidate.member_id))
    })
}

fn prepare_candidate_attempt(
    state: &EdgeState,
    surface: DownstreamSurface,
    headers: &HeaderMap,
    original_body: &Value,
    request_id: &str,
    started: Instant,
    member: &PickedMember,
    candidate: Option<&DispatchCandidate>,
    public_model: &str,
) -> Result<(UpstreamChannel, reqwest::Url, UpstreamPrepare), Response> {
    let lead = UpstreamChannel::from_protocol(state.upstream.protocol);
    let channel = candidate
        .and_then(|candidate| channel_from_transport_key(&candidate.transport_key))
        .unwrap_or(lead);
    let mut attempt_state = state.clone();
    attempt_state.upstream.model = candidate_upstream_model(candidate, public_model);
    let permit = Arc::new(Semaphore::new(1))
        .try_acquire_owned()
        .expect("fresh semaphore has a permit");
    let admitted = AdmittedRequest {
        state: attempt_state,
        request_id: request_id.to_owned(),
        started,
        permit,
        headers: headers.clone(),
        body: original_body.clone(),
        member: Some(member.clone()),
        affinity_key: None,
    };
    let prepared = channel.transport().prepare(surface, &admitted)?;
    let url = attempt_url(state, prepared.path, candidate)?;
    Ok((channel, url, prepared))
}

fn candidate_upstream_model(
    candidate: Option<&DispatchCandidate>,
    public_model: &str,
) -> Option<String> {
    let from_candidate = candidate
        .map(|candidate| candidate.upstream_model.trim())
        .filter(|model| !model.is_empty())
        .map(str::to_owned);
    if from_candidate.is_some() {
        return from_candidate;
    }
    let public = public_model.trim();
    if public.is_empty() {
        None
    } else {
        Some(public.to_owned())
    }
}

fn attempt_url(
    state: &EdgeState,
    path: &str,
    candidate: Option<&DispatchCandidate>,
) -> Result<reqwest::Url, Response> {
    if let Some(endpoint) = candidate
        .map(|candidate| candidate.upstream_endpoint.trim())
        .filter(|endpoint| !endpoint.is_empty())
    {
        return join_candidate_endpoint(state, endpoint, path);
    }
    join_upstream(state, path)
}

/// Same trailing-slash rule as `validate_start_spec`: `Url::join` treats a
/// last segment without `/` as a file, so `https://api.example/v1` + `responses`
/// would drop `/v1`.
fn join_candidate_endpoint(
    state: &EdgeState,
    endpoint: &str,
    path: &str,
) -> Result<reqwest::Url, Response> {
    let Ok(mut base) = reqwest::Url::parse(endpoint) else {
        return candidate_url_error(state);
    };
    if !base.path().ends_with('/') {
        let with_slash = format!("{}/", base.path());
        base.set_path(&with_slash);
    }
    match base.join(path) {
        Ok(url) => Ok(url),
        Err(_) => candidate_url_error(state),
    }
}

fn candidate_url_error(state: &EdgeState) -> Result<reqwest::Url, Response> {
    state.record_upstream_failure();
    Err(error_response(
        StatusCode::BAD_GATEWAY,
        "upstream_error",
        "The upstream model provider is unavailable.",
        None,
    ))
}

fn channel_from_transport_key(key: &str) -> Option<UpstreamChannel> {
    match key.trim() {
        "openai:generic" => Some(UpstreamChannel::OpenAiChat),
        "anthropic:claude" => Some(UpstreamChannel::Anthropic),
        "codex:codex" => Some(UpstreamChannel::CodexResponses),
        "grok:grok" => Some(UpstreamChannel::Grok),
        _ => None,
    }
}

fn exclude_member(excluded: &mut Vec<String>, member: &PickedMember) {
    if !excluded.iter().any(|id| id == &member.source_id) {
        excluded.push(member.source_id.clone());
    }
    if !member.ticket_id.is_empty() && !excluded.iter().any(|id| id == &member.ticket_id) {
        excluded.push(member.ticket_id.clone());
    }
}

fn exhausted_or_last_fail(
    state: &EdgeState,
    request_id: &str,
    started: Instant,
    public_model: &str,
    failover_from: Option<&str>,
    member: &PickedMember,
    last_fail: Option<&(
        UpstreamErrorClass,
        StatusCode,
        Option<HeaderValue>,
        Option<String>,
    )>,
) -> Response {
    if let Some((class, status, retry_after, detail)) = last_fail {
        if matches!(
            class,
            UpstreamErrorClass::Entitlement | UpstreamErrorClass::Request
        ) {
            return map_request_or_upstream(
                state,
                request_id,
                started,
                *class,
                *status,
                retry_after.clone(),
                detail.as_deref(),
                member,
                failover_from,
            );
        }
    }
    exhausted(state, request_id, started, public_model, failover_from)
}

fn exhausted(
    state: &EdgeState,
    request_id: &str,
    started: Instant,
    public_model: &str,
    failover_from: Option<&str>,
) -> Response {
    let retry_after = state.account_picker.soonest_retry_after(public_model);
    tracing::warn!(
        target: "core.adapter",
        profile_id = %state.profile_id,
        request_id = %request_id,
        failover = failover_from.is_some(),
        failover_from = failover_from.unwrap_or(""),
        op = "upstream",
        code = "pool_exhausted",
        status = 503_u16,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "v2 route pool has no healthy candidate"
    );
    state.record_upstream_failure();
    pool_exhausted_response(retry_after)
}

fn map_request_or_upstream(
    state: &EdgeState,
    request_id: &str,
    started: Instant,
    class: UpstreamErrorClass,
    status: StatusCode,
    retry_after: Option<HeaderValue>,
    detail: Option<&str>,
    member: &PickedMember,
    failover_from: Option<&str>,
) -> Response {
    if class == UpstreamErrorClass::Request {
        tracing::warn!(
            target: "core.adapter",
            profile_id = %state.profile_id,
            request_id = %request_id,
            account_id = %member.source_id,
            ticket_id = %member.ticket_id,
            failover = failover_from.is_some(),
            failover_from = failover_from.unwrap_or(""),
            op = "upstream",
            code = "invalid_request",
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            upstream_detail = detail.unwrap_or(""),
            "bridge upstream rejected the request"
        );
        state.record_upstream_failure();
        return map_v2_request_error(status, retry_after);
    }
    map_upstream_http_error(
        state,
        request_id,
        started,
        status,
        retry_after,
        detail,
        Some(member),
        failover_from,
    )
}
