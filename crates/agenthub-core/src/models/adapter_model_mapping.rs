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
//! - Codex ChatGPT subscription → Grok / Kimi (local GET /models)
//!
//! Subscription fallback catalogs (ChatGPT / Claude Code / Grok) live in
//! [`subscription_fallback_models.json`]. Live login lists override them.
//! Mapping entries may keep older ids for rewrite without advertising them.
//!
//! Request-scoped edge pick (`decide_model_switch`) lives in
//! `bridge::model_switch`, not here. This file is the static table.

use std::sync::OnceLock;

use super::{AdapterSourceProduct, AdapterTargetProtocol, AgentId};

const FALLBACK_MODELS_JSON: &str = include_str!("subscription_fallback_models.json");

/// Retired OpenRouter stealth backup. Do not inject it into `/models` or pin
/// it as a default — the upstream 404s (`GLM-5.3 Flash` testing period ended).
pub const OPENROUTER_BACKUP_MODEL: &str = "stealth/ox-alpha";

pub fn is_openrouter_backup_model(model: &str) -> bool {
    super::strip_claude_context_marker(model).eq_ignore_ascii_case(OPENROUTER_BACKUP_MODEL)
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

/// Official ChatGPT / Codex ids the local Responses surface may rewrite.
/// Listing uses [`static_fallback_models`], not this table. Older ids stay
/// here so a leftover request still maps.
const CODEX_GROK_MODELS: &[AdapterModelMapEntry] = &[
    AdapterModelMapEntry {
        source_model: "gpt-5.6-sol",
        target_model: "gpt-5.6-sol",
        notes: Some("ChatGPT Codex default"),
    },
    AdapterModelMapEntry {
        source_model: "gpt-5.6-terra",
        target_model: "gpt-5.6-terra",
        notes: Some("ChatGPT Codex everyday"),
    },
    AdapterModelMapEntry {
        source_model: "gpt-5.6-luna",
        target_model: "gpt-5.6-luna",
        notes: Some("ChatGPT Codex fast"),
    },
    AdapterModelMapEntry {
        source_model: "gpt-5.6-cyber",
        target_model: "gpt-5.6-cyber",
        notes: Some("ChatGPT Codex security-focused tier"),
    },
    AdapterModelMapEntry {
        source_model: "gpt-5.6",
        target_model: "gpt-5.6",
        notes: Some("ChatGPT Codex base alias"),
    },
    AdapterModelMapEntry {
        source_model: "gpt-5.4",
        target_model: "gpt-5.4",
        notes: Some("Retired ChatGPT Codex id; still rewritten if requested"),
    },
    AdapterModelMapEntry {
        source_model: "gpt-5.1-codex",
        target_model: "gpt-5.1-codex",
        notes: Some("Retired Codex CLI id; still rewritten if requested"),
    },
    AdapterModelMapEntry {
        source_model: "gpt-5",
        target_model: "gpt-5",
        notes: Some("Retired ChatGPT Responses id; still rewritten if requested"),
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
        default_target_model: Some("grok-4.6"),
        entries: &[],
        allow_passthrough: false,
    },
    AdapterModelMappingTable {
        id: "grok-subscription-codex-v1",
        source: AdapterSourceProduct::XaiGrokSubscription,
        target: AgentId::Codex,
        target_protocol: AdapterTargetProtocol::OpenAiResponses,
        default_target_model: Some("grok-4.6"),
        entries: &[],
        allow_passthrough: false,
    },
    AdapterModelMappingTable {
        id: "codex-subscription-grok-v1",
        source: AdapterSourceProduct::CodexChatGptSubscription,
        target: AgentId::Grok,
        target_protocol: AdapterTargetProtocol::OpenAiResponses,
        default_target_model: Some("gpt-5.6-sol"),
        entries: CODEX_GROK_MODELS,
        allow_passthrough: false,
    },
    // Codex → Kimi local bridge: advertise ChatGPT fallback ids (not
    // kimi-* leftovers). Same catalog shape as Codex → Grok.
    AdapterModelMappingTable {
        id: "codex-subscription-kimi-v1",
        source: AdapterSourceProduct::CodexChatGptSubscription,
        target: AgentId::Kimi,
        target_protocol: AdapterTargetProtocol::OpenAiChatCompletions,
        default_target_model: Some("gpt-5.6-sol"),
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
    AdapterModelMappingTable {
        id: "kiro-claude-v1",
        source: AdapterSourceProduct::Kiro,
        target: AgentId::Claude,
        target_protocol: AdapterTargetProtocol::AnthropicMessages,
        default_target_model: Some("auto"),
        entries: &[],
        allow_passthrough: true,
    },
    AdapterModelMappingTable {
        id: "kiro-codex-v1",
        source: AdapterSourceProduct::Kiro,
        target: AgentId::Codex,
        target_protocol: AdapterTargetProtocol::OpenAiResponses,
        default_target_model: Some("auto"),
        entries: &[],
        allow_passthrough: true,
    },
    AdapterModelMappingTable {
        id: "kiro-grok-v1",
        source: AdapterSourceProduct::Kiro,
        target: AgentId::Grok,
        target_protocol: AdapterTargetProtocol::OpenAiResponses,
        default_target_model: Some("auto"),
        entries: &[],
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

#[derive(Debug, Clone, serde::Deserialize)]
struct FallbackFile {
    chatgpt: FallbackEntry,
    claude: FallbackEntry,
    grok: FallbackEntry,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct FallbackEntry {
    default: String,
    models: Vec<String>,
}

fn fallback_file() -> &'static FallbackFile {
    static FILE: OnceLock<FallbackFile> = OnceLock::new();
    FILE.get_or_init(|| {
        let mut file: FallbackFile = serde_json::from_str(FALLBACK_MODELS_JSON)
            .expect("subscription_fallback_models.json must parse");
        normalize_fallback_entry(&mut file.chatgpt);
        normalize_fallback_entry(&mut file.claude);
        normalize_fallback_entry(&mut file.grok);
        file
    })
}

fn normalize_fallback_entry(entry: &mut FallbackEntry) {
    let default = entry.default.trim();
    if default.is_empty() {
        return;
    }
    if !entry.models.iter().any(|model| model.trim() == default) {
        entry.models.insert(0, default.to_owned());
    }
}

/// Static `GET /models` ids when a login's live catalog is empty.
///
/// Keyed by source product, not target agent. Loaded from
/// `subscription_fallback_models.json`. Other sources have no fallback here
/// and keep using mapping-table listing.
pub fn static_fallback_models(source: AdapterSourceProduct) -> &'static [String] {
    let file = fallback_file();
    match source {
        AdapterSourceProduct::CodexChatGptSubscription => file.chatgpt.models.as_slice(),
        AdapterSourceProduct::ClaudeSubscription => file.claude.models.as_slice(),
        AdapterSourceProduct::XaiGrokSubscription => file.grok.models.as_slice(),
        _ => &[],
    }
}

/// Model ids the local bridge may advertise on `GET /v1/models`.
///
/// Subscription sources prefer [`static_fallback_models`]. Other sources union
/// mapping `entries[].target_model`, non-empty `default_target_model`, and a
/// non-empty configured profile/upstream default. Dedup preserves first-seen
/// order. Missing tables fail closed: only a non-leftover configured default
/// is returned.
///
/// Leftover prefixes 400 on official Codex Responses, so ChatGPT-subscription
/// sources drop them. Other upstreams use those prefixes as real ids
/// (`grok-4.6`, `kimi-k2.5`).
pub fn list_local_bridge_models(
    source: AdapterSourceProduct,
    target: AgentId,
    default_model: Option<&str>,
) -> Vec<String> {
    let configured = nonempty_model(default_model);
    let drop_leftover = source == AdapterSourceProduct::CodexChatGptSubscription;
    let mut listed = Vec::new();
    for model in static_fallback_models(source) {
        push_listed_model(&mut listed, model, drop_leftover);
    }
    if listed.is_empty() {
        if let Some(table) = find_adapter_model_mapping(source, target) {
            listed.reserve(table.entries.len() + 2);
            for entry in table.entries {
                push_listed_model(&mut listed, entry.target_model, drop_leftover);
            }
            if let Some(model) = table.default_target_model {
                push_listed_model(&mut listed, model, drop_leftover);
            }
        }
    }
    if let Some(model) = configured {
        push_listed_model(&mut listed, model, drop_leftover);
    }
    listed
}

/// Drop the retired OpenRouter stealth backup. Do not invent a replacement id.
pub fn with_openrouter_backup_model(mut listed: Vec<String>, _include: bool) -> Vec<String> {
    listed.retain(|model| !is_openrouter_backup_model(model));
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
    table.allow_passthrough || table.default_target_model.is_some() || !table.entries.is_empty()
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
mod tests;
