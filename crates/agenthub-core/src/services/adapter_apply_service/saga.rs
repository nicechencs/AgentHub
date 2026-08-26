use serde_json::json;

use crate::error::{AppError, Result};
use crate::models::{
    AdapterApplyRequest, AdapterApplyResult, AdapterProfile, AdapterProfileFilter,
    AdapterProfileStatus, AdapterRoute, AdapterRouteAnalysis, AdapterRouteRequest,
    AdapterSourceKind, AgentId, Provider, ProviderInput,
};
use crate::services::adapter_route_constants::{
    is_kimi_code_membership_source, KIMI_CLAUDE_RULE_ID,
};
use crate::services::ProviderLiveSagaGuard;

use super::{AdapterApplyService, ApplySnapshot, GeneratedApplySpec};

// Re-exported so `tests` (`use super::*`) keeps seeing specs helpers.
#[allow(unused_imports)]
pub(super) use super::specs::*;

pub(super) const RULE_ID: &str = KIMI_CLAUDE_RULE_ID;
pub(super) const KIMI_PI_RULE_ID: &str = "kimi-membership-to-pi-v1";
pub(super) const KIMI_GROK_RULE_ID: &str = "kimi-membership-to-grok-v1";
pub(super) const DEEPSEEK_DSH_RULE_ID: &str = "deepseek-api-to-dsh-v1";
pub(super) const PREVIOUS_CURRENT_ID: &str = "previousCurrentId";
pub(super) const PREVIOUS_BACKUP_ID: &str = "previousBackupId";

impl AdapterApplyService {
    // Only exercised by `adapter_route_service` tests; kept pub(crate) for them.
    #[allow(dead_code)]
    pub(crate) fn apply_has_arm(
        rule_id: &str,
        source_kind: AdapterSourceKind,
        target: AgentId,
        route: AdapterRoute,
    ) -> bool {
        match (source_kind, target, route) {
            (_, AgentId::Claude, AdapterRoute::NativeEndpoint) => {
                rule_id == RULE_ID || is_claude_native_explicit_rule(rule_id)
            }
            (_, AgentId::Codex, AdapterRoute::NativeEndpoint) => is_codex_native_rule(rule_id),
            (_, AgentId::Grok, AdapterRoute::NativeEndpoint) => is_grok_native_rule(rule_id),
            (sk, AgentId::Pi, AdapterRoute::ConfigSync)
                if sk == AdapterSourceKind::Provider || rule_id == KIMI_PI_RULE_ID =>
            {
                rule_id == KIMI_PI_RULE_ID || is_explicit_api_to_pi_rule(rule_id)
            }
            (AdapterSourceKind::Account, AgentId::Pi, AdapterRoute::ConfigSync) => {
                is_explicit_api_to_pi_rule(rule_id) || is_subscription_pi_rule(rule_id)
            }
            (AdapterSourceKind::Provider, AgentId::Dsh, AdapterRoute::ConfigSync) => {
                rule_id == DEEPSEEK_DSH_RULE_ID
            }
            _ => false,
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
            (source_kind, AgentId::Grok, AdapterRoute::NativeEndpoint)
                if analysis
                    .rule_id
                    .as_deref()
                    .is_some_and(is_grok_native_rule) =>
            {
                let rule = analysis.rule_id.as_deref().expect("checked above");
                if rule == KIMI_GROK_RULE_ID {
                    self.secrets
                        .validate_kimi_membership_source(source_kind, source_id)?;
                } else {
                    self.secrets
                        .validate_explicit_api_source(rule, source_kind, source_id)?;
                }
                self.apply_generated(grok_native_spec(source_kind, source_id, rule)?)
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
                "adapter apply currently supports Kimi membership Provider/Account -> Claude/Pi/Grok, OpenAI API -> Grok, GLM/DeepSeek ticket -> Claude/Codex, API or subscription Account -> Pi, and DeepSeek API provider -> DSH".into(),
            )),
        }
    }

    pub(super) fn apply_generated(
        &self,
        mut spec: GeneratedApplySpec,
    ) -> Result<AdapterApplyResult> {
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
        let previous_current = match self.providers.get_current(spec.target_agent) {
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
                        if self
                            .compensate_apply(
                                &saga_guard,
                                &spec.provider_id,
                                spec.target_agent,
                                &snapshot,
                            )
                            .is_err()
                        {
                            return Err(self.fail_rollback_incomplete(profile));
                        }
                        return Err(self.fail_profile(profile, &error));
                    }
                }
                result.provider
            }
            Err(error) => {
                if self
                    .compensate_apply(&saga_guard, &spec.provider_id, spec.target_agent, &snapshot)
                    .is_err()
                {
                    return Err(self.fail_rollback_incomplete(profile));
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
                if self
                    .compensate_apply(&saga_guard, &spec.provider_id, spec.target_agent, &snapshot)
                    .is_err()
                {
                    return Err(self.fail_rollback_incomplete(attention));
                }
                attention.last_error_code = Some("adapter.profile_finalize".into());
                attention.updated_at = now();
                let _ = self.profiles.update(&attention);
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

    pub(super) fn ensure_supported(
        &self,
        request: &AdapterApplyRequest,
    ) -> Result<AdapterRouteAnalysis> {
        self.reject_non_membership_kimi_provider(request)?;
        let analysis = self.routes.analyze(&AdapterRouteRequest {
            source_kind: request.source_kind,
            source_id: request.source_id.clone(),
            target_agent_id: request.target_agent_id,
        })?;
        if apply_request_supported(
            request.source_kind,
            request.target_agent_id,
            analysis.route,
            analysis.rule_id.as_deref(),
            analysis.support,
            analysis.gate_kind,
        ) {
            Ok(analysis)
        } else {
            Err(AppError::Unsupported(
                "adapter apply currently supports Kimi membership Provider/Account -> Claude/Pi/Grok, OpenAI API -> Grok, GLM/DeepSeek ticket -> Claude/Codex, API or subscription Account -> Pi, and DeepSeek API provider -> DSH".into(),
            ))
        }
    }

    /// Kimi provider rows are not interchangeable with generic OpenAI-compatible
    /// providers for the Claude/Pi apply surface. Keep this source validation
    /// before route dispatch so a closed source cannot create a failed profile
    /// while a later secret/live-config step rejects it.
    fn reject_non_membership_kimi_provider(&self, request: &AdapterApplyRequest) -> Result<()> {
        if request.source_kind != AdapterSourceKind::Provider
            || !matches!(request.target_agent_id, AgentId::Claude | AgentId::Pi)
        {
            return Ok(());
        }

        let source_id = request.source_id.trim();
        let Some(provider) = self.providers.get_by_id(source_id)? else {
            return Ok(());
        };
        if provider.agent_id == AgentId::Kimi
            && !is_kimi_code_membership_source(
                provider.agent_id,
                &provider.meta,
                &provider.settings_config,
            )
        {
            return Err(AppError::Unsupported(
                "adapter apply currently supports Kimi membership providers for Claude/Pi; Moonshot and other non-membership Kimi providers are unsupported".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn persist_previous_backup_id(
        &self,
        saga_guard: &ProviderLiveSagaGuard<'_>,
        provider: &Provider,
        backup_id: &str,
    ) -> Result<Provider> {
        let already = provider
            .meta
            .get(PREVIOUS_BACKUP_ID)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .is_some_and(|id| !id.is_empty());
        if already {
            // First-bind snapshot wins. Later repair/re-switch must not
            // replace it with a leftover 本机路由 projection.
            return Ok(provider.clone());
        }
        let mut input = provider_input(provider);
        let Some(meta) = input.meta.as_object_mut() else {
            return Ok(provider.clone());
        };
        meta.insert(PREVIOUS_BACKUP_ID.into(), json!(backup_id));
        self.providers.update_with_guard(saga_guard, &input)
    }

    pub(super) fn restore_previous_binding(
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
                self.providers.restore_named_backup_or_clean_codex(
                    saga_guard,
                    backup_id,
                    target_agent,
                )?;
            }
        }
        if let Some(previous_id) = previous_id {
            match self.providers.get(previous_id, Some(target_agent)) {
                Ok(_) => {
                    self.providers
                        .switch_with_guard(saga_guard, previous_id, target_agent)?;
                    if target_agent == AgentId::Codex {
                        crate::integrations::agents::codex::leftover::strip_live_bridge_leftovers(
                        )?;
                    }
                    return Ok(());
                }
                Err(AppError::NotFound(_)) => {}
                Err(error) => return Err(error),
            }
        }
        if let Some(backup_id) = backup_id.filter(|_| !is_subscription_oauth) {
            self.providers.restore_named_backup_or_clean_codex(
                saga_guard,
                backup_id,
                target_agent,
            )?;
        } else if target_agent == AgentId::Codex {
            crate::integrations::agents::codex::leftover::strip_live_bridge_leftovers()?;
        }
        Ok(())
    }

    pub(super) fn fail_profile(&self, mut profile: AdapterProfile, error: &AppError) -> AppError {
        profile.status = AdapterProfileStatus::NeedsAttention;
        profile.last_error_code = Some(error.code().into());
        profile.updated_at = now();
        let _ = self.profiles.update(&profile);
        AppError::message(
            "adapter_apply.failed",
            format!("adapter apply failed: {}", error.code()),
        )
    }

    pub(super) fn fail_rollback_incomplete(&self, mut profile: AdapterProfile) -> AppError {
        profile.status = AdapterProfileStatus::NeedsAttention;
        profile.last_error_code = Some("adapter.rollback_incomplete".into());
        profile.updated_at = now();
        let _ = self.profiles.update(&profile);
        AppError::message(
            "adapter.rollback_incomplete",
            "adapter apply failed and repair rollback was incomplete",
        )
    }

    /// Inverse of a successful live switch: restore the generated pool row
    /// (or delete a create), re-select the pre-switch current provider when one
    /// existed, then force the pre-switch live config. Pool/binding restoration
    /// never writes live config, so a later live-restore failure cannot revert
    /// the diagnosable database snapshot. Every step is attempted even if an
    /// earlier step fails.
    pub(super) fn compensate_apply(
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
            // current is re-selected below without writing live config.
            input.is_current = generated_was_previous;
            if let Err(error) = self.providers.update_pool_with_guard(saga_guard, &input) {
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
                let mut input = provider_input(previous);
                input.is_current = true;
                if let Err(error) = self.providers.update_pool_with_guard(saga_guard, &input) {
                    failed = Some(error);
                }
            }
        }

        // Always restore live last so a pool restore that would otherwise
        // materialize a drifted value cannot leave the on-disk config changed.
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
