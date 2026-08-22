//! Per-skill / root exclusive locks under `<source_root>/.locks/`.
//!
//! Mutual exclusion is process-local, matching [`crate::utils::agent_lock`].
//! Leftover or malformed lock files do not block a later acquire.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{AppError, Result};
use crate::utils::agent_lock::{release_lock_path, try_claim_lock_path};

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
    pid: u32,
    created_unix_ms: u64,
    token: String,
}

impl SkillLockOwner {
    fn current() -> Self {
        Self {
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
            "pid={}\ncreated_unix_ms={}\ntoken={}\n",
            self.pid, self.created_unix_ms, self.token
        )
    }

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
            match key.trim() {
                "pid" => pid = Some(value.trim().parse::<u32>().ok()?),
                "created_unix_ms" => created_unix_ms = Some(value.trim().parse::<u64>().ok()?),
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
            pid: pid?,
            created_unix_ms: created_unix_ms?,
            token: token?,
        })
    }
}

/// Lightweight exclusive lock for a skill id or the shared skill root.
pub(crate) struct SkillScopedLock {
    path: PathBuf,
    file: Option<std::fs::File>,
    token: String,
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

        if !try_claim_lock_path(&path) {
            return Err(lock_held_error(key));
        }

        let owner = SkillLockOwner::current();
        match write_owner_file(&path, &owner.serialize()) {
            Ok(file) => Ok(Self {
                path,
                file: Some(file),
                token: owner.token,
            }),
            Err(error) => {
                release_lock_path(&path);
                let _ = fs::remove_file(&path);
                Err(error.into())
            }
        }
    }
}

impl Drop for SkillScopedLock {
    fn drop(&mut self) {
        drop(self.file.take());
        if let Ok(raw) = fs::read_to_string(&self.path) {
            if SkillLockOwner::parse(&raw).is_some_and(|owner| owner.token == self.token) {
                let _ = fs::remove_file(&self.path);
            }
        }
        release_lock_path(&self.path);
    }
}

fn write_owner_file(path: &Path, metadata: &str) -> io::Result<std::fs::File> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(metadata.as_bytes())?;
    let _ = file.sync_all();
    Ok(file)
}

fn lock_held_error(key: &str) -> AppError {
    AppError::message(
        "skill.lock",
        format!("another skill write is already running for '{key}'"),
    )
}

#[cfg(test)]
mod tests;
