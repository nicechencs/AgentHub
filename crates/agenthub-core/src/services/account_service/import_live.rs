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

        // 远端票按授权指纹去重；loopback 桥票按 agent+kind 槽位 upsert
        // （bind 会轮换 port+bearer，见 docs/account-authorization-pool.md）。
        if let Some(existing) =
            self.find_duplicate_authorization(adapter, agent, live.kind, &live.credentials)?
        {
            return self.merge_into_existing(
                adapter,
                existing,
                live.kind,
                display,
                live.credentials,
                extra,
                make_current,
            );
        }

        let now = now_ts();
        let row = Account {
            id: format!("{}-live-{}", agent.as_str(), Uuid::new_v4()),
            agent_id: agent,
            kind: live.kind,
            label: display,
            credentials: live.credentials,
            extra,
            status: "active".into(),
            is_current: make_current,
            created_at: now.clone(),
            updated_at: now,
        };
        if make_current {
            let (created, _binding) = self.connections.create_and_activate_account(&row)?;
            self.stamp_account_surface(created)
        } else {
            let created = self.repo.create(&row)?;
            self.stamp_account_surface(created)
        }
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
        let now = now_ts();
        let mut row = existing.clone();
        row.kind = kind;
        row.label = label;
        row.credentials = credentials;
        row.extra = extra;
        row.status = "active".into();
        row.updated_at = now;
        if mark_current {
            row.is_current = true;
        }

        let updated = if row.is_current {
            let (updated, _binding) = self.connections.update_and_activate_account(&row)?;
            updated
        } else {
            self.repo.update(&row)?
        };

        let incoming_loopback = credentials_are_loopback(&updated.credentials);
        let leftovers = self.repo.list(Some(updated.agent_id))?;
        for other in leftovers {
            if other.id == updated.id || other.kind != updated.kind {
                continue;
            }
            if !same_live_slot(updated.agent_id, &updated.credentials, &other.credentials) {
                continue;
            }
            let should_delete = if incoming_loopback {
                credentials_are_loopback(&other.credentials)
            } else {
                !credentials_are_loopback(&other.credentials)
                    && accounts_same_authorization(
                        adapter,
                        updated.kind,
                        &updated.credentials,
                        &other,
                    )
            };
            if should_delete {
                // Prefer consistency path so an active leftover never leaves a dangling binding.
                // Propagate delete errors — never report merge success with leftover rows.
                self.connections
                    .delete_account(&other.id, updated.agent_id)?;
            }
        }

        self.stamp_account_surface(updated)
    }

    /// Classify the persisted row and write `extra.surface` before the import
    /// / add path returns. `classify_source_product` reads the stored row, so
    /// this runs after the first successful persist.
    pub(super) fn stamp_account_surface(&self, account: Account) -> Result<Account> {
        let product = AdapterRouteService::new(self.db.clone())
            .classify_source_product(AdapterSourceKind::Account, &account.id)?;
        let surface = TicketSurface::from_product(product);
        if TicketSurface::from_persisted_json(&account.extra)
            == PersistedTicketSurface::Known(surface)
        {
            return Ok(account);
        }
        let mut stamped = account;
        attach_persisted_surface(&mut stamped.extra, surface);
        stamped.updated_at = now_ts();
        self.repo.update(&stamped)
    }
}
