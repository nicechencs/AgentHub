//! Local-bridge edge registry — single declaration for overlapping rule fields.
//!
//! Matrix cells (`to_cell`) and `LIVE_BRIDGE_RULES` (rule_id / source / target /
//! transport / local protocol / default_model) both derive from this table.
//! Runtime-only projection metadata (profile names, slugs, base URLs, TOML)
//! stays in `adapter_bridge_service`. App Server remains a recorded closed
//! candidate here so the matrix has no handwritten LocalBridge leftover.

use super::*;

/// One local-bridge conversion edge. Identity + planning fields only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalBridgeEdge {
    pub rule_id: &'static str,
    pub source: AdapterSourceProduct,
    pub credential: AdapterCredentialClass,
    pub transport: AdapterUpstreamTransport,
    pub target: AgentId,
    pub protocol: AdapterTargetProtocol,
    pub version: &'static str,
    pub support: AdapterSupport,
    pub can_apply: bool,
    pub reason: &'static str,
    pub limitations: &'static [&'static str],
    pub verified_at: &'static str,
    pub gates: AdapterCapabilityGates,
    /// Model id presented on the local loopback / listed by `/v1/models`.
    /// Empty means the projection deliberately omits a default model.
    pub default_model: &'static str,
    /// RFC §7. Opening an edge is a later evidence step; keep closed here.
    pub multi_account: bool,
}

impl LocalBridgeEdge {
    pub const fn to_cell(self) -> AdapterCapabilityCell {
        AdapterCapabilityCell {
            key: AdapterCapabilityKey {
                source: self.source,
                credential: self.credential,
                transport: self.transport,
                target: self.target,
                protocol: self.protocol,
                version: self.version,
            },
            route: AdapterRoute::LocalBridge,
            support: self.support,
            can_apply: self.can_apply,
            reason: self.reason,
            limitations: self.limitations,
            rule_id: self.rule_id,
            verified_at: self.verified_at,
            gates: self.gates,
            multi_account: self.multi_account,
        }
    }
}

pub const KIMI_CODEX_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: "kimi-membership-to-codex-v1",
    source: AdapterSourceProduct::KimiCodeMembership,
    credential: AdapterCredentialClass::ApiKey,
    transport: AdapterUpstreamTransport::LocalBridgeChatCompletions,
    target: AgentId::Codex,
    protocol: AdapterTargetProtocol::OpenAiResponses,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: true,
    reason: "Kimi Code 会员接到 Codex 需要本机转发。",
    limitations: KIMI_CODEX_LIMITS,
    verified_at: VERIFIED_AT,
    gates: AdapterCapabilityGates::all_open(),
    default_model: "kimi-k2.5",
    multi_account: false,
};

pub const ANTHROPIC_CODEX_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: "anthropic-api-to-codex-v1",
    source: AdapterSourceProduct::AnthropicApi,
    credential: AdapterCredentialClass::ApiKey,
    transport: AdapterUpstreamTransport::LocalBridgeAnthropicMessages,
    target: AgentId::Codex,
    protocol: AdapterTargetProtocol::OpenAiResponses,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: true,
    reason: "这份 Anthropic API Key 接到 Codex 需要本机转发。",
    limitations: ANTHROPIC_CODEX_LIMITS,
    verified_at: VERIFIED_AT,
    gates: AdapterCapabilityGates::all_open(),
    default_model: "claude-sonnet-4-20250514",
    multi_account: false,
};

pub const OPENAI_CODEX_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: "openai-api-to-codex-v1",
    source: AdapterSourceProduct::OpenaiApi,
    credential: AdapterCredentialClass::ApiKey,
    transport: AdapterUpstreamTransport::LocalBridgeChatCompletions,
    target: AgentId::Codex,
    protocol: AdapterTargetProtocol::OpenAiResponses,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: true,
    reason: "这份 OpenAI API Key 接到 Codex 需要本机转发。",
    limitations: OPENAI_CODEX_LIMITS,
    verified_at: "2026-08-21",
    gates: AdapterCapabilityGates::all_open(),
    default_model: "gpt-4o",
    multi_account: false,
};

pub const OPENAI_CLAUDE_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: "openai-api-to-claude-v1",
    source: AdapterSourceProduct::OpenaiApi,
    credential: AdapterCredentialClass::ApiKey,
    transport: AdapterUpstreamTransport::LocalBridgeChatCompletions,
    target: AgentId::Claude,
    protocol: AdapterTargetProtocol::AnthropicMessages,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: true,
    reason: "这份 OpenAI 兼容登录接到 Claude 需要本机转发。",
    limitations: OPENAI_CLAUDE_LIMITS,
    verified_at: "2026-08-23",
    gates: AdapterCapabilityGates::all_open(),
    default_model: "gpt-4o",
    multi_account: false,
};

pub const OPENAI_GROK_BRIDGE_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: "openai-api-to-grok-bridge-v1",
    source: AdapterSourceProduct::OpenaiApi,
    credential: AdapterCredentialClass::ApiKey,
    transport: AdapterUpstreamTransport::LocalBridgeChatCompletions,
    target: AgentId::Grok,
    protocol: AdapterTargetProtocol::OpenAiResponses,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: true,
    reason: "这份 OpenAI 兼容登录接到 Grok 需要本机转发。",
    limitations: OPENAI_GROK_BRIDGE_LIMITS,
    verified_at: "2026-08-23",
    gates: AdapterCapabilityGates::all_open(),
    default_model: "gpt-4o",
    multi_account: false,
};

pub const GROK_CLAUDE_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: "grok-subscription-to-claude-v1",
    source: AdapterSourceProduct::XaiGrokSubscription,
    credential: AdapterCredentialClass::OauthOther,
    transport: AdapterUpstreamTransport::XaiResponsesOauth,
    target: AgentId::Claude,
    protocol: AdapterTargetProtocol::AnthropicMessages,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: true,
    reason: GROK_SUBSCRIPTION_TO_CLAUDE_REASON,
    limitations: GROK_CLAUDE_LIMITS,
    verified_at: "2026-08-15",
    gates: AdapterCapabilityGates::all_open(),
    default_model: "grok-4.5",
    multi_account: false,
};

pub const GROK_CODEX_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: "grok-subscription-to-codex-v1",
    source: AdapterSourceProduct::XaiGrokSubscription,
    credential: AdapterCredentialClass::OauthOther,
    transport: AdapterUpstreamTransport::XaiResponsesOauth,
    target: AgentId::Codex,
    protocol: AdapterTargetProtocol::OpenAiResponses,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: true,
    reason: GROK_SUBSCRIPTION_TO_CODEX_REASON,
    limitations: GROK_CODEX_LIMITS,
    verified_at: "2026-08-20",
    gates: AdapterCapabilityGates::all_open(),
    default_model: "grok-4.5",
    multi_account: false,
};

/// Closed App Server candidate. Not a live writer.
pub const CODEX_CLAUDE_APP_SERVER_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: "codex-subscription-to-claude-app-server-v0",
    source: AdapterSourceProduct::CodexChatGptSubscription,
    credential: AdapterCredentialClass::OauthAuthJson,
    transport: AdapterUpstreamTransport::CodexAppServer,
    target: AgentId::Claude,
    protocol: AdapterTargetProtocol::AnthropicMessages,
    version: "0",
    support: AdapterSupport::Experimental,
    can_apply: false,
    reason: CODEX_SUBSCRIPTION_TO_CLAUDE_CANDIDATE_REASON,
    limitations: CODEX_CLAUDE_LIMITS,
    verified_at: VERIFIED_AT,
    gates: AdapterCapabilityGates::all_closed(),
    default_model: "",
    multi_account: false,
};

pub const CODEX_CLAUDE_RESPONSES_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: "codex-subscription-to-claude-responses-v1",
    source: AdapterSourceProduct::CodexChatGptSubscription,
    credential: AdapterCredentialClass::OauthAuthJson,
    transport: AdapterUpstreamTransport::CodexResponsesOauth,
    target: AgentId::Claude,
    protocol: AdapterTargetProtocol::AnthropicMessages,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: true,
    reason: CODEX_SUBSCRIPTION_TO_CLAUDE_REASON,
    limitations: CODEX_CLAUDE_LIMITS,
    verified_at: "2026-08-15",
    gates: AdapterCapabilityGates::all_open(),
    default_model: "gpt-5.4",
    multi_account: false,
};

pub const CODEX_CLAUDE_OAUTH_OTHER_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: "codex-subscription-to-claude-responses-v1",
    source: AdapterSourceProduct::CodexChatGptSubscription,
    credential: AdapterCredentialClass::OauthOther,
    transport: AdapterUpstreamTransport::CodexResponsesOauth,
    target: AgentId::Claude,
    protocol: AdapterTargetProtocol::AnthropicMessages,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: true,
    reason: CODEX_SUBSCRIPTION_TO_CLAUDE_REASON,
    limitations: CODEX_CLAUDE_LIMITS,
    verified_at: "2026-08-15",
    gates: AdapterCapabilityGates::all_open(),
    default_model: "gpt-5.4",
    multi_account: false,
};

pub const CODEX_GROK_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: CODEX_SUBSCRIPTION_TO_GROK_RULE_ID,
    source: AdapterSourceProduct::CodexChatGptSubscription,
    credential: AdapterCredentialClass::OauthAuthJson,
    transport: AdapterUpstreamTransport::CodexResponsesOauth,
    target: AgentId::Grok,
    protocol: AdapterTargetProtocol::OpenAiResponses,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: true,
    reason: CODEX_SUBSCRIPTION_TO_GROK_REASON,
    limitations: CODEX_CHAT_LIMITS,
    verified_at: "2026-08-20",
    gates: AdapterCapabilityGates::all_open(),
    default_model: "",
    multi_account: false,
};

pub const CODEX_GROK_OAUTH_OTHER_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: CODEX_SUBSCRIPTION_TO_GROK_RULE_ID,
    source: AdapterSourceProduct::CodexChatGptSubscription,
    credential: AdapterCredentialClass::OauthOther,
    transport: AdapterUpstreamTransport::CodexResponsesOauth,
    target: AgentId::Grok,
    protocol: AdapterTargetProtocol::OpenAiResponses,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: true,
    reason: CODEX_SUBSCRIPTION_TO_GROK_REASON,
    limitations: CODEX_CHAT_LIMITS,
    verified_at: "2026-08-20",
    gates: AdapterCapabilityGates::all_open(),
    default_model: "",
    multi_account: false,
};

pub const CODEX_KIMI_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: CODEX_SUBSCRIPTION_TO_KIMI_RULE_ID,
    source: AdapterSourceProduct::CodexChatGptSubscription,
    credential: AdapterCredentialClass::OauthAuthJson,
    transport: AdapterUpstreamTransport::CodexResponsesOauth,
    target: AgentId::Kimi,
    protocol: AdapterTargetProtocol::OpenAiChatCompletions,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: true,
    reason: CODEX_SUBSCRIPTION_TO_KIMI_REASON,
    limitations: CODEX_CHAT_LIMITS,
    verified_at: "2026-08-20",
    gates: AdapterCapabilityGates::all_open(),
    default_model: "",
    multi_account: false,
};

pub const CODEX_KIMI_OAUTH_OTHER_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: CODEX_SUBSCRIPTION_TO_KIMI_RULE_ID,
    source: AdapterSourceProduct::CodexChatGptSubscription,
    credential: AdapterCredentialClass::OauthOther,
    transport: AdapterUpstreamTransport::CodexResponsesOauth,
    target: AgentId::Kimi,
    protocol: AdapterTargetProtocol::OpenAiChatCompletions,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: true,
    reason: CODEX_SUBSCRIPTION_TO_KIMI_REASON,
    limitations: CODEX_CHAT_LIMITS,
    verified_at: "2026-08-20",
    gates: AdapterCapabilityGates::all_open(),
    default_model: "",
    multi_account: false,
};

pub const CODEX_DSH_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: CODEX_SUBSCRIPTION_TO_DSH_RULE_ID,
    source: AdapterSourceProduct::CodexChatGptSubscription,
    credential: AdapterCredentialClass::OauthAuthJson,
    transport: AdapterUpstreamTransport::CodexResponsesOauth,
    target: AgentId::Dsh,
    protocol: AdapterTargetProtocol::OpenAiChatCompletions,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: true,
    reason: CODEX_SUBSCRIPTION_TO_DSH_REASON,
    limitations: CODEX_CHAT_LIMITS,
    verified_at: "2026-08-20",
    gates: AdapterCapabilityGates::all_open(),
    default_model: "",
    multi_account: false,
};

pub const CODEX_DSH_OAUTH_OTHER_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: CODEX_SUBSCRIPTION_TO_DSH_RULE_ID,
    source: AdapterSourceProduct::CodexChatGptSubscription,
    credential: AdapterCredentialClass::OauthOther,
    transport: AdapterUpstreamTransport::CodexResponsesOauth,
    target: AgentId::Dsh,
    protocol: AdapterTargetProtocol::OpenAiChatCompletions,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: true,
    reason: CODEX_SUBSCRIPTION_TO_DSH_REASON,
    limitations: CODEX_CHAT_LIMITS,
    verified_at: "2026-08-20",
    gates: AdapterCapabilityGates::all_open(),
    default_model: "",
    multi_account: false,
};

/// Claude subscription → Codex. Direction is ③-open; bind waits on fixtures / e2e.
pub const CLAUDE_CODEX_EDGE: LocalBridgeEdge = LocalBridgeEdge {
    rule_id: CLAUDE_SUBSCRIPTION_TO_CODEX_RULE_ID,
    source: AdapterSourceProduct::ClaudeSubscription,
    credential: AdapterCredentialClass::OauthOther,
    transport: AdapterUpstreamTransport::LocalBridgeAnthropicMessages,
    target: AgentId::Codex,
    protocol: AdapterTargetProtocol::OpenAiResponses,
    version: MATRIX_VERSION,
    support: AdapterSupport::Experimental,
    can_apply: false,
    reason: CLAUDE_SUBSCRIPTION_TO_CODEX_REASON,
    limitations: CLAUDE_CODEX_LIMITS,
    verified_at: "2026-08-22",
    gates: AdapterCapabilityGates::all_closed(),
    default_model: "claude-sonnet-4-20250514",
    multi_account: false,
};

/// Every local-bridge matrix cell. Order does not matter for lookup.
pub const LOCAL_BRIDGE_EDGES: &[LocalBridgeEdge] = &[
    KIMI_CODEX_EDGE,
    ANTHROPIC_CODEX_EDGE,
    OPENAI_CODEX_EDGE,
    OPENAI_CLAUDE_EDGE,
    OPENAI_GROK_BRIDGE_EDGE,
    GROK_CLAUDE_EDGE,
    GROK_CODEX_EDGE,
    CODEX_CLAUDE_APP_SERVER_EDGE,
    CODEX_CLAUDE_RESPONSES_EDGE,
    CODEX_CLAUDE_OAUTH_OTHER_EDGE,
    CODEX_GROK_EDGE,
    CODEX_GROK_OAUTH_OTHER_EDGE,
    CODEX_KIMI_EDGE,
    CODEX_KIMI_OAUTH_OTHER_EDGE,
    CODEX_DSH_EDGE,
    CODEX_DSH_OAUTH_OTHER_EDGE,
    CLAUDE_CODEX_EDGE,
];
