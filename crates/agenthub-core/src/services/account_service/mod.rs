//! Account pool service — CRUD, import-live, and safe live switching.
//!
//! Credentials use the existing storage scheme (no additional at-rest encryption).
//!
//! Split for maintainability only — public path stays
//! [`crate::services::AccountService`].

mod import_live;
mod live_reconcile;
mod pool_crud;
mod surface;
mod switch_saga;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::Arc;

use crate::adapters::{AdapterRegistry, AgentAdapter};
use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{AgentId, BackupKind};
use crate::services::{BackupService, ConnectionService};
use crate::storage::{AccountRepo, Database};
use crate::utils::agent_lock::AgentWriteLock;

pub const MAX_ACCOUNT_ID_LEN: usize = 128;
pub const MAX_ACCOUNT_LABEL_LEN: usize = 256;

/// Business facade over [`AccountRepo`].
pub struct AccountService {
    pub(super) db: Database,
    pub(super) repo: AccountRepo,
    pub(super) registry: AdapterRegistry,
    pub(super) backup: Option<BackupService>,
    pub(super) lock_dir: Option<PathBuf>,
    pub(super) connections: ConnectionService,
}

impl AccountService {
    pub fn new(db: Database) -> Self {
        Self::with_registry(db, AdapterRegistry::default())
    }

    pub fn with_registry(db: Database, registry: AdapterRegistry) -> Self {
        Self {
            db: db.clone(),
            repo: AccountRepo::new(db.clone()),
            registry,
            backup: None,
            lock_dir: None,
            connections: ConnectionService::new(db),
        }
    }

    /// Full live-switch service with shared backup root / lock directory.
    pub fn with_live(db: Database, registry: AdapterRegistry, backups_root: PathBuf) -> Self {
        let lock_dir = backups_root.parent().unwrap_or(&backups_root).join("locks");
        Self {
            db: db.clone(),
            repo: AccountRepo::new(db.clone()),
            backup: Some(BackupService::new(
                db.clone(),
                registry.clone(),
                backups_root,
            )),
            registry,
            lock_dir: Some(lock_dir),
            connections: ConnectionService::new(db),
        }
    }


    pub fn repo(&self) -> &AccountRepo {
        &self.repo
    }

    pub(super) fn snapshot_after_pool_change(&self, agent: AgentId, note: &str) {
        let Some(backup) = self.backup.as_ref() else {
            return;
        };
        if let Err(error) = backup.snapshot(agent, BackupKind::AutoSwitch, Some(note)) {
            if error.code() != "not_found" {
                tracing::warn!(
                    target: targets::BACKUP,
                    agent = agent.as_str(),
                    error = %error,
                    "automatic post-change live snapshot failed"
                );
            }
        }
    }

    pub(super) fn adapter(&self, agent: AgentId) -> Result<Arc<dyn AgentAdapter>> {
        self.registry.get(agent).ok_or_else(|| {
            AppError::NotFound(format!(
                "no adapter registered for agent {}",
                agent.as_str()
            ))
        })
    }

    pub(super) fn acquire_live_lock(&self, agent: AgentId) -> Result<Option<AgentWriteLock>> {
        self.lock_dir
            .as_deref()
            .map(|lock_dir| AgentWriteLock::acquire(lock_dir, agent))
            .transpose()
    }
}

