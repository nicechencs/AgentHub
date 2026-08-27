use std::sync::atomic::AtomicBool;
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
use super::super::http::EdgeState;
use super::super::surface::DownstreamSurface;
use super::super::ANTHROPIC_API_VERSION;
use crate::models::{AdapterSourceProduct, AgentId};

use super::super::pair_policy::{identity_relay, pair_adapter_active, pair_edge_can_apply};
use super::{RecoveryPolicy, UpstreamChannel};

fn listener_state(
    protocol: BridgeUpstreamProtocol,
    local_surface: BridgeLocalSurface,
) -> EdgeState {
    EdgeState {
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
        stopping: Arc::new(AtomicBool::new(false)),
        admission: Arc::new(Semaphore::new(1)),
        observed_upstream: Arc::new(Mutex::new(BridgeUpstreamStatus::Unknown)),
        grok_replay: Arc::new(GrokReasoningReplay::new()),
        listed_models: Arc::from(Vec::<String>::new()),
        reload_upstream_auth: None,
        mapping_source: None,
        mapping_target: None,
        custom_openai: false,
        route_index: None,
        auth_reload: crate::bridge::auth_reload::AuthReloadCoordinator::new(),
        codex_ingress_grok_upstream: false,
        grok_ingress_codex_upstream: false,
        continuations: std::sync::Arc::new(super::super::continuation::ContinuationBindings::new()),
        account_picker: crate::bridge::runtime::BridgeStartSpec::new(
            "transport-test",
            0,
            "local-dummy",
            BridgeUpstreamConfig {
                base_url: "http://127.0.0.1/v1/".to_owned(),
                model: Some("configured-model".to_owned()),
                source_connection_id: None,
                auth: ResolvedAuth::bearer("upstream-dummy-token"),
                protocol,
                local_surface,
            },
        )
        .account_picker(),
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
        member: None,
        affinity_key: None,
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
fn passthrough_is_declared_only_for_matching_wire_surfaces() {
    assert!(!UpstreamChannel::OpenAiChat.passthrough_for(DownstreamSurface::Responses));
    assert!(!UpstreamChannel::Anthropic.passthrough_for(DownstreamSurface::ChatCompletions));
    assert!(UpstreamChannel::OpenAiChat.passthrough_for(DownstreamSurface::ChatCompletions));
    assert!(UpstreamChannel::Anthropic.passthrough_for(DownstreamSurface::Messages));
    assert!(UpstreamChannel::CodexResponses.passthrough_for(DownstreamSurface::Responses));
    assert!(UpstreamChannel::Grok.passthrough_for(DownstreamSurface::Responses));
    assert!(!UpstreamChannel::Grok.passthrough_for(DownstreamSurface::Messages));
}

fn pair_admitted(
    protocol: BridgeUpstreamProtocol,
    source: AdapterSourceProduct,
    target: AgentId,
    codex_to_grok: bool,
    grok_to_codex: bool,
    body: serde_json::Value,
) -> AdmittedRequest {
    let mut request = admitted(protocol, BridgeLocalSurface::Responses, body);
    request.state.mapping_source = Some(source);
    request.state.mapping_target = Some(target);
    request.state.codex_ingress_grok_upstream = codex_to_grok;
    request.state.grok_ingress_codex_upstream = grok_to_codex;
    request.state.listed_models = Arc::from(vec!["grok-4.5".to_owned(), "gpt-5.4".to_owned()]);
    request
}

#[test]
fn flag_off_keeps_responses_identity_relay_for_codex_and_grok() {
    let grok = pair_admitted(
        BridgeUpstreamProtocol::XaiResponsesOauth,
        AdapterSourceProduct::XaiGrokSubscription,
        AgentId::Codex,
        false,
        false,
        json!({ "model": "grok-4.5", "input": "hi" }),
    );
    assert!(identity_relay(
        UpstreamChannel::Grok,
        DownstreamSurface::Responses,
        &grok.state
    ));
    assert!(!pair_adapter_active(&grok.state, UpstreamChannel::Grok));
    let codex = pair_admitted(
        BridgeUpstreamProtocol::CodexResponsesOauth,
        AdapterSourceProduct::CodexChatGptSubscription,
        AgentId::Grok,
        false,
        false,
        json!({ "model": "gpt-5.4", "input": "hi" }),
    );
    assert!(identity_relay(
        UpstreamChannel::CodexResponses,
        DownstreamSurface::Responses,
        &codex.state
    ));
}

#[test]
fn flag_on_disables_implicit_responses_passthrough_for_cross_product() {
    let grok = pair_admitted(
        BridgeUpstreamProtocol::XaiResponsesOauth,
        AdapterSourceProduct::XaiGrokSubscription,
        AgentId::Codex,
        true,
        false,
        json!({ "model": "grok-4.5", "store": true, "input": "hi" }),
    );
    assert!(UpstreamChannel::Grok.passthrough_for(DownstreamSurface::Responses));
    assert!(!identity_relay(
        UpstreamChannel::Grok,
        DownstreamSurface::Responses,
        &grok.state
    ));
    assert!(pair_adapter_active(&grok.state, UpstreamChannel::Grok));
}

#[test]
fn same_dialect_does_not_force_pair_adapter_when_flags_on() {
    let same = pair_admitted(
        BridgeUpstreamProtocol::XaiResponsesOauth,
        AdapterSourceProduct::XaiGrokSubscription,
        AgentId::Grok,
        true,
        true,
        json!({ "model": "grok-4.5", "input": "hi" }),
    );
    assert!(identity_relay(
        UpstreamChannel::Grok,
        DownstreamSurface::Responses,
        &same.state
    ));
    assert!(!pair_adapter_active(&same.state, UpstreamChannel::Grok));
}

#[test]
fn closed_matrix_cell_does_not_use_pair_adapter() {
    assert!(pair_edge_can_apply(
        Some(AdapterSourceProduct::XaiGrokSubscription),
        Some(AgentId::Codex)
    ));
    assert!(pair_edge_can_apply(
        Some(AdapterSourceProduct::CodexChatGptSubscription),
        Some(AgentId::Grok)
    ));
    assert!(!pair_edge_can_apply(
        Some(AdapterSourceProduct::ClaudeSubscription),
        Some(AgentId::Codex)
    ));
}

#[test]
fn flag_on_codex_to_grok_prepare_strips_store_and_system_items() {
    let admitted = pair_admitted(
        BridgeUpstreamProtocol::XaiResponsesOauth,
        AdapterSourceProduct::XaiGrokSubscription,
        AgentId::Codex,
        true,
        false,
        json!({
            "model": "grok-4.5",
            "store": true,
            "metadata": { "x": 1 },
            "input": [
                {
                    "type": "message",
                    "role": "system",
                    "content": [{ "type": "input_text", "text": "sys" }]
                },
                {
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "hi" }]
                }
            ]
        }),
    );
    let prepared = UpstreamChannel::Grok
        .prepare(DownstreamSurface::Responses, &admitted)
        .expect("prepare");
    assert!(prepared.body.get("store").is_none(), "{}", prepared.body);
    assert!(prepared.body.get("metadata").is_none(), "{}", prepared.body);
    assert_no_system_or_developer_items(&prepared.body);
    assert!(prepared.grok_identity.is_some());
}

#[test]
fn flag_on_grok_to_codex_prepare_uses_official_allowlist_without_grok_identity() {
    let admitted = pair_admitted(
        BridgeUpstreamProtocol::CodexResponsesOauth,
        AdapterSourceProduct::CodexChatGptSubscription,
        AgentId::Grok,
        false,
        true,
        json!({
            "model": "gpt-5.4",
            "prompt_cache_key": "cache-1",
            "store": true,
            "input": "hi"
        }),
    );
    let prepared = UpstreamChannel::CodexResponses
        .prepare(DownstreamSurface::Responses, &admitted)
        .expect("prepare");
    assert_eq!(prepared.body["store"], false);
    assert!(prepared.body.get("prompt_cache_key").is_none());
    assert!(prepared.grok_identity.is_none());
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

fn assert_no_system_or_developer_items(body: &serde_json::Value) {
    let Some(items) = body.get("input").and_then(serde_json::Value::as_array) else {
        return;
    };
    for item in items {
        let role = item.get("role").and_then(serde_json::Value::as_str);
        assert_ne!(role, Some("system"), "{item}");
        assert_ne!(role, Some("developer"), "{item}");
    }
}

#[test]
fn official_codex_messages_prepare_folds_system_and_forces_store_false() {
    let body = json!({
        "model": "claude-sonnet-4-20250514",
        "max_tokens": 32,
        "system": "Claude Code system prompt.",
        "messages": [
            { "role": "system", "content": "Extra system." },
            { "role": "user", "content": "ping" }
        ]
    });
    let admitted = admitted(
        BridgeUpstreamProtocol::CodexResponsesOauth,
        BridgeLocalSurface::Messages,
        body,
    );
    let prepared = UpstreamChannel::from_protocol(BridgeUpstreamProtocol::CodexResponsesOauth)
        .prepare(DownstreamSurface::Messages, &admitted)
        .expect("prepare messages");
    assert_eq!(prepared.body["store"], false);
    assert_no_system_or_developer_items(&prepared.body);
    assert!(
        prepared.body.get("max_output_tokens").is_none(),
        "official Codex Responses rejects max_output_tokens: {}",
        prepared.body
    );
    let instructions = prepared.body["instructions"]
        .as_str()
        .expect("instructions");
    assert!(
        instructions.contains("Claude Code system prompt."),
        "{instructions}"
    );
    assert!(instructions.contains("Extra system."), "{instructions}");
    assert_eq!(prepared.body["input"][0]["role"], "user");
    assert_eq!(prepared.body["input"][0]["content"][0]["text"], "ping");
}

#[test]
fn official_codex_chat_prepare_folds_developer_and_forces_store_false() {
    let body = json!({
        "model": "claude-sonnet-4-20250514",
        "max_tokens": 32,
        "messages": [
            { "role": "system", "content": "Be brief." },
            { "role": "user", "content": "hello" },
            { "role": "developer", "content": "Stay in English." }
        ]
    });
    let admitted = admitted(
        BridgeUpstreamProtocol::CodexResponsesOauth,
        BridgeLocalSurface::ChatCompletions,
        body,
    );
    let prepared = UpstreamChannel::from_protocol(BridgeUpstreamProtocol::CodexResponsesOauth)
        .prepare(DownstreamSurface::ChatCompletions, &admitted)
        .expect("prepare chat");
    assert_eq!(prepared.body["store"], false);
    assert_no_system_or_developer_items(&prepared.body);
    assert!(
        prepared.body.get("max_output_tokens").is_none(),
        "official Codex Responses rejects max_output_tokens: {}",
        prepared.body
    );
    let instructions = prepared.body["instructions"]
        .as_str()
        .expect("instructions");
    assert!(instructions.contains("Be brief."), "{instructions}");
    assert!(instructions.contains("Stay in English."), "{instructions}");
    assert_eq!(prepared.body["input"][0]["role"], "user");
    assert_eq!(prepared.body["input"][0]["content"][0]["text"], "hello");
}

#[test]
fn official_codex_responses_passthrough_strips_system_items() {
    let body = json!({
        "model": "claude-sonnet-4-20250514",
        "max_output_tokens": 64,
        "temperature": 0.2,
        "top_p": 0.9,
        "input": [
            {
                "type": "message",
                "role": "system",
                "content": [{ "type": "input_text", "text": "You are a coding agent." }]
            },
            {
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": "hello" }]
            }
        ]
    });
    let admitted = admitted(
        BridgeUpstreamProtocol::CodexResponsesOauth,
        BridgeLocalSurface::Responses,
        body,
    );
    let prepared = UpstreamChannel::from_protocol(BridgeUpstreamProtocol::CodexResponsesOauth)
        .prepare(DownstreamSurface::Responses, &admitted)
        .expect("prepare responses");
    assert_eq!(prepared.body["store"], false);
    assert_no_system_or_developer_items(&prepared.body);
    assert!(
        prepared.body.get("max_output_tokens").is_none(),
        "official Codex Responses rejects max_output_tokens: {}",
        prepared.body
    );
    assert_eq!(prepared.body["temperature"], 0.2);
    assert_eq!(prepared.body["top_p"], 0.9);
    let user_text = prepared.body["input"][0]["content"][0]["text"]
        .as_str()
        .expect("user text");
    assert!(user_text.contains("You are a coding agent."), "{user_text}");
    assert!(user_text.contains("hello"), "{user_text}");
}

#[test]
fn anthropic_prepare_passthroughs_messages_surface() {
    let body = json!({
        "model": "claude-sonnet-4",
        "max_tokens": 16,
        "messages": [{"role": "user", "content": "hi"}]
    });
    let admitted = admitted(
        BridgeUpstreamProtocol::AnthropicMessages,
        BridgeLocalSurface::Messages,
        body.clone(),
    );
    let prepared = UpstreamChannel::from_protocol(BridgeUpstreamProtocol::AnthropicMessages)
        .prepare(DownstreamSurface::Messages, &admitted)
        .expect("messages prepare");
    assert_eq!(prepared.path, "messages");
    assert_eq!(prepared.body["model"], "claude-sonnet-4");
    assert_eq!(prepared.body["max_tokens"], 16);
}

#[test]
fn openai_chat_prepare_passthroughs_chat_surface() {
    let body = json!({
        "model": "gpt-test",
        "messages": [{"role": "user", "content": "hi"}],
        "stream": true,
        "response_format": {"type": "json_object"}
    });
    let admitted = admitted(
        BridgeUpstreamProtocol::OpenAiChatCompletions,
        BridgeLocalSurface::ChatCompletions,
        body.clone(),
    );
    let prepared = UpstreamChannel::from_protocol(BridgeUpstreamProtocol::OpenAiChatCompletions)
        .prepare(DownstreamSurface::ChatCompletions, &admitted)
        .expect("chat prepare");
    assert_eq!(prepared.path, "chat/completions");
    assert_eq!(prepared.body["model"], "configured-model");
    assert_eq!(prepared.body["messages"], body["messages"]);
    assert_eq!(prepared.body["response_format"], body["response_format"]);
    assert!(prepared.stream);
}

#[test]
fn anthropic_prepare_converts_chat_surface() {
    let admitted = admitted(
        BridgeUpstreamProtocol::AnthropicMessages,
        BridgeLocalSurface::ChatCompletions,
        json!({
            "model": "gpt-test",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 32
        }),
    );
    let prepared = UpstreamChannel::from_protocol(BridgeUpstreamProtocol::AnthropicMessages)
        .prepare(DownstreamSurface::ChatCompletions, &admitted)
        .expect("chat to messages prepare");
    assert_eq!(prepared.path, "messages");
    assert_eq!(prepared.body["model"], "configured-model");
    assert_eq!(prepared.body["messages"][0]["role"], "user");
    assert_eq!(prepared.body["max_tokens"], 32);
}

#[test]
fn grok_prepare_converts_chat_surface_to_responses() {
    let admitted = admitted(
        BridgeUpstreamProtocol::XaiResponsesOauth,
        BridgeLocalSurface::ChatCompletions,
        json!({
            "model": "gpt-test",
            "messages": [{"role": "user", "content": "hi"}],
            "stream": false
        }),
    );
    let prepared = UpstreamChannel::from_protocol(BridgeUpstreamProtocol::XaiResponsesOauth)
        .prepare(DownstreamSurface::ChatCompletions, &admitted)
        .expect("chat to grok responses prepare");
    assert_eq!(prepared.path, "responses");
    assert_eq!(prepared.body["model"], "configured-model");
    assert_eq!(prepared.body["input"][0]["role"], "user");
    assert_eq!(prepared.body["input"][0]["content"][0]["text"], "hi");
}
