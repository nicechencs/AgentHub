use super::*;
use crate::models::{
    decide_adapter_capability, AdapterCredentialClass, AdapterRoute, AdapterSourceProduct,
    AdapterSupport, TicketSurface,
};

#[test]
fn table_registers_every_agent_accepts_and_writer() {
    let claude = agent_bind_capability(AgentId::Claude);
    assert_eq!(claude.accepts, &[AgentAccept::AnthropicMessagesEnv]);
    assert!(claude.writer);
    assert_eq!(
        AgentAccept::AnthropicMessagesEnv.hears(),
        &[TicketProtocol::AnthropicMessages]
    );

    let codex = agent_bind_capability(AgentId::Codex);
    assert_eq!(codex.accepts, &[AgentAccept::OpenAiResponses]);
    assert!(codex.writer);

    let pi = agent_bind_capability(AgentId::Pi);
    assert_eq!(
        pi.accepts,
        &[
            AgentAccept::PiProviderSlot,
            AgentAccept::PiAnthropicOauthSlot,
            AgentAccept::PiCodexOauthSlot,
            AgentAccept::PiXaiOauthSlot,
        ]
    );
    assert!(pi.writer);
    assert_eq!(
        AgentAccept::PiProviderSlot.hears(),
        &[
            TicketProtocol::AnthropicMessages,
            TicketProtocol::OpenaiChat
        ]
    );
    assert_eq!(
        AgentAccept::PiAnthropicOauthSlot.hears(),
        &[TicketProtocol::AnthropicPkce]
    );
    assert_eq!(
        AgentAccept::PiCodexOauthSlot.hears(),
        &[TicketProtocol::OpenaiCodexPkce]
    );
    assert_eq!(
        AgentAccept::PiXaiOauthSlot.hears(),
        &[TicketProtocol::XaiDeviceCode]
    );

    let grok = agent_bind_capability(AgentId::Grok);
    assert_eq!(grok.accepts, &[AgentAccept::OpenAiChatToml]);
    assert!(
        grok.writer,
        "Grok has a toml/account writer; that is not a cross-Agent edge"
    );
    assert_eq!(
        AgentAccept::OpenAiChatToml.hears(),
        &[
            TicketProtocol::OpenaiResponses,
            TicketProtocol::OpenaiChat,
            TicketProtocol::AnthropicMessages
        ]
    );

    let kimi = agent_bind_capability(AgentId::Kimi);
    assert_eq!(kimi.accepts, &[AgentAccept::OpenAiChat]);
    assert!(kimi.writer);
    assert_eq!(
        AgentAccept::OpenAiChat.hears(),
        &[TicketProtocol::OpenaiChat]
    );

    let cursor = agent_bind_capability(AgentId::Cursor);
    assert!(cursor.accepts.is_empty());
    assert!(!cursor.writer);

    let workbuddy = agent_bind_capability(AgentId::WorkBuddy);
    assert_eq!(workbuddy.accepts, &[AgentAccept::WorkBuddyModelsJson]);
    assert!(
        workbuddy.writer,
        "workbuddy.rs ConfigWrite is Partial / write_config projects models.json"
    );
    assert_eq!(
        AgentAccept::WorkBuddyModelsJson.hears(),
        &[TicketProtocol::OpenaiChat]
    );

    let zcode = agent_bind_capability(AgentId::Zcode);
    assert_eq!(zcode.accepts, &[AgentAccept::ZcodeV2ProviderSlot]);
    assert!(zcode.writer);
    assert_eq!(zcode.occupancy, LiveOccupancy::CatalogAppend);

    let dsh = agent_bind_capability(AgentId::Dsh);
    assert_eq!(dsh.accepts, &[AgentAccept::DshLlmPluginSlot]);
    assert!(
        dsh.writer,
        "dsh.rs ConfigWrite is Partial / write_config projects the official LLM plugin"
    );
    assert_eq!(
        AgentAccept::DshLlmPluginSlot.hears(),
        &[TicketProtocol::OpenaiChat]
    );
}

#[test]
fn cursor_reason_is_no_writer_for_any_ticket_speaks() {
    for surface in [
        TicketSurface::KimiCodeMembership,
        TicketSurface::AnthropicApi,
        TicketSurface::OpenaiApi,
        TicketSurface::XaiApi,
        TicketSurface::GlmCodingPlan,
        TicketSurface::DeepseekApi,
        TicketSurface::CodexChatgptSubscription,
        TicketSurface::ClaudeSubscription,
        TicketSurface::GrokXaiSubscription,
        TicketSurface::Unknown,
    ] {
        let reason = unsupported_reason_for_target(AgentId::Cursor, surface.speaks());
        assert_eq!(reason, AGENT_NO_WRITER_REASON, "{surface:?}");
        assert!(reason.contains("不能写入配置"));
    }
}

#[test]
fn kimi_ticket_and_grok_use_the_verified_native_edge() {
    let speaks = TicketSurface::KimiCodeMembership.speaks();
    assert!(speaks_intersect_accepts(
        speaks,
        agent_bind_capability(AgentId::Grok).accepts
    ));
    let decision = crate::models::decide_adapter_capability(
        AdapterSourceProduct::KimiCodeMembership,
        crate::models::AdapterCredentialClass::ApiKey,
        AgentId::Grok,
    );
    assert_eq!(decision.route, AdapterRoute::NativeEndpoint);
    assert!(decision.can_apply);
}

#[test]
fn anthropic_ticket_and_grok_are_same_protocol_no_edge() {
    let speaks = TicketSurface::AnthropicApi.speaks();
    assert!(speaks_intersect_accepts(
        speaks,
        agent_bind_capability(AgentId::Grok).accepts
    ));
    assert_eq!(
        unsupported_reason_for_target(AgentId::Grok, speaks),
        SAME_PROTOCOL_NO_EDGE_REASON
    );
    assert_eq!(
        SAME_PROTOCOL_NO_EDGE_REASON,
        "这条接法还没做好，现在接不上。"
    );

    let decision = decide_adapter_capability(
        AdapterSourceProduct::AnthropicApi,
        AdapterCredentialClass::ApiKey,
        AgentId::Grok,
    );
    assert!(!decision.can_apply);
    assert_eq!(decision.rule_id, None);
}

#[test]
fn capability_table_never_opens_can_apply() {
    for target in AgentId::ALL {
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
            let reason =
                unsupported_reason_for_target(target, TicketSurface::from_product(source).speaks());
            assert!(
                reason == AGENT_NO_WRITER_REASON
                    || reason == PROTOCOL_MISMATCH_REASON
                    || reason == SAME_PROTOCOL_NO_EDGE_REASON
            );
        }
    }

    let cursor = decide_adapter_capability(
        AdapterSourceProduct::KimiCodeMembership,
        AdapterCredentialClass::ApiKey,
        AgentId::Cursor,
    );
    assert!(!cursor.can_apply);
    assert_eq!(cursor.support, AdapterSupport::Unsupported);
    assert_eq!(cursor.reason, AGENT_NO_WRITER_REASON);

    let kimi_grok = decide_adapter_capability(
        AdapterSourceProduct::KimiCodeMembership,
        AdapterCredentialClass::ApiKey,
        AgentId::Grok,
    );
    assert!(kimi_grok.can_apply);
    assert_eq!(kimi_grok.route, AdapterRoute::NativeEndpoint);
}

#[test]
fn live_occupancy_is_exhaustive_and_catalog_for_zcode() {
    for agent in AgentId::ALL {
        let _ = agent_bind_capability(agent).occupancy;
    }
    assert_eq!(
        agent_bind_capability(AgentId::Claude).occupancy,
        LiveOccupancy::Exclusive
    );
    assert_eq!(
        agent_bind_capability(AgentId::Pi).occupancy,
        LiveOccupancy::NamedSlots
    );
    assert_eq!(
        agent_bind_capability(AgentId::WorkBuddy).occupancy,
        LiveOccupancy::CatalogAppend
    );
    assert_eq!(
        agent_bind_capability(AgentId::Zcode).occupancy,
        LiveOccupancy::CatalogAppend
    );
}
