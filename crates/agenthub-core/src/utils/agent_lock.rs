//! Per-agent exclusive live-write lock shared by provider and account switches.
//!
//! Mutual exclusion is an OS exclusive lock on the lock leaf, held on the open
//! file handle until [`AgentWriteLock`] drops. CLI and desktop processes that
//! share a data directory therefore cannot write the same Agent live files at
//! once. The lock-file body is owner metadata for diagnostics; crash leftovers
//! are reclaimable because the OS releases the handle lock.

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
    if lock_path_is_held(path) || os_lock_is_busy(path) {
        let owner = std::fs::read_to_string(path)
            .ok()
            .as_deref()
            .and_then(LockOwner::parse);
        return LockInspection {
            agent: agent.to_string(),
            path: display,
            status: "held".into(),
            pid: owner.as_ref().map(|owner| owner.pid),
            created_unix_ms: owner.as_ref().map(|owner| owner.created_unix_ms),
            note: None,
        };
    }

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
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                release_lock_path(path);
                Ok(None)
            }
            Err(error) => {
                release_lock_path(path);
                // Deliberately do NOT remove the lock leaf here. The path may
                // be a foreign symlink or another non-regular file that
                // open_lock_leaf refused to open; deleting it would touch a
                // target we do not own, breaking the fail-closed promise.
                Err(error)
            }
        }
    }
}

impl Drop for AgentWriteLock {
    fn drop(&mut self) {
        // Unlink while the exclusive OS lock is still held. Dropping the
        // handle first opens a TOCTOU window where another process can create
        // a new lock leaf on the same path and both appear to hold the lock.
        if let Some(file) = self.file.take() {
            if let Ok(raw) = std::fs::read_to_string(&self.path) {
                if LockOwner::parse(&raw).is_some_and(|owner| owner.token == self.token) {
                    let _ = std::fs::remove_file(&self.path);
                }
            }
            drop(file);
        }
        release_lock_path(&self.path);
    }
}

fn write_owner_file(path: &Path, metadata: &str) -> io::Result<std::fs::File> {
    let mut file = open_lock_leaf(path)?;
    if !try_lock_exclusive(&file)? {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "live-write lock is held by another process",
        ));
    }
    file.set_len(0)?;
    file.write_all(metadata.as_bytes())?;
    let _ = file.sync_all();
    Ok(file)
}

/// Non-blocking exclusive lock on an already-opened lock leaf.
pub(crate) fn try_lock_exclusive(file: &std::fs::File) -> io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;

        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc == 0 {
            return Ok(true);
        }
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(code) if code == libc::EWOULDBLOCK || code == libc::EAGAIN => Ok(false),
            _ => Err(err),
        }
    }

    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;

        const LOCKFILE_FAIL_IMMEDIATELY: u32 = 0x0000_0001;
        const LOCKFILE_EXCLUSIVE_LOCK: u32 = 0x0000_0002;
        const ERROR_LOCK_VIOLATION: u32 = 33;

        #[repr(C)]
        struct Overlapped {
            internal: usize,
            internal_high: usize,
            offset: u32,
            offset_high: u32,
            event: *mut core::ffi::c_void,
        }

        unsafe extern "system" {
            fn LockFileEx(
                file: *mut core::ffi::c_void,
                flags: u32,
                reserved: u32,
                bytes_low: u32,
                bytes_high: u32,
                overlapped: *mut Overlapped,
            ) -> i32;
            fn GetLastError() -> u32;
        }

        let mut overlapped = Overlapped {
            internal: 0,
            internal_high: 0,
            // Lock one byte past typical metadata so Windows exclusive
            // locking does not block reading or rewriting the owner body.
            offset: u32::MAX - 1,
            offset_high: 0,
            event: core::ptr::null_mut(),
        };
        let ok = unsafe {
            LockFileEx(
                file.as_raw_handle() as *mut core::ffi::c_void,
                LOCKFILE_FAIL_IMMEDIATELY | LOCKFILE_EXCLUSIVE_LOCK,
                0,
                1,
                0,
                &mut overlapped,
            )
        };
        if ok != 0 {
            return Ok(true);
        }
        let code = unsafe { GetLastError() };
        if code == ERROR_LOCK_VIOLATION {
            Ok(false)
        } else {
            Err(io::Error::from_raw_os_error(code as i32))
        }
    }
}

fn os_lock_is_busy(path: &Path) -> bool {
    let mut options = OpenOptions::new();
    options.write(true).create(false);
    let Ok(file) = options.open(path) else {
        return false;
    };
    match try_lock_exclusive(&file) {
        Ok(acquired) => !acquired,
        Err(_) => false,
    }
}

/// Open a lock leaf without following a pre-existing link, then validate the
/// opened handle itself is a regular file.
///
/// Opening with no-follow/reparse protection before validating the resulting
/// handle closes the check-then-open race against a symlink, junction, or
/// other reparse-point replacement of the lock path: such leaves fail closed
/// instead of truncating an unrelated target file.
pub(crate) fn open_lock_leaf(path: &Path) -> io::Result<std::fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(false);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.custom_flags(libc::O_NOFOLLOW);
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }

    let file = options.open(path)?;
    ensure_regular_lock_leaf(&file)?;
    Ok(file)
}

#[cfg(unix)]
fn ensure_regular_lock_leaf(file: &std::fs::File) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    // Validate the opened descriptor itself. This is deliberately fstat,
    // rather than a path-based metadata call, so a concurrent path swap
    // cannot turn the check into a symlink-following check-then-open race.
    let mut stat = unsafe { std::mem::zeroed::<libc::stat>() };
    if unsafe { libc::fstat(file.as_raw_fd(), &mut stat) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if stat.st_mode & libc::S_IFMT == libc::S_IFREG {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "lock leaf is not a regular file",
        ))
    }
}

#[cfg(windows)]
fn ensure_regular_lock_leaf(file: &std::fs::File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;

    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

    #[repr(C)]
    struct FileAttributeTagInfo {
        file_attributes: u32,
        reparse_tag: u32,
    }

    unsafe extern "system" {
        #[link_name = "GetFileInformationByHandleEx"]
        fn get_file_information_by_handle_ex(
            file: *mut core::ffi::c_void,
            information_class: u32,
            information: *mut core::ffi::c_void,
            information_size: u32,
        ) -> i32;
    }

    const FILE_ATTRIBUTE_TAG_INFO_CLASS: u32 = 9;
    let mut information = FileAttributeTagInfo {
        file_attributes: 0,
        reparse_tag: 0,
    };
    let ok = unsafe {
        get_file_information_by_handle_ex(
            file.as_raw_handle() as *mut core::ffi::c_void,
            FILE_ATTRIBUTE_TAG_INFO_CLASS,
            (&mut information as *mut FileAttributeTagInfo).cast(),
            std::mem::size_of::<FileAttributeTagInfo>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    if information.file_attributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
        || information.reparse_tag != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "lock leaf is a directory or reparse point",
        ));
    }
    Ok(())
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
