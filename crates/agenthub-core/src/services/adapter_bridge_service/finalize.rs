use super::rules::*;
use super::*;

/// Why `resolve_restore_material` failed for an auto-start local route.
///
/// `SourceMissing` / `ProfileCorrupt` cannot succeed until the user rebuilds
/// the route; restore must stop retrying them. `LoginUnusable` is recoverable
/// after the user signs in again. `Transient` keeps the historical retryable
/// marker. `Ineligible` is a race against the auto-start list and is skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreSourceFailureKind {
    SourceMissing,
    LoginUnusable,
    ProfileCorrupt,
    Transient,
    Ineligible,
}

impl RestoreSourceFailureKind {
    pub fn reason(self) -> &'static str {
        match self {
            Self::SourceMissing => "source_missing",
            Self::LoginUnusable => "login_unusable",
            Self::ProfileCorrupt => "profile_corrupt",
            Self::Transient => "transient",
            Self::Ineligible => "ineligible",
        }
    }

    pub fn persist_code(self) -> &'static str {
        match self {
            Self::SourceMissing => "adapter.bridge_source_missing",
            Self::LoginUnusable => "adapter.bridge_source_expired",
            Self::ProfileCorrupt => "adapter.profile_invalid",
            Self::Transient => "adapter.bridge_restore_source",
            Self::Ineligible => "adapter.bridge_restore_ineligible",
        }
    }

    pub fn recoverable(self) -> bool {
        matches!(self, Self::LoginUnusable | Self::Transient)
    }

    pub fn warn_message(self) -> &'static str {
        match self {
            Self::SourceMissing => "adapter bridge restore skipped: source no longer exists",
            Self::LoginUnusable => "adapter bridge restore skipped: source login is unusable",
            Self::ProfileCorrupt => "adapter bridge restore skipped: profile is corrupt",
            Self::Transient => "adapter bridge source could not be restored",
            Self::Ineligible => {
                "adapter bridge restore skipped: profile is not auto-start eligible"
            }
        }
    }
}

/// Classified restore-source failure. `error_code` is the underlying AppError
/// code written to logs as `restore_error`; `kind.reason()` is the stable
/// reason field.
#[derive(Debug, Clone)]
pub struct RestoreSourceFailure {
    pub kind: RestoreSourceFailureKind,
    pub error_code: &'static str,
    pub message: String,
}

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
        let rule = rule_for_id(&profile.rule_id).ok_or_else(|| {
            AppError::InvalidArg("这条本机路由已失效，无法启动。请删除后重建。".into())
        })?;
        let (_url, model, _listed, _protocol, window) = super::prepare::openai_source_upstream(
            self,
            &rule,
            profile.source_kind,
            &profile.source_id,
        );
        let mut input =
            projected_provider_input(&profile, &local_bearer, bound_port, &model, window)?;
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
    ///
    /// Source-resolution failures use stable codes so the desktop host can
    /// distinguish a deleted source (`adapter.bridge_source_missing`) from an
    /// expired login (`adapter.bridge_source_expired`) and a corrupt profile
    /// (`adapter.profile_invalid` / `adapter.provider_missing`).
    pub fn resolve_restore_material(
        &self,
        profile_id: &str,
    ) -> Result<AdapterBridgeRestoreMaterial> {
        let profile = match self.bridge_profile(profile_id) {
            Ok(profile) => profile,
            Err(error) if error.code() == "not_found" => return Err(error),
            Err(_) => {
                return Err(AppError::message(
                    "adapter.profile_invalid",
                    "这条本机路由已失效，无法启动。请删除后重建。",
                ));
            }
        };
        if profile.status != AdapterProfileStatus::Active || !profile.auto_start {
            return Err(AppError::message(
                "adapter.bridge_restore_ineligible",
                "adapter bridge profile is not eligible for automatic restore",
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
            AppError::message(
                "adapter.profile_invalid",
                "这条本机路由已失效，无法启动。请删除后重建。",
            )
        })?;
        if !self.source_row_exists(profile.source_kind, &profile.source_id)? {
            return Err(AppError::message(
                "adapter.bridge_source_missing",
                "bridge restore source no longer exists",
            ));
        }
        let upstream_auth = self
            .resolve_upstream_auth(&rule, profile.source_kind, &profile.source_id)
            .map_err(|_| {
                AppError::message(
                    "adapter.bridge_source_expired",
                    "bridge restore source login is unusable",
                )
            })?;
        let (
            upstream_base_url,
            upstream_model,
            configured_listed_models,
            protocol,
            context_window_tokens,
        ) = super::prepare::openai_source_upstream(
            self,
            &rule,
            profile.source_kind,
            &profile.source_id,
        );
        let material = self.attach_route_index(
            AdapterBridgeRuntimeMaterial {
                profile_id: profile.id.clone(),
                source_id: profile.source_id.clone(),
                preferred_port: Some(local_port),
                upstream_base_url,
                upstream_model: upstream_model.clone(),
                configured_listed_models,
                context_window_tokens,
                protocol,
                local_surface: rule.local_surface,
                source: rule.source,
                target_agent: rule.target_agent,
                downstream_dialect: crate::models::RouteDownstreamDialect::for_agent(
                    rule.target_agent,
                ),
                upstream_auth,
                local_bearer: local_bearer_from_provider(&provider)?,
                route_index: None,
                index_enabled: false,
                codex_ingress_grok_upstream: false,
                grok_ingress_codex_upstream: false,
                schedule_policy: Default::default(),
                kiro_http: if protocol == BridgeUpstreamProtocol::KiroHttp {
                    self.secrets
                        .resolve_kiro_http_params(profile.source_kind, &profile.source_id)
                        .ok()
                } else {
                    None
                },
            },
            &profile,
        )?;
        Ok(AdapterBridgeRestoreMaterial {
            material,
            needs_reprojection: !provider_matches_current_projection(
                &provider,
                &profile,
                Some(local_port),
                &upstream_model,
                context_window_tokens,
            ),
            profile,
        })
    }

    pub fn classify_restore_source_failure(
        &self,
        profile_id: &str,
        error: &AppError,
    ) -> RestoreSourceFailureKind {
        match error.code() {
            "adapter.bridge_source_missing" => RestoreSourceFailureKind::SourceMissing,
            "adapter.bridge_source_expired" => RestoreSourceFailureKind::LoginUnusable,
            "adapter.bridge_restore_ineligible" => RestoreSourceFailureKind::Ineligible,
            "adapter.profile_invalid"
            | "adapter.provider_missing"
            | "adapter.provider_conflict" => RestoreSourceFailureKind::ProfileCorrupt,
            "not_found" | "invalid_arg" => {
                if self
                    .source_row_exists_for_profile(profile_id)
                    .ok()
                    .flatten()
                    == Some(false)
                {
                    RestoreSourceFailureKind::SourceMissing
                } else {
                    RestoreSourceFailureKind::ProfileCorrupt
                }
            }
            _ => RestoreSourceFailureKind::Transient,
        }
    }

    /// Persist a restore-source failure. Unrestorable rows lose auto-start so
    /// the next process start will not retry them. Recoverable rows keep
    /// `active` + auto-start with a `retryable:` marker.
    pub fn record_restore_source_failure(
        &self,
        profile_id: &str,
        kind: RestoreSourceFailureKind,
    ) -> Result<AdapterProfile> {
        match kind {
            RestoreSourceFailureKind::Ineligible => self.stored_bridge_profile(profile_id),
            RestoreSourceFailureKind::LoginUnusable | RestoreSourceFailureKind::Transient => {
                self.mark_retryable(profile_id, kind.persist_code())
            }
            RestoreSourceFailureKind::SourceMissing | RestoreSourceFailureKind::ProfileCorrupt => {
                self.stop_unrestorable_restore(profile_id, kind.persist_code())
            }
        }
    }

    pub fn restore_source_failure_from_error(
        &self,
        profile_id: &str,
        error: &AppError,
    ) -> RestoreSourceFailure {
        RestoreSourceFailure {
            kind: self.classify_restore_source_failure(profile_id, error),
            error_code: error.code(),
            message: error.to_string(),
        }
    }

    fn stop_unrestorable_restore(
        &self,
        profile_id: &str,
        error_code: &str,
    ) -> Result<AdapterProfile> {
        let mut profile = self.stored_bridge_profile(profile_id)?;
        let code = error_code.trim();
        if code.is_empty() {
            return Err(AppError::InvalidArg(
                "adapter bridge error code must not be empty".into(),
            ));
        }
        profile.auto_start = false;
        profile.status = AdapterProfileStatus::NeedsAttention;
        profile.last_error_code = Some(code.into());
        profile.updated_at = now();
        self.profiles.update(&profile)
    }

    fn stored_bridge_profile(&self, profile_id: &str) -> Result<AdapterProfile> {
        self.profiles
            .get(profile_id)?
            .ok_or_else(|| AppError::NotFound(format!("adapter profile not found: {profile_id}")))
    }

    fn source_row_exists(&self, source_kind: AdapterSourceKind, source_id: &str) -> Result<bool> {
        let source_id = source_id.trim();
        if source_id.is_empty() {
            return Ok(false);
        }
        match source_kind {
            AdapterSourceKind::Provider => Ok(self.providers.get_by_id(source_id)?.is_some()),
            AdapterSourceKind::Account => Ok(self.secrets.accounts.get_by_id(source_id)?.is_some()),
        }
    }

    fn source_row_exists_for_profile(&self, profile_id: &str) -> Result<Option<bool>> {
        let Some(profile) = self.profiles.get(profile_id)? else {
            return Ok(None);
        };
        self.source_row_exists(profile.source_kind, &profile.source_id)
            .map(Some)
    }
}
