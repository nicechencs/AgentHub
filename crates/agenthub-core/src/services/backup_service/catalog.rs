//! Backup catalog / content index: list, fetch, identical-snapshot reuse.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{AppError, Result};
use crate::models::{AgentId, BackupRecord};

use super::snapshot::{index_stored_file, planned_matches_manifest, read_manifest, PlannedEntry};
use super::BackupService;

impl BackupService {
    /// List indexed backups newest-first; optional agent filter.
    pub fn list(&self, agent: Option<AgentId>) -> Result<Vec<BackupRecord>> {
        self.repo.list(agent)
    }

    /// Fetch a single backup by id.
    pub fn get_by_id(&self, id: &str) -> Result<BackupRecord> {
        self.repo
            .get_by_id(id)?
            .ok_or_else(|| AppError::NotFound(format!("backup not found: {id}")))
    }

    /// Newest-first scan: reuse a completed snapshot whose stored basenames,
    /// live sources, and content hashes match `planned`.
    pub(super) fn find_identical_snapshot(
        &self,
        agent: AgentId,
        planned: &[PlannedEntry],
        total_size: u64,
    ) -> Result<Option<BackupRecord>> {
        let records = self.repo.list(Some(agent))?;
        for rec in records {
            if rec.size != total_size || rec.files.len() != planned.len() {
                continue;
            }
            if !rec
                .files
                .iter()
                .map(String::as_str)
                .eq(planned.iter().map(|e| e.stored.as_str()))
            {
                continue;
            }
            let dir = match self.validate_snapshot_dir(&rec) {
                Ok(dir) => dir,
                Err(_) => continue,
            };
            let Ok(Some(manifest)) = read_manifest(&dir) else {
                continue;
            };
            if planned_matches_manifest(planned, &manifest, &dir) {
                return Ok(Some(rec));
            }
        }
        Ok(None)
    }

    /// Hash → stored regular file in a prior snapshot for this agent.
    /// Newest snapshot wins when the same hash appears more than once.
    pub(super) fn content_index_for_agent(
        &self,
        agent: AgentId,
    ) -> Result<HashMap<String, PathBuf>> {
        let mut index = HashMap::new();
        let records = self.repo.list(Some(agent))?;
        for rec in records {
            let dir = match self.validate_snapshot_dir(&rec) {
                Ok(dir) => dir,
                Err(_) => continue,
            };
            match read_manifest(&dir) {
                Ok(Some(manifest)) => {
                    for entry in manifest.entries {
                        index_stored_file(&mut index, &dir, &entry.stored, entry.sha256);
                    }
                }
                Ok(None) => {
                    for name in &rec.files {
                        index_stored_file(&mut index, &dir, name, None);
                    }
                }
                Err(_) => continue,
            }
        }
        Ok(index)
    }
}
