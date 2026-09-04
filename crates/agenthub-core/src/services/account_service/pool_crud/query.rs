use std::time::Instant;

use chrono::Utc;
use rusqlite::{Transaction, TransactionBehavior};

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{Account, AccountKind, AgentId};
use crate::services::account_split::{
    accounts_share_authorization, is_mixed_live_bundle, split_mixed_account,
};
use crate::services::adapter_route_constants::CONNECTION_SECRET_MARKER;
use crate::storage::{account_create_conn, account_update_conn, ConnectionTrashRepo};
use crate::utils::redact::api_key_secret;

use super::super::surface::*;
use super::super::AccountService;

fn account_has_usable_api_key(account: &Account) -> bool {
    let Some(secret) = api_key_secret(&account.credentials) else {
        return false;
    };
    let trimmed = secret.trim();
    !trimmed.is_empty() && trimmed != "***" && trimmed != CONNECTION_SECRET_MARKER
}

fn persist_split_account_heal(
    db: &crate::storage::Database,
    original: &Account,
    split: Vec<Account>,
    existing: &[Account],
) -> Result<()> {
    db.with_conn(|conn| {
        let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
        let mut pieces = split.into_iter();
        let Some(first) = pieces.next() else {
            return Ok(());
        };
        account_update_conn(&tx, &first)?;
        let trash = ConnectionTrashRepo::list_in_conn(&tx, Some(original.agent_id), None)?;
        for piece in pieces {
            if existing
                .iter()
                .any(|other| other.id != original.id && accounts_share_authorization(&piece, other))
            {
                continue;
            }
            if trash.iter().any(|item| {
                item.account
                    .as_ref()
                    .is_some_and(|other| accounts_share_authorization(&piece, other))
            }) {
                continue;
            }
            account_create_conn(&tx, &piece)?;
        }
        tx.commit()?;
        Ok(())
    })
}

impl AccountService {
    /// SQLite pool plus local identity/expiry heals. No live-file sync and no
    /// upstream quota HTTP — GUI list paths stay off the network.
    pub fn list_pool(&self, agent: Option<AgentId>) -> Result<Vec<Account>> {
        self.connections.reconcile_known_agents(agent);
        self.heal_unusable_api_key_accounts(agent);
        self.heal_mixed_bundle_accounts(agent);
        let mut items = self.repo.list(agent)?;
        // Persist identity extracted from stored tokens so GUI sees email/sub
        // after redaction (JWT lives only in credentials until healed).
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
        sort_accounts(&mut items);
        Ok(items)
    }

    /// Recycle API Key rows that were persisted without a usable secret
    /// (`***` / marker / empty). Those leftover live imports show up as
    /// nameless cards and cannot be switched or forwarded.
    fn heal_unusable_api_key_accounts(&self, agent: Option<AgentId>) {
        let Ok(rows) = self.repo.list(agent) else {
            return;
        };
        for row in rows {
            if row.kind != AccountKind::ApiKey || account_has_usable_api_key(&row) {
                continue;
            }
            tracing::info!(
                module = targets::ACCOUNT,
                op = "recycle",
                agent = row.agent_id.as_str(),
                id = row.id.as_str(),
                "recycled login with no usable key"
            );
            let _ = self.connections.delete_account(&row.id, row.agent_id);
        }
    }

    /// Expand leftover mixed official-login + API Key rows so the pool never
    /// keeps a combined snapshot that would write both families on switch.
    fn heal_mixed_bundle_accounts(&self, agent: Option<AgentId>) {
        let Ok(rows) = self.repo.list(agent) else {
            return;
        };
        for row in rows {
            if !is_mixed_live_bundle(&row.credentials) {
                continue;
            }
            let split = split_mixed_account(&row);
            if split.is_empty() || (split.len() == 1 && split[0].credentials == row.credentials) {
                continue;
            }
            let Ok(existing) = self.repo.list(Some(row.agent_id)) else {
                continue;
            };
            if let Err(error) = persist_split_account_heal(&self.db, &row, split, &existing) {
                tracing::warn!(
                    module = targets::ACCOUNT,
                    op = "split_mixed",
                    account_id = %row.id,
                    agent = row.agent_id.as_str(),
                    error = %error,
                    "failed to split mixed login"
                );
            }
        }
    }

    /// File-backed agents can rotate credentials while they are running.
    /// Reconcile a safe live snapshot before mapping rows so a stale DB
    /// snapshot cannot be shown as a dead login. Still no quota HTTP.
    pub fn list(&self, agent: Option<AgentId>) -> Result<Vec<Account>> {
        self.sync_current_live(agent);
        let mut items = self.list_pool(agent)?;
        // Live auth health describes the file currently observed by the adapter,
        // rather than the persisted pool row. Surface it only on the pool row
        // that still corresponds to that live authorization, and never write it
        // back to the database.
        self.merge_live_auth_state(&mut items, agent);
        Ok(items)
    }

    /// Probe upstream 5h/7d quota for one OAuth account and persist healed fields.
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
}
