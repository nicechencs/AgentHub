use super::*;

#[test]
fn missing_key_is_fail_closed() {
    let key = AdapterCapabilityKey {
        source: AdapterSourceProduct::AnthropicApi,
        credential: AdapterCredentialClass::ApiKey,
        transport: AdapterUpstreamTransport::NativeHttp,
        target: AgentId::Codex,
        protocol: AdapterTargetProtocol::OpenAiResponses,
        version: MATRIX_VERSION,
    };
    assert!(lookup_adapter_capability(&key).is_none());
    let decision = decide_adapter_capability(
        AdapterSourceProduct::AnthropicApi,
        AdapterCredentialClass::ApiKey,
        AgentId::Codex,
    )
    .public_surface();
    assert_eq!(decision.route, AdapterRoute::LocalBridge);
    assert_eq!(decision.support, AdapterSupport::Experimental);
    assert!(decision.can_apply);
    assert_eq!(decision.rule_id, Some("anthropic-api-to-codex-v1"));
}

#[test]
fn openai_api_to_codex_is_experimental_local_bridge() {
    let key = AdapterCapabilityKey {
        source: AdapterSourceProduct::OpenaiApi,
        credential: AdapterCredentialClass::ApiKey,
        transport: AdapterUpstreamTransport::NativeHttp,
        target: AgentId::Codex,
        protocol: AdapterTargetProtocol::OpenAiResponses,
        version: MATRIX_VERSION,
    };
    assert!(lookup_adapter_capability(&key).is_none());
    let decision = decide_adapter_capability(
        AdapterSourceProduct::OpenaiApi,
        AdapterCredentialClass::ApiKey,
        AgentId::Codex,
    )
    .public_surface();
    assert_eq!(decision.route, AdapterRoute::LocalBridge);
    assert_eq!(decision.support, AdapterSupport::Experimental);
    assert!(decision.can_apply);
    assert_eq!(decision.rule_id, Some("openai-api-to-codex-v1"));
    assert_eq!(
        decision.transport,
        AdapterUpstreamTransport::LocalBridgeChatCompletions
    );
}

#[test]
fn kimi_claude_and_codex_cells_are_applicable() {
    let claude = decide_adapter_capability(
        AdapterSourceProduct::KimiCodeMembership,
        AdapterCredentialClass::ApiKey,
        AgentId::Claude,
    );
    assert_eq!(claude.route, AdapterRoute::NativeEndpoint);
    assert!(claude.can_apply);
    assert_eq!(claude.rule_id, Some("kimi-membership-to-claude-v1"));

    let codex = decide_adapter_capability(
        AdapterSourceProduct::KimiCodeMembership,
        AdapterCredentialClass::ApiKey,
        AgentId::Codex,
    );
    assert_eq!(codex.route, AdapterRoute::LocalBridge);
    assert!(codex.can_apply);
    assert_eq!(codex.support, AdapterSupport::Experimental);
}

#[test]
fn official_codex_oauth_can_apply_onto_codex() {
    for credential in [
        AdapterCredentialClass::OauthAuthJson,
        AdapterCredentialClass::OauthOther,
    ] {
        let decision = decide_adapter_capability(
            AdapterSourceProduct::CodexChatGptSubscription,
            credential,
            AgentId::Codex,
        )
        .public_surface();
        assert_eq!(decision.route, AdapterRoute::NativeEndpoint);
        assert!(decision.can_apply);
        assert_eq!(decision.rule_id, Some(CODEX_SUBSCRIPTION_TO_CODEX_RULE_ID));
        assert_eq!(decision.reason, CODEX_SUBSCRIPTION_TO_CODEX_REASON);
        assert!(!decision.reason.contains("本机路由"));
    }
}

#[test]
fn codex_oauth_to_claude_opens_only_the_responses_cell() {
    let decision = decide_adapter_capability(
        AdapterSourceProduct::CodexChatGptSubscription,
        AdapterCredentialClass::OauthAuthJson,
        AgentId::Claude,
    )
    .public_surface();
    assert_eq!(decision.route, AdapterRoute::LocalBridge);
    assert_eq!(decision.support, AdapterSupport::Experimental);
    assert!(decision.can_apply);
    assert_eq!(decision.reason, CODEX_SUBSCRIPTION_TO_CLAUDE_REASON);
    assert_eq!(decision.gate_kind, AdapterGateKind::None);
    assert!(!decision.reason.contains("Messages"));
    let gates = decision.gates.expect("candidate retains gate record");
    assert!(gates.all_passed());

    let app_server = lookup_adapter_capability(&AdapterCapabilityKey {
        source: AdapterSourceProduct::CodexChatGptSubscription,
        credential: AdapterCredentialClass::OauthAuthJson,
        transport: AdapterUpstreamTransport::CodexAppServer,
        target: AgentId::Claude,
        protocol: AdapterTargetProtocol::AnthropicMessages,
        version: "0",
    })
    .expect("app server candidate cell");
    assert!(!app_server.can_apply);
    assert!(!app_server.gates.all_passed());
    assert!(!AdapterCapabilityDecision::from_cell(app_server).can_apply);

    let responses = lookup_adapter_capability(&AdapterCapabilityKey {
        source: AdapterSourceProduct::CodexChatGptSubscription,
        credential: AdapterCredentialClass::OauthAuthJson,
        transport: AdapterUpstreamTransport::CodexResponsesOauth,
        target: AgentId::Claude,
        protocol: AdapterTargetProtocol::AnthropicMessages,
        version: MATRIX_VERSION,
    })
    .expect("Responses cell");
    assert!(responses.can_apply);
    assert_eq!(
        responses.rule_id,
        "codex-subscription-to-claude-responses-v1"
    );
    assert_eq!(responses.gates, AdapterCapabilityGates::all_open());
}

#[test]
fn every_matrix_cell_has_reason_and_version() {
    for cell in ADAPTER_CAPABILITY_MATRIX {
        assert!(!cell.reason.is_empty());
        assert!(!cell.key.version.is_empty());
        assert!(!cell.rule_id.is_empty());
        assert!(!cell.verified_at.is_empty());
        if cell.can_apply {
            assert!(
                cell.gates.all_passed(),
                "{} claims can_apply with closed gates",
                cell.rule_id
            );
        }
        assert!(
            !cell.multi_account,
            "{} must keep multi_account closed until evidenced",
            cell.rule_id
        );
    }
}

#[test]
fn local_bridge_multi_account_stays_fail_closed() {
    assert!(!local_bridge_multi_account(
        "grok-subscription-to-claude-v1"
    ));
    assert!(!local_bridge_multi_account(
        "codex-subscription-to-claude-responses-v1"
    ));
    assert!(!local_bridge_multi_account("missing-rule"));
}

#[test]
fn maturity_maps_open_stable_experimental_preview_and_none() {
    let kimi_claude = decide_adapter_capability(
        AdapterSourceProduct::KimiCodeMembership,
        AdapterCredentialClass::ApiKey,
        AgentId::Claude,
    );
    assert_eq!(
        adapter_maturity_from_decision(&kimi_claude),
        AdapterMaturity::Stable
    );

    let kimi_codex = decide_adapter_capability(
        AdapterSourceProduct::KimiCodeMembership,
        AdapterCredentialClass::ApiKey,
        AgentId::Codex,
    );
    assert_eq!(
        adapter_maturity_from_decision(&kimi_codex),
        AdapterMaturity::Experimental
    );

    let codex_claude = decide_adapter_capability(
        AdapterSourceProduct::CodexChatGptSubscription,
        AdapterCredentialClass::OauthAuthJson,
        AgentId::Claude,
    )
    .public_surface();
    assert_eq!(
        adapter_maturity_from_decision(&codex_claude),
        AdapterMaturity::Experimental
    );
    assert!(codex_claude.can_apply);

    let anthropic_codex = decide_adapter_capability(
        AdapterSourceProduct::AnthropicApi,
        AdapterCredentialClass::ApiKey,
        AgentId::Codex,
    )
    .public_surface();
    assert_eq!(
        adapter_maturity_from_decision(&anthropic_codex),
        AdapterMaturity::Experimental
    );
    assert!(anthropic_codex.can_apply);

    let other = decide_adapter_capability(
        AdapterSourceProduct::Other,
        AdapterCredentialClass::Unknown,
        AgentId::Claude,
    )
    .public_surface();
    assert_eq!(
        adapter_maturity_from_decision(&other),
        AdapterMaturity::None
    );
}

#[test]
fn pi_config_sync_rules_can_apply() {
    let kimi_pi = decide_adapter_capability(
        AdapterSourceProduct::KimiCodeMembership,
        AdapterCredentialClass::ApiKey,
        AgentId::Pi,
    );
    assert_eq!(kimi_pi.route, AdapterRoute::ConfigSync);
    assert!(kimi_pi.can_apply);
    assert_eq!(kimi_pi.gate_kind, AdapterGateKind::None);
    assert_eq!(kimi_pi.rule_id, Some("kimi-membership-to-pi-v1"));

    let anthropic_pi = decide_adapter_capability(
        AdapterSourceProduct::AnthropicApi,
        AdapterCredentialClass::ApiKey,
        AgentId::Pi,
    );
    assert_eq!(anthropic_pi.route, AdapterRoute::ConfigSync);
    assert!(anthropic_pi.can_apply);
    assert_eq!(anthropic_pi.gate_kind, AdapterGateKind::None);
    assert_eq!(anthropic_pi.rule_id, Some("anthropic-api-to-pi-v1"));

    let openai_pi = decide_adapter_capability(
        AdapterSourceProduct::OpenaiApi,
        AdapterCredentialClass::ApiKey,
        AgentId::Pi,
    );
    assert_eq!(openai_pi.route, AdapterRoute::ConfigSync);
    assert!(openai_pi.can_apply);
    assert_eq!(openai_pi.rule_id, Some("openai-api-to-pi-v1"));

    let xai_pi = decide_adapter_capability(
        AdapterSourceProduct::XaiApi,
        AdapterCredentialClass::ApiKey,
        AgentId::Pi,
    );
    assert_eq!(xai_pi.route, AdapterRoute::ConfigSync);
    assert!(xai_pi.can_apply);
    assert_eq!(xai_pi.rule_id, Some("xai-api-to-pi-v1"));

    for (source, rule) in [
        (
            AdapterSourceProduct::GlmCodingPlan,
            "glm-coding-plan-to-pi-v1",
        ),
        (AdapterSourceProduct::DeepseekApi, "deepseek-api-to-pi-v1"),
    ] {
        let cell = ADAPTER_CAPABILITY_MATRIX
            .iter()
            .find(|cell| {
                cell.key.source == source
                    && cell.key.credential == AdapterCredentialClass::ApiKey
                    && cell.key.target == AgentId::Pi
            })
            .expect("GLM/DeepSeek Pi cell");
        assert_eq!(cell.key.transport, AdapterUpstreamTransport::NativeHttp);
        assert_eq!(cell.key.protocol, AdapterTargetProtocol::PiProviderConfig);
        assert_eq!(cell.key.version, MATRIX_VERSION);
        assert_eq!(cell.verified_at, "2026-08-15");
        assert_eq!(cell.gates, AdapterCapabilityGates::all_open());
        let pi = decide_adapter_capability(source, AdapterCredentialClass::ApiKey, AgentId::Pi);
        assert_eq!(pi.route, AdapterRoute::ConfigSync);
        assert_eq!(pi.support, AdapterSupport::Experimental);
        assert!(pi.can_apply);
        assert_eq!(pi.rule_id, Some(rule));
        assert_eq!(pi.gate_kind, AdapterGateKind::None);
    }
}

#[test]
fn subscription_pi_cells_are_native_http_and_applicable() {
    for (source, credential, reason, rule_id) in [
        (
            AdapterSourceProduct::ClaudeSubscription,
            AdapterCredentialClass::OauthOther,
            CLAUDE_SUBSCRIPTION_TO_PI_REASON,
            "claude-subscription-to-pi-v1",
        ),
        (
            AdapterSourceProduct::CodexChatGptSubscription,
            AdapterCredentialClass::OauthAuthJson,
            CODEX_SUBSCRIPTION_TO_PI_REASON,
            "codex-subscription-to-pi-v1",
        ),
        (
            AdapterSourceProduct::CodexChatGptSubscription,
            AdapterCredentialClass::OauthOther,
            CODEX_SUBSCRIPTION_TO_PI_REASON,
            "codex-subscription-to-pi-v1",
        ),
        (
            AdapterSourceProduct::XaiGrokSubscription,
            AdapterCredentialClass::OauthOther,
            GROK_SUBSCRIPTION_TO_PI_REASON,
            "grok-subscription-to-pi-v1",
        ),
    ] {
        let cell = ADAPTER_CAPABILITY_MATRIX
            .iter()
            .find(|cell| {
                cell.key.source == source
                    && cell.key.credential == credential
                    && cell.key.target == AgentId::Pi
            })
            .expect("subscription Pi cell");
        assert_eq!(cell.key.transport, AdapterUpstreamTransport::NativeHttp);
        assert_eq!(cell.key.protocol, AdapterTargetProtocol::PiProviderConfig);
        assert_eq!(cell.key.version, MATRIX_VERSION);
        assert_eq!(cell.verified_at, "2026-08-15");
        assert_eq!(cell.route, AdapterRoute::ConfigSync);
        assert_eq!(cell.support, AdapterSupport::Experimental);
        assert!(cell.can_apply);
        assert_eq!(cell.gates, AdapterCapabilityGates::all_open());
        assert_eq!(cell.reason, reason);
        assert_eq!(cell.rule_id, rule_id);

        let decision = decide_adapter_capability(source, credential, AgentId::Pi);
        assert_eq!(decision.route, AdapterRoute::ConfigSync);
        assert_eq!(decision.support, AdapterSupport::Experimental);
        assert!(decision.can_apply);
        assert_eq!(decision.gate_kind, AdapterGateKind::None);
        assert_eq!(decision.reason, reason);
        assert_eq!(decision.rule_id, Some(rule_id));
    }
}

#[test]
fn codex_app_server_candidate_stays_v0_and_closed() {
    let cell = lookup_adapter_capability(&AdapterCapabilityKey {
        source: AdapterSourceProduct::CodexChatGptSubscription,
        credential: AdapterCredentialClass::OauthAuthJson,
        transport: AdapterUpstreamTransport::CodexAppServer,
        target: AgentId::Claude,
        protocol: AdapterTargetProtocol::AnthropicMessages,
        version: "0",
    })
    .expect("App Server candidate");
    assert_eq!(cell.rule_id, "codex-subscription-to-claude-app-server-v0");
    assert_eq!(cell.key.version, "0");
    assert!(!cell.can_apply);
    assert_eq!(cell.gates, AdapterCapabilityGates::all_closed());
}

#[test]
fn glm_and_deepseek_claude_cells_are_experimental_and_applicable() {
    let glm = decide_adapter_capability(
        AdapterSourceProduct::GlmCodingPlan,
        AdapterCredentialClass::ApiKey,
        AgentId::Claude,
    );
    assert_eq!(glm.route, AdapterRoute::NativeEndpoint);
    assert_eq!(glm.support, AdapterSupport::Experimental);
    assert!(glm.can_apply);
    assert_eq!(glm.rule_id, Some("glm-coding-plan-to-claude-v1"));
    assert!(glm.gates.expect("glm cell keeps gates").all_passed());

    let deepseek = decide_adapter_capability(
        AdapterSourceProduct::DeepseekApi,
        AdapterCredentialClass::ApiKey,
        AgentId::Claude,
    );
    assert_eq!(deepseek.route, AdapterRoute::NativeEndpoint);
    assert_eq!(deepseek.support, AdapterSupport::Experimental);
    assert!(deepseek.can_apply);
    assert_eq!(deepseek.rule_id, Some("deepseek-api-to-claude-v1"));
    assert!(deepseek
        .gates
        .expect("deepseek cell keeps gates")
        .all_passed());
}

#[test]
fn glm_and_deepseek_codex_cells_are_experimental_native_responses() {
    for (source, rule) in [
        (
            AdapterSourceProduct::GlmCodingPlan,
            "glm-coding-plan-to-codex-v1",
        ),
        (
            AdapterSourceProduct::DeepseekApi,
            "deepseek-api-to-codex-v1",
        ),
    ] {
        let decision =
            decide_adapter_capability(source, AdapterCredentialClass::ApiKey, AgentId::Codex);
        assert_eq!(decision.route, AdapterRoute::NativeEndpoint);
        assert_eq!(decision.support, AdapterSupport::Experimental);
        assert!(decision.can_apply);
        assert_eq!(decision.rule_id, Some(rule));
        assert_eq!(decision.transport, AdapterUpstreamTransport::NativeHttp);
        assert_eq!(
            decision.protocol,
            Some(AdapterTargetProtocol::OpenAiResponses)
        );
        assert!(decision.gates.expect("native cell gates").all_passed());
    }
}

#[test]
fn deepseek_api_to_dsh_can_apply() {
    let dsh = decide_adapter_capability(
        AdapterSourceProduct::DeepseekApi,
        AdapterCredentialClass::ApiKey,
        AgentId::Dsh,
    );
    assert_eq!(dsh.route, AdapterRoute::ConfigSync);
    assert_eq!(dsh.support, AdapterSupport::Stable);
    assert!(dsh.can_apply);
    assert_eq!(dsh.rule_id, Some("deepseek-api-to-dsh-v1"));
}

#[test]
fn registered_surfaces_have_writable_pi_cells() {
    for source in [
        AdapterSourceProduct::GlmCodingPlan,
        AdapterSourceProduct::DeepseekApi,
    ] {
        let decision =
            decide_adapter_capability(source, AdapterCredentialClass::ApiKey, AgentId::Pi)
                .public_surface();
        assert_eq!(decision.route, AdapterRoute::ConfigSync);
        assert!(decision.can_apply);
        assert!(decision.rule_id.is_some());
    }

    let openai_grok = decide_adapter_capability(
        AdapterSourceProduct::OpenaiApi,
        AdapterCredentialClass::ApiKey,
        AgentId::Grok,
    )
    .public_surface();
    assert_eq!(openai_grok.route, AdapterRoute::NativeEndpoint);
    assert!(openai_grok.can_apply);
    assert_eq!(openai_grok.rule_id, Some("openai-api-to-grok-v1"));

    let xai_grok = decide_adapter_capability(
        AdapterSourceProduct::XaiApi,
        AdapterCredentialClass::ApiKey,
        AgentId::Grok,
    )
    .public_surface();
    assert_eq!(xai_grok.route, AdapterRoute::Unsupported);
    assert!(!xai_grok.can_apply);
    assert_eq!(xai_grok.reason, SAME_PROTOCOL_NO_EDGE_REASON);
    assert_eq!(xai_grok.reason, "这条接到方式还没做好，暂不能绑定。");
    assert!(!xai_grok.reason.contains("仅支持预览"));
}

#[test]
fn cursor_target_uses_no_writer_reason_not_source_copy() {
    for source in [
        AdapterSourceProduct::KimiCodeMembership,
        AdapterSourceProduct::AnthropicApi,
        AdapterSourceProduct::OpenaiApi,
        AdapterSourceProduct::XaiApi,
        AdapterSourceProduct::GlmCodingPlan,
        AdapterSourceProduct::DeepseekApi,
        AdapterSourceProduct::CodexChatGptSubscription,
        AdapterSourceProduct::ClaudeSubscription,
        AdapterSourceProduct::XaiGrokSubscription,
        AdapterSourceProduct::Other,
    ] {
        let credential = match source {
            AdapterSourceProduct::CodexChatGptSubscription
            | AdapterSourceProduct::ClaudeSubscription
            | AdapterSourceProduct::XaiGrokSubscription => AdapterCredentialClass::OauthOther,
            AdapterSourceProduct::Other => AdapterCredentialClass::Unknown,
            _ => AdapterCredentialClass::ApiKey,
        };
        let decision =
            decide_adapter_capability(source, credential, AgentId::Cursor).public_surface();
        assert_eq!(decision.route, AdapterRoute::Unsupported, "{source:?}");
        assert!(!decision.can_apply, "{source:?}");
        assert_eq!(decision.reason, AGENT_NO_WRITER_REASON, "{source:?}");
        assert!(
            !decision.reason.contains("仅支持预览"),
            "Cursor must not use source-product copy: {}",
            decision.reason
        );
    }
}

#[test]
fn workbuddy_hears_no_protocol_so_codex_login_stays_closed() {
    let decision = decide_adapter_capability(
        AdapterSourceProduct::CodexChatGptSubscription,
        AdapterCredentialClass::OauthAuthJson,
        AgentId::WorkBuddy,
    )
    .public_surface();
    assert_eq!(decision.route, AdapterRoute::Unsupported);
    assert!(!decision.can_apply);
    assert_eq!(decision.reason, PROTOCOL_MISMATCH_REASON);
}

#[test]
fn kimi_to_grok_is_an_open_native_endpoint() {
    let decision = decide_adapter_capability(
        AdapterSourceProduct::KimiCodeMembership,
        AdapterCredentialClass::ApiKey,
        AgentId::Grok,
    )
    .public_surface();
    assert_eq!(decision.route, AdapterRoute::NativeEndpoint);
    assert!(decision.can_apply);
    assert_eq!(decision.rule_id, Some("kimi-membership-to-grok-v1"));
}

#[test]
fn grok_subscription_to_claude_uses_xai_responses_oauth() {
    let decision = decide_adapter_capability(
        AdapterSourceProduct::XaiGrokSubscription,
        AdapterCredentialClass::OauthOther,
        AgentId::Claude,
    )
    .public_surface();
    assert_eq!(decision.route, AdapterRoute::LocalBridge);
    assert!(decision.can_apply);
    assert_eq!(decision.rule_id, Some("grok-subscription-to-claude-v1"));
    assert_eq!(decision.reason, GROK_SUBSCRIPTION_TO_CLAUDE_REASON);
    assert_eq!(
        decision.transport,
        AdapterUpstreamTransport::XaiResponsesOauth
    );
    assert_eq!(
        decision.protocol,
        Some(AdapterTargetProtocol::AnthropicMessages)
    );
}

#[test]
fn grok_subscription_to_codex_is_open_local_bridge() {
    let decision = decide_adapter_capability(
        AdapterSourceProduct::XaiGrokSubscription,
        AdapterCredentialClass::OauthOther,
        AgentId::Codex,
    )
    .public_surface();
    assert_eq!(decision.route, AdapterRoute::LocalBridge);
    assert!(decision.can_apply);
    assert_eq!(decision.rule_id, Some("grok-subscription-to-codex-v1"));
    assert_eq!(decision.reason, GROK_SUBSCRIPTION_TO_CODEX_REASON);
    assert_eq!(
        decision.transport,
        AdapterUpstreamTransport::XaiResponsesOauth
    );
    assert_eq!(
        decision.protocol,
        Some(AdapterTargetProtocol::OpenAiResponses)
    );
}

#[test]
fn codex_subscription_to_grok_kimi_dsh_is_open_local_bridge() {
    for (target, rule_id, reason) in [
        (
            AgentId::Grok,
            CODEX_SUBSCRIPTION_TO_GROK_RULE_ID,
            CODEX_SUBSCRIPTION_TO_GROK_REASON,
        ),
        (
            AgentId::Kimi,
            CODEX_SUBSCRIPTION_TO_KIMI_RULE_ID,
            CODEX_SUBSCRIPTION_TO_KIMI_REASON,
        ),
        (
            AgentId::Dsh,
            CODEX_SUBSCRIPTION_TO_DSH_RULE_ID,
            CODEX_SUBSCRIPTION_TO_DSH_REASON,
        ),
    ] {
        for credential in [
            AdapterCredentialClass::OauthAuthJson,
            AdapterCredentialClass::OauthOther,
        ] {
            let decision = decide_adapter_capability(
                AdapterSourceProduct::CodexChatGptSubscription,
                credential,
                target,
            )
            .public_surface();
            assert_eq!(decision.route, AdapterRoute::LocalBridge, "{target:?}");
            assert!(decision.can_apply, "{target:?}");
            assert_eq!(decision.rule_id, Some(rule_id), "{target:?}");
            assert_eq!(decision.reason, reason, "{target:?}");
            assert_eq!(
                decision.transport,
                AdapterUpstreamTransport::CodexResponsesOauth,
                "{target:?}"
            );
            let expected_protocol = if target == AgentId::Grok {
                AdapterTargetProtocol::OpenAiResponses
            } else {
                AdapterTargetProtocol::OpenAiChatCompletions
            };
            assert_eq!(decision.protocol, Some(expected_protocol), "{target:?}");
            assert!(!decision.reason.contains("实验"));
            assert!(!decision.reason.contains("未验证"));
        }
    }
}

#[test]
fn grok_subscription_to_kimi_and_dsh_stay_closed_with_clear_reasons() {
    let kimi = decide_adapter_capability(
        AdapterSourceProduct::XaiGrokSubscription,
        AdapterCredentialClass::OauthOther,
        AgentId::Kimi,
    )
    .public_surface();
    assert_eq!(kimi.route, AdapterRoute::Unsupported);
    assert!(!kimi.can_apply);
    assert_eq!(kimi.reason, GROK_SUBSCRIPTION_TO_KIMI_REASON);
    assert!(
        GROK_SUBSCRIPTION_TO_KIMI_REASON.contains("Codex 官方登录"),
        "closed copy must name Codex official login as the supported upstream"
    );

    let dsh = decide_adapter_capability(
        AdapterSourceProduct::XaiGrokSubscription,
        AdapterCredentialClass::OauthOther,
        AgentId::Dsh,
    )
    .public_surface();
    assert_eq!(dsh.route, AdapterRoute::Unsupported);
    assert!(!dsh.can_apply);
    assert_eq!(dsh.reason, GROK_SUBSCRIPTION_TO_DSH_REASON);
    assert!(
        GROK_SUBSCRIPTION_TO_DSH_REASON.contains("Codex 官方登录"),
        "closed copy must name Codex official login as the supported upstream"
    );
}

#[test]
fn local_bridge_matrix_cells_are_exactly_the_catalog() {
    let mut from_catalog: Vec<_> = LOCAL_BRIDGE_EDGES
        .iter()
        .map(|edge| edge.to_cell())
        .collect();
    let mut from_matrix: Vec<_> = ADAPTER_CAPABILITY_MATRIX
        .iter()
        .copied()
        .filter(|cell| cell.route == AdapterRoute::LocalBridge)
        .collect();
    from_catalog.sort_by_key(|cell| (cell.rule_id, format!("{:?}", cell.key.credential)));
    from_matrix.sort_by_key(|cell| (cell.rule_id, format!("{:?}", cell.key.credential)));
    assert_eq!(
        from_catalog, from_matrix,
        "every LocalBridge matrix cell must be LocalBridgeEdge::to_cell(); do not hand-write cells"
    );
}

#[test]
fn claude_subscription_to_codex_is_preview_local_bridge_from_catalog() {
    let cell = lookup_adapter_capability(&CLAUDE_CODEX_EDGE.to_cell().key).expect("cell");
    assert_eq!(cell.rule_id, CLAUDE_SUBSCRIPTION_TO_CODEX_RULE_ID);
    assert_eq!(
        cell.key.transport,
        AdapterUpstreamTransport::LocalBridgeAnthropicMessages
    );
    assert_eq!(cell.key.protocol, AdapterTargetProtocol::OpenAiResponses);
    assert!(!cell.can_apply);
    assert_eq!(cell.gates, AdapterCapabilityGates::all_closed());
    assert_eq!(cell.reason, CLAUDE_SUBSCRIPTION_TO_CODEX_REASON);

    let decision = decide_adapter_capability(
        AdapterSourceProduct::ClaudeSubscription,
        AdapterCredentialClass::OauthOther,
        AgentId::Codex,
    )
    .public_surface();
    assert_eq!(decision.route, AdapterRoute::LocalBridge);
    assert_eq!(decision.support, AdapterSupport::Experimental);
    assert!(!decision.can_apply);
    assert_eq!(decision.gate_kind, AdapterGateKind::PreviewOnly);
    assert_eq!(decision.rule_id, Some(CLAUDE_SUBSCRIPTION_TO_CODEX_RULE_ID));
    assert_eq!(decision.reason, CLAUDE_SUBSCRIPTION_TO_CODEX_REASON);
    assert_eq!(
        adapter_maturity_from_decision(&decision),
        AdapterMaturity::Preview
    );
    assert!(
        !decision.reason.contains("产品不做"),
        "Claude → Codex is ③-open; reason must not say product-closed"
    );
}
