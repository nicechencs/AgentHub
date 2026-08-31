//! Persistence for managed-write fingerprints (table `live_write_fingerprints`).
//!
//! One row per (agent, live config path) holding the SHA-256 of the bytes
//! AgentHub last wrote to that file. Consumers live in
//! `services::live_fingerprint`, which decides from this record whether a
//! live file is still byte-identical to AgentHub's last write.

use rusqlite::{params, OptionalExtension};

use crate::error::Result;
use crate::storage::Database;

pub(crate) struct LiveFingerprintRepo {
    db: Database,
}

impl LiveFingerprintRepo {
    pub(crate) fn new(db: Database) -> Self {
        Self { db }
    }

    /// Insert or refresh the fingerprint for one live path.
    pub(crate) fn upsert(
        &self,
        agent_id: &str,
        path: &str,
        sha256: &str,
        written_at: &str,
    ) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                r#"
                INSERT INTO live_write_fingerprints (agent_id, path, sha256, written_at)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(agent_id, path) DO UPDATE SET
                    sha256 = excluded.sha256,
                    written_at = excluded.written_at
                "#,
                params![agent_id, path, sha256, written_at],
            )?;
            Ok(())
        })
    }

    /// SHA-256 AgentHub last wrote to this live path, if any.
    pub(crate) fn get(&self, agent_id: &str, path: &str) -> Result<Option<String>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT sha256 FROM live_write_fingerprints WHERE agent_id = ?1 AND path = ?2",
                params![agent_id, path],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
        })
    }

    /// Drop every fingerprint row for one agent. Returns the removed count.
    // Part of the repo API for unbind cleanup; no production caller yet.
    #[allow(dead_code)]
    pub(crate) fn delete_for_agent(&self, agent_id: &str) -> Result<u64> {
        self.db.with_conn(|conn| {
            let n = conn.execute(
                "DELETE FROM live_write_fingerprints WHERE agent_id = ?1",
                params![agent_id],
            )?;
            Ok(n as u64)
        })
    }
}

#[cfg(test)]
mod tests;
