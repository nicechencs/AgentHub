//! Account pool service — CRUD, import-live, and safe live switching.
//!
//! Credentials use the existing storage scheme (no additional at-rest encryption).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use chrono::Utc;
use uuid::Uuid;

use crate::adapters::{AdapterRegistry, AgentAdapter};
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{
    Account, AccountInput, AccountKind, AccountSwitchResult, AgentId, BackupKind, Capability,
    LiveAccount,
};
use crate::services::{BackupService, ConnectionService};
use crate::storage::{AccountRepo, Database};
use crate::utils::agent_lock::AgentWriteLock;
use crate::utils::redact::mask_secret_preview;
use serde_json::{json, Value};

pub const MAX_ACCOUNT_ID_LEN: usize = 128;
pub const MAX_ACCOUNT_LABEL_LEN: usize = 256;

/// Business facade over [`AccountRepo`].
pub struct AccountService {
    repo: AccountRepo,
    registry: AdapterRegistry,
    backup: Option<BackupService>,
    lock_dir: Option<PathBuf>,
    connections: ConnectionService,
}

impl AccountService {
    /// CRUD-only construction (no live switch).
    pub fn new(db: Database) -> Self {
        Self::with_registry(db, AdapterRegistry::default())
    }

    pub fn with_registry(db: Database, registry: AdapterRegistry) -> Self {
        Self {
            repo: AccountRepo::new(db.clone()),
            registry,
            backup: None,
            lock_dir: None,
            connections: ConnectionService::new(db),
        }
    }

    /// Full live-switch service with shared backup root / lock directory.
    pub fn with_live(db: Database, registry: AdapterRegistry, backups_root: PathBuf) -> Self {
        let lock_dir = backups_root.parent().unwrap_or(&backups_root).join("locks");
        Self {
            repo: AccountRepo::new(db.clone()),
            backup: Some(BackupService::new(
                db.clone(),
                registry.clone(),
                backups_root,
            )),
            registry,
            lock_dir: Some(lock_dir),
            connections: ConnectionService::new(db),
        }
    }

    pub fn list(&self, agent: Option<AgentId>) -> Result<Vec<Account>> {
        // File-backed agents can rotate credentials while they are running.
        // Reconcile a safe live snapshot before mapping rows for the UI so a
        // stale DB snapshot cannot be shown as a dead login.
        self.sync_current_live(agent);
        let mut items = self.repo.list(agent)?;
        // Persist identity extracted from stored tokens so GUI sees email/sub
        // after redaction (JWT lives only in credentials until healed).
        // Also promote token expiry and (for current OAuth) best-effort 5h/7d quota.
        for item in &mut items {
            let expected_updated_at = item.updated_at.clone();
            let mut dirty = false;
            if super::account_identity_heal::heal_account_identity(item) {
                dirty = true;
            }
            if item.kind == AccountKind::Oauth && super::account_quota::heal_token_expiry(item) {
                dirty = true;
            }
            // Tick quota countdown from absolute reset timestamps (no network).
            if item.kind == AccountKind::Oauth
                && super::account_quota::refresh_quota_reset_label(item, Utc::now())
            {
                dirty = true;
            }
            // Only probe upstream quota for the active OAuth account to keep list snappy.
            if item.is_current
                && item.kind == AccountKind::Oauth
                && super::account_quota::try_refresh_account_quota(item, false)
            {
                dirty = true;
            }
            if dirty {
                let updated_at = now_ts();
                match self
                    .repo
                    .update_healed_fields(item, &expected_updated_at, &updated_at)
                {
                    Ok(updated) => *item = updated,
                    Err(e) => {
                        tracing::warn!(
                            module = targets::ACCOUNT,
                            account_id = %item.id,
                            agent = item.agent_id.as_str(),
                            error = %e,
                            "failed to persist healed account identity/quota"
                        );
                    }
                }
            }
        }
        // Live auth health describes the file currently observed by the adapter,
        // rather than the persisted pool row. Surface it only on the pool row
        // that still corresponds to that live authorization, and never write it
        // back to the database.
        self.merge_live_auth_state(&mut items, agent);
        sort_accounts(&mut items);
        Ok(items)
    }

    /// Best-effort reconciliation of the adapter's current live credentials.
    /// Exact authorizations and safe rotations update their existing pool row;
    /// a verified, distinct live grant is retained as its own row instead of
    /// being mistaken for a token refresh. Pi is expanded by provider before
    /// reconciliation because its live snapshot is a combined auth.json file.
    fn sync_current_live(&self, agent: Option<AgentId>) {
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
    fn read_live_accounts(
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
    /// Authorization fingerprints are checked before identity. This lets an
    /// exact token/key rotation update its owning row even when an adapter
    /// cannot expose a stable identity, while all other unknown/ambiguous
    /// cases fail closed. No account is ever deleted by this path.
    fn reconcile_live_account(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        live: LiveAccount,
    ) -> Result<Option<Account>> {
        if live.agent != agent || live_account_is_empty(&live) {
            return Ok(None);
        }
        let rows = self.repo.list(Some(agent))?;

        let authorization_matches = rows
            .iter()
            .filter(|row| row.kind == live.kind)
            .filter(|row| same_live_slot(agent, &live.credentials, &row.credentials))
            .filter(|row| accounts_same_authorization(adapter, live.kind, &live.credentials, row))
            .cloned()
            .collect::<Vec<_>>();
        if let Some(existing) = pick_primary_authorization_match(authorization_matches) {
            let (row, changed) = self.update_live_row(adapter, existing, live);
            return Ok(Some(self.persist_reconciled_live_row(agent, row, changed)?));
        }

        let Some(live_identity) = stable_live_identity(adapter, live.kind, &live.credentials)
        else {
            tracing::warn!(
                module = targets::ACCOUNT,
                agent = agent.as_str(),
                "live account identity is unknown; refusing non-exact reconcile"
            );
            return Ok(None);
        };

        let identity_matches = rows
            .iter()
            .filter(|row| same_live_slot(agent, &live.credentials, &row.credentials))
            .filter(|row| {
                stable_live_identity(adapter, row.kind, &row.credentials).as_deref()
                    == Some(live_identity.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        if identity_matches.len() > 1 {
            tracing::warn!(
                module = targets::ACCOUNT,
                agent = agent.as_str(),
                matches = identity_matches.len(),
                "live identity has multiple grants; retaining the observed grant separately"
            );
        }

        // A non-exact credential change is a rotation only when it is the one
        // and only authorization for the stable identity *and* it belongs to
        // the current live slot. In particular, never choose between multiple
        // grants for the same identity based on a label or list order.
        let current = rows.iter().find(|row| {
            row.is_current && same_live_slot(agent, &live.credentials, &row.credentials)
        });
        if let ([existing], Some(current)) = (identity_matches.as_slice(), current) {
            if existing.id == current.id
                && stable_live_identity(adapter, current.kind, &current.credentials).as_deref()
                    == Some(live_identity.as_str())
            {
                let (row, changed) = self.update_live_row(adapter, current.clone(), live);
                return Ok(Some(self.persist_reconciled_live_row(agent, row, changed)?));
            }
        }

        // The live authorization is not exact, and is not an unambiguous
        // rotation of the current row. Retain it as a separate grant. For
        // single-current agents this is an external live login, so make the
        // new row current rather than leaving the UI bound to a different
        // account. Pi is different: each provider shares one auth.json and
        // reconciling a provider must never choose a global current row.
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
            label,
            credentials: live.credentials,
            extra,
            status: "active".into(),
            is_current: agent != AgentId::Pi,
            created_at: now.clone(),
            updated_at: now,
        };
        let created = if row.is_current {
            self.connections.create_and_activate_account(&row)?.0
        } else {
            self.repo.create(&row)?
        };
        Ok(Some(created))
    }

    fn update_live_row(
        &self,
        adapter: &dyn AgentAdapter,
        mut row: Account,
        live: LiveAccount,
    ) -> (Account, bool) {
        if !live_credentials_changed(&row, &live) {
            return (row, false);
        }
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
        row.kind = live.kind;
        row.status = "active".into();
        let _ = super::account_identity_heal::heal_account_identity(&mut row);
        let _ = super::account_quota::heal_token_expiry(&mut row);
        (row, true)
    }

    /// Persist a reconciled row and, for single-current agents, atomically
    /// align both the legacy current flag and the active connection binding.
    /// Pi provider slots are concurrent entries in one auth.json, so they must
    /// never be globally activated by reconciliation.
    fn persist_reconciled_live_row(
        &self,
        agent: AgentId,
        mut row: Account,
        changed: bool,
    ) -> Result<Account> {
        if agent == AgentId::Pi {
            return if changed {
                let expected_updated_at = row.updated_at.clone();
                self.repo
                    .update_healed_fields(&row, &expected_updated_at, &now_ts())
            } else {
                Ok(row)
            };
        }
        if !changed && row.is_current {
            return Ok(row);
        }
        row.is_current = true;
        row.updated_at = now_ts();
        Ok(self.connections.update_and_activate_account(&row)?.0)
    }

    /// Add a transient, desensitized AuthState view to the current pool row.
    /// This intentionally runs after all persistence/healing and does not call
    /// AccountRepo::update, keeping live state separate from the account pool.
    fn merge_live_auth_state(&self, items: &mut [Account], agent: Option<AgentId>) {
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

    fn validate_live_switch_identity(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        live: &LiveAccount,
    ) -> Result<()> {
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
    pub fn refresh_quota(&self, id_or_label: &str, agent: AgentId) -> Result<Account> {
        let mut account = self.get(id_or_label, Some(agent))?;
        if account.kind != AccountKind::Oauth {
            return Err(AppError::Unsupported(
                "quota refresh is only supported for OAuth accounts".into(),
            ));
        }
        let expected_updated_at = account.updated_at.clone();
        let mut dirty = super::account_identity_heal::heal_account_identity(&mut account);
        if super::account_quota::heal_token_expiry(&mut account) {
            dirty = true;
        }
        // Explicit refreshes are user-visible: propagate network, auth and
        // parsing failures instead of the list path's best-effort behavior.
        if super::account_quota::refresh_account_quota(&mut account, true)? {
            dirty = true;
        }
        if !dirty {
            return Ok(account);
        }
        let updated_at = now_ts();
        self.repo
            .update_healed_fields(&account, &expected_updated_at, &updated_at)
    }

    /// Resolve by id first, then exact label (optionally scoped to agent).
    pub fn get(&self, id_or_label: &str, agent: Option<AgentId>) -> Result<Account> {
        let key = id_or_label.trim();
        if key.is_empty() {
            return Err(AppError::InvalidArg(
                "account id or label must not be empty".into(),
            ));
        }

        if let Some(a) = self.repo.get_by_id(key)? {
            if let Some(agent) = agent {
                if a.agent_id != agent {
                    return Err(AppError::NotFound(format!(
                        "account not found: {key} (agent filter: {})",
                        agent.as_str()
                    )));
                }
            }
            return Ok(a);
        }

        let matches = self.repo.list_by_label(key, agent)?;
        match matches.len() {
            0 => Err(AppError::NotFound(format!("account not found: {key}"))),
            1 => Ok(matches.into_iter().next().expect("len 1")),
            n => Err(AppError::InvalidArg(format!(
                "ambiguous account label '{key}': found {n} accounts; specify --agent or use id"
            ))),
        }
    }

    pub fn delete(&self, id_or_label: &str, agent: AgentId) -> Result<()> {
        let started = Instant::now();
        let result = (|| {
            let account = self.get(id_or_label, Some(agent))?;
            // Clear active binding in the same transaction when deleting the active row.
            self.connections.delete_account(&account.id, agent)
        })();
        log_account_op("delete", agent, started, &result);
        result
    }

    /// Add an API Key account to the pool (does not switch live).
    ///
    /// `env_key` is optional and only applied when the adapter credentials
    /// object accepts `env_key` (e.g. Claude `settings.json` field name).
    pub fn add_api_key(
        &self,
        agent: AgentId,
        label: Option<&str>,
        api_key: &str,
    ) -> Result<Account> {
        self.add_api_key_with_env(agent, label, api_key, None)
    }

    pub fn add_api_key_with_env(
        &self,
        agent: AgentId,
        label: Option<&str>,
        api_key: &str,
        env_key: Option<&str>,
    ) -> Result<Account> {
        let started = Instant::now();
        let result = self.add_api_key_inner(agent, label, api_key, env_key);
        log_account_op("add_api_key", agent, started, &result);
        result
    }

    fn add_api_key_inner(
        &self,
        agent: AgentId,
        label: Option<&str>,
        api_key: &str,
        env_key: Option<&str>,
    ) -> Result<Account> {
        let adapter = self.adapter(agent)?;
        let live = adapter.build_api_key_account(api_key)?;
        let mut credentials = live.credentials;
        if let Some(ek) = env_key.map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(obj) = credentials.as_object_mut() {
                // Only set when this credential shape already uses env_key
                // or is the Claude-style api_key format.
                if obj.contains_key("env_key")
                    || obj.get("format").and_then(|v| v.as_str()) == Some("api_key")
                {
                    obj.insert("env_key".into(), json!(ek));
                }
            }
        }
        let display = label
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or(live.label_hint.clone())
            .unwrap_or_else(|| format!("{} (API Key)", mask_secret_preview(api_key)));
        validate_label(&display, "account label", MAX_ACCOUNT_LABEL_LEN)?;

        let extra = attach_identity_meta(
            adapter.as_ref(),
            AccountKind::ApiKey,
            &credentials,
            &display,
            live.extra,
        );

        // 同一 agent 下相同授权票（同一 API Key）不重复建池。
        if let Some(existing) = self.find_duplicate_authorization(
            adapter.as_ref(),
            agent,
            AccountKind::ApiKey,
            &credentials,
        )? {
            return self.merge_into_existing(
                adapter.as_ref(),
                existing,
                AccountKind::ApiKey,
                display,
                credentials,
                extra,
                false,
            );
        }

        let now = now_ts();
        let row = Account {
            id: format!("{}-acc-{}", agent.as_str(), Uuid::new_v4()),
            agent_id: agent,
            kind: AccountKind::ApiKey,
            label: display,
            credentials,
            extra,
            status: "active".into(),
            is_current: false,
            created_at: now.clone(),
            updated_at: now,
        };
        self.repo.create(&row)
    }

    /// Update an existing API Key account (label and/or key).
    ///
    /// - `label`: when `Some` and non-empty after trim, replaces the display label
    /// - `api_key`: when `Some` and non-empty after trim, rebuilds credentials via adapter
    ///
    /// A current row with a new key is written to live files. Label-only edits
    /// and non-current rows stay pool-only. This must not reuse [`Self::switch`],
    /// which treats the existing live file as authoritative for the current row.
    pub fn update_api_key(
        &self,
        agent: AgentId,
        id_or_label: &str,
        label: Option<&str>,
        api_key: Option<&str>,
    ) -> Result<Account> {
        let started = Instant::now();
        let result = (|| {
            let stored = self.update_api_key_inner(agent, id_or_label, label, api_key)?;
            self.sync_current_account_live(&stored, api_key, "after API Key account update")?;
            Ok(stored)
        })();
        log_account_op("update_api_key", agent, started, &result);
        result
    }

    fn update_api_key_inner(
        &self,
        agent: AgentId,
        id_or_label: &str,
        label: Option<&str>,
        api_key: Option<&str>,
    ) -> Result<Account> {
        let mut account = self.get(id_or_label, Some(agent))?;
        if account.kind != AccountKind::ApiKey {
            return Err(AppError::InvalidArg(
                "only API Key accounts can be updated via update_api_key".into(),
            ));
        }

        let new_label = label
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let new_key = api_key
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        if new_label.is_none() && new_key.is_none() {
            return Err(AppError::InvalidArg(
                "update_api_key requires a non-empty label and/or api_key".into(),
            ));
        }

        let adapter = self.adapter(agent)?;

        if let Some(ref key) = new_key {
            let live = adapter.build_api_key_account(key)?;
            // Preserve env_key from existing credentials when the new live
            // snapshot does not set one (adapter default still applied).
            let mut creds = live.credentials;
            if let Some(prev_env) = account
                .credentials
                .get("env_key")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                if creds.get("env_key").and_then(|v| v.as_str()).is_none() {
                    if let Some(obj) = creds.as_object_mut() {
                        obj.insert("env_key".into(), json!(prev_env));
                    }
                }
            }
            let display = new_label.clone().unwrap_or_else(|| {
                live.label_hint
                    .clone()
                    .unwrap_or_else(|| format!("{} (API Key)", mask_secret_preview(key)))
            });
            validate_label(&display, "account label", MAX_ACCOUNT_LABEL_LEN)?;
            let extra = attach_identity_meta(
                adapter.as_ref(),
                AccountKind::ApiKey,
                &creds,
                &display,
                live.extra,
            );

            // Same key as another pool row → merge into that row and drop this one.
            if let Some(existing) = self.find_duplicate_authorization(
                adapter.as_ref(),
                agent,
                AccountKind::ApiKey,
                &creds,
            )? {
                if existing.id != account.id {
                    let merged = self.merge_into_existing(
                        adapter.as_ref(),
                        existing,
                        AccountKind::ApiKey,
                        display,
                        creds,
                        extra,
                        account.is_current,
                    )?;
                    self.connections.delete_account(&account.id, agent)?;
                    return Ok(merged);
                }
            }

            account.credentials = creds;
            account.extra = extra;
            account.label = display;
        } else if let Some(display) = new_label {
            validate_label(&display, "account label", MAX_ACCOUNT_LABEL_LEN)?;
            account.label = display;
            // Refresh identity meta with new label without changing credentials.
            account.extra = attach_identity_meta(
                adapter.as_ref(),
                AccountKind::ApiKey,
                &account.credentials,
                &account.label,
                account.extra.clone(),
            );
        }

        account.updated_at = now_ts();
        account.status = "active".into();
        self.repo.update(&account)
    }

    /// Create a pool account from a fully formed input (e.g. OAuth PKCE result).
    /// Does not write live credentials.
    pub fn create(&self, input: AccountInput) -> Result<Account> {
        let started = Instant::now();
        let agent = input.agent_id;
        let result = self.create_inner(input);
        log_account_op("create", agent, started, &result);
        result
    }

    fn create_inner(&self, input: AccountInput) -> Result<Account> {
        validate_label(&input.label, "account label", MAX_ACCOUNT_LABEL_LEN)?;
        let label = input.label.trim().to_string();
        let adapter = self.adapter(input.agent_id).ok();
        let extra = if let Some(ref ad) = adapter {
            attach_identity_meta(
                ad.as_ref(),
                input.kind,
                &input.credentials,
                &label,
                input.extra,
            )
        } else {
            input.extra
        };

        if let Some(ref ad) = adapter {
            if let Some(existing) = self.find_duplicate_authorization(
                ad.as_ref(),
                input.agent_id,
                input.kind,
                &input.credentials,
            )? {
                return self.merge_into_existing(
                    ad.as_ref(),
                    existing,
                    input.kind,
                    label,
                    input.credentials,
                    extra,
                    input.is_current,
                );
            }
        }

        let now = now_ts();
        let row = Account {
            id: format!("{}-acc-{}", input.agent_id.as_str(), Uuid::new_v4()),
            agent_id: input.agent_id,
            kind: input.kind,
            label,
            credentials: input.credentials,
            extra,
            status: "active".into(),
            is_current: input.is_current,
            created_at: now.clone(),
            updated_at: now,
        };
        if row.is_current {
            let (created, _binding) = self.connections.create_and_activate_account(&row)?;
            Ok(created)
        } else {
            self.repo.create(&row)
        }
    }

    /// Refresh OAuth tokens for a saved account (uses `refresh_token` grant).
    /// Updates pool credentials; does not rewrite live files.
    pub fn refresh_token(&self, id_or_label: &str, agent: AgentId) -> Result<Account> {
        let started = Instant::now();
        let result = self.refresh_token_inner(id_or_label, agent);
        log_account_op("refresh_token", agent, started, &result);
        result
    }

    fn refresh_token_inner(&self, id_or_label: &str, agent: AgentId) -> Result<Account> {
        if agent == AgentId::Grok {
            return Err(AppError::Unsupported(
                "Grok CLI 会在本机 auth.json 中自动续期，请使用“同步当前登录”".into(),
            ));
        }
        let mut account = self.get(id_or_label, Some(agent))?;
        let expected_updated_at = account.updated_at.clone();
        if account.kind != AccountKind::Oauth {
            return Err(AppError::Unsupported(
                "token refresh is only supported for OAuth accounts".into(),
            ));
        }

        // Heal first so Pi body.refresh is promoted to refresh_token.
        let _ = super::account_identity_heal::heal_account_identity(&mut account);

        let (mut creds, extra_base, new_identity) = if agent == AgentId::Pi {
            let creds = crate::oauth::refresh_pi_provider(&account.credentials)?;
            let identity = crate::oauth::identity_from_credentials(&creds);
            let extra = json!({
                "source": "oauth_refresh",
                "provider": creds.get("provider").cloned().unwrap_or(json!(null)),
            });
            (creds, extra, identity)
        } else {
            let provider_id = account
                .credentials
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or(agent.as_str());
            let refresh = account
                .credentials
                .get("refresh_token")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    AppError::message(
                        "oauth.refresh",
                        "account has no refresh_token; re-run OAuth login",
                    )
                })?;

            let provider = crate::oauth::oauth_provider_for(agent).ok_or_else(|| {
                AppError::Unsupported(format!(
                    "OAuth refresh is not configured for {} (provider={provider_id})",
                    agent.as_str()
                ))
            })?;

            let bundle = provider.refresh(refresh)?;
            let mut creds = bundle.credentials;
            if creds
                .get("refresh_token")
                .and_then(|v| v.as_str())
                .is_none()
                || creds
                    .get("refresh_token")
                    .and_then(|v| v.as_str())
                    .map(|s| s.is_empty())
                    .unwrap_or(true)
            {
                if let Some(obj) = creds.as_object_mut() {
                    obj.insert("refresh_token".into(), serde_json::json!(refresh));
                }
            }
            // Keep Codex accounts in live-writable auth_json shape after refresh.
            // Generic OAuth refresh returns a flat token bundle; without this step
            // a successful refresh would re-break account switch.
            if agent == AgentId::Codex {
                // Preserve prior body tokens (account_id / id_token) when refresh omits them.
                if let Some(prior_body) = account.credentials.get("body").cloned() {
                    if let Some(obj) = creds.as_object_mut() {
                        obj.entry("body".to_string()).or_insert(prior_body);
                    }
                }
                for key in ["account_id", "id_token", "email", "sub", "plan_type"] {
                    if creds.get(key).and_then(|v| v.as_str()).is_none() {
                        if let Some(v) = account.credentials.get(key).cloned() {
                            if let Some(obj) = creds.as_object_mut() {
                                obj.insert(key.into(), v);
                            }
                        }
                    }
                }
                creds = crate::adapters::normalize_codex_oauth_credentials(&creds)?;
            }
            let prior_identity = crate::oauth::identity_from_credentials(&account.credentials);
            let mut new_identity = crate::oauth::identity_from_credentials(&creds);
            new_identity.merge_missing(&prior_identity);
            if let Some(obj) = creds.as_object_mut() {
                crate::oauth::apply_identity_to_credentials(obj, &new_identity);
            }
            (creds, bundle.extra, new_identity)
        };

        // Keep prior identity fields when the refresh response omits them.
        let prior_identity = crate::oauth::identity_from_credentials(&account.credentials);
        let mut new_identity = new_identity;
        new_identity.merge_missing(&prior_identity);
        if let Some(obj) = creds.as_object_mut() {
            crate::oauth::apply_identity_to_credentials(obj, &new_identity);
        }

        account.credentials = creds;

        let mut extra = extra_base;
        if let Some(obj) = extra.as_object_mut() {
            if let Some(exp) = account.credentials.get("expires_at").cloned() {
                obj.insert("expiresAt".into(), exp);
            }
            obj.insert("source".into(), serde_json::json!("oauth_refresh"));
            if let Some(ref email) = new_identity.email {
                obj.insert("email".into(), json!(email));
            }
            if let Some(ref plan) = new_identity.subscription {
                obj.insert("subscription".into(), json!(plan));
            }
            if let Some(label) = new_identity.display_label() {
                obj.insert("identityLabel".into(), json!(label));
            }
            if let Some(p) = account.credentials.get("provider").and_then(|v| v.as_str()) {
                obj.insert("provider".into(), json!(p));
            }
        }
        // Prefer adapter identity_label for final extra shape.
        if let Ok(adapter) = self.adapter(agent) {
            extra = attach_identity_meta(
                adapter.as_ref(),
                account.kind,
                &account.credentials,
                &account.label,
                extra,
            );
        }
        account.extra = extra;

        // Upgrade generic OAuth labels once we learn a real identity.
        if let Some(lab) = new_identity.display_label() {
            if is_generic_oauth_label(&account.label, agent)
                || super::account_identity_heal::needs_identity_heal(&account)
            {
                if agent == AgentId::Pi {
                    if let Some(p) = account.credentials.get("provider").and_then(|v| v.as_str()) {
                        account.label = format!("pi:{p} · {lab}");
                    } else {
                        account.label = lab;
                    }
                } else {
                    account.label = lab;
                }
            }
        }

        let _ = super::account_quota::heal_token_expiry(&mut account);
        // Fresh access token → re-probe 5h/7d windows when supported.
        let _ = super::account_quota::try_refresh_account_quota(&mut account, true);

        account.updated_at = now_ts();
        account.status = "active".into();
        if agent == AgentId::Pi {
            return self.persist_pi_oauth_account_update(&account, &expected_updated_at);
        }
        self.repo
            .update_healed_fields(&account, &expected_updated_at, &account.updated_at)
    }

    /// Import the agent's current live file credentials into the account pool.
    pub fn import_live(&self, agent: AgentId, name: Option<&str>) -> Result<Account> {
        let started = Instant::now();
        let result = self.import_live_inner(agent, name);
        if result.is_ok() {
            self.snapshot_after_pool_change(agent, "after live account import");
        }
        log_account_op("import", agent, started, &result);
        result
    }

    fn import_live_inner(&self, agent: AgentId, name: Option<&str>) -> Result<Account> {
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
    fn import_pi_providers_inner(&self, name: Option<&str>) -> Result<Account> {
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

    fn upsert_live_account(
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

        // 仅按「授权票」去重：同 token/key 再 import → upsert；
        // 同人不同 token → 新行（见 docs/account-authorization-pool.md）。
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
            Ok(created)
        } else {
            self.repo.create(&row)
        }
    }

    /// 查找与给定凭据为「同一授权票」的已有行（非身份）。
    fn find_duplicate_authorization(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        kind: AccountKind,
        credentials: &Value,
    ) -> Result<Option<Account>> {
        let candidates = self.repo.list(Some(agent))?;
        let matches: Vec<Account> = candidates
            .into_iter()
            .filter(|a| a.kind == kind)
            .filter(|a| same_live_slot(agent, credentials, &a.credentials))
            .filter(|a| accounts_same_authorization(adapter, kind, credentials, a))
            .collect();
        Ok(pick_primary_authorization_match(matches))
    }

    /// 合并进已有授权行；仅清理 **同授权指纹** 的冗余行，绝不按身份删其它授权。
    fn merge_into_existing(
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

        let leftovers = self.repo.list(Some(updated.agent_id))?;
        for other in leftovers {
            if other.id == updated.id || other.kind != updated.kind {
                continue;
            }
            if same_live_slot(updated.agent_id, &updated.credentials, &other.credentials)
                && accounts_same_authorization(adapter, updated.kind, &updated.credentials, &other)
            {
                // Prefer consistency path so an active leftover never leaves a dangling binding.
                // Propagate delete errors — never report merge success with leftover rows.
                self.connections
                    .delete_account(&other.id, updated.agent_id)?;
            }
        }

        Ok(updated)
    }

    /// Safe account switch: validate → lock → backfill → backup → apply → verify → DB.
    pub fn switch(&self, id_or_label: &str, agent: AgentId) -> Result<AccountSwitchResult> {
        let started = Instant::now();
        let result = self.switch_inner(id_or_label, agent);
        log_account_op("switch", agent, started, &result);
        result
    }

    fn switch_inner(&self, id_or_label: &str, agent: AgentId) -> Result<AccountSwitchResult> {
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

        Ok(AccountSwitchResult {
            account,
            backup: snapshot,
            backfilled_account_id,
        })
    }

    pub fn repo(&self) -> &AccountRepo {
        &self.repo
    }

    /// Persist a Pi OAuth provider entry through the same process/file lock
    /// boundary used by account switches. The live file is restored when the
    /// subsequent pool mutation fails so a partially completed OAuth flow can
    /// be retried safely.
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

    /// After an API-key pool save: if this row is current **and** the key
    /// changed, apply the stored credentials to live files. Label-only edits
    /// stay in the pool.
    fn sync_current_account_live(
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

        let live_before = adapter.read_account().ok();
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
            return Err(compensated_current_account_apply_error(error, live_rollback));
        }
        Ok(())
    }

    /// Import/update changes the AgentHub pool, not the live files. Keep an
    /// audit snapshot of the live state when the service is running with the
    /// live backup dependency; a missing live file is a normal no-op.
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

    fn adapter(&self, agent: AgentId) -> Result<Arc<dyn AgentAdapter>> {
        self.registry.get(agent).ok_or_else(|| {
            AppError::NotFound(format!(
                "no adapter registered for agent {}",
                agent.as_str()
            ))
        })
    }

    fn acquire_live_lock(&self, agent: AgentId) -> Result<Option<AgentWriteLock>> {
        self.lock_dir
            .as_deref()
            .map(|lock_dir| AgentWriteLock::acquire(lock_dir, agent))
            .transpose()
    }
}

/// Process-local counterpart to the optional cross-process file lock. Services
/// built without a backup root have no `lock_dir`, but concurrent UI reads can
/// still reconcile the same live snapshot, so they must share this guard.
fn live_reconcile_lock(agent: AgentId) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<AgentId, Arc<Mutex<()>>>>> = OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Arc::clone(
        locks
            .entry(agent)
            .or_insert_with(|| Arc::new(Mutex::new(()))),
    )
}

fn log_account_op<T>(op: &str, agent: AgentId, started: Instant, result: &Result<T>) {
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    match result {
        Ok(_) => {
            let msg = match op {
                "switch" => "switched account",
                "delete" => "deleted account",
                "add_api_key" => "added api key account",
                "update_api_key" => "updated api key account",
                "import" => "imported account",
                _ => "ok",
            };
            tracing::info!(
                module = targets::ACCOUNT,
                op,
                agent = agent.as_str(),
                elapsed_ms,
                "{msg}"
            );
        }
        Err(err) => {
            tracing::error!(
                module = targets::ACCOUNT,
                op,
                agent = agent.as_str(),
                code = err.code(),
                elapsed_ms,
                "account operation failed"
            );
        }
    }
}

fn compensated_current_account_apply_error(
    primary: AppError,
    live_rollback: Option<AppError>,
) -> AppError {
    let Some(rollback) = live_rollback else {
        return primary;
    };
    AppError::message(
        "account.current.apply.rollback",
        format!(
            "applying the current account failed [{}]; compensation status: live={}",
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
        "account.switch.rollback",
        format!(
            "account switch failed [{}]; compensation status: live={live}, database={database}",
            primary.code()
        ),
    )
}

fn now_ts() -> String {
    Utc::now().format("%Y-%m-%d %H:%M:%S%.6f").to_string()
}

fn read_optional_file(path: &std::path::Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn probe_auth_revision(adapter: &dyn AgentAdapter) -> Option<String> {
    match adapter.read_auth() {
        Ok(state) => state.revision,
        Err(error) if error.code() == "not_found" || error.code() == "unsupported" => None,
        Err(error) => {
            tracing::debug!(
                module = targets::ACCOUNT,
                agent = adapter.id().as_str(),
                error_code = error.code(),
                "live auth revision probe unavailable"
            );
            None
        }
    }
}

/// Return a live account snapshot only when the adapter's opaque revision is
/// stable on both sides of the read. `None` remains a valid revision for
/// adapters without a revision probe; a transition between `Some` and `None`
/// is still treated as a conflict and retried.
fn capture_stable_live_snapshot(
    adapter: &dyn AgentAdapter,
    attempts: usize,
) -> Result<(Option<LiveAccount>, Option<String>)> {
    for _ in 0..attempts.max(1) {
        let before = probe_auth_revision(adapter);
        let live = match adapter.read_account() {
            Ok(live) if live.agent == adapter.id() => Some(live),
            Ok(live) => {
                return Err(AppError::InvalidArg(format!(
                    "adapter returned account for {}, expected {}",
                    live.agent.as_str(),
                    adapter.id().as_str()
                )))
            }
            Err(error) if error.code() == "not_found" || error.code() == "unsupported" => None,
            Err(error) => return Err(error),
        };
        let after = probe_auth_revision(adapter);
        if before == after {
            return Ok((live, after));
        }
    }
    Err(live_revision_conflict())
}

fn live_revision_conflict() -> AppError {
    AppError::message(
        "account.live_conflict",
        "live account changed while switching; retry the switch",
    )
}

fn live_account_is_empty(live: &LiveAccount) -> bool {
    live.credentials
        .as_object()
        .map(|o| o.is_empty())
        .unwrap_or(true)
}

/// Pi has one live auth file with independent provider entries. Matching an
/// identity across providers is not evidence that either provider's grant may
/// be overwritten, nor that it is the UI's globally selected account.
fn same_live_slot(agent: AgentId, incoming: &Value, existing: &Value) -> bool {
    if agent != AgentId::Pi {
        return true;
    }
    incoming
        .get("provider")
        .and_then(|value| value.as_str())
        .zip(existing.get("provider").and_then(|value| value.as_str()))
        .is_some_and(|(incoming, existing)| incoming == existing)
}

/// 是否为「同一授权票」（非身份）。见 `docs/account-authorization-pool.md`。
fn accounts_same_authorization(
    adapter: &dyn AgentAdapter,
    kind: AccountKind,
    incoming_credentials: &Value,
    existing: &Account,
) -> bool {
    if existing.kind != kind {
        return false;
    }
    // 完整凭据相等：同 live 再 import
    if &existing.credentials == incoming_credentials {
        return true;
    }
    match (
        adapter.authorization_key(kind, incoming_credentials),
        adapter.authorization_key(kind, &existing.credentials),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Compare the serialized live credential payload, not the authorization key.
///
/// A Grok CLI refresh rotates the access/refresh token pair while preserving
/// the same user and grant. Treat that as an update to the current row; the
/// authorization-key matcher intentionally remains token-sensitive for
/// explicit imports of separate grants.
fn live_credentials_changed(current: &Account, live: &LiveAccount) -> bool {
    let Some(current_body) = current.credentials.get("body") else {
        return current.credentials != live.credentials;
    };
    let Some(live_body) = live.credentials.get("body") else {
        return current.credentials != live.credentials;
    };
    current.credentials.get("format") != live.credentials.get("format") || current_body != live_body
}

/// Stable identity extracted from credentials only. Passing `None` as the
/// label hint is intentional: a display label (especially a token preview)
/// is not an identity proof and must never authorize a live overwrite.
fn stable_live_identity(
    adapter: &dyn AgentAdapter,
    kind: AccountKind,
    credentials: &Value,
) -> Option<String> {
    if let Some(identity) = adapter
        .identity_label(kind, credentials, None)
        .map(|identity| identity.trim().to_owned())
        .filter(|identity| !identity.is_empty())
    {
        return Some(identity);
    }
    // A number of file formats wrap the account object under `body`,
    // `auth`, or provider-specific maps.  Read only well-known stable
    // identity fields recursively; never use arbitrary labels/token previews.
    find_stable_identity_field(credentials)
}

fn find_stable_identity_field(value: &Value) -> Option<String> {
    const KEYS: &[&str] = &[
        "email",
        "email_address",
        "emailAddress",
        "user_id",
        "userId",
        "principal_id",
        "principalId",
        "sub",
        "account_id",
        "accountId",
        "account_uuid",
    ];
    match value {
        Value::Object(map) => {
            for key in KEYS {
                if let Some(Value::String(identity)) = map.get(*key) {
                    let identity = identity.trim();
                    if !identity.is_empty() {
                        return Some(identity.to_owned());
                    }
                }
            }
            map.values().find_map(find_stable_identity_field)
        }
        Value::Array(items) => items.iter().find_map(find_stable_identity_field),
        _ => None,
    }
}

/// True when label is a placeholder like `Claude · OAuth` / `claude oauth`.
fn is_generic_oauth_label(label: &str, agent: AgentId) -> bool {
    let t = label.trim();
    if t.is_empty() {
        return true;
    }
    let lower = t.to_ascii_lowercase();
    let agent_name = agent.display_name().to_ascii_lowercase();
    let agent_id = agent.as_str().to_ascii_lowercase();
    lower == format!("{agent_name} · oauth")
        || lower == format!("{agent_name} ·oauth")
        || lower == format!("{agent_name} oauth")
        || lower == format!("{agent_id} oauth")
        || lower == format!("{agent_id}-oauth")
        || lower == format!("{agent_id} · oauth")
        || lower.ends_with(" · oauth")
        || lower.ends_with(" oauth")
}

/// 写入 extra.identityLabel（及 email）供 UI 分组；不参与去重。
fn attach_identity_meta(
    adapter: &dyn AgentAdapter,
    kind: AccountKind,
    credentials: &Value,
    label: &str,
    mut extra: Value,
) -> Value {
    let id_label = adapter.identity_label(kind, credentials, Some(label));
    if let Some(obj) = extra.as_object_mut() {
        if let Some(ref lab) = id_label {
            obj.insert("identityLabel".into(), json!(lab));
            if lab.contains('@') {
                obj.entry("email".to_string()).or_insert_with(|| json!(lab));
            }
        }
    } else if let Some(lab) = id_label {
        let mut map = serde_json::Map::new();
        map.insert("identityLabel".into(), json!(lab));
        if lab.contains('@') {
            map.insert("email".into(), json!(lab));
        }
        if let Value::Object(old) = extra {
            for (k, v) in old {
                map.entry(k).or_insert(v);
            }
        }
        extra = Value::Object(map);
    }
    extra
}

/// 同授权指纹的多条历史冗余：优先 current → 更早 created_at → 更小 id。
fn pick_primary_authorization_match(mut matches: Vec<Account>) -> Option<Account> {
    if matches.is_empty() {
        return None;
    }
    matches.sort_by(|a, b| {
        b.is_current
            .cmp(&a.is_current)
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    matches.into_iter().next()
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

fn agent_rank(id: AgentId) -> usize {
    AgentId::ALL
        .iter()
        .position(|a| *a == id)
        .unwrap_or(usize::MAX)
}

fn sort_accounts(items: &mut [Account]) {
    items.sort_by(|a, b| {
        agent_rank(a.agent_id)
            .cmp(&agent_rank(b.agent_id))
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.id.cmp(&b.id))
    });
}

#[cfg(test)]
mod tests;
