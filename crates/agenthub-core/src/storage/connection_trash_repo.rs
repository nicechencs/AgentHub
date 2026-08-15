//! Connection trash (recovery bin) repository — SQL only, no orchestration.

use chrono::{Duration, Utc};
use rusqlite::{params, Connection};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::models::{AgentId, ConnectionTrashItem, ConnectionTrashKind};
use crate::storage::Database;

/// SQLite access for the `connection_trash` table.
#[derive(Clone)]
pub struct ConnectionTrashRepo {
    db: Database,
}

#[derive(Debug)]
pub(crate) struct TrashPayloadRow {
    pub kind: ConnectionTrashKind,
    pub source_id: String,
    pub agent_id: AgentId,
    pub payload: Value,
}

impl ConnectionTrashRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// List recoverable rows after purging expired entries.
    pub fn list(&self, agent: Option<AgentId>, now: &str) -> Result<Vec<ConnectionTrashItem>> {
        self.db.with_conn(|conn| {
            purge_expired_conn(conn, now)?;
            list_conn(conn, agent)
        })
    }

    /// Permanently remove one recovery-bin row.
    pub fn delete(&self, id: &str) -> Result<()> {
        self.db.with_conn(|conn| delete_conn(conn, id))
    }

    pub(crate) fn insert_conn<T: serde::Serialize>(
        conn: &Connection,
        source_id: &str,
        agent_id: AgentId,
        kind: ConnectionTrashKind,
        label: &str,
        was_current: bool,
        payload: &T,
        deleted_at: &str,
    ) -> Result<()> {
        insert_trash_conn(
            conn,
            source_id,
            agent_id,
            kind,
            label,
            was_current,
            payload,
            deleted_at,
        )
    }

    pub(crate) fn load_payload_conn(conn: &Connection, id: &str) -> Result<TrashPayloadRow> {
        load_trash_payload_conn(conn, id)
    }

    pub(crate) fn delete_conn(conn: &Connection, id: &str) -> Result<()> {
        delete_conn(conn, id)
    }
}

fn purge_expired_conn(conn: &Connection, now: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM connection_trash WHERE expires_at <= ?1",
        params![now],
    )?;
    Ok(())
}

fn list_conn(conn: &Connection, agent: Option<AgentId>) -> Result<Vec<ConnectionTrashItem>> {
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

fn delete_conn(conn: &Connection, id: &str) -> Result<()> {
    let n = conn.execute("DELETE FROM connection_trash WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(AppError::NotFound(format!("trash item not found: {id}")));
    }
    Ok(())
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
