//! Skill packages + assignments repository (P12).

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{AppError, Result};
use crate::platform::AgentKey;
use crate::storage::Database;

/// Shared skill package row (`skill_packages.id` == skill id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillPackageRow {
    pub id: String,
    pub source_kind: String,
    pub locator: String,
    pub revision: String,
    pub manifest_json: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Per-agent desired/observed assignment (`skill_package_id`, `agent_key`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillAssignmentRow {
    pub skill_package_id: String,
    pub agent_key: String,
    pub desired_enabled: bool,
    pub projection_mode: String,
    pub applied_revision: Option<String>,
    pub observed_status: String,
    pub last_error: Option<String>,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct SkillRepo {
    db: Database,
}

impl SkillRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn db(&self) -> &Database {
        &self.db
    }

    // ---- packages ----

    pub fn get_package(&self, id: &str) -> Result<Option<SkillPackageRow>> {
        self.db.with_conn(|conn| get_package_conn(conn, id))
    }

    pub fn list_packages(&self) -> Result<Vec<SkillPackageRow>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, source_kind, locator, revision, manifest_json, created_at, updated_at
                FROM skill_packages
                ORDER BY id
                "#,
            )?;
            let rows = stmt.query_map([], map_package_row)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    pub fn upsert_package(&self, row: &SkillPackageRow) -> Result<SkillPackageRow> {
        self.db.with_conn(|conn| {
            upsert_package_conn(conn, row)?;
            get_package_conn(conn, &row.id)?.ok_or_else(|| {
                AppError::message("db.skill_package", "package missing after upsert")
            })
        })
    }

    pub fn delete_package(&self, id: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            let n = conn.execute("DELETE FROM skill_packages WHERE id = ?1", params![id])?;
            if n == 0 {
                return Err(AppError::NotFound(format!("skill package not found: {id}")));
            }
            Ok(())
        })
    }

    // ---- assignments ----

    pub fn get_assignment(
        &self,
        skill_package_id: &str,
        agent_key: &str,
    ) -> Result<Option<SkillAssignmentRow>> {
        self.db
            .with_conn(|conn| get_assignment_conn(conn, skill_package_id, agent_key))
    }

    pub fn list_assignments_for_skill(
        &self,
        skill_package_id: &str,
    ) -> Result<Vec<SkillAssignmentRow>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT skill_package_id, agent_key, desired_enabled, projection_mode,
                       applied_revision, observed_status, last_error, updated_at
                FROM skill_assignments
                WHERE skill_package_id = ?1
                ORDER BY agent_key
                "#,
            )?;
            let rows = stmt.query_map(params![skill_package_id], map_assignment_row)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    pub fn list_assignments_for_agent(&self, agent_key: &str) -> Result<Vec<SkillAssignmentRow>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r#"
                SELECT skill_package_id, agent_key, desired_enabled, projection_mode,
                       applied_revision, observed_status, last_error, updated_at
                FROM skill_assignments
                WHERE agent_key = ?1
                ORDER BY skill_package_id
                "#,
            )?;
            let rows = stmt.query_map(params![agent_key], map_assignment_row)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    pub fn upsert_assignment(&self, row: &SkillAssignmentRow) -> Result<SkillAssignmentRow> {
        self.db.with_conn(|conn| {
            // Package must exist (FK).
            if get_package_conn(conn, &row.skill_package_id)?.is_none() {
                return Err(AppError::NotFound(format!(
                    "skill package not found: {}",
                    row.skill_package_id
                )));
            }
            upsert_assignment_conn(conn, row)?;
            get_assignment_conn(conn, &row.skill_package_id, &row.agent_key)?.ok_or_else(|| {
                AppError::message("db.skill_assignment", "assignment missing after upsert")
            })
        })
    }

    /// Update observed fields only (keeps desired_enabled / projection_mode).
    pub fn update_observed(
        &self,
        skill_package_id: &str,
        agent_key: &str,
        observed_status: &str,
        applied_revision: Option<&str>,
        last_error: Option<&str>,
        updated_at: &str,
    ) -> Result<SkillAssignmentRow> {
        self.db.with_conn(|conn| {
            let existing =
                get_assignment_conn(conn, skill_package_id, agent_key)?.ok_or_else(|| {
                    AppError::NotFound(format!(
                        "skill assignment not found: {skill_package_id}/{agent_key}"
                    ))
                })?;
            let row = SkillAssignmentRow {
                skill_package_id: existing.skill_package_id,
                agent_key: existing.agent_key,
                desired_enabled: existing.desired_enabled,
                projection_mode: existing.projection_mode,
                applied_revision: applied_revision.map(|s| s.to_string()),
                observed_status: observed_status.to_string(),
                last_error: last_error.map(|s| s.to_string()),
                updated_at: updated_at.to_string(),
            };
            upsert_assignment_conn(conn, &row)?;
            get_assignment_conn(conn, skill_package_id, agent_key)?.ok_or_else(|| {
                AppError::message(
                    "db.skill_assignment",
                    "assignment missing after update_observed",
                )
            })
        })
    }

    pub fn delete_assignment(&self, skill_package_id: &str, agent_key: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            let n = conn.execute(
                "DELETE FROM skill_assignments WHERE skill_package_id = ?1 AND agent_key = ?2",
                params![skill_package_id, agent_key],
            )?;
            if n == 0 {
                return Err(AppError::NotFound(format!(
                    "skill assignment not found: {skill_package_id}/{agent_key}"
                )));
            }
            Ok(())
        })
    }
}

fn map_package_row(row: &Row<'_>) -> rusqlite::Result<SkillPackageRow> {
    Ok(SkillPackageRow {
        id: row.get(0)?,
        source_kind: row.get(1)?,
        locator: row.get(2)?,
        revision: row.get(3)?,
        manifest_json: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn map_assignment_row(row: &Row<'_>) -> rusqlite::Result<SkillAssignmentRow> {
    let desired: i64 = row.get(2)?;
    Ok(SkillAssignmentRow {
        skill_package_id: row.get(0)?,
        agent_key: row.get(1)?,
        desired_enabled: desired != 0,
        projection_mode: row.get(3)?,
        applied_revision: row.get(4)?,
        observed_status: row.get(5)?,
        last_error: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn get_package_conn(conn: &Connection, id: &str) -> Result<Option<SkillPackageRow>> {
    conn.query_row(
        r#"
        SELECT id, source_kind, locator, revision, manifest_json, created_at, updated_at
        FROM skill_packages
        WHERE id = ?1
        "#,
        params![id],
        map_package_row,
    )
    .optional()
    .map_err(Into::into)
}

fn upsert_package_conn(conn: &Connection, row: &SkillPackageRow) -> Result<()> {
    if row.id.trim().is_empty() {
        return Err(AppError::InvalidArg(
            "skill package id must not be empty".into(),
        ));
    }
    conn.execute(
        r#"
        INSERT INTO skill_packages (
            id, source_kind, locator, revision, manifest_json, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(id) DO UPDATE SET
            source_kind = excluded.source_kind,
            locator = excluded.locator,
            revision = excluded.revision,
            manifest_json = excluded.manifest_json,
            updated_at = excluded.updated_at
        "#,
        params![
            row.id,
            row.source_kind,
            row.locator,
            row.revision,
            row.manifest_json,
            row.created_at,
            row.updated_at,
        ],
    )?;
    Ok(())
}

fn get_assignment_conn(
    conn: &Connection,
    skill_package_id: &str,
    agent_key: &str,
) -> Result<Option<SkillAssignmentRow>> {
    conn.query_row(
        r#"
        SELECT skill_package_id, agent_key, desired_enabled, projection_mode,
               applied_revision, observed_status, last_error, updated_at
        FROM skill_assignments
        WHERE skill_package_id = ?1 AND agent_key = ?2
        "#,
        params![skill_package_id, agent_key],
        map_assignment_row,
    )
    .optional()
    .map_err(Into::into)
}

fn upsert_assignment_conn(conn: &Connection, row: &SkillAssignmentRow) -> Result<()> {
    let _ = AgentKey::parse(row.agent_key.as_str())?;
    if row.skill_package_id.trim().is_empty() {
        return Err(AppError::InvalidArg(
            "skill_package_id must not be empty".into(),
        ));
    }
    conn.execute(
        r#"
        INSERT INTO skill_assignments (
            skill_package_id, agent_key, desired_enabled, projection_mode,
            applied_revision, observed_status, last_error, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(skill_package_id, agent_key) DO UPDATE SET
            desired_enabled = excluded.desired_enabled,
            projection_mode = excluded.projection_mode,
            applied_revision = excluded.applied_revision,
            observed_status = excluded.observed_status,
            last_error = excluded.last_error,
            updated_at = excluded.updated_at
        "#,
        params![
            row.skill_package_id,
            row.agent_key,
            if row.desired_enabled { 1i64 } else { 0i64 },
            row.projection_mode,
            row.applied_revision,
            row.observed_status,
            row.last_error,
            row.updated_at,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
