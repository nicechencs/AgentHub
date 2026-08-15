//! Per-agent exclusive live-write lock shared by provider and account switches.
//!
//! Lock file format (line-oriented):
//! ```text
//! pid=<os pid>
//! created_unix_ms=<epoch millis>
//! token=<uuid>
//! ```

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::models::AgentId;
use crate::platform::AgentKey;

/// Conservative upper bound for a live provider/account switch.
/// Locks older than this are treated as abandoned even if the PID still
/// appears alive (PID reuse / hung process safety net).
const LOCK_TTL: Duration = Duration::from_secs(30 * 60);

/// How many create/reclaim attempts after observing an existing lock file.
const LOCK_ACQUIRE_ATTEMPTS: usize = 3;

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
    match LockOwner::parse(&raw) {
        Some(owner) => {
            let stale = owner.is_stale();
            LockInspection {
                agent: agent.to_string(),
                path: display,
                status: if stale { "stale" } else { "held" }.into(),
                pid: Some(owner.pid),
                created_unix_ms: Some(owner.created_unix_ms),
                note: stale.then(|| "owner process gone or lock older than TTL".into()),
            }
        }
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

/// Per-agent exclusive live-write lock with owner metadata and stale recovery.
#[derive(Debug)]
pub struct AgentWriteLock {
    path: PathBuf,
    file: Option<std::fs::File>,
    /// Identity of this holder; Drop only unlinks when the file still carries it.
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

    /// Parse owner metadata. Unknown keys are ignored for forward compatibility;
    /// missing/invalid required fields fail closed (not reclaimable).
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

    fn is_stale(&self) -> bool {
        if lock_age_ms(self.created_unix_ms) >= LOCK_TTL.as_millis() as u64 {
            return true;
        }
        !process_is_alive(self.pid)
    }

    fn same_identity(&self, other: &Self) -> bool {
        self.pid == other.pid
            && self.created_unix_ms == other.created_unix_ms
            && self.token == other.token
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

        for _ in 0..LOCK_ACQUIRE_ATTEMPTS {
            match Self::try_create(&path) {
                Ok(lock) => {
                    logging::log_debug(
                        targets::LOCK,
                        "acquire",
                        &format!("acquired lock path={}", path.display()),
                    );
                    return Ok(lock);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    match try_reclaim_stale_lock(&path) {
                        Ok(true) => {
                            logging::log_warn(
                                targets::LOCK,
                                "acquire",
                                &format!("reclaimed stale lock path={}", path.display()),
                            );
                        }
                        Ok(false) => {
                            let err = lock_held_error(agent_key.as_str());
                            logging::log_app_error_agent(
                                targets::LOCK,
                                "acquire",
                                agent_key.as_str(),
                                &err,
                            );
                            return Err(err);
                        }
                        Err(err) => {
                            logging::log_app_error_agent(
                                targets::LOCK,
                                "acquire",
                                agent_key.as_str(),
                                &err,
                            );
                            return Err(err);
                        }
                    }
                }
                Err(error) => {
                    let err: AppError = error.into();
                    logging::log_app_error_agent(
                        targets::LOCK,
                        "acquire",
                        agent_key.as_str(),
                        &err,
                    );
                    return Err(err);
                }
            }
        }

        let err = lock_held_error(agent_key.as_str());
        logging::log_app_error_agent(targets::LOCK, "acquire", agent_key.as_str(), &err);
        Err(err)
    }

    fn try_create(path: &Path) -> std::io::Result<Self> {
        let owner = LockOwner::current();
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        if let Err(error) = file.write_all(owner.serialize().as_bytes()) {
            drop(file);
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
        let _ = file.sync_all();
        Ok(Self {
            path: path.to_path_buf(),
            file: Some(file),
            token: owner.token,
        })
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
    }
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

fn try_reclaim_stale_lock(path: &Path) -> Result<bool> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.into()),
    };

    let owner = match LockOwner::parse(&raw) {
        Some(owner) => owner,
        None => return Ok(false),
    };

    if !owner.is_stale() {
        return Ok(false);
    }

    let raw_again = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.into()),
    };
    if raw_again != raw {
        return Ok(false);
    }
    let owner_again = match LockOwner::parse(&raw_again) {
        Some(owner) => owner,
        None => return Ok(false),
    };
    if !owner.same_identity(&owner_again) {
        return Ok(false);
    }

    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(_) => Ok(false),
    }
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn lock_age_ms(created_unix_ms: u64) -> u64 {
    unix_now_ms().saturating_sub(created_unix_ms)
}

fn process_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }

    #[cfg(windows)]
    {
        windows_process_is_alive(pid)
    }

    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    {
        use std::process::{Command, Stdio};
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(true)
    }

    #[cfg(not(any(windows, unix)))]
    {
        let _ = pid;
        true
    }
}

#[cfg(windows)]
fn windows_process_is_alive(pid: u32) -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(
            desired_access: u32,
            inherit_handle: i32,
            process_id: u32,
        ) -> *mut core::ffi::c_void;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
        fn GetExitCodeProcess(handle: *mut core::ffi::c_void, exit_code: *mut u32) -> i32;
        fn GetLastError() -> u32;
    }

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    const ERROR_ACCESS_DENIED: u32 = 5;

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return GetLastError() == ERROR_ACCESS_DENIED;
        }
        let mut exit_code = 0u32;
        let ok = GetExitCodeProcess(handle, &mut exit_code);
        CloseHandle(handle);
        ok != 0 && exit_code == STILL_ACTIVE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn acquire_and_drop_releases_lock() {
        let dir = tempdir().unwrap();
        {
            let _lock = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
            let err = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap_err();
            assert_eq!(err.code(), "agent.lock");
        }
        let _again = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
    }

    #[test]
    fn different_agents_do_not_block_each_other() {
        let dir = tempdir().unwrap();
        let _a = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
        let _b = AgentWriteLock::acquire(dir.path(), AgentId::Codex).unwrap();
    }

    #[test]
    fn malformed_lock_is_fail_closed() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("provider-grok.lock"),
            b"not a valid owner record",
        )
        .unwrap();
        let err = AgentWriteLock::acquire(dir.path(), AgentId::Grok).unwrap_err();
        assert_eq!(err.code(), "agent.lock");
    }

    #[test]
    fn inspect_locks_reports_held_and_malformed() {
        let dir = tempdir().unwrap();
        let _held = AgentWriteLock::acquire(dir.path(), AgentId::Claude).unwrap();
        std::fs::write(dir.path().join("provider-grok.lock"), b"not-a-lock").unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"ignore").unwrap();

        let rows = inspect_locks(dir.path());
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].agent, "claude");
        assert_eq!(rows[0].status, "held");
        assert_eq!(rows[0].pid, Some(std::process::id()));
        assert_eq!(rows[1].agent, "grok");
        assert_eq!(rows[1].status, "malformed");
    }
}
