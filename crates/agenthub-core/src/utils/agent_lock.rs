//! Per-agent exclusive live-write lock shared by provider and account switches.
//!
//! The lock file is a persistent rendezvous/diagnostic file. Mutual exclusion
//! is provided by the OS advisory lock held by the open file descriptor; the
//! owner metadata is never used to reclaim or release that lock.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::models::AgentId;
use crate::platform::AgentKey;

pub(crate) const LOCK_PROTOCOL_VERSION: u32 = 2;

/// OS advisory lock held for the lifetime of an owning lock object.
///
/// A hidden sidecar inode is intentionally kept after release. This prevents
/// a pathname replacement race: all contenders open and lock the same sidecar
/// inode, while the visible metadata file may be removed after a normal drop.
#[derive(Debug)]
pub(crate) struct AdvisoryFileLock {
    #[allow(dead_code)]
    file: File,
    #[cfg(windows)]
    overlapped: Box<WindowsOverlapped>,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Debug)]
struct WindowsOverlapped {
    internal: usize,
    internal_high: usize,
    offset: u32,
    offset_high: u32,
    h_event: *mut core::ffi::c_void,
}

impl AdvisoryFileLock {
    /// Open `path` and try to acquire an exclusive, non-blocking OS lock.
    /// `Ok(None)` means another handle currently owns the lock.
    pub(crate) fn try_acquire(path: &Path) -> io::Result<Option<Self>> {
        let file = open_lock_leaf(path)?;

        #[cfg(unix)]
        {
            if !try_lock_unix(&file)? {
                return Ok(None);
            }
            return Ok(Some(Self { file }));
        }

        #[cfg(windows)]
        {
            let mut overlapped = Box::new(WindowsOverlapped {
                internal: 0,
                internal_high: 0,
                offset: 0,
                offset_high: 0,
                h_event: std::ptr::null_mut(),
            });
            match try_lock_windows(&file, &mut overlapped)? {
                true => return Ok(Some(Self { file, overlapped })),
                false => return Ok(None),
            }
        }

        #[cfg(not(any(unix, windows)))]
        {
            drop(file);
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "advisory lock is unsupported on this platform",
            ))
        }
    }
}

/// Open a lock or metadata leaf without following a pre-existing link.
///
/// The OS handle is the authority for both the advisory lock and metadata
/// writes.  Opening with no-follow/reparse protection before checking the
/// resulting handle closes the check-then-open race against a symlink,
/// junction, or other reparse-point replacement.
fn open_lock_leaf(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.custom_flags(libc::O_NOFOLLOW);
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        const FILE_SHARE_WRITE: u32 = 0x0000_0002;

        // Do not share delete: an open lock/metadata leaf cannot be renamed
        // or unlinked underneath the handle while it is being used.
        options
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }

    let file = options.open(path)?;
    ensure_regular_lock_leaf(&file)?;
    Ok(file)
}

fn open_existing_lock_leaf(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.custom_flags(libc::O_NOFOLLOW);
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        const FILE_SHARE_WRITE: u32 = 0x0000_0002;

        options
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }

    let file = options.open(path)?;
    ensure_regular_lock_leaf(&file)?;
    Ok(file)
}

pub(crate) fn read_metadata_file(path: &Path) -> io::Result<String> {
    let mut file = open_existing_lock_leaf(path)?;
    let mut raw = String::new();
    file.read_to_string(&mut raw)?;
    Ok(raw)
}

#[cfg(unix)]
fn ensure_regular_lock_leaf(file: &File) -> io::Result<()> {
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
fn ensure_regular_lock_leaf(file: &File) -> io::Result<()> {
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
    if information.file_attributes
        & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT)
        != 0
        || information.reparse_tag != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "lock leaf is a directory or reparse point",
        ));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn ensure_regular_lock_leaf(file: &File) -> io::Result<()> {
    let metadata = file.metadata()?;
    if metadata.file_type().is_file() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "lock leaf is not a regular file",
        ))
    }
}

/// Write diagnostic metadata to the public `.lock` pathname. The caller must
/// hold the corresponding advisory sidecar lock before calling this helper.
pub(crate) fn write_metadata_file(path: &Path, metadata: &str) -> io::Result<()> {
    let mut file = open_lock_leaf(path)?;
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(metadata.as_bytes())?;
    file.sync_all()
}

/// Stable inode used for the real OS lock. The visible `.lock` file remains
/// diagnostics and may be removed after a normal owner drop without opening a
/// pathname-replacement race: contenders always lock this sidecar first.
pub(crate) fn advisory_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("lock");
    path.with_file_name(format!(".{name}.os-lock"))
}

#[cfg(unix)]
fn try_lock_unix(file: &File) -> io::Result<bool> {
    use std::os::fd::AsRawFd;

    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;

    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }

    let result = unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) };
    if result == 0 {
        return Ok(true);
    }

    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        // Linux uses EWOULDBLOCK == EAGAIN == 11; macOS uses 35.
        Some(11) | Some(35) => Ok(false),
        _ => Err(error),
    }
}

#[cfg(windows)]
fn try_lock_windows(file: &File, overlapped: &mut WindowsOverlapped) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;

    const LOCKFILE_FAIL_IMMEDIATELY: u32 = 0x0000_0001;
    const LOCKFILE_EXCLUSIVE_LOCK: u32 = 0x0000_0002;
    const ERROR_LOCK_VIOLATION: i32 = 33;

    unsafe extern "system" {
        fn LockFileEx(
            file: *mut core::ffi::c_void,
            flags: u32,
            reserved: u32,
            bytes_to_lock_low: u32,
            bytes_to_lock_high: u32,
            overlapped: *mut WindowsOverlapped,
        ) -> i32;
    }

    let ok = unsafe {
        LockFileEx(
            file.as_raw_handle() as *mut core::ffi::c_void,
            LOCKFILE_FAIL_IMMEDIATELY | LOCKFILE_EXCLUSIVE_LOCK,
            0,
            u32::MAX,
            u32::MAX,
            overlapped,
        )
    };
    if ok != 0 {
        return Ok(true);
    }

    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_LOCK_VIOLATION) {
        Ok(false)
    } else {
        Err(error)
    }
}

#[cfg(windows)]
impl Drop for AdvisoryFileLock {
    fn drop(&mut self) {
        use std::os::windows::io::AsRawHandle;

        unsafe extern "system" {
            fn UnlockFileEx(
                file: *mut core::ffi::c_void,
                reserved: u32,
                bytes_to_unlock_low: u32,
                bytes_to_unlock_high: u32,
                overlapped: *mut WindowsOverlapped,
            ) -> i32;
        }

        unsafe {
            let _ = UnlockFileEx(
                self.file.as_raw_handle() as *mut core::ffi::c_void,
                0,
                u32::MAX,
                u32::MAX,
                &mut *self.overlapped,
            );
        }
    }
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

/// Scan `{data_dir}/locks/provider-*.lock` without changing lock ownership.
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
    let raw = match read_metadata_file(path) {
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

    let Some(owner) = LockOwner::parse(&raw) else {
        return LockInspection {
            agent: agent.to_string(),
            path: display,
            status: "malformed".into(),
            pid: None,
            created_unix_ms: None,
            note: Some("lock metadata is missing required pid/created_unix_ms/token".into()),
        };
    };

    let os_path = advisory_path(path);
    if !os_path.exists() {
        return LockInspection {
            agent: agent.to_string(),
            path: display,
            status: "stale".into(),
            pid: Some(owner.pid),
            created_unix_ms: Some(owner.created_unix_ms),
            note: Some("legacy lock metadata has no active OS lock".into()),
        };
    }

    match AdvisoryFileLock::try_acquire(&os_path) {
        Ok(Some(_probe)) => LockInspection {
            agent: agent.to_string(),
            path: display,
            status: "stale".into(),
            pid: Some(owner.pid),
            created_unix_ms: Some(owner.created_unix_ms),
            note: Some("lock file is not currently held".into()),
        },
        Ok(None) => LockInspection {
            agent: agent.to_string(),
            path: display,
            status: "held".into(),
            pid: Some(owner.pid),
            created_unix_ms: Some(owner.created_unix_ms),
            note: None,
        },
        Err(error) => LockInspection {
            agent: agent.to_string(),
            path: display,
            // If the OS cannot confirm that the lock is free, report it as
            // held rather than suggesting an unsafe reclaim.
            status: "held".into(),
            pid: Some(owner.pid),
            created_unix_ms: Some(owner.created_unix_ms),
            note: Some(format!("could not inspect OS lock: {error}")),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LockOwner {
    protocol: u32,
    pid: u32,
    created_unix_ms: u64,
    token: String,
}

impl LockOwner {
    fn current() -> Self {
        Self {
            protocol: LOCK_PROTOCOL_VERSION,
            pid: std::process::id(),
            created_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or(0),
            token: Uuid::new_v4().to_string(),
        }
    }

    fn serialize(&self) -> String {
        format!(
            "protocol={}\npid={}\ncreated_unix_ms={}\ntoken={}\n",
            self.protocol, self.pid, self.created_unix_ms, self.token
        )
    }

    /// Metadata is diagnostic only. Invalid/partial metadata never authorizes
    /// reclaim, but it also never prevents the OS lock from being acquired
    /// after the previous handle has closed.
    fn parse(raw: &str) -> Option<Self> {
        // Protocol 1 is the pre-sidecar visible-only lock format. Keeping it
        // parseable lets inspection remain useful, while acquisition can
        // conservatively reject it during the migration handshake.
        let mut protocol = Some(1);
        let mut pid = None;
        let mut created_unix_ms = None;
        let mut token = None;

        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once('=')?;
            match key.trim() {
                "protocol" => protocol = Some(value.trim().parse::<u32>().ok()?),
                "pid" => pid = Some(value.trim().parse::<u32>().ok()?),
                "created_unix_ms" => {
                    created_unix_ms = Some(value.trim().parse::<u64>().ok()?)
                }
                "token" => {
                    let value = value.trim();
                    if value.is_empty() {
                        return None;
                    }
                    token = Some(value.to_owned());
                }
                _ => {}
            }
        }

        Some(Self {
            protocol: protocol?,
            pid: pid?,
            created_unix_ms: created_unix_ms?,
            token: token?,
        })
    }

    fn same_identity(&self, other: &Self) -> bool {
        self == other
    }

    fn is_current_protocol(&self) -> bool {
        self.protocol == LOCK_PROTOCOL_VERSION
    }
}

/// Per-agent exclusive live-write lock. The OS handle, not owner metadata,
/// controls ownership and is automatically released when this value drops or
/// its process exits.
#[derive(Debug)]
pub struct AgentWriteLock {
    path: PathBuf,
    owner: LockOwner,
    pub(crate) _advisory: AdvisoryFileLock,
}

impl AgentWriteLock {
    /// Acquire the shared per-agent live-write lock.
    ///
    /// The historical `provider-{agent}.lock` filename is retained so provider
    /// and account switches mutually exclude on the same path.
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
        let os_path = advisory_path(path);
        let Some(advisory) = AdvisoryFileLock::try_acquire(&os_path)? else {
            return Ok(None);
        };

        // A visible file without the protocol marker is owned by (or may be
        // the interrupted write of) a pre-sidecar binary. The old binary
        // cannot lock the sidecar, so never overwrite that metadata merely
        // because the sidecar exists. A protocol-v2 file is safe to replace
        // once this process owns the sidecar; an absent file is the first
        // acquisition. All other read/parse failures fail closed.
        match read_metadata_file(path) {
            Ok(raw)
                if LockOwner::parse(&raw)
                    .is_some_and(|owner| owner.is_current_protocol()) => {}
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }

        let owner = LockOwner::current();
        write_metadata_file(path, &owner.serialize())?;
        Ok(Some(Self {
            path: path.to_path_buf(),
            owner,
            _advisory: advisory,
        }))
    }
}

impl Drop for AgentWriteLock {
    fn drop(&mut self) {
        // The sidecar remains held until this Drop body finishes. A normal
        // owner removes only matching metadata; replacement content is left
        // untouched. The sidecar itself is intentionally persistent.
        if let Ok(raw) = read_metadata_file(&self.path) {
            if LockOwner::parse(&raw).is_some_and(|owner| owner.same_identity(&self.owner)) {
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

#[cfg(test)]
mod tests;
