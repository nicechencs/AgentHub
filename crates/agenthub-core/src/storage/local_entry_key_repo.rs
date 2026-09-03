//! Named extra loopback bearers. Empty `token` is only a display name for the
//! pool's default hub token.

use rusqlite::{params, OptionalExtension};

use crate::error::{AppError, Result};
use crate::storage::Database;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalEntryKey {
    pub id: String,
    pub pool_id: String,
    pub name: String,
    pub token: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct LocalEntryKeyRepo {
    db: Database,
}

impl LocalEntryKeyRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn list(&self) -> Result<Vec<LocalEntryKey>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, pool_id, name, token, created_at, updated_at
                FROM local_entry_keys
                ORDER BY created_at ASC, id ASC
                "#,
            )?;
            let rows = stmt.query_map([], map_row)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(AppError::from)
        })
    }

    pub fn get(&self, id: &str) -> Result<Option<LocalEntryKey>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                r#"
                SELECT id, pool_id, name, token, created_at, updated_at
                FROM local_entry_keys
                WHERE id = ?1
                "#,
                params![id],
                map_row,
            )
            .optional()
            .map_err(AppError::from)
        })
    }

    pub fn insert(&self, row: &LocalEntryKey) -> Result<LocalEntryKey> {
        validate(row)?;
        self.db.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO local_entry_keys (id, pool_id, name, token, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    row.id,
                    row.pool_id,
                    row.name,
                    row.token,
                    row.created_at,
                    row.updated_at,
                ],
            )
            .map_err(map_constraint)?;
            Ok(())
        })?;
        self.get(&row.id)?
            .ok_or_else(|| AppError::message("db.local_entry_key", "row missing after insert"))
    }

    pub fn update(&self, row: &LocalEntryKey) -> Result<LocalEntryKey> {
        validate(row)?;
        self.db.with_conn(|conn| {
            let changed = conn.execute(
                r#"
                UPDATE local_entry_keys
                SET pool_id = ?2, name = ?3, token = ?4, updated_at = ?5
                WHERE id = ?1
                "#,
                params![row.id, row.pool_id, row.name, row.token, row.updated_at],
            )
            .map_err(map_constraint)?;
            if changed == 0 {
                return Err(AppError::NotFound(format!(
                    "entry key not found: {}",
                    row.id
                )));
            }
            Ok(())
        })?;
        self.get(&row.id)?
            .ok_or_else(|| AppError::message("db.local_entry_key", "row missing after update"))
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            let changed = conn.execute("DELETE FROM local_entry_keys WHERE id = ?1", params![id])?;
            if changed == 0 {
                return Err(AppError::NotFound(format!("entry key not found: {id}")));
            }
            Ok(())
        })
    }
}

fn validate(row: &LocalEntryKey) -> Result<()> {
    if row.id.trim().is_empty() {
        return Err(AppError::InvalidArg("entry key id must not be empty".into()));
    }
    if row.pool_id.trim().is_empty() {
        return Err(AppError::InvalidArg(
            "entry key pool id must not be empty".into(),
        ));
    }
    if row.name.trim().is_empty() {
        return Err(AppError::InvalidArg(
            "entry key name must not be empty".into(),
        ));
    }
    if row.created_at.trim().is_empty() || row.updated_at.trim().is_empty() {
        return Err(AppError::InvalidArg(
            "entry key timestamps must not be empty".into(),
        ));
    }
    Ok(())
}

fn map_constraint(error: rusqlite::Error) -> AppError {
    match &error {
        rusqlite::Error::SqliteFailure(info, _)
            if info.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            AppError::InvalidArg("entry key already exists".into())
        }
        _ => AppError::from(error),
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LocalEntryKey> {
    Ok(LocalEntryKey {
        id: row.get(0)?,
        pool_id: row.get(1)?,
        name: row.get(2)?,
        token: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}
