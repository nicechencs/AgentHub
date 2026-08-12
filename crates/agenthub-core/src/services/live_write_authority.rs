//! Shared authority for per-agent writes to live agent configuration.
//!
//! Provider switching, direct configuration writes, restores, and destructive
//! purges must all serialize on this authority.  Its historical lock filename
//! remains `provider-{agent}.lock` so older processes and current integrations
//! continue to mutually exclude on the same path.

use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::platform::AgentKey;
use crate::storage::Database;
use crate::utils::agent_lock::AgentWriteLock;

/// Database-rooted per-agent authority for mutations of live agent files.
#[derive(Debug, Clone)]
pub struct LiveWriteAuthority {
    lock_dir: PathBuf,
}

/// RAII proof that one agent's live-write authority is held.
///
/// This type has no public constructor: callers can only obtain it from a
/// [`LiveWriteAuthority`] and can pass it to `*_with_guard` APIs to avoid
/// re-entering the same cross-process lock during a larger saga.
#[derive(Debug)]
pub struct LiveWriteGuard {
    lock_dir: PathBuf,
    agent_key: AgentKey,
    _lock: AgentWriteLock,
}

impl LiveWriteAuthority {
    /// Derive the stable shared lock root for all services built over `db`.
    pub fn from_database(db: &Database) -> Self {
        Self {
            lock_dir: database_lock_dir(db),
        }
    }

    /// Acquire the authority for a built-in agent.
    pub fn acquire(&self, agent: AgentId) -> Result<LiveWriteGuard> {
        self.acquire_key(&AgentKey::from_agent_id(agent))
    }

    /// Acquire the authority for a key-native platform agent.
    pub fn acquire_key(&self, agent_key: &AgentKey) -> Result<LiveWriteGuard> {
        let lock = AgentWriteLock::acquire_key(&self.lock_dir, agent_key).map_err(remap_lock)?;
        Ok(LiveWriteGuard {
            lock_dir: self.lock_dir.clone(),
            agent_key: agent_key.clone(),
            _lock: lock,
        })
    }

    /// Reject a guard from another database authority or another agent.
    pub fn validate_guard(&self, guard: &LiveWriteGuard, agent: AgentId) -> Result<()> {
        self.validate_guard_key(guard, &AgentKey::from_agent_id(agent))
    }

    /// Key-native counterpart to [`Self::validate_guard`].
    pub fn validate_guard_key(&self, guard: &LiveWriteGuard, agent_key: &AgentKey) -> Result<()> {
        if guard.lock_dir != self.lock_dir || guard.agent_key != *agent_key {
            return Err(AppError::InvalidArg(
                "live-write guard does not match this authority and agent".into(),
            ));
        }
        Ok(())
    }

    /// Exposed for diagnostics and narrowly-scoped tests; callers must not
    /// manufacture lock paths from a different data root.
    pub fn lock_dir(&self) -> &Path {
        &self.lock_dir
    }

    /// Parent data root shared by the database, `backups/`, and `locks/`.
    pub fn data_root(&self) -> &Path {
        self.lock_dir.parent().unwrap_or(&self.lock_dir)
    }
}

impl LiveWriteGuard {
    pub fn agent_key(&self) -> &AgentKey {
        &self.agent_key
    }
}

fn remap_lock(error: AppError) -> AppError {
    if error.code() == "agent.lock" {
        AppError::message(
            "provider.lock",
            "another live write is already running for this agent",
        )
    } else {
        error
    }
}

/// Every service constructed from one database uses its parent `locks/`
/// directory. This intentionally does not depend on a service-local backup
/// root, so CLI, desktop, and direct Core composition share one authority.
fn database_lock_dir(db: &Database) -> PathBuf {
    let database_dir = db
        .with_conn(|conn| {
            conn.query_row(
                "SELECT file FROM pragma_database_list WHERE name = 'main'",
                [],
                |row| row.get::<_, String>(0),
            )
            .map_err(Into::into)
        })
        .ok()
        .and_then(|path| PathBuf::from(path).parent().map(Path::to_path_buf));
    database_dir
        .unwrap_or_else(std::env::temp_dir)
        .join("locks")
}
