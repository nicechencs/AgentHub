//! First-class Agent bind-entry registry: `accepts[]` + `writer`.
//!
//! Compile-time table `AgentId → { accepts, writer }` used by the planner to
//! explain why a bind is infeasible. **This table never opens `can_apply`.**
//! Writable routes still come only from an explicit matrix cell ∩ plan
//! `write_gate`. See [connection-binding-model.md] §2.2 / §6.2.

use serde::{Deserialize, Serialize};

use crate::models::{AgentId, TicketProtocol};

/// Target has no live-config writer and cannot be a bind sink (e.g. Cursor).
pub const AGENT_NO_WRITER_REASON: &str = "这个工具不能写入配置，接不上。";

/// `票.speaks ∩ agent.accepts` is empty.
pub const PROTOCOL_MISMATCH_REASON: &str = "这份登录接不到这个工具。";

/// Protocols overlap, but the graph has no verified edge for this pair.
pub const SAME_PROTOCOL_NO_EDGE_REASON: &str = "这条接法还没做好，现在接不上。";

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
    /// Grok `config.toml` (`api_backend` is `responses` | `chat_completions` | `messages`).
    OpenAiChatToml,
    /// OpenAI Chat Completions (Kimi `config.toml`).
    OpenAiChat,
    /// WorkBuddy `models.json` model-list slot (`write_config` in workbuddy.rs).
    WorkBuddyModelsJson,
    /// DSH home-level official LLM plugin slot (`write_config` in dsh.rs, Partial).
    DshLlmPluginSlot,
    /// ZCode `~/.zcode/v2/config.json` catalog row (`provider` map, not exclusive live).
    ZcodeV2ProviderSlot,
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
            Self::OpenAiChatToml => &[
                TicketProtocol::OpenaiResponses,
                TicketProtocol::OpenaiChat,
                TicketProtocol::AnthropicMessages,
            ],
            Self::OpenAiChat => &[TicketProtocol::OpenaiChat],
            // workbuddy.rs writes models.json; ProviderPresets is unsupported
            // and no ticket wire protocol is documented for that slot.
            Self::WorkBuddyModelsJson => &[],
            // dsh.rs writes the official DeepSeek Chat Completions plugin row.
            Self::DshLlmPluginSlot => &[TicketProtocol::OpenaiChat],
            // zcode.rs upserts one provider row with a model list; edges still need matrix cells.
            Self::ZcodeV2ProviderSlot => &[
                TicketProtocol::AnthropicMessages,
                TicketProtocol::OpenaiChat,
            ],
        }
    }
}

/// How apply occupies the agent's live config.
///
/// Hub still keeps one current binding per agent; this only describes the
/// on-disk write so callers do not assume "current" means "the only live row".
/// Wire names (`exclusive` / `namedSlots` / `catalogAppend`) are projected on
/// the agent catalog so UI can label add / write without matching AgentId.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum LiveOccupancy {
    /// Replace the live credential family (Claude env, Codex `auth.json`).
    #[default]
    Exclusive,
    /// Merge one finite named slot; siblings stay (Pi).
    NamedSlots,
    /// Upsert one unbounded catalog row; siblings stay (ZCode providers, WorkBuddy models).
    CatalogAppend,
}

/// Bind-entry registration for one Agent. Exhaustive via [`agent_bind_capability`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentBindCapability {
    pub accepts: &'static [AgentAccept],
    /// Whether this Agent can write live config. `true` means an account /
    /// config writer exists — **not** that a cross-Agent edge is open.
    pub writer: bool,
    pub occupancy: LiveOccupancy,
}

/// Compile-time `AgentId → { accepts, writer }`. New Agent: register here first.
pub const fn agent_bind_capability(id: AgentId) -> AgentBindCapability {
    match id {
        AgentId::Claude => AgentBindCapability {
            accepts: &[AgentAccept::AnthropicMessagesEnv],
            writer: true,
            occupancy: LiveOccupancy::Exclusive,
        },
        AgentId::Codex => AgentBindCapability {
            accepts: &[AgentAccept::OpenAiResponses],
            writer: true,
            occupancy: LiveOccupancy::Exclusive,
        },
        AgentId::Pi => AgentBindCapability {
            accepts: &[
                AgentAccept::PiProviderSlot,
                AgentAccept::PiAnthropicOauthSlot,
                AgentAccept::PiCodexOauthSlot,
                AgentAccept::PiXaiOauthSlot,
            ],
            writer: true,
            occupancy: LiveOccupancy::NamedSlots,
        },
        AgentId::Grok => AgentBindCapability {
            accepts: &[AgentAccept::OpenAiChatToml],
            // Account / toml writer exists; this does not open a cross-Agent edge.
            writer: true,
            occupancy: LiveOccupancy::Exclusive,
        },
        AgentId::Kimi => AgentBindCapability {
            accepts: &[AgentAccept::OpenAiChat],
            writer: true,
            occupancy: LiveOccupancy::Exclusive,
        },
        AgentId::Cursor => AgentBindCapability {
            accepts: &[],
            writer: false,
            occupancy: LiveOccupancy::Exclusive,
        },
        // adapters/workbuddy.rs: ConfigWrite = Full, `write_config` merges models.json by id.
        AgentId::WorkBuddy => AgentBindCapability {
            accepts: &[AgentAccept::WorkBuddyModelsJson],
            writer: true,
            occupancy: LiveOccupancy::CatalogAppend,
        },
        // adapters/dsh.rs: ConfigWrite = Partial, write_config projects the
        // official LLM plugin row. Writer exists; edges still come from the matrix.
        AgentId::Dsh => AgentBindCapability {
            accepts: &[AgentAccept::DshLlmPluginSlot],
            writer: true,
            occupancy: LiveOccupancy::NamedSlots,
        },
        // adapters/zcode.rs: ConfigWrite = Partial, catalog-append one provider row.
        AgentId::Zcode => AgentBindCapability {
            accepts: &[AgentAccept::ZcodeV2ProviderSlot],
            writer: true,
            occupancy: LiveOccupancy::CatalogAppend,
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
