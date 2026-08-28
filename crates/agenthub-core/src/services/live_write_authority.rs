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
/// re-entering the same OS exclusive lock during a larger saga.
#[derive(Debug)]
pub struct LiveWriteGuard {
    lock_dir: PathBuf,
    agent_key: AgentKey,
    _lock: AgentWriteLock,
}

impl LiveWriteAuthority {
    /// Derive the stable shared lock root for all services built over `db`.
    ///
    /// File-backed databases must resolve a durable parent directory so every
    /// process sharing the same DB also shares the same lock path. In-memory
    /// databases are allowed a process-local temp root. Unresolvable paths
    /// fail closed rather than inventing a shared temp lock directory.
    pub fn from_database(db: &Database) -> Self {
        Self::try_from_database(db).unwrap_or_else(|error| {
            panic!("LiveWriteAuthority::from_database: {error}");
        })
    }

    /// Fallible counterpart to [`Self::from_database`].
    pub fn try_from_database(db: &Database) -> Result<Self> {
        Ok(Self {
            lock_dir: database_lock_dir(db)?,
        })
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
fn database_lock_dir(db: &Database) -> Result<PathBuf> {
    let path = main_database_file(db)?;
    if is_memory_database_path(&path) {
        // In-memory databases have no durable data root; keep locks process-local.
        return Ok(std::env::temp_dir().join("agenthub-memory-db-locks"));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok(parent.join("locks"))
}

fn main_database_file(db: &Database) -> Result<PathBuf> {
    let path = db.with_conn(|conn| {
        conn.query_row(
            "SELECT file FROM pragma_database_list WHERE name = 'main'",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(Into::into)
    })?;
    if path.trim().is_empty() {
        return Err(AppError::message(
            "provider.lock_dir",
            "main database path is empty; cannot derive live-write lock directory",
        ));
    }
    Ok(PathBuf::from(path))
}

fn is_memory_database_path(path: &Path) -> bool {
    let raw = path.to_string_lossy();
    raw.is_empty() || raw == ":memory:" || raw.starts_with("file:memdb")
}

#[cfg(test)]
mod tests;
