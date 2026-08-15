use super::*;
use crate::models::{
    decide_adapter_capability, AdapterCredentialClass, AdapterSourceProduct, AdapterSupport,
    TicketSurface,
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

    let kimi = agent_bind_capability(AgentId::Kimi);
    assert_eq!(kimi.accepts, &[AgentAccept::OpenAiChat]);
    assert!(kimi.writer);

    let cursor = agent_bind_capability(AgentId::Cursor);
    assert!(cursor.accepts.is_empty());
    assert!(!cursor.writer);

    let workbuddy = agent_bind_capability(AgentId::WorkBuddy);
    assert_eq!(workbuddy.accepts, &[AgentAccept::WorkBuddyModelsJson]);
    assert!(
        workbuddy.writer,
        "workbuddy.rs ConfigWrite is Full / write_config projects models.json"
    );
    assert!(
        AgentAccept::WorkBuddyModelsJson.hears().is_empty(),
        "WorkBuddy slot has no documented ticket protocol"
    );

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
        assert!(reason.contains("无配置写入"));
    }
}

#[test]
fn kimi_ticket_and_grok_share_chat_but_have_no_verified_edge() {
    let speaks = TicketSurface::KimiCodeMembership.speaks();
    assert!(speaks_intersect_accepts(
        speaks,
        agent_bind_capability(AgentId::Grok).accepts
    ));
    assert_eq!(
        unsupported_reason_for_target(AgentId::Grok, speaks),
        SAME_PROTOCOL_NO_EDGE_REASON
    );
    assert!(SAME_PROTOCOL_NO_EDGE_REASON.contains("同协议但无已验证的边"));
}

#[test]
fn anthropic_ticket_and_grok_are_protocol_mismatch() {
    let speaks = TicketSurface::AnthropicApi.speaks();
    assert!(!speaks_intersect_accepts(
        speaks,
        agent_bind_capability(AgentId::Grok).accepts
    ));
    assert_eq!(
        unsupported_reason_for_target(AgentId::Grok, speaks),
        PROTOCOL_MISMATCH_REASON
    );
    assert!(PROTOCOL_MISMATCH_REASON.contains("协议不通"));
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
    assert!(!kimi_grok.can_apply);
    assert_eq!(kimi_grok.reason, SAME_PROTOCOL_NO_EDGE_REASON);
    assert!(!kimi_grok.reason.contains("仅支持预览"));
}
