use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::http::HeaderMap;
use reqwest::header::AUTHORIZATION;
use serde_json::json;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::bridge::grok_cli::{is_reasoning_decode_failure, GrokReasoningReplay};
use crate::bridge::runtime::{
    BridgeLocalSurface, BridgeUpstreamConfig, BridgeUpstreamProtocol, BridgeUpstreamStatus,
    ResolvedAuth,
};

use super::super::admission::AdmittedRequest;
use super::super::http::ListenerState;
use super::super::surface::DownstreamSurface;
use super::super::ANTHROPIC_API_VERSION;
use super::{RecoveryPolicy, UpstreamChannel};

fn listener_state(
    protocol: BridgeUpstreamProtocol,
    local_surface: BridgeLocalSurface,
) -> ListenerState {
    ListenerState {
        profile_id: Arc::from("transport-test"),
        local_token: Arc::from("local-dummy"),
        upstream: BridgeUpstreamConfig {
            base_url: "http://127.0.0.1/v1/".to_owned(),
            model: Some("configured-model".to_owned()),
            source_connection_id: None,
            auth: ResolvedAuth::bearer("upstream-dummy-token"),
            protocol,
            local_surface,
        },
        upstream_url: reqwest::Url::parse("http://127.0.0.1/v1/").expect("test url"),
        client: reqwest::Client::new(),
        force_shutdown: CancellationToken::new(),
        admission: Arc::new(Semaphore::new(1)),
        observed_upstream: Arc::new(Mutex::new(BridgeUpstreamStatus::Unknown)),
        grok_replay: Arc::new(GrokReasoningReplay::new()),
        listed_models: Arc::from(Vec::<String>::new()),
        reload_upstream_auth: None,
    }
}

fn admitted(
    protocol: BridgeUpstreamProtocol,
    local_surface: BridgeLocalSurface,
    body: serde_json::Value,
) -> AdmittedRequest {
    let state = listener_state(protocol, local_surface);
    let permit = state
        .admission
        .clone()
        .try_acquire_owned()
        .expect("test permit");
    AdmittedRequest {
        state,
        request_id: "req-transport-test".to_owned(),
        started: Instant::now(),
        permit,
        headers: HeaderMap::new(),
        body,
    }
}

fn prepare_responses(protocol: BridgeUpstreamProtocol) -> super::UpstreamPrepare {
    let body = json!({ "model": "m", "input": "hi" });
    let admitted = admitted(protocol, BridgeLocalSurface::Responses, body);
    UpstreamChannel::from_protocol(protocol)
        .prepare(DownstreamSurface::Responses, &admitted)
        .expect("prepare responses")
}

#[test]
fn prepare_selects_upstream_path_by_channel() {
    assert_eq!(
        prepare_responses(BridgeUpstreamProtocol::OpenAiChatCompletions).path,
        "chat/completions"
    );
    assert_eq!(
        prepare_responses(BridgeUpstreamProtocol::AnthropicMessages).path,
        "messages"
    );
    assert_eq!(
        prepare_responses(BridgeUpstreamProtocol::CodexResponsesOauth).path,
        "responses"
    );
    assert_eq!(
        prepare_responses(BridgeUpstreamProtocol::XaiResponsesOauth).path,
        "responses"
    );
    assert_eq!(UpstreamChannel::OpenAiChat.path(), "chat/completions");
    assert_eq!(UpstreamChannel::Anthropic.path(), "messages");
    assert_eq!(UpstreamChannel::CodexResponses.path(), "responses");
    assert_eq!(UpstreamChannel::Grok.path(), "responses");
}

#[test]
fn passthrough_is_declared_only_for_responses_oauth_channels() {
    assert_eq!(
        [
            UpstreamChannel::OpenAiChat.passthrough(),
            UpstreamChannel::Anthropic.passthrough(),
            UpstreamChannel::CodexResponses.passthrough(),
            UpstreamChannel::Grok.passthrough(),
        ],
        [false, false, true, true]
    );
}

#[test]
fn openai_chat_prepare_does_not_invent_grok_identity() {
    let prepared = prepare_responses(BridgeUpstreamProtocol::OpenAiChatCompletions);
    assert!(prepared.grok_identity.is_none());
    assert!(prepared.cache_seed.is_none());
}

#[test]
fn apply_auth_injects_openai_bearer() {
    let request = UpstreamChannel::OpenAiChat
        .apply_auth(
            reqwest::Client::new().post("http://127.0.0.1/v1/x"),
            "openai-auth-token-a2",
            None,
        )
        .build()
        .expect("openai auth request");
    let headers = request.headers();
    assert_eq!(
        headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer openai-auth-token-a2")
    );
    assert!(headers.get("x-api-key").is_none());
}

#[test]
fn apply_auth_injects_anthropic_headers_without_bearer() {
    let request = UpstreamChannel::Anthropic
        .apply_auth(
            reqwest::Client::new().post("http://127.0.0.1/v1/x"),
            "anthropic-auth-token-a2",
            None,
        )
        .build()
        .expect("anthropic auth request");
    let headers = request.headers();
    assert_eq!(
        headers
            .get("x-api-key")
            .and_then(|value| value.to_str().ok()),
        Some("anthropic-auth-token-a2")
    );
    assert_eq!(
        headers
            .get("anthropic-version")
            .and_then(|value| value.to_str().ok()),
        Some(ANTHROPIC_API_VERSION)
    );
    assert!(headers.get(AUTHORIZATION).is_none());
}

#[test]
fn apply_auth_injects_grok_bearer_and_identity_headers() {
    let request = UpstreamChannel::Grok
        .apply_auth(
            reqwest::Client::new().post("http://127.0.0.1/v1/x"),
            "grok-auth-token-a2",
            None,
        )
        .build()
        .expect("grok auth request");
    let headers = request.headers();
    assert_eq!(
        headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer grok-auth-token-a2")
    );
    assert!(headers.get("x-grok-client-version").is_some());
}

#[test]
fn recovery_policy_matches_channel() {
    assert_eq!(
        UpstreamChannel::Grok.recovery(),
        RecoveryPolicy::Oauth401ReloadAndGrokReasoning
    );
    assert_eq!(
        UpstreamChannel::CodexResponses.recovery(),
        RecoveryPolicy::Oauth401Reload
    );
    assert_eq!(UpstreamChannel::OpenAiChat.recovery(), RecoveryPolicy::None);
    assert_eq!(UpstreamChannel::Anthropic.recovery(), RecoveryPolicy::None);
    assert_eq!(
        UpstreamChannel::from_protocol(BridgeUpstreamProtocol::XaiResponsesOauth),
        UpstreamChannel::Grok
    );
    assert_eq!(
        UpstreamChannel::from_protocol(BridgeUpstreamProtocol::CodexResponsesOauth),
        UpstreamChannel::CodexResponses
    );
    assert_eq!(
        UpstreamChannel::from_protocol(BridgeUpstreamProtocol::OpenAiChatCompletions),
        UpstreamChannel::OpenAiChat
    );
    assert_eq!(
        UpstreamChannel::from_protocol(BridgeUpstreamProtocol::AnthropicMessages),
        UpstreamChannel::Anthropic
    );
    assert_eq!(
        UpstreamChannel::OpenAiChat.protocol(),
        BridgeUpstreamProtocol::OpenAiChatCompletions
    );
    assert_eq!(
        UpstreamChannel::Grok.protocol(),
        BridgeUpstreamProtocol::XaiResponsesOauth
    );
    assert!(is_reasoning_decode_failure(
        r#"{"error":{"message":"could not decrypt the provided encrypted_content"}}"#
    ));
    assert!(!is_reasoning_decode_failure("unrelated 400"));
}
