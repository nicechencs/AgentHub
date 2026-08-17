//! First-class Agent bind-entry registry: `accepts[]` + `writer`.
//!
//! Compile-time table `AgentId → { accepts, writer }` used by the planner to
//! explain why a bind is infeasible. **This table never opens `can_apply`.**
//! Writable routes still come only from an explicit matrix cell ∩ plan
//! `write_gate`. See [connection-binding-model.md] §2.2 / §6.2.

use crate::models::{AgentId, TicketProtocol};

/// Target has no live-config writer and cannot be a bind sink (e.g. Cursor).
pub const AGENT_NO_WRITER_REASON: &str = "该 Agent 无配置写入能力，不能作为绑定落点";

/// `票.speaks ∩ agent.accepts` is empty.
pub const PROTOCOL_MISMATCH_REASON: &str =
    "协议不通：登录所说的上游协议与该 Agent 所听的入口没有交集。";

/// Protocols overlap, but the graph has no verified edge for this pair.
pub const SAME_PROTOCOL_NO_EDGE_REASON: &str =
    "同协议但无已验证的边：登录与该 Agent 入口相通，但协议图上尚无已验证的适配边。";

/// What an Agent listens to: a wire protocol and/or a named config slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentAccept {
    /// Anthropic Messages via Claude env / settings slot (`ANTHROPIC_AUTH_TOKEN`).
    AnthropicMessagesEnv,
    /// OpenAI Responses (Codex live config).
    OpenAiResponses,
    /// Pi `models.json` provider slot (reshape target, not a wire protocol).
    PiProviderSlot,
    /// Pi Anthropic OAuth / subscription login slot.
    PiAnthropicOauthSlot,
    /// Pi Codex OAuth / subscription login slot.
    PiCodexOauthSlot,
    /// Pi xAI OAuth / subscription login slot.
    PiXaiOauthSlot,
    /// OpenAI Chat Completions via Grok `config.toml`.
    OpenAiChatToml,
    /// OpenAI Chat Completions (Kimi `config.toml`).
    OpenAiChat,
    /// WorkBuddy `models.json` model-list slot (`write_config` in workbuddy.rs).
    WorkBuddyModelsJson,
    /// DSH home-level official LLM plugin slot (`write_config` in dsh.rs, Partial).
    DshLlmPluginSlot,
}

impl AgentAccept {
    /// Ticket protocols this accept can hear. Slots list the protocols they
    /// can be filled by; an empty slice means no documented ticket protocol.
    pub const fn hears(self) -> &'static [TicketProtocol] {
        match self {
            Self::AnthropicMessagesEnv => &[TicketProtocol::AnthropicMessages],
            Self::OpenAiResponses => &[TicketProtocol::OpenaiResponses],
            Self::PiProviderSlot => &[
                TicketProtocol::AnthropicMessages,
                TicketProtocol::OpenaiChat,
            ],
            Self::PiAnthropicOauthSlot => &[TicketProtocol::AnthropicPkce],
            Self::PiCodexOauthSlot => &[TicketProtocol::OpenaiCodexPkce],
            Self::PiXaiOauthSlot => &[TicketProtocol::XaiDeviceCode],
            Self::OpenAiChatToml | Self::OpenAiChat => &[TicketProtocol::OpenaiChat],
            // workbuddy.rs writes models.json; ProviderPresets is unsupported
            // and no ticket wire protocol is documented for that slot.
            Self::WorkBuddyModelsJson => &[],
            // dsh.rs writes the official DeepSeek Chat Completions plugin row.
            Self::DshLlmPluginSlot => &[TicketProtocol::OpenaiChat],
        }
    }
}

/// Bind-entry registration for one Agent. Exhaustive via [`agent_bind_capability`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentBindCapability {
    pub accepts: &'static [AgentAccept],
    /// Whether this Agent can write live config. `true` means an account /
    /// config writer exists — **not** that a cross-Agent edge is open.
    pub writer: bool,
}

/// Compile-time `AgentId → { accepts, writer }`. New Agent: register here first.
pub const fn agent_bind_capability(id: AgentId) -> AgentBindCapability {
    match id {
        AgentId::Claude => AgentBindCapability {
            accepts: &[AgentAccept::AnthropicMessagesEnv],
            writer: true,
        },
        AgentId::Codex => AgentBindCapability {
            accepts: &[AgentAccept::OpenAiResponses],
            writer: true,
        },
        AgentId::Pi => AgentBindCapability {
            accepts: &[
                AgentAccept::PiProviderSlot,
                AgentAccept::PiAnthropicOauthSlot,
                AgentAccept::PiCodexOauthSlot,
                AgentAccept::PiXaiOauthSlot,
            ],
            writer: true,
        },
        AgentId::Grok => AgentBindCapability {
            accepts: &[AgentAccept::OpenAiChatToml],
            // Account / toml writer exists; this does not open a cross-Agent edge.
            writer: true,
        },
        AgentId::Kimi => AgentBindCapability {
            accepts: &[AgentAccept::OpenAiChat],
            writer: true,
        },
        AgentId::Cursor => AgentBindCapability {
            accepts: &[],
            writer: false,
        },
        // adapters/workbuddy.rs: ConfigWrite = Full, `write_config` projects models.json.
        AgentId::WorkBuddy => AgentBindCapability {
            accepts: &[AgentAccept::WorkBuddyModelsJson],
            writer: true,
        },
        // adapters/dsh.rs: ConfigWrite = Partial, write_config projects the
        // official LLM plugin row. Writer exists; edges still come from the matrix.
        AgentId::Dsh => AgentBindCapability {
            accepts: &[AgentAccept::DshLlmPluginSlot],
            writer: true,
        },
    }
}

/// `票.speaks ∩ agent.accepts` — used only to pick an infeasible reason.
pub fn speaks_intersect_accepts(speaks: &[TicketProtocol], accepts: &[AgentAccept]) -> bool {
    accepts.iter().any(|accept| {
        accept
            .hears()
            .iter()
            .any(|heard| speaks.iter().any(|spoke| spoke == heard))
    })
}

/// Infeasible reason from the bind-entry table. Never implies `can_apply`.
pub fn unsupported_reason_for_target(target: AgentId, speaks: &[TicketProtocol]) -> &'static str {
    let cap = agent_bind_capability(target);
    if !cap.writer {
        return AGENT_NO_WRITER_REASON;
    }
    if speaks_intersect_accepts(speaks, cap.accepts) {
        SAME_PROTOCOL_NO_EDGE_REASON
    } else {
        PROTOCOL_MISMATCH_REASON
    }
}

#[cfg(test)]
mod tests;
