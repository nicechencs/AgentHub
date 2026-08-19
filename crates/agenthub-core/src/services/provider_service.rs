//! Provider pool service — CRUD, import-live, and safe live switching.

use std::path::PathBuf;
use std::time::Instant;

use chrono::Utc;
use uuid::Uuid;

use crate::adapters::AdapterRegistry;
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{
    attach_persisted_surface, AdapterSourceKind, AgentConfig, AgentId, BackupKind, Capability,
    PersistedTicketSurface, Provider, ProviderInput, ProviderSwitchResult, TicketSurface,
};
use crate::services::switch_undo::{
    clear_switch_undo, extract_probe_url, peek_switch_undo, probe_url_latency_ms,
    record_switch_undo, PROVIDER_UNDO_PREFIX,
};
use crate::services::{
    AdapterRouteService, AdapterSecretResolver, BackupService, ConnectionService,
    LiveWriteAuthority, LiveWriteGuard,
};
use crate::storage::{AdapterProfileRepo, Database, ProviderRepo};
use crate::utils::loopback::is_loopback_base_url;
use crate::utils::redact::redact_text;

/// Maximum Unicode scalar values allowed in a provider id.
pub const MAX_PROVIDER_ID_LEN: usize = 128;
/// Maximum Unicode scalar values allowed in a provider name.
pub const MAX_PROVIDER_NAME_LEN: usize = 256;

/// Business facade over [`ProviderRepo`].
#[derive(Clone)]
pub struct ProviderService {
    db: Database,
    repo: ProviderRepo,
    registry: AdapterRegistry,
    backup: Option<BackupService>,
    authority: LiveWriteAuthority,
    connections: ConnectionService,
    secret_resolver: AdapterSecretResolver,
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

/// RAII guard for a cross-boundary, per-agent provider saga.
///
/// Holding this guard retains the same cross-process lock used by ordinary
/// provider switches. Guarded APIs validate both the originating service and
/// target agent, so callers cannot accidentally use a Claude saga guard for a
/// different agent or service.
pub struct ProviderLiveSagaGuard<'a> {
    service: &'a ProviderService,
    agent: AgentId,
    guard: LiveWriteGuard,
}

impl std::fmt::Debug for ProviderLiveSagaGuard<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderLiveSagaGuard")
            .field("agent", &self.agent)
            .finish_non_exhaustive()
    }
}

impl ProviderLiveSagaGuard<'_> {
    pub fn agent(&self) -> AgentId {
        self.agent
    }

    /// Borrow the shared authority proof for another Core live-write service.
    /// This is intended for one larger orchestration saga and avoids nested
    /// acquisition of the same cross-process lock.
    pub fn as_live_write_guard(&self) -> &LiveWriteGuard {
        &self.guard
    }
}

impl ProviderService {
    /// Construct the provider-pool service without live-write orchestration.
    /// CRUD and import-live are available; [`Self::switch`] fails closed until
    /// a backup root is configured through [`Self::with_live`].
    pub fn new(db: Database) -> Self {
        Self::with_registry(db, AdapterRegistry::default())
    }

    /// Inject adapters for tests or callers that only need CRUD/import-live.
    pub fn with_registry(db: Database, registry: AdapterRegistry) -> Self {
        Self {
            db: db.clone(),
            repo: ProviderRepo::new(db.clone()),
            registry,
            backup: None,
            authority: LiveWriteAuthority::from_database(&db),
            connections: ConnectionService::new(db.clone()),
            secret_resolver: AdapterSecretResolver::new(db),
        }
    }

    /// Construct the full live-switch service with explicit shared
    /// dependencies and backup location.
    pub fn with_live(db: Database, registry: AdapterRegistry, backups_root: PathBuf) -> Self {
        Self {
            db: db.clone(),
            repo: ProviderRepo::new(db.clone()),
            backup: Some(BackupService::new(
                db.clone(),
                registry.clone(),
                backups_root,
            )),
            registry,
            authority: LiveWriteAuthority::from_database(&db),
            connections: ConnectionService::new(db.clone()),
            secret_resolver: AdapterSecretResolver::new(db),
        }
    }

    /// Deterministic list: [`AgentId::ALL`] order, then name, then id.
    pub fn list(&self, agent: Option<AgentId>) -> Result<Vec<Provider>> {
        let mut items = self.repo.list(agent)?;
        sort_providers(&mut items);
        Ok(items)
    }

    /// Resolve by primary key id first; otherwise by exact name.
    ///
    /// - Missing → [`AppError::NotFound`]
    /// - Multiple name matches → [`AppError::InvalidArg`] (ambiguous)
    /// - Optional `agent` scopes both id and name lookup
    pub fn get(&self, id_or_name: &str, agent: Option<AgentId>) -> Result<Provider> {
        let key = id_or_name.trim();
        if key.is_empty() {
            return Err(AppError::InvalidArg(
                "provider id or name must not be empty".into(),
            ));
        }

        if let Some(p) = self.repo.get_by_id(key)? {
            if let Some(agent) = agent {
                if p.agent_id != agent {
                    return Err(AppError::NotFound(format!(
                        "provider not found: {key} (agent filter: {})",
                        agent.as_str()
                    )));
                }
            }
            return Ok(p);
        }

        let matches = self.repo.list_by_name(key, agent)?;
        match matches.len() {
            0 => Err(AppError::NotFound(format!("provider not found: {key}"))),
            1 => Ok(matches.into_iter().next().expect("len 1")),
            n => Err(AppError::InvalidArg(format!(
                "ambiguous provider name '{key}': found {n} providers; specify --agent or use id"
            ))),
        }
    }

    /// Create a new provider. Core owns timestamps.
    ///
    /// Duplicate id → [`AppError::InvalidArg`].
    pub fn create(&self, input: &ProviderInput) -> Result<Provider> {
        let started = Instant::now();
        let agent = input.agent_id;
        let result = (|| {
            let guard = self.begin_live_saga(agent)?;
            self.create_with_guard(&guard, input)
        })();
        log_provider_op("create", agent, started, &result);
        result
    }

    /// Create a provider while an existing per-agent saga guard remains held.
    pub fn create_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        input: &ProviderInput,
    ) -> Result<Provider> {
        self.validate_live_saga_guard(guard, input.agent_id)?;
        self.create_inner(input)
    }

    fn create_inner(&self, input: &ProviderInput) -> Result<Provider> {
        validate_provider_input(input)?;
        let now = now_ts();
        let row = Provider {
            id: input.id.clone(),
            agent_id: input.agent_id,
            name: input.name.clone(),
            settings_config: input.settings_config.clone(),
            meta: input.meta.clone(),
            is_current: input.is_current,
            created_at: now.clone(),
            updated_at: now,
        };
        let created = if row.is_current {
            let (created, _binding) = self.connections.create_and_activate_provider(&row)?;
            created
        } else {
            self.repo.create(&row)?
        };
        self.stamp_provider_surface(created)
    }

    /// Update an existing provider by id. Core owns `updated_at`; preserves `created_at`.
    ///
    /// - Missing → [`AppError::NotFound`]
    /// - `agent_id` change → [`AppError::InvalidArg`]
    pub fn update(&self, input: &ProviderInput) -> Result<Provider> {
        let started = Instant::now();
        let agent = input.agent_id;
        let result = (|| {
            let guard = self.begin_live_saga(agent)?;
            self.update_with_guard(&guard, input)
        })();
        log_provider_op("update", agent, started, &result);
        result
    }

    /// Update a provider while an existing per-agent saga guard remains held.
    pub fn update_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        input: &ProviderInput,
    ) -> Result<Provider> {
        self.validate_live_saga_guard(guard, input.agent_id)?;
        self.update_and_snapshot(input)
    }

    fn update_and_snapshot(&self, input: &ProviderInput) -> Result<Provider> {
        let stored = self.update_inner(input)?;
        self.sync_current_provider_live(&stored, "after provider update")?;
        Ok(stored)
    }

    fn update_inner(&self, input: &ProviderInput) -> Result<Provider> {
        validate_provider_input(input)?;
        let row = Provider {
            id: input.id.clone(),
            agent_id: input.agent_id,
            name: input.name.clone(),
            settings_config: input.settings_config.clone(),
            meta: input.meta.clone(),
            is_current: input.is_current,
            // Repo preserves the stored created_at; placeholder only.
            created_at: String::new(),
            updated_at: now_ts(),
        };
        let updated = if row.is_current {
            let (updated, _binding) = self.connections.update_and_activate_provider(&row)?;
            updated
        } else {
            // Demote path: update + clear binding when this row was active.
            self.connections.update_provider_non_current(&row)?
        };
        self.stamp_provider_surface(updated)
    }

    /// Insert or update. On existing rows: preserve `created_at`, reject `agent_id` change.
    pub fn upsert(&self, input: &ProviderInput) -> Result<Provider> {
        let started = Instant::now();
        let agent = input.agent_id;
        let result = (|| {
            let guard = self.begin_live_saga(agent)?;
            self.upsert_with_guard(&guard, input)
        })();
        log_provider_op("upsert", agent, started, &result);
        result
    }

    /// Upsert a provider while an existing per-agent saga guard remains held.
    pub fn upsert_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        input: &ProviderInput,
    ) -> Result<Provider> {
        self.validate_live_saga_guard(guard, input.agent_id)?;
        self.upsert_and_snapshot(input)
    }

    fn upsert_and_snapshot(&self, input: &ProviderInput) -> Result<Provider> {
        let stored = self.upsert_inner(input)?;
        self.sync_current_provider_live(&stored, "after provider upsert")?;
        Ok(stored)
    }

    fn upsert_inner(&self, input: &ProviderInput) -> Result<Provider> {
        validate_provider_input(input)?;
        let now = now_ts();
        let row = Provider {
            id: input.id.clone(),
            agent_id: input.agent_id,
            name: input.name.clone(),
            settings_config: input.settings_config.clone(),
            meta: input.meta.clone(),
            is_current: input.is_current,
            // Used only for the insert path; update path preserves stored created_at.
            created_at: now.clone(),
            updated_at: now,
        };
        if row.is_current {
            let (upserted, _binding) = self.connections.upsert_and_activate_provider(&row)?;
            self.stamp_provider_surface(upserted)
        } else {
            // Demote / plain upsert: clear binding only if it references this id.
            let upserted = self.connections.upsert_provider_non_current(&row)?;
            self.stamp_provider_surface(upserted)
        }
    }

    /// Delete by primary key id.
    ///
    /// - Empty / invalid id → [`AppError::InvalidArg`]
    /// - Missing → [`AppError::NotFound`]
    pub fn delete(&self, id: &str, agent: AgentId) -> Result<()> {
        let started = Instant::now();
        let result = (|| {
            let guard = self.begin_live_saga(agent)?;
            self.delete_with_guard(&guard, id, agent)
        })();
        log_provider_op("delete", agent, started, &result);
        result
    }

    /// Delete a provider while an existing per-agent saga guard remains held.
    pub fn delete_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        id: &str,
        agent: AgentId,
    ) -> Result<()> {
        self.validate_live_saga_guard(guard, agent)?;
        self.delete_inner(id, agent)
    }

    fn delete_inner(&self, id: &str, agent: AgentId) -> Result<()> {
        validate_id(id)?;
        // Clear active binding in the same transaction when deleting the active row.
        self.connections.delete_provider(id, agent)
    }

    /// Acquire the per-agent cross-process lock for an entire provider saga.
    /// The returned guard releases it on drop.
    pub fn begin_live_saga(&self, agent: AgentId) -> Result<ProviderLiveSagaGuard<'_>> {
        let guard = self.acquire_live_lock(agent)?;
        Ok(ProviderLiveSagaGuard {
            service: self,
            agent,
            guard,
        })
    }

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

    /// Restore a live config while an existing saga guard remains held.
    pub fn restore_live_config_snapshot_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        snapshot: &ProviderLiveConfigSnapshot,
    ) -> Result<()> {
        self.validate_live_saga_guard(guard, snapshot.agent)?;
        let adapter = self.adapter(snapshot.agent)?;
        adapter.write_config(&snapshot.config)
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

    fn import_live_inner(&self, agent: AgentId, name: Option<&str>) -> Result<Provider> {
        let _lock = self.acquire_live_lock(agent)?;
        let adapter = self.adapter(agent)?;
        let live = adapter.read_config()?;
        ensure_config_agent(&live, agent)?;
        if live_config_is_empty(&live.raw) {
            return Err(AppError::NotFound(format!(
                "no live provider config found for agent {}",
                agent.as_str()
            )));
        }

        let display_name = name
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Imported {}", now_ts()));
        validate_name(&display_name)?;

        // A live config is a single canonical snapshot per agent. Reuse the
        // existing live-import row instead of creating a UUID row on every
        // refresh. Manual providers are deliberately ignored: only rows whose
        // metadata explicitly identifies `source = live` participate here.
        let saved = if let Some(existing) = self.find_live_import(agent)? {
            let desired_name = name.unwrap_or(&existing.name);
            if existing.settings_config == live.raw
                && existing.name == desired_name
                && existing.is_current
            {
                self.stamp_provider_surface(existing)?
            } else {
                let input = ProviderInput {
                    id: existing.id,
                    agent_id: agent,
                    name: desired_name.to_owned(),
                    settings_config: live.raw,
                    meta: existing.meta,
                    is_current: true,
                };
                let updated = self.update_inner(&input)?;
                self.stamp_provider_surface(updated)?
            }
        } else {
            let input = ProviderInput {
                id: format!("{}-live-{}", agent.as_str(), Uuid::new_v4()),
                agent_id: agent,
                name: display_name,
                settings_config: live.raw,
                meta: serde_json::json!({ "source": "live" }),
                is_current: true,
            };
            // Use inner create so import is a single log op (not create + import).
            let created = self.create_inner(&input)?;
            self.stamp_provider_surface(created)?
        };
        self.collapse_extra_loopback_providers(agent, &saved)?;
        Ok(saved)
    }

    /// After upserting the canonical live row, drop leftover same-agent
    /// loopback providers. Adapter-generated projections stay hidden and are
    /// not Connections tickets; remote-URL rows are left untouched.
    fn collapse_extra_loopback_providers(&self, agent: AgentId, kept: &Provider) -> Result<()> {
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

    /// Classify the persisted row and write `meta.surface` before upsert /
    /// import_live returns. `classify_source_product` reads the stored row.
    ///
    /// Adapter-generated projections are not tickets: skip them even when
    /// classify would return `unknown`.
    fn stamp_provider_surface(&self, provider: Provider) -> Result<Provider> {
        if self.is_generated_projection(&provider)? {
            return Ok(provider);
        }
        let product = AdapterRouteService::new(self.db.clone())
            .classify_source_product(AdapterSourceKind::Provider, &provider.id)?;
        let surface = TicketSurface::from_product(product);
        if TicketSurface::from_persisted_json(&provider.meta)
            == PersistedTicketSurface::Known(surface)
        {
            return Ok(provider);
        }
        let expected_updated_at = provider.updated_at.clone();
        let mut stamped = provider;
        attach_persisted_surface(&mut stamped.meta, surface);
        stamped.updated_at = now_ts();
        self.repo
            .update_healed_fields(&stamped, &expected_updated_at, &stamped.updated_at)
    }

    /// Projections are not tickets. Match `generatedBy=adapter` or an existing
    /// profile that already points at this row as `generated_provider_id`.
    fn is_generated_projection(&self, provider: &Provider) -> Result<bool> {
        if provider
            .meta
            .get("generatedBy")
            .and_then(|value| value.as_str())
            == Some("adapter")
        {
            return Ok(true);
        }
        Ok(AdapterProfileRepo::new(self.db.clone())
            .list_filtered(&Default::default())?
            .iter()
            .any(|profile| profile.generated_provider_id.as_deref() == Some(provider.id.as_str())))
    }

    /// Locate the canonical live-import row for one agent. Older databases may
    /// contain more than one duplicate; choose deterministically and leave
    /// the other historical rows untouched rather than deleting user data.
    fn find_live_import(&self, agent: AgentId) -> Result<Option<Provider>> {
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

    /// Apply a saved provider to the live agent config.
    ///
    /// Order is fixed: validate/lock -> read live -> backfill old current ->
    /// snapshot -> atomic adapter write -> select target in the DB. Backup,
    /// apply, and final DB failures compensate earlier DB/live changes. A
    /// rollback-specific error reports compensation failure using error codes
    /// only, so adapter messages cannot expose provider secrets.
    pub fn switch(&self, id_or_name: &str, agent: AgentId) -> Result<ProviderSwitchResult> {
        let guard = self.begin_live_saga(agent)?;
        self.switch_with_guard(&guard, id_or_name, agent)
    }

    /// Apply a provider while an existing saga guard remains held.
    pub fn switch_with_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        id_or_name: &str,
        agent: AgentId,
    ) -> Result<ProviderSwitchResult> {
        let started = Instant::now();
        let result = (|| {
            self.validate_live_saga_guard(guard, agent)?;
            self.switch_locked_inner(id_or_name, agent)
        })();
        log_provider_op("switch", agent, started, &result);
        result
    }

    fn switch_locked_inner(
        &self,
        id_or_name: &str,
        agent: AgentId,
    ) -> Result<ProviderSwitchResult> {
        let backup = self.backup.as_ref().ok_or_else(|| {
            AppError::Unsupported(
                "provider live switching requires an explicitly configured backup root".into(),
            )
        })?;

        let target = self.get(id_or_name, Some(agent))?;
        let target_is_reference = self.secret_resolver.is_reference_provider(&target)?;
        let materialized_target = if target_is_reference {
            Some(self.secret_resolver.materialize_for_live(&target)?)
        } else {
            None
        };
        let adapter = self.adapter(agent)?;
        let live_before = adapter.read_config()?;
        ensure_config_agent(&live_before, agent)?;
        let current = self.repo.get_current(agent)?;
        let previous_current_id = current.as_ref().map(|provider| provider.id.clone());

        let live_for_backfill = if live_config_is_empty(&live_before.raw) {
            None
        } else if let Some(current) = current.as_ref() {
            Some(
                self.secret_resolver
                    .scrub_for_backfill(current, &live_before.raw)?,
            )
        } else {
            Some(live_before.raw.clone())
        };
        let backfilled_provider_id = current
            .as_ref()
            .filter(|_| live_for_backfill.is_some())
            .map(|provider| provider.id.clone());

        // If the selected row is already current, its backfilled live value is
        // authoritative and must not immediately be overwritten by stale L1.
        let target_raw = if let Some(target) = materialized_target {
            // Do not reuse live state for a generated reference: source key
            // rotation must take effect even when this row is already current.
            target.settings_config
        } else {
            match (&current, &live_for_backfill) {
                (Some(current), Some(raw)) if current.id == target.id => raw.clone(),
                _ => target.settings_config.clone(),
            }
        };
        let target_config = AgentConfig {
            agent,
            raw: target_raw,
        };

        // Persist the complete live value first. Later stages compensate this
        // row on failure, giving a deterministic write sequence:
        // backfill -> backup -> live apply -> final DB selection.
        let backfilled = match (&current, &live_for_backfill) {
            (Some(current), Some(raw)) => {
                Some(self.repo.backfill_current(current, raw, &now_ts())?)
            }
            _ => None,
        };
        let rollback_backfill = || match (&current, &backfilled) {
            (Some(original), Some(applied)) => self
                .repo
                .restore_backfill(original, &applied.updated_at)
                .err(),
            _ => None,
        };
        let expected_target_updated_at = backfilled
            .as_ref()
            .filter(|row| row.id == target.id)
            .map_or(target.updated_at.as_str(), |row| row.updated_at.as_str());

        let snapshot = match backup.snapshot(
            agent,
            BackupKind::AutoSwitch,
            Some(&format!("before provider switch to {}", target.id)),
        ) {
            Ok(record) => Some(record),
            Err(error) if error.code() == "not_found" => None,
            Err(error) => {
                let db_rollback = rollback_backfill();
                return Err(compensated_switch_error(error, None, db_rollback));
            }
        };

        if let Err(error) = adapter.write_config(&target_config) {
            let live_rollback = adapter.write_config(&live_before).err();
            let db_rollback = rollback_backfill();
            return Err(compensated_switch_error(error, live_rollback, db_rollback));
        }
        let now = now_ts();
        // Single transaction: is_current + demote accounts + binding (B1 cleanup).
        let provider = match self.connections.activate_provider(
            agent,
            &target.id,
            expected_target_updated_at,
            &now,
        ) {
            Ok((provider, _binding)) => provider,
            Err(error) => {
                let live_rollback = adapter.write_config(&live_before).err();
                let db_rollback = rollback_backfill();
                return Err(compensated_switch_error(error, live_rollback, db_rollback));
            }
        };

        if let Some(from_id) = previous_current_id {
            if from_id != provider.id {
                record_switch_undo(
                    &self.db,
                    PROVIDER_UNDO_PREFIX,
                    agent,
                    &from_id,
                    &provider.id,
                )?;
            } else {
                clear_switch_undo(&self.db, PROVIDER_UNDO_PREFIX, agent)?;
            }
        } else {
            clear_switch_undo(&self.db, PROVIDER_UNDO_PREFIX, agent)?;
        }

        Ok(ProviderSwitchResult {
            provider,
            backup: snapshot,
            backfilled_provider_id,
        })
    }

    /// Re-apply the previous provider after a successful [`Self::switch`], if recorded.
    ///
    /// Returns `false` when there is no undo target (never switched, or already undone).
    pub fn undo_switch(&self, agent: AgentId) -> Result<bool> {
        let Some(from_id) = peek_switch_undo(&self.db, PROVIDER_UNDO_PREFIX, agent)? else {
            return Ok(false);
        };
        // Re-switch writes a reverse undo slot; clear afterward for one-shot toast UX.
        self.switch(&from_id, agent)?;
        clear_switch_undo(&self.db, PROVIDER_UNDO_PREFIX, agent)?;
        Ok(true)
    }

    /// Best-effort TCP/HTTP reachability probe of a saved provider base URL.
    ///
    /// Returns round-trip milliseconds when the endpoint answers (any HTTP status
    /// counts as reachable). Missing URL or network failure → error.
    pub fn test_latency(&self, agent: AgentId, provider_id: &str) -> Result<u64> {
        let provider = self.get(provider_id, Some(agent))?;
        let url = extract_probe_url(&provider.settings_config).ok_or_else(|| {
            AppError::InvalidArg(
                "该连接没有可探测的 Base URL（base_url / ANTHROPIC_BASE_URL 等）".into(),
            )
        })?;
        probe_url_latency_ms(&url)
    }

    /// Storage access for tests / future write paths (not used by list/show CLI).
    pub fn repo(&self) -> &ProviderRepo {
        &self.repo
    }

    /// After a pool save: if this row is the active connection, write the
    /// **new stored value** to live files. This must not reuse [`Self::switch`],
    /// which treats the existing live file as authoritative for the current row.
    ///
    /// Non-current rows stay pool-only. Missing backup root / writer capability
    /// is a no-op so CRUD tests and half-surface agents do not touch real files.
    fn sync_current_provider_live(&self, stored: &Provider, note: &str) -> Result<()> {
        if !stored.is_current {
            self.snapshot_after_pool_change(stored.agent_id, note);
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
            self.snapshot_after_pool_change(stored.agent_id, note);
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
            self.snapshot_after_pool_change(stored.agent_id, note);
            return Ok(());
        }

        if let Err(error) = backup.snapshot(
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
        Ok(())
    }

    /// Pool-only changes keep an audit snapshot of the untouched live file.
    /// A missing live file is a normal no-op.
    fn snapshot_after_pool_change(&self, agent: AgentId, note: &str) {
        let Some(backup) = self.backup.as_ref() else {
            return;
        };
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

    fn adapter(&self, agent: AgentId) -> Result<std::sync::Arc<dyn crate::adapters::AgentAdapter>> {
        self.registry.get(agent).ok_or_else(|| {
            AppError::NotFound(format!(
                "no adapter registered for agent {}",
                agent.as_str()
            ))
        })
    }

    fn acquire_live_lock(&self, agent: AgentId) -> Result<LiveWriteGuard> {
        self.authority.acquire(agent)
    }

    fn validate_live_saga_guard(
        &self,
        guard: &ProviderLiveSagaGuard<'_>,
        agent: AgentId,
    ) -> Result<()> {
        if !std::ptr::eq(self, guard.service) || guard.agent != agent {
            return Err(AppError::InvalidArg(
                "provider live saga guard does not match this service and agent".into(),
            ));
        }
        self.authority.validate_guard(&guard.guard, agent)
    }
}

fn log_provider_op<T>(op: &str, agent: AgentId, started: Instant, result: &Result<T>) {
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    match result {
        Ok(_) => {
            let msg = match op {
                "switch" => "switched provider",
                "delete" => "deleted provider",
                "create" => "created provider",
                "update" => "updated provider",
                "upsert" => "upserted provider",
                "import" => "imported provider",
                _ => "ok",
            };
            tracing::info!(
                module = targets::PROVIDER,
                op,
                agent = agent.as_str(),
                elapsed_ms,
                "{msg}"
            );
        }
        Err(err) => {
            let msg = redact_text(&err.to_string());
            tracing::error!(
                module = targets::PROVIDER,
                op,
                agent = agent.as_str(),
                code = err.code(),
                elapsed_ms,
                "{msg}"
            );
        }
    }
}

fn compensated_current_apply_error(primary: AppError, live_rollback: Option<AppError>) -> AppError {
    let Some(rollback) = live_rollback else {
        return primary;
    };
    AppError::message(
        "provider.current.apply.rollback",
        format!(
            "applying the current provider failed [{}]; compensation status: live={}",
            primary.code(),
            rollback.code()
        ),
    )
}

fn compensated_switch_error(
    primary: AppError,
    live_rollback: Option<AppError>,
    db_rollback: Option<AppError>,
) -> AppError {
    if live_rollback.is_none() && db_rollback.is_none() {
        return primary;
    }
    let live = live_rollback.as_ref().map_or("ok", AppError::code);
    let database = db_rollback.as_ref().map_or("ok", AppError::code);
    AppError::message(
        "provider.switch.rollback",
        format!(
            "provider switch failed [{}]; compensation status: live={live}, database={database}",
            primary.code()
        ),
    )
}

fn now_ts() -> String {
    Utc::now().format("%Y-%m-%d %H:%M:%S%.6f").to_string()
}

fn ensure_config_agent(config: &AgentConfig, expected: AgentId) -> Result<()> {
    if config.agent != expected {
        return Err(AppError::InvalidArg(format!(
            "adapter returned config for {}, expected {}",
            config.agent.as_str(),
            expected.as_str()
        )));
    }
    require_json_object(&config.raw, "live settings_config")
}

fn live_config_is_empty(raw: &serde_json::Value) -> bool {
    let Some(object) = raw.as_object() else {
        return false;
    };
    object.is_empty()
        || (object.get("format").and_then(|value| value.as_str()) == Some("toml")
            && object
                .get("content")
                .and_then(|value| value.as_str())
                .is_some_and(str::is_empty))
}

fn validate_provider_input(input: &ProviderInput) -> Result<()> {
    validate_id(&input.id)?;
    validate_name(&input.name)?;
    require_json_object(&input.settings_config, "settings_config")?;
    require_json_object(&input.meta, "meta")?;
    Ok(())
}

fn validate_id(id: &str) -> Result<()> {
    validate_label(id, "provider id", MAX_PROVIDER_ID_LEN)
}

fn validate_name(name: &str) -> Result<()> {
    validate_label(name, "provider name", MAX_PROVIDER_NAME_LEN)
}

fn validate_label(value: &str, field: &str, max_chars: usize) -> Result<()> {
    if value.is_empty() {
        return Err(AppError::InvalidArg(format!("{field} must not be empty")));
    }
    if value != value.trim() {
        return Err(AppError::InvalidArg(format!(
            "{field} must not have surrounding whitespace"
        )));
    }
    if value.chars().count() > max_chars {
        return Err(AppError::InvalidArg(format!(
            "{field} exceeds maximum length of {max_chars} characters"
        )));
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(AppError::InvalidArg(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(())
}

fn require_json_object(value: &serde_json::Value, field: &str) -> Result<()> {
    if !value.is_object() {
        return Err(AppError::InvalidArg(format!(
            "{field} must be a JSON object"
        )));
    }
    Ok(())
}

fn agent_rank(id: AgentId) -> usize {
    AgentId::ALL
        .iter()
        .position(|a| *a == id)
        .unwrap_or(usize::MAX)
}

fn sort_providers(items: &mut [Provider]) {
    items.sort_by(|a, b| {
        agent_rank(a.agent_id)
            .cmp(&agent_rank(b.agent_id))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
}

#[cfg(test)]
mod tests;
