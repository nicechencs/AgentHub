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
//! local bearer is persisted solely in the generated Codex provider's `auth`
//! object; adapter profiles retain only the provider id.

use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use toml_edit::DocumentMut;

use crate::bridge::{
    BridgeStartSpec, BridgeUpstreamConfig, BridgeUpstreamProtocol, ResolvedAuth,
};
use crate::error::{AppError, Result};
use crate::models::{
    AdapterProfile, AdapterProfileFilter, AdapterProfileMode, AdapterProfileStatus, AdapterRoute,
    AdapterRouteRequest, AdapterSourceKind, AdapterSupport, AgentId, Provider, ProviderInput,
};
use crate::services::{AdapterRouteService, AdapterSecretResolver};
use crate::storage::{AdapterProfileRepo, Database, ProviderRepo};

const RULE_ID: &str = "kimi-membership-to-codex-v1";
const ANTHROPIC_RULE_ID: &str = "anthropic-api-to-codex-v1";
const RULE_VERSION: &str = "1";
const KIMI_CHAT_BASE_URL: &str = "https://api.kimi.com/coding/v1";
const ANTHROPIC_MESSAGES_BASE_URL: &str = "https://api.anthropic.com/v1";
const DEFAULT_MODEL: &str = "kimi-k2.5";
const ANTHROPIC_DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";
const PROVIDER_SLUG: &str = "agenthub_kimi_bridge";
const ANTHROPIC_PROVIDER_SLUG: &str = "agenthub_anthropic_bridge";
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
};

fn rule_for_id(rule_id: &str) -> Option<CodexBridgeRule> {
    match rule_id {
        RULE_ID => Some(KIMI_CODEX_RULE),
        ANTHROPIC_RULE_ID => Some(ANTHROPIC_CODEX_RULE),
        _ => None,
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
    routes: AdapterRouteService,
    profiles: AdapterProfileRepo,
    providers: ProviderRepo,
    secrets: AdapterSecretResolver,
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

    /// Deterministically resolve the profile identity before entering a host
    /// saga lock. This has no side effects and lets every lifecycle operation
    /// (apply/start/stop/restore/remove) use the same per-profile authority.
    pub fn profile_id_for_request(&self, request: &AdapterBridgePrepareRequest) -> Result<String> {
        let rule = self.ensure_supported(request)?;
        Ok(stable_id(rule.profile_prefix, request.source_id.trim()))
    }

    /// Begin an idempotent local-bridge saga without starting a listener or
    /// writing any live agent configuration.
    pub fn prepare(&self, request: &AdapterBridgePrepareRequest) -> Result<AdapterBridgePrepared> {
        let rule = self.ensure_supported(request)?;
        let source_id = request.source_id.trim();
        // Validate/retrieve the source before any profile row is written. The
        // token stays only in the returned in-memory material.
        let upstream_auth = match rule.protocol {
            BridgeUpstreamProtocol::KimiChatCompletions => {
                self.secrets.resolve_kimi_membership_auth(source_id)?
            }
            BridgeUpstreamProtocol::AnthropicMessages => self
                .secrets
                .resolve_anthropic_auth(request.source_kind, source_id)?,
        };
        let profile_id = stable_id(rule.profile_prefix, source_id);
        let provider_id = stable_id(rule.provider_prefix, source_id);
        let stamp = now();
        let proposed = AdapterProfile {
            id: profile_id,
            name: format!("{} ({})", rule.profile_name, safe_label(source_id)),
            source_kind: request.source_kind,
            source_id: source_id.into(),
            target_agent_id: AgentId::Codex,
            route: AdapterRoute::LocalBridge,
            mode: AdapterProfileMode::Api,
            status: AdapterProfileStatus::Applying,
            rule_id: rule.rule_id.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: request.auto_start,
            last_error_code: None,
            created_at: stamp.clone(),
            updated_at: stamp,
        };

        let prior_profile = self.profiles.get(&proposed.id)?;
        let existing_provider = self.providers.get_by_id(&provider_id)?;
        // A provider can only be reused when the profile already established
        // ownership. This prevents a user-created/provider-id collision from
        // being silently adopted by a new bridge profile.
        if prior_profile.is_none() && existing_provider.is_some() {
            return Err(AppError::message(
                "adapter.provider_conflict",
                "generated provider id is owned by another provider",
            ));
        }

        let mut profile = self.profiles.create_or_get(&proposed)?;
        if !same_profile_contract(&profile, &proposed) {
            return Err(AppError::message(
                "adapter.profile_conflict",
                "adapter profile conflicts with requested bridge rule",
            ));
        }

        let generated_provider_exists = if let Some(provider) = existing_provider.as_ref() {
            validate_generated_provider(provider, &profile, profile.local_port)?;
            true
        } else {
            false
        };

        if profile.status == AdapterProfileStatus::NeedsAttention {
            profile.status = AdapterProfileStatus::Applying;
            profile.last_error_code = None;
            profile.updated_at = now();
            profile = self.profiles.update(&profile)?;
        }

        let local_bearer = match existing_provider.as_ref() {
            Some(provider) => local_bearer_from_provider(provider)?,
            None => generate_local_bearer()?,
        };
        Ok(AdapterBridgePrepared {
            material: AdapterBridgeRuntimeMaterial {
                profile_id: profile.id.clone(),
                source_connection_id: profile.source_id.clone(),
                preferred_port: profile.local_port,
                upstream_base_url: rule.upstream_base_url.into(),
                upstream_model: rule.default_model.into(),
                protocol: rule.protocol,
                upstream_auth,
                local_bearer,
            },
            profile,
            generated_provider_exists,
        })
    }

    /// Re-read the generated provider immediately before the desktop host
    /// persists its projection. `prepare` can run before listener bind/health,
    /// so its cached existence result is not safe to use after that gap.  This
    /// revalidation rejects a user-owned id collision and derives Create,
    /// Update, or None from the current row while the caller holds its provider
    /// live-saga guard.
    pub fn revalidate_provider_projection(
        &self,
        prepared: &AdapterBridgePrepared,
        port: u16,
    ) -> Result<AdapterBridgeProviderProjection> {
        validate_bound_port(port)?;
        let profile = self.profiles.get(&prepared.profile.id)?.ok_or_else(|| {
            AppError::NotFound(format!(
                "adapter profile not found: {}",
                prepared.profile.id
            ))
        })?;
        if !same_profile_contract(&profile, &prepared.profile) {
            return Err(AppError::message(
                "adapter.profile_conflict",
                "adapter profile changed while bridge was starting",
            ));
        }
        let provider_id = profile.generated_provider_id.as_deref().ok_or_else(|| {
            AppError::message(
                "adapter.provider_conflict",
                "bridge profile has no generated provider id",
            )
        })?;
        let input = projected_provider_input(&profile, &prepared.material.local_bearer, port)?;
        match self.providers.get_by_id(provider_id)? {
            Some(provider) => {
                validate_generated_provider(&provider, &profile, profile.local_port)?;
                if local_bearer_from_provider(&provider)? != prepared.material.local_bearer {
                    return Err(AppError::message(
                        "adapter.provider_conflict",
                        "generated bridge provider bearer changed while bridge was starting",
                    ));
                }
                if profile.status == AdapterProfileStatus::Active
                    && profile.local_port == Some(port)
                {
                    Ok(AdapterBridgeProviderProjection::None)
                } else {
                    Ok(AdapterBridgeProviderProjection::Update(input))
                }
            }
            None => Ok(AdapterBridgeProviderProjection::Create(input)),
        }
    }

    /// Mark a saga complete after its generated provider was persisted and
    /// switched by `ProviderService`. This method owns profile state only.
    pub fn finalize(
        &self,
        prepared: &AdapterBridgePrepared,
        bound_port: u16,
    ) -> Result<AdapterProfile> {
        validate_bound_port(bound_port)?;
        let mut profile = self.profiles.get(&prepared.profile.id)?.ok_or_else(|| {
            AppError::NotFound(format!(
                "adapter profile not found: {}",
                prepared.profile.id
            ))
        })?;
        if !same_profile_contract(&profile, &prepared.profile) {
            return Err(AppError::message(
                "adapter.profile_conflict",
                "adapter profile changed while bridge was starting",
            ));
        }
        let provider_id = profile.generated_provider_id.as_deref().ok_or_else(|| {
            AppError::message(
                "adapter.provider_conflict",
                "bridge profile has no generated provider id",
            )
        })?;
        let provider = self.providers.get_by_id(provider_id)?.ok_or_else(|| {
            AppError::message(
                "adapter.provider_missing",
                "generated bridge provider is missing",
            )
        })?;
        validate_generated_provider(&provider, &profile, Some(bound_port))?;
        if local_bearer_from_provider(&provider)? != prepared.material.local_bearer {
            return Err(AppError::message(
                "adapter.provider_conflict",
                "generated bridge provider bearer changed while bridge was starting",
            ));
        }

        if profile.status == AdapterProfileStatus::Active
            && profile.local_port == Some(bound_port)
            && profile.last_error_code.is_none()
        {
            return Ok(profile);
        }
        profile.status = AdapterProfileStatus::Active;
        profile.local_port = Some(bound_port);
        profile.last_error_code = None;
        profile.updated_at = now();
        self.profiles.update(&profile)
    }

    /// Record a host/projection/switch failure without storing its dynamic
    /// message. Stable error codes are safe for UI and log correlation.
    pub fn mark_needs_attention(
        &self,
        profile_id: &str,
        error_code: &str,
    ) -> Result<AdapterProfile> {
        let mut profile = self.bridge_profile(profile_id)?;
        let code = error_code.trim();
        if code.is_empty() {
            return Err(AppError::InvalidArg(
                "adapter bridge error code must not be empty".into(),
            ));
        }
        profile.status = AdapterProfileStatus::NeedsAttention;
        profile.last_error_code = Some(code.into());
        profile.updated_at = now();
        self.profiles.update(&profile)
    }

    /// Record a transient restoration failure without changing the persisted
    /// status enum. Older profile schemas constrain `status`, so the explicit
    /// `retryable:` marker in `last_error_code` preserves the already-consistent
    /// `active` projection. Unlike `needs_attention`, it never claims a
    /// recoverable runtime failure is a persisted consistency failure.
    pub fn mark_retryable(&self, profile_id: &str, error_code: &str) -> Result<AdapterProfile> {
        let mut profile = self.bridge_profile(profile_id)?;
        let code = error_code.trim();
        if code.is_empty() {
            return Err(AppError::InvalidArg(
                "adapter bridge error code must not be empty".into(),
            ));
        }
        // Do not promote a first-time `applying` profile: without a generated
        // provider and bound port it has no active projection to restore.
        // Existing active profiles retain their active projection and are
        // therefore retried by `list_auto_start_profiles`.
        profile.last_error_code = Some(format!("{RETRYABLE_ERROR_PREFIX}{code}"));
        profile.updated_at = now();
        self.profiles.update(&profile)
    }

    /// Clear only a transient runtime marker after a successful restore.
    /// Deliberately leaves any other error (in particular `NeedsAttention`)
    /// untouched so a healthy listener cannot erase an inconsistency signal.
    pub fn clear_retryable_error(&self, profile_id: &str) -> Result<AdapterProfile> {
        let mut profile = self.bridge_profile(profile_id)?;
        let retryable = profile
            .last_error_code
            .as_deref()
            .is_some_and(|code| code.starts_with(RETRYABLE_ERROR_PREFIX));
        if !retryable {
            return Ok(profile);
        }
        profile.last_error_code = None;
        profile.updated_at = now();
        self.profiles.update(&profile)
    }

    /// Build a demoted provider projection for a restore-time port rebind.
    /// The desktop host writes the row through `ProviderService` under its
    /// live-saga guard, then calls [`Self::persist_restored_port`].
    pub fn projection_for_restored_port(
        &self,
        profile_id: &str,
        bound_port: u16,
    ) -> Result<(ProviderInput, bool)> {
        validate_bound_port(bound_port)?;
        let profile = self.bridge_profile(profile_id)?;
        if profile.status != AdapterProfileStatus::Active {
            return Err(AppError::InvalidArg(
                "only active bridge profiles can realign a restored port".into(),
            ));
        }
        let provider_id = profile.generated_provider_id.as_deref().ok_or_else(|| {
            AppError::message(
                "adapter.provider_missing",
                "bridge profile has no generated provider id",
            )
        })?;
        let provider = self.providers.get_by_id(provider_id)?.ok_or_else(|| {
            AppError::message(
                "adapter.provider_missing",
                "generated bridge provider is missing",
            )
        })?;
        validate_generated_provider(&provider, &profile, profile.local_port)?;
        let local_bearer = local_bearer_from_provider(&provider)?;
        let mut input = projected_provider_input(&profile, &local_bearer, bound_port)?;
        input.is_current = false;
        Ok((input, provider.is_current))
    }

    /// Persist the bound port after a successful restore-time rebind and clear
    /// any retryable marker on the active profile.
    pub fn persist_restored_port(
        &self,
        profile_id: &str,
        bound_port: u16,
    ) -> Result<AdapterProfile> {
        validate_bound_port(bound_port)?;
        let mut profile = self.bridge_profile(profile_id)?;
        profile.local_port = Some(bound_port);
        profile.last_error_code = None;
        profile.updated_at = now();
        self.profiles.update(&profile)
    }

    /// Update only the persisted host-restore preference for a bridge profile.
    pub fn set_auto_start(&self, profile_id: &str, auto_start: bool) -> Result<AdapterProfile> {
        let mut profile = self.bridge_profile(profile_id)?;
        if profile.auto_start == auto_start {
            return Ok(profile);
        }
        profile.auto_start = auto_start;
        profile.updated_at = now();
        self.profiles.update(&profile)
    }

    /// Validates that a profile and its generated provider are the exact
    /// Kimi-membership -> Codex bridge projection before deletion.
    ///
    /// The current provider is always rejected: callers must switch the
    /// Codex Connection first. A listener is deliberately not stopped here;
    /// the desktop controller performs that reversible operation between this
    /// preflight and [`Self::complete_remove`].
    pub fn preflight_remove(&self, profile_id: &str) -> Result<AdapterBridgeRemoval> {
        let profile = self.bridge_profile(profile_id)?;
        let rule = rule_for_id(&profile.rule_id).ok_or_else(|| {
            AppError::InvalidArg("adapter profile is not a supported Codex bridge".into())
        })?;
        let expected_provider_id = stable_id(rule.provider_prefix, &profile.source_id);
        if profile.generated_provider_id.as_deref() != Some(expected_provider_id.as_str()) {
            return Err(AppError::message(
                "adapter.provider_conflict",
                "bridge profile generated provider id is invalid",
            ));
        }

        let generated_provider = match self.providers.get_by_id(&expected_provider_id)? {
            Some(provider) => {
                validate_generated_provider(&provider, &profile, profile.local_port)?;
                if provider.is_current {
                    return Err(AppError::Unsupported(
                        "先在 Connections 切换后再移除此适配器".into(),
                    ));
                }
                Some(provider)
            }
            // A listener or projection failure can leave an applying or
            // retryable/attention-needed profile before any generated provider exists.
            // Such a profile is still safe to clean up, but an active profile
            // without its projection is corrupted and must fail closed.
            None if profile.status != AdapterProfileStatus::Active => None,
            None => {
                return Err(AppError::message(
                    "adapter.provider_missing",
                    "active bridge profile has no generated provider",
                ));
            }
        };

        Ok(AdapterBridgeRemoval {
            profile,
            generated_provider,
        })
    }

    /// Deletes the already-validated profile after its non-current generated
    /// provider has been removed through `ProviderService`.
    ///
    /// The check is repeated after listener shutdown to detect any profile or
    /// provider mutation that occurred after `preflight_remove`.
    pub fn complete_remove(&self, removal: &AdapterBridgeRemoval) -> Result<()> {
        let profile = self.bridge_profile(&removal.profile.id)?;
        if profile.generated_provider_id != removal.profile.generated_provider_id
            || profile.source_id != removal.profile.source_id
            || profile.target_agent_id != removal.profile.target_agent_id
            || profile.rule_id != removal.profile.rule_id
            || profile.rule_version != removal.profile.rule_version
        {
            return Err(AppError::message(
                "adapter.profile_conflict",
                "adapter bridge profile changed while removing it",
            ));
        }
        if let Some(provider_id) = removal.generated_provider_id() {
            if self.providers.get_by_id(provider_id)?.is_some() {
                return Err(AppError::message(
                    "adapter.provider_conflict",
                    "generated bridge provider still exists during profile removal",
                ));
            }
        }
        self.profiles.delete(&profile.id)
    }

    /// Safe metadata for profiles that the desktop host should attempt to
    /// restore. Call `resolve_restore_material` per profile and report any
    /// individual failure through `mark_needs_attention` so one bad source
    /// cannot prevent other bridges from being restored.
    pub fn list_auto_start_profiles(&self) -> Result<Vec<AdapterProfile>> {
        self.profiles.list_filtered(&AdapterProfileFilter {
            route: Some(AdapterRoute::LocalBridge),
            status: Some(AdapterProfileStatus::Active),
            auto_start: Some(true),
            ..AdapterProfileFilter::default()
        })
    }

    /// Re-resolve ephemeral upstream auth and local bearer for one persisted
    /// active bridge. This is for application startup only; it never starts a
    /// host or writes either profile/provider row.
    pub fn resolve_restore_material(
        &self,
        profile_id: &str,
    ) -> Result<AdapterBridgeRestoreMaterial> {
        let profile = self.bridge_profile(profile_id)?;
        if profile.status != AdapterProfileStatus::Active || !profile.auto_start {
            return Err(AppError::InvalidArg(
                "adapter bridge profile is not eligible for automatic restore".into(),
            ));
        }
        let local_port = profile.local_port.ok_or_else(|| {
            AppError::message(
                "adapter.profile_invalid",
                "active bridge profile has no local port",
            )
        })?;
        let provider_id = profile.generated_provider_id.as_deref().ok_or_else(|| {
            AppError::message(
                "adapter.provider_missing",
                "bridge profile has no generated provider id",
            )
        })?;
        let provider = self.providers.get_by_id(provider_id)?.ok_or_else(|| {
            AppError::message(
                "adapter.provider_missing",
                "generated bridge provider is missing",
            )
        })?;
        validate_generated_provider(&provider, &profile, Some(local_port))?;
        let rule = rule_for_id(&profile.rule_id).ok_or_else(|| {
            AppError::InvalidArg("adapter profile is not a supported Codex bridge".into())
        })?;
        let upstream_auth = match rule.protocol {
            BridgeUpstreamProtocol::KimiChatCompletions => {
                self.secrets.resolve_kimi_membership_auth(&profile.source_id)?
            }
            BridgeUpstreamProtocol::AnthropicMessages => self
                .secrets
                .resolve_anthropic_auth(profile.source_kind, &profile.source_id)?,
        };
        Ok(AdapterBridgeRestoreMaterial {
            material: AdapterBridgeRuntimeMaterial {
                profile_id: profile.id.clone(),
                source_connection_id: profile.source_id.clone(),
                preferred_port: Some(local_port),
                upstream_base_url: rule.upstream_base_url.into(),
                upstream_model: rule.default_model.into(),
                protocol: rule.protocol,
                upstream_auth,
                local_bearer: local_bearer_from_provider(&provider)?,
            },
            profile,
        })
    }

    fn ensure_supported(&self, request: &AdapterBridgePrepareRequest) -> Result<CodexBridgeRule> {
        let analysis = self.routes.analyze(&AdapterRouteRequest {
            source_kind: request.source_kind,
            source_id: request.source_id.clone(),
            target_agent_id: request.target_agent_id,
        })?;
        let rule = analysis
            .rule_id
            .as_deref()
            .and_then(rule_for_id)
            .ok_or_else(|| {
                AppError::Unsupported(
                    "adapter bridge currently supports Kimi membership or Anthropic API → Codex"
                        .into(),
                )
            })?;
        let source_ok = match rule.protocol {
            BridgeUpstreamProtocol::KimiChatCompletions => {
                request.source_kind == AdapterSourceKind::Provider
            }
            BridgeUpstreamProtocol::AnthropicMessages => matches!(
                request.source_kind,
                AdapterSourceKind::Provider | AdapterSourceKind::Account
            ),
        };
        if source_ok
            && request.target_agent_id == AgentId::Codex
            && analysis.route == AdapterRoute::LocalBridge
            && analysis.support == AdapterSupport::Experimental
        {
            Ok(rule)
        } else {
            Err(AppError::Unsupported(
                "adapter bridge currently supports Kimi membership or Anthropic API → Codex".into(),
            ))
        }
    }

    fn bridge_profile(&self, profile_id: &str) -> Result<AdapterProfile> {
        let profile_id = profile_id.trim();
        if profile_id.is_empty() {
            return Err(AppError::InvalidArg(
                "adapter bridge profile id must not be empty".into(),
            ));
        }
        let profile = self.profiles.get(profile_id)?.ok_or_else(|| {
            AppError::NotFound(format!("adapter profile not found: {profile_id}"))
        })?;
        let supported_source = match profile.rule_id.as_str() {
            RULE_ID => profile.source_kind == AdapterSourceKind::Provider,
            ANTHROPIC_RULE_ID => matches!(
                profile.source_kind,
                AdapterSourceKind::Provider | AdapterSourceKind::Account
            ),
            _ => false,
        };
        if !supported_source
            || profile.target_agent_id != AgentId::Codex
            || profile.route != AdapterRoute::LocalBridge
            || rule_for_id(&profile.rule_id).is_none()
            || profile.rule_version != RULE_VERSION
        {
            return Err(AppError::InvalidArg(
                "adapter profile is not a supported Codex bridge".into(),
            ));
        }
        Ok(profile)
    }
}

fn projected_provider_input(
    profile: &AdapterProfile,
    local_bearer: &str,
    port: u16,
) -> Result<ProviderInput> {
    validate_bound_port(port)?;
    let rule = rule_for_id(&profile.rule_id).ok_or_else(|| {
        AppError::InvalidArg("adapter profile is not a supported Codex bridge".into())
    })?;
    let provider_id = profile.generated_provider_id.as_deref().ok_or_else(|| {
        AppError::message(
            "adapter.provider_conflict",
            "bridge profile has no generated provider id",
        )
    })?;
    let local_bearer = local_bearer.trim();
    if local_bearer.is_empty() {
        return Err(AppError::message(
            "adapter.local_bearer",
            "bridge local bearer is unavailable",
        ));
    }
    Ok(ProviderInput {
        id: provider_id.into(),
        agent_id: AgentId::Codex,
        name: format!("{} ({})", rule.provider_name, safe_label(&profile.source_id)),
        settings_config: json!({
            "format": "toml",
            "content": codex_bridge_toml(&rule, port),
            "auth": { "OPENAI_API_KEY": local_bearer },
        }),
        meta: generated_provider_meta(profile, &rule),
        is_current: false,
    })
}

fn generated_provider_meta(profile: &AdapterProfile, rule: &CodexBridgeRule) -> Value {
    json!({
        "preset": "openai-compatible",
        "generatedBy": GENERATED_BY,
        "adapterRuleId": rule.rule_id,
        "adapterRuleVersion": 1,
        "adapterSecretMode": "local_token",
        "adapterProfileId": profile.id,
        "adapterSourceRef": {"kind": profile.source_kind.as_str(), "id": profile.source_id},
        "adapterBridge": {
            "kind": rule.bridge_kind,
            "loopbackOnly": true,
        },
    })
}

fn codex_bridge_toml(rule: &CodexBridgeRule, port: u16) -> String {
    format!(
        "model_provider = \"{slug}\"\nmodel = \"{model}\"\nmodel_reasoning_effort = \"high\"\ndisable_response_storage = true\npreferred_auth_method = \"apikey\"\n\n[model_providers.{slug}]\nname = \"{name}\"\nbase_url = \"http://127.0.0.1:{port}/v1\"\nwire_api = \"responses\"\n",
        slug = rule.provider_slug,
        model = rule.default_model,
        name = rule.toml_name,
    )
}

fn validate_generated_provider(
    provider: &Provider,
    profile: &AdapterProfile,
    expected_port: Option<u16>,
) -> Result<()> {
    if !provider_owned_by(provider, profile) {
        return Err(AppError::message(
            "adapter.provider_conflict",
            "generated provider does not belong to adapter bridge profile",
        ));
    }
    let rule = rule_for_id(&profile.rule_id).ok_or_else(invalid_projection)?;
    let _ = local_bearer_from_provider(provider)?;
    let content = provider
        .settings_config
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(invalid_projection)?;
    let document = content
        .parse::<DocumentMut>()
        .map_err(|_| invalid_projection())?;
    let codex_provider = document
        .get("model_providers")
        .and_then(|item| item.as_table())
        .and_then(|providers| providers.get(rule.provider_slug))
        .and_then(|item| item.as_table())
        .ok_or_else(invalid_projection)?;
    if document
        .get("model_provider")
        .and_then(|item| item.as_str())
        != Some(rule.provider_slug)
        || codex_provider
            .get("wire_api")
            .and_then(|item| item.as_str())
            != Some("responses")
        || codex_provider
            .get("base_url")
            .and_then(|item| item.as_str())
            .is_none()
    {
        return Err(invalid_projection());
    }
    if let Some(port) = expected_port {
        if content != codex_bridge_toml(&rule, port) {
            return Err(AppError::message(
                "adapter.provider_conflict",
                "generated bridge provider does not match the bound port",
            ));
        }
    }
    Ok(())
}

fn local_bearer_from_provider(provider: &Provider) -> Result<String> {
    if provider
        .settings_config
        .get("format")
        .and_then(Value::as_str)
        != Some("toml")
    {
        return Err(invalid_projection());
    }
    let local_bearer = provider
        .settings_config
        .get("auth")
        .and_then(Value::as_object)
        .and_then(|auth| auth.get("OPENAI_API_KEY"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "***")
        .ok_or_else(invalid_projection)?;
    Ok(local_bearer.into())
}

fn provider_owned_by(provider: &Provider, profile: &AdapterProfile) -> bool {
    let Some(rule) = rule_for_id(&profile.rule_id) else {
        return false;
    };
    provider.id == stable_id(rule.provider_prefix, &profile.source_id)
        && provider.agent_id == AgentId::Codex
        && provider.meta.get("preset").and_then(Value::as_str) == Some("openai-compatible")
        && provider.meta.get("generatedBy").and_then(Value::as_str) == Some(GENERATED_BY)
        && provider.meta.get("adapterRuleId").and_then(Value::as_str) == Some(rule.rule_id)
        && provider
            .meta
            .get("adapterRuleVersion")
            .and_then(Value::as_u64)
            == Some(1)
        && provider
            .meta
            .get("adapterSecretMode")
            .and_then(Value::as_str)
            == Some("local_token")
        && provider
            .meta
            .get("adapterProfileId")
            .and_then(Value::as_str)
            == Some(profile.id.as_str())
        && provider
            .meta
            .get("adapterSourceRef")
            .and_then(|value| value.get("kind"))
            .and_then(Value::as_str)
            == Some(profile.source_kind.as_str())
        && provider
            .meta
            .get("adapterSourceRef")
            .and_then(|value| value.get("id"))
            .and_then(Value::as_str)
            == Some(profile.source_id.as_str())
        && provider
            .meta
            .get("adapterBridge")
            .and_then(|value| value.get("kind"))
            .and_then(Value::as_str)
            == Some(rule.bridge_kind)
        && provider
            .meta
            .get("adapterBridge")
            .and_then(|value| value.get("loopbackOnly"))
            .and_then(Value::as_bool)
            == Some(true)
}

/// 幂等判定：已有桥投影是否已是当前规则的完整契约。
/// 不比较 `name`：展示名随票 display 变化，不是契约。
fn same_profile_contract(existing: &AdapterProfile, proposed: &AdapterProfile) -> bool {
    existing.id == proposed.id
        && existing.source_kind == proposed.source_kind
        && existing.source_id == proposed.source_id
        && existing.target_agent_id == proposed.target_agent_id
        && existing.route == proposed.route
        && existing.mode == proposed.mode
        && existing.rule_id == proposed.rule_id
        && existing.rule_version == proposed.rule_version
        && existing.generated_provider_id == proposed.generated_provider_id
}

fn validate_bound_port(port: u16) -> Result<()> {
    if port == 0 {
        return Err(AppError::InvalidArg(
            "adapter bridge bound port must be between 1 and 65535".into(),
        ));
    }
    Ok(())
}

fn generate_local_bearer() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|error| {
        AppError::message("adapter.local_bearer", format!("random failed: {error}"))
    })?;
    Ok(format!("ahb_{}", URL_SAFE_NO_PAD.encode(bytes)))
}

fn invalid_projection() -> AppError {
    AppError::message(
        "adapter.provider_conflict",
        "generated bridge provider has an invalid projection",
    )
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn stable_id(prefix: &str, source_id: &str) -> String {
    format!(
        "{prefix}-{}-{:016x}",
        safe_label(source_id),
        fnv1a(source_id.as_bytes())
    )
}

fn safe_label(value: &str) -> String {
    let label: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let label = label.trim_matches('-');
    if label.is_empty() {
        "source".into()
    } else {
        label.chars().take(40).collect()
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests;
