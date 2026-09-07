use super::*;

#[test]
fn kimi_paths_have_default_and_explicit_maps() {
    let claude =
        find_adapter_model_mapping(AdapterSourceProduct::KimiCodeMembership, AgentId::Claude)
            .expect("kimi→claude table");
    assert_eq!(
        claude.map_model("kimi-k2.5"),
        AdapterModelMapResult::Mapped("kimi-k2.5")
    );
    assert_eq!(
        claude.map_model(""),
        AdapterModelMapResult::Mapped("kimi-k2.5")
    );
    assert_eq!(
        claude.map_model("unknown-model"),
        AdapterModelMapResult::Missing
    );

    let codex =
        find_adapter_model_mapping(AdapterSourceProduct::KimiCodeMembership, AgentId::Codex)
            .expect("kimi→codex table");
    assert_eq!(codex.default_target_model, Some("kimi-k2.5"));
    assert_eq!(
        map_adapter_model(
            AdapterSourceProduct::KimiCodeMembership,
            AgentId::Codex,
            "kimi-k2.5"
        ),
        Some("kimi-k2.5")
    );

    let pi = find_adapter_model_mapping(AdapterSourceProduct::KimiCodeMembership, AgentId::Pi)
        .expect("kimi→pi table");
    assert_eq!(pi.map_model(""), AdapterModelMapResult::Mapped("kimi-k2.5"));
    assert_eq!(
        map_adapter_model(
            AdapterSourceProduct::KimiCodeMembership,
            AgentId::Pi,
            "kimi-k2.5"
        ),
        Some("kimi-k2.5")
    );

    let anthropic_pi = find_adapter_model_mapping(AdapterSourceProduct::AnthropicApi, AgentId::Pi)
        .expect("anthropic→pi table");
    assert!(anthropic_pi.allow_passthrough);
    assert!(anthropic_pi.default_target_model.is_none());
    assert_eq!(
        anthropic_pi.map_model("claude-sonnet-4-5"),
        AdapterModelMapResult::Passthrough
    );
    assert_eq!(
        map_adapter_model(
            AdapterSourceProduct::AnthropicApi,
            AgentId::Pi,
            "claude-sonnet-4-5"
        ),
        None
    );

    for source in [
        AdapterSourceProduct::OpenaiApi,
        AdapterSourceProduct::XaiApi,
    ] {
        let table = find_adapter_model_mapping(source, AgentId::Pi).expect("passthrough table");
        assert!(table.allow_passthrough);
        assert!(table.default_target_model.is_none());
    }

    let openai_codex = find_adapter_model_mapping(AdapterSourceProduct::OpenaiApi, AgentId::Codex)
        .expect("openai→codex table");
    assert_eq!(openai_codex.default_target_model, Some("gpt-4o"));
    assert_eq!(
        openai_codex.map_model(""),
        AdapterModelMapResult::Mapped("gpt-4o")
    );
    assert_eq!(
        map_adapter_model(AdapterSourceProduct::OpenaiApi, AgentId::Codex, "gpt-4o"),
        Some("gpt-4o")
    );
    assert_eq!(
        openai_codex.map_model("unknown-model"),
        AdapterModelMapResult::Missing
    );
}

#[test]
fn codex_to_claude_mapping_is_reserved_empty() {
    let table = find_adapter_model_mapping(
        AdapterSourceProduct::CodexChatGptSubscription,
        AgentId::Claude,
    )
    .expect("reserved table");
    assert!(table.entries.is_empty());
    assert!(table.default_target_model.is_none());
    assert_eq!(table.map_model("gpt-5"), AdapterModelMapResult::Missing);
    assert_eq!(
        map_adapter_model(
            AdapterSourceProduct::CodexChatGptSubscription,
            AgentId::Claude,
            "gpt-5"
        ),
        None
    );
}

#[test]
fn unknown_source_has_no_table() {
    assert!(find_adapter_model_mapping(AdapterSourceProduct::Other, AgentId::Claude).is_none());
}

#[test]
fn custom_openai_passthroughs_stealth_ox_alpha() {
    for target in [AgentId::Claude, AgentId::Codex, AgentId::Grok] {
        assert_eq!(
            map_edge_model(
                AdapterSourceProduct::OpenaiApi,
                target,
                "stealth/ox-alpha",
                true,
            ),
            AdapterModelMapResult::Passthrough
        );
    }
    assert_eq!(
        map_edge_model(
            AdapterSourceProduct::OpenaiApi,
            AgentId::Codex,
            "stealth/ox-alpha",
            false,
        ),
        AdapterModelMapResult::Missing
    );
}

#[test]
fn deepseek_to_dsh_has_default_and_passthrough() {
    let table = find_adapter_model_mapping(AdapterSourceProduct::DeepseekApi, AgentId::Dsh)
        .expect("deepseek→dsh table");
    assert_eq!(
        table.map_model(""),
        AdapterModelMapResult::Mapped("deepseek-v4-flash")
    );
    assert_eq!(
        table.map_model("deepseek-reasoner"),
        AdapterModelMapResult::Passthrough
    );
}

#[test]
fn subscription_fallback_json_lists_current_chatgpt_claude_and_grok() {
    assert_eq!(
        static_fallback_models(AdapterSourceProduct::CodexChatGptSubscription),
        &[
            "gpt-5.6-sol".to_string(),
            "gpt-5.6-terra".to_string(),
            "gpt-5.6-luna".to_string(),
            "gpt-5.6-cyber".to_string(),
            "gpt-5.6".to_string(),
        ]
    );
    assert_eq!(
        static_fallback_models(AdapterSourceProduct::ClaudeSubscription)[0],
        "claude-sonnet-5"
    );
    assert!(
        static_fallback_models(AdapterSourceProduct::ClaudeSubscription)
            .iter()
            .any(|model| model == "claude-opus-5")
    );
    assert_eq!(
        static_fallback_models(AdapterSourceProduct::XaiGrokSubscription),
        &[
            "grok-4.6".to_string(),
            "grok-4.5".to_string(),
            "grok-build-0.1".to_string()
        ]
    );
    assert!(static_fallback_models(AdapterSourceProduct::KimiCodeMembership).is_empty());
    assert_eq!(
        find_adapter_model_mapping(
            AdapterSourceProduct::CodexChatGptSubscription,
            AgentId::Grok,
        )
        .expect("codex→grok")
        .default_target_model,
        Some(static_fallback_models(AdapterSourceProduct::CodexChatGptSubscription)[0].as_str())
    );
    assert_eq!(
        find_adapter_model_mapping(AdapterSourceProduct::XaiGrokSubscription, AgentId::Claude)
            .expect("grok→claude")
            .default_target_model,
        Some(static_fallback_models(AdapterSourceProduct::XaiGrokSubscription)[0].as_str())
    );
}

#[test]
fn codex_to_grok_listed_models_are_dispatch_accepted() {
    let listed = list_local_bridge_models(
        AdapterSourceProduct::CodexChatGptSubscription,
        AgentId::Grok,
        Some("grok-4.5"),
    );
    assert!(!listed.is_empty());
    for model in &listed {
        assert!(
            !is_leftover_bridge_model(model),
            "leftover id must not be listed: {model}"
        );
    }
    for leftover in [
        "grok-4.5",
        "claude-sonnet-4",
        "kimi-k2.5",
        "deepseek-chat",
        "agenthub_codex_bridge",
    ] {
        assert!(
            !listed.iter().any(|model| model == leftover),
            "leftover {leftover} must not appear in {listed:?}"
        );
    }
    assert_eq!(
        listed,
        static_fallback_models(AdapterSourceProduct::CodexChatGptSubscription)
    );
    assert!(!listed.iter().any(|model| model == "gpt-5.4"));
    let table = find_adapter_model_mapping(
        AdapterSourceProduct::CodexChatGptSubscription,
        AgentId::Grok,
    )
    .expect("codex→grok table");
    assert_eq!(
        table.map_model("gpt-5.1-codex"),
        AdapterModelMapResult::Mapped("gpt-5.1-codex")
    );
    assert_eq!(
        table.map_model("gpt-5.6-cyber"),
        AdapterModelMapResult::Mapped("gpt-5.6-cyber")
    );
    assert_eq!(
        table.map_model("gpt-5.6"),
        AdapterModelMapResult::Mapped("gpt-5.6")
    );
    assert_eq!(
        table.map_model("gpt-5"),
        AdapterModelMapResult::Mapped("gpt-5")
    );
    assert_eq!(
        table.map_model(""),
        AdapterModelMapResult::Mapped("gpt-5.6-sol")
    );
}

#[test]
fn missing_mapping_lists_configured_default_or_empty() {
    assert!(list_local_bridge_models(AdapterSourceProduct::Other, AgentId::Grok, None).is_empty());
    assert!(
        list_local_bridge_models(AdapterSourceProduct::Other, AgentId::Grok, Some("")).is_empty()
    );
    assert_eq!(
        list_local_bridge_models(AdapterSourceProduct::Other, AgentId::Grok, Some("gpt-5.4")),
        vec!["gpt-5.4".to_string()]
    );
    assert_eq!(
        list_local_bridge_models(AdapterSourceProduct::Other, AgentId::Grok, Some("grok-4.5")),
        vec!["grok-4.5".to_string()]
    );
}

#[test]
fn codex_to_kimi_listed_models_are_dispatch_accepted() {
    let listed = list_local_bridge_models(
        AdapterSourceProduct::CodexChatGptSubscription,
        AgentId::Kimi,
        Some("gpt-5.4"),
    );
    assert!(!listed.is_empty());
    for model in &listed {
        assert!(
            !is_leftover_bridge_model(model),
            "leftover id must not be listed: {model}"
        );
    }
    let mut expected =
        static_fallback_models(AdapterSourceProduct::CodexChatGptSubscription).to_vec();
    expected.push("gpt-5.4".to_string());
    assert_eq!(listed, expected);
    let table = find_adapter_model_mapping(
        AdapterSourceProduct::CodexChatGptSubscription,
        AgentId::Kimi,
    )
    .expect("codex→kimi table");
    assert_eq!(
        table.map_model("gpt-5.1-codex"),
        AdapterModelMapResult::Mapped("gpt-5.1-codex")
    );
    assert_eq!(
        table.map_model("gpt-5.6-cyber"),
        AdapterModelMapResult::Mapped("gpt-5.6-cyber")
    );
    assert_eq!(
        table.map_model("gpt-5.6"),
        AdapterModelMapResult::Mapped("gpt-5.6")
    );
    assert_eq!(
        table.map_model("gpt-5"),
        AdapterModelMapResult::Mapped("gpt-5")
    );
}

#[test]
fn grok_and_claude_use_shared_fallback_catalog() {
    assert_eq!(
        list_local_bridge_models(
            AdapterSourceProduct::XaiGrokSubscription,
            AgentId::Claude,
            Some("grok-4.5")
        ),
        static_fallback_models(AdapterSourceProduct::XaiGrokSubscription)
    );
    assert_eq!(
        list_local_bridge_models(
            AdapterSourceProduct::XaiGrokSubscription,
            AgentId::Codex,
            None
        ),
        static_fallback_models(AdapterSourceProduct::XaiGrokSubscription)
    );
    assert_eq!(
        list_local_bridge_models(
            AdapterSourceProduct::ClaudeSubscription,
            AgentId::Codex,
            None
        ),
        static_fallback_models(AdapterSourceProduct::ClaudeSubscription)
    );
}
