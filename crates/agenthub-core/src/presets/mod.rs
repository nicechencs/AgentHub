//! L3 built-in product resources: read-only provider preset registry.
//!
//! Ported from frontend `src/config/presets/index.ts` for the first P1 slice.
//! Core is the authority; the frontend file remains a temporary mirror until
//! GUI is wired to core. Do not introduce a second mutable source.

use crate::models::{AgentId, ConfigFormat, ProviderPreset};

/// Static definition used only inside this module.
struct PresetDef {
    agent: AgentId,
    id: &'static str,
    label: &'static str,
    format: ConfigFormat,
    template: &'static str,
}

impl PresetDef {
    fn to_preset(&self) -> ProviderPreset {
        ProviderPreset {
            agent: self.agent,
            id: self.id.into(),
            label: self.label.into(),
            format: self.format,
            template: self.template.into(),
        }
    }
}

/// Built-in presets in deterministic order: [`AgentId::ALL`], then the
/// per-agent order matching `src/config/presets/index.ts`.
/// Templates use common field shapes (Claude env+model; Codex
/// model_providers + wire_api + reasoning effort).
const BUILTIN: &[PresetDef] = &[
    // --- claude ---
    PresetDef {
        agent: AgentId::Claude,
        id: "anthropic",
        label: "Anthropic 官方",
        format: ConfigFormat::Json,
        template: "{\n  \"env\": {}\n}",
    },
    PresetDef {
        agent: AgentId::Claude,
        id: "anthropic-compatible",
        label: "Anthropic 兼容",
        format: ConfigFormat::Json,
        template: "{\n  \"env\": {\n    \"ANTHROPIC_BASE_URL\": \"https://your-relay.example.com\",\n    \"ANTHROPIC_AUTH_TOKEN\": \"sk-xxxxxxxx\"\n  },\n  \"model\": \"sonnet\"\n}",
    },
    // --- codex ---
    PresetDef {
        agent: AgentId::Codex,
        id: "openai",
        label: "OpenAI 官方",
        format: ConfigFormat::Toml,
        template: "model = \"gpt-5.1-codex\"\n",
    },
    PresetDef {
        agent: AgentId::Codex,
        id: "openai-compatible",
        label: "OpenAI 兼容",
        format: ConfigFormat::Toml,
        template: "model_provider = \"custom\"\nmodel = \"gpt-5.1-codex\"\nmodel_reasoning_effort = \"high\"\ndisable_response_storage = true\npreferred_auth_method = \"apikey\"\n\n[model_providers.custom]\nname = \"custom\"\nbase_url = \"https://your-relay.example.com/v1\"\nwire_api = \"responses\"\n",
    },
    // --- kimi ---
    PresetDef {
        agent: AgentId::Kimi,
        id: "moonshot",
        label: "Moonshot 官方",
        format: ConfigFormat::Toml,
        template: "default_model = \"kimi-k2\"\ndefault_provider = \"moonshot\"\n\n[providers.moonshot]\ntype = \"kimi\"\nbase_url = \"https://api.moonshot.cn/v1\"\napi_key = \"\"\n\n[models.\"kimi-k2\"]\nprovider = \"moonshot\"\nmodel = \"kimi-k2\"\nmax_context_size = 131072\n",
    },
    PresetDef {
        agent: AgentId::Kimi,
        id: "openai-compatible",
        label: "OpenAI 兼容",
        format: ConfigFormat::Toml,
        template: "default_model = \"kimi-k2\"\ndefault_provider = \"custom\"\n\n[providers.custom]\ntype = \"openai\"\nbase_url = \"https://your-relay.example.com/v1\"\napi_key = \"sk-xxxxxxxx\"\n\n[models.\"kimi-k2\"]\nprovider = \"custom\"\nmodel = \"kimi-k2\"\nmax_context_size = 131072\n",
    },
    // --- grok ---
    PresetDef {
        agent: AgentId::Grok,
        id: "xai",
        label: "xAI 官方",
        format: ConfigFormat::Toml,
        template: "[models]\ndefault = \"grok\"\nweb_search = \"grok\"\n\n[model.\"grok\"]\nmodel = \"grok-4.5\"\napi_key = \"xai-xxxxxxxx\"\napi_backend = \"responses\"\ncontext_window = 1000000\nsupports_backend_search = true\n",
    },
    PresetDef {
        agent: AgentId::Grok,
        id: "openai-compatible",
        label: "OpenAI 兼容",
        format: ConfigFormat::Toml,
        template: "[models]\ndefault = \"grok\"\nweb_search = \"grok\"\n\n[model.\"grok\"]\nmodel = \"grok-4.5\"\nbase_url = \"https://your-relay.example.com/v1\"\napi_key = \"sk-xxxxxxxx\"\napi_backend = \"responses\"\ncontext_window = 1000000\nsupports_backend_search = true\n",
    },
];

/// List all built-in provider presets (deterministic order).
pub fn list_all() -> Vec<ProviderPreset> {
    BUILTIN.iter().map(PresetDef::to_preset).collect()
}

/// List built-in presets, optionally filtered by agent.
///
/// Without a filter, returns all presets in [`AgentId::ALL`] order.
pub fn list(filter: Option<AgentId>) -> Vec<ProviderPreset> {
    match filter {
        None => list_all(),
        Some(agent) => list_for(agent),
    }
}

/// List built-in presets for a single agent (stable per-agent order).
pub fn list_for(agent: AgentId) -> Vec<ProviderPreset> {
    BUILTIN
        .iter()
        .filter(|p| p.agent == agent)
        .map(PresetDef::to_preset)
        .collect()
}

/// Total number of built-in presets (for tests / diagnostics).
pub fn count() -> usize {
    BUILTIN.len()
}

#[cfg(test)]
mod tests;
