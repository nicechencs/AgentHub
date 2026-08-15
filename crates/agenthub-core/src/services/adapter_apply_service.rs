//! Write-side adapter apply for Kimi membership → Claude native, Pi config_sync,
//! GLM/DeepSeek ticket → Claude, explicit API → Pi, and DeepSeek API → DSH config_sync.
//!
//! The generated provider deliberately stores only a reference marker.  The
//! secret is materialized in memory by `AdapterSecretResolver` at ProviderService's
//! live-write boundary.

use std::path::PathBuf;

use chrono::Utc;
use serde_json::json;

use crate::adapters::AdapterRegistry;
use crate::error::{AppError, Result};
use crate::models::{
    map_adapter_model, AdapterApplyRequest, AdapterApplyResult, AdapterProfile,
    AdapterProfileFilter, AdapterProfileMode, AdapterProfileStatus, AdapterRoute,
    AdapterRouteAnalysis, AdapterRouteRequest, AdapterSourceKind, AdapterSourceProduct,
    AdapterSupport, AgentId, Provider, ProviderInput,
};
use crate::services::adapter_route_constants::{
    claude_native_base_url, ANTHROPIC_AUTH_TOKEN_ENV, ANTHROPIC_BASE_URL_ENV,
    ANTHROPIC_PI_PROVIDER_SLOT, CONNECTION_SECRET_MARKER, DEEPSEEK_API_BASE_URL,
    DEEPSEEK_CLAUDE_RULE_ID, DEEPSEEK_CODEX_BASE_URL, DEEPSEEK_CODEX_DEFAULT_MODEL,
    DEEPSEEK_CODEX_PROVIDER_PREFIX, DEEPSEEK_CODEX_PROVIDER_SLUG, DEEPSEEK_CODEX_RULE_ID,
    DEEPSEEK_PI_PROVIDER_SLOT, DEEPSEEK_PI_RULE_ID, DSH_API_KEY_ENV, DSH_DEEPSEEK_PROVIDER_SLOT,
    DSH_DEFAULT_MODEL, GLM_CLAUDE_RULE_ID, GLM_CODEX_BASE_URL, GLM_CODEX_DEFAULT_MODEL,
    GLM_CODEX_PROVIDER_PREFIX, GLM_CODEX_PROVIDER_SLUG, GLM_CODEX_RULE_ID, GLM_PI_BASE_URL,
    GLM_PI_PROVIDER_SLOT, GLM_PI_RULE_ID, KIMI_CLAUDE_RULE_ID, KIMI_PI_BASE_URL,
    KIMI_PI_PROVIDER_SLOT, OPENAI_PI_PROVIDER_SLOT, XAI_PI_PROVIDER_SLOT,
};
use crate::services::{
    AdapterRouteService, AdapterSecretResolver, ProviderLiveConfigSnapshot, ProviderLiveSagaGuard,
    ProviderService,
};
use crate::storage::{AdapterProfileRepo, Database};

const RULE_ID: &str = KIMI_CLAUDE_RULE_ID;
const KIMI_PI_RULE_ID: &str = "kimi-membership-to-pi-v1";
const ANTHROPIC_PI_RULE_ID: &str = "anthropic-api-to-pi-v1";
const OPENAI_PI_RULE_ID: &str = "openai-api-to-pi-v1";
const XAI_PI_RULE_ID: &str = "xai-api-to-pi-v1";
const CLAUDE_SUBSCRIPTION_PI_RULE_ID: &str = "claude-subscription-to-pi-v1";
const CODEX_SUBSCRIPTION_PI_RULE_ID: &str = "codex-subscription-to-pi-v1";
const GROK_SUBSCRIPTION_PI_RULE_ID: &str = "grok-subscription-to-pi-v1";
const DEEPSEEK_DSH_RULE_ID: &str = "deepseek-api-to-dsh-v1";
const RULE_VERSION: &str = "1";
const CLAUDE_PROVIDER_PREFIX: &str = "claude-kimi-adapter";
const CLAUDE_GLM_PROVIDER_PREFIX: &str = "claude-glm-adapter";
const CLAUDE_DEEPSEEK_PROVIDER_PREFIX: &str = "claude-deepseek-adapter";
const CODEX_GLM_PROFILE_PREFIX: &str = "adapter-glm-codex";
const CODEX_DEEPSEEK_PROFILE_PREFIX: &str = "adapter-deepseek-codex";
const PI_KIMI_PROVIDER_PREFIX: &str = "pi-kimi-adapter";
const PI_ANTHROPIC_PROVIDER_PREFIX: &str = "pi-anthropic-adapter";
const PI_OPENAI_PROVIDER_PREFIX: &str = "pi-openai-adapter";
const PI_XAI_PROVIDER_PREFIX: &str = "pi-xai-adapter";
const PI_GLM_PROVIDER_PREFIX: &str = "pi-glm-adapter";
const PI_DEEPSEEK_PROVIDER_PREFIX: &str = "pi-deepseek-adapter";
const PI_CLAUDE_OAUTH_PROVIDER_PREFIX: &str = "pi-claude-oauth-adapter";
const PI_CODEX_OAUTH_PROVIDER_PREFIX: &str = "pi-codex-oauth-adapter";
const PI_GROK_OAUTH_PROVIDER_PREFIX: &str = "pi-grok-oauth-adapter";
const DSH_DEEPSEEK_PROVIDER_PREFIX: &str = "dsh-deepseek-adapter";
const CLAUDE_PROFILE_PREFIX: &str = "adapter-kimi-claude";
const CLAUDE_GLM_PROFILE_PREFIX: &str = "adapter-glm-claude";
const CLAUDE_DEEPSEEK_PROFILE_PREFIX: &str = "adapter-deepseek-claude";
const PI_KIMI_PROFILE_PREFIX: &str = "adapter-kimi-pi";
const PI_ANTHROPIC_PROFILE_PREFIX: &str = "adapter-anthropic-pi";
const PI_OPENAI_PROFILE_PREFIX: &str = "adapter-openai-pi";
const PI_XAI_PROFILE_PREFIX: &str = "adapter-xai-pi";
const PI_GLM_PROFILE_PREFIX: &str = "adapter-glm-pi";
const PI_DEEPSEEK_PROFILE_PREFIX: &str = "adapter-deepseek-pi";
const DSH_DEEPSEEK_PROFILE_PREFIX: &str = "adapter-deepseek-dsh";
const PREVIOUS_CURRENT_ID: &str = "previousCurrentId";
const PREVIOUS_BACKUP_ID: &str = "previousBackupId";

/// Applies supported write-side routes and owns their generated profiles.
pub struct AdapterApplyService {
    routes: AdapterRouteService,
    profiles: AdapterProfileRepo,
    providers: ProviderService,
    secrets: AdapterSecretResolver,
}

/// Pre-switch snapshot used to reverse a successful live switch when profile
/// finalization (or the switch itself) fails. Deliberately private and
/// non-serializable: the live config may contain materialized credentials.
struct ApplySnapshot {
    /// Generated provider row before create/update in this apply, if any.
    generated_before: Option<Provider>,
    /// Target agent current provider before switch (may equal `generated_before`).
    previous_current: Option<Provider>,
    live_config: ProviderLiveConfigSnapshot,
    created: bool,
}

struct GeneratedApplySpec {
    target_agent: AgentId,
    provider_id: String,
    proposed: AdapterProfile,
    provider: ProviderInput,
}

impl AdapterApplyService {
    pub fn new(db: Database, registry: AdapterRegistry, backups_root: PathBuf) -> Self {
        Self {
            routes: AdapterRouteService::new(db.clone()),
            profiles: AdapterProfileRepo::new(db.clone()),
            providers: ProviderService::with_live(db.clone(), registry, backups_root),
            secrets: AdapterSecretResolver::new(db.clone()),
        }
    }

    pub fn apply(&self, request: &AdapterApplyRequest) -> Result<AdapterApplyResult> {
        let analysis = self.ensure_supported(request)?;
        let source_id = request.source_id.trim();
        match (request.source_kind, request.target_agent_id, analysis.route) {
            (source_kind, AgentId::Claude, AdapterRoute::NativeEndpoint) => {
                match analysis.rule_id.as_deref() {
                    Some(RULE_ID) => {
                        // Validate before creating a profile or provider: a dangling/masked
                        // source must be a completely side-effect-free failure.
                        self.secrets
                            .validate_kimi_membership_source(source_kind, source_id)?;
                        self.apply_generated(claude_native_spec(
                            source_kind,
                            source_id,
                            RULE_ID,
                        )?)
                    }
                    Some(rule) if is_claude_native_explicit_rule(rule) => {
                        self.secrets.validate_explicit_api_source(rule, source_kind, source_id)?;
                        self.apply_generated(claude_native_spec(source_kind, source_id, rule)?)
                    }
                    _ => Err(AppError::Unsupported(
                        "adapter apply currently supports Kimi membership Provider/Account or GLM/DeepSeek ticket -> Claude".into(),
                    )),
                }
            }
            (source_kind, AgentId::Codex, AdapterRoute::NativeEndpoint)
                if analysis
                    .rule_id
                    .as_deref()
                    .is_some_and(is_codex_native_rule) =>
            {
                let rule = analysis.rule_id.as_deref().expect("checked above");
                self.secrets
                    .validate_explicit_api_source(rule, source_kind, source_id)?;
                self.apply_generated(codex_native_spec(source_kind, source_id, rule)?)
            }
            (source_kind, AgentId::Pi, AdapterRoute::ConfigSync)
                if source_kind == AdapterSourceKind::Provider
                    || analysis.rule_id.as_deref() == Some(KIMI_PI_RULE_ID) =>
            {
                match analysis.rule_id.as_deref() {
                    Some(KIMI_PI_RULE_ID) => {
                        self.secrets
                            .validate_kimi_membership_source(source_kind, source_id)?;
                        self.apply_generated(pi_kimi_spec(source_kind, source_id))
                    }
                    Some(rule) if is_explicit_api_to_pi_rule(rule) => {
                        self.secrets.validate_explicit_api_source(
                            rule,
                            source_kind,
                            source_id,
                        )?;
                        self.apply_generated(pi_explicit_api_spec(
                            source_kind,
                            source_id,
                            rule,
                        )?)
                    }
                    _ => Err(AppError::Unsupported(
                        "adapter apply currently supports Kimi membership Provider/Account or explicit API provider -> Pi".into(),
                    )),
                }
            }
            (AdapterSourceKind::Account, AgentId::Pi, AdapterRoute::ConfigSync)
                if analysis
                    .rule_id
                    .as_deref()
                    .is_some_and(is_explicit_api_to_pi_rule) =>
            {
                let rule = analysis.rule_id.as_deref().expect("checked above");
                self.secrets.validate_explicit_api_source(
                    rule,
                    AdapterSourceKind::Account,
                    source_id,
                )?;
                self.apply_generated(pi_explicit_api_spec(
                    AdapterSourceKind::Account,
                    source_id,
                    rule,
                )?)
            }
            (AdapterSourceKind::Account, AgentId::Pi, AdapterRoute::ConfigSync)
                if analysis
                    .rule_id
                    .as_deref()
                    .is_some_and(is_subscription_pi_rule) =>
            {
                let rule = analysis.rule_id.as_deref().expect("checked above");
                self.secrets.validate_subscription_oauth_source(
                    rule,
                    AdapterSourceKind::Account,
                    source_id,
                )?;
                self.apply_generated(pi_subscription_spec(source_id, rule)?)
            }
            (AdapterSourceKind::Provider, AgentId::Dsh, AdapterRoute::ConfigSync) => {
                match analysis.rule_id.as_deref() {
                    Some(DEEPSEEK_DSH_RULE_ID) => {
                        self.secrets.validate_deepseek_api_source(source_id)?;
                        self.apply_generated(dsh_deepseek_spec(source_id))
                    }
                    _ => Err(AppError::Unsupported(
                        "adapter apply currently supports only DeepSeek API provider -> DSH".into(),
                    )),
                }
            }
            _ => Err(AppError::Unsupported(
                "adapter apply currently supports Kimi membership Provider/Account -> Claude/Pi, GLM/DeepSeek ticket -> Claude, API or subscription Account -> Pi, and DeepSeek API provider -> DSH".into(),
            )),
        }
    }

    fn apply_generated(&self, mut spec: GeneratedApplySpec) -> Result<AdapterApplyResult> {
        // Acquire before reading or creating any generated-provider/profile
        // state. The guard covers every compensation input and mutation below.
        let saga_guard = self.providers.begin_live_saga(spec.target_agent)?;
        let mut profile = self.profiles.create_or_get(&spec.proposed)?;
        if !same_profile_contract(&profile, &spec.proposed) {
            return Err(AppError::message(
                "adapter.profile_conflict",
                "adapter profile conflicts with requested rule",
            ));
        }

        let existing = match self
            .providers
            .get(&spec.provider_id, Some(spec.target_agent))
        {
            Ok(existing) => Some(existing),
            Err(AppError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        if let Some(existing) = existing.as_ref() {
            if !provider_owned_by(existing, &profile) {
                return Err(AppError::message(
                    "adapter.provider_conflict",
                    "generated provider id is owned by another provider",
                ));
            }
            if profile.status == AdapterProfileStatus::Active
                && provider_matches_projection(existing, &spec.provider)
                && existing.is_current
            {
                // Only a complete current projection is idempotent.  A
                // demoted or drifted row must be repaired and switched again.
                return Ok(AdapterApplyResult {
                    profile,
                    provider: existing.redacted(),
                });
            }
        }

        if profile.status != AdapterProfileStatus::Applying {
            profile.status = AdapterProfileStatus::Applying;
            profile.last_error_code = None;
            profile.updated_at = now();
            profile = self.profiles.update(&profile)?;
        }

        // Capture compensation inputs before demote/create: a repair demotes a
        // current generated row, so get_current after that would miss the
        // pre-saga binding needed for full inverse.
        let previous_current = match self.providers.repo().get_current(spec.target_agent) {
            Ok(current) => current,
            Err(error) => return Err(self.fail_profile(profile, &error)),
        };
        let live_config = match self
            .providers
            .capture_live_config_snapshot_with_guard(&saga_guard, spec.target_agent)
        {
            Ok(snapshot) => snapshot,
            Err(error) => return Err(self.fail_profile(profile, &error)),
        };
        let generated_before = existing.clone();
        let created = existing.is_none();
        let snapshot = ApplySnapshot {
            generated_before,
            previous_current: previous_current.clone(),
            live_config,
            created,
        };
        stamp_previous_restore_meta(
            &mut spec.provider.meta,
            previous_current.as_ref(),
            &spec.provider_id,
            existing.as_ref(),
        );

        // Create/repair the pool row before switch; the switched provider is
        // returned from switch_with_guard below.
        if let Err(error) = match existing {
            Some(existing) if provider_matches_projection(&existing, &spec.provider) => Ok(()),
            Some(_) => {
                // Repair the persisted generated projection before switching it
                // live.  Explicitly demote first so the switch owns the only
                // live write and re-validates/materializes the source secret.
                let repaired = ProviderInput {
                    is_current: false,
                    ..spec.provider.clone()
                };
                self.providers
                    .update_with_guard(&saga_guard, &repaired)
                    .map(|_| ())
            }
            None => self
                .providers
                .create_with_guard(&saga_guard, &spec.provider)
                .map(|_| ()),
        } {
            return Err(self.fail_profile(profile, &error));
        }

        let switched = match self.providers.switch_with_guard(
            &saga_guard,
            &spec.provider_id,
            spec.target_agent,
        ) {
            Ok(result) => {
                if let Some(backup_id) = result.backup.as_ref().map(|backup| backup.id.as_str()) {
                    if let Err(error) =
                        self.persist_previous_backup_id(&saga_guard, &result.provider, backup_id)
                    {
                        if let Err(restore_error) = self.compensate_apply(
                            &saga_guard,
                            &spec.provider_id,
                            spec.target_agent,
                            &snapshot,
                        ) {
                            return Err(self.fail_profile(profile, &restore_error));
                        }
                        return Err(self.fail_profile(profile, &error));
                    }
                }
                result.provider
            }
            Err(error) => {
                if let Err(restore_error) = self.compensate_apply(
                    &saga_guard,
                    &spec.provider_id,
                    spec.target_agent,
                    &snapshot,
                ) {
                    return Err(self.fail_profile(profile, &restore_error));
                }
                return Err(self.fail_profile(profile, &error));
            }
        };

        profile.status = AdapterProfileStatus::Active;
        profile.generated_provider_id = Some(spec.provider_id.clone());
        profile.last_error_code = None;
        profile.updated_at = now();
        let profile = match self.profiles.update(&profile) {
            Ok(profile) => profile,
            Err(_) => {
                let mut attention = profile;
                attention.status = AdapterProfileStatus::NeedsAttention;
                let restore_error = self
                    .compensate_apply(&saga_guard, &spec.provider_id, spec.target_agent, &snapshot)
                    .err();
                attention.last_error_code = Some(
                    if restore_error.is_some() {
                        "adapter.rollback_incomplete"
                    } else {
                        "adapter.profile_finalize"
                    }
                    .into(),
                );
                attention.updated_at = now();
                let _ = self.profiles.update(&attention);
                if restore_error.is_some() {
                    return Err(AppError::message(
                        "adapter.rollback_incomplete",
                        "adapter profile finalization failed and repair rollback was incomplete",
                    ));
                }
                return Err(AppError::message(
                    "adapter.profile_finalize",
                    "adapter profile finalization failed",
                ));
            }
        };
        Ok(AdapterApplyResult {
            profile,
            provider: switched.redacted(),
        })
    }

    pub fn list(
        &self,
        source_kind: Option<AdapterSourceKind>,
        source_id: Option<&str>,
        target_agent_id: Option<AgentId>,
    ) -> Result<Vec<AdapterProfile>> {
        self.list_filtered(&AdapterProfileFilter {
            source_kind,
            source_id: source_id.map(str::to_owned),
            target_agent_id,
            ..AdapterProfileFilter::default()
        })
    }

    /// Lists profiles using the full typed filter, including product `mode`.
    pub fn list_filtered(&self, filter: &AdapterProfileFilter) -> Result<Vec<AdapterProfile>> {
        self.profiles.list_filtered(filter)
    }

    /// Removes the profile and its generated provider when it still exists.
    pub fn remove(&self, profile_id: &str) -> Result<()> {
        // Lock the profile's target agent (Claude native or Pi config_sync),
        // not a hardcoded Claude saga. Read the profile first so the lock
        // matches the generated provider that will be deleted.
        let profile = self.profiles.get(profile_id)?.ok_or_else(|| {
            AppError::NotFound(format!("adapter profile not found: {profile_id}"))
        })?;
        if !owns_apply_profile(&profile) {
            return Err(AppError::Unsupported(
                "adapter apply remove supports Claude native, Pi config_sync, and DSH config_sync profiles".into(),
            ));
        }
        let saga_guard = self.providers.begin_live_saga(profile.target_agent_id)?;
        let profile = self.profiles.get(profile_id)?.ok_or_else(|| {
            AppError::NotFound(format!("adapter profile not found: {profile_id}"))
        })?;
        if let Some(provider_id) = profile.generated_provider_id.as_deref() {
            let provider = match self
                .providers
                .get(provider_id, Some(profile.target_agent_id))
            {
                Ok(provider) => Some(provider),
                Err(AppError::NotFound(_)) => None,
                Err(error) => return Err(error),
            };
            if let Some(provider) = provider {
                if !provider_owned_by(&provider, &profile) {
                    return Err(AppError::message(
                        "adapter.provider_conflict",
                        "generated provider does not belong to adapter profile",
                    ));
                }
                if provider.is_current {
                    self.restore_previous_binding(&saga_guard, &provider, profile.target_agent_id)?;
                }
                self.providers.delete_with_guard(
                    &saga_guard,
                    provider_id,
                    profile.target_agent_id,
                )?;
            }
        }
        self.profiles.delete(profile_id)
    }

    fn ensure_supported(&self, request: &AdapterApplyRequest) -> Result<AdapterRouteAnalysis> {
        let analysis = self.routes.analyze(&AdapterRouteRequest {
            source_kind: request.source_kind,
            source_id: request.source_id.clone(),
            target_agent_id: request.target_agent_id,
        })?;
        let supported = match (request.source_kind, request.target_agent_id, analysis.route) {
            (source_kind, AgentId::Claude, AdapterRoute::NativeEndpoint) => analysis
                .rule_id
                .as_deref()
                .is_some_and(|rule| is_claude_native_apply_rule(rule, source_kind)),
            (source_kind, AgentId::Codex, AdapterRoute::NativeEndpoint) => analysis
                .rule_id
                .as_deref()
                .is_some_and(|rule| is_codex_native_rule(rule) && is_api_source_kind(source_kind)),
            (AdapterSourceKind::Provider, AgentId::Pi, AdapterRoute::ConfigSync) => {
                (analysis.support == AdapterSupport::Stable
                    && analysis.rule_id.as_deref() == Some(KIMI_PI_RULE_ID))
                    || (analysis.support == AdapterSupport::Stable
                        && analysis
                            .rule_id
                            .as_deref()
                            .is_some_and(is_explicit_api_to_pi_rule))
                    || (analysis.support == AdapterSupport::Experimental
                        && analysis
                            .rule_id
                            .as_deref()
                            .is_some_and(is_explicit_api_to_pi_rule))
            }
            (AdapterSourceKind::Account, AgentId::Pi, AdapterRoute::ConfigSync) => {
                (analysis.support == AdapterSupport::Stable
                    && analysis.rule_id.as_deref() == Some(KIMI_PI_RULE_ID))
                    || (analysis.support == AdapterSupport::Stable
                        && analysis
                            .rule_id
                            .as_deref()
                            .is_some_and(is_explicit_api_to_pi_rule))
                    || (analysis.support == AdapterSupport::Experimental
                        && analysis
                            .rule_id
                            .as_deref()
                            .is_some_and(is_explicit_api_to_pi_rule))
                    || (analysis.support == AdapterSupport::Experimental
                        && analysis
                            .rule_id
                            .as_deref()
                            .is_some_and(is_subscription_pi_rule))
            }
            (AdapterSourceKind::Provider, AgentId::Dsh, AdapterRoute::ConfigSync) => {
                analysis.support == AdapterSupport::Stable
                    && analysis.rule_id.as_deref() == Some(DEEPSEEK_DSH_RULE_ID)
            }
            _ => false,
        };
        if supported {
            Ok(analysis)
        } else {
            Err(AppError::Unsupported(
                "adapter apply currently supports Kimi membership Provider/Account -> Claude/Pi, GLM/DeepSeek ticket -> Claude, API or subscription Account -> Pi, and DeepSeek API provider -> DSH".into(),
            ))
        }
    }

    fn persist_previous_backup_id(
        &self,
        saga_guard: &ProviderLiveSagaGuard<'_>,
        provider: &Provider,
        backup_id: &str,
    ) -> Result<Provider> {
        let mut input = provider_input(provider);
        let Some(meta) = input.meta.as_object_mut() else {
            return Ok(provider.clone());
        };
        meta.insert(PREVIOUS_BACKUP_ID.into(), json!(backup_id));
        self.providers.update_with_guard(saga_guard, &input)
    }

    fn restore_previous_binding(
        &self,
        saga_guard: &ProviderLiveSagaGuard<'_>,
        generated: &Provider,
        target_agent: AgentId,
    ) -> Result<()> {
        let is_subscription_oauth = is_subscription_pi_rule(
            generated
                .meta
                .get("adapterRuleId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default(),
        );
        let previous_id = generated
            .meta
            .get(PREVIOUS_CURRENT_ID)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty() && *id != generated.id);
        let backup_id = generated
            .meta
            .get(PREVIOUS_BACKUP_ID)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty());
        if is_subscription_oauth {
            if let Some(backup_id) = backup_id {
                self.providers
                    .restore_named_backup_with_guard(saga_guard, backup_id)?;
            }
        }
        if let Some(previous_id) = previous_id {
            match self.providers.get(previous_id, Some(target_agent)) {
                Ok(_) => {
                    self.providers
                        .switch_with_guard(saga_guard, previous_id, target_agent)?;
                    return Ok(());
                }
                Err(AppError::NotFound(_)) => {}
                Err(error) => return Err(error),
            }
        }
        if let Some(backup_id) = backup_id.filter(|_| !is_subscription_oauth) {
            self.providers
                .restore_named_backup_with_guard(saga_guard, backup_id)?;
        }
        Ok(())
    }

    fn fail_profile(&self, mut profile: AdapterProfile, error: &AppError) -> AppError {
        profile.status = AdapterProfileStatus::NeedsAttention;
        profile.last_error_code = Some(error.code().into());
        profile.updated_at = now();
        let _ = self.profiles.update(&profile);
        AppError::message(
            "adapter_apply.failed",
            format!("adapter apply failed: {}", error.code()),
        )
    }

    /// Inverse of a successful live switch: restore the generated pool row
    /// (or delete a create), re-select the pre-switch current provider when one
    /// existed, then force the pre-switch live config. Every step is attempted
    /// even if an earlier step fails.
    fn compensate_apply(
        &self,
        saga_guard: &ProviderLiveSagaGuard<'_>,
        provider_id: &str,
        target_agent: AgentId,
        snapshot: &ApplySnapshot,
    ) -> Result<()> {
        let mut failed: Option<AppError> = None;
        let previous_id = snapshot
            .previous_current
            .as_ref()
            .map(|provider| provider.id.as_str());
        let generated_was_previous = snapshot
            .generated_before
            .as_ref()
            .is_some_and(|provider| previous_id == Some(provider.id.as_str()));

        if let Some(old) = &snapshot.generated_before {
            let mut input = provider_input(old);
            // When the generated row itself was current, re-activate via the
            // pool update path (no second live switch). A different previous
            // current is re-selected below with switch_with_guard.
            input.is_current = generated_was_previous;
            if let Err(error) = self.providers.update_with_guard(saga_guard, &input) {
                failed = Some(error);
            }
        } else if snapshot.created {
            if let Err(error) =
                self.providers
                    .delete_with_guard(saga_guard, provider_id, target_agent)
            {
                failed = Some(error);
            }
        }

        if let Some(previous) = &snapshot.previous_current {
            if !generated_was_previous {
                if let Err(error) =
                    self.providers
                        .switch_with_guard(saga_guard, &previous.id, target_agent)
                {
                    failed = Some(error);
                }
            }
        }

        // Always restore live last so a switch-back that backfills a drifted
        // value cannot leave the on-disk Claude config changed.
        if let Err(error) = self
            .providers
            .restore_live_config_snapshot_with_guard(saga_guard, &snapshot.live_config)
        {
            failed = Some(error);
        }

        match failed {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

/// 幂等判定：已有投影是否已是当前规则的完整契约。
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

fn owns_apply_profile(profile: &AdapterProfile) -> bool {
    matches!(
        (profile.target_agent_id, profile.route),
        (AgentId::Claude, AdapterRoute::NativeEndpoint)
            | (AgentId::Codex, AdapterRoute::NativeEndpoint)
            | (AgentId::Pi, AdapterRoute::ConfigSync)
            | (AgentId::Dsh, AdapterRoute::ConfigSync)
    )
}

fn generated_provider_prefix(profile: &AdapterProfile) -> Option<&'static str> {
    match (
        profile.target_agent_id,
        profile.route,
        profile.rule_id.as_str(),
    ) {
        (AgentId::Claude, AdapterRoute::NativeEndpoint, RULE_ID) => Some(CLAUDE_PROVIDER_PREFIX),
        (AgentId::Claude, AdapterRoute::NativeEndpoint, GLM_CLAUDE_RULE_ID) => {
            Some(CLAUDE_GLM_PROVIDER_PREFIX)
        }
        (AgentId::Claude, AdapterRoute::NativeEndpoint, DEEPSEEK_CLAUDE_RULE_ID) => {
            Some(CLAUDE_DEEPSEEK_PROVIDER_PREFIX)
        }
        (AgentId::Codex, AdapterRoute::NativeEndpoint, GLM_CODEX_RULE_ID) => {
            Some(GLM_CODEX_PROVIDER_PREFIX)
        }
        (AgentId::Codex, AdapterRoute::NativeEndpoint, DEEPSEEK_CODEX_RULE_ID) => {
            Some(DEEPSEEK_CODEX_PROVIDER_PREFIX)
        }
        (AgentId::Pi, AdapterRoute::ConfigSync, KIMI_PI_RULE_ID) => Some(PI_KIMI_PROVIDER_PREFIX),
        (AgentId::Pi, AdapterRoute::ConfigSync, ANTHROPIC_PI_RULE_ID) => {
            Some(PI_ANTHROPIC_PROVIDER_PREFIX)
        }
        (AgentId::Pi, AdapterRoute::ConfigSync, OPENAI_PI_RULE_ID) => {
            Some(PI_OPENAI_PROVIDER_PREFIX)
        }
        (AgentId::Pi, AdapterRoute::ConfigSync, XAI_PI_RULE_ID) => Some(PI_XAI_PROVIDER_PREFIX),
        (AgentId::Pi, AdapterRoute::ConfigSync, GLM_PI_RULE_ID) => Some(PI_GLM_PROVIDER_PREFIX),
        (AgentId::Pi, AdapterRoute::ConfigSync, DEEPSEEK_PI_RULE_ID) => {
            Some(PI_DEEPSEEK_PROVIDER_PREFIX)
        }
        (AgentId::Pi, AdapterRoute::ConfigSync, CLAUDE_SUBSCRIPTION_PI_RULE_ID) => {
            Some(PI_CLAUDE_OAUTH_PROVIDER_PREFIX)
        }
        (AgentId::Pi, AdapterRoute::ConfigSync, CODEX_SUBSCRIPTION_PI_RULE_ID) => {
            Some(PI_CODEX_OAUTH_PROVIDER_PREFIX)
        }
        (AgentId::Pi, AdapterRoute::ConfigSync, GROK_SUBSCRIPTION_PI_RULE_ID) => {
            Some(PI_GROK_OAUTH_PROVIDER_PREFIX)
        }
        (AgentId::Dsh, AdapterRoute::ConfigSync, DEEPSEEK_DSH_RULE_ID) => {
            Some(DSH_DEEPSEEK_PROVIDER_PREFIX)
        }
        _ => None,
    }
}

fn provider_owned_by(provider: &crate::models::Provider, profile: &AdapterProfile) -> bool {
    let Some(prefix) = generated_provider_prefix(profile) else {
        return false;
    };
    provider.id == stable_id(prefix, &profile.source_id)
        && provider.agent_id == profile.target_agent_id
        && provider
            .meta
            .get("generatedBy")
            .and_then(serde_json::Value::as_str)
            == Some("adapter")
        && provider
            .meta
            .get("adapterRuleId")
            .and_then(serde_json::Value::as_str)
            == Some(profile.rule_id.as_str())
        && provider
            .meta
            .get("adapterRuleVersion")
            .and_then(serde_json::Value::as_u64)
            == Some(1)
        && provider
            .meta
            .get("adapterSecretMode")
            .and_then(serde_json::Value::as_str)
            == Some("source_reference")
        && provider
            .meta
            .get("adapterProfileId")
            .and_then(serde_json::Value::as_str)
            == Some(profile.id.as_str())
        && provider
            .meta
            .get("adapterSourceRef")
            .and_then(|v| v.get("kind"))
            .and_then(serde_json::Value::as_str)
            == Some(profile.source_kind.as_str())
        && provider
            .meta
            .get("adapterSourceRef")
            .and_then(|v| v.get("id"))
            .and_then(serde_json::Value::as_str)
            == Some(profile.source_id.as_str())
}

fn is_claude_native_explicit_rule(rule_id: &str) -> bool {
    matches!(rule_id, GLM_CLAUDE_RULE_ID | DEEPSEEK_CLAUDE_RULE_ID)
}

fn is_codex_native_rule(rule_id: &str) -> bool {
    matches!(rule_id, GLM_CODEX_RULE_ID | DEEPSEEK_CODEX_RULE_ID)
}

fn is_api_source_kind(source_kind: AdapterSourceKind) -> bool {
    matches!(
        source_kind,
        AdapterSourceKind::Provider | AdapterSourceKind::Account
    )
}

fn is_claude_native_apply_rule(rule_id: &str, source_kind: AdapterSourceKind) -> bool {
    match (rule_id, source_kind) {
        (RULE_ID, AdapterSourceKind::Provider | AdapterSourceKind::Account) => true,
        (GLM_CLAUDE_RULE_ID | DEEPSEEK_CLAUDE_RULE_ID, _) => true,
        _ => false,
    }
}

fn claude_native_layout(
    rule_id: &str,
) -> Result<(&'static str, &'static str, &'static str, &'static str)> {
    let base_url = claude_native_base_url(rule_id).ok_or_else(|| {
        AppError::Unsupported(
            "adapter apply currently supports Kimi / GLM / DeepSeek -> Claude".into(),
        )
    })?;
    match rule_id {
        RULE_ID => Ok((
            CLAUDE_PROFILE_PREFIX,
            CLAUDE_PROVIDER_PREFIX,
            "Kimi Code",
            base_url,
        )),
        GLM_CLAUDE_RULE_ID => Ok((
            CLAUDE_GLM_PROFILE_PREFIX,
            CLAUDE_GLM_PROVIDER_PREFIX,
            "GLM",
            base_url,
        )),
        DEEPSEEK_CLAUDE_RULE_ID => Ok((
            CLAUDE_DEEPSEEK_PROFILE_PREFIX,
            CLAUDE_DEEPSEEK_PROVIDER_PREFIX,
            "DeepSeek",
            base_url,
        )),
        _ => Err(AppError::Unsupported(
            "adapter apply currently supports Kimi / GLM / DeepSeek -> Claude".into(),
        )),
    }
}

fn claude_native_spec(
    source_kind: AdapterSourceKind,
    source_id: &str,
    rule_id: &str,
) -> Result<GeneratedApplySpec> {
    let (profile_prefix, provider_prefix, display, base_url) = claude_native_layout(rule_id)?;
    let profile_id = stable_id(profile_prefix, source_id);
    let provider_id = stable_id(provider_prefix, source_id);
    let created_at = now();
    Ok(GeneratedApplySpec {
        target_agent: AgentId::Claude,
        provider_id: provider_id.clone(),
        proposed: AdapterProfile {
            id: profile_id.clone(),
            name: format!("{display} → Claude ({})", safe_label(source_id)),
            source_kind,
            source_id: source_id.into(),
            target_agent_id: AgentId::Claude,
            route: AdapterRoute::NativeEndpoint,
            mode: AdapterProfileMode::Api,
            status: AdapterProfileStatus::Applying,
            rule_id: rule_id.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        },
        provider: ProviderInput {
            id: provider_id,
            agent_id: AgentId::Claude,
            name: format!("{display} ({})", safe_label(source_id)),
            settings_config: json!({"env": {
                ANTHROPIC_BASE_URL_ENV: base_url,
                ANTHROPIC_AUTH_TOKEN_ENV: CONNECTION_SECRET_MARKER,
            }}),
            meta: generated_meta(
                rule_id,
                &profile_id,
                source_kind,
                source_id,
                Some("anthropic-compatible"),
            ),
            is_current: false,
        },
    })
}

fn codex_native_spec(
    source_kind: AdapterSourceKind,
    source_id: &str,
    rule_id: &str,
) -> Result<GeneratedApplySpec> {
    let (profile_prefix, provider_prefix, display, base_url, model, slug) = match rule_id {
        GLM_CODEX_RULE_ID => (
            CODEX_GLM_PROFILE_PREFIX,
            GLM_CODEX_PROVIDER_PREFIX,
            "GLM Coding Plan",
            GLM_CODEX_BASE_URL,
            GLM_CODEX_DEFAULT_MODEL,
            GLM_CODEX_PROVIDER_SLUG,
        ),
        DEEPSEEK_CODEX_RULE_ID => (
            CODEX_DEEPSEEK_PROFILE_PREFIX,
            DEEPSEEK_CODEX_PROVIDER_PREFIX,
            "DeepSeek",
            DEEPSEEK_CODEX_BASE_URL,
            DEEPSEEK_CODEX_DEFAULT_MODEL,
            DEEPSEEK_CODEX_PROVIDER_SLUG,
        ),
        _ => {
            return Err(AppError::Unsupported(
                "adapter apply currently supports GLM / DeepSeek API -> Codex".into(),
            ))
        }
    };
    let profile_id = stable_id(profile_prefix, source_id);
    let provider_id = stable_id(provider_prefix, source_id);
    let created_at = now();
    let content = format!(
        "model_provider = \"{slug}\"\n\
         model = \"{model}\"\n\
         model_reasoning_effort = \"high\"\n\
         preferred_auth_method = \"apikey\"\n\
         \n\
         [model_providers.{slug}]\n\
         name = \"{display}\"\n\
         base_url = \"{base_url}\"\n\
         wire_api = \"responses\"\n\
         experimental_bearer_token = \"{marker}\"\n",
        marker = CONNECTION_SECRET_MARKER,
    );
    Ok(GeneratedApplySpec {
        target_agent: AgentId::Codex,
        provider_id: provider_id.clone(),
        proposed: AdapterProfile {
            id: profile_id.clone(),
            name: format!("{display} → Codex ({})", safe_label(source_id)),
            source_kind,
            source_id: source_id.into(),
            target_agent_id: AgentId::Codex,
            route: AdapterRoute::NativeEndpoint,
            mode: AdapterProfileMode::Api,
            status: AdapterProfileStatus::Applying,
            rule_id: rule_id.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        },
        provider: ProviderInput {
            id: provider_id,
            agent_id: AgentId::Codex,
            name: format!("{display} ({})", safe_label(source_id)),
            settings_config: json!({
                "format": "toml",
                "content": content,
                "auth": { "OPENAI_API_KEY": CONNECTION_SECRET_MARKER },
            }),
            meta: generated_meta(rule_id, &profile_id, source_kind, source_id, None),
            is_current: false,
        },
    })
}

fn pi_kimi_spec(source_kind: AdapterSourceKind, source_id: &str) -> GeneratedApplySpec {
    let profile_id = stable_id(PI_KIMI_PROFILE_PREFIX, source_id);
    let provider_id = stable_id(PI_KIMI_PROVIDER_PREFIX, source_id);
    let created_at = now();
    let model = map_adapter_model(AdapterSourceProduct::KimiCodeMembership, AgentId::Pi, "")
        .unwrap_or("kimi-k2.5");
    GeneratedApplySpec {
        target_agent: AgentId::Pi,
        provider_id: provider_id.clone(),
        proposed: AdapterProfile {
            id: profile_id.clone(),
            name: format!("Kimi → Pi ({})", safe_label(source_id)),
            source_kind,
            source_id: source_id.into(),
            target_agent_id: AgentId::Pi,
            route: AdapterRoute::ConfigSync,
            mode: AdapterProfileMode::Api,
            status: AdapterProfileStatus::Applying,
            rule_id: KIMI_PI_RULE_ID.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        },
        provider: ProviderInput {
            id: provider_id,
            agent_id: AgentId::Pi,
            name: format!("Kimi Code ({})", safe_label(source_id)),
            settings_config: json!({
                "models": {
                    "providers": {
                        KIMI_PI_PROVIDER_SLOT: {
                            "baseUrl": KIMI_PI_BASE_URL,
                            "apiKey": CONNECTION_SECRET_MARKER,
                            "api": "openai-completions",
                            "models": [{ "id": model }],
                        }
                    }
                }
            }),
            meta: generated_meta(KIMI_PI_RULE_ID, &profile_id, source_kind, source_id, None),
            is_current: false,
        },
    }
}

fn is_explicit_api_to_pi_rule(rule_id: &str) -> bool {
    matches!(
        rule_id,
        ANTHROPIC_PI_RULE_ID
            | OPENAI_PI_RULE_ID
            | XAI_PI_RULE_ID
            | GLM_PI_RULE_ID
            | DEEPSEEK_PI_RULE_ID
    )
}

fn is_subscription_pi_rule(rule_id: &str) -> bool {
    matches!(
        rule_id,
        CLAUDE_SUBSCRIPTION_PI_RULE_ID
            | CODEX_SUBSCRIPTION_PI_RULE_ID
            | GROK_SUBSCRIPTION_PI_RULE_ID
    )
}

fn pi_subscription_layout(rule_id: &str) -> Result<(&'static str, &'static str, &'static str)> {
    match rule_id {
        CLAUDE_SUBSCRIPTION_PI_RULE_ID => Ok((
            PI_CLAUDE_OAUTH_PROVIDER_PREFIX,
            "Claude",
            ANTHROPIC_PI_PROVIDER_SLOT,
        )),
        CODEX_SUBSCRIPTION_PI_RULE_ID => Ok((
            PI_CODEX_OAUTH_PROVIDER_PREFIX,
            "Codex / ChatGPT",
            "openai-codex",
        )),
        GROK_SUBSCRIPTION_PI_RULE_ID => Ok((
            PI_GROK_OAUTH_PROVIDER_PREFIX,
            "Grok / xAI",
            XAI_PI_PROVIDER_SLOT,
        )),
        _ => Err(AppError::Unsupported(
            "adapter apply currently supports Claude / Codex / Grok subscription -> Pi".into(),
        )),
    }
}

fn pi_subscription_spec(source_id: &str, rule_id: &str) -> Result<GeneratedApplySpec> {
    let (provider_prefix, display, slot) = pi_subscription_layout(rule_id)?;
    let profile_id = stable_id(&format!("adapter-{provider_prefix}"), source_id);
    let provider_id = stable_id(provider_prefix, source_id);
    let created_at = now();
    Ok(GeneratedApplySpec {
        target_agent: AgentId::Pi,
        provider_id: provider_id.clone(),
        proposed: AdapterProfile {
            id: profile_id.clone(),
            name: format!("{display} 订阅 → Pi ({})", safe_label(source_id)),
            source_kind: AdapterSourceKind::Account,
            source_id: source_id.into(),
            target_agent_id: AgentId::Pi,
            route: AdapterRoute::ConfigSync,
            mode: AdapterProfileMode::Oauth,
            status: AdapterProfileStatus::Applying,
            rule_id: rule_id.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        },
        provider: ProviderInput {
            id: provider_id,
            agent_id: AgentId::Pi,
            name: format!("{display} 订阅 ({})", safe_label(source_id)),
            settings_config: json!({
                "auth": {
                    (slot): {
                        "type": "oauth",
                        "access": CONNECTION_SECRET_MARKER,
                        "refresh": CONNECTION_SECRET_MARKER,
                    }
                }
            }),
            meta: generated_meta(
                rule_id,
                &profile_id,
                AdapterSourceKind::Account,
                source_id,
                None,
            ),
            is_current: false,
        },
    })
}

fn pi_explicit_api_layout(
    rule_id: &str,
) -> Result<(&'static str, &'static str, &'static str, &'static str)> {
    match rule_id {
        ANTHROPIC_PI_RULE_ID => Ok((
            PI_ANTHROPIC_PROFILE_PREFIX,
            PI_ANTHROPIC_PROVIDER_PREFIX,
            "Anthropic",
            ANTHROPIC_PI_PROVIDER_SLOT,
        )),
        OPENAI_PI_RULE_ID => Ok((
            PI_OPENAI_PROFILE_PREFIX,
            PI_OPENAI_PROVIDER_PREFIX,
            "OpenAI",
            OPENAI_PI_PROVIDER_SLOT,
        )),
        XAI_PI_RULE_ID => Ok((
            PI_XAI_PROFILE_PREFIX,
            PI_XAI_PROVIDER_PREFIX,
            "xAI",
            XAI_PI_PROVIDER_SLOT,
        )),
        GLM_PI_RULE_ID => Ok((
            PI_GLM_PROFILE_PREFIX,
            PI_GLM_PROVIDER_PREFIX,
            "GLM Coding Plan",
            GLM_PI_PROVIDER_SLOT,
        )),
        DEEPSEEK_PI_RULE_ID => Ok((
            PI_DEEPSEEK_PROFILE_PREFIX,
            PI_DEEPSEEK_PROVIDER_PREFIX,
            "DeepSeek",
            DEEPSEEK_PI_PROVIDER_SLOT,
        )),
        _ => Err(AppError::Unsupported(
            "adapter apply currently supports Anthropic / OpenAI / xAI / GLM / DeepSeek API -> Pi"
                .into(),
        )),
    }
}

fn pi_explicit_api_spec(
    source_kind: AdapterSourceKind,
    source_id: &str,
    rule_id: &str,
) -> Result<GeneratedApplySpec> {
    let (profile_prefix, provider_prefix, display, slot) = pi_explicit_api_layout(rule_id)?;
    let (base_url, model) = match rule_id {
        GLM_PI_RULE_ID => (GLM_PI_BASE_URL, "glm-4.6"),
        DEEPSEEK_PI_RULE_ID => (DEEPSEEK_API_BASE_URL, "deepseek-chat"),
        _ => ("", ""),
    };
    let mut pi_provider = json!({"apiKey": CONNECTION_SECRET_MARKER});
    if !base_url.is_empty() {
        pi_provider["baseUrl"] = json!(base_url);
        pi_provider["api"] = json!("openai-completions");
        pi_provider["models"] = json!([{ "id": model }]);
    }
    let profile_id = stable_id(profile_prefix, source_id);
    let provider_id = stable_id(provider_prefix, source_id);
    let created_at = now();
    Ok(GeneratedApplySpec {
        target_agent: AgentId::Pi,
        provider_id: provider_id.clone(),
        proposed: AdapterProfile {
            id: profile_id.clone(),
            name: format!("{display} → Pi ({})", safe_label(source_id)),
            source_kind,
            source_id: source_id.into(),
            target_agent_id: AgentId::Pi,
            route: AdapterRoute::ConfigSync,
            mode: AdapterProfileMode::Api,
            status: AdapterProfileStatus::Applying,
            rule_id: rule_id.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        },
        provider: ProviderInput {
            id: provider_id,
            agent_id: AgentId::Pi,
            name: format!("{display} ({})", safe_label(source_id)),
            settings_config: json!({
                "models": {
                    "providers": {
                        (slot): pi_provider
                    }
                }
            }),
            meta: generated_meta(rule_id, &profile_id, source_kind, source_id, None),
            is_current: false,
        },
    })
}

fn dsh_deepseek_spec(source_id: &str) -> GeneratedApplySpec {
    let profile_id = stable_id(DSH_DEEPSEEK_PROFILE_PREFIX, source_id);
    let provider_id = stable_id(DSH_DEEPSEEK_PROVIDER_PREFIX, source_id);
    let created_at = now();
    let model = map_adapter_model(AdapterSourceProduct::DeepseekApi, AgentId::Dsh, "")
        .unwrap_or(DSH_DEFAULT_MODEL);
    GeneratedApplySpec {
        target_agent: AgentId::Dsh,
        provider_id: provider_id.clone(),
        proposed: AdapterProfile {
            id: profile_id.clone(),
            name: format!("DeepSeek → DSH ({})", safe_label(source_id)),
            source_kind: AdapterSourceKind::Provider,
            source_id: source_id.into(),
            target_agent_id: AgentId::Dsh,
            route: AdapterRoute::ConfigSync,
            mode: AdapterProfileMode::Api,
            status: AdapterProfileStatus::Applying,
            rule_id: DEEPSEEK_DSH_RULE_ID.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        },
        provider: ProviderInput {
            id: provider_id,
            agent_id: AgentId::Dsh,
            name: format!("DeepSeek API ({})", safe_label(source_id)),
            settings_config: json!({
                "provider": DSH_DEEPSEEK_PROVIDER_SLOT,
                "model": model,
                "apiKeyEnv": DSH_API_KEY_ENV,
                "baseURL": DEEPSEEK_API_BASE_URL,
                "api_key": CONNECTION_SECRET_MARKER,
            }),
            meta: generated_meta(
                DEEPSEEK_DSH_RULE_ID,
                &profile_id,
                AdapterSourceKind::Provider,
                source_id,
                Some("deepseek"),
            ),
            is_current: false,
        },
    }
}

fn generated_meta(
    rule_id: &str,
    profile_id: &str,
    source_kind: AdapterSourceKind,
    source_id: &str,
    preset: Option<&str>,
) -> serde_json::Value {
    let mut meta = serde_json::Map::new();
    if let Some(preset) = preset {
        meta.insert("preset".into(), json!(preset));
    }
    meta.insert("generatedBy".into(), json!("adapter"));
    meta.insert("adapterRuleId".into(), json!(rule_id));
    meta.insert("adapterRuleVersion".into(), json!(1));
    meta.insert("adapterSecretMode".into(), json!("source_reference"));
    meta.insert("adapterProfileId".into(), json!(profile_id));
    meta.insert(
        "adapterSourceRef".into(),
        json!({"kind": source_kind.as_str(), "id": source_id}),
    );
    serde_json::Value::Object(meta)
}

/// 投影契约：id / agent / settings / 合同 meta。
/// 不比较 `name`：展示名随票 display 变化，重 bind 不得因此重写 live。
fn provider_matches_projection(
    provider: &crate::models::Provider,
    projection: &ProviderInput,
) -> bool {
    provider.id == projection.id
        && provider.agent_id == projection.agent_id
        && provider.settings_config == projection.settings_config
        && projection_contract_meta(&provider.meta) == projection_contract_meta(&projection.meta)
}

fn projection_contract_meta(meta: &serde_json::Value) -> serde_json::Value {
    let mut cloned = meta.clone();
    if let Some(object) = cloned.as_object_mut() {
        object.remove(PREVIOUS_CURRENT_ID);
        object.remove(PREVIOUS_BACKUP_ID);
    }
    cloned
}

fn stamp_previous_restore_meta(
    meta: &mut serde_json::Value,
    previous_current: Option<&Provider>,
    generated_id: &str,
    existing: Option<&Provider>,
) {
    let Some(object) = meta.as_object_mut() else {
        return;
    };
    let previous_id = previous_current
        .map(|provider| provider.id.as_str())
        .filter(|id| *id != generated_id);
    match previous_id {
        Some(id) => {
            object.insert(PREVIOUS_CURRENT_ID.into(), json!(id));
        }
        None => {
            if let Some(existing_id) = existing
                .and_then(|provider| provider.meta.get(PREVIOUS_CURRENT_ID))
                .cloned()
            {
                object.insert(PREVIOUS_CURRENT_ID.into(), existing_id);
            }
        }
    }
    if let Some(existing_backup) = existing
        .and_then(|provider| provider.meta.get(PREVIOUS_BACKUP_ID))
        .cloned()
    {
        object.insert(PREVIOUS_BACKUP_ID.into(), existing_backup);
    }
}

fn provider_input(provider: &Provider) -> ProviderInput {
    ProviderInput {
        id: provider.id.clone(),
        agent_id: provider.agent_id,
        name: provider.name.clone(),
        settings_config: provider.settings_config.clone(),
        meta: provider.meta.clone(),
        is_current: provider.is_current,
    }
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
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
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
