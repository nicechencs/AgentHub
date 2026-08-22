use rusqlite::{Connection, Transaction, TransactionBehavior};

use crate::error::{AppError, Result};
use crate::models::{Account, AgentId, ConnectionTrashKind};
use crate::storage::{
    account_create_conn, account_delete_if_revision_conn, account_get_by_id_conn,
    account_list_current_conn, account_update_conn, account_update_if_revision_conn,
    account_delete_for_agent_conn, binding_set_connection_refs_conn,
    provider_clear_current_conn, ConnectionTrashRepo,
};

use super::{ActiveBinding, ConnectionService};

impl ConnectionService {
    /// Create a new account and make it the sole active connection (is_current path).
    pub fn create_and_activate_account(
        &self,
        account: &Account,
    ) -> Result<(Account, ActiveBinding)> {
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let result = self.create_and_activate_account_conn(&tx, account)?;
            tx.commit()?;
            Ok(result)
        })
    }

    /// Insert + activate inside an already-open IMMEDIATE transaction.
    pub(crate) fn create_and_activate_account_conn(
        &self,
        conn: &Connection,
        account: &Account,
    ) -> Result<(Account, ActiveBinding)> {
        self.require_current_flag(account.is_current, "account")?;
        let created = account_create_conn(conn, account)?;
        provider_clear_current_conn(conn, created.agent_id)?;
        let binding = binding_set_connection_refs_conn(
            conn,
            &Self::key(created.agent_id),
            Some(created.id.clone()),
            None,
            &created.updated_at,
        )?;
        Ok((created, binding.into()))
    }

    /// Insert a non-current account inside an already-open IMMEDIATE transaction.
    pub(crate) fn create_account_conn(
        &self,
        conn: &Connection,
        account: &Account,
    ) -> Result<Account> {
        if account.is_current {
            return Err(AppError::InvalidArg(
                "create_account_conn requires is_current=false".into(),
            ));
        }
        account_create_conn(conn, account)
    }

    /// Update an existing account and make it the sole active connection.
    ///
    /// `expected_updated_at` is the caller's snapshot CAS token. A mismatch
    /// means another writer already committed; the caller must re-read.
    pub fn update_and_activate_account(
        &self,
        account: &Account,
        expected_updated_at: &str,
    ) -> Result<(Account, ActiveBinding)> {
        self.update_and_activate_account_with_revisions(account, expected_updated_at, None)
    }

    /// Duplicate-merge variant with transaction-local optimistic checks. The
    /// checks run after BEGIN IMMEDIATE and immediately before the account,
    /// counterpart and binding writes, closing the read/validate/write window
    /// left by a caller-side snapshot.
    pub fn update_and_activate_account_with_revisions(
        &self,
        account: &Account,
        expected_target_updated_at: &str,
        expected_current: Option<(&str, &str)>,
    ) -> Result<(Account, ActiveBinding)> {
        self.require_current_flag(account.is_current, "account")?;
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let existing = account_get_by_id_conn(&tx, &account.id)?.ok_or_else(|| {
                AppError::NotFound(format!("account not found: {}", account.id))
            })?;
            if existing.updated_at != expected_target_updated_at {
                return Err(AppError::message(
                    "account.merge.conflict",
                    format!("merge target changed before activation: {}", account.id),
                ));
            }
            if let Some((expected_id, expected_updated_at)) = expected_current {
                let current = account_list_current_conn(&tx, account.agent_id)?;
                if current.len() != 1
                    || current[0].id != expected_id
                    || current[0].updated_at != expected_updated_at
                {
                    return Err(AppError::message(
                        "account.merge.conflict",
                        "current account changed before duplicate activation",
                    ));
                }
            }
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

    /// Atomically activate a duplicate target and remove the current source.
    /// Both revision checks and all account/binding/trash changes happen in
    /// one IMMEDIATE transaction, so a source conflict cannot leave the target
    /// active while the live adapter still points at the source.
    pub fn update_and_activate_account_with_revisions_and_delete_source(
        &self,
        account: &Account,
        expected_target_updated_at: &str,
        source_id: &str,
        expected_source_updated_at: &str,
    ) -> Result<(Account, ActiveBinding, Account)> {
        self.require_current_flag(account.is_current, "account")?;
        let now = Self::now();
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let target = account_get_by_id_conn(&tx, &account.id)?.ok_or_else(|| {
                AppError::NotFound(format!("account not found: {}", account.id))
            })?;
            if target.updated_at != expected_target_updated_at {
                return Err(AppError::message(
                    "account.merge.conflict",
                    format!("merge target changed before activation: {}", account.id),
                ));
            }
            let source = account_get_by_id_conn(&tx, source_id)?.ok_or_else(|| {
                AppError::NotFound(format!("account not found: {source_id}"))
            })?;
            if source.agent_id != account.agent_id || source.updated_at != expected_source_updated_at
            {
                return Err(AppError::message(
                    "account.merge.conflict",
                    format!("source account changed before duplicate merge: {source_id}"),
                ));
            }

            let updated = account_update_conn(&tx, account)?;
            provider_clear_current_conn(&tx, updated.agent_id)?;
            let binding = binding_set_connection_refs_conn(
                &tx,
                &Self::key(updated.agent_id),
                Some(updated.id.clone()),
                None,
                &updated.updated_at,
            )?;
            ConnectionTrashRepo::insert_conn(
                &tx,
                &source.id,
                source.agent_id,
                ConnectionTrashKind::Account,
                &source.label,
                source.is_current,
                &source,
                &now,
            )?;
            account_delete_for_agent_conn(&tx, &source.id, source.agent_id)?;
            self.clear_connection_refs_if_match_conn(
                &tx,
                source.agent_id,
                Some(&source.id),
                None,
                &now,
            )?;
            tx.commit()?;
            Ok((updated, binding.into(), source))
        })
    }

    /// Target activation for a frozen merge/update plan. Callers must already
    /// hold an IMMEDIATE transaction; this never lists the pool to choose a
    /// target and never starts a nested transaction.
    pub(crate) fn activate_account_if_revision_conn(
        &self,
        conn: &Connection,
        account: &Account,
        expected_updated_at: &str,
    ) -> Result<(Account, ActiveBinding)> {
        self.require_current_flag(account.is_current, "account")?;
        let existing = account_get_by_id_conn(conn, &account.id)?.ok_or_else(|| {
            AppError::NotFound(format!("account not found: {}", account.id))
        })?;
        if existing.updated_at != expected_updated_at {
            return Err(AppError::message(
                "account.merge.conflict",
                format!("merge target changed before activation: {}", account.id),
            ));
        }
        let updated = account_update_if_revision_conn(conn, account, expected_updated_at)?;
        provider_clear_current_conn(conn, updated.agent_id)?;
        let binding = binding_set_connection_refs_conn(
            conn,
            &Self::key(updated.agent_id),
            Some(updated.id.clone()),
            None,
            &updated.updated_at,
        )?;
        Ok((updated, binding.into()))
    }

    /// Non-current in-place update for a frozen plan. Same IMMEDIATE
    /// transaction as determination; CAS on the snapshot revision.
    pub(crate) fn update_account_if_revision_conn(
        &self,
        conn: &Connection,
        account: &Account,
        expected_updated_at: &str,
    ) -> Result<Account> {
        let existing = account_get_by_id_conn(conn, &account.id)?.ok_or_else(|| {
            AppError::NotFound(format!("account not found: {}", account.id))
        })?;
        if existing.updated_at != expected_updated_at {
            return Err(AppError::message(
                "account.merge.conflict",
                format!("account changed before update: {}", account.id),
            ));
        }
        account_update_if_revision_conn(conn, account, expected_updated_at)
    }

    /// Trash + delete one frozen duplicate. Revision mismatch uses a delete
    /// conflict code so a failure after target activation is never classified
    /// as a pre-mutation merge conflict. The surrounding IMMEDIATE transaction
    /// rolls the activation back.
    pub(crate) fn trash_delete_account_if_revision_conn(
        &self,
        conn: &Connection,
        id: &str,
        agent: AgentId,
        expected_updated_at: &str,
        now: &str,
    ) -> Result<Account> {
        let account = account_get_by_id_conn(conn, id)?
            .ok_or_else(|| AppError::NotFound(format!("account not found: {id}")))?;
        if account.agent_id != agent {
            return Err(AppError::NotFound(format!(
                "account not found: {id} (agent filter: {})",
                agent.as_str()
            )));
        }
        if account.updated_at != expected_updated_at {
            return Err(AppError::message(
                "account.merge.delete.conflict",
                format!("account changed before duplicate deletion: {id}"),
            ));
        }
        ConnectionTrashRepo::insert_conn(
            conn,
            &account.id,
            agent,
            ConnectionTrashKind::Account,
            &account.label,
            account.is_current,
            &account,
            now,
        )?;
        account_delete_if_revision_conn(conn, id, agent, expected_updated_at)?;
        self.clear_connection_refs_if_match_conn(conn, agent, Some(id), None, now)?;
        Ok(account)
    }
}
