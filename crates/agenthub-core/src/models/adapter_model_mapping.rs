//! Model id mapping tables for Adapter routes.
//!
//! Kimi → Pi `config_sync` apply reads the default / explicit mapping.
//! Anthropic → Pi allows passthrough and does not invent a model id.
//! Other apply/bridge paths may still ignore these tables. Missing source
//! models fail closed unless the table opts into passthrough.
//!
//! Reserved for:
//! - existing Kimi Code membership paths (Claude / Codex / Pi)
//! - Anthropic API Key → Pi
//! - future Codex subscription → Claude Code (empty until gates open)

use super::adapter_capability_matrix::{AdapterSourceProduct, AdapterTargetProtocol};
use super::AgentId;

/// One source-model → target-model mapping row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterModelMapEntry {
    pub source_model: &'static str,
    pub target_model: &'static str,
    pub notes: Option<&'static str>,
}

/// Result of resolving a source model against a mapping table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterModelMapResult {
    /// Explicit or default target model id (static table data).
    Mapped(&'static str),
    /// Table opted into passthrough; caller must keep the original source id.
    Passthrough,
    /// No mapping and passthrough disabled — fail closed.
    Missing,
}

/// Named mapping table scoped to one source product and target agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterModelMappingTable {
    pub id: &'static str,
    pub source: AdapterSourceProduct,
    pub target: AgentId,
    pub target_protocol: AdapterTargetProtocol,
    /// Default model when the source does not pin one.
    pub default_target_model: Option<&'static str>,
    pub entries: &'static [AdapterModelMapEntry],
    /// When true, unknown non-empty source models yield [`AdapterModelMapResult::Passthrough`].
    pub allow_passthrough: bool,
}

impl AdapterModelMappingTable {
    /// Resolve a source model id against this table.
    pub fn map_model(&self, source_model: &str) -> AdapterModelMapResult {
        let needle = source_model.trim();
        if needle.is_empty() {
            return match self.default_target_model {
                Some(model) => AdapterModelMapResult::Mapped(model),
                None => AdapterModelMapResult::Missing,
            };
        }
        if let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.source_model.eq_ignore_ascii_case(needle))
        {
            return AdapterModelMapResult::Mapped(entry.target_model);
        }
        if self.allow_passthrough {
            return AdapterModelMapResult::Passthrough;
        }
        AdapterModelMapResult::Missing
    }

    pub fn has_explicit_mapping(&self, source_model: &str) -> bool {
        let needle = source_model.trim();
        self.entries
            .iter()
            .any(|entry| entry.source_model.eq_ignore_ascii_case(needle))
    }
}

const KIMI_CLAUDE_MODELS: &[AdapterModelMapEntry] = &[
    AdapterModelMapEntry {
        source_model: "kimi-k2.5",
        target_model: "kimi-k2.5",
        notes: Some("Kimi Anthropic-compatible default"),
    },
    AdapterModelMapEntry {
        source_model: "kimi-for-coding",
        target_model: "kimi-k2.5",
        notes: Some("Alias used by some Kimi coding presets"),
    },
];

const KIMI_CODEX_MODELS: &[AdapterModelMapEntry] = &[AdapterModelMapEntry {
    source_model: "kimi-k2.5",
    target_model: "kimi-k2.5",
    notes: Some("Local bridge presents the same model id to Codex"),
}];

const KIMI_PI_MODELS: &[AdapterModelMapEntry] = &[AdapterModelMapEntry {
    source_model: "kimi-k2.5",
    target_model: "kimi-k2.5",
    notes: Some("Pi kimi-for-coding provider model slot"),
}];

/// Anthropic / OpenAI / xAI → Pi do not rewrite model ids; callers may passthrough or omit.
const ANTHROPIC_PI_MODELS: &[AdapterModelMapEntry] = &[];
const OPENAI_PI_MODELS: &[AdapterModelMapEntry] = &[];
const XAI_PI_MODELS: &[AdapterModelMapEntry] = &[];

const DEEPSEEK_DSH_MODELS: &[AdapterModelMapEntry] = &[
    AdapterModelMapEntry {
        source_model: "deepseek-v4-flash",
        target_model: "deepseek-v4-flash",
        notes: Some("DSH official default"),
    },
    AdapterModelMapEntry {
        source_model: "deepseek-chat",
        target_model: "deepseek-chat",
        notes: Some("Official Chat Completions alias"),
    },
];

/// Future Codex → Claude table: structure only, no active mappings.
const CODEX_CLAUDE_MODELS: &[AdapterModelMapEntry] = &[];

/// All known mapping tables. Lookup is fail-closed when no table matches.
pub const ADAPTER_MODEL_MAPPING_TABLES: &[AdapterModelMappingTable] = &[
    AdapterModelMappingTable {
        id: "kimi-membership-claude-v1",
        source: AdapterSourceProduct::KimiCodeMembership,
        target: AgentId::Claude,
        target_protocol: AdapterTargetProtocol::AnthropicMessages,
        default_target_model: Some("kimi-k2.5"),
        entries: KIMI_CLAUDE_MODELS,
        allow_passthrough: false,
    },
    AdapterModelMappingTable {
        id: "kimi-membership-codex-v1",
        source: AdapterSourceProduct::KimiCodeMembership,
        target: AgentId::Codex,
        target_protocol: AdapterTargetProtocol::OpenAiResponses,
        default_target_model: Some("kimi-k2.5"),
        entries: KIMI_CODEX_MODELS,
        allow_passthrough: false,
    },
    AdapterModelMappingTable {
        id: "kimi-membership-pi-v1",
        source: AdapterSourceProduct::KimiCodeMembership,
        target: AgentId::Pi,
        target_protocol: AdapterTargetProtocol::PiProviderConfig,
        default_target_model: Some("kimi-k2.5"),
        entries: KIMI_PI_MODELS,
        allow_passthrough: false,
    },
    AdapterModelMappingTable {
        id: "anthropic-api-pi-v1",
        source: AdapterSourceProduct::AnthropicApi,
        target: AgentId::Pi,
        target_protocol: AdapterTargetProtocol::PiProviderConfig,
        default_target_model: None,
        entries: ANTHROPIC_PI_MODELS,
        allow_passthrough: true,
    },
    AdapterModelMappingTable {
        id: "openai-api-pi-v1",
        source: AdapterSourceProduct::OpenaiApi,
        target: AgentId::Pi,
        target_protocol: AdapterTargetProtocol::PiProviderConfig,
        default_target_model: None,
        entries: OPENAI_PI_MODELS,
        allow_passthrough: true,
    },
    AdapterModelMappingTable {
        id: "xai-api-pi-v1",
        source: AdapterSourceProduct::XaiApi,
        target: AgentId::Pi,
        target_protocol: AdapterTargetProtocol::PiProviderConfig,
        default_target_model: None,
        entries: XAI_PI_MODELS,
        allow_passthrough: true,
    },
    AdapterModelMappingTable {
        id: "codex-subscription-claude-v0",
        source: AdapterSourceProduct::CodexChatGptSubscription,
        target: AgentId::Claude,
        target_protocol: AdapterTargetProtocol::AnthropicMessages,
        default_target_model: None,
        entries: CODEX_CLAUDE_MODELS,
        allow_passthrough: false,
    },
    AdapterModelMappingTable {
        id: "deepseek-api-dsh-v1",
        source: AdapterSourceProduct::DeepseekApi,
        target: AgentId::Dsh,
        target_protocol: AdapterTargetProtocol::DshProviderConfig,
        default_target_model: Some("deepseek-v4-flash"),
        entries: DEEPSEEK_DSH_MODELS,
        allow_passthrough: true,
    },
];

pub fn find_adapter_model_mapping(
    source: AdapterSourceProduct,
    target: AgentId,
) -> Option<&'static AdapterModelMappingTable> {
    ADAPTER_MODEL_MAPPING_TABLES
        .iter()
        .find(|table| table.source == source && table.target == target)
}

/// Map a model for an explicit source/target pair.
///
/// Returns `Some` only for [`AdapterModelMapResult::Mapped`]. Passthrough and
/// missing both yield `None` so callers that need the original id must call
/// [`AdapterModelMappingTable::map_model`] directly.
pub fn map_adapter_model(
    source: AdapterSourceProduct,
    target: AgentId,
    source_model: &str,
) -> Option<&'static str> {
    match find_adapter_model_mapping(source, target)?.map_model(source_model) {
        AdapterModelMapResult::Mapped(model) => Some(model),
        AdapterModelMapResult::Passthrough | AdapterModelMapResult::Missing => None,
    }
}

#[cfg(test)]
mod tests {
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

        let anthropic_pi =
            find_adapter_model_mapping(AdapterSourceProduct::AnthropicApi, AgentId::Pi)
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
}
