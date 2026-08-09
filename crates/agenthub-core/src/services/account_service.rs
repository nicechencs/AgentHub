//! Account pool service — CRUD, import-live, and safe live switching.
//!
//! Credentials use the existing storage scheme (no additional at-rest encryption).

use std::path::PathBuf;
use std::sync::Arc;
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
use crate::utils::redact::{mask_secret_preview, redact_text};
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
        let mut items = self.repo.list(agent)?;
        // Persist identity extracted from stored tokens so GUI sees email/sub
        // after redaction (JWT lives only in credentials until healed).
        // Also promote token expiry and (for current OAuth) best-effort 5h/7d quota.
        for item in &mut items {
            let mut dirty = false;
            if super::account_identity_heal::heal_account_identity(item) {
                dirty = true;
            }
            if item.kind == AccountKind::Oauth
                && super::account_quota::heal_token_expiry(item)
            {
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
                if let Err(e) = self.repo.update(item) {
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
        sort_accounts(&mut items);
        Ok(items)
    }

    /// Force-refresh upstream 5h/7d quota windows for one OAuth account.
    pub fn refresh_quota(&self, id_or_label: &str, agent: AgentId) -> Result<Account> {
        let mut account = self.get(id_or_label, Some(agent))?;
        if account.kind != AccountKind::Oauth {
            return Err(AppError::Unsupported(
                "quota refresh is only supported for OAuth accounts".into(),
            ));
        }
        let _ = super::account_identity_heal::heal_account_identity(&mut account);
        let _ = super::account_quota::heal_token_expiry(&mut account);
        let _ = super::account_quota::try_refresh_account_quota(&mut account, true);
        account.updated_at = Utc::now().format("%Y-%m-%d %H:%M:%S%.6f").to_string();
        self.repo.update(&account)?;
        Ok(account)
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

    /// Update an existing API Key account (label and/or key). Does not rewrite live files.
    ///
    /// - `label`: when `Some` and non-empty after trim, replaces the display label
    /// - `api_key`: when `Some` and non-empty after trim, rebuilds credentials via adapter
    pub fn update_api_key(
        &self,
        agent: AgentId,
        id_or_label: &str,
        label: Option<&str>,
        api_key: Option<&str>,
    ) -> Result<Account> {
        let started = Instant::now();
        let result = self.update_api_key_inner(agent, id_or_label, label, api_key);
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
        let mut account = self.get(id_or_label, Some(agent))?;
        if account.kind != AccountKind::Oauth {
            return Err(AppError::Unsupported(
                "token refresh is only supported for OAuth accounts".into(),
            ));
        }

        // Heal first so Pi body.refresh is promoted to refresh_token.
        let _ = super::account_identity_heal::heal_account_identity(&mut account);

        let (mut creds, extra_base, new_identity) = if agent == AgentId::Pi {
            let creds = crate::oauth::refresh_pi_provider(&account.credentials)?;
            // Merge into live auth.json so Pi CLI keeps working.
            if let Some(body) = creds.get("body") {
                let merged = crate::adapters::pi_auth::merge_auth_json(body)?;
                let path = crate::adapters::pi_auth::pi_auth_path()?;
                let mut bytes = serde_json::to_vec_pretty(&merged)?;
                bytes.push(b'\n');
                crate::utils::atomic::atomic_write(&path, &bytes)?;
            }
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
                    if let Some(p) = account
                        .credentials
                        .get("provider")
                        .and_then(|v| v.as_str())
                    {
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
        self.repo.update(&account)
    }

    /// Import the agent's current live file credentials into the account pool.
    pub fn import_live(&self, agent: AgentId, name: Option<&str>) -> Result<Account> {
        let started = Instant::now();
        let result = self.import_live_inner(agent, name);
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
    /// Returns the activated (or primary) account for UI focus.
    fn import_pi_providers_inner(&self, name: Option<&str>) -> Result<Account> {
        let adapter = self.registry.require(AgentId::Pi, Capability::AccountSwitch)?;
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
            // Only the last imported provider becomes current (live switch once).
            let display_name = if is_last {
                name
            } else {
                None
            };
            let acc = self.upsert_live_account(
                adapter.as_ref(),
                AgentId::Pi,
                live,
                display_name,
                is_last,
            )?;
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
        let extra = attach_identity_meta(
            adapter,
            live.kind,
            &live.credentials,
            &display,
            extra,
        );

        // 仅按「授权票」去重：同 token/key 再 import → upsert；
        // 同人不同 token → 新行（见 docs/account-authorization-pool.md）。
        if let Some(existing) = self.find_duplicate_authorization(
            adapter,
            agent,
            live.kind,
            &live.credentials,
        )? {
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
            if accounts_same_authorization(adapter, updated.kind, &updated.credentials, &other) {
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
        let _lock = self.acquire_live_lock(agent)?.ok_or_else(|| {
            AppError::Unsupported("account live switching is not configured".into())
        })?;

        let adapter = self.registry.require(agent, Capability::AccountSwitch)?;

        let target = self.get(id_or_label, Some(agent))?;
        let live_before = match adapter.read_account() {
            Ok(live) => Some(live),
            Err(err) if err.code() == "not_found" || err.code() == "unsupported" => None,
            Err(err) => return Err(err),
        };
        let current = self.repo.get_current(agent)?;

        let live_for_backfill = live_before
            .as_ref()
            .filter(|live| !live_account_is_empty(live));
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
            let msg = redact_text(&err.to_string());
            tracing::error!(
                module = targets::ACCOUNT,
                op,
                agent = agent.as_str(),
                code = err.code(),
                elapsed_ms,
                "{msg}"
            );
        }
    }
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

fn live_account_is_empty(live: &LiveAccount) -> bool {
    live.credentials
        .as_object()
        .map(|o| o.is_empty())
        .unwrap_or(true)
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
