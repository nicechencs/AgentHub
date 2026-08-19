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
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample(id: &str, agent: AgentId, kind: BackupKind, created_at: &str) -> BackupRecord {
        BackupRecord {
            id: id.into(),
            agent_id: Some(agent),
            kind,
            path: format!("live/{}/{id}", agent.as_str()),
            files: vec!["settings.json".into()],
            size: 10,
            note: None,
            created_at: created_at.into(),
        }
    }

    #[test]
    fn insert_list_newest_first_and_filter() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let repo = BackupRepo::new(db);

        repo.insert(&sample(
            "a",
            AgentId::Claude,
            BackupKind::Manual,
            "2026-01-01T10:00:00Z",
        ))
        .unwrap();
        repo.insert(&sample(
            "b",
            AgentId::Claude,
            BackupKind::AutoSwitch,
            "2026-01-03T10:00:00Z",
        ))
        .unwrap();
        repo.insert(&sample(
            "c",
            AgentId::Codex,
            BackupKind::PreUninstall,
            "2026-01-02T10:00:00Z",
        ))
        .unwrap();
        // Same timestamp — secondary order by id DESC.
        repo.insert(&sample(
            "z",
            AgentId::Claude,
            BackupKind::PreRestore,
            "2026-01-03T10:00:00Z",
        ))
        .unwrap();

        let all = repo.list(None).unwrap();
        assert_eq!(
            all.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["z", "b", "c", "a"]
        );

        let claude = repo.list(Some(AgentId::Claude)).unwrap();
        assert_eq!(
            claude.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["z", "b", "a"]
        );
        assert!(claude.iter().all(|r| r.agent_id == Some(AgentId::Claude)));

        let got = repo.get_by_id("c").unwrap().expect("found");
        assert_eq!(got.kind, BackupKind::PreUninstall);
        assert_eq!(got.agent_id, Some(AgentId::Codex));
        assert!(repo.get_by_id("missing").unwrap().is_none());

        assert!(repo.delete("c").unwrap());
        assert!(repo.get_by_id("c").unwrap().is_none());
        assert!(!repo.delete("c").unwrap());
        assert!(!repo.delete("missing").unwrap());
    }

    #[test]
    fn touch_created_at_updates_only_timestamp() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("t.db")).unwrap();
        let repo = BackupRepo::new(db);
        let rec = sample(
            "a",
            AgentId::Claude,
            BackupKind::Manual,
            "2026-01-01T10:00:00Z",
        );
        repo.insert(&rec).unwrap();

        let updated = repo.touch_created_at("a", "2026-02-01T10:00:00Z").unwrap();
        assert_eq!(updated.id, "a");
        assert_eq!(updated.created_at, "2026-02-01T10:00:00Z");
        assert_eq!(updated.kind, BackupKind::Manual);
        assert_eq!(updated.size, 10);
        assert_eq!(updated.files, rec.files);
        assert_eq!(updated.path, rec.path);
        assert_eq!(updated.note, rec.note);
        assert_eq!(updated.agent_id, rec.agent_id);

        let err = repo
            .touch_created_at("missing", "2026-03-01T00:00:00Z")
            .unwrap_err();
        assert_eq!(err.code(), "not_found");
    }
}
