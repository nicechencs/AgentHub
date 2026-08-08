//! agent_active_bindings repository — one current pointer per agent_key.

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{AppError, Result};
use crate::platform::AgentKey;
use crate::storage::Database;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveBindingRow {
    pub agent_key: String,
    pub account_id: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub config_profile_id: Option<String>,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Convenience wrapper used by unit tests. Production writers use ConnectionService.
#[derive(Clone)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct ActiveBindingRepo {
    db: Database,
}

#[cfg_attr(not(test), allow(dead_code))]
impl ActiveBindingRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Read-only lookup. Production writers must go through [`crate::services::ConnectionService`].
    pub fn get(&self, agent_key: &str) -> Result<Option<ActiveBindingRow>> {
        self.db.with_conn(|conn| get_conn(conn, agent_key))
    }

    /// Crate-private write path for tests and ConnectionService composition only.
    pub(crate) fn upsert(&self, row: &ActiveBindingRow) -> Result<ActiveBindingRow> {
        self.db.with_conn(|conn| {
            upsert_conn(conn, row)?;
            get_conn(conn, &row.agent_key)?
                .ok_or_else(|| AppError::message("db.binding", "binding missing after upsert"))
        })
    }

    /// Crate-private write path for tests and ConnectionService composition only.
    pub(crate) fn clear(&self, agent_key: &str) -> Result<()> {
        self.db.with_conn(|conn| clear_conn(conn, agent_key))
    }

    /// Crate-private write path for tests and ConnectionService composition only.
    pub(crate) fn set_refs(
        &self,
        agent_key: &str,
        account_id: Option<String>,
        provider_id: Option<String>,
        model_id: Option<String>,
        now: &str,
    ) -> Result<ActiveBindingRow> {
        self.db.with_conn(|conn| {
            set_refs_conn(conn, agent_key, account_id, provider_id, model_id, now)
        })
    }
}

/// Connection-scoped get for multi-table transactions.
pub(crate) fn get_conn_pub(conn: &Connection, agent_key: &str) -> Result<Option<ActiveBindingRow>> {
    get_conn(conn, agent_key)
}

/// Delete the entire binding row (including model/profile). Used by explicit clear.
pub(crate) fn clear_conn(conn: &Connection, agent_key: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM agent_active_bindings WHERE agent_key = ?1",
        params![agent_key],
    )?;
    Ok(())
}

/// Clear only account_id/provider_id; preserve model_id/config_profile_id.
///
/// If all four reference fields would be empty, deletes the meaningless row and
/// returns `None`.
pub(crate) fn clear_connection_refs_conn(
    conn: &Connection,
    agent_key: &str,
    now: &str,
) -> Result<Option<ActiveBindingRow>> {
    let Some(existing) = get_conn(conn, agent_key)? else {
        return Ok(None);
    };
    if existing.model_id.is_none() && existing.config_profile_id.is_none() {
        clear_conn(conn, agent_key)?;
        return Ok(None);
    }
    let row = ActiveBindingRow {
        agent_key: agent_key.to_string(),
        account_id: None,
        provider_id: None,
        model_id: existing.model_id,
        config_profile_id: existing.config_profile_id,
        revision: existing.revision + 1,
        created_at: existing.created_at,
        updated_at: now.to_string(),
    };
    upsert_conn(conn, &row)?;
    get_conn(conn, agent_key)?
        .ok_or_else(|| {
            AppError::message("db.binding", "binding missing after clear_connection_refs")
        })
        .map(Some)
}

/// Set only connection-side refs (account/provider). Preserves model_id and
/// config_profile_id so Account/Provider lifecycle never wipes independent fields.
pub(crate) fn set_connection_refs_conn(
    conn: &Connection,
    agent_key: &str,
    account_id: Option<String>,
    provider_id: Option<String>,
    now: &str,
) -> Result<ActiveBindingRow> {
    let existing = get_conn(conn, agent_key)?;
    let revision = existing.as_ref().map(|r| r.revision + 1).unwrap_or(1);
    let created_at = existing
        .as_ref()
        .map(|r| r.created_at.clone())
        .unwrap_or_else(|| now.to_string());
    let model_id = existing.as_ref().and_then(|r| r.model_id.clone());
    let config_profile_id = existing.as_ref().and_then(|r| r.config_profile_id.clone());
    let row = ActiveBindingRow {
        agent_key: agent_key.to_string(),
        account_id,
        provider_id,
        model_id,
        config_profile_id,
        revision,
        created_at,
        updated_at: now.to_string(),
    };
    upsert_conn(conn, &row)?;
    get_conn(conn, agent_key)?
        .ok_or_else(|| AppError::message("db.binding", "binding missing after set_connection_refs"))
}

/// Connection-scoped set_refs (may set model_id explicitly). Prefer
/// [`set_connection_refs_conn`] for Account/Provider lifecycle paths.
pub(crate) fn set_refs_conn(
    conn: &Connection,
    agent_key: &str,
    account_id: Option<String>,
    provider_id: Option<String>,
    model_id: Option<String>,
    now: &str,
) -> Result<ActiveBindingRow> {
    let existing = get_conn(conn, agent_key)?;
    let revision = existing.as_ref().map(|r| r.revision + 1).unwrap_or(1);
    let created_at = existing
        .as_ref()
        .map(|r| r.created_at.clone())
        .unwrap_or_else(|| now.to_string());
    // Explicit model_id argument; config_profile_id is always preserved.
    let config_profile_id = existing.and_then(|r| r.config_profile_id);
    let row = ActiveBindingRow {
        agent_key: agent_key.to_string(),
        account_id,
        provider_id,
        model_id,
        config_profile_id,
        revision,
        created_at,
        updated_at: now.to_string(),
    };
    upsert_conn(conn, &row)?;
    get_conn(conn, agent_key)?
        .ok_or_else(|| AppError::message("db.binding", "binding missing after set_refs"))
}

fn map_row(row: &Row<'_>) -> rusqlite::Result<ActiveBindingRow> {
    Ok(ActiveBindingRow {
        agent_key: row.get(0)?,
        account_id: row.get(1)?,
        provider_id: row.get(2)?,
        model_id: row.get(3)?,
        config_profile_id: row.get(4)?,
        revision: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn get_conn(conn: &Connection, agent_key: &str) -> Result<Option<ActiveBindingRow>> {
    conn.query_row(
        r#"
        SELECT agent_key, account_id, provider_id, model_id, config_profile_id,
               revision, created_at, updated_at
        FROM agent_active_bindings
        WHERE agent_key = ?1
        "#,
        params![agent_key],
        map_row,
    )
    .optional()
    .map_err(Into::into)
}

fn upsert_conn(conn: &Connection, row: &ActiveBindingRow) -> Result<()> {
    let _ = AgentKey::parse(row.agent_key.as_str())?;
    conn.execute(
        r#"
        INSERT INTO agent_active_bindings (
            agent_key, account_id, provider_id, model_id, config_profile_id,
            revision, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(agent_key) DO UPDATE SET
            account_id = excluded.account_id,
            provider_id = excluded.provider_id,
            model_id = excluded.model_id,
            config_profile_id = excluded.config_profile_id,
            revision = excluded.revision,
            updated_at = excluded.updated_at
        "#,
        params![
            row.agent_key,
            row.account_id,
            row.provider_id,
            row.model_id,
            row.config_profile_id,
            row.revision,
            row.created_at,
            row.updated_at,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
