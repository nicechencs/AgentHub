//! Upstream channel: path/body reshape, auth inject, and recovery policy.
//!
//! Dispatch matches downstream surface after this module has already chosen
//! the upstream request. Identity relay is declared explicitly by both the
//! upstream channel and downstream surface; all other pairs use the neutral IR.

mod anthropic;
mod codex;
mod failover;
mod grok;
mod openai_chat;

use std::time::Instant;

use axum::http::{header, StatusCode};
use axum::response::Response;
use serde_json::Value;

use crate::bridge::account::PickedMember;
use crate::bridge::grok_cli::{
    grok_session_id_for_account, is_reasoning_decode_failure, strip_encrypted_reasoning,
    GrokCliRequestIdentity,
};
use crate::bridge::request_fsm::{RequestDecision, RequestFsm, SwitchClass};
use crate::bridge::runtime::BridgeUpstreamProtocol;

use super::admission::AdmittedRequest;
use super::http::{
    error_response, log_protocol_error, protocol_error_response, stopping_response, EdgeState,
};
use super::stream::UpstreamBodyError;
use super::surface::DownstreamSurface;
use super::upstream::{
    access_jwt_near_expiry, apply_grok_replay, extract_upstream_error_detail, grok_replay_model,
    map_upstream_http_error, post_upstream, read_bounded_upstream_error, replay_session,
    try_reload_member_auth,
};

use anthropic::AnthropicTransport;
use codex::CodexTransport;
use grok::GrokTransport;
use openai_chat::OpenAiChatTransport;

#[cfg(test)]
mod tests;

pub(super) struct UpstreamPrepare {
    pub path: &'static str,
    pub body: Value,
    pub grok_identity: Option<GrokCliRequestIdentity>,
    pub cache_seed: Option<String>,
    pub stream: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UpstreamDecode {
    ChatCompletions,
    AnthropicMessages,
    OpenAiResponses,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RecoveryPolicy {
    /// API key upstreams: no 401 reload, no grok strip.
    None,
    /// Codex OAuth: RetryGate 401 reload at most once before first event.
    Oauth401Reload,
    /// Grok OAuth: 401 reload plus encrypted-reasoning 400 strip retry.
    Oauth401ReloadAndGrokReasoning,
}

impl RecoveryPolicy {
    fn reloads_on_401(self) -> bool {
        matches!(
            self,
            Self::Oauth401Reload | Self::Oauth401ReloadAndGrokReasoning
        )
    }

    fn strips_grok_reasoning(self) -> bool {
        matches!(self, Self::Oauth401ReloadAndGrokReasoning)
    }
}

/// Sync per-channel policy owned by the four `*Transport` types.
/// [`UpstreamChannel`] is protocol identity; resolve once with [`UpstreamChannel::transport`].
pub(super) trait UpstreamTransport: Send + Sync {
    fn path(&self) -> &'static str;

    fn apply_auth(
        &self,
        builder: reqwest::RequestBuilder,
        token: &str,
        grok_identity: Option<&GrokCliRequestIdentity>,
    ) -> reqwest::RequestBuilder;

    fn prepare(
        &self,
        surface: DownstreamSurface,
        admitted: &AdmittedRequest,
    ) -> Result<UpstreamPrepare, Response>;

    fn decode_kind(&self) -> UpstreamDecode;

    fn recovery(&self) -> RecoveryPolicy;
}

/// Protocol identity 1:1 with [`BridgeUpstreamProtocol`].
/// Transport implementation is [`Self::transport`], not a trait impl on this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UpstreamChannel {
    OpenAiChat,
    Anthropic,
    CodexResponses,
    Grok,
}

impl UpstreamChannel {
    pub(super) fn from_protocol(protocol: BridgeUpstreamProtocol) -> Self {
        match protocol {
            BridgeUpstreamProtocol::OpenAiChatCompletions => Self::OpenAiChat,
            BridgeUpstreamProtocol::AnthropicMessages => Self::Anthropic,
            BridgeUpstreamProtocol::CodexResponsesOauth => Self::CodexResponses,
            BridgeUpstreamProtocol::XaiResponsesOauth => Self::Grok,
        }
    }

    /// Inverse of [`Self::from_protocol`]. Tests lock the 1:1 table.
    #[allow(dead_code)]
    pub(super) fn protocol(self) -> BridgeUpstreamProtocol {
        match self {
            Self::OpenAiChat => BridgeUpstreamProtocol::OpenAiChatCompletions,
            Self::Anthropic => BridgeUpstreamProtocol::AnthropicMessages,
            Self::CodexResponses => BridgeUpstreamProtocol::CodexResponsesOauth,
            Self::Grok => BridgeUpstreamProtocol::XaiResponsesOauth,
        }
    }

    /// Stable name for the gateway usage spool / `gateway_usage` table.
    #[allow(dead_code)]
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::OpenAiChat => "openai_chat",
            Self::Anthropic => "anthropic",
            Self::CodexResponses => "codex_responses",
            Self::Grok => "grok",
        }
    }

    /// Official ChatGPT / Codex Responses requires `stream: true`.
    /// Grok and other channels follow the downstream request.
    pub(super) fn forces_upstream_stream(self) -> bool {
        matches!(self, Self::CodexResponses)
    }

    /// Resolve the transport implementation once. Callers must not match on
    /// this enum for path / auth / prepare / decode / recovery.
    pub(super) fn transport(self) -> &'static dyn UpstreamTransport {
        match self {
            Self::OpenAiChat => &OpenAiChatTransport,
            Self::Anthropic => &AnthropicTransport,
            Self::CodexResponses => &CodexTransport,
            Self::Grok => &GrokTransport,
        }
    }

    /// Whether the upstream wire protocol is identical to the requested
    /// downstream surface. Keep this surface-aware: a Responses upstream must
    /// never be accidentally relayed to a Messages or Chat client.
    ///
    /// Codex and Grok both speak Responses; that match alone is not
    /// transparent. Cross-product pair adapters consult feature flags via
    /// [`super::pair_policy::identity_relay`].
    pub(super) fn passthrough_for(self, surface: DownstreamSurface) -> bool {
        matches!(
            (self, surface),
            (Self::OpenAiChat, DownstreamSurface::ChatCompletions)
                | (Self::Anthropic, DownstreamSurface::Messages)
                | (
                    Self::CodexResponses | Self::Grok,
                    DownstreamSurface::Responses
                )
        )
    }
}

pub(super) struct UpstreamSendOutcome {
    pub response: reqwest::Response,
    pub member: PickedMember,
    pub channel: UpstreamChannel,
    pub cache_seed: Option<String>,
    pub stream: bool,
}

pub(super) use failover::send_upstream_v2;

pub(super) async fn send_upstream(
    state: &EdgeState,
    url: reqwest::Url,
    channel: UpstreamChannel,
    request_id: &str,
    started: Instant,
    identity: Option<GrokCliRequestIdentity>,
    body: Value,
    cache_seed: Option<&str>,
    member: PickedMember,
    continuation_locked: bool,
) -> Result<UpstreamSendOutcome, Response> {
    let transport = channel.transport();
    let recovery = transport.recovery();
    let original_body = body;
    let original_identity = identity;
    let mut member = member;
    let mut failover_from: Option<String> = None;
    let mut fsm = RequestFsm::new(state.account_picker.multi_account());
    let mut grok_strip_attempt = 0u8;

    loop {
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
        // Same-account near-expiry preload. A no-op must still allow one 401 retry.
        if recovery.reloads_on_401()
            && !fsm.retry_used()
            && access_jwt_near_expiry(&member.auth.token())
            && try_reload_member_auth(&member)
        {
            fsm.record_retry();
        }

        loop {
            let token = member.auth.token();
            let builder = transport.apply_auth(
                state.client.post(url.clone()).json(&body),
                &token,
                identity.as_ref(),
            );
            let response = post_upstream(state, builder, request_id).await?;
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
                    channel,
                    cache_seed: None,
                    stream: false,
                });
            }
            let status = response.status();
            let retry_after = response.headers().get(header::RETRY_AFTER).cloned();
            if status == StatusCode::UNAUTHORIZED {
                match switch_or_reload(
                    state,
                    request_id,
                    &mut fsm,
                    recovery.reloads_on_401(),
                    &mut member,
                    &mut failover_from,
                    &mut grok_strip_attempt,
                ) {
                    AuthFollowup::Reload => continue,
                    AuthFollowup::Switch if continuation_locked => {
                        let detail = read_error_detail(response, &state.force_shutdown).await?;
                        return Err(map_upstream_http_error(
                            state,
                            request_id,
                            started,
                            status,
                            retry_after,
                            detail.as_deref(),
                            Some(&member),
                            failover_from.as_deref(),
                        ));
                    }
                    AuthFollowup::Switch => break,
                    AuthFollowup::Fail => {
                        let detail = read_error_detail(response, &state.force_shutdown).await?;
                        return Err(map_upstream_http_error(
                            state,
                            request_id,
                            started,
                            status,
                            retry_after,
                            detail.as_deref(),
                            Some(&member),
                            failover_from.as_deref(),
                        ));
                    }
                }
            }

            let can_recover = recovery.strips_grok_reasoning()
                && status == StatusCode::BAD_REQUEST
                && grok_strip_attempt < 2;
            let error_body =
                match read_bounded_upstream_error(response, &state.force_shutdown).await {
                    Ok(body) => body,
                    Err(UpstreamBodyError::Stopping) => return Err(stopping_response()),
                    Err(UpstreamBodyError::InvalidOrTooLarge | UpstreamBodyError::IncompleteStream) => {
                        Vec::new()
                    }
                };
            if !can_recover {
                let detail = extract_upstream_error_detail(&error_body);
                return Err(map_upstream_http_error(
                    state,
                    request_id,
                    started,
                    status,
                    retry_after,
                    detail.as_deref(),
                    Some(&member),
                    failover_from.as_deref(),
                ));
            }
            let err_text = String::from_utf8_lossy(&error_body);
            if !is_reasoning_decode_failure(&err_text) {
                let detail = extract_upstream_error_detail(&error_body);
                return Err(map_upstream_http_error(
                    state,
                    request_id,
                    started,
                    status,
                    retry_after,
                    detail.as_deref(),
                    Some(&member),
                    failover_from.as_deref(),
                ));
            }
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
        }
    }
}

enum AuthFollowup {
    Reload,
    Switch,
    Fail,
}

/// Reload stays on this member (inner loop). Switch restarts the outer loop
/// so identity/body are rebuilt for the next account.
fn switch_or_reload(
    state: &EdgeState,
    request_id: &str,
    fsm: &mut RequestFsm,
    oauth_401: bool,
    member: &mut PickedMember,
    failover_from: &mut Option<String>,
    grok_strip_attempt: &mut u8,
) -> AuthFollowup {
    let has_failover = state.account_picker.failover(&member.source_id).is_some();
    match fsm.on_failure(oauth_401, SwitchClass::AccountFailure, has_failover) {
        RequestDecision::ReloadSameAccount => {
            fsm.record_retry();
            if try_reload_member_auth(member) {
                tracing::info!(
                    target: "core.adapter",
                    profile_id = %state.profile_id,
                    request_id = %request_id,
                    account_id = %member.source_id,
                    "retrying upstream request after oauth access reload"
                );
                return AuthFollowup::Reload;
            }
            let has_failover = state.account_picker.failover(&member.source_id).is_some();
            if fsm.on_failure(false, SwitchClass::AccountFailure, has_failover)
                == RequestDecision::SwitchAccount
            {
                return take_switch(
                    state,
                    request_id,
                    fsm,
                    member,
                    failover_from,
                    grok_strip_attempt,
                );
            }
            AuthFollowup::Fail
        }
        RequestDecision::SwitchAccount => take_switch(
            state,
            request_id,
            fsm,
            member,
            failover_from,
            grok_strip_attempt,
        ),
        RequestDecision::Fail => AuthFollowup::Fail,
    }
}

fn take_switch(
    state: &EdgeState,
    request_id: &str,
    fsm: &mut RequestFsm,
    member: &mut PickedMember,
    failover_from: &mut Option<String>,
    grok_strip_attempt: &mut u8,
) -> AuthFollowup {
    state.account_picker.isolate(&member.source_id);
    let Some(next) = state.account_picker.failover(&member.source_id) else {
        return AuthFollowup::Fail;
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
        *failover_from = Some(member.source_id.clone());
    }
    *member = next;
    fsm.record_switch();
    *grok_strip_attempt = 0;
    AuthFollowup::Switch
}

fn identity_for_member(
    base: &Option<GrokCliRequestIdentity>,
    cache_seed: Option<&str>,
    account_id: Option<&str>,
) -> Option<GrokCliRequestIdentity> {
    let mut identity = base.clone()?;
    identity.session_id = cache_seed.and_then(|seed| grok_session_id_for_account(seed, account_id));
    Some(identity)
}

fn log_serving_account(
    state: &EdgeState,
    request_id: &str,
    member: &PickedMember,
    failover: bool,
    failover_from: Option<&str>,
) {
    tracing::debug!(
        target: "core.adapter",
        profile_id = %state.profile_id,
        request_id = %request_id,
        account_id = %member.source_id,
        ticket_id = %member.ticket_id,
        failover,
        failover_from = failover_from.unwrap_or(""),
        "bridge upstream accepted"
    );
}

async fn read_error_detail(
    response: reqwest::Response,
    force_shutdown: &tokio_util::sync::CancellationToken,
) -> Result<Option<String>, Response> {
    let error_body = match read_bounded_upstream_error(response, force_shutdown).await {
        Ok(body) => body,
        Err(UpstreamBodyError::Stopping) => return Err(stopping_response()),
        Err(UpstreamBodyError::InvalidOrTooLarge | UpstreamBodyError::IncompleteStream) => {
            Vec::new()
        }
    };
    Ok(extract_upstream_error_detail(&error_body))
}

fn parse_bridge_request(
    surface: DownstreamSurface,
    admitted: &AdmittedRequest,
) -> Result<crate::bridge::types::BridgeRequest, Response> {
    match surface.parse_request(&admitted.body) {
        Ok(request) => Ok(request),
        Err(error) => {
            log_protocol_error(
                &admitted.state,
                &admitted.request_id,
                admitted.started,
                &error,
            );
            Err(protocol_error_response(error))
        }
    }
}

/// Validate a JSON request object for an identity relay and extract `stream`.
/// The caller still applies the configured model policy and any provider-
/// specific safety normalization after this generic validation.
pub(super) fn passthrough_responses_object(body: Value) -> Result<(Value, bool), Response> {
    if !body.is_object() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "The request body must be valid JSON.",
            None,
        ));
    }
    let stream_requested = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    Ok((body, stream_requested))
}

fn overwrite_configured_model(body: &mut Value, model: Option<&str>, listed: &[String]) {
    overwrite_configured_model_with(body, model, false, listed);
}

fn overwrite_configured_model_with(
    body: &mut Value,
    model: Option<&str>,
    keep_request_model: bool,
    listed: &[String],
) {
    let request_model = body
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if !request_model.is_empty() {
        if listed
            .iter()
            .any(|item| item.eq_ignore_ascii_case(request_model))
        {
            return;
        }
        if keep_request_model {
            return;
        }
    }
    if let Some(model) = model.filter(|value| !value.trim().is_empty()) {
        body["model"] = Value::String(model.to_owned());
    }
}

fn models_surface_unreachable() -> ! {
    unreachable!("models are synthesized by list_models, not conversation prepare")
}
