//! Backups table repository — storage boundary only (no file I/O).

use rusqlite::{params, OptionalExtension, Row};

use crate::error::{AppError, Result};
use crate::models::{AgentId, BackupKind, BackupRecord};
use crate::storage::Database;

/// SQLite access for the `backups` table.
#[derive(Clone)]
pub struct BackupRepo {
    db: Database,
}

impl BackupRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub(crate) fn keep_live_file_copies(&self) -> bool {
        self.db
            .load_app_settings()
            .map(|settings| settings.keep_live_file_copies)
            .unwrap_or(true)
    }

    /// Insert a fully-formed backup index row.
    pub fn insert(&self, record: &BackupRecord) -> Result<()> {
        let files = serde_json::to_string(&record.files)?;
        let agent_id = record.agent_id.map(|a| a.as_str().to_string());
        let size = i64::try_from(record.size).map_err(|_| {
            AppError::InvalidArg(format!("backup size exceeds i64 range: {}", record.size))
        })?;
        self.db.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO backups (
                    id, agent_id, kind, path, files, size, note, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    record.id,
                    agent_id,
                    record.kind.as_str(),
                    record.path,
                    files,
                    size,
                    record.note,
                    record.created_at,
                ],
            )?;
            Ok(())
        })
    }

    /// List backups newest-first (`created_at DESC`, then `id DESC`).
    /// Optional agent filter matches `agent_id` exactly.
    pub fn list(&self, agent: Option<AgentId>) -> Result<Vec<BackupRecord>> {
        self.db.with_conn(|conn| {
            let mut out = Vec::new();
            if let Some(agent) = agent {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, agent_id, kind, path, files, size, note, created_at
                    FROM backups
                    WHERE agent_id = ?1
                    ORDER BY created_at DESC, id DESC
                    "#,
                )?;
                let rows = stmt.query_map(params![agent.as_str()], map_backup_row)?;
                for row in rows {
                    out.push(row?);
                }
            } else {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, agent_id, kind, path, files, size, note, created_at
                    FROM backups
                    ORDER BY created_at DESC, id DESC
                    "#,
                )?;
                let rows = stmt.query_map([], map_backup_row)?;
                for row in rows {
                    out.push(row?);
                }
            }
            Ok(out)
        })
    }

    pub fn get_by_id(&self, id: &str) -> Result<Option<BackupRecord>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                r#"
                SELECT id, agent_id, kind, path, files, size, note, created_at
                FROM backups
                WHERE id = ?1
                "#,
                params![id],
                map_backup_row,
            )
            .optional()
            .map_err(AppError::from)
        })
    }

    /// Bump `created_at` on an existing row. All other columns stay unchanged.
    /// Used when a new live snapshot is byte-identical to a historical one.
    pub fn touch_created_at(&self, id: &str, created_at: &str) -> Result<BackupRecord> {
        self.db.with_conn(|conn| {
            let n = conn.execute(
                "UPDATE backups SET created_at = ?1 WHERE id = ?2",
                params![created_at, id],
            )?;
            if n == 0 {
                return Err(AppError::NotFound(format!("backup not found: {id}")));
            }
            Ok(())
        })?;
        self.get_by_id(id)?
            .ok_or_else(|| AppError::NotFound(format!("backup not found: {id}")))
    }

    /// Delete a backup index row by id. Returns `true` if a row was removed.
    pub fn delete(&self, id: &str) -> Result<bool> {
        self.db.with_conn(|conn| {
            let n = conn.execute("DELETE FROM backups WHERE id = ?1", params![id])?;
            Ok(n > 0)
        })
    }
}

fn map_backup_row(row: &Row<'_>) -> rusqlite::Result<BackupRecord> {
    let id: String = row.get(0)?;
    let agent_raw: Option<String> = row.get(1)?;
    let agent_id = match agent_raw {
        None => None,
        Some(raw) if raw.is_empty() => None,
        Some(raw) => Some(AgentId::parse(&raw).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid agent_id in backups row: {raw}"),
                )),
            )
        })?),
    };
    let kind_raw: String = row.get(2)?;
    let kind = BackupKind::parse(&kind_raw).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid backup kind: {kind_raw}"),
            )),
        )
    })?;
    let path: String = row.get(3)?;
    let files_raw: String = row.get(4)?;
    let size_i: i64 = row.get(5)?;
    let note: Option<String> = row.get(6)?;
    let created_at: String = row.get(7)?;

    let files: Vec<String> = serde_json::from_str(&files_raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let size = u64::try_from(size_i).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("negative backup size: {size_i}"),
            )),
        )
    })?;

    Ok(BackupRecord {
        id,
        agent_id,
        kind,
        path,
        files,
        size,
        note,
        created_at,
    })
}

#[cfg(test)]
mod tests;
