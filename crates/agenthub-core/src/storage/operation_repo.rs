//! SQLite access for the `operations` lifecycle audit table.

use rusqlite::{params, OptionalExtension, Row};

use crate::error::{AppError, Result};
use crate::platform::lifecycle::{OperationKind, OperationRecord, OperationStatus};
use crate::storage::Database;

#[derive(Clone)]
pub struct OperationRepo {
    db: Database,
}

impl OperationRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn insert_running(
        &self,
        id: &str,
        agent_key: &str,
        kind: OperationKind,
        step: &str,
        started_at: &str,
    ) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO operations (
                    id, agent_key, kind, status, step, error_code, summary,
                    observed_status, observed_version, started_at, finished_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, NULL, NULL, ?6, NULL)
                "#,
                params![
                    id,
                    agent_key,
                    kind.as_str(),
                    OperationStatus::Running.as_str(),
                    step,
                    started_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn update_step(&self, id: &str, step: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            let n = conn.execute(
                "UPDATE operations SET step = ?1 WHERE id = ?2 AND status = ?3",
                params![step, id, OperationStatus::Running.as_str()],
            )?;
            if n == 0 {
                return Err(AppError::NotFound(format!(
                    "running operation not found: {id}"
                )));
            }
            Ok(())
        })
    }

    pub fn finalize(
        &self,
        id: &str,
        status: OperationStatus,
        step: Option<&str>,
        error_code: Option<&str>,
        summary: Option<&str>,
        observed_status: Option<&str>,
        observed_version: Option<&str>,
        finished_at: &str,
    ) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                r#"
                UPDATE operations SET
                    status = ?1,
                    step = COALESCE(?2, step),
                    error_code = ?3,
                    summary = ?4,
                    observed_status = ?5,
                    observed_version = ?6,
                    finished_at = ?7
                WHERE id = ?8
                "#,
                params![
                    status.as_str(),
                    step,
                    error_code,
                    summary,
                    observed_status,
                    observed_version,
                    finished_at,
                    id,
                ],
            )?;
            Ok(())
        })
    }

    /// Mark all still-running rows as interrupted (process restart recovery).
    pub fn interrupt_all_running(&self, finished_at: &str) -> Result<u64> {
        self.db.with_conn(|conn| {
            let n = conn.execute(
                r#"
                UPDATE operations SET
                    status = ?1,
                    error_code = 'lifecycle.interrupted',
                    summary = COALESCE(summary, 'process restarted while operation was running'),
                    finished_at = ?2
                WHERE status = ?3
                "#,
                params![
                    OperationStatus::Interrupted.as_str(),
                    finished_at,
                    OperationStatus::Running.as_str(),
                ],
            )?;
            Ok(n as u64)
        })
    }

    pub fn get(&self, id: &str) -> Result<Option<OperationRecord>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                r#"
                SELECT id, agent_key, kind, status, step, error_code, summary,
                       observed_status, observed_version, started_at, finished_at
                FROM operations WHERE id = ?1
                "#,
                params![id],
                map_row,
            )
            .optional()
            .map_err(AppError::from)
        })
    }

    pub fn list_for_agent(&self, agent_key: &str, limit: u32) -> Result<Vec<OperationRecord>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, agent_key, kind, status, step, error_code, summary,
                       observed_status, observed_version, started_at, finished_at
                FROM operations
                WHERE agent_key = ?1
                ORDER BY started_at DESC, id DESC
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![agent_key, limit as i64], map_row)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    pub fn list_running(&self) -> Result<Vec<OperationRecord>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, agent_key, kind, status, step, error_code, summary,
                       observed_status, observed_version, started_at, finished_at
                FROM operations
                WHERE status = ?1
                ORDER BY started_at ASC
                "#,
            )?;
            let rows = stmt.query_map(params![OperationStatus::Running.as_str()], map_row)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }
}

fn map_row(row: &Row<'_>) -> rusqlite::Result<OperationRecord> {
    let kind_s: String = row.get(2)?;
    let status_s: String = row.get(3)?;
    Ok(OperationRecord {
        id: row.get(0)?,
        agent_key: row.get(1)?,
        kind: OperationKind::parse(&kind_s).unwrap_or(OperationKind::Install),
        status: OperationStatus::parse(&status_s).unwrap_or(OperationStatus::Failed),
        step: row.get(4)?,
        error_code: row.get(5)?,
        summary: row.get(6)?,
        observed_status: row.get(7)?,
        observed_version: row.get(8)?,
        started_at: row.get(9)?,
        finished_at: row.get(10)?,
    })
}


