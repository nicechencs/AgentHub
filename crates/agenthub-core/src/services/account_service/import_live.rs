use std::time::Instant;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::adapters::AgentAdapter;
use crate::error::{AppError, Result};
use crate::models::{
    attach_persisted_surface, Account, AgentId, Capability, LiveAccount, PersistedTicketSurface,
    TicketSurface,
};
use crate::services::adapter_projection::projection_import_error;
use crate::services::AdapterRouteService;

use super::surface::*;
use super::{AccountService, MAX_ACCOUNT_LABEL_LEN};

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
        let lives = self.read_live_accounts(adapter.as_ref(), agent)?;
        if lives.is_empty() {
            return Err(AppError::NotFound(
                "no live account credentials found".into(),
            ));
        }

        // 「同步当前登录」is a user override: always copy the live file onto the
        // matching row. Do not run rt/mtime bidirectional overlay here.
        // Grok nested auth.json slots import one row per person, like Pi
        // providers. Keep an existing current if that person is still in the
        // file; otherwise activate the default `::client` slot, not last-sorted.
        let mut grants = Vec::new();
        let mut blocked_projection = false;
        for live in lives {
            if self.classify_live_account(agent, &live)?.is_projection() {
                blocked_projection = true;
                continue;
            }
            grants.push(live);
        }
        if grants.is_empty() {
            if blocked_projection {
                return Err(projection_import_error());
            }
            return Err(AppError::message(
                "account.import",
                "live import produced no accounts",
            ));
        }
        let current = self.repo.get_current(agent)?;
        let chosen =
            self.pick_live_grant_to_activate(adapter.as_ref(), agent, &grants, current.as_ref());
        let mut chosen_live = None;
        let mut others = Vec::new();
        for (index, live) in grants.into_iter().enumerate() {
            if index == chosen {
                chosen_live = Some(live);
            } else {
                others.push(live);
            }
        }
        for live in others {
            self.upsert_live_account(adapter.as_ref(), agent, live, None, false)?;
        }
        let live = chosen_live.ok_or_else(|| {
            AppError::message("account.import", "live import produced no accounts")
        })?;
        self.upsert_live_account(adapter.as_ref(), agent, live, name, true)?
            .ok_or_else(|| AppError::message("account.import", "live import produced no accounts"))
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

        let mut grants = Vec::new();
        let mut blocked_projection = false;
        for live in lives {
            if self
                .classify_live_account(AgentId::Pi, &live)?
                .is_projection()
            {
                blocked_projection = true;
                continue;
            }
            grants.push(live);
        }
        if grants.is_empty() {
            if blocked_projection {
                return Err(projection_import_error());
            }
            return Err(AppError::message(
                "account.import",
                "Pi import produced no accounts",
            ));
        }
        let n = grants.len();
        let mut last = None;
        for (i, live) in grants.into_iter().enumerate() {
            let display_name = if i + 1 == n { name } else { None };
            if let Some(account) =
                self.upsert_live_account(adapter.as_ref(), AgentId::Pi, live, display_name, false)?
            {
                last = Some(account);
            }
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
    ) -> Result<Option<Account>> {
        if live.agent != agent {
            return Err(AppError::InvalidArg(format!(
                "adapter returned account for {}, expected {}",
                live.agent.as_str(),
                agent.as_str()
            )));
        }

        if let Some(trash_id) = self.matching_live_trash_id(adapter, agent, &live)? {
            if !make_current {
                tracing::debug!(
                    module = crate::logging::targets::ACCOUNT,
                    agent = agent.as_str(),
                    "live import skipped a recycle-bin login"
                );
                return Ok(None);
            }
            self.connections.restore_trash(&trash_id)?;
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
        let mut row = Account {
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
        row = self.prepare_account_surface(row);
        let _ = crate::services::account_identity_heal::heal_account_identity(&mut row);
        self.commit_authorization_merge(
            adapter,
            &row,
            live.kind,
            row.label.clone(),
            row.credentials.clone(),
            row.extra.clone(),
            make_current,
        )
        .map(|committed| Some(committed.stored))
        .map_err(|error| error.into_error())
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
        attach_persisted_surface(&mut account.extra, TicketSurface::from_product(product));
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

    /// Keep last4 / host / hash already stamped on the pool row when a live
    /// snapshot arrives without them (redacted `***`, leftover import). Never
    /// invents a key or copies another login's identity.
    pub(super) fn copy_persisted_identity(from: &Value, into: &mut Value) {
        for key in [
            "secretTail",
            "secretHash",
            "endpoint",
            "baseUrl",
            "base_url",
        ] {
            copy_missing_extra_string(from, into, key);
        }
        let old_label = from
            .get("identityLabel")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        let new_label = into
            .get("identityLabel")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        let old_has_identity = crate::utils::redact::secret_tail_from_masked_preview(old_label)
            .is_some()
            || (old_label.contains('@') && !old_label.contains(' '));
        let new_is_weak = new_label.is_empty() || new_label.eq_ignore_ascii_case("API Key");
        if old_has_identity && new_is_weak {
            if let Some(obj) = into.as_object_mut() {
                obj.insert("identityLabel".into(), json!(old_label));
            }
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

fn copy_missing_extra_string(from: &Value, into: &mut Value, key: &str) {
    let incoming = into
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if !incoming.is_empty() {
        return;
    }
    let Some(value) = from
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    if let Some(obj) = into.as_object_mut() {
        obj.insert(key.into(), json!(value));
    }
}
