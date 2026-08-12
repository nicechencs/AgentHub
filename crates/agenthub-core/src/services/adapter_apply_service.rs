//! Narrow, write-side adapter projection for Kimi membership -> Claude.
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
    AdapterApplyRequest, AdapterApplyResult, AdapterProfile, AdapterProfileStatus, AdapterRoute,
    AdapterRouteRequest, AdapterSourceKind, AdapterSupport, AgentId, Provider, ProviderInput,
};
use crate::services::{
    AdapterRouteService, AdapterSecretResolver, ProviderLiveConfigSnapshot, ProviderLiveSagaGuard,
    ProviderService,
};
use crate::storage::{AdapterProfileRepo, Database};

const RULE_ID: &str = "kimi-membership-to-claude-v1";
const RULE_VERSION: &str = "1";
const KIMI_CLAUDE_BASE_URL: &str = "https://api.kimi.com/coding/";
const CONNECTION_SECRET_MARKER: &str = "$AGENTHUB_CONNECTION_SECRET$";

/// Applies the sole supported write-side route and owns its generated profile.
pub struct AdapterApplyService {
    routes: AdapterRouteService,
    profiles: AdapterProfileRepo,
    providers: ProviderService,
    secrets: AdapterSecretResolver,
}

/// Pre-switch snapshot used to reverse a successful live switch when profile
/// finalization (or the switch itself) fails. Deliberately private and
/// non-serializable: the live config may contain materialized credentials.
struct ClaudeApplySnapshot {
    /// Generated provider row before create/update in this apply, if any.
    generated_before: Option<Provider>,
    /// Claude current provider before switch (may equal `generated_before`).
    previous_current: Option<Provider>,
    live_config: ProviderLiveConfigSnapshot,
    created: bool,
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
        self.ensure_supported(request)?;
        let source_id = request.source_id.trim();
        // Validate before creating a profile or provider: a dangling/masked
        // source must be a completely side-effect-free failure.
        self.secrets.validate_kimi_membership_source(source_id)?;
        // Acquire before reading or creating any generated-provider/profile
        // state. The guard covers every compensation input and mutation below.
        let saga_guard = self.providers.begin_live_saga(AgentId::Claude)?;
        let profile_id = stable_id("adapter-kimi-claude", source_id);
        let provider_id = stable_id("claude-kimi-adapter", source_id);
        let created_at = now();
        let proposed = AdapterProfile {
            id: profile_id.clone(),
            name: format!("Kimi → Claude ({})", safe_label(source_id)),
            source_kind: AdapterSourceKind::Provider,
            source_id: source_id.into(),
            target_agent_id: AgentId::Claude,
            route: AdapterRoute::NativeEndpoint,
            status: AdapterProfileStatus::Applying,
            rule_id: RULE_ID.into(),
            rule_version: RULE_VERSION.into(),
            generated_provider_id: Some(provider_id.clone()),
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        };
        let mut profile = self.profiles.create_or_get(&proposed)?;
        if !same_profile_contract(&profile, &proposed) {
            return Err(AppError::message(
                "adapter.profile_conflict",
                "adapter profile conflicts with requested rule",
            ));
        }

        let provider = ProviderInput {
            id: provider_id.clone(),
            agent_id: AgentId::Claude,
            name: format!("Kimi Code ({})", safe_label(source_id)),
            settings_config: json!({"env": {
                "ANTHROPIC_BASE_URL": KIMI_CLAUDE_BASE_URL,
                "ANTHROPIC_AUTH_TOKEN": CONNECTION_SECRET_MARKER,
            }}),
            meta: json!({
                "preset": "anthropic-compatible",
                "generatedBy": "adapter",
                "adapterRuleId": RULE_ID,
                "adapterRuleVersion": 1,
                "adapterSecretMode": "source_reference",
                "adapterProfileId": profile_id,
                "adapterSourceRef": {"kind": "provider", "id": source_id},
            }),
            is_current: false,
        };
        let existing = match self.providers.get(&provider_id, Some(AgentId::Claude)) {
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
                && provider_matches_projection(existing, &provider)
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
        let previous_current = match self.providers.repo().get_current(AgentId::Claude) {
            Ok(current) => current,
            Err(error) => return Err(self.fail_profile(profile, &error)),
        };
        let live_config = match self
            .providers
            .capture_live_config_snapshot_with_guard(&saga_guard, AgentId::Claude)
        {
            Ok(snapshot) => snapshot,
            Err(error) => return Err(self.fail_profile(profile, &error)),
        };
        let generated_before = existing.clone();
        let created = existing.is_none();
        let snapshot = ClaudeApplySnapshot {
            generated_before,
            previous_current,
            live_config,
            created,
        };

        // Create/repair the pool row before switch; the switched provider is
        // returned from switch_with_guard below.
        if let Err(error) = match existing {
            Some(existing) if provider_matches_projection(&existing, &provider) => Ok(()),
            Some(_) => {
                // Repair the persisted generated projection before switching it
                // live.  Explicitly demote first so the switch owns the only
                // live write and re-validates/materializes the source secret.
                let repaired = ProviderInput {
                    is_current: false,
                    ..provider.clone()
                };
                self.providers
                    .update_with_guard(&saga_guard, &repaired)
                    .map(|_| ())
            }
            None => self
                .providers
                .create_with_guard(&saga_guard, &provider)
                .map(|_| ()),
        } {
            return Err(self.fail_profile(profile, &error));
        }

        let switched =
            match self
                .providers
                .switch_with_guard(&saga_guard, &provider_id, AgentId::Claude)
            {
                Ok(result) => result.provider,
                Err(error) => {
                    if let Err(restore_error) =
                        self.compensate_claude_apply(&saga_guard, &provider_id, &snapshot)
                    {
                        return Err(self.fail_profile(profile, &restore_error));
                    }
                    return Err(self.fail_profile(profile, &error));
                }
            };

        profile.status = AdapterProfileStatus::Active;
        profile.generated_provider_id = Some(provider_id.clone());
        profile.last_error_code = None;
        profile.updated_at = now();
        let profile = match self.profiles.update(&profile) {
            Ok(profile) => profile,
            Err(_) => {
                let mut attention = profile;
                attention.status = AdapterProfileStatus::NeedsAttention;
                let restore_error = self
                    .compensate_claude_apply(&saga_guard, &provider_id, &snapshot)
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
        self.profiles.list(source_kind, source_id, target_agent_id)
    }

    /// Removes the profile and its generated provider when it still exists.
    pub fn remove(&self, profile_id: &str) -> Result<()> {
        // This service only owns the Kimi -> Claude direct route. Acquire the
        // live-write authority before reading any profile/provider state used
        // for its delete decision so another process cannot switch or repair
        // the same generated provider between preflight and deletion.
        let saga_guard = self.providers.begin_live_saga(AgentId::Claude)?;
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
                    return Err(AppError::Unsupported(
                        "先在 Connections 切换后再移除此适配器".into(),
                    ));
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

    fn ensure_supported(&self, request: &AdapterApplyRequest) -> Result<()> {
        let analysis = self.routes.analyze(&AdapterRouteRequest {
            source_kind: request.source_kind,
            source_id: request.source_id.clone(),
            target_agent_id: request.target_agent_id,
        })?;
        if request.source_kind == AdapterSourceKind::Provider
            && request.target_agent_id == AgentId::Claude
            && analysis.route == AdapterRoute::NativeEndpoint
            && analysis.support == AdapterSupport::Stable
        {
            Ok(())
        } else {
            Err(AppError::Unsupported(
                "adapter apply currently supports only Kimi membership provider -> Claude".into(),
            ))
        }
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

    /// Inverse of a successful Claude switch: restore the generated pool row
    /// (or delete a create), re-select the pre-switch current provider when one
    /// existed, then force the pre-switch live config. Every step is attempted
    /// even if an earlier step fails.
    fn compensate_claude_apply(
        &self,
        saga_guard: &ProviderLiveSagaGuard<'_>,
        provider_id: &str,
        snapshot: &ClaudeApplySnapshot,
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
                    .delete_with_guard(saga_guard, provider_id, AgentId::Claude)
            {
                failed = Some(error);
            }
        }

        if let Some(previous) = &snapshot.previous_current {
            if !generated_was_previous {
                if let Err(error) =
                    self.providers
                        .switch_with_guard(saga_guard, &previous.id, AgentId::Claude)
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

fn same_profile_contract(existing: &AdapterProfile, proposed: &AdapterProfile) -> bool {
    existing.id == proposed.id
        && existing.name == proposed.name
        && existing.source_kind == proposed.source_kind
        && existing.source_id == proposed.source_id
        && existing.target_agent_id == proposed.target_agent_id
        && existing.route == proposed.route
        && existing.rule_id == proposed.rule_id
        && existing.rule_version == proposed.rule_version
        && existing.generated_provider_id == proposed.generated_provider_id
}

fn provider_owned_by(provider: &crate::models::Provider, profile: &AdapterProfile) -> bool {
    provider.id == stable_id("claude-kimi-adapter", &profile.source_id)
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

fn provider_matches_projection(
    provider: &crate::models::Provider,
    projection: &ProviderInput,
) -> bool {
    provider.id == projection.id
        && provider.agent_id == projection.agent_id
        && provider.name == projection.name
        && provider.settings_config == projection.settings_config
        && provider.meta == projection.meta
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
