//! Model id mapping tables for Adapter routes.
//!
//! Kimi → Pi `config_sync` apply reads the default / explicit mapping.
//! Anthropic → Pi allows passthrough and does not invent a model id.
//! Other apply/bridge paths may still ignore these tables. Missing source
//! models fail closed unless the table opts into passthrough.
//!
//! Reserved for:
//! - existing Kimi Code membership paths (Claude / Codex / Pi / Grok)
//! - Anthropic API Key → Pi
//! - OpenAI API → Grok / Codex
//! - Grok subscription → Claude Code
//! - Codex ChatGPT subscription → Grok (local GET /models)

use super::{AdapterSourceProduct, AdapterTargetProtocol, AgentId};

/// OpenRouter backup Chat Completions model. Do not invent other OpenRouter ids.
pub const OPENROUTER_BACKUP_MODEL: &str = "stealth/ox-alpha";

pub fn is_openrouter_backup_model(model: &str) -> bool {
    model.trim().eq_ignore_ascii_case(OPENROUTER_BACKUP_MODEL)
}

/// Official ChatGPT / Codex Responses 400 leftover / CN model ids.
/// Kept next to the listing table so `models` does not import `bridge`.
fn is_leftover_bridge_model(model: &str) -> bool {
    let model = model.trim();
    model.starts_with("grok-")
        || model.starts_with("claude-")
        || model.starts_with("kimi-")
        || model.starts_with("deepseek-")
        || (model.starts_with("agenthub_") && model.ends_with("_bridge"))
}

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
const GLM_PI_MODELS: &[AdapterModelMapEntry] = &[];
const DEEPSEEK_PI_MODELS: &[AdapterModelMapEntry] = &[];
const KIMI_GROK_MODELS: &[AdapterModelMapEntry] = &[AdapterModelMapEntry {
    source_model: "kimi-k2.5",
    target_model: "kimi-k2.5",
    notes: Some("Grok OpenAI Chat Completions model slot"),
}];
const OPENAI_GROK_MODELS: &[AdapterModelMapEntry] = &[AdapterModelMapEntry {
    source_model: "gpt-4o",
    target_model: "gpt-4o",
    notes: Some("Grok OpenAI Chat Completions model slot"),
}];
const OPENAI_CODEX_MODELS: &[AdapterModelMapEntry] = &[AdapterModelMapEntry {
    source_model: "gpt-4o",
    target_model: "gpt-4o",
    notes: Some("Local bridge presents the same model id to Codex"),
}];
const OPENAI_CLAUDE_MODELS: &[AdapterModelMapEntry] = &[AdapterModelMapEntry {
    source_model: "gpt-4o",
    target_model: "gpt-4o",
    notes: Some("Local bridge presents the same model id to Claude"),
}];

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

/// Official ChatGPT / Codex ids Grok CLI may pick on the loopback Responses
/// surface. Leftover prefixes (`grok-*` / `claude-*` / `kimi-*` / `deepseek-*`
/// / `agenthub_*_bridge`) are omitted by dispatch, so they must not appear.
const CODEX_GROK_MODELS: &[AdapterModelMapEntry] = &[
    AdapterModelMapEntry {
        source_model: "gpt-5.4",
        target_model: "gpt-5.4",
        notes: Some("ChatGPT Codex default; accepted by official Responses"),
    },
    AdapterModelMapEntry {
        source_model: "gpt-5.1-codex",
        target_model: "gpt-5.1-codex",
        notes: Some("Official Codex CLI model id"),
    },
    AdapterModelMapEntry {
        source_model: "gpt-5",
        target_model: "gpt-5",
        notes: Some("Official ChatGPT Responses model"),
    },
];

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
        id: "kimi-membership-grok-v1",
        source: AdapterSourceProduct::KimiCodeMembership,
        target: AgentId::Grok,
        target_protocol: AdapterTargetProtocol::OpenAiChatCompletions,
        default_target_model: Some("kimi-k2.5"),
        entries: KIMI_GROK_MODELS,
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
        id: "openai-api-grok-v1",
        source: AdapterSourceProduct::OpenaiApi,
        target: AgentId::Grok,
        target_protocol: AdapterTargetProtocol::OpenAiChatCompletions,
        default_target_model: Some("gpt-4o"),
        entries: OPENAI_GROK_MODELS,
        allow_passthrough: false,
    },
    AdapterModelMappingTable {
        id: "openai-api-codex-v1",
        source: AdapterSourceProduct::OpenaiApi,
        target: AgentId::Codex,
        target_protocol: AdapterTargetProtocol::OpenAiResponses,
        default_target_model: Some("gpt-4o"),
        entries: OPENAI_CODEX_MODELS,
        allow_passthrough: false,
    },
    AdapterModelMappingTable {
        id: "openai-api-claude-v1",
        source: AdapterSourceProduct::OpenaiApi,
        target: AgentId::Claude,
        target_protocol: AdapterTargetProtocol::AnthropicMessages,
        default_target_model: Some("gpt-4o"),
        entries: OPENAI_CLAUDE_MODELS,
        allow_passthrough: false,
    },
    AdapterModelMappingTable {
        id: "glm-coding-plan-pi-v1",
        source: AdapterSourceProduct::GlmCodingPlan,
        target: AgentId::Pi,
        target_protocol: AdapterTargetProtocol::PiProviderConfig,
        default_target_model: Some("glm-4.6"),
        entries: GLM_PI_MODELS,
        allow_passthrough: true,
    },
    AdapterModelMappingTable {
        id: "deepseek-api-pi-v1",
        source: AdapterSourceProduct::DeepseekApi,
        target: AgentId::Pi,
        target_protocol: AdapterTargetProtocol::PiProviderConfig,
        default_target_model: Some("deepseek-chat"),
        entries: DEEPSEEK_PI_MODELS,
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
        id: "grok-subscription-claude-v1",
        source: AdapterSourceProduct::XaiGrokSubscription,
        target: AgentId::Claude,
        target_protocol: AdapterTargetProtocol::AnthropicMessages,
        default_target_model: Some("grok-4.5"),
        entries: &[],
        allow_passthrough: false,
    },
    AdapterModelMappingTable {
        id: "grok-subscription-codex-v1",
        source: AdapterSourceProduct::XaiGrokSubscription,
        target: AgentId::Codex,
        target_protocol: AdapterTargetProtocol::OpenAiResponses,
        default_target_model: Some("grok-4.5"),
        entries: &[],
        allow_passthrough: false,
    },
    AdapterModelMappingTable {
        id: "codex-subscription-grok-v1",
        source: AdapterSourceProduct::CodexChatGptSubscription,
        target: AgentId::Grok,
        target_protocol: AdapterTargetProtocol::OpenAiResponses,
        default_target_model: Some("gpt-5.4"),
        entries: CODEX_GROK_MODELS,
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

/// Model ids the local bridge may advertise on `GET /v1/models`.
///
/// Union of mapping `entries[].target_model`, non-empty `default_target_model`,
/// and a non-empty configured profile/upstream default. Dedup preserves first
/// seen order. Missing tables fail closed: only a non-leftover configured
/// default is returned.
///
/// Leftover prefixes 400 on official Codex Responses, so ChatGPT-subscription
/// sources drop them. Other upstreams use those prefixes as real ids
/// (`grok-4.5`, `kimi-k2.5`).
pub fn list_local_bridge_models(
    source: AdapterSourceProduct,
    target: AgentId,
    default_model: Option<&str>,
) -> Vec<String> {
    let configured = nonempty_model(default_model);
    let drop_leftover = source == AdapterSourceProduct::CodexChatGptSubscription;
    let Some(table) = find_adapter_model_mapping(source, target) else {
        return match configured {
            Some(model) if !(drop_leftover && is_leftover_bridge_model(model)) => {
                vec![model.to_owned()]
            }
            _ => Vec::new(),
        };
    };

    let mut listed = Vec::with_capacity(table.entries.len() + 2);
    for entry in table.entries {
        push_listed_model(&mut listed, entry.target_model, drop_leftover);
    }
    if let Some(model) = table.default_target_model {
        push_listed_model(&mut listed, model, drop_leftover);
    }
    if let Some(model) = configured {
        push_listed_model(&mut listed, model, drop_leftover);
    }
    listed
}

/// Append stealth/ox-alpha only when this edge is the OpenRouter / custom
/// backup that should list it. Callers must not pass `include` for official
/// Grok / GPT start_specs.
pub fn with_openrouter_backup_model(mut listed: Vec<String>, include: bool) -> Vec<String> {
    if include {
        push_listed_model(&mut listed, OPENROUTER_BACKUP_MODEL, false);
    }
    listed
}

fn nonempty_model(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|model| !model.is_empty())
}

/// Resolve a model for one running edge. Custom OpenAI-compat / OpenRouter
/// passthrough unknown ids (`stealth/ox-alpha`); official OpenAI tables stay fail-closed.
pub fn map_edge_model(
    source: AdapterSourceProduct,
    target: AgentId,
    source_model: &str,
    custom_openai_compat: bool,
) -> AdapterModelMapResult {
    let Some(table) = find_adapter_model_mapping(source, target) else {
        return AdapterModelMapResult::Missing;
    };
    let result = table.map_model(source_model);
    if custom_openai_compat
        && source == AdapterSourceProduct::OpenaiApi
        && matches!(result, AdapterModelMapResult::Missing)
        && is_openrouter_backup_model(source_model)
    {
        return AdapterModelMapResult::Passthrough;
    }
    result
}

/// Whether this mapping table is actually consulted at runtime.
/// Empty reserved tables (no default, no entries, no passthrough) still send
/// the request to the lead; they must not trigger a model switch.
pub fn mapping_table_is_active(table: &AdapterModelMappingTable) -> bool {
    table.allow_passthrough
        || table.default_target_model.is_some()
        || !table.entries.is_empty()
}

/// One running (or known) edge the model-switch helper can pick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSwitchCandidate {
    pub profile_id: String,
    pub source: AdapterSourceProduct,
    pub target: AgentId,
    pub custom_openai_compat: bool,
    /// Same local surface as the authenticated lead. Cross-surface is never switched.
    pub same_surface: bool,
    pub running: bool,
    /// Models this edge advertises on GET /v1/models. A hit stays on the lead
    /// even when the mapping table is reserved-empty.
    pub listed_models: Vec<String>,
}

/// Per-request pick after the lead EdgeState is authenticated and the body model is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSwitchDecision {
    /// Stay on the authenticated lead.
    Stay,
    /// Use this other running edge for this request only.
    SwitchTo { profile_id: String },
    /// Lead cannot map the model, and no running alternate can serve it.
    Unavailable,
}

/// After gateway auth, if the lead mapping is Missing and another running
/// edge can serve the model (Mapped or Passthrough), switch for this request.
/// AccountPicker is not used here — that is same-class failover, not cross-vendor.
pub fn decide_model_switch(
    lead: &ModelSwitchCandidate,
    model: &str,
    others: &[ModelSwitchCandidate],
) -> ModelSwitchDecision {
    let lead_result = map_edge_model(
        lead.source,
        lead.target,
        model,
        lead.custom_openai_compat,
    );
    if lead_serves(lead, model, lead_result) {
        return ModelSwitchDecision::Stay;
    }

    let mut capable_running: Option<&ModelSwitchCandidate> = None;
    for candidate in others {
        if candidate.profile_id == lead.profile_id {
            continue;
        }
        if candidate.target != lead.target || !candidate.same_surface {
            continue;
        }
        let result = map_edge_model(
            candidate.source,
            candidate.target,
            model,
            candidate.custom_openai_compat,
        );
        if !matches!(
            result,
            AdapterModelMapResult::Mapped(_) | AdapterModelMapResult::Passthrough
        ) {
            continue;
        }
        if candidate.running && capable_running.is_none() {
            capable_running = Some(candidate);
        }
    }

    if let Some(alternate) = capable_running {
        return ModelSwitchDecision::SwitchTo {
            profile_id: alternate.profile_id.clone(),
        };
    }
    ModelSwitchDecision::Unavailable
}

fn lead_serves(lead: &ModelSwitchCandidate, model: &str, result: AdapterModelMapResult) -> bool {
    match result {
        AdapterModelMapResult::Mapped(_) | AdapterModelMapResult::Passthrough => true,
        AdapterModelMapResult::Missing => {
            let needle = model.trim();
            if !needle.is_empty()
                && lead
                    .listed_models
                    .iter()
                    .any(|listed| listed.eq_ignore_ascii_case(needle))
            {
                return true;
            }
            if lead.custom_openai_compat && lead.listed_models.is_empty() {
                return true;
            }
            if lead.custom_openai_compat && is_openrouter_backup_model(needle) {
                return true;
            }
            find_adapter_model_mapping(lead.source, lead.target)
                .is_none_or(|table| !mapping_table_is_active(table))
                && lead.listed_models.is_empty()
        }
    }
}

fn push_listed_model(listed: &mut Vec<String>, model: &str, drop_leftover: bool) {
    let model = model.trim();
    if model.is_empty() {
        return;
    }
    if drop_leftover && is_leftover_bridge_model(model) {
        return;
    }
    if listed.iter().any(|existing| existing == model) {
        return;
    }
    listed.push(model.to_owned());
}

#[cfg(test)]
mod switch_tests;

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

        let openai_codex =
            find_adapter_model_mapping(AdapterSourceProduct::OpenaiApi, AgentId::Codex)
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

    fn cand(
        id: &str,
        source: AdapterSourceProduct,
        target: AgentId,
        custom: bool,
        running: bool,
    ) -> ModelSwitchCandidate {
        ModelSwitchCandidate {
            profile_id: id.into(),
            source,
            target,
            custom_openai_compat: custom,
            same_surface: true,
            running,
            listed_models: Vec::new(),
        }
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
    fn model_switch_picks_running_openrouter_when_lead_misses() {
        let lead = cand(
            "official-claude",
            AdapterSourceProduct::XaiGrokSubscription,
            AgentId::Claude,
            false,
            true,
        );
        let alt = cand(
            "openrouter-claude",
            AdapterSourceProduct::OpenaiApi,
            AgentId::Claude,
            true,
            true,
        );
        assert_eq!(
            decide_model_switch(&lead, "stealth/ox-alpha", &[alt.clone()]),
            ModelSwitchDecision::SwitchTo {
                profile_id: "openrouter-claude".into()
            }
        );
        let stopped = cand(
            "openrouter-claude",
            AdapterSourceProduct::OpenaiApi,
            AgentId::Claude,
            true,
            false,
        );
        assert_eq!(
            decide_model_switch(&lead, "stealth/ox-alpha", &[stopped]),
            ModelSwitchDecision::Unavailable
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
        assert_eq!(listed[0], "gpt-5.4");
        assert!(listed.iter().any(|model| model == "gpt-5.1-codex"));
        assert!(listed.iter().any(|model| model == "gpt-5"));
    }

    #[test]
    fn missing_mapping_lists_configured_default_or_empty() {
        assert!(
            list_local_bridge_models(AdapterSourceProduct::Other, AgentId::Grok, None).is_empty()
        );
        assert!(
            list_local_bridge_models(AdapterSourceProduct::Other, AgentId::Grok, Some(""))
                .is_empty()
        );
        assert_eq!(
            list_local_bridge_models(AdapterSourceProduct::Other, AgentId::Grok, Some("gpt-5.4")),
            vec!["gpt-5.4".to_string()]
        );
        assert_eq!(
            list_local_bridge_models(AdapterSourceProduct::Other, AgentId::Grok, Some("grok-4.5")),
            vec!["grok-4.5".to_string()]
        );
        assert!(list_local_bridge_models(
            AdapterSourceProduct::CodexChatGptSubscription,
            AgentId::Kimi,
            Some("grok-4.5")
        )
        .is_empty());
    }

    #[test]
    fn grok_edges_list_default_when_mapping_entries_empty() {
        assert_eq!(
            list_local_bridge_models(
                AdapterSourceProduct::XaiGrokSubscription,
                AgentId::Claude,
                Some("grok-4.5")
            ),
            vec!["grok-4.5".to_string()]
        );
        assert_eq!(
            list_local_bridge_models(
                AdapterSourceProduct::XaiGrokSubscription,
                AgentId::Codex,
                Some("grok-4.5")
            ),
            vec!["grok-4.5".to_string()]
        );
    }
}
