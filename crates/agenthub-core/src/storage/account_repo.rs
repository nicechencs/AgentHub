//! Account table repository — storage boundary only (no business rules).

use rusqlite::{params, Connection, OptionalExtension, Row, Transaction, TransactionBehavior};

use crate::error::{AppError, Result};
use crate::models::{Account, AccountKind, AgentId};
use crate::storage::Database;

/// SQLite access for the `accounts` table.
#[derive(Clone)]
pub struct AccountRepo {
    db: Database,
}

impl AccountRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn create(&self, account: &Account) -> Result<Account> {
        self.mutate(|conn| {
            if get_by_id_conn(conn, &account.id)?.is_some() {
                return Err(AppError::InvalidArg(format!(
                    "account already exists: {}",
                    account.id
                )));
            }
            insert_row(conn, account)?;
            clear_other_currents_if_needed(conn, account)?;
            get_by_id_conn(conn, &account.id)?
                .ok_or_else(|| AppError::message("db.account", "account missing after create"))
        })
    }

    pub fn update(&self, account: &Account) -> Result<Account> {
        self.mutate(|conn| {
            let existing = get_by_id_conn(conn, &account.id)?
                .ok_or_else(|| AppError::NotFound(format!("account not found: {}", account.id)))?;
            if existing.agent_id != account.agent_id {
                return Err(AppError::InvalidArg(format!(
                    "account agent_id is immutable (id={}, existing={}, requested={})",
                    account.id,
                    existing.agent_id.as_str(),
                    account.agent_id.as_str()
                )));
            }
            let mut row = account.clone();
            row.created_at = existing.created_at;
            update_row(conn, &row)?;
            clear_other_currents_if_needed(conn, &row)?;
            get_by_id_conn(conn, &row.id)?
                .ok_or_else(|| AppError::message("db.account", "account missing after update"))
        })
    }

    /// Persist fields healed or refreshed from an account snapshot without
    /// allowing a stale caller to overwrite `is_current` (or other identity
    /// fields). The expected `updated_at` is an optimistic concurrency token.
    pub fn update_healed_fields(
        &self,
        account: &Account,
        expected_updated_at: &str,
        updated_at: &str,
    ) -> Result<Account> {
        self.mutate(|conn| {
            let credentials = serde_json::to_string(&account.credentials)?;
            let extra = serde_json::to_string(&account.extra)?;
            let changed = conn.execute(
                r#"
                UPDATE accounts SET
                    label = ?2,
                    credentials = ?3,
                    extra = ?4,
                    status = ?5,
                    updated_at = ?6
                WHERE id = ?1 AND agent_id = ?7 AND updated_at = ?8
                "#,
                params![
                    account.id,
                    account.label,
                    credentials,
                    extra,
                    account.status,
                    updated_at,
                    account.agent_id.as_str(),
                    expected_updated_at,
                ],
            )?;
            if changed != 1 {
                return Err(AppError::message(
                    "account.conflict",
                    format!("account changed before field update: {}", account.id),
                ));
            }
            get_by_id_conn(conn, &account.id)?.ok_or_else(|| {
                AppError::message("db.account", "account missing after field update")
            })
        })
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        self.mutate(|conn| {
            let n = conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
            if n == 0 {
                return Err(AppError::NotFound(format!("account not found: {id}")));
            }
            Ok(())
        })
    }

    pub fn delete_for_agent(&self, id: &str, agent: AgentId) -> Result<()> {
        self.mutate(|conn| {
            let existing = get_by_id_conn(conn, id)?
                .ok_or_else(|| AppError::NotFound(format!("account not found: {id}")))?;
            if existing.agent_id != agent {
                return Err(AppError::NotFound(format!(
                    "account not found: {id} (agent filter: {})",
                    agent.as_str()
                )));
            }
            let n = conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
            if n == 0 {
                return Err(AppError::NotFound(format!("account not found: {id}")));
            }
            Ok(())
        })
    }

    pub fn list(&self, agent: Option<AgentId>) -> Result<Vec<Account>> {
        self.db.with_conn(|conn| {
            let mut out = Vec::new();
            if let Some(agent) = agent {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, agent_id, kind, label, credentials, extra,
                           status, is_current, created_at, updated_at
                    FROM accounts
                    WHERE agent_id = ?1
                    ORDER BY agent_id ASC, label ASC, id ASC
                    "#,
                )?;
                let rows = stmt.query_map(params![agent.as_str()], map_account_row)?;
                for row in rows {
                    out.push(row?);
                }
            } else {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, agent_id, kind, label, credentials, extra,
                           status, is_current, created_at, updated_at
                    FROM accounts
                    ORDER BY agent_id ASC, label ASC, id ASC
                    "#,
                )?;
                let rows = stmt.query_map([], map_account_row)?;
                for row in rows {
                    out.push(row?);
                }
            }
            Ok(out)
        })
    }

    pub fn get_by_id(&self, id: &str) -> Result<Option<Account>> {
        self.db.with_conn(|conn| get_by_id_conn(conn, id))
    }

    pub fn get_current(&self, agent: AgentId) -> Result<Option<Account>> {
        self.db.with_conn(|conn| get_current_conn(conn, agent))
    }

    /// Clear `is_current` for every account of `agent` (cross-pool demotion).
    ///
    /// Used when a provider becomes the active live auth path so official
    /// account and API config stay mutually exclusive at the DB layer.
    pub fn clear_current(&self, agent: AgentId) -> Result<()> {
        self.mutate(|conn| {
            conn.execute(
                "UPDATE accounts SET is_current = 0 WHERE agent_id = ?1 AND is_current != 0",
                params![agent.as_str()],
            )?;
            Ok(())
        })
    }

    pub fn list_by_label(&self, label: &str, agent: Option<AgentId>) -> Result<Vec<Account>> {
        self.db.with_conn(|conn| {
            let mut out = Vec::new();
            if let Some(agent) = agent {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, agent_id, kind, label, credentials, extra,
                           status, is_current, created_at, updated_at
                    FROM accounts
                    WHERE label = ?1 AND agent_id = ?2
                    ORDER BY agent_id ASC, label ASC, id ASC
                    "#,
                )?;
                let rows = stmt.query_map(params![label, agent.as_str()], map_account_row)?;
                for row in rows {
                    out.push(row?);
                }
            } else {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, agent_id, kind, label, credentials, extra,
                           status, is_current, created_at, updated_at
                    FROM accounts
                    WHERE label = ?1
                    ORDER BY agent_id ASC, label ASC, id ASC
                    "#,
                )?;
                let rows = stmt.query_map(params![label], map_account_row)?;
                for row in rows {
                    out.push(row?);
                }
            }
            Ok(out)
        })
    }

    /// Persist the complete live credentials into the observed current account.
    pub fn backfill_current(
        &self,
        observed: &Account,
        credentials: &serde_json::Value,
        updated_at: &str,
    ) -> Result<Account> {
        self.mutate(|conn| {
            let current = get_current_conn(conn, observed.agent_id)?.ok_or_else(|| {
                AppError::message("account.state", "current account changed before backfill")
            })?;
            if current.id != observed.id || current.updated_at != observed.updated_at {
                return Err(AppError::message(
                    "account.state",
                    "current account changed before backfill",
                ));
            }
            let raw = serde_json::to_string(credentials)?;
            let changed = conn.execute(
                r#"
                UPDATE accounts
                SET credentials = ?2, updated_at = ?3
                WHERE id = ?1 AND agent_id = ?4 AND is_current != 0
                  AND updated_at = ?5
                "#,
                params![
                    observed.id,
                    raw,
                    updated_at,
                    observed.agent_id.as_str(),
                    observed.updated_at,
                ],
            )?;
            if changed != 1 {
                return Err(AppError::message(
                    "account.state",
                    "current account changed during backfill",
                ));
            }
            get_by_id_conn(conn, &observed.id)?.ok_or_else(|| {
                AppError::message("db.account", "account missing after live backfill")
            })
        })
    }

    /// Restore the exact row changed by a successful live backfill.
    pub fn restore_backfill(
        &self,
        original: &Account,
        expected_backfilled_updated_at: &str,
    ) -> Result<()> {
        self.mutate(|conn| {
            let current = get_current_conn(conn, original.agent_id)?.ok_or_else(|| {
                AppError::message(
                    "account.state",
                    "current account changed before backfill rollback",
                )
            })?;
            if current.id != original.id || current.updated_at != expected_backfilled_updated_at {
                return Err(AppError::message(
                    "account.state",
                    "current account changed before backfill rollback",
                ));
            }
            let raw = serde_json::to_string(&original.credentials)?;
            let changed = conn.execute(
                r#"
                UPDATE accounts
                SET credentials = ?2, updated_at = ?3
                WHERE id = ?1 AND agent_id = ?4 AND is_current != 0
                  AND updated_at = ?5
                "#,
                params![
                    original.id,
                    raw,
                    original.updated_at,
                    original.agent_id.as_str(),
                    expected_backfilled_updated_at,
                ],
            )?;
            if changed != 1 {
                return Err(AppError::message(
                    "account.state",
                    "account changed during backfill rollback",
                ));
            }
            Ok(())
        })
    }

    /// Atomically select an account after backup and live apply succeed.
    pub fn select_current(
        &self,
        target_id: &str,
        agent: AgentId,
        expected_target_updated_at: &str,
        updated_at: &str,
    ) -> Result<Account> {
        self.mutate(|conn| {
            select_current_conn(
                conn,
                target_id,
                agent,
                expected_target_updated_at,
                updated_at,
            )
        })
    }

    fn mutate<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        self.db.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
            let out = f(&tx)?;
            tx.commit()?;
            Ok(out)
        })
    }
}

/// Clear currents (connection-scoped) for composing multi-table transactions.
pub(crate) fn clear_current_conn(conn: &Connection, agent: AgentId) -> Result<()> {
    conn.execute(
        "UPDATE accounts SET is_current = 0 WHERE agent_id = ?1 AND is_current != 0",
        params![agent.as_str()],
    )?;
    Ok(())
}

/// Connection-scoped get by id.
pub(crate) fn get_by_id_conn_pub(conn: &Connection, id: &str) -> Result<Option<Account>> {
    get_by_id_conn(conn, id)
}

/// Force one account to be the sole current for `agent` (no expected-revision check).
///
/// Used by active-binding reconcile so legacy `is_current` mirrors a valid binding.
pub(crate) fn force_sole_current_conn(
    conn: &Connection,
    target_id: &str,
    agent: AgentId,
    updated_at: &str,
) -> Result<Account> {
    let target = get_by_id_conn(conn, target_id)?
        .ok_or_else(|| AppError::NotFound(format!("account not found: {target_id}")))?;
    if target.agent_id != agent {
        return Err(AppError::NotFound(format!(
            "account not found: {target_id} (agent filter: {})",
            agent.as_str()
        )));
    }
    conn.execute(
        "UPDATE accounts SET is_current = 0, updated_at = ?2 WHERE agent_id = ?1 AND is_current != 0",
        params![agent.as_str(), updated_at],
    )?;
    let changed = conn.execute(
        "UPDATE accounts SET is_current = 1, updated_at = ?3 WHERE id = ?1 AND agent_id = ?2",
        params![target_id, agent.as_str(), updated_at],
    )?;
    if changed != 1 {
        return Err(AppError::message(
            "db.account",
            format!("account missing while enforcing current: {target_id}"),
        ));
    }
    get_by_id_conn(conn, target_id)?
        .ok_or_else(|| AppError::message("db.account", "account missing after enforce current"))
}

/// Connection-scoped list of rows with `is_current != 0` for an agent.
pub(crate) fn list_current_conn(conn: &Connection, agent: AgentId) -> Result<Vec<Account>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, agent_id, kind, label, credentials, extra,
               status, is_current, created_at, updated_at
        FROM accounts
        WHERE agent_id = ?1 AND is_current != 0
        "#,
    )?;
    let rows = stmt.query_map(params![agent.as_str()], map_account_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Connection-scoped list of every account for one agent.
pub(crate) fn list_for_agent_conn(conn: &Connection, agent: AgentId) -> Result<Vec<Account>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, agent_id, kind, label, credentials, extra,
               status, is_current, created_at, updated_at
        FROM accounts
        WHERE agent_id = ?1
        ORDER BY agent_id ASC, label ASC, id ASC
        "#,
    )?;
    let rows = stmt.query_map(params![agent.as_str()], map_account_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Update one account only when `updated_at` still matches the caller's snapshot.
pub(crate) fn update_if_revision_conn(
    conn: &Connection,
    account: &Account,
    expected_updated_at: &str,
) -> Result<Account> {
    let existing = get_by_id_conn(conn, &account.id)?
        .ok_or_else(|| AppError::NotFound(format!("account not found: {}", account.id)))?;
    if existing.agent_id != account.agent_id {
        return Err(AppError::InvalidArg(format!(
            "account agent_id is immutable (id={}, existing={}, requested={})",
            account.id,
            existing.agent_id.as_str(),
            account.agent_id.as_str()
        )));
    }
    let mut row = account.clone();
    row.created_at = existing.created_at;
    let credentials = serde_json::to_string(&row.credentials)?;
    let extra = serde_json::to_string(&row.extra)?;
    let is_current: i64 = if row.is_current { 1 } else { 0 };
    let n = conn.execute(
        r#"
        UPDATE accounts SET
            agent_id = ?2,
            kind = ?3,
            label = ?4,
            credentials = ?5,
            extra = ?6,
            status = ?7,
            is_current = ?8,
            created_at = ?9,
            updated_at = ?10
        WHERE id = ?1 AND agent_id = ?11 AND updated_at = ?12
        "#,
        params![
            row.id,
            row.agent_id.as_str(),
            row.kind.as_str(),
            row.label,
            credentials,
            extra,
            row.status,
            is_current,
            row.created_at,
            row.updated_at,
            row.agent_id.as_str(),
            expected_updated_at,
        ],
    )?;
    if n != 1 {
        return Err(AppError::message(
            "account.merge.conflict",
            format!("account changed before update: {}", account.id),
        ));
    }
    clear_other_currents_if_needed(conn, &row)?;
    get_by_id_conn(conn, &row.id)?
        .ok_or_else(|| AppError::message("db.account", "account missing after revision update"))
}

/// Delete one account only when `updated_at` still matches the caller's snapshot.
pub(crate) fn delete_if_revision_conn(
    conn: &Connection,
    id: &str,
    agent: AgentId,
    expected_updated_at: &str,
) -> Result<()> {
    let n = conn.execute(
        "DELETE FROM accounts WHERE id = ?1 AND agent_id = ?2 AND updated_at = ?3",
        params![id, agent.as_str(), expected_updated_at],
    )?;
    if n == 1 {
        return Ok(());
    }
    if get_by_id_conn(conn, id)?.is_some() {
        Err(AppError::message(
            "account.merge.delete.conflict",
            format!("account changed before duplicate deletion: {id}"),
        ))
    } else {
        Err(AppError::NotFound(format!(
            "account not found: {id} (agent filter: {})",
            agent.as_str()
        )))
    }
}

/// Connection-scoped create (insert + same-table current demotion).
pub(crate) fn create_conn(conn: &Connection, account: &Account) -> Result<Account> {
    if get_by_id_conn(conn, &account.id)?.is_some() {
        return Err(AppError::InvalidArg(format!(
            "account already exists: {}",
            account.id
        )));
    }
    insert_row(conn, account)?;
    clear_other_currents_if_needed(conn, account)?;
    get_by_id_conn(conn, &account.id)?
        .ok_or_else(|| AppError::message("db.account", "account missing after create"))
}

/// Connection-scoped update (preserve created_at + same-table current demotion).
pub(crate) fn update_conn(conn: &Connection, account: &Account) -> Result<Account> {
    let existing = get_by_id_conn(conn, &account.id)?
        .ok_or_else(|| AppError::NotFound(format!("account not found: {}", account.id)))?;
    if existing.agent_id != account.agent_id {
        return Err(AppError::InvalidArg(format!(
            "account agent_id is immutable (id={}, existing={}, requested={})",
            account.id,
            existing.agent_id.as_str(),
            account.agent_id.as_str()
        )));
    }
    let mut row = account.clone();
    row.created_at = existing.created_at;
    update_row(conn, &row)?;
    clear_other_currents_if_needed(conn, &row)?;
    get_by_id_conn(conn, &row.id)?
        .ok_or_else(|| AppError::message("db.account", "account missing after update"))
}

/// Connection-scoped delete scoped to agent.
pub(crate) fn delete_for_agent_conn(conn: &Connection, id: &str, agent: AgentId) -> Result<()> {
    let existing = get_by_id_conn(conn, id)?
        .ok_or_else(|| AppError::NotFound(format!("account not found: {id}")))?;
    if existing.agent_id != agent {
        return Err(AppError::NotFound(format!(
            "account not found: {id} (agent filter: {})",
            agent.as_str()
        )));
    }
    let n = conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(AppError::NotFound(format!("account not found: {id}")));
    }
    Ok(())
}

/// Connection-scoped select_current for composing multi-table transactions.
pub(crate) fn select_current_conn(
    conn: &Connection,
    target_id: &str,
    agent: AgentId,
    expected_target_updated_at: &str,
    updated_at: &str,
) -> Result<Account> {
    let target = get_by_id_conn(conn, target_id)?
        .ok_or_else(|| AppError::NotFound(format!("account not found: {target_id}")))?;
    if target.agent_id != agent {
        return Err(AppError::NotFound(format!(
            "account not found: {target_id} (agent filter: {})",
            agent.as_str()
        )));
    }
    if target.updated_at != expected_target_updated_at {
        return Err(AppError::message(
            "account.state",
            format!("account changed before switch: {target_id}"),
        ));
    }
    let _ = get_current_conn(conn, agent)?;
    conn.execute(
        "UPDATE accounts SET is_current = 0, updated_at = ?2 WHERE agent_id = ?1 AND is_current != 0",
        params![agent.as_str(), updated_at],
    )?;
    let changed = conn.execute(
        "UPDATE accounts SET is_current = 1, updated_at = ?3 WHERE id = ?1 AND agent_id = ?2",
        params![target_id, agent.as_str(), updated_at],
    )?;
    if changed != 1 {
        return Err(AppError::message(
            "db.account",
            format!("account missing during switch: {target_id}"),
        ));
    }
    get_by_id_conn(conn, target_id)?
        .ok_or_else(|| AppError::message("db.account", "account missing after switch"))
}

fn get_by_id_conn(conn: &Connection, id: &str) -> Result<Option<Account>> {
    conn.query_row(
        r#"
        SELECT id, agent_id, kind, label, credentials, extra,
               status, is_current, created_at, updated_at
        FROM accounts
        WHERE id = ?1
        "#,
        params![id],
        map_account_row,
    )
    .optional()
    .map_err(AppError::from)
}

fn get_current_conn(conn: &Connection, agent: AgentId) -> Result<Option<Account>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, agent_id, kind, label, credentials, extra,
               status, is_current, created_at, updated_at
        FROM accounts
        WHERE agent_id = ?1 AND is_current != 0
        ORDER BY id ASC
        "#,
    )?;
    let mut rows = stmt.query(params![agent.as_str()])?;
    let first = rows.next()?.map(map_account_row).transpose()?;
    if rows.next()?.is_some() {
        return Err(AppError::message(
            "account.state",
            format!(
                "multiple current accounts found for agent {}",
                agent.as_str()
            ),
        ));
    }
    Ok(first)
}

fn insert_row(conn: &Connection, account: &Account) -> Result<()> {
    let credentials = serde_json::to_string(&account.credentials)?;
    let extra = serde_json::to_string(&account.extra)?;
    let is_current: i64 = if account.is_current { 1 } else { 0 };
    conn.execute(
        r#"
        INSERT INTO accounts (
            id, agent_id, kind, label, credentials, extra,
            status, is_current, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            account.id,
            account.agent_id.as_str(),
            account.kind.as_str(),
            account.label,
            credentials,
            extra,
            account.status,
            is_current,
            account.created_at,
            account.updated_at,
        ],
    )?;
    Ok(())
}

fn update_row(conn: &Connection, account: &Account) -> Result<()> {
    let credentials = serde_json::to_string(&account.credentials)?;
    let extra = serde_json::to_string(&account.extra)?;
    let is_current: i64 = if account.is_current { 1 } else { 0 };
    let n = conn.execute(
        r#"
        UPDATE accounts SET
            agent_id = ?2,
            kind = ?3,
            label = ?4,
            credentials = ?5,
            extra = ?6,
            status = ?7,
            is_current = ?8,
            created_at = ?9,
            updated_at = ?10
        WHERE id = ?1
        "#,
        params![
            account.id,
            account.agent_id.as_str(),
            account.kind.as_str(),
            account.label,
            credentials,
            extra,
            account.status,
            is_current,
            account.created_at,
            account.updated_at,
        ],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!(
            "account not found: {}",
            account.id
        )));
    }
    Ok(())
}

fn clear_other_currents_if_needed(conn: &Connection, account: &Account) -> Result<()> {
    if !account.is_current {
        return Ok(());
    }
    conn.execute(
        r#"
        UPDATE accounts
        SET is_current = 0
        WHERE agent_id = ?1 AND id != ?2 AND is_current != 0
        "#,
        params![account.agent_id.as_str(), account.id],
    )?;
    Ok(())
}

fn map_account_row(row: &Row<'_>) -> rusqlite::Result<Account> {
    let id: String = row.get(0)?;
    let agent_raw: String = row.get(1)?;
    let agent_id = AgentId::parse(&agent_raw).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid agent_id in accounts row: {agent_raw}"),
            )),
        )
    })?;
    let kind_raw: String = row.get(2)?;
    let kind = AccountKind::parse(&kind_raw).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid account kind: {kind_raw}"),
            )),
        )
    })?;
    let label: Option<String> = row.get(3)?;
    let credentials_raw: String = row.get(4)?;
    let extra_raw: String = row.get(5)?;
    let status: String = row.get(6)?;
    let is_current_i: i64 = row.get(7)?;
    let created_at: String = row.get(8)?;
    let updated_at: String = row.get(9)?;

    let credentials = serde_json::from_str(&credentials_raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let extra = serde_json::from_str(&extra_raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
    })?;

    Ok(Account {
        id,
        agent_id,
        kind,
        label: label.unwrap_or_default(),
        credentials,
        extra,
        status,
        is_current: is_current_i != 0,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests;
