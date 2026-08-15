use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::adapters::{AdapterRegistry, AgentAdapter};
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{
    attach_persisted_surface, Account, AccountInput, AccountKind, AccountSwitchResult,
    AdapterSourceKind, AgentId, BackupKind, Capability, LiveAccount, PersistedTicketSurface,
    TicketSurface,
};
use crate::services::switch_undo::{
    clear_switch_undo, peek_switch_undo, record_switch_undo, ACCOUNT_UNDO_PREFIX,
};
use crate::services::{AdapterRouteService, BackupService, ConnectionService};
use crate::storage::{AccountRepo, Database};
use crate::utils::agent_lock::AgentWriteLock;
use crate::utils::redact::mask_secret_preview;

use super::surface::*;
use super::{AccountService, MAX_ACCOUNT_ID_LEN, MAX_ACCOUNT_LABEL_LEN};

impl AccountService {
    pub fn switch(&self, id_or_label: &str, agent: AgentId) -> Result<AccountSwitchResult> {
        let started = Instant::now();
        let result = self.switch_inner(id_or_label, agent);
        log_account_op("switch", agent, started, &result);
        result
    }

    pub(super) fn switch_inner(&self, id_or_label: &str, agent: AgentId) -> Result<AccountSwitchResult> {
        let backup = self.backup.as_ref().ok_or_else(|| {
            AppError::Unsupported(
                "account live switching requires an explicitly configured backup root".into(),
            )
        })?;
        let process_lock = live_reconcile_lock(agent);
        let _process_lock = process_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _lock = self.acquire_live_lock(agent)?.ok_or_else(|| {
            AppError::Unsupported("account live switching is not configured".into())
        })?;

        let adapter = self.registry.require(agent, Capability::AccountSwitch)?;

        let mut target = self.get(id_or_label, Some(agent))?;
        // The live credentials are only accepted when their file revision is
        // unchanged across `revision-before -> read_account -> revision-after`.
        // A bounded retry absorbs an in-progress CLI atomic write without ever
        // backfilling from a torn snapshot.
        let (live_before, revision) = capture_stable_live_snapshot(adapter.as_ref(), 2)?;

        // A Pi LiveAccount here is deliberately the full auth.json snapshot:
        // it is safe for backup/rollback, but never safe to reconcile into a
        // single pool row. Provider reconciliation happens in list/sync only.
        if agent != AgentId::Pi {
            if let Some(live) = live_before
                .as_ref()
                .filter(|live| !live_account_is_empty(live))
            {
                self.validate_live_switch_identity(adapter.as_ref(), agent, live)?;
                self.reconcile_live_account(adapter.as_ref(), agent, live.clone())?;
                target = self.get(id_or_label, Some(agent))?;
            }
        }

        let current = self.repo.get_current(agent)?;
        let previous_current_id = current.as_ref().map(|account| account.id.clone());

        let live_for_backfill = if agent == AgentId::Pi {
            None
        } else {
            live_before
                .as_ref()
                .filter(|live| !live_account_is_empty(live))
                .filter(|live| {
                    current.as_ref().is_some_and(|current| {
                        accounts_same_authorization(
                            adapter.as_ref(),
                            live.kind,
                            &live.credentials,
                            current,
                        ) || stable_live_identity(
                            adapter.as_ref(),
                            current.kind,
                            &current.credentials,
                        )
                        .zip(stable_live_identity(
                            adapter.as_ref(),
                            live.kind,
                            &live.credentials,
                        ))
                        .is_some_and(|(current, live)| current == live)
                    })
                })
        };
        let backfilled_account_id = current
            .as_ref()
            .filter(|_| live_for_backfill.is_some())
            .map(|a| a.id.clone());

        let apply_live = match (&current, live_for_backfill) {
            (Some(cur), Some(live)) if cur.id == target.id => live.clone(),
            _ => target.to_live(),
        };

        let backfilled = match (&current, live_for_backfill) {
            (Some(cur), Some(live)) => Some(self.repo.backfill_current(
                cur,
                &live.credentials,
                &now_ts(),
            )?),
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
            Some(&format!("before account switch to {}", target.id)),
        ) {
            Ok(record) => Some(record),
            Err(error) if error.code() == "not_found" => None,
            Err(error) => {
                let db_rollback = rollback_backfill();
                return Err(compensated_switch_error(error, None, db_rollback));
            }
        };

        // Snapshotting can itself take long enough for a CLI to refresh the
        // file. Check the opaque revision after backup and immediately before
        // apply; on conflict report any failed DB compensation rather than
        // silently discarding it.
        if let Some(observed_revision) = revision.as_deref() {
            if probe_auth_revision(adapter.as_ref()).as_deref() != Some(observed_revision) {
                let db_rollback = rollback_backfill();
                return Err(compensated_switch_error(
                    live_revision_conflict(),
                    None,
                    db_rollback,
                ));
            }
        }

        if let Err(error) = adapter.apply_account(&apply_live) {
            let live_rollback = match &live_before {
                Some(before) => adapter.apply_account(before).err(),
                None => None,
            };
            let db_rollback = rollback_backfill();
            return Err(compensated_switch_error(error, live_rollback, db_rollback));
        }

        let now = now_ts();
        // Single transaction: is_current + demote providers + binding (B1 cleanup).
        let account = match self.connections.activate_account(
            agent,
            &target.id,
            expected_target_updated_at,
            &now,
        ) {
            Ok((account, _binding)) => account,
            Err(error) => {
                let live_rollback = match &live_before {
                    Some(before) => adapter.apply_account(before).err(),
                    None => None,
                };
                let db_rollback = rollback_backfill();
                return Err(compensated_switch_error(error, live_rollback, db_rollback));
            }
        };

        if let Some(from_id) = previous_current_id {
            if from_id != account.id {
                record_switch_undo(&self.db, ACCOUNT_UNDO_PREFIX, agent, &from_id, &account.id)?;
            } else {
                clear_switch_undo(&self.db, ACCOUNT_UNDO_PREFIX, agent)?;
            }
        } else {
            clear_switch_undo(&self.db, ACCOUNT_UNDO_PREFIX, agent)?;
        }

        Ok(AccountSwitchResult {
            account,
            backup: snapshot,
            backfilled_account_id,
        })
    }

    /// Re-apply the previous account after a successful [`Self::switch`], if recorded.
    pub fn undo_switch(&self, agent: AgentId) -> Result<bool> {
        let Some(from_id) = peek_switch_undo(&self.db, ACCOUNT_UNDO_PREFIX, agent)? else {
            return Ok(false);
        };
        self.switch(&from_id, agent)?;
        clear_switch_undo(&self.db, ACCOUNT_UNDO_PREFIX, agent)?;
        Ok(true)
    }

    pub fn persist_pi_oauth_live(&self, live: LiveAccount, label: String) -> Result<Account> {
        if live.agent != AgentId::Pi || live.kind != AccountKind::Oauth {
            return Err(AppError::InvalidArg(
                "Pi OAuth mutation requires a Pi OAuth live account".into(),
            ));
        }
        let process_lock = live_reconcile_lock(AgentId::Pi);
        let _process_lock = process_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _file_lock = self.acquire_live_lock(AgentId::Pi)?.ok_or_else(|| {
            AppError::Unsupported("Pi OAuth mutation requires a configured lock directory".into())
        })?;

        let path = crate::adapters::pi_auth::pi_auth_path()?;
        let original = read_optional_file(&path)?;
        let patch = live
            .credentials
            .get("body")
            .cloned()
            .ok_or_else(|| AppError::message("oauth.device", "missing Pi auth body"))?;
        let merged = crate::adapters::pi_auth::merge_auth_json(&patch)?;
        let mut bytes = serde_json::to_vec_pretty(&merged)?;
        bytes.push(b'\n');
        crate::utils::atomic::atomic_write(&path, &bytes)?;

        let result = self.create(AccountInput {
            agent_id: AgentId::Pi,
            kind: AccountKind::Oauth,
            label,
            credentials: live.credentials,
            extra: live.extra,
            is_current: false,
        });
        if let Err(error) = result {
            let rollback = match original {
                Some(previous) => crate::utils::atomic::atomic_write(&path, &previous).err(),
                None => std::fs::remove_file(&path).err().map(AppError::from),
            };
            if let Some(rollback) = rollback {
                return Err(AppError::message(
                    "oauth.device.rollback",
                    format!(
                        "Pi OAuth pool mutation failed ({}); file rollback failed ({})",
                        error.code(),
                        rollback
                    ),
                ));
            }
            return Err(error);
        }
        result
    }

    /// Persist a refreshed Pi OAuth row and its provider entry under the shared
    /// process/file lock. A DB conflict restores the exact auth.json bytes.
    pub fn persist_pi_oauth_account_update(
        &self,
        account: &Account,
        expected_updated_at: &str,
    ) -> Result<Account> {
        if account.agent_id != AgentId::Pi || account.kind != AccountKind::Oauth {
            return Err(AppError::InvalidArg(
                "Pi OAuth mutation requires a Pi OAuth account".into(),
            ));
        }
        let process_lock = live_reconcile_lock(AgentId::Pi);
        let _process_lock = process_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _file_lock = self.acquire_live_lock(AgentId::Pi)?.ok_or_else(|| {
            AppError::Unsupported("Pi OAuth mutation requires a configured lock directory".into())
        })?;

        let path = crate::adapters::pi_auth::pi_auth_path()?;
        let original = read_optional_file(&path)?;
        let patch = account
            .credentials
            .get("body")
            .cloned()
            .ok_or_else(|| AppError::message("oauth.refresh", "missing Pi auth body"))?;
        let merged = crate::adapters::pi_auth::merge_auth_json(&patch)?;
        let mut bytes = serde_json::to_vec_pretty(&merged)?;
        bytes.push(b'\n');
        crate::utils::atomic::atomic_write(&path, &bytes)?;

        let updated_at = now_ts();
        match self
            .repo
            .update_healed_fields(account, expected_updated_at, &updated_at)
        {
            Ok(updated) => Ok(updated),
            Err(error) => {
                let rollback = match original {
                    Some(previous) => crate::utils::atomic::atomic_write(&path, &previous).err(),
                    None => std::fs::remove_file(&path).err().map(AppError::from),
                };
                if let Some(rollback) = rollback {
                    return Err(AppError::message(
                        "oauth.refresh.rollback",
                        format!(
                            "Pi OAuth DB update failed ({}); file rollback failed ({})",
                            error.code(),
                            rollback
                        ),
                    ));
                }
                Err(error)
            }
        }
    }
}
