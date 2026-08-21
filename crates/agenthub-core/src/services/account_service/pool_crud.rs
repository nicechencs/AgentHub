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
            if crate::services::account_identity_heal::heal_account_identity(item) {
                dirty = true;
            }
            if item.kind == AccountKind::Oauth
                && crate::services::account_quota::heal_token_expiry(item)
            {
                dirty = true;
            }
            // Tick quota countdown from absolute reset timestamps (no network).
            if item.kind == AccountKind::Oauth
                && crate::services::account_quota::refresh_quota_reset_label(item, Utc::now())
            {
                dirty = true;
            }
            // Only probe upstream quota for the active OAuth account to keep list snappy.
            if item.is_current
                && item.kind == AccountKind::Oauth
                && crate::services::account_quota::try_refresh_account_quota(item, false)
            {
                dirty = true;
            }
            if dirty {
                match self.persist_healed_fields(item, &expected_updated_at) {
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
    pub fn refresh_quota(&self, id_or_label: &str, agent: AgentId) -> Result<Account> {
        let mut account = self.get(id_or_label, Some(agent))?;
        if account.kind != AccountKind::Oauth {
            return Err(AppError::Unsupported(
                "quota refresh is only supported for OAuth accounts".into(),
            ));
        }
        let expected_updated_at = account.updated_at.clone();
        let mut dirty = crate::services::account_identity_heal::heal_account_identity(&mut account);
        if crate::services::account_quota::heal_token_expiry(&mut account) {
            dirty = true;
        }
        // Explicit refreshes are user-visible: propagate network, auth and
        // parsing failures instead of the list path's best-effort behavior.
        if crate::services::account_quota::refresh_account_quota(&mut account, true)? {
            dirty = true;
        }
        if !dirty {
            return Ok(account);
        }
        self.persist_healed_fields(&account, &expected_updated_at)
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

    pub(super) fn add_api_key_inner(
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
        let created = self.repo.create(&row)?;
        self.stamp_account_surface(created)
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

    pub(super) fn update_api_key_inner(
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

    pub(super) fn create_inner(&self, input: AccountInput) -> Result<Account> {
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
            self.stamp_account_surface(created)
        } else {
            let created = self.repo.create(&row)?;
            self.stamp_account_surface(created)
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

    pub(super) fn refresh_token_inner(&self, id_or_label: &str, agent: AgentId) -> Result<Account> {
        let mut account = self.get(id_or_label, Some(agent))?;
        if account.kind != AccountKind::Oauth {
            return Err(AppError::Unsupported(
                "token refresh is only supported for OAuth accounts".into(),
            ));
        }
        // CLI-owned grants rotate in the official auth.json. Hitting the token
        // endpoint here would invalidate the CLI's refresh token.
        self.refuse_cli_owned_oauth_refresh(&account)?;
        let refresh_lock = self.acquire_oauth_refresh_lock(&account.id);
        let _refresh_lock = refresh_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        account = self.get(id_or_label, Some(agent))?;
        self.refuse_cli_owned_oauth_refresh(&account)?;
        let expected_updated_at = account.updated_at.clone();

        // Heal first so Pi body.refresh is promoted to refresh_token.
        let _ = crate::services::account_identity_heal::heal_account_identity(&mut account);

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
                || crate::services::account_identity_heal::needs_identity_heal(&account)
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

        let _ = crate::services::account_quota::heal_token_expiry(&mut account);
        // Fresh access token → re-probe 5h/7d windows when supported.
        let _ = crate::services::account_quota::try_refresh_account_quota(&mut account, true);

        account.updated_at = now_ts();
        account.status = "active".into();
        if agent == AgentId::Pi {
            return self.persist_pi_oauth_account_update(&account, &expected_updated_at);
        }
        self.persist_healed_fields(&account, &expected_updated_at)
    }
}
