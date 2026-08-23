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
    AdapterSourceKind, AgentId, BackupKind, Capability, LiveAccount,
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
    pub(super) fn sync_current_live(&self, agent: Option<AgentId>) {
        let adapters = match agent {
            Some(id) => self.registry.get(id).into_iter().collect::<Vec<_>>(),
            None => self.registry.all(),
        };
        for adapter in adapters {
            let id = adapter.id();
            if !adapter.capability(Capability::AccountSwitch).is_usable() {
                continue;
            }
            // Keep the snapshot, match, and persistence decision serialized with
            // account switches. The process-local guard is still required for
            // CRUD-only services that have no file-lock directory configured.
            let process_lock = live_reconcile_lock(id);
            let _process_lock = process_lock
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let _file_lock = match self.acquire_live_lock(id) {
                Ok(lock) => lock,
                Err(error) => {
                    tracing::debug!(
                        module = targets::ACCOUNT,
                        agent = id.as_str(),
                        error_code = error.code(),
                        "live account sync skipped because the live lock is held"
                    );
                    continue;
                }
            };
            let lives = match self.read_live_accounts(adapter.as_ref(), id) {
                Ok(lives) => lives,
                Err(error) if error.code() == "not_found" || error.code() == "unsupported" => {
                    continue;
                }
                Err(error) => {
                    tracing::debug!(
                        module = targets::ACCOUNT,
                        agent = id.as_str(),
                        error_code = error.code(),
                        "live account sync skipped"
                    );
                    continue;
                }
            };
            for live in lives {
                if self.leftover_live_skips_identity(id) {
                    continue;
                }
                if let Err(error) = self.reconcile_live_account(adapter.as_ref(), id, live) {
                    tracing::warn!(
                        module = targets::ACCOUNT,
                        agent = id.as_str(),
                        error_code = error.code(),
                        "failed to persist live account rotation"
                    );
                }
            }
        }
    }

    /// Read the live account slots represented by an adapter snapshot. Pi's
    /// auth.json is a combined file snapshot, so it must be expanded before it
    /// reaches pool reconciliation; the combined snapshot is only safe for
    /// backup / complete-file rollback.
    pub(super) fn read_live_accounts(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
    ) -> Result<Vec<LiveAccount>> {
        let snapshot = adapter.read_account()?;
        if snapshot.agent != agent {
            return Err(AppError::InvalidArg(format!(
                "adapter returned account for {}, expected {}",
                snapshot.agent.as_str(),
                agent.as_str()
            )));
        }
        if live_account_is_empty(&snapshot) {
            return Err(AppError::NotFound(
                "no live account credentials found".into(),
            ));
        }
        if agent != AgentId::Pi {
            return Ok(vec![snapshot]);
        }

        let body = snapshot.credentials.get("body").ok_or_else(|| {
            AppError::InvalidArg("Pi combined live account is missing credentials.body".into())
        })?;
        crate::adapters::pi_auth::expand_auth_to_live_accounts(body)
    }

    /// Reconcile one safe live snapshot into the account pool.
    ///
    /// Exact tokens match first. Same-agent OAuth identity overwrites one row
    /// and collapses leftover same-identity rows. Unknown identity stays
    /// fail-closed except for identical credentials.
    pub(super) fn reconcile_live_account(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        live: LiveAccount,
    ) -> Result<Option<Account>> {
        if live.agent != agent || live_account_is_empty(&live) {
            return Ok(None);
        }
        // Leftover-shaped Codex live is 本机路由 config, not an official grant.
        // Do not import or re-promote it after 官方登录 activate.
        if leftover_shaped_codex_live(agent) {
            return Ok(None);
        }
        let rows = self.repo.list(Some(agent))?;

        let matches = authorization_duplicates(adapter, agent, live.kind, &live.credentials, &rows);
        if let Some(existing) = pick_primary_authorization_match(matches.clone()) {
            let (row, changed) = self.update_live_row(adapter, existing, live);
            if matches.len() > 1 {
                let mark_current = agent != AgentId::Pi;
                return self
                    .commit_authorization_merge(
                        adapter,
                        &row,
                        row.kind,
                        row.label.clone(),
                        row.credentials.clone(),
                        row.extra.clone(),
                        mark_current,
                    )
                    .map(|committed| Some(committed.stored))
                    .map_err(|error| error.into_error());
            }
            return Ok(Some(self.persist_reconciled_live_row(agent, row, changed)?));
        }

        if stable_live_identity(adapter, live.kind, &live.credentials).is_none() {
            // API-key / file snapshots often have no email/sub. Exact
            // authorization already matched above; anything else stays
            // fail-closed instead of inventing a pool row.
            tracing::debug!(
                module = targets::ACCOUNT,
                agent = agent.as_str(),
                "live account identity is unknown; refusing non-exact reconcile"
            );
            return Ok(None);
        }

        // New identity on this agent (Pi: this live slot). Make it current for
        // single-current agents; Pi provider slots never take a global current.
        let label = live
            .label_hint
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("{} · OAuth", agent.display_name()));
        validate_label(&label, "account label", MAX_ACCOUNT_LABEL_LEN)?;
        let mut extra = live.extra.clone();
        if let Some(obj) = extra.as_object_mut() {
            obj.insert("source".into(), json!("live"));
        }
        let extra = attach_identity_meta(adapter, live.kind, &live.credentials, &label, extra);
        let now = now_ts();
        let row = Account {
            id: format!("{}-live-{}", agent.as_str(), Uuid::new_v4()),
            agent_id: agent,
            kind: live.kind,
            label: label.clone(),
            credentials: live.credentials.clone(),
            extra: extra.clone(),
            status: "active".into(),
            is_current: agent != AgentId::Pi,
            created_at: now.clone(),
            updated_at: now,
        };
        let row = self.prepare_account_surface(row);
        let created = self
            .commit_authorization_merge(
                adapter,
                &row,
                live.kind,
                label,
                live.credentials,
                extra,
                row.is_current,
            )
            .map(|committed| committed.stored)
            .map_err(|error| error.into_error())?;
        Ok(Some(created))
    }

    pub(super) fn update_live_row(
        &self,
        adapter: &dyn AgentAdapter,
        mut row: Account,
        live: LiveAccount,
    ) -> (Account, bool) {
        if !live_credentials_changed(&row, &live) {
            return (row, false);
        }
        let persisted_extra = row.extra.clone();
        let display =
            if row.kind == AccountKind::Oauth && is_generic_oauth_label(&row.label, row.agent_id) {
                live.label_hint
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| row.label.clone())
            } else {
                row.label.clone()
            };
        let mut extra = live.extra.clone();
        if let Some(obj) = extra.as_object_mut() {
            obj.insert("source".into(), json!("live"));
        }
        row.label = display.clone();
        row.credentials = live.credentials;
        row.extra = attach_identity_meta(adapter, live.kind, &row.credentials, &display, extra);
        Self::copy_persisted_surface(&persisted_extra, &mut row.extra);
        row.kind = live.kind;
        row.status = "active".into();
        let _ = crate::services::account_identity_heal::heal_account_identity(&mut row);
        let _ = crate::services::account_quota::heal_token_expiry(&mut row);
        row = self.prepare_account_surface(row);
        (row, true)
    }

    /// Persist identity/quota heals. A CAS conflict means another list/heal
    /// already wrote; reload that row instead of warning on every GUI poll.
    pub(super) fn persist_healed_fields(
        &self,
        account: &Account,
        expected_updated_at: &str,
    ) -> Result<Account> {
        let updated_at = now_ts();
        match self
            .repo
            .update_healed_fields(account, expected_updated_at, &updated_at)
        {
            Ok(updated) => Ok(updated),
            Err(error) if error.code() == "account.conflict" => {
                tracing::debug!(
                    module = targets::ACCOUNT,
                    account_id = %account.id,
                    agent = account.agent_id.as_str(),
                    "heal lost the race; using latest row"
                );
                self.repo
                    .get_by_id(&account.id)?
                    .ok_or_else(|| AppError::NotFound(format!("account not found: {}", account.id)))
            }
            Err(error) => Err(error),
        }
    }

    /// Persist a reconciled row and, for single-current agents, atomically
    /// align both the legacy current flag and the active connection binding.
    /// Pi provider slots are concurrent entries in one auth.json, so they must
    /// never be globally activated by reconciliation.
    pub(super) fn persist_reconciled_live_row(
        &self,
        agent: AgentId,
        row: Account,
        changed: bool,
    ) -> Result<Account> {
        let original = row.clone();
        let expected_updated_at = row.updated_at.clone();
        let original_extra = row.extra.clone();
        let mut row = self.prepare_account_surface(row);
        let surface_changed = row.extra != original_extra;
        if surface_changed && !changed {
            return match self.stamp_account_surface(original) {
                Ok(account) => Ok(account),
                Err(error) if error.code() == "account.conflict" => self
                    .repo
                    .get_by_id(&row.id)?
                    .ok_or_else(|| AppError::NotFound(format!("account not found: {}", row.id))),
                Err(error) => Err(error),
            };
        }
        if agent == AgentId::Pi {
            return if changed {
                self.persist_healed_fields(&row, &expected_updated_at)
            } else {
                Ok(row)
            };
        }
        if !changed && row.is_current {
            return Ok(row);
        }
        if leftover_shaped_codex_live(agent) && !row.is_current {
            return if changed {
                self.persist_healed_fields(&row, &expected_updated_at)
            } else {
                Ok(row)
            };
        }
        row.is_current = true;
        row.updated_at = now_ts();
        match self
            .connections
            .update_and_activate_account(&row, &expected_updated_at)
        {
            Ok((updated, _)) => Ok(updated),
            Err(error)
                if error.code() == "account.merge.conflict"
                    || error.code() == "account.conflict" =>
            {
                self.repo
                    .get_by_id(&row.id)?
                    .ok_or_else(|| AppError::NotFound(format!("account not found: {}", row.id)))
            }
            Err(error) => Err(error),
        }
    }

    /// Add a transient, desensitized AuthState view to the current pool row.
    /// This intentionally runs after all persistence/healing and does not call
    /// AccountRepo::update, keeping live state separate from the account pool.
    pub(super) fn merge_live_auth_state(&self, items: &mut [Account], agent: Option<AgentId>) {
        let adapters = match agent {
            Some(id) => self.registry.get(id).into_iter().collect::<Vec<_>>(),
            None => self.registry.all(),
        };
        for adapter in adapters {
            let id = adapter.id();
            let Ok(state) = adapter.read_auth() else {
                continue;
            };
            if state.agent != id {
                continue;
            }
            let Ok(lives) = self.read_live_accounts(adapter.as_ref(), id) else {
                continue;
            };
            let Some(current) = items.iter_mut().find(|row| {
                row.agent_id == id
                    && row.is_current
                    && lives.iter().any(|live| {
                        same_live_slot(id, &live.credentials, &row.credentials)
                            && row.kind == live.kind
                            && accounts_same_authorization(
                                adapter.as_ref(),
                                live.kind,
                                &live.credentials,
                                row,
                            )
                    })
            }) else {
                continue;
            };
            if !current.extra.is_object() {
                current.extra = json!({});
            }
            if let Some(extra) = current.extra.as_object_mut() {
                extra.insert("authHealth".into(), json!(state.health));
                extra.insert("authSource".into(), json!(state.source));
                extra.insert("liveRevision".into(), json!(state.revision));
            }
        }
    }

    /// Leftover 本机路由 live must not block switching back to 官方登录.
    pub(super) fn leftover_live_skips_identity(&self, agent: AgentId) -> bool {
        if agent != AgentId::Codex {
            return false;
        }
        use crate::integrations::agents::codex::leftover;
        if leftover::live_config_is_bridge_leftover() {
            return true;
        }
        crate::storage::ProviderRepo::new(self.db.clone())
            .get_current(agent)
            .ok()
            .flatten()
            .is_some_and(|provider| leftover::provider_is_bridge_leftover(&provider))
    }

    pub(super) fn validate_live_switch_identity(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        live: &LiveAccount,
    ) -> Result<()> {
        if leftover_shaped_codex_live(agent) {
            return Ok(());
        }
        let rows = self.repo.list(Some(agent))?;
        let exact = rows.iter().any(|row| {
            row.kind == live.kind
                && same_live_slot(agent, &live.credentials, &row.credentials)
                && accounts_same_authorization(adapter, live.kind, &live.credentials, row)
        });
        if exact {
            return Ok(());
        }
        let Some(identity) = stable_live_identity(adapter, live.kind, &live.credentials) else {
            return Err(AppError::message(
                "account.identity_conflict",
                "live account identity is unknown; refusing to backfill or switch",
            ));
        };
        let identity_count = rows
            .iter()
            .filter(|row| {
                stable_live_identity(adapter, row.kind, &row.credentials).as_deref()
                    == Some(identity.as_str())
            })
            .count();
        if identity_count > 1 {
            return Err(AppError::message(
                "account.identity_conflict",
                "live account identity is ambiguous; refusing to backfill or switch",
            ));
        }
        if rows.iter().any(|row| {
            row.is_current && stable_live_identity(adapter, row.kind, &row.credentials).is_none()
        }) {
            return Err(AppError::message(
                "account.identity_conflict",
                "current account identity is unknown; refusing to backfill or switch",
            ));
        }
        Ok(())
    }

    /// Force-refresh upstream 5h/7d quota windows for one OAuth account.
    pub(super) fn sync_current_account_live(
        &self,
        stored: &Account,
        api_key: Option<&str>,
        note: &str,
    ) -> Result<()> {
        let key_changed = api_key
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        if !stored.is_current || !key_changed {
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
        if !adapter.capability(Capability::AccountSwitch).is_usable() {
            self.snapshot_after_pool_change(stored.agent_id, note);
            return Ok(());
        }

        let live_before = match adapter.read_account() {
            Ok(live) => Some(live),
            Err(error) if error.code() == "not_found" => None,
            Err(error) => return Err(error),
        };
        let apply_live = stored.to_live();
        if live_before
            .as_ref()
            .is_some_and(|before| before.credentials == apply_live.credentials)
        {
            self.snapshot_after_pool_change(stored.agent_id, note);
            return Ok(());
        }

        let _lock = self.acquire_live_lock(stored.agent_id)?;
        if let Err(error) = backup.snapshot(
            stored.agent_id,
            BackupKind::AutoSwitch,
            Some(&format!("before applying current account {}", stored.id)),
        ) {
            if error.code() != "not_found" {
                return Err(error);
            }
        }
        if let Err(error) = adapter.apply_account(&apply_live) {
            // Codex/Pi (and similar) can store API-key accounts but refuse to
            // apply that format to live files. Keep the pool update; do not
            // fail the save or attempt a live rollback of an unapplied write.
            if error.code() == "unsupported" {
                self.snapshot_after_pool_change(stored.agent_id, note);
                return Ok(());
            }
            let live_rollback = match &live_before {
                Some(before) => adapter.apply_account(before).err(),
                None => None,
            };
            return Err(compensated_current_account_apply_error(
                error,
                live_rollback,
            ));
        }
        Ok(())
    }
}

fn leftover_shaped_codex_live(agent: AgentId) -> bool {
    agent == AgentId::Codex
        && crate::integrations::agents::codex::leftover::live_config_is_bridge_leftover()
}

pub(super) fn compensated_current_account_apply_error_with_db(
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
        "account.current.apply.rollback",
        format!(
            "applying the current account failed [{}]; compensation status: live={live}, database={database}",
            primary.code()
        ),
    )
}
