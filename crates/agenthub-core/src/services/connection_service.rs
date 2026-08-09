//! ConnectionService — unique active binding write entry (P10 cleanup B1 / R01).
//!
//! Single DB transaction dual-writes:
//! - legacy `accounts` / `providers`.`is_current`
//! - `agent_active_bindings` connection refs (`account_id` / `provider_id`)
//!
//! Independent extension fields (`model_id`, `config_profile_id`) are preserved
//! across Account/Provider lifecycle ops. Only [`Self::clear`] deletes the whole row.
//!
//! Live apply (adapter FS writes) stays in AccountService / ProviderService
//! and runs **before** this service is called for switch paths.

use chrono::{Duration, Utc};
use rusqlite::{params, Connection, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{Account, AgentId, ConnectionTrashItem, ConnectionTrashKind, Provider};
use crate::platform::AgentKey;
use crate::storage::{
    account_clear_current_conn, account_create_conn, account_delete_for_agent_conn,
    account_force_sole_current_conn, account_get_by_id_conn, account_list_current_conn,
    account_select_current_conn, account_update_conn, binding_clear_conn,
    binding_clear_connection_refs_conn, binding_get_conn, binding_set_connection_refs_conn,
    provider_clear_current_conn, provider_create_conn, provider_delete_for_agent_conn,
    provider_force_sole_current_conn, provider_get_by_id_conn, provider_list_current_conn,
    provider_select_current_conn, provider_update_conn, provider_upsert_conn, ActiveBindingRow,
    Database,
};

#[cfg(test)]
use crate::storage::AccountRepo;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveBinding {
    pub agent_key: String,
    pub account_id: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub config_profile_id: Option<String>,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ActiveBindingRow> for ActiveBinding {
    fn from(r: ActiveBindingRow) -> Self {
        Self {
            agent_key: r.agent_key,
            account_id: r.account_id,
            provider_id: r.provider_id,
            model_id: r.model_id,
            config_profile_id: r.config_profile_id,
            revision: r.revision,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Clone)]
pub struct ConnectionService {
    db: Database,
    /// Used only by cfg(test) activate helpers that resolve rows before activate.
    #[cfg(test)]
    accounts: AccountRepo,
}

impl ConnectionService {
    pub fn new(db: Database) -> Self {
        Self {
            #[cfg(test)]
            accounts: AccountRepo::new(db.clone()),
            db,
        }
    }

    fn key(agent: AgentId) -> String {
        AgentKey::from_agent_id(agent).into_string()
    }

    /// Project-local timestamp format (matches Account/Provider service stamps).
    fn now() -> String {
        chrono::Utc::now()
            .format("%Y-%m-%d %H:%M:%S%.6f")
            .to_string()
    }

    /// Read and, when needed, repair active binding + legacy currents in one transaction.
    ///
    /// Rules:
    /// - A legal connection binding is source of truth; legacy `is_current` is mirrored.
    /// - model/profile-only bindings are valid: keep them and clear legacy currents.
    /// - Dangling connection refs are cleared (model/profile preserved) then backfilled.
    /// - Never returns a connection ref that points at a missing object.
    pub fn get_active(&self, agent: AgentId) -> Result<Option<ActiveBinding>> {
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let out = self.reconcile_active_conn(&tx, agent)?;
            tx.commit()?;
            Ok(out)
        })
    }

    /// After live apply: one transaction selects account current, demotes
    /// provider currents, and upserts connection refs (model/profile preserved).
    pub fn activate_account(
        &self,
        agent: AgentId,
        account_id: &str,
        expected_updated_at: &str,
        updated_at: &str,
    ) -> Result<(Account, ActiveBinding)> {
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let account = account_select_current_conn(
                &tx,
                account_id,
                agent,
                expected_updated_at,
                updated_at,
            )?;
            provider_clear_current_conn(&tx, agent)?;
            let binding = binding_set_connection_refs_conn(
                &tx,
                &Self::key(agent),
                Some(account.id.clone()),
                None,
                updated_at,
            )?;
            tx.commit()?;
            Ok((account, binding.into()))
        })
    }

    /// After live apply: one transaction selects provider current, demotes
    /// account currents, and upserts connection refs (model/profile preserved).
    pub fn activate_provider(
        &self,
        agent: AgentId,
        provider_id: &str,
        expected_updated_at: &str,
        updated_at: &str,
    ) -> Result<(Provider, ActiveBinding)> {
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let provider = provider_select_current_conn(
                &tx,
                provider_id,
                agent,
                expected_updated_at,
                updated_at,
            )?;
            account_clear_current_conn(&tx, agent)?;
            let binding = binding_set_connection_refs_conn(
                &tx,
                &Self::key(agent),
                None,
                Some(provider.id.clone()),
                updated_at,
            )?;
            tx.commit()?;
            Ok((provider, binding.into()))
        })
    }

    /// Create a new account and make it the sole active connection (is_current path).
    pub fn create_and_activate_account(
        &self,
        account: &Account,
    ) -> Result<(Account, ActiveBinding)> {
        self.require_current_flag(account.is_current, "account")?;
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let created = account_create_conn(&tx, account)?;
            provider_clear_current_conn(&tx, created.agent_id)?;
            let binding = binding_set_connection_refs_conn(
                &tx,
                &Self::key(created.agent_id),
                Some(created.id.clone()),
                None,
                &created.updated_at,
            )?;
            tx.commit()?;
            Ok((created, binding.into()))
        })
    }

    /// Update an existing account and make it the sole active connection.
    pub fn update_and_activate_account(
        &self,
        account: &Account,
    ) -> Result<(Account, ActiveBinding)> {
        self.require_current_flag(account.is_current, "account")?;
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let updated = account_update_conn(&tx, account)?;
            provider_clear_current_conn(&tx, updated.agent_id)?;
            let binding = binding_set_connection_refs_conn(
                &tx,
                &Self::key(updated.agent_id),
                Some(updated.id.clone()),
                None,
                &updated.updated_at,
            )?;
            tx.commit()?;
            Ok((updated, binding.into()))
        })
    }

    /// Create a new provider and make it the sole active connection.
    pub fn create_and_activate_provider(
        &self,
        provider: &Provider,
    ) -> Result<(Provider, ActiveBinding)> {
        self.require_current_flag(provider.is_current, "provider")?;
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let created = provider_create_conn(&tx, provider)?;
            account_clear_current_conn(&tx, created.agent_id)?;
            let binding = binding_set_connection_refs_conn(
                &tx,
                &Self::key(created.agent_id),
                None,
                Some(created.id.clone()),
                &created.updated_at,
            )?;
            tx.commit()?;
            Ok((created, binding.into()))
        })
    }

    /// Update an existing provider and make it the sole active connection.
    pub fn update_and_activate_provider(
        &self,
        provider: &Provider,
    ) -> Result<(Provider, ActiveBinding)> {
        self.require_current_flag(provider.is_current, "provider")?;
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let updated = provider_update_conn(&tx, provider)?;
            account_clear_current_conn(&tx, updated.agent_id)?;
            let binding = binding_set_connection_refs_conn(
                &tx,
                &Self::key(updated.agent_id),
                None,
                Some(updated.id.clone()),
                &updated.updated_at,
            )?;
            tx.commit()?;
            Ok((updated, binding.into()))
        })
    }

    /// Upsert a provider and make it the sole active connection.
    pub fn upsert_and_activate_provider(
        &self,
        provider: &Provider,
    ) -> Result<(Provider, ActiveBinding)> {
        self.require_current_flag(provider.is_current, "provider")?;
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let stored = provider_upsert_conn(&tx, provider)?;
            account_clear_current_conn(&tx, stored.agent_id)?;
            let binding = binding_set_connection_refs_conn(
                &tx,
                &Self::key(stored.agent_id),
                None,
                Some(stored.id.clone()),
                &stored.updated_at,
            )?;
            tx.commit()?;
            Ok((stored, binding.into()))
        })
    }

    /// Update provider with `is_current=false`. Clears connection refs if binding
    /// pointed at it; model/profile are preserved.
    pub fn update_provider_non_current(&self, provider: &Provider) -> Result<Provider> {
        if provider.is_current {
            return Err(AppError::InvalidArg(
                "update_provider_non_current requires is_current=false".into(),
            ));
        }
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let updated = provider_update_conn(&tx, provider)?;
            self.clear_connection_refs_if_match_conn(
                &tx,
                updated.agent_id,
                None,
                Some(updated.id.as_str()),
                &updated.updated_at,
            )?;
            tx.commit()?;
            Ok(updated)
        })
    }

    /// Upsert provider with `is_current=false`. Clears connection refs only when
    /// they reference this id; model/profile are preserved.
    pub fn upsert_provider_non_current(&self, provider: &Provider) -> Result<Provider> {
        if provider.is_current {
            return Err(AppError::InvalidArg(
                "upsert_provider_non_current requires is_current=false".into(),
            ));
        }
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let stored = provider_upsert_conn(&tx, provider)?;
            self.clear_connection_refs_if_match_conn(
                &tx,
                stored.agent_id,
                None,
                Some(stored.id.as_str()),
                &stored.updated_at,
            )?;
            tx.commit()?;
            Ok(stored)
        })
    }

    /// Move an account to the local recovery bin, then clear its active binding.
    /// The agent's own files are intentionally untouched.
    pub fn delete_account(&self, id: &str, agent: AgentId) -> Result<()> {
        let now = Self::now();
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let account = account_get_by_id_conn(&tx, id)?
                .ok_or_else(|| AppError::NotFound(format!("account not found: {id}")))?;
            if account.agent_id != agent {
                return Err(AppError::NotFound(format!(
                    "account not found: {id} (agent filter: {})",
                    agent.as_str()
                )));
            }
            insert_trash_conn(
                &tx,
                &account.id,
                agent,
                ConnectionTrashKind::Account,
                &account.label,
                account.is_current,
                &account,
                &now,
            )?;
            account_delete_for_agent_conn(&tx, id, agent)?;
            self.clear_connection_refs_if_match_conn(&tx, agent, Some(id), None, &now)?;
            tx.commit()?;
            Ok(())
        })
    }

    /// Move a provider to the local recovery bin, then clear its active binding.
    /// The agent's own files are intentionally untouched.
    pub fn delete_provider(&self, id: &str, agent: AgentId) -> Result<()> {
        let now = Self::now();
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let provider = provider_get_by_id_conn(&tx, id)?
                .ok_or_else(|| AppError::NotFound(format!("provider not found: {id}")))?;
            if provider.agent_id != agent {
                return Err(AppError::NotFound(format!(
                    "provider not found: {id} (agent filter: {})",
                    agent.as_str()
                )));
            }
            insert_trash_conn(
                &tx,
                &provider.id,
                agent,
                ConnectionTrashKind::Provider,
                &provider.name,
                provider.is_current,
                &provider,
                &now,
            )?;
            provider_delete_for_agent_conn(&tx, id, agent)?;
            self.clear_connection_refs_if_match_conn(&tx, agent, None, Some(id), &now)?;
            tx.commit()?;
            Ok(())
        })
    }

    /// List recoverable connection rows.  Secret material is retained in the
    /// database for restore and redacted by the Tauri boundary before return.
    pub fn list_trash(&self, agent: Option<AgentId>) -> Result<Vec<ConnectionTrashItem>> {
        let now = Self::now();
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM connection_trash WHERE expires_at <= ?1",
                params![now],
            )?;
            let mut stmt = if agent.is_some() {
                conn.prepare(
                    "SELECT id, agent_id, source_kind, source_id, label, was_current, payload, deleted_at, expires_at
                     FROM connection_trash WHERE agent_id = ?1 ORDER BY deleted_at DESC, id DESC",
                )?
            } else {
                conn.prepare(
                    "SELECT id, agent_id, source_kind, source_id, label, was_current, payload, deleted_at, expires_at
                     FROM connection_trash ORDER BY deleted_at DESC, id DESC",
                )?
            };
            let rows = if let Some(agent) = agent {
                stmt.query_map(params![agent.as_str()], map_trash_row)?
            } else {
                stmt.query_map([], map_trash_row)?
            };
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(AppError::from)
        })
    }

    /// Restore a row to the AgentHub pool without applying it to the agent.
    pub fn restore_trash(&self, id: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let row = load_trash_payload_conn(&tx, id)?;
            match row.kind {
                ConnectionTrashKind::Account => {
                    let mut account: Account = serde_json::from_value(row.payload)?;
                    if account.id != row.source_id || account.agent_id != row.agent_id {
                        return Err(AppError::InvalidArg("回收记录与账号内容不一致".into()));
                    }
                    if account_get_by_id_conn(&tx, &account.id)?.is_some() {
                        return Err(AppError::InvalidArg(format!(
                            "account already exists: {}",
                            account.id
                        )));
                    }
                    // Restoring never silently applies a live credential.
                    account.is_current = false;
                    account_create_conn(&tx, &account)?;
                }
                ConnectionTrashKind::Provider => {
                    let mut provider: Provider = serde_json::from_value(row.payload)?;
                    if provider.id != row.source_id || provider.agent_id != row.agent_id {
                        return Err(AppError::InvalidArg("回收记录与 API Key 配置不一致".into()));
                    }
                    if provider_get_by_id_conn(&tx, &provider.id)?.is_some() {
                        return Err(AppError::InvalidArg(format!(
                            "provider already exists: {}",
                            provider.id
                        )));
                    }
                    provider.is_current = false;
                    provider_create_conn(&tx, &provider)?;
                }
            }
            tx.execute("DELETE FROM connection_trash WHERE id = ?1", params![id])?;
            tx.commit()?;
            Ok(())
        })
    }

    /// Permanently remove one recovery-bin row.
    pub fn delete_trash(&self, id: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            let n = conn.execute("DELETE FROM connection_trash WHERE id = ?1", params![id])?;
            if n == 0 {
                return Err(AppError::NotFound(format!("trash item not found: {id}")));
            }
            Ok(())
        })
    }

    /// Explicit clear of the whole active binding (including model/profile) and
    /// both legacy current flags so lazy-backfill cannot resurrect it.
    pub fn clear(&self, agent: AgentId) -> Result<()> {
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            account_clear_current_conn(&tx, agent)?;
            provider_clear_current_conn(&tx, agent)?;
            binding_clear_conn(&tx, &Self::key(agent))?;
            tx.commit()?;
            Ok(())
        })
    }

    /// Crate-private helper for tests: activate via the full atomic path (not binding-only).
    #[cfg(test)]
    pub(crate) fn record_account_active(
        &self,
        agent: AgentId,
        account_id: &str,
    ) -> Result<ActiveBinding> {
        let acc = self
            .accounts
            .get_by_id(account_id)?
            .ok_or_else(|| AppError::NotFound(format!("account not found: {account_id}")))?;
        if acc.agent_id != agent {
            return Err(AppError::InvalidArg(format!(
                "account {} does not belong to agent {}",
                account_id,
                agent.as_str()
            )));
        }
        let (_account, binding) =
            self.activate_account(agent, account_id, &acc.updated_at, &Self::now())?;
        Ok(binding)
    }

    fn require_current_flag(&self, is_current: bool, kind: &str) -> Result<()> {
        if !is_current {
            return Err(AppError::InvalidArg(format!(
                "{kind} activate path requires is_current=true"
            )));
        }
        Ok(())
    }

    fn has_extension_fields(row: &ActiveBindingRow) -> bool {
        row.model_id.is_some() || row.config_profile_id.is_some()
    }

    /// Single-transaction reconcile used by [`Self::get_active`].
    fn reconcile_active_conn(
        &self,
        conn: &Connection,
        agent: AgentId,
    ) -> Result<Option<ActiveBinding>> {
        let key = Self::key(agent);
        let now = Self::now();
        let existing = binding_get_conn(conn, &key)?;

        match existing {
            Some(row) => match self.resolve_binding_target_conn(conn, agent, &row)? {
                BindingTarget::Account(id) => {
                    self.mirror_account_active_conn(conn, agent, &id, &now)?;
                    let fresh = binding_get_conn(conn, &key)?.ok_or_else(|| {
                        AppError::message("db.binding", "binding missing after mirror")
                    })?;
                    Ok(Some(fresh.into()))
                }
                BindingTarget::Provider(id) => {
                    self.mirror_provider_active_conn(conn, agent, &id, &now)?;
                    let fresh = binding_get_conn(conn, &key)?.ok_or_else(|| {
                        AppError::message("db.binding", "binding missing after mirror")
                    })?;
                    Ok(Some(fresh.into()))
                }
                BindingTarget::Empty => {
                    if Self::has_extension_fields(&row) {
                        // model/profile-only: valid binding, no connection current.
                        account_clear_current_conn(conn, agent)?;
                        provider_clear_current_conn(conn, agent)?;
                        Ok(Some(row.into()))
                    } else {
                        // Meaningless empty row.
                        binding_clear_conn(conn, &key)?;
                        Ok(None)
                    }
                }
                BindingTarget::Invalid => {
                    tracing::warn!(
                        module = targets::ACCOUNT,
                        op = "reconcile",
                        agent = agent.as_str(),
                        account_id = ?row.account_id,
                        provider_id = ?row.provider_id,
                        "clearing invalid connection refs during reconcile (preserving model/profile)"
                    );
                    // Drop bad connection refs; keep extension fields.
                    let remaining = binding_clear_connection_refs_conn(conn, &key, &now)?;
                    // Try to reattach from legacy currents without wiping model/profile.
                    if let Some(rebuilt) = self.lazy_backfill_conn(conn, agent)? {
                        return Ok(Some(rebuilt.into()));
                    }
                    if let Some(ext) = remaining {
                        account_clear_current_conn(conn, agent)?;
                        provider_clear_current_conn(conn, agent)?;
                        return Ok(Some(ext.into()));
                    }
                    Ok(None)
                }
            },
            None => self
                .lazy_backfill_conn(conn, agent)
                .map(|r| r.map(Into::into)),
        }
    }

    fn resolve_binding_target_conn(
        &self,
        conn: &Connection,
        agent: AgentId,
        row: &ActiveBindingRow,
    ) -> Result<BindingTarget> {
        match (&row.account_id, &row.provider_id) {
            (Some(aid), None) => {
                let Some(acc) = account_get_by_id_conn(conn, aid)? else {
                    return Ok(BindingTarget::Invalid);
                };
                if acc.agent_id != agent {
                    return Ok(BindingTarget::Invalid);
                }
                Ok(BindingTarget::Account(aid.clone()))
            }
            (None, Some(pid)) => {
                let Some(p) = provider_get_by_id_conn(conn, pid)? else {
                    return Ok(BindingTarget::Invalid);
                };
                if p.agent_id != agent {
                    return Ok(BindingTarget::Invalid);
                }
                Ok(BindingTarget::Provider(pid.clone()))
            }
            (None, None) => Ok(BindingTarget::Empty),
            (Some(_), Some(_)) => Ok(BindingTarget::Invalid),
        }
    }

    fn mirror_account_active_conn(
        &self,
        conn: &Connection,
        agent: AgentId,
        account_id: &str,
        now: &str,
    ) -> Result<()> {
        let accounts = account_list_current_conn(conn, agent)?;
        let providers = provider_list_current_conn(conn, agent)?;
        let sole_ok = accounts.len() == 1
            && accounts[0].id == account_id
            && accounts[0].is_current
            && providers.is_empty();
        if !sole_ok {
            account_force_sole_current_conn(conn, account_id, agent, now)?;
            provider_clear_current_conn(conn, agent)?;
        }
        let key = Self::key(agent);
        let needs_write = match binding_get_conn(conn, &key)? {
            Some(b) => b.account_id.as_deref() != Some(account_id) || b.provider_id.is_some(),
            None => true,
        };
        if needs_write {
            binding_set_connection_refs_conn(conn, &key, Some(account_id.to_string()), None, now)?;
        }
        Ok(())
    }

    fn mirror_provider_active_conn(
        &self,
        conn: &Connection,
        agent: AgentId,
        provider_id: &str,
        now: &str,
    ) -> Result<()> {
        let accounts = account_list_current_conn(conn, agent)?;
        let providers = provider_list_current_conn(conn, agent)?;
        let sole_ok = providers.len() == 1
            && providers[0].id == provider_id
            && providers[0].is_current
            && accounts.is_empty();
        if !sole_ok {
            provider_force_sole_current_conn(conn, provider_id, agent, now)?;
            account_clear_current_conn(conn, agent)?;
        }
        let key = Self::key(agent);
        let needs_write = match binding_get_conn(conn, &key)? {
            Some(b) => b.provider_id.as_deref() != Some(provider_id) || b.account_id.is_some(),
            None => true,
        };
        if needs_write {
            binding_set_connection_refs_conn(conn, &key, None, Some(provider_id.to_string()), now)?;
        }
        Ok(())
    }

    /// Clear connection refs only when they match the deleted/demoted object.
    fn clear_connection_refs_if_match_conn(
        &self,
        conn: &Connection,
        agent: AgentId,
        account_id: Option<&str>,
        provider_id: Option<&str>,
        now: &str,
    ) -> Result<()> {
        let key = Self::key(agent);
        let Some(row) = binding_get_conn(conn, &key)? else {
            return Ok(());
        };
        let matches_account = account_id.is_some_and(|id| row.account_id.as_deref() == Some(id));
        let matches_provider = provider_id.is_some_and(|id| row.provider_id.as_deref() == Some(id));
        if matches_account || matches_provider {
            binding_clear_connection_refs_conn(conn, &key, now)?;
        }
        Ok(())
    }

    /// Deterministic single-choice backfill of **connection** refs from legacy is_current.
    /// Prefers account over provider; preserves existing model/profile on the binding row.
    fn lazy_backfill_conn(
        &self,
        conn: &Connection,
        agent: AgentId,
    ) -> Result<Option<ActiveBindingRow>> {
        let now = Self::now();
        let mut current_accounts = account_list_current_conn(conn, agent)?;
        let mut current_providers = provider_list_current_conn(conn, agent)?;

        current_accounts.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        current_providers.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let pick_account = current_accounts.into_iter().next();
        let pick_provider = current_providers.into_iter().next();

        match (pick_account, pick_provider) {
            (Some(a), Some(_)) => {
                tracing::warn!(
                    module = targets::ACCOUNT,
                    op = "lazy_backfill",
                    agent = agent.as_str(),
                    "legacy dual-current resolved into single binding (prefer account)"
                );
                self.mirror_account_active_conn(conn, agent, &a.id, &now)?;
                binding_get_conn(conn, &Self::key(agent))?
                    .ok_or_else(|| {
                        AppError::message("db.binding", "binding missing after backfill")
                    })
                    .map(Some)
            }
            (Some(a), None) => {
                self.mirror_account_active_conn(conn, agent, &a.id, &now)?;
                binding_get_conn(conn, &Self::key(agent))?
                    .ok_or_else(|| {
                        AppError::message("db.binding", "binding missing after backfill")
                    })
                    .map(Some)
            }
            (None, Some(p)) => {
                self.mirror_provider_active_conn(conn, agent, &p.id, &now)?;
                binding_get_conn(conn, &Self::key(agent))?
                    .ok_or_else(|| {
                        AppError::message("db.binding", "binding missing after backfill")
                    })
                    .map(Some)
            }
            (None, None) => Ok(None),
        }
    }
}

#[derive(Debug)]
struct TrashPayloadRow {
    kind: ConnectionTrashKind,
    source_id: String,
    agent_id: AgentId,
    payload: Value,
}

fn insert_trash_conn<T: serde::Serialize>(
    conn: &Connection,
    source_id: &str,
    agent_id: AgentId,
    kind: ConnectionTrashKind,
    label: &str,
    was_current: bool,
    payload: &T,
    deleted_at: &str,
) -> Result<()> {
    let expires_at = (Utc::now() + Duration::days(30))
        .format("%Y-%m-%d %H:%M:%S%.6f")
        .to_string();
    let payload = serde_json::to_string(payload)?;
    conn.execute(
        "INSERT INTO connection_trash
         (id, agent_id, source_kind, source_id, label, was_current, payload, deleted_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            Uuid::new_v4().to_string(),
            agent_id.as_str(),
            kind.as_str(),
            source_id,
            label,
            if was_current { 1 } else { 0 },
            payload,
            deleted_at,
            expires_at,
        ],
    )?;
    Ok(())
}

fn load_trash_payload_conn(conn: &Connection, id: &str) -> Result<TrashPayloadRow> {
    let (kind_raw, source_id, agent_raw, payload_raw): (String, String, String, String) = conn
        .query_row(
            "SELECT source_kind, source_id, agent_id, payload FROM connection_trash WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("trash item not found: {id}"))
            }
            other => AppError::from(other),
        })?;
    let kind = ConnectionTrashKind::parse(&kind_raw)
        .ok_or_else(|| AppError::InvalidArg(format!("invalid trash kind: {kind_raw}")))?;
    let agent_id = AgentId::parse(&agent_raw)
        .ok_or_else(|| AppError::InvalidArg(format!("invalid trash agent: {agent_raw}")))?;
    let payload = serde_json::from_str(&payload_raw)?;
    Ok(TrashPayloadRow {
        kind,
        source_id,
        agent_id,
        payload,
    })
}

fn map_trash_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConnectionTrashItem> {
    let id: String = row.get(0)?;
    let agent_raw: String = row.get(1)?;
    let agent_id = AgentId::parse(&agent_raw).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid agent_id in connection_trash row: {agent_raw}"),
            )),
        )
    })?;
    let kind_raw: String = row.get(2)?;
    let kind = ConnectionTrashKind::parse(&kind_raw).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid source_kind in connection_trash row: {kind_raw}"),
            )),
        )
    })?;
    let source_id: String = row.get(3)?;
    let label: String = row.get(4)?;
    let was_current: i64 = row.get(5)?;
    let payload_raw: String = row.get(6)?;
    let deleted_at: String = row.get(7)?;
    let expires_at: String = row.get(8)?;
    let payload: Value = serde_json::from_str(&payload_raw).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(err))
    })?;

    let (account, provider) = match kind {
        ConnectionTrashKind::Account => {
            let account = serde_json::from_value(payload).map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            })?;
            (Some(account), None)
        }
        ConnectionTrashKind::Provider => {
            let provider = serde_json::from_value(payload).map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            })?;
            (None, Some(provider))
        }
    };

    Ok(ConnectionTrashItem {
        id,
        agent_id,
        kind,
        source_id,
        label,
        was_current: was_current != 0,
        deleted_at,
        expires_at,
        account,
        provider,
    })
}

enum BindingTarget {
    Account(String),
    Provider(String),
    Empty,
    Invalid,
}

#[cfg(test)]
mod tests;
