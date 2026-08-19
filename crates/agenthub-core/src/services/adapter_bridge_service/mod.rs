//! Synchronous control-plane persistence for Kimi Code membership -> Codex.
//!
//! This service deliberately does **not** own the loopback listener or write a
//! live Codex configuration. A Tauri host drives the saga in this order:
//!
//! 1. [`AdapterBridgeService::prepare`] creates/reuses an `applying` profile
//!    and returns non-serializable runtime material.
//! 2. The host starts and probes a [`crate::bridge::BridgeRuntimeHost`] using
//!    [`AdapterBridgePrepared::start_spec`].
//! 3. The host persists the returned [`AdapterBridgeProviderProjection`] via
//!    `ProviderService`, switches it through the normal live-config owner,
//!    then calls [`AdapterBridgeService::finalize`].
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

use crate::bridge::{BridgeStartSpec, BridgeUpstreamConfig, BridgeUpstreamProtocol, ResolvedAuth};
use crate::error::{AppError, Result};
use crate::models::{
    AdapterProfile, AdapterProfileFilter, AdapterProfileMode, AdapterProfileStatus, AdapterRoute,
    AdapterRouteRequest, AdapterSourceKind, AdapterSupport, AgentId, Provider, ProviderInput,
};
use crate::services::{AdapterRouteService, AdapterSecretResolver};
use crate::storage::{AdapterProfileRepo, Database, ProviderRepo};

const RULE_ID: &str = "kimi-membership-to-codex-v1";
const ANTHROPIC_RULE_ID: &str = "anthropic-api-to-codex-v1";
const CODEX_CLAUDE_RULE_ID: &str = "codex-subscription-to-claude-responses-v1";
const GROK_CLAUDE_RULE_ID: &str = "grok-subscription-to-claude-v1";
const RULE_VERSION: &str = "1";
const KIMI_CHAT_BASE_URL: &str = "https://api.kimi.com/coding/v1";
const ANTHROPIC_MESSAGES_BASE_URL: &str = "https://api.anthropic.com/v1";
const CHATGPT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex/";
const DEFAULT_MODEL: &str = "kimi-k2.5";
const ANTHROPIC_DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";
const CODEX_DEFAULT_MODEL: &str = "gpt-5.4";
const PROVIDER_SLUG: &str = "agenthub_kimi_bridge";
const ANTHROPIC_PROVIDER_SLUG: &str = "agenthub_anthropic_bridge";
const CODEX_CLAUDE_PROVIDER_SLUG: &str = "claude-codex-adapter-bridge";
const GROK_CLAUDE_BASE_URL: &str = "https://api.x.ai/v1";
const GROK_CLAUDE_DEFAULT_MODEL: &str = "grok-4.5";
const GENERATED_BY: &str = "adapter";
const BRIDGE_HEALTH_TIMEOUT: Duration = Duration::from_secs(4);
const RETRYABLE_ERROR_PREFIX: &str = "retryable:";

#[derive(Clone, Copy)]
struct CodexBridgeRule {
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
    bridge_kind: &'static str,
    target_agent: AgentId,
    mode: AdapterProfileMode,
}

const KIMI_CODEX_RULE: CodexBridgeRule = CodexBridgeRule {
    rule_id: RULE_ID,
    profile_prefix: "adapter-kimi-codex-bridge",
    provider_prefix: "codex-kimi-adapter-bridge",
    profile_name: "Kimi → Codex Bridge",
    provider_name: "Kimi Code Bridge",
    toml_name: "AgentHub Kimi Bridge",
    provider_slug: PROVIDER_SLUG,
    upstream_base_url: KIMI_CHAT_BASE_URL,
    default_model: DEFAULT_MODEL,
    protocol: BridgeUpstreamProtocol::KimiChatCompletions,
    bridge_kind: "responses_to_chat_completions",
    target_agent: AgentId::Codex,
    mode: AdapterProfileMode::Api,
};

const ANTHROPIC_CODEX_RULE: CodexBridgeRule = CodexBridgeRule {
    rule_id: ANTHROPIC_RULE_ID,
    profile_prefix: "adapter-anthropic-codex-bridge",
    provider_prefix: "codex-anthropic-adapter-bridge",
    profile_name: "Anthropic → Codex Bridge",
    provider_name: "Anthropic Bridge",
    toml_name: "AgentHub Anthropic Bridge",
    provider_slug: ANTHROPIC_PROVIDER_SLUG,
    upstream_base_url: ANTHROPIC_MESSAGES_BASE_URL,
    default_model: ANTHROPIC_DEFAULT_MODEL,
    protocol: BridgeUpstreamProtocol::AnthropicMessages,
    bridge_kind: "responses_to_anthropic_messages",
    target_agent: AgentId::Codex,
    mode: AdapterProfileMode::Api,
};

const CODEX_CLAUDE_RULE: CodexBridgeRule = CodexBridgeRule {
    rule_id: CODEX_CLAUDE_RULE_ID,
    profile_prefix: "adapter-codex-claude-bridge",
    provider_prefix: CODEX_CLAUDE_PROVIDER_SLUG,
    profile_name: "Codex → Claude Code Bridge",
    provider_name: "Codex Subscription Bridge",
    toml_name: "",
    provider_slug: "",
    upstream_base_url: CHATGPT_CODEX_BASE_URL,
    default_model: CODEX_DEFAULT_MODEL,
    protocol: BridgeUpstreamProtocol::CodexResponsesOauth,
    bridge_kind: "messages_to_codex_responses",
    target_agent: AgentId::Claude,
    mode: AdapterProfileMode::Oauth,
};

const GROK_CLAUDE_RULE: CodexBridgeRule = CodexBridgeRule {
    rule_id: GROK_CLAUDE_RULE_ID,
    profile_prefix: "adapter-grok-claude-bridge",
    provider_prefix: "claude-grok-adapter-bridge",
    profile_name: "Grok → Claude Code Bridge",
    provider_name: "Grok Subscription Bridge",
    toml_name: "",
    provider_slug: "",
    upstream_base_url: GROK_CLAUDE_BASE_URL,
    default_model: GROK_CLAUDE_DEFAULT_MODEL,
    protocol: BridgeUpstreamProtocol::KimiChatCompletions,
    bridge_kind: "messages_to_xai_chat_completions",
    target_agent: AgentId::Claude,
    mode: AdapterProfileMode::Oauth,
};

/// Live local-bridge writers. `rule_for_id` and the secret-resolver coverage
/// test both read this slice so a new rule cannot ship without a matcher check.
const LIVE_BRIDGE_RULES: &[CodexBridgeRule] = &[
    KIMI_CODEX_RULE,
    ANTHROPIC_CODEX_RULE,
    CODEX_CLAUDE_RULE,
    GROK_CLAUDE_RULE,
];

mod finalize;
mod prepare;
mod removal;
mod rules;

use rules::*;
pub(super) use rules::*;

#[cfg(test)]
mod tests;

pub(super) fn rule_for_id(rule_id: &str) -> Option<CodexBridgeRule> {
    LIVE_BRIDGE_RULES
        .iter()
        .copied()
        .find(|rule| rule.rule_id == rule_id)
}

/// `(target_agent, rule_id)` for every live local-bridge writer.
pub(crate) fn live_bridge_rule_projections() -> impl Iterator<Item = (AgentId, &'static str)> {
    LIVE_BRIDGE_RULES
        .iter()
        .map(|rule| (rule.target_agent, rule.rule_id))
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
    protocol: BridgeUpstreamProtocol,
    upstream_auth: ResolvedAuth,
    local_bearer: String,
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
            .field("protocol", &self.protocol)
            .field("upstream_auth", &self.upstream_auth)
            .field("local_bearer", &"REDACTED")
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
            protocol: BridgeUpstreamProtocol::KimiChatCompletions,
            upstream_auth: ResolvedAuth::bearer(upstream_token),
            local_bearer: local_bearer.into(),
        }
    }

    /// Build a host input without serializing either bearer. `port` can be
    /// `None` to reuse the persisted port, or `Some(0)` for explicit
    /// reallocation after a bind conflict.
    pub fn start_spec(&self, port: Option<u16>) -> BridgeStartSpec {
        BridgeStartSpec::new(
            self.profile_id.clone(),
            port.or(self.preferred_port).unwrap_or(0),
            self.local_bearer.clone(),
            BridgeUpstreamConfig {
                base_url: self.upstream_base_url.clone(),
                model: Some(self.upstream_model.clone()),
                source_connection_id: Some(self.source_connection_id.clone()),
                auth: self.upstream_auth.clone(),
                protocol: self.protocol,
            },
        )
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

        // ChatGPT's Codex Responses surface has no `/models` endpoint. The
        // authenticated loopback health check is the only safe preflight for
        // this upstream; the first real request remains the upstream probe.
        if self.protocol == BridgeUpstreamProtocol::CodexResponsesOauth {
            return Ok(());
        }

        let upstream_url = format!("{}/models", self.upstream_base_url.trim_end_matches('/'));
        let mut upstream_req = client.get(upstream_url);
        upstream_req = match self.protocol {
            BridgeUpstreamProtocol::KimiChatCompletions => {
                upstream_req.bearer_auth(self.upstream_auth.token())
            }
            BridgeUpstreamProtocol::AnthropicMessages => upstream_req
                .header("x-api-key", self.upstream_auth.token())
                .header(
                    "anthropic-version",
                    crate::services::adapter_route_constants::ANTHROPIC_API_VERSION,
                ),
            BridgeUpstreamProtocol::CodexResponsesOauth => {
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
}

impl std::fmt::Debug for AdapterBridgePrepared {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdapterBridgePrepared")
            .field("profile", &self.profile)
            .field("material", &self.material)
            .field("generated_provider_exists", &self.generated_provider_exists)
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
        let input = projected_provider_input(&self.profile, &self.material.local_bearer, port)?;
        if self.generated_provider_exists {
            if self.profile.status == AdapterProfileStatus::Active
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
}

impl std::fmt::Debug for AdapterBridgeRestoreMaterial {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdapterBridgeRestoreMaterial")
            .field("profile", &self.profile)
            .field("material", &self.material)
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
}

impl AdapterBridgeService {
    pub fn new(db: Database) -> Self {
        Self {
            routes: AdapterRouteService::new(db.clone()),
            profiles: AdapterProfileRepo::new(db.clone()),
            providers: ProviderRepo::new(db.clone()),
            secrets: AdapterSecretResolver::new(db),
        }
    }
}
