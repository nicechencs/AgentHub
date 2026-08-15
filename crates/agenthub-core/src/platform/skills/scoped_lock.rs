//! Per-skill / root exclusive locks under `<source_root>/.locks/`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{AppError, Result};

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

/// Lightweight exclusive lock (same format as AgentWriteLock) for skill ids.
pub(crate) struct SkillScopedLock {
    path: PathBuf,
    file: Option<std::fs::File>,
    token: String,
}

impl SkillScopedLock {
    fn acquire(lock_dir: &Path, key: &str) -> Result<Self> {
        use std::fs::OpenOptions;
        use std::io::Write;
        use uuid::Uuid;

        fs::create_dir_all(lock_dir)?;
        // Sanitize key for filename.
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
        let token = Uuid::new_v4().to_string();
        let body = format!(
            "pid={}\ncreated_unix_ms={}\ntoken={token}\n",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );

        for _ in 0..3 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    file.write_all(body.as_bytes())?;
                    let _ = file.sync_all();
                    return Ok(Self {
                        path,
                        file: Some(file),
                        token,
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    // Stale reclaim: if owner pid dead or file unreadable, remove.
                    if let Ok(raw) = fs::read_to_string(&path) {
                        let mut pid = None;
                        for line in raw.lines() {
                            if let Some(v) = line.strip_prefix("pid=") {
                                pid = v.trim().parse::<u32>().ok();
                            }
                        }
                        let dead = pid.is_some_and(|p| !process_is_alive_skill(p));
                        if dead {
                            let _ = fs::remove_file(&path);
                            continue;
                        }
                    } else {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                    return Err(AppError::message(
                        "skill.lock",
                        format!("another skill write is already running for '{key}'"),
                    ));
                }
                Err(e) => return Err(AppError::from(e)),
            }
        }
        Err(AppError::message(
            "skill.lock",
            format!("could not acquire skill lock for '{key}'"),
        ))
    }
}

impl Drop for SkillScopedLock {
    fn drop(&mut self) {
        drop(self.file.take());
        if let Ok(raw) = fs::read_to_string(&self.path) {
            if raw.contains(&self.token) {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

pub(crate) fn process_is_alive_skill(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map(|o| {
                let s = String::from_utf8_lossy(&o.stdout);
                s.contains(&pid.to_string())
            })
            .unwrap_or(true)
    }
    #[cfg(not(windows))]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
}
