//! v2 indexed failover: typed errors, candidate exclusion, cooldown, pool_exhausted.
//!
//! Only used when `EdgeState.route_index` is Some. v1 `send_upstream` stays
//! byte-for-byte on edges without an index.

use std::collections::HashSet;
use std::time::Instant;

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use serde_json::Value;

use crate::bridge::account::PickedMember;
use crate::bridge::grok_cli::{
    is_reasoning_decode_failure, strip_encrypted_reasoning, GrokCliRequestIdentity,
};
use crate::bridge::route_index::DispatchCandidate;
use crate::bridge::upstream_class::{
    classify_http, cooldown_from_retry_after, FailoverDecision, UpstreamErrorClass,
};

use super::super::http::{stopping_response, EdgeState};
use super::super::stream::UpstreamBodyError;
use super::super::upstream::{
    apply_grok_replay, extract_upstream_error_detail, grok_replay_model, map_upstream_http_error,
    map_v2_request_error, pool_exhausted_response, post_upstream_attempt,
    read_bounded_upstream_error, replay_session, UpstreamConnectError,
};
use super::{identity_for_member, log_serving_account, UpstreamChannel, UpstreamSendOutcome};

const V2_MAX_ATTEMPTS: usize = 8;

pub(super) async fn send_upstream_v2(
    state: &EdgeState,
    url: reqwest::Url,
    channel: UpstreamChannel,
    request_id: &str,
    started: Instant,
    identity: Option<GrokCliRequestIdentity>,
    body: Value,
    cache_seed: Option<&str>,
    member: PickedMember,
    candidates: &[DispatchCandidate],
    public_model: &str,
) -> Result<UpstreamSendOutcome, Response> {
    let recovery = channel.recovery();
    let original_body = body;
    let original_identity = identity;
    let mut member = member;
    let mut excluded: Vec<String> = Vec::new();
    let mut reloaded: HashSet<String> = HashSet::new();
    let mut failover_from: Option<String> = None;
    let mut grok_strip_attempt = 0u8;
    let max_attempts = candidates.len().clamp(1, V2_MAX_ATTEMPTS);
    let mut attempts = 0usize;

    loop {
        attempts += 1;
        if attempts > max_attempts {
            return Err(exhausted(
                state,
                request_id,
                started,
                public_model,
                failover_from.as_deref(),
            ));
        }
        let fingerprint = member.authorization_fingerprint();
        let account_id = state
            .account_picker
            .partition_account_id(&member)
            .map(str::to_owned);
        let account_id = account_id.as_deref();
        let mut identity = identity_for_member(&original_identity, cache_seed, account_id);
        let mut body = original_body.clone();
        if recovery.strips_grok_reasoning() {
            apply_grok_replay(state, &mut body, cache_seed, account_id);
        }

        tracing::info!(
            target: "core.adapter",
            profile_id = %state.profile_id,
            request_id = %request_id,
            route_id = state.route_index.as_ref().map(|index| index.route_id.as_str()).unwrap_or(""),
            member_id = %member.source_id,
            ?channel,
            model = %public_model,
            attempt = attempts,
            retry_reason = failover_from.as_deref().unwrap_or(""),
            "v2 upstream attempt"
        );

        loop {
            let token = member.auth.token();
            let builder = channel.apply_auth(
                state.client.post(url.clone()).json(&body),
                &token,
                identity.as_ref(),
            );
            let response = match post_upstream_attempt(state, builder, request_id).await {
                Ok(response) => response,
                Err(UpstreamConnectError::Stopping) => return Err(stopping_response()),
                Err(UpstreamConnectError::Timeout | UpstreamConnectError::Unavailable) => {
                    // Transient: failover only because no downstream byte is committed here.
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
                return Ok(UpstreamSendOutcome { response, member });
            }

            let status = response.status();
            let retry_after = response.headers().get(header::RETRY_AFTER).cloned();
            let error_body =
                match read_bounded_upstream_error(response, &state.force_shutdown).await {
                    Ok(body) => body,
                    Err(UpstreamBodyError::Stopping) => return Err(stopping_response()),
                    Err(UpstreamBodyError::InvalidOrTooLarge) => Vec::new(),
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
                    let replay_seed = replay_session(cache_seed, account_id);
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
                    state.isolate_authorization(&member);
                    exclude_member(&mut excluded, &member);
                    break;
                }
                FailoverDecision::ExcludeMemberModel => {
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
                    exclude_member(&mut excluded, &member);
                    break;
                }
                FailoverDecision::FailoverIfUncommitted => {
                    exclude_member(&mut excluded, &member);
                    break;
                }
            }
        }

        let Some(next) = state.pick_v2(candidates, public_model, &excluded) else {
            return Err(exhausted(
                state,
                request_id,
                started,
                public_model,
                failover_from.as_deref(),
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

fn exclude_member(excluded: &mut Vec<String>, member: &PickedMember) {
    if !excluded.iter().any(|id| id == &member.source_id) {
        excluded.push(member.source_id.clone());
    }
    if !member.ticket_id.is_empty() && !excluded.iter().any(|id| id == &member.ticket_id) {
        excluded.push(member.ticket_id.clone());
    }
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
