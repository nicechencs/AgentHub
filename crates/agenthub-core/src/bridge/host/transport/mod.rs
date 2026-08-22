//! Upstream channel: path/body reshape, auth inject, and recovery policy.
//!
//! Dispatch matches downstream surface (and Responses passthrough) after this
//! module has already chosen the upstream request. Async send stays on the
//! enum so it does not need `async-trait` or boxing.

mod anthropic;
mod codex;
mod grok;
mod openai_chat;

use std::time::Instant;

use axum::http::{header, StatusCode};
use axum::response::Response;
use serde_json::Value;

use crate::bridge::grok_cli::{
    is_reasoning_decode_failure, strip_encrypted_reasoning, GrokCliRequestIdentity,
};
use crate::bridge::runtime::BridgeUpstreamProtocol;
use crate::bridge::types::{EmissionState, RetryClass, RetryGate};

use super::admission::AdmittedRequest;
use super::http::{
    error_response, log_protocol_error, protocol_error_response, stopping_response, ListenerState,
};
use super::stream::UpstreamBodyError;
use super::surface::DownstreamSurface;
use super::upstream::{
    access_jwt_near_expiry, apply_grok_replay, extract_upstream_error_detail, grok_replay_model,
    map_upstream_http_error, post_upstream, read_bounded_upstream_error, try_reload_upstream_auth,
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

/// Sync per-channel policy. Async send lives on [`UpstreamChannel`] instead.
pub(super) trait UpstreamTransport {
    fn passthrough(&self) -> bool {
        false
    }

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

    pub(super) fn protocol(self) -> BridgeUpstreamProtocol {
        match self {
            Self::OpenAiChat => BridgeUpstreamProtocol::OpenAiChatCompletions,
            Self::Anthropic => BridgeUpstreamProtocol::AnthropicMessages,
            Self::CodexResponses => BridgeUpstreamProtocol::CodexResponsesOauth,
            Self::Grok => BridgeUpstreamProtocol::XaiResponsesOauth,
        }
    }

    pub(super) fn apply_auth(
        self,
        builder: reqwest::RequestBuilder,
        token: &str,
        grok_identity: Option<&GrokCliRequestIdentity>,
    ) -> reqwest::RequestBuilder {
        UpstreamTransport::apply_auth(&self, builder, token, grok_identity)
    }

    pub(super) fn prepare(
        self,
        surface: DownstreamSurface,
        admitted: &AdmittedRequest,
    ) -> Result<UpstreamPrepare, Response> {
        UpstreamTransport::prepare(&self, surface, admitted)
    }

    pub(super) fn decode_kind(self) -> UpstreamDecode {
        UpstreamTransport::decode_kind(&self)
    }

    pub(super) fn recovery(self) -> RecoveryPolicy {
        UpstreamTransport::recovery(&self)
    }

    pub(super) fn path(self) -> &'static str {
        UpstreamTransport::path(&self)
    }

    pub(super) fn passthrough(self) -> bool {
        UpstreamTransport::passthrough(&self)
    }
}

impl UpstreamTransport for UpstreamChannel {
    fn passthrough(&self) -> bool {
        match self {
            Self::OpenAiChat => OpenAiChatTransport.passthrough(),
            Self::Anthropic => AnthropicTransport.passthrough(),
            Self::CodexResponses => CodexTransport.passthrough(),
            Self::Grok => GrokTransport.passthrough(),
        }
    }

    fn path(&self) -> &'static str {
        match self {
            Self::OpenAiChat => OpenAiChatTransport.path(),
            Self::Anthropic => AnthropicTransport.path(),
            Self::CodexResponses => CodexTransport.path(),
            Self::Grok => GrokTransport.path(),
        }
    }

    fn apply_auth(
        &self,
        builder: reqwest::RequestBuilder,
        token: &str,
        grok_identity: Option<&GrokCliRequestIdentity>,
    ) -> reqwest::RequestBuilder {
        match self {
            Self::OpenAiChat => OpenAiChatTransport.apply_auth(builder, token, grok_identity),
            Self::Anthropic => AnthropicTransport.apply_auth(builder, token, grok_identity),
            Self::CodexResponses => CodexTransport.apply_auth(builder, token, grok_identity),
            Self::Grok => GrokTransport.apply_auth(builder, token, grok_identity),
        }
    }

    fn prepare(
        &self,
        surface: DownstreamSurface,
        admitted: &AdmittedRequest,
    ) -> Result<UpstreamPrepare, Response> {
        match self {
            Self::OpenAiChat => OpenAiChatTransport.prepare(surface, admitted),
            Self::Anthropic => AnthropicTransport.prepare(surface, admitted),
            Self::CodexResponses => CodexTransport.prepare(surface, admitted),
            Self::Grok => GrokTransport.prepare(surface, admitted),
        }
    }

    fn decode_kind(&self) -> UpstreamDecode {
        match self {
            Self::OpenAiChat => OpenAiChatTransport.decode_kind(),
            Self::Anthropic => AnthropicTransport.decode_kind(),
            Self::CodexResponses => CodexTransport.decode_kind(),
            Self::Grok => GrokTransport.decode_kind(),
        }
    }

    fn recovery(&self) -> RecoveryPolicy {
        match self {
            Self::OpenAiChat => OpenAiChatTransport.recovery(),
            Self::Anthropic => AnthropicTransport.recovery(),
            Self::CodexResponses => CodexTransport.recovery(),
            Self::Grok => GrokTransport.recovery(),
        }
    }
}

pub(super) async fn send_upstream(
    state: &ListenerState,
    url: reqwest::Url,
    channel: UpstreamChannel,
    request_id: &str,
    started: Instant,
    mut identity: Option<GrokCliRequestIdentity>,
    mut body: Value,
    cache_seed: Option<&str>,
) -> Result<reqwest::Response, Response> {
    let recovery = channel.recovery();
    if recovery.strips_grok_reasoning() {
        apply_grok_replay(state, &mut body, cache_seed);
    }
    let retry_gate = RetryGate::default();
    let mut auth_reloaded = false;
    // Only consume the 401 retry slot when a follow/refresh actually swapped
    // the in-memory bearer. A no-op near-expiry reread must still allow one 401 retry.
    if recovery.reloads_on_401() && access_jwt_near_expiry(&state.upstream.auth.token()) {
        auth_reloaded = try_reload_upstream_auth(state);
    }
    let mut attempt = 0u8;
    loop {
        let token = state.upstream.auth.token();
        let builder = channel.apply_auth(
            state.client.post(url.clone()).json(&body),
            &token,
            identity.as_ref(),
        );
        let response = post_upstream(state, builder).await?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let retry_after = response.headers().get(header::RETRY_AFTER).cloned();
        let auth_attempts = if auth_reloaded { 1 } else { 0 };
        if status == StatusCode::UNAUTHORIZED
            && recovery.reloads_on_401()
            && retry_gate.can_retry(EmissionState::Idle, RetryClass::Transient, auth_attempts)
            && try_reload_upstream_auth(state)
        {
            auth_reloaded = true;
            tracing::info!(
                target: "core.adapter",
                profile_id = %state.profile_id,
                request_id = %request_id,
                "retrying upstream request after oauth access reload"
            );
            continue;
        }
        let can_recover =
            recovery.strips_grok_reasoning() && status == StatusCode::BAD_REQUEST && attempt < 2;
        let error_body = match read_bounded_upstream_error(response, &state.force_shutdown).await {
            Ok(body) => body,
            Err(UpstreamBodyError::Stopping) => return Err(stopping_response()),
            Err(UpstreamBodyError::InvalidOrTooLarge) => Vec::new(),
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
            ));
        }
        let model = grok_replay_model(&body, state.upstream.model.as_deref());
        state.grok_replay.clear(&model, cache_seed);
        strip_encrypted_reasoning(&mut body);
        attempt += 1;
        if attempt >= 2 {
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
            attempt,
            "retrying Grok request after encrypted reasoning rejection"
        );
    }
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

fn passthrough_responses_object(body: Value) -> Result<(Value, bool), Response> {
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

fn overwrite_configured_model(body: &mut Value, model: Option<&str>) {
    if let Some(model) = model {
        body["model"] = Value::String(model.to_owned());
    }
}

fn models_surface_unreachable() -> ! {
    unreachable!("models are synthesized by list_models, not conversation prepare")
}
