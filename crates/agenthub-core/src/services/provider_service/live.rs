//! Live config import, capture/restore, and current-row live apply.

use std::path::Path;
use std::time::Instant;

use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{
    AdapterProfileFilter, AgentConfig, AgentId, BackupKind, Capability, Provider, ProviderInput,
};
use crate::services::adapter_projection::{
    classify_provider_config, leftover_live_flag, projection_import_error,
};
use crate::services::provider_identity::stamp_secret_hash;
use crate::services::switch_undo::extract_probe_url;
use crate::storage::AdapterProfileRepo;
use crate::utils::loopback::is_loopback_base_url;

use super::compensate::{compensated_current_apply_error, compensated_current_apply_error_with_db};
use super::pool::ProviderCommittedMutation;
use super::{
    ensure_config_agent, is_placeholder_import_name, live_config_is_empty, log_provider_op,
    log_switch_write, now_ts, validate_name, ProviderLiveSagaGuard, ProviderService,
};

/// Fail-closed copy when an agent cannot write the saved login to live files.
pub(super) fn live_write_unsupported(agent: AgentId) -> AppError {
    match agent {
        AgentId::Cursor => AppError::Unsupported(
            "Cursor 暂时不能把这份登录写到本机配置。请用 Cursor 自己的登录，或设置 CURSOR_API_KEY。"
                .into(),
        ),
        other => AppError::Unsupported(format!(
            "{} 暂时不能把这份登录写到本机配置。",
            other.as_str()
        )),
    }
}

pub(super) fn require_live_config_write(
    adapter: &dyn crate::adapters::AgentAdapter,
    agent: AgentId,
) -> Result<()> {
    if adapter.capability(Capability::ConfigWrite).is_usable() {
        Ok(())
    } else {
        Err(live_write_unsupported(agent))
    }
}

pub(super) fn log_live_switch_paths(
    agent: AgentId,
    adapter: &dyn crate::adapters::AgentAdapter,
    last4: &str,
) {
    for path in adapter.live_backup_paths() {
        log_switch_write(agent, &display_home_path(&path), last4);
    }
}

fn display_home_path(path: &Path) -> String {
    match crate::utils::paths::home_dir() {
        Ok(home) => path
            .strip_prefix(&home)
            .map(|rest| format!("~/{}", rest.display()))
            .unwrap_or_else(|_| path.display().to_string()),
        Err(_) => path.display().to_string(),
    }
}

/// An in-memory copy of one agent's complete live configuration, held only
/// while a cross-boundary saga may need to compensate a successful switch.
/// It intentionally has no serialization implementation or value-bearing
/// `Debug` output because provider configs can contain credentials.
#[derive(Clone)]
pub struct ProviderLiveConfigSnapshot {
    agent: AgentId,
    config: AgentConfig,
}

impl std::fmt::Debug for ProviderLiveConfigSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderLiveConfigSnapshot")
            .field("agent", &self.agent)
            .field("config", &"REDACTED")
            .finish()
    }
}

impl ProviderService {
    /// Capture the exact current live config for a narrowly-scoped saga
    /// compensation. The snapshot is never persisted or returned to a UI.
    pub fn capture_live_config_snapshot(
        &self,
        agent: AgentId,
    ) -> Result<ProviderLiveConfigSnapshot> {
        let guard = self.begin_live_saga(agent)?;
        self.capture_live_config_snapshot_with_guard(&guard, agent)
    }

    /// Capture a live config while an existing saga guard remains held.
    pub fn capture_live_config_snapshot_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        agent: AgentId,
    ) -> Result<ProviderLiveConfigSnapshot> {
        self.validate_live_saga_guard(guard, agent)?;
        let config = self.adapter(agent)?.read_config()?;
        ensure_config_agent(&config, agent)?;
        Ok(ProviderLiveConfigSnapshot { agent, config })
    }

    /// Restore a snapshot captured by [`Self::capture_live_config_snapshot`].
    /// ProviderService owns this write so compensation uses the same per-agent
    /// live-config lock as normal switches rather than bypassing it in a host.
    pub fn restore_live_config_snapshot(
        &self,
        snapshot: &ProviderLiveConfigSnapshot,
    ) -> Result<()> {
        let guard = self.begin_live_saga(snapshot.agent)?;
        self.restore_live_config_snapshot_with_guard(&guard, snapshot)
    }

    /// Restore a named live backup while an existing provider saga guard is held.
    pub fn restore_named_backup_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        backup_id: &str,
    ) -> Result<()> {
        let backup = self.backup.as_ref().ok_or_else(|| {
            AppError::Unsupported(
                "provider live restore requires an explicitly configured backup root".into(),
            )
        })?;
        let record = backup.get_by_id(backup_id)?;
        let agent = record.agent_id.ok_or_else(|| {
            AppError::InvalidArg(format!(
                "backup {backup_id} has no agent_id; cannot restore live files"
            ))
        })?;
        self.validate_live_saga_guard(guard, agent)?;
        backup
            .restore_with_guard(guard.as_live_write_guard(), backup_id)
            .map(|_| ())
    }

    /// Restore a first-bind official snapshot. A leftover 本机路由 snapshot
    /// is treated as "no previous config" and stripped instead.
    pub fn restore_named_backup_or_clean_codex(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        backup_id: &str,
        target: AgentId,
    ) -> Result<()> {
        if target == AgentId::Codex && self.backup_is_codex_bridge_leftover(backup_id) {
            return crate::integrations::agents::codex::leftover::strip_live_bridge_leftovers()
                .map(|_| ());
        }
        self.restore_named_backup_with_guard(guard, backup_id)?;
        if target == AgentId::Codex {
            crate::integrations::agents::codex::leftover::strip_live_bridge_leftovers()?;
        }
        Ok(())
    }

    pub(super) fn backup_is_codex_bridge_leftover(&self, backup_id: &str) -> bool {
        let Some(backup) = self.backup.as_ref() else {
            return false;
        };
        backup.get_by_id(backup_id).ok().is_some_and(|record| {
            crate::integrations::agents::codex::leftover::backup_is_bridge_leftover(&record)
        })
    }

    /// Persist first-bind restore pointers. Existing values are kept.
    pub fn persist_first_bind_restore_meta_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        provider: &Provider,
        previous_current_id: Option<&str>,
        backup_id: Option<&str>,
    ) -> Result<Provider> {
        let mut input = ProviderInput {
            id: provider.id.clone(),
            agent_id: provider.agent_id,
            name: provider.name.clone(),
            settings_config: provider.settings_config.clone(),
            meta: provider.meta.clone(),
            is_current: provider.is_current,
        };
        let Some(meta) = input.meta.as_object_mut() else {
            return Ok(provider.clone());
        };
        let mut changed = false;
        let has_previous = meta
            .get("previousCurrentId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .is_some_and(|id| !id.is_empty());
        if !has_previous {
            if let Some(id) = previous_current_id
                .map(str::trim)
                .filter(|id| !id.is_empty() && *id != provider.id)
            {
                meta.insert("previousCurrentId".into(), serde_json::json!(id));
                changed = true;
            }
        }
        let has_backup = meta
            .get("previousBackupId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .is_some_and(|id| !id.is_empty());
        if !has_backup {
            if let Some(id) = backup_id.map(str::trim).filter(|id| !id.is_empty()) {
                meta.insert("previousBackupId".into(), serde_json::json!(id));
                changed = true;
            }
        }
        if !changed {
            return Ok(provider.clone());
        }
        self.update_with_guard(guard, &input)
    }

    /// Restore a live config while an existing saga guard remains held.
    pub fn restore_live_config_snapshot_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        snapshot: &ProviderLiveConfigSnapshot,
    ) -> Result<()> {
        self.validate_live_saga_guard(guard, snapshot.agent)?;
        let adapter = self.adapter(snapshot.agent)?;
        adapter.restore_config(&snapshot.config)
    }

    /// Capture the agent's complete live provider config as a new current row.
    ///
    /// Secrets are preserved in the L1 provider pool; callers must use
    /// [`Provider::redacted`] before displaying/serializing the result.
    pub fn import_live(&self, agent: AgentId, name: Option<&str>) -> Result<Provider> {
        let started = Instant::now();
        let result = self.import_live_inner(agent, name);
        if result.is_ok() {
            self.snapshot_after_pool_change(agent, "after live provider import");
        }
        log_provider_op("import", agent, started, &result);
        result
    }

    pub(super) fn import_live_inner(&self, agent: AgentId, name: Option<&str>) -> Result<Provider> {
        let _lock = self.acquire_live_lock(agent)?;
        let adapter = self.adapter(agent)?;
        let mut live = adapter.read_config()?;
        ensure_config_agent(&live, agent)?;
        if agent == AgentId::Zcode {
            crate::adapters::zcode::scrub_non_portable_provider_secrets(&mut live.raw);
        }
        if live_config_is_empty(&live.raw) {
            return Err(AppError::NotFound(format!(
                "no live provider config found for agent {}",
                agent.as_str()
            )));
        }

        let hint = crate::integrations::agents::codex::live_import_hint(&live.raw);
        let derived_name = hint.as_ref().map(|hint| hint.label.clone());
        let display_name = name
            .map(str::to_owned)
            .or(derived_name.clone())
            .unwrap_or_else(|| format!("Imported {}", now_ts()));
        validate_name(&display_name)?;

        let profiles =
            AdapterProfileRepo::new(self.db.clone()).list_filtered(&AdapterProfileFilter {
                target_agent_id: Some(agent),
                ..Default::default()
            })?;
        let providers = self.repo.list(Some(agent))?;
        if classify_provider_config(
            agent,
            &live.raw,
            &profiles,
            &providers,
            leftover_live_flag(agent),
        )
        .is_projection()
        {
            return Err(projection_import_error());
        }

        // A live config is a single canonical snapshot per agent. Reuse the
        // existing live-import row instead of creating a UUID row on every
        // refresh. Manual providers are deliberately ignored: only rows whose
        // metadata explicitly identifies `source = live` participate here.
        // Import adds/updates the pool only. It must not steal the current
        // official login; 用这份登录 / 切换 writes live via switch().
        let saved = if let Some(existing) = self.find_live_import(agent)? {
            let desired_name = name
                .map(str::to_owned)
                .or_else(|| {
                    if is_placeholder_import_name(&existing.name) {
                        derived_name.clone()
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| existing.name.clone());
            let mut meta = existing.meta.clone();
            if let Some(hint) = &hint {
                if meta
                    .get("preset")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none()
                {
                    if let serde_json::Value::Object(map) = &mut meta {
                        map.insert("preset".into(), serde_json::json!(hint.preset));
                    }
                }
            }
            stamp_secret_hash(&mut meta, &live.raw);
            if existing.settings_config == live.raw
                && existing.name == desired_name
                && existing.meta == meta
            {
                self.stamp_provider_surface(existing)?
            } else {
                let input = ProviderInput {
                    id: existing.id,
                    agent_id: agent,
                    name: desired_name,
                    settings_config: live.raw,
                    meta,
                    is_current: existing.is_current,
                };
                self.update_inner(&input)?
            }
        } else {
            let mut meta = serde_json::json!({ "source": "live" });
            if let Some(hint) = &hint {
                if let serde_json::Value::Object(map) = &mut meta {
                    map.insert("preset".into(), serde_json::json!(hint.preset));
                }
            }
            stamp_secret_hash(&mut meta, &live.raw);
            let input = ProviderInput {
                id: format!("{}-live-{}", agent.as_str(), Uuid::new_v4()),
                agent_id: agent,
                name: display_name,
                settings_config: live.raw,
                meta,
                is_current: false,
            };
            // Use inner create so import is a single log op (not create + import).
            self.create_inner(&input)?
        };
        self.collapse_extra_loopback_providers(agent, &saved)?;
        self.resolve_after_identity_heal(saved)
    }

    /// After upserting the canonical live row, drop leftover same-agent
    /// loopback providers. Adapter-generated projections stay hidden and are
    /// not Connections tickets; remote-URL rows are left untouched.
    pub(super) fn collapse_extra_loopback_providers(
        &self,
        agent: AgentId,
        kept: &Provider,
    ) -> Result<()> {
        let Some(url) = extract_probe_url(&kept.settings_config) else {
            return Ok(());
        };
        if !is_loopback_base_url(&url) {
            return Ok(());
        }
        let others = self.repo.list(Some(agent))?;
        for other in others {
            if other.id == kept.id {
                continue;
            }
            if self.is_generated_projection(&other)? {
                continue;
            }
            let Some(other_url) = extract_probe_url(&other.settings_config) else {
                continue;
            };
            if !is_loopback_base_url(&other_url) {
                continue;
            }
            self.delete_inner(&other.id, agent)?;
        }
        Ok(())
    }

    /// Locate the canonical live-import row for one agent. Older databases may
    /// contain more than one duplicate; choose deterministically and leave
    /// the other historical rows untouched rather than deleting user data.
    pub(super) fn find_live_import(&self, agent: AgentId) -> Result<Option<Provider>> {
        let mut candidates: Vec<Provider> = self
            .repo
            .list(Some(agent))?
            .into_iter()
            .filter(|provider| {
                provider.meta.get("source").and_then(|value| value.as_str()) == Some("live")
            })
            .collect();
        candidates.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        Ok(candidates.into_iter().next())
    }

    /// After a pool save: if this row is the active connection, write the
    /// **new stored value** to live files. This must not reuse [`Self::switch`],
    /// which treats the existing live file as authoritative for the current row.
    ///
    /// Non-current rows stay pool-only. Missing backup root / writer capability
    /// is a no-op so CRUD tests and half-surface agents do not touch real files.
    pub(super) fn prepare_current_provider_live(
        &self,
        live_guard: &crate::services::LiveWriteGuard,
        agent: AgentId,
        will_be_current: bool,
        note: &str,
    ) -> Result<
        Option<(
            std::sync::Arc<dyn crate::adapters::AgentAdapter>,
            AgentConfig,
        )>,
    > {
        if !will_be_current {
            return Ok(None);
        }
        let Some(backup) = self.backup.as_ref() else {
            return Ok(None);
        };
        let adapter = match self.adapter(agent) {
            Ok(adapter) => adapter,
            Err(_) => return Ok(None),
        };
        if !adapter.capability(Capability::ConfigWrite).is_usable() {
            return Ok(None);
        }
        let live_before = adapter.read_config()?;
        ensure_config_agent(&live_before, agent)?;
        if let Err(error) =
            backup.snapshot_with_guard(live_guard, agent, BackupKind::AutoSwitch, Some(note))
        {
            if error.code() != "not_found" {
                return Err(error);
            }
        }
        Ok(Some((adapter, live_before)))
    }

    /// Apply a current provider while the caller's [`ProviderLiveSagaGuard`]
    /// remains held. Live failure restores only this transaction's precise
    /// footprint after a full CAS of the expected post-commit state.
    pub(super) fn apply_current_provider_live_committed(
        &self,
        committed: &ProviderCommittedMutation,
        adapter: std::sync::Arc<dyn crate::adapters::AgentAdapter>,
        live_before: AgentConfig,
    ) -> Result<()> {
        let stored = &committed.stored;
        let materialized = match self.secret_resolver.materialize_for_live(stored) {
            Ok(value) => value,
            Err(error) => {
                let db_rollback = self
                    .restore_committed_provider_mutation(stored.agent_id, committed)
                    .err();
                return Err(compensated_current_apply_error_with_db(
                    error,
                    None,
                    db_rollback,
                ));
            }
        };
        let target_config = AgentConfig {
            agent: stored.agent_id,
            raw: materialized.settings_config,
        };
        if live_before.raw == target_config.raw {
            return Ok(());
        }
        if let Err(error) = adapter.write_config(&target_config) {
            let live_rollback = adapter.write_config(&live_before).err();
            let db_rollback = self
                .restore_committed_provider_mutation(stored.agent_id, committed)
                .err();
            return Err(compensated_current_apply_error_with_db(
                error,
                live_rollback,
                db_rollback,
            ));
        }
        log_live_switch_paths(
            stored.agent_id,
            adapter.as_ref(),
            &super::switch_write_last4(stored),
        );
        Ok(())
    }

    pub(super) fn sync_current_provider_live(
        &self,
        live_guard: &crate::services::LiveWriteGuard,
        stored: &Provider,
        note: &str,
    ) -> Result<()> {
        if !stored.is_current {
            self.snapshot_after_pool_change_locked(live_guard, stored.agent_id, note);
            return Ok(());
        }
        let Some(backup) = self.backup.as_ref() else {
            return Ok(());
        };
        let adapter = match self.adapter(stored.agent_id) {
            Ok(adapter) => adapter,
            Err(_) => return Ok(()),
        };
        if !adapter.capability(Capability::ConfigWrite).is_usable() {
            self.snapshot_after_pool_change_locked(live_guard, stored.agent_id, note);
            return Ok(());
        }

        let live_before = adapter.read_config()?;
        ensure_config_agent(&live_before, stored.agent_id)?;
        let materialized = self.secret_resolver.materialize_for_live(stored)?;
        let target_config = AgentConfig {
            agent: stored.agent_id,
            raw: materialized.settings_config,
        };
        if live_before.raw == target_config.raw {
            self.snapshot_after_pool_change_locked(live_guard, stored.agent_id, note);
            return Ok(());
        }

        if let Err(error) = backup.snapshot_with_guard(
            live_guard,
            stored.agent_id,
            BackupKind::AutoSwitch,
            Some(&format!("before applying current provider {}", stored.id)),
        ) {
            if error.code() != "not_found" {
                return Err(error);
            }
        }
        if let Err(error) = adapter.write_config(&target_config) {
            let live_rollback = adapter.write_config(&live_before).err();
            return Err(compensated_current_apply_error(error, live_rollback));
        }
        log_live_switch_paths(
            stored.agent_id,
            adapter.as_ref(),
            &super::switch_write_last4(stored),
        );
        Ok(())
    }

    /// Pool-only changes keep an audit snapshot of the untouched live file.
    /// A missing live file is a normal no-op.
    pub(super) fn snapshot_after_pool_change_locked(
        &self,
        live_guard: &crate::services::LiveWriteGuard,
        agent: AgentId,
        note: &str,
    ) {
        let Some(backup) = self.backup.as_ref() else {
            return;
        };
        if !backup.keep_live_file_copies() {
            return;
        }
        if let Err(error) =
            backup.snapshot_with_guard(live_guard, agent, BackupKind::AutoSwitch, Some(note))
        {
            if error.code() != "not_found" {
                tracing::warn!(
                    target: targets::BACKUP,
                    agent = agent.as_str(),
                    error = %error,
                    "automatic post-change live snapshot failed"
                );
            }
        }
    }

    pub(super) fn snapshot_after_pool_change(&self, agent: AgentId, note: &str) {
        let Some(backup) = self.backup.as_ref() else {
            return;
        };
        if !backup.keep_live_file_copies() {
            return;
        }
        if let Err(error) = backup.snapshot(agent, BackupKind::AutoSwitch, Some(note)) {
            if error.code() != "not_found" {
                tracing::warn!(
                    target: targets::BACKUP,
                    agent = agent.as_str(),
                    error = %error,
                    "automatic post-change live snapshot failed"
                );
            }
        }
    }

    pub(super) fn adapter(
        &self,
        agent: AgentId,
    ) -> Result<std::sync::Arc<dyn crate::adapters::AgentAdapter>> {
        self.registry.get(agent).ok_or_else(|| {
            AppError::NotFound(format!(
                "no adapter registered for agent {}",
                agent.as_str()
            ))
        })
    }
}
