//! Synchronous control-plane persistence for Kimi Code membership -> Codex.
//!
//! This service deliberately does **not** own the loopback listener or write a
//! live Codex configuration. A Tauri host drives the saga in this order:
//!
//! 1. [`AdapterBridgeService::prepare`] creates/reuses an `applying` profile
//!    and returns non-serializable runtime material.
//! 2. The host starts and probes a [`crate::bridge::BridgeRuntimeHost`] using
//!    [`AdapterBridgePrepared::start_spec`].
//! 3. The host calls [`AdapterBridgeService::persist_bridge_projection_inner`]
//!    (or restore-port realign) with the same [`crate::services::ProviderService`]
//!    that issued the live saga guard, then [`AdapterBridgeService::finalize`].
//! 4. Any failure is recorded through
//!    [`AdapterBridgeService::mark_needs_attention`].
//!
//! Source credentials are only returned inside process memory. The generated
//! local bearer is persisted solely in the generated target provider; adapter
//! profiles retain only the provider id.

use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use toml_edit::DocumentMut;

use crate::bridge::grok_cli::GROK_CLI_PROXY_BASE_URL;
use crate::bridge::{
    index_from_member_listings, BridgeLocalSurface, BridgeStartSpec, BridgeUpstreamConfig,
    BridgeUpstreamProtocol, EffectiveRouteIndex, MemberListing, ResolvedAuth,
};
use crate::error::{AppError, Result};
use crate::models::{
    list_local_bridge_models, AdapterCredentialClass, AdapterProfile, AdapterProfileFilter,
    AdapterProfileMode, AdapterProfileStatus, AdapterRoute, AdapterRouteRequest, AdapterSourceKind,
    AdapterSourceProduct, AdapterSupport, AdapterTargetProtocol, AdapterUpstreamTransport, AgentId,
    LocalBridgeEdge, Provider, ProviderInput, RouteMember, RouteSchedulePolicy,
    ANTHROPIC_CODEX_EDGE, CODEX_CLAUDE_RESPONSES_EDGE, CODEX_DSH_EDGE, CODEX_GROK_EDGE,
    CODEX_KIMI_EDGE, GROK_CLAUDE_EDGE, GROK_CODEX_EDGE, KIMI_CODEX_EDGE, OPENAI_CLAUDE_EDGE,
    OPENAI_CODEX_EDGE, OPENAI_GROK_BRIDGE_EDGE,
};
use crate::services::{AdapterRouteService, AdapterSecretResolver, RoutePoolService};
use crate::storage::{AdapterProfileRepo, Database, ProviderRepo};

const RULE_ID: &str = KIMI_CODEX_EDGE.rule_id;
const ANTHROPIC_RULE_ID: &str = ANTHROPIC_CODEX_EDGE.rule_id;
const OPENAI_RULE_ID: &str = OPENAI_CODEX_EDGE.rule_id;
const OPENAI_CLAUDE_BRIDGE_RULE_ID: &str = OPENAI_CLAUDE_EDGE.rule_id;
const OPENAI_GROK_LOCAL_RULE_ID: &str = OPENAI_GROK_BRIDGE_EDGE.rule_id;
const CODEX_CLAUDE_RULE_ID: &str = CODEX_CLAUDE_RESPONSES_EDGE.rule_id;
const GROK_CLAUDE_RULE_ID: &str = GROK_CLAUDE_EDGE.rule_id;
const GROK_CODEX_RULE_ID: &str = GROK_CODEX_EDGE.rule_id;
const CODEX_GROK_RULE_ID: &str = CODEX_GROK_EDGE.rule_id;
const CODEX_KIMI_RULE_ID: &str = CODEX_KIMI_EDGE.rule_id;
const CODEX_DSH_RULE_ID: &str = CODEX_DSH_EDGE.rule_id;
const RULE_VERSION: &str = "1";
const KIMI_CHAT_BASE_URL: &str = "https://api.kimi.com/coding/v1";
const ANTHROPIC_MESSAGES_BASE_URL: &str = "https://api.anthropic.com/v1";
const OPENAI_CHAT_BASE_URL: &str = crate::services::adapter_route_constants::OPENAI_GROK_BASE_URL;
const CHATGPT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex/";
const DEFAULT_MODEL: &str = KIMI_CODEX_EDGE.default_model;
// Used by `tests.rs` assertions.
#[allow(dead_code)]
const ANTHROPIC_DEFAULT_MODEL: &str = ANTHROPIC_CODEX_EDGE.default_model;
#[allow(dead_code)]
const OPENAI_DEFAULT_MODEL: &str = OPENAI_CODEX_EDGE.default_model;
#[allow(dead_code)]
const CODEX_DEFAULT_MODEL: &str = CODEX_CLAUDE_RESPONSES_EDGE.default_model;
const PROVIDER_SLUG: &str = "agenthub_kimi_bridge";
const ANTHROPIC_PROVIDER_SLUG: &str = "agenthub_anthropic_bridge";
const OPENAI_PROVIDER_SLUG: &str = "agenthub_openai_bridge";
const CODEX_CLAUDE_PROVIDER_SLUG: &str = "claude-codex-adapter-bridge";
const GENERATED_BY: &str = "adapter";
const BRIDGE_HEALTH_TIMEOUT: Duration = Duration::from_secs(4);
const RETRYABLE_ERROR_PREFIX: &str = "retryable:";

#[derive(Clone, Copy)]
pub(super) struct CodexBridgeRule {
    rule_id: &'static str,
    profile_prefix: &'static str,
    provider_prefix: &'static str,
    profile_name: &'static str,
    provider_name: &'static str,
    toml_name: &'static str,
    provider_slug: &'static str,
    upstream_base_url: &'static str,
    default_model: &'static str,
    protocol: BridgeUpstreamProtocol,
    local_surface: BridgeLocalSurface,
    bridge_kind: &'static str,
    legacy_bridge_kinds: &'static [&'static str],
    source: AdapterSourceProduct,
    target_agent: AgentId,
    mode: AdapterProfileMode,
}

/// Map catalog target protocol → listener surface. Local-bridge edges never
/// use Pi/DSH config protocols; those variants are a compile-time hole.
const fn local_surface_of(edge: &LocalBridgeEdge) -> BridgeLocalSurface {
    match edge.protocol {
        AdapterTargetProtocol::AnthropicMessages => BridgeLocalSurface::Messages,
        AdapterTargetProtocol::OpenAiResponses => BridgeLocalSurface::Responses,
        AdapterTargetProtocol::OpenAiChatCompletions => BridgeLocalSurface::ChatCompletions,
        AdapterTargetProtocol::PiProviderConfig | AdapterTargetProtocol::DshProviderConfig => {
            BridgeLocalSurface::Responses
        }
    }
}

/// Map catalog upstream transport → runtime protocol.
const fn upstream_protocol_of(edge: &LocalBridgeEdge) -> BridgeUpstreamProtocol {
    match edge.transport {
        AdapterUpstreamTransport::LocalBridgeChatCompletions => {
            BridgeUpstreamProtocol::OpenAiChatCompletions
        }
        AdapterUpstreamTransport::LocalBridgeAnthropicMessages => {
            BridgeUpstreamProtocol::AnthropicMessages
        }
        AdapterUpstreamTransport::CodexResponsesOauth => {
            BridgeUpstreamProtocol::CodexResponsesOauth
        }
        AdapterUpstreamTransport::XaiResponsesOauth => BridgeUpstreamProtocol::XaiResponsesOauth,
        AdapterUpstreamTransport::NativeHttp
        | AdapterUpstreamTransport::CodexAppServer
        | AdapterUpstreamTransport::None => BridgeUpstreamProtocol::OpenAiChatCompletions,
    }
}

const fn live_writer_mode(edge: &LocalBridgeEdge) -> AdapterProfileMode {
    match edge.credential {
        AdapterCredentialClass::ApiKey => AdapterProfileMode::Api,
        AdapterCredentialClass::OauthAuthJson
        | AdapterCredentialClass::OauthOther
        | AdapterCredentialClass::Unknown => AdapterProfileMode::Oauth,
    }
}

const KIMI_CODEX_RULE: CodexBridgeRule = CodexBridgeRule {
    rule_id: KIMI_CODEX_EDGE.rule_id,
    profile_prefix: "adapter-kimi-codex-bridge",
    provider_prefix: "codex-kimi-adapter-bridge",
    profile_name: "Kimi → Codex Bridge",
    provider_name: "Kimi Code Bridge",
    toml_name: "AgentHub Kimi Bridge",
    provider_slug: PROVIDER_SLUG,
    upstream_base_url: KIMI_CHAT_BASE_URL,
    default_model: KIMI_CODEX_EDGE.default_model,
    protocol: upstream_protocol_of(&KIMI_CODEX_EDGE),
    local_surface: local_surface_of(&KIMI_CODEX_EDGE),
    bridge_kind: "responses_to_chat_completions",
    legacy_bridge_kinds: &[],
    source: KIMI_CODEX_EDGE.source,
    target_agent: KIMI_CODEX_EDGE.target,
    mode: live_writer_mode(&KIMI_CODEX_EDGE),
};

const ANTHROPIC_CODEX_RULE: CodexBridgeRule = CodexBridgeRule {
    rule_id: ANTHROPIC_CODEX_EDGE.rule_id,
    profile_prefix: "adapter-anthropic-codex-bridge",
    provider_prefix: "codex-anthropic-adapter-bridge",
    profile_name: "Anthropic → Codex Bridge",
    provider_name: "Anthropic Bridge",
    toml_name: "AgentHub Anthropic Bridge",
    provider_slug: ANTHROPIC_PROVIDER_SLUG,
    upstream_base_url: ANTHROPIC_MESSAGES_BASE_URL,
    default_model: ANTHROPIC_CODEX_EDGE.default_model,
    protocol: upstream_protocol_of(&ANTHROPIC_CODEX_EDGE),
    local_surface: local_surface_of(&ANTHROPIC_CODEX_EDGE),
    bridge_kind: "responses_to_anthropic_messages",
    legacy_bridge_kinds: &[],
    source: ANTHROPIC_CODEX_EDGE.source,
    target_agent: ANTHROPIC_CODEX_EDGE.target,
    mode: live_writer_mode(&ANTHROPIC_CODEX_EDGE),
};

const OPENAI_CODEX_RULE: CodexBridgeRule = CodexBridgeRule {
    rule_id: OPENAI_CODEX_EDGE.rule_id,
    profile_prefix: "adapter-openai-codex-bridge",
    provider_prefix: "codex-openai-adapter-bridge",
    profile_name: "OpenAI → Codex Bridge",
    provider_name: "OpenAI Bridge",
    toml_name: "AgentHub OpenAI Bridge",
    provider_slug: OPENAI_PROVIDER_SLUG,
    upstream_base_url: OPENAI_CHAT_BASE_URL,
    default_model: OPENAI_CODEX_EDGE.default_model,
    protocol: upstream_protocol_of(&OPENAI_CODEX_EDGE),
    local_surface: local_surface_of(&OPENAI_CODEX_EDGE),
    bridge_kind: "responses_to_chat_completions",
    legacy_bridge_kinds: &[],
    source: OPENAI_CODEX_EDGE.source,
    target_agent: OPENAI_CODEX_EDGE.target,
    mode: live_writer_mode(&OPENAI_CODEX_EDGE),
};

const OPENAI_CLAUDE_RULE: CodexBridgeRule = CodexBridgeRule {
    rule_id: OPENAI_CLAUDE_EDGE.rule_id,
    profile_prefix: "adapter-openai-claude-bridge",
    provider_prefix: "claude-openai-adapter-bridge",
    profile_name: "OpenAI → Claude Code Bridge",
    provider_name: "OpenAI Bridge",
    toml_name: "",
    provider_slug: "",
    upstream_base_url: OPENAI_CHAT_BASE_URL,
    default_model: OPENAI_CLAUDE_EDGE.default_model,
    protocol: upstream_protocol_of(&OPENAI_CLAUDE_EDGE),
    local_surface: local_surface_of(&OPENAI_CLAUDE_EDGE),
    bridge_kind: "messages_to_chat_completions",
    legacy_bridge_kinds: &[],
    source: OPENAI_CLAUDE_EDGE.source,
    target_agent: OPENAI_CLAUDE_EDGE.target,
    mode: live_writer_mode(&OPENAI_CLAUDE_EDGE),
};

const OPENAI_GROK_BRIDGE_RULE: CodexBridgeRule = CodexBridgeRule {
    rule_id: OPENAI_GROK_BRIDGE_EDGE.rule_id,
    profile_prefix: "adapter-openai-grok-bridge",
    provider_prefix: "grok-openai-adapter-bridge",
    profile_name: "OpenAI → Grok 本机路由",
    provider_name: "OpenAI 本机路由",
    toml_name: "AgentHub OpenAI Route",
    provider_slug: "agenthub_openai_bridge",
    upstream_base_url: OPENAI_CHAT_BASE_URL,
    default_model: OPENAI_GROK_BRIDGE_EDGE.default_model,
    protocol: upstream_protocol_of(&OPENAI_GROK_BRIDGE_EDGE),
    local_surface: local_surface_of(&OPENAI_GROK_BRIDGE_EDGE),
    bridge_kind: "responses_to_chat_completions",
    legacy_bridge_kinds: &[],
    source: OPENAI_GROK_BRIDGE_EDGE.source,
    target_agent: OPENAI_GROK_BRIDGE_EDGE.target,
    mode: live_writer_mode(&OPENAI_GROK_BRIDGE_EDGE),
};

const CODEX_CLAUDE_RULE: CodexBridgeRule = CodexBridgeRule {
    rule_id: CODEX_CLAUDE_RESPONSES_EDGE.rule_id,
    profile_prefix: "adapter-codex-claude-bridge",
    provider_prefix: CODEX_CLAUDE_PROVIDER_SLUG,
    profile_name: "Codex → Claude Code Bridge",
    provider_name: "Codex Subscription Bridge",
    toml_name: "",
    provider_slug: "",
    upstream_base_url: CHATGPT_CODEX_BASE_URL,
    default_model: CODEX_CLAUDE_RESPONSES_EDGE.default_model,
    protocol: upstream_protocol_of(&CODEX_CLAUDE_RESPONSES_EDGE),
    local_surface: local_surface_of(&CODEX_CLAUDE_RESPONSES_EDGE),
    bridge_kind: "messages_to_codex_responses",
    legacy_bridge_kinds: &[],
    source: CODEX_CLAUDE_RESPONSES_EDGE.source,
    target_agent: CODEX_CLAUDE_RESPONSES_EDGE.target,
    mode: live_writer_mode(&CODEX_CLAUDE_RESPONSES_EDGE),
};

const GROK_CLAUDE_RULE: CodexBridgeRule = CodexBridgeRule {
    rule_id: GROK_CLAUDE_EDGE.rule_id,
    profile_prefix: "adapter-grok-claude-bridge",
    provider_prefix: "claude-grok-adapter-bridge",
    profile_name: "Grok → Claude Code Bridge",
    provider_name: "Grok Subscription Bridge",
    toml_name: "",
    provider_slug: "",
    upstream_base_url: GROK_CLI_PROXY_BASE_URL,
    default_model: GROK_CLAUDE_EDGE.default_model,
    protocol: upstream_protocol_of(&GROK_CLAUDE_EDGE),
    local_surface: local_surface_of(&GROK_CLAUDE_EDGE),
    bridge_kind: "messages_to_xai_responses",
    legacy_bridge_kinds: &["messages_to_xai_chat_completions"],
    source: GROK_CLAUDE_EDGE.source,
    target_agent: GROK_CLAUDE_EDGE.target,
    mode: live_writer_mode(&GROK_CLAUDE_EDGE),
};

const GROK_CODEX_RULE: CodexBridgeRule = CodexBridgeRule {
    rule_id: GROK_CODEX_EDGE.rule_id,
    profile_prefix: "adapter-grok-codex-bridge",
    provider_prefix: "codex-grok-adapter-bridge",
    profile_name: "Grok → Codex 本机路由",
    provider_name: "Grok 本机路由",
    toml_name: "AgentHub Grok Route",
    provider_slug: "agenthub_grok_bridge",
    upstream_base_url: GROK_CLI_PROXY_BASE_URL,
    default_model: GROK_CODEX_EDGE.default_model,
    protocol: upstream_protocol_of(&GROK_CODEX_EDGE),
    local_surface: local_surface_of(&GROK_CODEX_EDGE),
    bridge_kind: "responses_to_xai_responses",
    legacy_bridge_kinds: &["responses_to_chat_completions"],
    source: GROK_CODEX_EDGE.source,
    target_agent: GROK_CODEX_EDGE.target,
    mode: live_writer_mode(&GROK_CODEX_EDGE),
};

const CODEX_GROK_RULE: CodexBridgeRule = CodexBridgeRule {
    rule_id: CODEX_GROK_EDGE.rule_id,
    profile_prefix: "adapter-codex-grok-bridge",
    provider_prefix: "grok-codex-adapter-bridge",
    profile_name: "Codex → Grok 本机路由",
    provider_name: "Codex 本机路由",
    toml_name: "AgentHub Codex Route",
    provider_slug: "agenthub_codex_bridge",
    upstream_base_url: CHATGPT_CODEX_BASE_URL,
    default_model: CODEX_GROK_EDGE.default_model,
    protocol: upstream_protocol_of(&CODEX_GROK_EDGE),
    local_surface: local_surface_of(&CODEX_GROK_EDGE),
    bridge_kind: "responses_to_codex_responses",
    legacy_bridge_kinds: &["chat_completions_to_codex_responses"],
    source: CODEX_GROK_EDGE.source,
    target_agent: CODEX_GROK_EDGE.target,
    mode: live_writer_mode(&CODEX_GROK_EDGE),
};

const CODEX_KIMI_RULE: CodexBridgeRule = CodexBridgeRule {
    rule_id: CODEX_KIMI_EDGE.rule_id,
    profile_prefix: "adapter-codex-kimi-bridge",
    provider_prefix: "kimi-codex-adapter-bridge",
    profile_name: "Codex → Kimi 本机路由",
    provider_name: "Codex 本机路由",
    toml_name: "AgentHub Codex Route",
    provider_slug: "agenthub_codex_bridge",
    upstream_base_url: CHATGPT_CODEX_BASE_URL,
    default_model: CODEX_KIMI_EDGE.default_model,
    protocol: upstream_protocol_of(&CODEX_KIMI_EDGE),
    local_surface: local_surface_of(&CODEX_KIMI_EDGE),
    bridge_kind: "chat_completions_to_codex_responses",
    legacy_bridge_kinds: &[],
    source: CODEX_KIMI_EDGE.source,
    target_agent: CODEX_KIMI_EDGE.target,
    mode: live_writer_mode(&CODEX_KIMI_EDGE),
};

const CODEX_DSH_RULE: CodexBridgeRule = CodexBridgeRule {
    rule_id: CODEX_DSH_EDGE.rule_id,
    profile_prefix: "adapter-codex-dsh-bridge",
    provider_prefix: "dsh-codex-adapter-bridge",
    profile_name: "Codex → DeepSeek Harness 本机路由",
    provider_name: "Codex 本机路由",
    toml_name: "",
    provider_slug: "",
    upstream_base_url: CHATGPT_CODEX_BASE_URL,
    default_model: CODEX_DSH_EDGE.default_model,
    protocol: upstream_protocol_of(&CODEX_DSH_EDGE),
    local_surface: local_surface_of(&CODEX_DSH_EDGE),
    bridge_kind: "chat_completions_to_codex_responses",
    legacy_bridge_kinds: &[],
    source: CODEX_DSH_EDGE.source,
    target_agent: CODEX_DSH_EDGE.target,
    mode: live_writer_mode(&CODEX_DSH_EDGE),
};

/// Live local-bridge writers. `rule_for_id` and the secret-resolver coverage
/// test both read this slice so a new rule cannot ship without a matcher check.
const LIVE_BRIDGE_RULES: &[CodexBridgeRule] = &[
    KIMI_CODEX_RULE,
    ANTHROPIC_CODEX_RULE,
    OPENAI_CODEX_RULE,
    OPENAI_CLAUDE_RULE,
    OPENAI_GROK_BRIDGE_RULE,
    CODEX_CLAUDE_RULE,
    GROK_CLAUDE_RULE,
    GROK_CODEX_RULE,
    CODEX_GROK_RULE,
    CODEX_KIMI_RULE,
    CODEX_DSH_RULE,
];

mod finalize;
mod persist_saga;
pub(super) mod prepare;
mod removal;
mod rules;

pub use persist_saga::{should_make_bridge_current, BridgeProviderSnapshot};

use rules::*;

#[cfg(test)]
mod tests;

pub(super) fn rule_for_id(rule_id: &str) -> Option<CodexBridgeRule> {
    LIVE_BRIDGE_RULES
        .iter()
        .copied()
        .find(|rule| rule.rule_id == rule_id)
}

/// `(target_agent, rule_id)` for every live local-bridge writer.
// Referenced only from `tests.rs` in this crate; keep for test coverage.
#[allow(dead_code)]
pub(crate) fn live_bridge_rule_projections() -> impl Iterator<Item = (AgentId, &'static str)> {
    LIVE_BRIDGE_RULES
        .iter()
        .map(|rule| (rule.target_agent, rule.rule_id))
}

fn listed_models_for_bridge(
    source: AdapterSourceProduct,
    target: AgentId,
    default_model: &str,
    custom_openai: bool,
    user_listed: &[String],
) -> Vec<String> {
    if !user_listed.is_empty() {
        let mut listed = Vec::new();
        for model in user_listed {
            let model = crate::models::strip_claude_context_marker(model);
            if model.is_empty() {
                continue;
            }
            if listed
                .iter()
                .any(|existing: &String| crate::models::listed_model_matches(existing, model))
            {
                continue;
            }
            listed.push(model.to_owned());
        }
        return crate::models::with_openrouter_backup_model(listed, custom_openai);
    }
    if custom_openai {
        return Vec::new();
    }
    let configured = default_model.trim();
    list_local_bridge_models(
        source,
        target,
        if configured.is_empty() {
            None
        } else {
            Some(configured)
        },
    )
}

fn index_endpoint_key(surface: BridgeLocalSurface) -> &'static str {
    match surface {
        BridgeLocalSurface::Responses => "responses",
        BridgeLocalSurface::Messages => "messages",
        BridgeLocalSurface::ChatCompletions => "chat_completions",
    }
}

fn index_provider_key(source: AdapterSourceProduct) -> &'static str {
    match source {
        AdapterSourceProduct::KimiCodeMembership => "kimi",
        AdapterSourceProduct::AnthropicApi => "anthropic",
        AdapterSourceProduct::OpenaiApi => "openai",
        AdapterSourceProduct::XaiApi => "xai",
        AdapterSourceProduct::GlmCodingPlan => "glm",
        AdapterSourceProduct::DeepseekApi => "deepseek",
        AdapterSourceProduct::CodexChatGptSubscription => "codex",
        AdapterSourceProduct::ClaudeSubscription => "claude",
        AdapterSourceProduct::XaiGrokSubscription => "grok",
        AdapterSourceProduct::Other => "other",
    }
}

fn index_transport_key(protocol: BridgeUpstreamProtocol) -> &'static str {
    match protocol {
        BridgeUpstreamProtocol::OpenAiChatCompletions => "openai:generic",
        BridgeUpstreamProtocol::AnthropicMessages => "anthropic:claude",
        BridgeUpstreamProtocol::CodexResponsesOauth => "codex:codex",
        BridgeUpstreamProtocol::XaiResponsesOauth => "grok:grok",
    }
}

/// Safe input for beginning a local bridge saga. It contains no credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]

pub struct AdapterBridgePrepareRequest {
    pub source_kind: AdapterSourceKind,
    pub source_id: String,
    pub target_agent_id: AgentId,
    /// Used only for a newly-created profile. Existing profiles keep their
    /// explicit persisted preference; use `set_auto_start` to change one.
    #[serde(default = "default_auto_start")]
    pub auto_start: bool,
}

fn default_auto_start() -> bool {
    false
}

/// In-memory runtime data for one bridge profile.
///
/// This type intentionally has no serde implementation and redacts its debug
/// representation. It may cross from the Core service to the desktop host,
/// but must never be returned through a Tauri command or written to logs.
#[derive(Clone)]
pub struct AdapterBridgeRuntimeMaterial {
    profile_id: String,
    source_connection_id: String,
    preferred_port: Option<u16>,
    upstream_base_url: String,
    upstream_model: String,
    configured_listed_models: Vec<String>,
    context_window_tokens: Option<u32>,
    protocol: BridgeUpstreamProtocol,
    local_surface: BridgeLocalSurface,
    source: AdapterSourceProduct,
    target_agent: AgentId,
    upstream_auth: ResolvedAuth,
    local_bearer: String,
    route_index: Option<EffectiveRouteIndex>,
    /// When the v2 index flag is on, a non-zero preferred port is occupancy-failed
    /// instead of rebound, even before the pool is enrolled.
    index_enabled: bool,
    codex_ingress_grok_upstream: bool,
    grok_ingress_codex_upstream: bool,
    schedule_policy: RouteSchedulePolicy,
}

impl std::fmt::Debug for AdapterBridgeRuntimeMaterial {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdapterBridgeRuntimeMaterial")
            .field("profile_id", &self.profile_id)
            .field("source_connection_id", &self.source_connection_id)
            .field("preferred_port", &self.preferred_port)
            .field("upstream_base_url", &self.upstream_base_url)
            .field("upstream_model", &self.upstream_model)
            .field("configured_listed_models", &self.configured_listed_models)
            .field("context_window_tokens", &self.context_window_tokens)
            .field("protocol", &self.protocol)
            .field("local_surface", &self.local_surface)
            .field("source", &self.source)
            .field("target_agent", &self.target_agent)
            .field("upstream_auth", &self.upstream_auth)
            .field("local_bearer", &"REDACTED")
            .field(
                "route_index",
                &self.route_index.as_ref().map(|index| index.generation),
            )
            .field("index_enabled", &self.index_enabled)
            .field(
                "codex_ingress_grok_upstream",
                &self.codex_ingress_grok_upstream,
            )
            .field(
                "grok_ingress_codex_upstream",
                &self.grok_ingress_codex_upstream,
            )
            .field("schedule_policy", &self.schedule_policy)
            .finish()
    }
}

impl AdapterBridgeRuntimeMaterial {
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn preferred_port(&self) -> Option<u16> {
        self.preferred_port
    }

    pub fn protocol(&self) -> BridgeUpstreamProtocol {
        self.protocol
    }

    pub fn source_connection_id(&self) -> &str {
        &self.source_connection_id
    }

    /// Construct material for host/controller tests without a full prepare saga.
    /// Production callers must use `prepare` / restore paths only.
    pub fn for_test(
        profile_id: impl Into<String>,
        preferred_port: Option<u16>,
        local_bearer: impl Into<String>,
        upstream_token: impl Into<String>,
    ) -> Self {
        Self {
            profile_id: profile_id.into(),
            source_connection_id: "test-source".into(),
            preferred_port,
            upstream_base_url: KIMI_CHAT_BASE_URL.into(),
            upstream_model: DEFAULT_MODEL.into(),
            configured_listed_models: Vec::new(),
            context_window_tokens: None,
            protocol: BridgeUpstreamProtocol::OpenAiChatCompletions,
            local_surface: BridgeLocalSurface::Responses,
            source: AdapterSourceProduct::KimiCodeMembership,
            target_agent: AgentId::Codex,
            upstream_auth: ResolvedAuth::bearer(upstream_token),
            local_bearer: local_bearer.into(),
            route_index: None,
            index_enabled: false,
            codex_ingress_grok_upstream: false,
            grok_ingress_codex_upstream: false,
            schedule_policy: RouteSchedulePolicy::PriorityFailover,
        }
    }

    /// Build a host input without serializing either bearer. `port` can be
    /// `None` to reuse the persisted port, or `Some(0)` for explicit
    /// reallocation after a bind conflict.
    pub fn start_spec(&self, port: Option<u16>) -> BridgeStartSpec {
        let custom = crate::services::adapter_route_constants::is_custom_openai_compat_url(
            &self.upstream_base_url,
        );
        let lead_listed = listed_models_for_bridge(
            self.source,
            self.target_agent,
            &self.upstream_model,
            custom,
            &self.configured_listed_models,
        );
        let mut spec = BridgeStartSpec::new(
            self.profile_id.clone(),
            port.or(self.preferred_port).unwrap_or(0),
            self.local_bearer.clone(),
            BridgeUpstreamConfig {
                base_url: self.upstream_base_url.clone(),
                model: Some(self.upstream_model.clone()),
                source_connection_id: Some(self.source_connection_id.clone()),
                auth: self.upstream_auth.clone(),
                protocol: self.protocol,
                local_surface: self.local_surface,
            },
        )
        .with_listed_models(lead_listed)
        .with_mapping(self.source, self.target_agent, custom)
        .with_pair_adapter_flags(
            self.codex_ingress_grok_upstream,
            self.grok_ingress_codex_upstream,
        )
        .with_schedule_policy(self.schedule_policy);
        if let Some(index) = &self.route_index {
            spec = spec
                .with_listed_models(index.list_models(index_endpoint_key(self.local_surface)))
                .with_route_index(index.clone());
        }
        spec
    }

    pub fn route_index(&self) -> Option<&EffectiveRouteIndex> {
        self.route_index.as_ref()
    }

    pub fn freeze_gateway_port(&self) -> bool {
        self.preferred_port.is_some_and(|port| port != 0)
            && (self.route_index.is_some() || self.index_enabled)
    }

    /// Seed last-successful snapshots so a partial rebuild can keep them.
    pub fn with_prior_route_index(mut self, index: EffectiveRouteIndex) -> Self {
        self.route_index = Some(index);
        self
    }

    /// Host/controller tests: attach a production-built index without prepare.
    pub fn with_route_index_for_test(self, index: EffectiveRouteIndex) -> Self {
        self.with_prior_route_index(index)
    }

    pub fn with_index_enabled_for_test(mut self) -> Self {
        self.index_enabled = true;
        self
    }

    /// Verify a freshly bound listener before its generated provider becomes
    /// current. Both requests are read-only and strictly bounded: the local
    /// request proves the listener accepted its private bearer, and the Kimi
    /// models request catches an invalid/revoked credential, throttle, server
    /// failure, or clearly unreachable endpoint without creating a completion.
    ///
    /// The method deliberately exposes only stable error codes. Runtime
    /// bearers and endpoint details remain in this in-process object.
    pub async fn verify_bound_health(&self, port: u16) -> Result<()> {
        validate_bound_port(port)?;
        let client = reqwest::Client::builder()
            .connect_timeout(BRIDGE_HEALTH_TIMEOUT)
            .timeout(BRIDGE_HEALTH_TIMEOUT)
            .build()
            .map_err(|_| {
                AppError::message("adapter.bridge_health_local", "health client unavailable")
            })?;

        let local = client
            .get(format!("http://127.0.0.1:{port}/health"))
            .bearer_auth(&self.local_bearer)
            .send()
            .await
            .map_err(|_| {
                AppError::message(
                    "adapter.bridge_health_local",
                    "bound listener did not answer health check",
                )
            })?;
        if !local.status().is_success() {
            return Err(AppError::message(
                "adapter.bridge_health_local",
                "bound listener rejected authenticated health check",
            ));
        }

        // ChatGPT Codex Responses and the xAI CLI chat-proxy have no `/models`
        // endpoint. The authenticated loopback health check is the only safe
        // preflight; the first real request remains the upstream probe.
        if matches!(
            self.protocol,
            BridgeUpstreamProtocol::CodexResponsesOauth | BridgeUpstreamProtocol::XaiResponsesOauth
        ) {
            return Ok(());
        }

        let upstream_url = format!("{}/models", self.upstream_base_url.trim_end_matches('/'));
        let mut upstream_req = client.get(upstream_url);
        upstream_req = match self.protocol {
            BridgeUpstreamProtocol::OpenAiChatCompletions => {
                upstream_req.bearer_auth(self.upstream_auth.token())
            }
            BridgeUpstreamProtocol::AnthropicMessages => upstream_req
                .header("x-api-key", self.upstream_auth.token())
                .header(
                    "anthropic-version",
                    crate::services::adapter_route_constants::ANTHROPIC_API_VERSION,
                ),
            BridgeUpstreamProtocol::CodexResponsesOauth
            | BridgeUpstreamProtocol::XaiResponsesOauth => {
                upstream_req.bearer_auth(self.upstream_auth.token())
            }
        };
        let upstream = upstream_req.send().await.map_err(|_| {
            AppError::message(
                "adapter.bridge_health_upstream",
                "upstream health probe failed",
            )
        })?;
        if upstream.status().is_success() {
            return Ok(());
        }

        let code = match upstream.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => "adapter.bridge_upstream_auth",
            StatusCode::TOO_MANY_REQUESTS => "adapter.bridge_upstream_throttled",
            status if status.is_server_error() => "adapter.bridge_upstream_unavailable",
            _ => "adapter.bridge_health_upstream",
        };
        Err(AppError::message(
            code,
            "upstream health probe was not successful",
        ))
    }
}

/// A prepared bridge saga. This is an in-process only object: it carries
/// runtime credentials but never implements serde.
#[derive(Clone)]
pub struct AdapterBridgePrepared {
    profile: AdapterProfile,
    material: AdapterBridgeRuntimeMaterial,
    generated_provider_exists: bool,
    generated_provider_is_current: bool,
}

impl std::fmt::Debug for AdapterBridgePrepared {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdapterBridgePrepared")
            .field("profile", &self.profile)
            .field("material", &self.material)
            .field("generated_provider_exists", &self.generated_provider_exists)
            .field(
                "generated_provider_is_current",
                &self.generated_provider_is_current,
            )
            .finish()
    }
}

impl AdapterBridgePrepared {
    pub fn profile(&self) -> &AdapterProfile {
        &self.profile
    }

    pub fn runtime_material(&self) -> &AdapterBridgeRuntimeMaterial {
        &self.material
    }

    /// Produce the provider database mutation required after the host has
    /// bound `port`. The caller must use `ProviderService` for the actual
    /// write/switch; this service never writes a live configuration.
    pub fn provider_projection(&self, port: u16) -> Result<AdapterBridgeProviderProjection> {
        validate_bound_port(port)?;
        let input = projected_provider_input(
            &self.profile,
            &self.material.local_bearer,
            port,
            &self.material.upstream_model,
            self.material.context_window_tokens,
        )?;
        if self.generated_provider_exists {
            if self.generated_provider_is_current
                && self.profile.status == AdapterProfileStatus::Active
                && self.profile.local_port == Some(port)
            {
                Ok(AdapterBridgeProviderProjection::None)
            } else {
                Ok(AdapterBridgeProviderProjection::Update(input))
            }
        } else {
            Ok(AdapterBridgeProviderProjection::Create(input))
        }
    }
}

/// How a Tauri saga should persist a generated provider once the listener has
/// successfully bound. The input contains the local bearer and therefore must
/// stay entirely within Rust process memory.
#[derive(Clone)]
pub enum AdapterBridgeProviderProjection {
    Create(ProviderInput),
    Update(ProviderInput),
    None,
}

impl std::fmt::Debug for AdapterBridgeProviderProjection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Create(input) => formatter
                .debug_tuple("Create")
                .field(&format_args!("ProviderInput(id={})", input.id))
                .finish(),
            Self::Update(input) => formatter
                .debug_tuple("Update")
                .field(&format_args!("ProviderInput(id={})", input.id))
                .finish(),
            Self::None => formatter.write_str("None"),
        }
    }
}

/// Runtime material recovered on application startup for one eligible bridge
/// profile. It is intentionally non-serializable for the same reason as
/// [`AdapterBridgeRuntimeMaterial`].
#[derive(Clone)]
pub struct AdapterBridgeRestoreMaterial {
    profile: AdapterProfile,
    material: AdapterBridgeRuntimeMaterial,
    needs_reprojection: bool,
}

impl std::fmt::Debug for AdapterBridgeRestoreMaterial {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdapterBridgeRestoreMaterial")
            .field("profile", &self.profile)
            .field("material", &self.material)
            .field("needs_reprojection", &self.needs_reprojection)
            .finish()
    }
}

impl AdapterBridgeRestoreMaterial {
    pub fn profile(&self) -> &AdapterProfile {
        &self.profile
    }

    pub fn runtime_material(&self) -> &AdapterBridgeRuntimeMaterial {
        &self.material
    }

    pub fn needs_reprojection(&self) -> bool {
        self.needs_reprojection
    }
}

/// Validated, in-process-only input for removing a generated bridge.
///
/// The generated provider includes a loopback bearer, so this type must never
/// cross a serialization or logging boundary.  It lets the desktop controller
/// stop the listener between an authoritative preflight and the normal
/// `ProviderService` deletion, while retaining enough state to compensate a
/// rare profile-delete failure by recreating the non-current provider.
#[derive(Clone)]
pub struct AdapterBridgeRemoval {
    profile: AdapterProfile,
    generated_provider: Option<Provider>,
}

impl std::fmt::Debug for AdapterBridgeRemoval {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdapterBridgeRemoval")
            .field("profile", &self.profile)
            .field(
                "generated_provider_id",
                &self
                    .generated_provider
                    .as_ref()
                    .map(|provider| &provider.id),
            )
            .finish()
    }
}

impl AdapterBridgeRemoval {
    pub fn profile(&self) -> &AdapterProfile {
        &self.profile
    }

    pub fn generated_provider_id(&self) -> Option<&str> {
        self.generated_provider
            .as_ref()
            .map(|provider| provider.id.as_str())
    }

    /// Returns the exact non-current provider input only for an in-process
    /// rollback.  Callers must not serialize or log this value.
    pub fn recovery_input(&self) -> Option<ProviderInput> {
        self.generated_provider
            .as_ref()
            .map(|provider| ProviderInput {
                id: provider.id.clone(),
                agent_id: provider.agent_id,
                name: provider.name.clone(),
                settings_config: provider.settings_config.clone(),
                meta: provider.meta.clone(),
                is_current: false,
            })
    }
}

/// Credential-safe bridge profile and provider projection service.
pub struct AdapterBridgeService {
    pub(super) routes: AdapterRouteService,
    pub(super) profiles: AdapterProfileRepo,
    pub(super) providers: ProviderRepo,
    pub(super) secrets: AdapterSecretResolver,
    pub(super) route_pools: RoutePoolService,
}

impl AdapterBridgeService {
    pub fn new(db: Database) -> Self {
        Self {
            routes: AdapterRouteService::new(db.clone()),
            profiles: AdapterProfileRepo::new(db.clone()),
            providers: ProviderRepo::new(db.clone()),
            route_pools: RoutePoolService::new(db.clone()),
            secrets: AdapterSecretResolver::new(db),
        }
    }

    pub fn attach_route_index(
        &self,
        mut material: AdapterBridgeRuntimeMaterial,
        profile: &AdapterProfile,
    ) -> AdapterBridgeRuntimeMaterial {
        material.index_enabled = self.route_pools.index_enabled();
        let (codex_to_grok, grok_to_codex) = self.route_pools.pair_adapter_flags();
        material.codex_ingress_grok_upstream = codex_to_grok;
        material.grok_ingress_codex_upstream = grok_to_codex;
        let _ = self.route_pools.ensure_legacy_pool(profile);
        if let Ok(Some(pool)) = self.route_pools.get(&material.profile_id) {
            material.schedule_policy = pool.schedule_policy;
        }
        if let Some(index) = self.route_index_for_material(&material, profile) {
            if let Ok(Some(pool)) = self.route_pools.get(&material.profile_id) {
                if let Some(port) = pool.gateway_port {
                    material.preferred_port = Some(port);
                }
            }
            material.route_index = Some(index);
        } else {
            material.route_index = None;
        }
        material
    }

    /// Persist v2 enrollment only after the listener is already bound and
    /// healthy. Occupancy / bind / health failures must not call this.
    pub fn enroll_v2_after_bind(&self, profile: &AdapterProfile, port: u16) -> Result<bool> {
        if profile.route != AdapterRoute::LocalBridge {
            return Ok(false);
        }
        if !self.route_pools.index_enabled() {
            return Ok(false);
        }
        self.route_pools.ensure_legacy_pool(profile)?;
        self.route_pools.enroll_v2(&profile.id, port)?;
        Ok(true)
    }

    fn route_index_for_material(
        &self,
        material: &AdapterBridgeRuntimeMaterial,
        profile: &AdapterProfile,
    ) -> Option<EffectiveRouteIndex> {
        if !self.route_pools.index_enabled() {
            return None;
        }
        let pool = self.route_pools.get(&material.profile_id).ok().flatten()?;
        if !pool.v2_enrolled {
            return None;
        }
        let members = self.route_pools.list_members(&pool.id).ok()?;
        let rule = rule_for_id(&profile.rule_id)?;
        let endpoint = index_endpoint_key(material.local_surface);
        let prior = material
            .route_index
            .as_ref()
            .map(EffectiveRouteIndex::capability_snapshots);
        let mut listings = Vec::new();
        for member in members.into_iter().filter(|member| member.enabled) {
            listings.push(self.listing_for_member(material, &rule, profile, &member));
        }
        let index = index_from_member_listings(
            pool.id.clone(),
            pool.policy_revision.max(0) as u64,
            endpoint,
            &listings,
            prior.as_deref(),
        );
        if self.route_pools.mixed_provider_enabled() {
            let rules = self
                .route_pools
                .list_rules(&pool.id)
                .ok()
                .unwrap_or_default();
            Some(index.with_mixed_provider_rules(true, rules))
        } else {
            Some(index)
        }
    }

    fn listing_for_member(
        &self,
        material: &AdapterBridgeRuntimeMaterial,
        rule: &CodexBridgeRule,
        profile: &AdapterProfile,
        member: &RouteMember,
    ) -> MemberListing {
        let product = self
            .routes
            .classify_source_product(member.source_kind, &member.source_id)
            .unwrap_or(material.source);
        let member_rule =
            rule_for_member_product(product, profile.target_agent_id).unwrap_or(*rule);
        let provider = index_provider_key(product);
        let is_lead = member.source_kind == profile.source_kind
            && member.source_id == material.source_connection_id;
        if is_lead {
            let custom = crate::services::adapter_route_constants::is_custom_openai_compat_url(
                &material.upstream_base_url,
            );
            return MemberListing {
                member_id: member.source_id.clone(),
                listed_models: listed_models_for_bridge(
                    product,
                    material.target_agent,
                    &material.upstream_model,
                    custom,
                    &material.configured_listed_models,
                ),
                upstream_provider: provider.to_owned(),
                upstream_dialect: provider.to_owned(),
                upstream_endpoint: material.upstream_base_url.clone(),
                transport_key: index_transport_key(material.protocol).to_owned(),
                snapshot_ok: true,
            };
        }
        let snapshot_ok = self
            .resolve_member_auth(member_rule.rule_id, member.source_kind, &member.source_id)
            .map(|auth| auth.has_token())
            .unwrap_or(false);
        if !snapshot_ok {
            return MemberListing {
                member_id: member.source_id.clone(),
                listed_models: Vec::new(),
                upstream_provider: provider.to_owned(),
                upstream_dialect: provider.to_owned(),
                upstream_endpoint: member_rule.upstream_base_url.to_owned(),
                transport_key: index_transport_key(member_rule.protocol).to_owned(),
                snapshot_ok: false,
            };
        }
        let (url, model, configured, protocol, _) = prepare::openai_source_upstream(
            self,
            &member_rule,
            member.source_kind,
            &member.source_id,
        );
        let custom = crate::services::adapter_route_constants::is_custom_openai_compat_url(&url);
        MemberListing {
            member_id: member.source_id.clone(),
            listed_models: listed_models_for_bridge(
                product,
                member_rule.target_agent,
                &model,
                custom,
                &configured,
            ),
            upstream_provider: provider.to_owned(),
            upstream_dialect: provider.to_owned(),
            upstream_endpoint: url,
            transport_key: index_transport_key(protocol).to_owned(),
            snapshot_ok: true,
        }
    }
}

fn rule_for_member_product(
    product: AdapterSourceProduct,
    target_agent: AgentId,
) -> Option<CodexBridgeRule> {
    LIVE_BRIDGE_RULES
        .iter()
        .copied()
        .find(|rule| rule.source == product && rule.target_agent == target_agent)
        .or_else(|| {
            LIVE_BRIDGE_RULES
                .iter()
                .copied()
                .find(|rule| rule.source == product)
        })
}
