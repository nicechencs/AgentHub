//! Live backup snapshot / restore / delete service.
//!
//! Incremental live snapshots:
//! - Same agent + same live source set + same SHA-256 content reuses the
//!   newest matching historical snapshot (`created_at` bumped; no new
//!   directory or DB row).
//! - A partial change creates a new `backups_root/live/<agent>/<id>/`
//!   snapshot. Unchanged file bytes are hardlinked from a prior snapshot
//!   when the hash is already stored; `std::fs::copy` is the fallback
//!   (never symlinks).
//!
//! Restore destinations are always derived from the registered adapter's
//! `live_backup_paths` — never from untrusted absolute paths alone.
//! Delete removes the snapshot directory (when contained) and the index row.
//!
//! Split for maintainability only — public path stays
//! [`crate::services::BackupService`].

mod catalog;
mod inspect;
mod path_safety;
mod restore;
mod snapshot;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;

use crate::adapters::AdapterRegistry;
use crate::error::Result;
use crate::models::AgentId;
use crate::services::{LiveWriteAuthority, LiveWriteGuard};
use crate::storage::{BackupRepo, Database};

// Re-export helpers so `tests` (`use super::*`) keep seeing them.
#[allow(unused_imports)]
use crate::error::AppError;
#[allow(unused_imports)]
use crate::models::{BackupKind, BackupRecord};
#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};

#[cfg(test)]
pub(super) use path_safety::{
    allocate_dest_name, ensure_existing_path_strictly_inside, is_path_inside, sanitize_basename,
};
#[cfg(test)]
pub(super) use restore::{apply_restore_plan, RestoreItem};
#[cfg(test)]
pub(super) use snapshot::{
    write_manifest, BackupManifest, ManifestEntry, MANIFEST_FILE, MANIFEST_VERSION,
};

pub(super) fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Result of a successful restore.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    /// The backup that was applied.
    pub restored: BackupRecord,
    /// Automatic PreRestore snapshot of live files before overwrite, if any
    /// backupable live files existed.
    pub pre_restore: Option<BackupRecord>,
    /// Live destinations that received a restored file.
    pub restored_paths: Vec<PathBuf>,
}

/// Orchestrates live file snapshots + backup index rows + restore/delete.
#[derive(Clone)]
pub struct BackupService {
    pub(super) repo: BackupRepo,
    pub(super) registry: AdapterRegistry,
    pub(super) backups_root: PathBuf,
    pub(super) authority: LiveWriteAuthority,
}

impl BackupService {
    /// Explicit dependencies — no implicit home/data-dir resolution.
    pub fn new(db: Database, registry: AdapterRegistry, backups_root: PathBuf) -> Self {
        Self {
            repo: BackupRepo::new(db.clone()),
            registry,
            backups_root,
            authority: LiveWriteAuthority::from_database(&db),
        }
    }

    pub fn backups_root(&self) -> &Path {
        &self.backups_root
    }

    /// Take the shared live-write lock for one Agent. Nested live sagas must
    /// reuse this guard via [`Self::snapshot_with_guard`].
    pub fn acquire_live_write(&self, agent: AgentId) -> Result<LiveWriteGuard> {
        self.authority.acquire(agent)
    }

    pub(crate) fn keep_live_file_copies(&self) -> bool {
        self.repo.keep_live_file_copies()
    }

    #[cfg(test)]
    pub(crate) fn repo(&self) -> &BackupRepo {
        &self.repo
    }
}
