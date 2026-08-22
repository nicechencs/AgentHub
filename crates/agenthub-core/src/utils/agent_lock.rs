//! Per-agent exclusive live-write lock shared by provider and account switches.
//!
//! AgentHub is a single-process app. Mutual exclusion is process-local; the
//! lock file is owner metadata for diagnostics and is never a cross-process
//! protocol. Crash leftovers and malformed metadata are reclaimed on acquire.

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::models::AgentId;
use crate::platform::AgentKey;

pub(crate) fn try_claim_lock_path(path: &Path) -> bool {
    held_lock_paths().insert(lock_identity(path))
}

pub(crate) fn release_lock_path(path: &Path) {
    held_lock_paths().remove(&lock_identity(path));
}

fn lock_path_is_held(path: &Path) -> bool {
    held_lock_paths().contains(&lock_identity(path))
}

fn lock_identity(path: &Path) -> PathBuf {
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf())
            .join(name),
        _ => path.to_path_buf(),
    }
}

fn held_lock_paths() -> std::sync::MutexGuard<'static, HashSet<PathBuf>> {
    static HELD_LOCK_PATHS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    HELD_LOCK_PATHS
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Doctor / CLI view of one live-write lock file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockInspection {
    pub agent: String,
    pub path: String,
    /// `held` | `stale` | `malformed`
    pub status: String,
    pub pid: Option<u32>,
    pub created_unix_ms: Option<u64>,
    pub note: Option<String>,
}

/// Scan `{data_dir}/locks/provider-*.lock` without acquiring them.
pub fn inspect_locks(lock_dir: &Path) -> Vec<LockInspection> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(lock_dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(agent) = name
            .strip_prefix("provider-")
            .and_then(|rest| rest.strip_suffix(".lock"))
        else {
            continue;
        };
        out.push(inspect_one(&path, agent));
    }
    out.sort_by(|a, b| a.agent.cmp(&b.agent));
    out
}

fn inspect_one(path: &Path, agent: &str) -> LockInspection {
    let display = path.display().to_string();
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            return LockInspection {
                agent: agent.to_string(),
                path: display,
                status: "malformed".into(),
                pid: None,
                created_unix_ms: None,
                note: Some(error.to_string()),
            };
        }
    };

    if lock_path_is_held(path) {
        let owner = LockOwner::parse(&raw);
        return LockInspection {
            agent: agent.to_string(),
            path: display,
            status: "held".into(),
            pid: owner.as_ref().map(|owner| owner.pid),
            created_unix_ms: owner.as_ref().map(|owner| owner.created_unix_ms),
            note: None,
        };
    }

    match LockOwner::parse(&raw) {
        Some(owner) => LockInspection {
            agent: agent.to_string(),
            path: display,
            status: "stale".into(),
            pid: Some(owner.pid),
            created_unix_ms: Some(owner.created_unix_ms),
            note: Some("lock file is leftover and not held in this process".into()),
        },
        None => LockInspection {
            agent: agent.to_string(),
            path: display,
            status: "malformed".into(),
            pid: None,
            created_unix_ms: None,
            note: Some("lock file missing required pid/created_unix_ms/token".into()),
        },
    }
}

/// Per-agent exclusive live-write lock with owner metadata.
#[derive(Debug)]
pub struct AgentWriteLock {
    path: PathBuf,
    file: Option<std::fs::File>,
    token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LockOwner {
    pid: u32,
    created_unix_ms: u64,
    token: String,
}

impl LockOwner {
    fn current() -> Self {
        Self {
            pid: std::process::id(),
            created_unix_ms: unix_now_ms(),
            token: Uuid::new_v4().to_string(),
        }
    }

    fn serialize(&self) -> String {
        format!(
            "pid={}\ncreated_unix_ms={}\ntoken={}\n",
            self.pid, self.created_unix_ms, self.token
        )
    }

    /// Parse owner metadata. Unknown keys are ignored; missing required fields
    /// fail the parse (the leftover file is still reclaimable on acquire).
    fn parse(raw: &str) -> Option<Self> {
        let mut pid = None;
        let mut created_unix_ms = None;
        let mut token = None;

        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            let value = value.trim();
            match key {
                "pid" => {
                    pid = Some(value.parse::<u32>().ok()?);
                }
                "created_unix_ms" => {
                    created_unix_ms = Some(value.parse::<u64>().ok()?);
                }
                "token" => {
                    if value.is_empty() {
                        return None;
                    }
                    token = Some(value.to_string());
                }
                _ => {}
            }
        }

        Some(Self {
            pid: pid?,
            created_unix_ms: created_unix_ms?,
            token: token?,
        })
    }
}

impl AgentWriteLock {
    /// Acquire the shared per-agent live-write lock.
    ///
    /// Uses the historical `provider-{agent}.lock` filename so provider and
    /// account switches mutually exclude on the same path (no second lock set).
    pub fn acquire(lock_dir: &Path, agent: AgentId) -> Result<Self> {
        Self::acquire_key(lock_dir, &AgentKey::from_agent_id(agent))
    }

    /// Key-native form used by platform capabilities that are not backed by
    /// the closed built-in `AgentId` set.
    pub fn acquire_key(lock_dir: &Path, agent_key: &AgentKey) -> Result<Self> {
        if let Err(error) = std::fs::create_dir_all(lock_dir) {
            let err: AppError = error.into();
            logging::log_app_error_agent(targets::LOCK, "acquire", agent_key.as_str(), &err);
            return Err(err);
        }
        let path = lock_dir.join(format!("provider-{}.lock", agent_key.as_str()));

        match Self::try_create(&path) {
            Ok(Some(lock)) => {
                logging::log_debug(
                    targets::LOCK,
                    "acquire",
                    &format!("acquired lock path={}", path.display()),
                );
                Ok(lock)
            }
            Ok(None) => {
                let err = lock_held_error(agent_key.as_str());
                logging::log_app_error_agent(targets::LOCK, "acquire", agent_key.as_str(), &err);
                Err(err)
            }
            Err(error) => {
                let err: AppError = error.into();
                logging::log_app_error_agent(targets::LOCK, "acquire", agent_key.as_str(), &err);
                Err(err)
            }
        }
    }

    fn try_create(path: &Path) -> io::Result<Option<Self>> {
        if !try_claim_lock_path(path) {
            return Ok(None);
        }
        let owner = LockOwner::current();
        match write_owner_file(path, &owner.serialize()) {
            Ok(file) => Ok(Some(Self {
                path: path.to_path_buf(),
                file: Some(file),
                token: owner.token,
            })),
            Err(error) => {
                release_lock_path(path);
                let _ = std::fs::remove_file(path);
                Err(error)
            }
        }
    }
}

impl Drop for AgentWriteLock {
    fn drop(&mut self) {
        drop(self.file.take());
        if let Ok(raw) = std::fs::read_to_string(&self.path) {
            if LockOwner::parse(&raw).is_some_and(|owner| owner.token == self.token) {
                let _ = std::fs::remove_file(&self.path);
            }
        }
        release_lock_path(&self.path);
    }
}

fn write_owner_file(path: &Path, metadata: &str) -> io::Result<std::fs::File> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(metadata.as_bytes())?;
    let _ = file.sync_all();
    Ok(file)
}

fn lock_held_error(agent_key: &str) -> AppError {
    AppError::message(
        "agent.lock",
        format!(
            "another live write is already running for agent {}",
            agent_key
        ),
    )
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
