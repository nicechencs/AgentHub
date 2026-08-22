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
    AgentId, BackupKind, Capability, LiveAccount, PersistedTicketSurface,
    TicketSurface,
};
use crate::services::switch_undo::{
    clear_switch_undo, peek_switch_undo, record_switch_undo, ACCOUNT_UNDO_PREFIX,
};
use crate::services::{AdapterRouteService, BackupService, ConnectionService};
use crate::storage::{AccountRepo, Database};
use crate::utils::agent_lock::AgentWriteLock;
use crate::utils::loopback::credentials_are_loopback;
use crate::utils::redact::mask_secret_preview;

use super::surface::*;
use super::{AccountService, MAX_ACCOUNT_ID_LEN, MAX_ACCOUNT_LABEL_LEN};

impl AccountService {
    pub fn import_live(&self, agent: AgentId, name: Option<&str>) -> Result<Account> {
        let started = Instant::now();
        let result = self.import_live_inner(agent, name);
        if result.is_ok() {
            self.snapshot_after_pool_change(agent, "after live account import");
        }
        log_account_op("import", agent, started, &result);
        result
    }

    pub(super) fn import_live_inner(&self, agent: AgentId, name: Option<&str>) -> Result<Account> {
        // Pi stores multi-provider credentials in one auth.json — expand to
        // one pool row per provider so Connections can show each OAuth login.
        if agent == AgentId::Pi {
            return self.import_pi_providers_inner(name);
        }

        let adapter = self.registry.require(agent, Capability::AccountSwitch)?;
        let _lock = self.acquire_live_lock(agent)?;
        let live = adapter.read_account()?;
        if live.agent != agent {
            return Err(AppError::InvalidArg(format!(
                "adapter returned account for {}, expected {}",
                live.agent.as_str(),
                agent.as_str()
            )));
        }

        self.upsert_live_account(adapter.as_ref(), agent, live, name, true)
    }

    /// Import each Pi auth.json provider as its own pool account.
    /// Returns the last imported account for UI focus. Pi providers are
    /// concurrent entries in one live file, so import does not guess a global
    /// current provider.
    pub(super) fn import_pi_providers_inner(&self, name: Option<&str>) -> Result<Account> {
        let adapter = self
            .registry
            .require(AgentId::Pi, Capability::AccountSwitch)?;
        let _lock = self.acquire_live_lock(AgentId::Pi)?;
        let body = crate::adapters::pi_auth::read_auth_json()?;
        let lives = crate::adapters::pi_auth::expand_auth_to_live_accounts(&body)?;
        if lives.is_empty() {
            return Err(AppError::NotFound(
                "Pi auth.json has no provider credentials to import".into(),
            ));
        }

        let mut last: Option<Account> = None;
        let n = lives.len();
        for (i, live) in lives.into_iter().enumerate() {
            let is_last = i + 1 == n;
            let display_name = if is_last { name } else { None };
            let acc =
                self.upsert_live_account(adapter.as_ref(), AgentId::Pi, live, display_name, false)?;
            last = Some(acc);
        }
        last.ok_or_else(|| AppError::message("account.import", "Pi import produced no accounts"))
    }

    pub(super) fn upsert_live_account(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        live: LiveAccount,
        name: Option<&str>,
        make_current: bool,
    ) -> Result<Account> {
        if live.agent != agent {
            return Err(AppError::InvalidArg(format!(
                "adapter returned account for {}, expected {}",
                live.agent.as_str(),
                agent.as_str()
            )));
        }

        let display = name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or(live.label_hint.clone())
            .unwrap_or_else(|| format!("Imported {}", now_ts()));
        validate_label(&display, "account label", MAX_ACCOUNT_LABEL_LEN)?;

        let mut extra = live.extra;
        if let Some(obj) = extra.as_object_mut() {
            obj.insert("source".into(), json!("live"));
        }
        let extra = attach_identity_meta(adapter, live.kind, &live.credentials, &display, extra);

        let now = now_ts();
        let row = Account {
            id: format!("{}-live-{}", agent.as_str(), Uuid::new_v4()),
            agent_id: agent,
            kind: live.kind,
            label: display.clone(),
            credentials: live.credentials.clone(),
            extra: extra.clone(),
            status: "active".into(),
            is_current: make_current,
            created_at: now.clone(),
            updated_at: now,
        };
        let row = self.prepare_account_surface(row);
        self.commit_authorization_merge(
            adapter,
            &row,
            live.kind,
            display,
            live.credentials,
            extra,
            make_current,
        )
        .map(|committed| committed.stored)
        .map_err(|error| error.into_error())
    }

    /// 查找与给定凭据为「同一授权票」的已有行（非身份）。
    ///
    /// Loopback 桥票按 agent+kind 槽位匹配，不看 token 指纹。远端票仍按
    /// `accounts_same_authorization`，且不会并进 loopback 行。
    pub(super) fn find_duplicate_authorization(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        kind: AccountKind,
        credentials: &Value,
    ) -> Result<Option<Account>> {
        let incoming_loopback = credentials_are_loopback(credentials);
        let candidates = self.repo.list(Some(agent))?;
        let matches: Vec<Account> = candidates
            .into_iter()
            .filter(|a| a.kind == kind)
            .filter(|a| same_live_slot(agent, credentials, &a.credentials))
            .filter(|a| {
                let existing_loopback = credentials_are_loopback(&a.credentials);
                if incoming_loopback {
                    existing_loopback
                } else {
                    !existing_loopback && accounts_same_authorization(adapter, kind, credentials, a)
                }
            })
            .collect();
        Ok(pick_primary_authorization_match(matches))
    }

    /// 合并进已有授权行。远端票只清理同授权指纹冗余；loopback 桥票清理
    /// 同 agent+kind 的其它 loopback 行。绝不按身份删其它授权。
    pub(super) fn merge_into_existing(
        &self,
        adapter: &dyn AgentAdapter,
        existing: Account,
        kind: AccountKind,
        label: String,
        credentials: Value,
        extra: Value,
        mark_current: bool,
    ) -> Result<Account> {
        self.merge_into_existing_with_footprint(
            adapter,
            existing,
            kind,
            label,
            credentials,
            extra,
            mark_current,
            None,
            None,
        )
        .map(|(updated, _deleted)| updated)
        .map_err(|error| error.into_error())
    }

    pub(super) fn merge_into_existing_with_footprint(
        &self,
        adapter: &dyn AgentAdapter,
        existing: Account,
        kind: AccountKind,
        label: String,
        credentials: Value,
        extra: Value,
        mark_current: bool,
        _expected_target_updated_at: Option<&str>,
        _expected_current: Option<(&str, &str)>,
    ) -> std::result::Result<
        (Account, Vec<Account>),
        super::pool_crud::AccountMutationError,
    > {
        self.commit_authorization_merge(
            adapter,
            &existing,
            kind,
            label,
            credentials,
            extra,
            mark_current,
        )
        .map(|committed| (committed.stored, committed.deleted))
    }

    /// Add the ticket surface to a prospective row before its first database
    /// mutation. Only a missing `extra.surface` is filled; Unrecognized and
    /// Known values are left untouched so a newer/future surface cannot be
    /// overwritten by this version's classifier.
    pub(super) fn prepare_account_surface(&self, mut account: Account) -> Account {
        if TicketSurface::from_persisted_json(&account.extra) != PersistedTicketSurface::Missing {
            return account;
        }
        let product = AdapterRouteService::classify_account_source_product(&account);
        attach_persisted_surface(
            &mut account.extra,
            TicketSurface::from_product(product),
        );
        account
    }

    pub(super) fn copy_persisted_surface(from: &Value, into: &mut Value) {
        let Some(surface) = from.get("surface") else {
            return;
        };
        if let Some(obj) = into.as_object_mut() {
            obj.insert("surface".into(), surface.clone());
        }
    }

    /// Repair a legacy row's surface using a narrow optimistic update. Only
    /// `extra.surface` and `updated_at` are written; credentials, label,
    /// current state and active binding are never copied from a stale caller.
    pub(super) fn stamp_account_surface(&self, account: Account) -> Result<Account> {
        let prepared = self.prepare_account_surface(account.clone());
        if prepared.extra == account.extra {
            return Ok(account);
        }
        let expected_updated_at = account.updated_at.clone();
        let updated_at = now_ts();
        let extra = serde_json::to_string(&prepared.extra)?;
        let changed = self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE accounts SET extra = ?2, updated_at = ?3 WHERE id = ?1 AND agent_id = ?4 AND updated_at = ?5",
                rusqlite::params![
                    &account.id,
                    extra,
                    &updated_at,
                    account.agent_id.as_str(),
                    &expected_updated_at,
                ],
            )
            .map_err(AppError::from)
        })?;
        if changed != 1 {
            return Err(AppError::message(
                "account.conflict",
                format!("account changed before surface update: {}", account.id),
            ));
        }
        self.repo
            .get_by_id(&account.id)?
            .ok_or_else(|| AppError::NotFound(format!("account not found: {}", account.id)))
    }
}
