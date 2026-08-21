//! Per-skill / root exclusive locks under `<source_root>/.locks/`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{AppError, Result};
use crate::utils::agent_lock::{
    advisory_path, read_metadata_file, write_metadata_file, AdvisoryFileLock,
    LOCK_PROTOCOL_VERSION,
};

/// Per-skill exclusive lock under `<source_root>/.locks/skill-<id>.lock`.
pub(crate) fn acquire_skill_lock(source_root: &Path, skill_id: &str) -> Result<SkillScopedLock> {
    let lock_dir = source_root.join(".locks");
    fs::create_dir_all(&lock_dir)?;
    SkillScopedLock::acquire(&lock_dir, skill_id)
}

pub(crate) fn acquire_skill_root_lock(source_root: &Path) -> Result<SkillScopedLock> {
    let lock_dir = source_root.join(".locks");
    fs::create_dir_all(&lock_dir)?;
    SkillScopedLock::acquire(&lock_dir, "__root__")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkillLockOwner {
    protocol: u32,
    pid: u32,
    created_unix_ms: u64,
    token: String,
}

impl SkillLockOwner {
    fn current() -> Self {
        Self {
            protocol: LOCK_PROTOCOL_VERSION,
            pid: std::process::id(),
            created_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or(0),
            token: uuid::Uuid::new_v4().to_string(),
        }
    }

    fn serialize(&self) -> String {
        format!(
            "protocol={}\npid={}\ncreated_unix_ms={}\ntoken={}\n",
            self.protocol, self.pid, self.created_unix_ms, self.token
        )
    }

    /// Metadata is diagnostic only. It is never used to reclaim or release
    /// the advisory lock, so empty/partial content cannot create a race.
    fn parse(raw: &str) -> Option<Self> {
        // Protocol 1 is the pre-sidecar visible-only skill lock. It remains
        // parseable for diagnostics but is never overwritten during acquire.
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

/// Lightweight exclusive lock for a skill id or the shared skill root.
/// The OS handle is the ownership token and releases automatically on Drop or
/// process exit. The OS-lock sidecar is never renamed or unlinked during
/// release; only matching diagnostic metadata may be removed.
pub(crate) struct SkillScopedLock {
    path: PathBuf,
    owner: SkillLockOwner,
    pub(crate) _advisory: AdvisoryFileLock,
}

impl SkillScopedLock {
    fn acquire(lock_dir: &Path, key: &str) -> Result<Self> {
        fs::create_dir_all(lock_dir)?;
        let safe: String = key
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = lock_dir.join(format!("skill-{safe}.lock"));

        let os_path = advisory_path(&path);
        let Some(advisory) = AdvisoryFileLock::try_acquire(&os_path)? else {
            return Err(lock_held_error(key));
        };

        // Never let a new sidecar lock overwrite a visible lock left by the
        // old create-new-only protocol. Protocol-v2 metadata is safe to
        // replace after the sidecar has been acquired; absent metadata is a
        // first acquisition. Read/parse failures remain fail-closed.
        match read_metadata_file(&path) {
            Ok(raw)
                if SkillLockOwner::parse(&raw)
                    .is_some_and(|owner| owner.is_current_protocol()) => {}
            Ok(_) => return Err(lock_held_error(key)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }

        // Metadata is written only after the OS lock has been acquired. A
        // crash while writing may leave malformed content, but the next
        // contender still relies solely on the OS lock result.
        let owner = SkillLockOwner::current();
        write_metadata_file(&path, &owner.serialize())?;

        Ok(Self {
            path,
            owner,
            _advisory: advisory,
        })
    }
}

impl Drop for SkillScopedLock {
    fn drop(&mut self) {
        if let Ok(raw) = read_metadata_file(&self.path) {
            if SkillLockOwner::parse(&raw)
                .is_some_and(|owner| owner.same_identity(&self.owner))
            {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

fn lock_held_error(key: &str) -> AppError {
    AppError::message(
        "skill.lock",
        format!("another skill write is already running for '{key}'"),
    )
}

#[cfg(test)]
mod tests;
