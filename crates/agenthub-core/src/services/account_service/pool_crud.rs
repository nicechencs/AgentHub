use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::adapters::AgentAdapter;
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{
    Account, AccountInput, AccountKind, AgentId, BackupKind, Capability, Provider,
};
use crate::services::ConnectionService;
use crate::storage::{
    account_get_by_id_conn, account_list_for_agent_conn, provider_get_by_id_conn,
    provider_list_for_agent_conn,
};
use crate::utils::loopback::credentials_are_loopback;
use crate::utils::redact::mask_secret_preview;

use super::live_reconcile::compensated_current_account_apply_error_with_db;
use super::surface::*;
use super::{AccountService, MAX_ACCOUNT_ID_LEN, MAX_ACCOUNT_LABEL_LEN};

#[derive(Clone, Debug, PartialEq, Eq)]
struct BindingRowSnapshot {
    agent_key: String,
    account_id: Option<String>,
    provider_id: Option<String>,
    model_id: Option<String>,
    config_profile_id: Option<String>,
    revision: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TrashRowSnapshot {
    id: String,
    agent_id: String,
    source_kind: String,
    source_id: String,
    label: String,
    was_current: i64,
    payload: String,
    deleted_at: String,
    expires_at: String,
}

#[derive(Clone, Default)]
struct AccountMutationFootprint {
    /// Exact ids this transaction owns. Compensation never infers ownership
    /// by scanning the rest of the agent pool.
    affected_account_ids: Vec<String>,
    before_accounts: Vec<Account>,
    after_accounts: Vec<Account>,
    before_providers: Vec<Provider>,
    after_providers: Vec<Provider>,
    before_binding: Option<BindingRowSnapshot>,
    after_binding: Option<BindingRowSnapshot>,
    before_trash: Vec<TrashRowSnapshot>,
    after_trash: Vec<TrashRowSnapshot>,
}

pub(super) struct AccountCommittedMutation {
    pub(super) stored: Account,
    pub(super) deleted: Vec<Account>,
    footprint: AccountMutationFootprint,
}

struct ApiKeyUpdatePayload {
    label: String,
    credentials: Option<Value>,
    extra: Value,
}

/// Distinguishes a rolled-back IMMEDIATE transaction from a committed one.
/// Compensation is allowed only after commit (live-apply / post-commit
/// failures). Pre-commit errors, including in-transaction CAS conflicts that
/// abort the transaction, must not restore stale extra-transaction snapshots.
#[derive(Debug)]
pub(super) struct AccountMutationError {
    error: AppError,
    #[allow(dead_code)]
    committed: bool,
}

impl AccountMutationError {
    pub(super) fn pre(error: AppError) -> Self {
        Self {
            error,
            committed: false,
        }
    }

    #[allow(dead_code)]
    pub(super) fn post(error: AppError) -> Self {
        Self {
            error,
            committed: true,
        }
    }

    pub(super) fn code(&self) -> &str {
        self.error.code()
    }

    pub(super) fn into_error(self) -> AppError {
        self.error
    }
}

impl From<AppError> for AccountMutationError {
    fn from(error: AppError) -> Self {
        Self::pre(error)
    }
}

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
        self.add_api_key_with_env_and_marker(agent, label, api_key, env_key, None)
    }

    /// Add an API Key account with an explicit product marker. The marker is
    /// optional for backward compatibility; the GUI supplies it for official
    /// Anthropic/OpenAI/xAI and Kimi Code/API products.
    pub fn add_api_key_with_env_and_marker(
        &self,
        agent: AgentId,
        label: Option<&str>,
        api_key: &str,
        env_key: Option<&str>,
        product_marker: Option<&str>,
    ) -> Result<Account> {
        let started = Instant::now();
        let result = self.add_api_key_inner(agent, label, api_key, env_key, product_marker);
        log_account_op("add_api_key", agent, started, &result);
        result
    }

    pub(super) fn add_api_key_inner(
        &self,
        agent: AgentId,
        label: Option<&str>,
        api_key: &str,
        env_key: Option<&str>,
        product_marker: Option<&str>,
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

        let mut extra = attach_identity_meta(
            adapter.as_ref(),
            AccountKind::ApiKey,
            &credentials,
            &display,
            live.extra,
        );
        if let Some(marker) = product_marker
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            validate_api_key_product_marker(agent, marker)?;
            if let Some(obj) = extra.as_object_mut() {
                obj.insert("provider".into(), json!(marker));
            }
        }

        let now = now_ts();
        let row = Account {
            id: format!("{}-acc-{}", agent.as_str(), Uuid::new_v4()),
            agent_id: agent,
            kind: AccountKind::ApiKey,
            label: display.clone(),
            credentials: credentials.clone(),
            extra: extra.clone(),
            status: "active".into(),
            is_current: false,
            created_at: now.clone(),
            updated_at: now,
        };
        let row = self.prepare_account_surface(row);
        self.commit_authorization_merge(
            adapter.as_ref(),
            &row,
            AccountKind::ApiKey,
            display,
            credentials,
            extra,
            false,
        )
        .map(|committed| committed.stored)
        .map_err(AccountMutationError::into_error)
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
            let key_changed = api_key
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());

            // Keep the same per-agent lock across the live snapshot, DB write,
            // live apply and any compensation. The old implementation only
            // acquired it after the DB mutation, allowing another process to
            // observe a half-committed account rotation.
            let _live_lock = if key_changed {
                self.acquire_live_lock(agent)?
            } else {
                None
            };
            let before = self.get(id_or_label, Some(agent))?;
            if before.kind != AccountKind::ApiKey {
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
            let payload = self.materialize_api_key_update(&adapter, &before, new_label, new_key.as_deref())?;

            let live_saga = if before.is_current && key_changed {
                let backup = self.backup.as_ref();
                adapter
                    .capability(Capability::AccountSwitch)
                    .is_usable()
                    .then_some(())
                    .and(backup)
                    .map(|backup| (adapter.clone(), backup))
            } else {
                None
            };
            let live_before = if let Some((adapter, backup)) = live_saga.as_ref() {
                let live_before = match adapter.read_account() {
                    Ok(live) => Some(live),
                    Err(error) if error.code() == "not_found" => None,
                    Err(error) => return Err(error),
                };
                if let Err(error) = backup.snapshot(
                    agent,
                    BackupKind::AutoSwitch,
                    Some(&format!("before applying current account {}", before.id)),
                ) {
                    if error.code() != "not_found" {
                        return Err(error);
                    }
                }
                Some((adapter.clone(), live_before))
            } else {
                None
            };

            // Adapter/materialization failures above never compensate. The
            // IMMEDIATE transaction below either commits a precise footprint
            // or rolls back; its errors are therefore also pre-commit.
            let committed = match self.commit_api_key_update(
                adapter.as_ref(),
                agent,
                &before.id,
                &before.updated_at,
                &payload,
            ) {
                Ok(committed) => committed,
                Err(progress) => return Err(progress.into_error()),
            };
            if let Some((adapter, live_before)) = live_before {
                let apply_live = committed.stored.to_live();
                if live_before
                    .as_ref()
                    .is_some_and(|before| before.credentials == apply_live.credentials)
                {
                    return Ok(committed.stored);
                }
                if let Err(error) = adapter.apply_account(&apply_live) {
                    // Keep the established pool-only behavior for adapters
                    // that can store a key but cannot apply it to live files.
                    if error.code() == "unsupported" {
                        self.snapshot_after_pool_change(agent, "after API Key account update");
                        return Ok(committed.stored);
                    }
                    let live_rollback = live_before
                        .as_ref()
                        .and_then(|before| adapter.apply_account(before).err());
                    let db_rollback = self
                        .restore_committed_account_mutation(agent, &committed)
                        .err();
                    return Err(compensated_current_account_apply_error_with_db(
                        error,
                        live_rollback,
                        db_rollback,
                    ));
                }
            } else {
                self.sync_current_account_live(
                    &committed.stored,
                    api_key,
                    "after API Key account update",
                )?;
            }
            Ok(committed.stored)
        })();
        log_account_op("update_api_key", agent, started, &result);
        result
    }

    fn materialize_api_key_update(
        &self,
        adapter: &std::sync::Arc<dyn AgentAdapter>,
        account: &Account,
        new_label: Option<String>,
        new_key: Option<&str>,
    ) -> Result<ApiKeyUpdatePayload> {
        if let Some(key) = new_key {
            let live = adapter.build_api_key_account(key)?;
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
            let display = new_label.unwrap_or_else(|| {
                live.label_hint
                    .clone()
                    .unwrap_or_else(|| format!("{} (API Key)", mask_secret_preview(key)))
            });
            validate_label(&display, "account label", MAX_ACCOUNT_LABEL_LEN)?;
            let mut extra = attach_identity_meta(
                adapter.as_ref(),
                AccountKind::ApiKey,
                &creds,
                &display,
                live.extra,
            );
            if let Some(provider) = account.extra.get("provider").cloned() {
                if let Some(obj) = extra.as_object_mut() {
                    obj.entry("provider").or_insert(provider);
                }
            }
            Ok(ApiKeyUpdatePayload {
                label: display,
                credentials: Some(creds),
                extra,
            })
        } else {
            let display = new_label.ok_or_else(|| {
                AppError::InvalidArg(
                    "update_api_key requires a non-empty label and/or api_key".into(),
                )
            })?;
            validate_label(&display, "account label", MAX_ACCOUNT_LABEL_LEN)?;
            let extra = attach_identity_meta(
                adapter.as_ref(),
                AccountKind::ApiKey,
                &account.credentials,
                &display,
                account.extra.clone(),
            );
            Ok(ApiKeyUpdatePayload {
                label: display,
                credentials: None,
                extra,
            })
        }
    }

    /// One IMMEDIATE transaction: snapshot the agent pool, decide source /
    /// target / leftover duplicates from that snapshot, mutate those exact
    /// ids with expected-revision CAS, and return the precise before/after
    /// footprint. The transaction never re-lists the pool to guess leftovers.
    fn commit_api_key_update(
        &self,
        adapter: &dyn AgentAdapter,
        agent: AgentId,
        source_id: &str,
        expected_source_updated_at: &str,
        payload: &ApiKeyUpdatePayload,
    ) -> std::result::Result<AccountCommittedMutation, AccountMutationError> {
        self.db
            .with_conn(|conn| {
                let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
                let now = now_ts();
                let accounts = account_list_for_agent_conn(&tx, agent)?;
                let providers = provider_list_for_agent_conn(&tx, agent)?;
                let binding = get_binding_row(&tx, agent)?;
                let trash = list_trash_conn(&tx, agent)?;

                let source = accounts
                    .iter()
                    .find(|row| row.id == source_id)
                    .cloned()
                    .ok_or_else(|| AppError::NotFound(format!("account not found: {source_id}")))?;
                if source.updated_at != expected_source_updated_at {
                    return Err(AppError::message(
                        "account.merge.conflict",
                        "account changed before API key merge",
                    ));
                }
                if source.kind != AccountKind::ApiKey {
                    return Err(AppError::InvalidArg(
                        "only API Key accounts can be updated via update_api_key".into(),
                    ));
                }

                let mut next = source.clone();
                next.label = payload.label.clone();
                if let Some(credentials) = &payload.credentials {
                    next.credentials = credentials.clone();
                }
                let mut extra = payload.extra.clone();
                Self::copy_persisted_surface(&source.extra, &mut extra);
                next.extra = extra;
                next.status = "active".into();
                next.updated_at = now.clone();
                next = self.prepare_account_surface(next);

                let duplicates = if payload.credentials.is_some() {
                    authorization_duplicates(
                        adapter,
                        agent,
                        AccountKind::ApiKey,
                        &next.credentials,
                        &accounts,
                    )
                    .into_iter()
                    .filter(|row| row.id != source.id)
                    .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };

                let mark_current = source.is_current;
                let committed = if let Some(target_existing) =
                    pick_primary_authorization_match(duplicates.clone())
                {
                    next.id = target_existing.id.clone();
                    next.created_at = target_existing.created_at.clone();
                    next.is_current = mark_current || target_existing.is_current;
                    Self::copy_persisted_surface(&target_existing.extra, &mut next.extra);
                    next = self.prepare_account_surface(next);
                    let mut deletes = duplicates
                        .into_iter()
                        .filter(|row| row.id != target_existing.id)
                        .collect::<Vec<_>>();
                    if !deletes.iter().any(|row| row.id == source.id) {
                        deletes.push(source.clone());
                    }
                    apply_frozen_account_plan(
                        &self.connections,
                        &tx,
                        agent,
                        &next,
                        &target_existing,
                        &deletes,
                        &source.id,
                        next.is_current,
                        &now,
                        &accounts,
                        &providers,
                        &binding,
                        &trash,
                    )?
                } else {
                    next.is_current = source.is_current;
                    apply_frozen_account_plan(
                        &self.connections,
                        &tx,
                        agent,
                        &next,
                        &source,
                        &[],
                        &source.id,
                        next.is_current,
                        &now,
                        &accounts,
                        &providers,
                        &binding,
                        &trash,
                    )?
                };
                tx.commit()?;
                Ok(committed)
            })
            .map_err(AccountMutationError::pre)
    }

    /// Snapshot the agent pool, decide insert vs merge from that snapshot, and
    /// commit inside one IMMEDIATE transaction. Duplicate membership is never
    /// taken from an outer stale `existing.id`.
    pub(super) fn commit_authorization_merge(
        &self,
        adapter: &dyn AgentAdapter,
        incoming: &Account,
        kind: AccountKind,
        label: String,
        credentials: Value,
        extra: Value,
        mark_current: bool,
    ) -> std::result::Result<AccountCommittedMutation, AccountMutationError> {
        self.db
            .with_conn(|conn| {
                let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
                let now = now_ts();
                let agent = incoming.agent_id;
                let accounts = account_list_for_agent_conn(&tx, agent)?;
                let providers = provider_list_for_agent_conn(&tx, agent)?;
                let binding = get_binding_row(&tx, agent)?;
                let trash = list_trash_conn(&tx, agent)?;

                let duplicates = authorization_duplicates(
                    adapter,
                    agent,
                    kind,
                    &credentials,
                    &accounts,
                );
                let committed = if let Some(target_existing) =
                    pick_primary_authorization_match(duplicates.clone())
                {
                    let deletes = duplicates
                        .into_iter()
                        .filter(|row| row.id != target_existing.id)
                        .collect::<Vec<_>>();

                    let mut merged_extra = extra;
                    Self::copy_persisted_surface(&target_existing.extra, &mut merged_extra);
                    let mut next = target_existing.clone();
                    next.kind = kind;
                    next.label = label;
                    next.credentials = credentials;
                    next.extra = merged_extra;
                    next.status = "active".into();
                    next.updated_at = now.clone();
                    if mark_current {
                        next.is_current = true;
                    }
                    next = self.prepare_account_surface(next);

                    apply_frozen_account_plan(
                        &self.connections,
                        &tx,
                        agent,
                        &next,
                        &target_existing,
                        &deletes,
                        &target_existing.id,
                        next.is_current,
                        &now,
                        &accounts,
                        &providers,
                        &binding,
                        &trash,
                    )?
                } else {
                    let mut next = incoming.clone();
                    next.kind = kind;
                    next.label = label;
                    next.credentials = credentials;
                    next.extra = extra;
                    next.status = "active".into();
                    next.updated_at = now.clone();
                    next.is_current = mark_current;
                    next = self.prepare_account_surface(next);
                    apply_frozen_account_insert(
                        &self.connections,
                        &tx,
                        agent,
                        &next,
                        mark_current,
                        &accounts,
                        &providers,
                        &binding,
                        &trash,
                    )?
                };
                tx.commit()?;
                Ok(committed)
            })
            .map_err(AccountMutationError::pre)
    }

    /// Restore only the precise committed footprint after a live apply fails.
    /// Live database rows must still match the post-commit expected state;
    /// any concurrent change fails closed.
    fn restore_committed_account_mutation(
        &self,
        agent: AgentId,
        committed: &AccountCommittedMutation,
    ) -> Result<()> {
        self.restore_account_rows_with_footprint(
            agent,
            &committed.footprint.before_accounts,
            &committed.footprint.after_accounts,
            &committed.stored,
            &committed.deleted,
            &committed.footprint,
            &committed.footprint.after_binding,
            &committed.footprint.after_trash,
        )
    }

    pub(super) fn restore_account_rows(
        &self,
        agent: AgentId,
        before: &[Account],
        after: &[Account],
        stored: &Account,
        deleted_rows: &[Account],
    ) -> Result<()> {
        let mut affected_account_ids = before.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        if !affected_account_ids.iter().any(|id| id == &stored.id) {
            affected_account_ids.push(stored.id.clone());
        }
        let footprint = AccountMutationFootprint {
            affected_account_ids,
            before_accounts: before.to_vec(),
            after_accounts: after.to_vec(),
            ..AccountMutationFootprint::default()
        };
        self.restore_account_rows_with_footprint(
            agent,
            before,
            after,
            stored,
            deleted_rows,
            &footprint,
            &None,
            &[],
        )
    }

    fn restore_account_rows_with_footprint(
        &self,
        agent: AgentId,
        before: &[Account],
        _after: &[Account],
        stored: &Account,
        deleted_rows: &[Account],
        footprint: &AccountMutationFootprint,
        after_binding: &Option<BindingRowSnapshot>,
        after_trash: &[TrashRowSnapshot],
    ) -> Result<()> {
        // Restore the surviving merge target first. If the deleted source was
        // current, inserting it while the target is still current can violate
        // the active-row invariant in callers that enforce it.
        let mut affected_ids = if footprint.affected_account_ids.is_empty() {
            let mut ids = vec![stored.id.clone()];
            for row in deleted_rows {
                if !ids.iter().any(|id| id == &row.id) {
                    ids.push(row.id.clone());
                }
            }
            ids
        } else {
            footprint.affected_account_ids.clone()
        };
        // A current write may demote the previously-current row(s). Those
        // rows are part of this mutation even when the target was an upsert.
        for row in before.iter().filter(|row| {
            row.is_current
                && (footprint.affected_account_ids.is_empty()
                    || footprint.affected_account_ids.iter().any(|id| id == &row.id))
        }) {
            if !affected_ids.iter().any(|id| id == &row.id) {
                affected_ids.push(row.id.clone());
            }
        }

        if affected_ids.is_empty() {
            return Ok(());
        }

        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            for id in affected_ids {
                let original = before.iter().find(|row| row.id == id);
                let explicitly_deleted = deleted_rows.iter().any(|row| row.id == id);

                match original {
                    Some(original) if explicitly_deleted => {
                        if get_account_row(&tx, &original.id)?.is_some() {
                            return Err(account_compensation_conflict(&original.id));
                        }
                        let credentials = serde_json::to_string(&original.credentials)?;
                        let extra = serde_json::to_string(&original.extra)?;
                        tx.execute(
                            r#"
                            INSERT INTO accounts (
                                id, agent_id, kind, label, credentials, extra,
                                status, is_current, created_at, updated_at
                            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                            "#,
                            params![
                                &original.id,
                                original.agent_id.as_str(),
                                original.kind.as_str(),
                                &original.label,
                                credentials,
                                extra,
                                &original.status,
                                if original.is_current { 1 } else { 0 },
                                &original.created_at,
                                &original.updated_at,
                            ],
                        )?;
                    }
                    Some(original) => {
                        // The target row is fully described by `stored`. A
                        // pre-existing current row is only demoted, so its
                        // pre-mutation fields must otherwise remain intact.
                        let expected = footprint
                            .after_accounts
                            .iter()
                            .find(|row| row.id == original.id)
                            .cloned()
                            .unwrap_or_else(|| {
                                if original.id == stored.id {
                                    stored.clone()
                                } else {
                                    let mut demoted = original.clone();
                                    demoted.is_current = false;
                                    demoted
                                }
                            });
                        if original == &expected {
                            continue;
                        }
                        ensure_account_row_matches(&tx, &expected)?;
                        let credentials = serde_json::to_string(&original.credentials)?;
                        let extra = serde_json::to_string(&original.extra)?;
                        let updated = tx.execute(
                            r#"
                            UPDATE accounts SET
                                agent_id = ?2, kind = ?3, label = ?4, credentials = ?5,
                                extra = ?6, status = ?7, is_current = ?8,
                                created_at = ?9, updated_at = ?10
                            WHERE id = ?1 AND agent_id = ?11 AND updated_at = ?12
                            "#,
                            params![
                                &original.id,
                                original.agent_id.as_str(),
                                original.kind.as_str(),
                                &original.label,
                                credentials,
                                extra,
                                &original.status,
                                if original.is_current { 1 } else { 0 },
                                &original.created_at,
                                &original.updated_at,
                                agent.as_str(),
                                &expected.updated_at,
                            ],
                        )?;
                        if updated != 1 {
                            return Err(account_compensation_conflict(&original.id));
                        }
                    }
                    None => {
                        // Account update/merge never creates a row. A row
                        // appearing for an affected id is an external write.
                        return Err(account_compensation_conflict(&id));
                    }
                }
            }
            let expected_after_binding = if footprint.after_binding.is_some()
                || footprint.before_binding.is_some()
            {
                &footprint.after_binding
            } else {
                after_binding
            };
            let binding_changed =
                footprint.before_binding.as_ref() != expected_after_binding.as_ref();
            if binding_changed && !footprint.before_providers.is_empty() {
                if footprint.after_providers.is_empty() {
                    restore_demoted_provider_rows(&tx, agent, &footprint.before_providers)?;
                } else {
                    restore_provider_rows_from_footprint(
                        &tx,
                        agent,
                        &footprint.before_providers,
                        &footprint.after_providers,
                    )?;
                }
            }
            if binding_changed {
                restore_account_binding(
                    &tx,
                    agent,
                    stored,
                    footprint.before_binding.as_ref(),
                    expected_after_binding.as_ref(),
                )?;
            }
            let expected_after_trash = if footprint.after_trash.is_empty() {
                after_trash
            } else {
                footprint.after_trash.as_slice()
            };
            if footprint.before_trash != expected_after_trash {
                remove_new_trash_rows(
                    &tx,
                    agent,
                    &footprint.before_trash,
                    expected_after_trash,
                    deleted_rows,
                )?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    pub(super) fn update_api_key_inner(
        &self,
        agent: AgentId,
        id_or_label: &str,
        label: Option<&str>,
        api_key: Option<&str>,
        expected_source_updated_at: &str,
    ) -> std::result::Result<(Account, Vec<Account>), AccountMutationError> {
        let account = self.get(id_or_label, Some(agent))?;
        if account.kind != AccountKind::ApiKey {
            return Err(AppError::InvalidArg(
                "only API Key accounts can be updated via update_api_key".into(),
            )
            .into());
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
            )
            .into());
        }
        let adapter = self.adapter(agent)?;
        let payload = self.materialize_api_key_update(
            &adapter,
            &account,
            new_label,
            new_key.as_deref(),
        )?;
        self.commit_api_key_update(
            adapter.as_ref(),
            agent,
            &account.id,
            expected_source_updated_at,
            &payload,
        )
        .map(|committed| (committed.stored, committed.deleted))
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

        let now = now_ts();
        let row = Account {
            id: format!("{}-acc-{}", input.agent_id.as_str(), Uuid::new_v4()),
            agent_id: input.agent_id,
            kind: input.kind,
            label: label.clone(),
            credentials: input.credentials.clone(),
            extra: extra.clone(),
            status: "active".into(),
            is_current: input.is_current,
            created_at: now.clone(),
            updated_at: now,
        };
        let row = self.prepare_account_surface(row);
        if let Some(ref ad) = adapter {
            return self
                .commit_authorization_merge(
                    ad.as_ref(),
                    &row,
                    input.kind,
                    label,
                    input.credentials,
                    extra,
                    input.is_current,
                )
                .map(|committed| committed.stored)
                .map_err(AccountMutationError::into_error);
        }
        if row.is_current {
            let (created, _binding) = self.connections.create_and_activate_account(&row)?;
            Ok(created)
        } else {
            let created = self.repo.create(&row)?;
            Ok(created)
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
        account = self.prepare_account_surface(account);
        let adapter = self.adapter(agent).ok();
        self.persist_refreshed_account(
            adapter.as_deref(),
            account,
            &expected_updated_at,
            agent == AgentId::Pi,
        )
    }

    fn persist_refreshed_account(
        &self,
        adapter: Option<&dyn AgentAdapter>,
        account: Account,
        expected_updated_at: &str,
        pi: bool,
    ) -> Result<Account> {
        let intended = account.clone();
        let persisted = if pi {
            self.persist_pi_oauth_account_update(&account, expected_updated_at)?
        } else {
            self.persist_healed_fields(&account, expected_updated_at)?
        };
        if persisted.credentials == intended.credentials {
            return Ok(persisted);
        }
        let same_grant = adapter.is_some_and(|adapter| {
            accounts_same_authorization(adapter, intended.kind, &intended.credentials, &persisted)
        });
        if !same_grant {
            return Ok(persisted);
        }
        let expected = persisted.updated_at.clone();
        let mut extra = intended.extra;
        Self::copy_persisted_surface(&persisted.extra, &mut extra);
        let mut retry = persisted;
        retry.credentials = intended.credentials;
        retry.label = intended.label;
        retry.extra = extra;
        retry.status = "active".into();
        retry = self.prepare_account_surface(retry);
        if pi {
            self.persist_pi_oauth_account_update(&retry, &expected)
        } else {
            self.persist_healed_fields(&retry, &expected)
        }
    }
}

fn authorization_duplicates(
    adapter: &dyn AgentAdapter,
    agent: AgentId,
    kind: AccountKind,
    credentials: &Value,
    snapshot: &[Account],
) -> Vec<Account> {
    let incoming_loopback = credentials_are_loopback(credentials);
    snapshot
        .iter()
        .filter(|candidate| {
            candidate.kind == kind
                && same_live_slot(agent, credentials, &candidate.credentials)
                && if incoming_loopback {
                    credentials_are_loopback(&candidate.credentials)
                } else {
                    !credentials_are_loopback(&candidate.credentials)
                        && accounts_same_authorization(adapter, kind, credentials, candidate)
                }
        })
        .cloned()
        .collect()
}

fn freeze_account_mutation_plan(
    tx: &Transaction<'_>,
    items: &[(String, String, String)],
) -> Result<()> {
    tx.execute_batch(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS account_mutation_plan (
            role TEXT NOT NULL,
            id TEXT NOT NULL,
            expected_updated_at TEXT NOT NULL
        );
        DELETE FROM account_mutation_plan;
        "#,
    )?;
    for (role, id, expected) in items {
        tx.execute(
            "INSERT INTO account_mutation_plan (role, id, expected_updated_at) VALUES (?1, ?2, ?3)",
            params![role, id, expected],
        )?;
    }
    Ok(())
}

fn revalidate_frozen_account_plan(
    conn: &Connection,
    items: &[(String, String, String)],
) -> Result<()> {
    for (_role, id, expected) in items {
        let live = account_get_by_id_conn(conn, id)?.ok_or_else(|| {
            AppError::NotFound(format!("account not found: {id}"))
        })?;
        if live.updated_at != *expected {
            return Err(AppError::message(
                "account.merge.conflict",
                format!("account {id} changed after merge snapshot"),
            ));
        }
    }
    Ok(())
}

fn apply_frozen_account_plan(
    connections: &ConnectionService,
    tx: &Transaction<'_>,
    agent: AgentId,
    next: &Account,
    target_before: &Account,
    deletes: &[Account],
    source_id: &str,
    mark_current: bool,
    now: &str,
    snapshot_accounts: &[Account],
    snapshot_providers: &[Provider],
    snapshot_binding: &Option<BindingRowSnapshot>,
    snapshot_trash: &[TrashRowSnapshot],
) -> Result<AccountCommittedMutation> {
    let mut items = vec![(
        "target".to_string(),
        target_before.id.clone(),
        target_before.updated_at.clone(),
    )];
    for row in deletes {
        if row.id == target_before.id {
            continue;
        }
        let role = if row.id == source_id {
            "source"
        } else {
            "extra"
        };
        items.push((role.to_string(), row.id.clone(), row.updated_at.clone()));
    }
    freeze_account_mutation_plan(tx, &items)?;
    revalidate_frozen_account_plan(tx, &items)?;

    let stored = if mark_current {
        connections
            .activate_account_if_revision_conn(tx, next, &target_before.updated_at)?
            .0
    } else {
        connections.update_account_if_revision_conn(tx, next, &target_before.updated_at)?
    };

    let mut deleted = Vec::new();
    let mut source_deletes = Vec::new();
    let mut extra_deletes = Vec::new();
    for row in deletes {
        if row.id == stored.id {
            continue;
        }
        if row.id == source_id {
            source_deletes.push(row);
        } else {
            extra_deletes.push(row);
        }
    }
    for row in source_deletes.into_iter().chain(extra_deletes) {
        connections.trash_delete_account_if_revision_conn(
            tx,
            &row.id,
            agent,
            &row.updated_at,
            now,
        )?;
        deleted.push(row.clone());
    }

    let mut affected_account_ids = vec![target_before.id.clone()];
    for row in &deleted {
        if !affected_account_ids.iter().any(|id| id == &row.id) {
            affected_account_ids.push(row.id.clone());
        }
    }
    if mark_current {
        for row in snapshot_accounts.iter().filter(|row| row.is_current) {
            if !affected_account_ids.iter().any(|id| id == &row.id) {
                affected_account_ids.push(row.id.clone());
            }
        }
    }

    let before_accounts = affected_account_ids
        .iter()
        .filter_map(|id| snapshot_accounts.iter().find(|row| row.id == *id).cloned())
        .collect::<Vec<_>>();
    let after_accounts = affected_account_ids
        .iter()
        .filter_map(|id| account_get_by_id_conn(tx, id).ok().flatten())
        .collect::<Vec<_>>();
    let before_providers = if mark_current {
        snapshot_providers
            .iter()
            .filter(|row| row.is_current)
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let after_providers = before_providers
        .iter()
        .filter_map(|row| provider_get_by_id_conn(tx, &row.id).ok().flatten())
        .collect::<Vec<_>>();
    let after_binding = get_binding_row(tx, agent)?;
    let after_trash = list_trash_conn(tx, agent)?;

    Ok(AccountCommittedMutation {
        stored,
        deleted,
        footprint: AccountMutationFootprint {
            affected_account_ids,
            before_accounts,
            after_accounts,
            before_providers,
            after_providers,
            before_binding: snapshot_binding.clone(),
            after_binding,
            before_trash: snapshot_trash.to_vec(),
            after_trash,
        },
    })
}

fn apply_frozen_account_insert(
    connections: &ConnectionService,
    tx: &Transaction<'_>,
    agent: AgentId,
    next: &Account,
    mark_current: bool,
    snapshot_accounts: &[Account],
    snapshot_providers: &[Provider],
    snapshot_binding: &Option<BindingRowSnapshot>,
    snapshot_trash: &[TrashRowSnapshot],
) -> Result<AccountCommittedMutation> {
    let stored = if mark_current {
        connections.create_and_activate_account_conn(tx, next)?.0
    } else {
        connections.create_account_conn(tx, next)?
    };

    let mut affected_account_ids = vec![stored.id.clone()];
    if mark_current {
        for row in snapshot_accounts.iter().filter(|row| row.is_current) {
            if !affected_account_ids.iter().any(|id| id == &row.id) {
                affected_account_ids.push(row.id.clone());
            }
        }
    }
    let before_accounts = affected_account_ids
        .iter()
        .filter_map(|id| snapshot_accounts.iter().find(|row| row.id == *id).cloned())
        .collect::<Vec<_>>();
    let after_accounts = affected_account_ids
        .iter()
        .filter_map(|id| account_get_by_id_conn(tx, id).ok().flatten())
        .collect::<Vec<_>>();
    let before_providers = if mark_current {
        snapshot_providers
            .iter()
            .filter(|row| row.is_current)
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let after_providers = before_providers
        .iter()
        .filter_map(|row| provider_get_by_id_conn(tx, &row.id).ok().flatten())
        .collect::<Vec<_>>();
    let after_binding = get_binding_row(tx, agent)?;
    let after_trash = list_trash_conn(tx, agent)?;

    Ok(AccountCommittedMutation {
        stored,
        deleted: Vec::new(),
        footprint: AccountMutationFootprint {
            affected_account_ids,
            before_accounts,
            after_accounts,
            before_providers,
            after_providers,
            before_binding: snapshot_binding.clone(),
            after_binding,
            before_trash: snapshot_trash.to_vec(),
            after_trash,
        },
    })
}



fn list_trash_conn(conn: &Connection, agent: AgentId) -> Result<Vec<TrashRowSnapshot>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, agent_id, source_kind, source_id, label, was_current,
               payload, deleted_at, expires_at
        FROM connection_trash WHERE agent_id = ?1
        "#,
    )?;
    let rows = stmt.query_map(params![agent.as_str()], |row| {
        Ok(TrashRowSnapshot {
            id: row.get(0)?,
            agent_id: row.get(1)?,
            source_kind: row.get(2)?,
            source_id: row.get(3)?,
            label: row.get(4)?,
            was_current: row.get(5)?,
            payload: row.get(6)?,
            deleted_at: row.get(7)?,
            expires_at: row.get(8)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(AppError::from)
}

fn restore_provider_rows_from_footprint(
    conn: &Connection,
    agent: AgentId,
    before: &[Provider],
    after: &[Provider],
) -> Result<()> {
    for original in before {
        let expected = after.iter().find(|row| row.id == original.id).cloned();
        match expected {
            Some(expected) => {
                if original == &expected {
                    continue;
                }
                ensure_provider_row_matches_for_compensation(conn, &expected)?;
                let settings = serde_json::to_string(&original.settings_config)?;
                let meta = serde_json::to_string(&original.meta)?;
                let restored = conn.execute(
                    r#"
                    UPDATE providers SET
                        agent_id = ?2, name = ?3, settings_config = ?4,
                        meta = ?5, is_current = ?6, created_at = ?7,
                        updated_at = ?8
                    WHERE id = ?1 AND agent_id = ?9 AND updated_at = ?10
                    "#,
                    params![
                        &original.id,
                        original.agent_id.as_str(),
                        &original.name,
                        settings,
                        meta,
                        if original.is_current { 1 } else { 0 },
                        &original.created_at,
                        &original.updated_at,
                        agent.as_str(),
                        &expected.updated_at,
                    ],
                )?;
                if restored != 1 {
                    return Err(account_compensation_conflict(&original.id));
                }
            }
            None => return Err(account_compensation_conflict(&original.id)),
        }
    }
    Ok(())
}

type AccountRowSnapshot = (
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
);

fn get_account_row(conn: &Connection, id: &str) -> Result<Option<AccountRowSnapshot>> {
    conn.query_row(
        r#"
        SELECT agent_id, kind, label, credentials, extra, status,
               is_current, created_at, updated_at
        FROM accounts WHERE id = ?1
        "#,
        params![id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
            ))
        },
    )
    .optional()
    .map_err(AppError::from)
}

fn ensure_account_row_matches(
    conn: &Connection,
    expected: &Account,
) -> Result<()> {
    let actual = get_account_row(conn, &expected.id)?;
    let credentials = serde_json::to_string(&expected.credentials)?;
    let extra = serde_json::to_string(&expected.extra)?;
    let matches = actual == Some((
        expected.agent_id.as_str().to_string(),
        expected.kind.as_str().to_string(),
        expected.label.clone(),
        credentials,
        extra,
        expected.status.clone(),
        if expected.is_current { 1 } else { 0 },
        expected.created_at.clone(),
        expected.updated_at.clone(),
    ));
    if matches {
        Ok(())
    } else {
        Err(account_compensation_conflict(&expected.id))
    }
}

fn account_compensation_conflict(id: &str) -> AppError {
    AppError::message(
        "account.current.apply.rollback.database",
        format!("account compensation CAS failed for {id}; database changed concurrently"),
    )
}

type ProviderRowSnapshot = (
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
    String,
);

fn get_provider_row_for_compensation(
    conn: &Connection,
    id: &str,
) -> Result<Option<ProviderRowSnapshot>> {
    conn.query_row(
        r#"
        SELECT agent_id, name, settings_config, meta,
               is_current, created_at, updated_at, id
        FROM providers WHERE id = ?1
        "#,
        params![id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        },
    )
    .optional()
    .map_err(AppError::from)
}

fn ensure_provider_row_matches_for_compensation(
    conn: &Connection,
    expected: &Provider,
) -> Result<()> {
    let actual = get_provider_row_for_compensation(conn, &expected.id)?;
    let settings = serde_json::to_string(&expected.settings_config)?;
    let meta = serde_json::to_string(&expected.meta)?;
    let matches = actual == Some((
        expected.agent_id.as_str().to_string(),
        expected.name.clone(),
        settings,
        meta,
        if expected.is_current { 1 } else { 0 },
        expected.created_at.clone(),
        expected.updated_at.clone(),
        expected.id.clone(),
    ));
    if matches {
        Ok(())
    } else {
        Err(account_compensation_conflict(&expected.id))
    }
}

fn restore_demoted_provider_rows(
    conn: &Connection,
    agent: AgentId,
    before: &[Provider],
) -> Result<()> {
    for original in before.iter().filter(|row| row.is_current) {
        let mut expected = original.clone();
        expected.is_current = false;
        ensure_provider_row_matches_for_compensation(conn, &expected)?;
        let settings = serde_json::to_string(&original.settings_config)?;
        let meta = serde_json::to_string(&original.meta)?;
        let restored = conn.execute(
            r#"
            UPDATE providers SET
                agent_id = ?2, name = ?3, settings_config = ?4,
                meta = ?5, is_current = ?6, created_at = ?7,
                updated_at = ?8
            WHERE id = ?1 AND agent_id = ?9 AND updated_at = ?10
            "#,
            params![
                &original.id,
                original.agent_id.as_str(),
                &original.name,
                settings,
                meta,
                if original.is_current { 1 } else { 0 },
                &original.created_at,
                &original.updated_at,
                agent.as_str(),
                &expected.updated_at,
            ],
        )?;
        if restored != 1 {
            return Err(account_compensation_conflict(&original.id));
        }
    }
    Ok(())
}

fn get_binding_row(conn: &Connection, agent: AgentId) -> Result<Option<BindingRowSnapshot>> {
    let key = crate::platform::AgentKey::from_agent_id(agent).into_string();
    conn.query_row(
        r#"
        SELECT agent_key, account_id, provider_id, model_id, config_profile_id,
               revision, created_at, updated_at
        FROM agent_active_bindings WHERE agent_key = ?1
        "#,
        params![key],
        |row| {
            Ok(BindingRowSnapshot {
                agent_key: row.get(0)?,
                account_id: row.get(1)?,
                provider_id: row.get(2)?,
                model_id: row.get(3)?,
                config_profile_id: row.get(4)?,
                revision: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(AppError::from)
}

fn restore_account_binding(
    conn: &Connection,
    agent: AgentId,
    stored: &Account,
    before: Option<&BindingRowSnapshot>,
    after: Option<&BindingRowSnapshot>,
) -> Result<()> {
    let Some(after) = after else {
        return Err(account_compensation_conflict(stored.id.as_str()));
    };
    let expected_revision = before.map(|row| row.revision + 1).unwrap_or(1);
    if after.revision != expected_revision
        || after.account_id.as_deref() != Some(stored.id.as_str())
        || after.provider_id.is_some()
    {
        return Err(account_compensation_conflict(stored.id.as_str()));
    }
    if get_binding_row(conn, agent)?.as_ref() != Some(after) {
        return Err(account_compensation_conflict(stored.id.as_str()));
    }

    if let Some(original) = before {
        let changed = conn.execute(
            r#"
            UPDATE agent_active_bindings SET
                account_id = ?2, provider_id = ?3, model_id = ?4,
                config_profile_id = ?5, revision = ?6, created_at = ?7,
                updated_at = ?8
            WHERE agent_key = ?1 AND revision = ?9 AND updated_at = ?10
            "#,
            params![
                &original.agent_key,
                &original.account_id,
                &original.provider_id,
                &original.model_id,
                &original.config_profile_id,
                original.revision,
                &original.created_at,
                &original.updated_at,
                after.revision,
                &after.updated_at,
            ],
        )?;
        if changed != 1 {
            return Err(account_compensation_conflict(stored.id.as_str()));
        }
    } else {
        let removed = conn.execute(
            "DELETE FROM agent_active_bindings WHERE agent_key = ?1 AND revision = ?2 AND updated_at = ?3",
            params![&after.agent_key, after.revision, &after.updated_at],
        )?;
        if removed != 1 {
            return Err(account_compensation_conflict(stored.id.as_str()));
        }
    }
    Ok(())
}

fn get_trash_row(conn: &Connection, id: &str) -> Result<Option<TrashRowSnapshot>> {
    conn.query_row(
        r#"
        SELECT id, agent_id, source_kind, source_id, label, was_current,
               payload, deleted_at, expires_at
        FROM connection_trash WHERE id = ?1
        "#,
        params![id],
        |row| {
            Ok(TrashRowSnapshot {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                source_kind: row.get(2)?,
                source_id: row.get(3)?,
                label: row.get(4)?,
                was_current: row.get(5)?,
                payload: row.get(6)?,
                deleted_at: row.get(7)?,
                expires_at: row.get(8)?,
            })
        },
    )
    .optional()
    .map_err(AppError::from)
}

fn remove_new_trash_rows(
    conn: &Connection,
    agent: AgentId,
    before: &[TrashRowSnapshot],
    after: &[TrashRowSnapshot],
    deleted_rows: &[Account],
) -> Result<()> {
    let expected = after
        .iter()
        .filter(|row| {
            row.agent_id == agent.as_str()
                && row.source_kind == "account"
                && !before.iter().any(|old| old.id == row.id)
                && deleted_rows.iter().any(|deleted| deleted.id == row.source_id)
        })
        .collect::<Vec<_>>();
    if expected.len() != deleted_rows.len() {
        return Err(account_compensation_conflict("connection_trash"));
    }
    for row in expected {
        if get_trash_row(conn, &row.id)?.as_ref() != Some(row) {
            return Err(account_compensation_conflict(&row.id));
        }
        let removed = conn.execute(
            "DELETE FROM connection_trash WHERE id = ?1",
            params![&row.id],
        )?;
        if removed != 1 {
            return Err(account_compensation_conflict(&row.id));
        }
    }
    Ok(())
}

fn validate_api_key_product_marker(agent: AgentId, marker: &str) -> Result<()> {
    let allowed = match agent {
        AgentId::Claude => ["anthropic"].as_slice(),
        AgentId::Codex => ["openai", "openai-api"].as_slice(),
        AgentId::Grok => ["xai", "xai-api"].as_slice(),
        AgentId::Kimi => ["kimi-code-membership", "kimi-api"].as_slice(),
        _ => [].as_slice(),
    };
    if allowed
        .iter()
        .any(|value| marker.eq_ignore_ascii_case(value))
    {
        Ok(())
    } else {
        Err(AppError::InvalidArg(format!(
            "unsupported API key product marker for {}: {}",
            agent.as_str(),
            marker
        )))
    }
}
