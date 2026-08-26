//! Provider switch / undo compensation. Do not change this sequence.

use std::time::Instant;

use crate::error::{AppError, Result};
use crate::models::{AgentConfig, AgentId, BackupKind, ProviderSwitchResult};
use crate::services::switch_undo::{
    clear_switch_undo, peek_switch_undo, record_switch_undo, PROVIDER_UNDO_PREFIX,
};

use super::{
    ensure_config_agent, live_config_is_empty, log_provider_op, now_ts, ProviderLiveSagaGuard,
    ProviderService,
};

impl ProviderService {
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

    pub(super) fn switch_locked_inner(
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
}

pub(super) fn compensated_switch_error(
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
