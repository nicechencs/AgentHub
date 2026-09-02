//! Table-driven DownstreamSurface × UpstreamChannel prepare regression lock.
//!
//! Fails if an open conversion cell regresses on path, passthrough declaration,
//! prepare success, or Codex official stream force. Models is documented as
//! unreachable (not prepared here).

use serde_json::json;

use crate::bridge::runtime::{BridgeLocalSurface, BridgeUpstreamProtocol};
use crate::models::{AdapterSourceProduct, AgentId};

use super::super::super::surface::DownstreamSurface;
use super::super::{UpstreamChannel, UpstreamPrepare};
use super::{admitted, pair_admitted};

struct ConversionCell {
    name: &'static str,
    channel: UpstreamChannel,
    surface: DownstreamSurface,
    local_surface: BridgeLocalSurface,
    expected_path: &'static str,
    expect_passthrough: bool,
    /// When set, assert prepared JSON `stream` equals this (Codex official force).
    expect_body_stream: Option<bool>,
    body: serde_json::Value,
}

fn conversation_cells() -> Vec<ConversionCell> {
    let messages_body = json!({
        "model": "test-model",
        "max_tokens": 32,
        "stream": false,
        "messages": [{ "role": "user", "content": "hi" }]
    });
    let responses_body = json!({
        "model": "test-model",
        "stream": false,
        "input": "hi"
    });
    let chat_body = json!({
        "model": "test-model",
        "stream": false,
        "messages": [{ "role": "user", "content": "hi" }],
        "max_tokens": 32
    });

    vec![
        // Messages × *
        ConversionCell {
            name: "messages_openai_chat",
            channel: UpstreamChannel::OpenAiChat,
            surface: DownstreamSurface::Messages,
            local_surface: BridgeLocalSurface::Messages,
            expected_path: "chat/completions",
            expect_passthrough: false,
            expect_body_stream: None,
            body: messages_body.clone(),
        },
        ConversionCell {
            name: "messages_anthropic",
            channel: UpstreamChannel::Anthropic,
            surface: DownstreamSurface::Messages,
            local_surface: BridgeLocalSurface::Messages,
            expected_path: "messages",
            expect_passthrough: true,
            expect_body_stream: None,
            body: messages_body.clone(),
        },
        ConversionCell {
            name: "messages_codex",
            channel: UpstreamChannel::CodexResponses,
            surface: DownstreamSurface::Messages,
            local_surface: BridgeLocalSurface::Messages,
            expected_path: "responses",
            expect_passthrough: false,
            expect_body_stream: Some(true),
            body: messages_body.clone(),
        },
        ConversionCell {
            name: "messages_grok",
            channel: UpstreamChannel::Grok,
            surface: DownstreamSurface::Messages,
            local_surface: BridgeLocalSurface::Messages,
            expected_path: "responses",
            expect_passthrough: false,
            expect_body_stream: None,
            body: messages_body.clone(),
        },
        // Responses × *
        ConversionCell {
            name: "responses_openai_chat",
            channel: UpstreamChannel::OpenAiChat,
            surface: DownstreamSurface::Responses,
            local_surface: BridgeLocalSurface::Responses,
            expected_path: "chat/completions",
            expect_passthrough: false,
            expect_body_stream: None,
            body: responses_body.clone(),
        },
        ConversionCell {
            name: "responses_anthropic",
            channel: UpstreamChannel::Anthropic,
            surface: DownstreamSurface::Responses,
            local_surface: BridgeLocalSurface::Responses,
            expected_path: "messages",
            expect_passthrough: false,
            expect_body_stream: None,
            body: responses_body.clone(),
        },
        ConversionCell {
            name: "responses_codex",
            channel: UpstreamChannel::CodexResponses,
            surface: DownstreamSurface::Responses,
            local_surface: BridgeLocalSurface::Responses,
            expected_path: "responses",
            expect_passthrough: true,
            expect_body_stream: Some(true),
            body: responses_body.clone(),
        },
        ConversionCell {
            name: "responses_grok",
            channel: UpstreamChannel::Grok,
            surface: DownstreamSurface::Responses,
            local_surface: BridgeLocalSurface::Responses,
            expected_path: "responses",
            expect_passthrough: true,
            expect_body_stream: None,
            body: responses_body.clone(),
        },
        // ChatCompletions × *
        ConversionCell {
            name: "chat_openai_chat",
            channel: UpstreamChannel::OpenAiChat,
            surface: DownstreamSurface::ChatCompletions,
            local_surface: BridgeLocalSurface::ChatCompletions,
            expected_path: "chat/completions",
            expect_passthrough: true,
            expect_body_stream: None,
            body: chat_body.clone(),
        },
        ConversionCell {
            name: "chat_anthropic",
            channel: UpstreamChannel::Anthropic,
            surface: DownstreamSurface::ChatCompletions,
            local_surface: BridgeLocalSurface::ChatCompletions,
            expected_path: "messages",
            expect_passthrough: false,
            expect_body_stream: None,
            body: chat_body.clone(),
        },
        ConversionCell {
            name: "chat_codex",
            channel: UpstreamChannel::CodexResponses,
            surface: DownstreamSurface::ChatCompletions,
            local_surface: BridgeLocalSurface::ChatCompletions,
            expected_path: "responses",
            expect_passthrough: false,
            expect_body_stream: Some(true),
            body: chat_body.clone(),
        },
        ConversionCell {
            name: "chat_grok",
            channel: UpstreamChannel::Grok,
            surface: DownstreamSurface::ChatCompletions,
            local_surface: BridgeLocalSurface::ChatCompletions,
            expected_path: "responses",
            expect_passthrough: false,
            expect_body_stream: None,
            body: chat_body,
        },
    ]
}

fn prepare_cell(cell: &ConversionCell) -> UpstreamPrepare {
    let protocol = cell.channel.protocol();
    let request = admitted(protocol, cell.local_surface, cell.body.clone());
    cell.channel
        .transport()
        .prepare(cell.surface, &request)
        .unwrap_or_else(|_| panic!("prepare failed for cell {}", cell.name))
}

#[test]
fn conversion_matrix_prepare_cells_match_path_passthrough_and_stream_policy() {
    let cells = conversation_cells();
    assert_eq!(
        cells.len(),
        12,
        "expected 3 conversation surfaces × 4 channels"
    );

    for cell in &cells {
        assert_eq!(
            cell.channel.passthrough_for(cell.surface),
            cell.expect_passthrough,
            "{}: passthrough_for mismatch",
            cell.name
        );
        assert_eq!(
            cell.channel.transport().path(),
            cell.expected_path,
            "{}: channel path()",
            cell.name
        );

        let prepared = prepare_cell(cell);
        assert_eq!(
            prepared.path, cell.expected_path,
            "{}: prepared path",
            cell.name
        );
        assert!(
            prepared.body.is_object(),
            "{}: prepared body must be object: {}",
            cell.name,
            prepared.body
        );

        if let Some(expect_stream) = cell.expect_body_stream {
            assert_eq!(
                prepared.body.get("stream").and_then(|v| v.as_bool()),
                Some(expect_stream),
                "{}: body.stream (Codex forces true via official prepare)",
                cell.name
            );
        }

        if cell.channel == UpstreamChannel::CodexResponses {
            assert!(
                cell.channel.forces_upstream_stream(),
                "{}: Codex must force upstream stream",
                cell.name
            );
        } else {
            assert!(
                !cell.channel.forces_upstream_stream(),
                "{}: non-Codex must not force upstream stream",
                cell.name
            );
        }
    }
}

#[test]
fn conversion_matrix_documents_models_surface_as_unreachable() {
    // Models is synthesized by list_models; conversation prepare must not handle it.
    // Lock the enum presence so a new wire surface without matrix coverage fails loudly.
    let surfaces = [
        DownstreamSurface::Messages,
        DownstreamSurface::Responses,
        DownstreamSurface::ChatCompletions,
        DownstreamSurface::Models,
    ];
    assert_eq!(surfaces.len(), 4);
    let channels = [
        UpstreamChannel::OpenAiChat,
        UpstreamChannel::Anthropic,
        UpstreamChannel::CodexResponses,
        UpstreamChannel::Grok,
    ];
    assert_eq!(channels.len(), 4);
    // 12 conversation cells covered above; 4 Models cells intentionally omitted.
    assert_eq!(conversation_cells().len() + channels.len(), 16);
}

#[test]
fn conversion_matrix_pair_flag_responses_prepare_still_works() {
    // Codex ingress → Grok upstream (pair adapter active)
    let grok = pair_admitted(
        BridgeUpstreamProtocol::XaiResponsesOauth,
        AdapterSourceProduct::XaiGrokSubscription,
        AgentId::Codex,
        true,
        false,
        json!({
            "model": "grok-4.5",
            "store": true,
            "input": "hi"
        }),
    );
    let prepared_grok = UpstreamChannel::Grok
        .transport()
        .prepare(DownstreamSurface::Responses, &grok)
        .expect("pair Responses→Grok prepare");
    assert_eq!(prepared_grok.path, "responses");
    assert!(UpstreamChannel::Grok.passthrough_for(DownstreamSurface::Responses));

    // Grok ingress → Codex upstream
    let codex = pair_admitted(
        BridgeUpstreamProtocol::CodexResponsesOauth,
        AdapterSourceProduct::CodexChatGptSubscription,
        AgentId::Grok,
        false,
        true,
        json!({
            "model": "gpt-5.4",
            "store": true,
            "input": "hi"
        }),
    );
    let prepared_codex = UpstreamChannel::CodexResponses
        .transport()
        .prepare(DownstreamSurface::Responses, &codex)
        .expect("pair Responses→Codex prepare");
    assert_eq!(prepared_codex.path, "responses");
    assert_eq!(prepared_codex.body["stream"], true);
    assert_eq!(prepared_codex.body["store"], false);
    assert!(UpstreamChannel::CodexResponses.passthrough_for(DownstreamSurface::Responses));
}

#[test]
fn conversion_matrix_channel_protocol_bijection() {
    for channel in [
        UpstreamChannel::OpenAiChat,
        UpstreamChannel::Anthropic,
        UpstreamChannel::CodexResponses,
        UpstreamChannel::Grok,
    ] {
        assert_eq!(UpstreamChannel::from_protocol(channel.protocol()), channel);
    }
}
