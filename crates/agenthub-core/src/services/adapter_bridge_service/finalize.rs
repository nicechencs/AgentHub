use super::rules::*;
use super::*;

impl AdapterBridgeService {
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
        // Existing active profiles keep their projection and stay eligible
        // for `list_auto_start_profiles`. A first-time apply has no bound
        // port: leaving `applying` shows a zombie route in Connections.
        if profile.status == AdapterProfileStatus::Applying && profile.local_port.is_none() {
            profile.status = AdapterProfileStatus::NeedsAttention;
        }
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

    /// Validates that a profile and its generated provider are an exact
    /// supported local-bridge projection before deletion.
    ///
    /// The current provider is always rejected: callers must switch the
    /// Codex Connection first. A listener is deliberately not stopped here;
    /// the desktop controller performs that reversible operation between this
    /// preflight and [`Self::complete_remove`].
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
            AppError::InvalidArg("adapter profile is not a supported local bridge".into())
        })?;
        let upstream_auth =
            self.resolve_upstream_auth(&rule, profile.source_kind, &profile.source_id)?;
        Ok(AdapterBridgeRestoreMaterial {
            material: AdapterBridgeRuntimeMaterial {
                profile_id: profile.id.clone(),
                source_connection_id: profile.source_id.clone(),
                preferred_port: Some(local_port),
                upstream_base_url: rule.upstream_base_url.into(),
                upstream_model: rule.default_model.into(),
                protocol: rule.protocol,
                local_surface: rule.local_surface,
                upstream_auth,
                local_bearer: local_bearer_from_provider(&provider)?,
            },
            needs_reprojection: !provider_matches_current_projection(
                &provider,
                &profile,
                Some(local_port),
            ),
            profile,
        })
    }
}
