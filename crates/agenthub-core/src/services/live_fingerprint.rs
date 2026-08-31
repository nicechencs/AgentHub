//! Hashes of live agent files AgentHub last created or changed.
//!
//! [`record_changed`] stores SHA-256s after a live write for paths whose
//! bytes appeared or changed. [`classify`] compares a file against that
//! record so restore can tell a still-managed file from a hand edit.
//! Failures are logged and never propagated.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::AgentId;
use crate::storage::live_fingerprint_repo::LiveFingerprintRepo;
use crate::storage::Database;

#[cfg(test)]
mod tests;

/// State of one live config file relative to AgentHub's last managed write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiveFileState {
    /// File exists and is byte-identical to what AgentHub last wrote.
    Managed,
    /// File exists but its bytes differ from AgentHub's last write.
    Edited,
    /// AgentHub wrote this file before, but it does not exist right now.
    Missing,
    /// AgentHub has no fingerprint for this path (never wrote it).
    Unknown,
}

/// SHA-256 of each path that can be read; missing or unreadable paths are omitted.
pub(crate) fn hash_existing(paths: &[PathBuf]) -> HashMap<PathBuf, String> {
    let mut out = HashMap::new();
    for path in paths {
        if let Ok(sha256) = sha256_file(path) {
            out.insert(path.clone(), sha256);
        }
    }
    out
}

/// Fingerprint paths that exist now and whose bytes are new or different vs `before`.
pub(crate) fn record_changed(
    db: &Database,
    agent_id: AgentId,
    before: &HashMap<PathBuf, String>,
    after_paths: &[PathBuf],
) {
    let mut changed = Vec::new();
    for path in after_paths {
        let Ok(after) = sha256_file(path) else {
            continue;
        };
        if before.get(path) == Some(&after) {
            continue;
        }
        changed.push(path.clone());
    }
    if !changed.is_empty() {
        record_written(db, agent_id, &changed);
    }
}

/// Record fingerprints for `paths` that currently exist.
///
/// Prefer [`record_changed`] after a live write so unchanged siblings are
/// not claimed. Failures are logged and swallowed.
pub(crate) fn record_written(db: &Database, agent_id: AgentId, paths: &[PathBuf]) {
    let repo = LiveFingerprintRepo::new(db.clone());
    let written_at = Utc::now().to_rfc3339();
    for path in paths {
        let sha256 = match sha256_file(path) {
            Ok(sha256) => sha256,
            Err(error) => {
                tracing::warn!(
                    target: targets::PROVIDER,
                    agent = agent_id.as_str(),
                    path = %path.display(),
                    error = %error,
                    "live write fingerprint skipped: file not readable"
                );
                continue;
            }
        };
        if let Err(error) = repo.upsert(
            agent_id.as_str(),
            &path.display().to_string(),
            &sha256,
            &written_at,
        ) {
            tracing::warn!(
                target: targets::PROVIDER,
                agent = agent_id.as_str(),
                path = %path.display(),
                error = %error,
                "live write fingerprint could not be stored"
            );
        }
    }
}

/// Compare one live config file against AgentHub's last managed write.
pub(crate) fn classify(db: &Database, agent: AgentId, path: &Path) -> Result<LiveFileState> {
    let repo = LiveFingerprintRepo::new(db.clone());
    let Some(expected) = repo.get(agent.as_str(), &path.display().to_string())? else {
        return Ok(LiveFileState::Unknown);
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LiveFileState::Missing)
        }
        Err(error) => return Err(AppError::from(error)),
    };
    if sha256_hex(&bytes).eq_ignore_ascii_case(&expected) {
        Ok(LiveFileState::Managed)
    } else {
        Ok(LiveFileState::Edited)
    }
}

fn sha256_file(path: &Path) -> Result<String> {
    Ok(sha256_hex(&std::fs::read(path)?))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}
