//! Snapshot materializer: copy/hardlink live files, write manifest, index the row.
//!
//! `snapshot()` is `snapshot_inner` only — it must not acquire the live-write
//! lock. `snapshot_with_guard` validates an enclosing guard then calls the same
//! inner path.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::models::{AgentId, BackupKind, BackupRecord};
use crate::services::LiveWriteGuard;
use crate::utils::paths::is_safe_path;

use super::path_safety::{
    allocate_dest_name, classify_path, ensure_regular_file, is_path_inside,
    is_path_strictly_inside, sanitize_basename, PathClass,
};
use super::{elapsed_ms, BackupService};

/// On-disk mapping written next to snapshot files.
///
/// `source` records the live path at snapshot time for deterministic pairing
/// with today's `live_backup_paths`; restore still rejects any source that is
/// not present on the current adapter allow-list.
pub const MANIFEST_FILE: &str = "manifest.json";
pub const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupManifest {
    pub version: u32,
    pub entries: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestEntry {
    /// Basename stored inside the snapshot directory.
    pub stored: String,
    /// Absolute live path that was copied (identity only; must match adapter).
    pub source: String,
    /// SHA-256 hex of file bytes at snapshot time. Absent on legacy manifests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// One live file about to be snapshotted (allocated dest name + content hash).
pub struct PlannedEntry {
    pub stored: String,
    pub source: PathBuf,
    pub sha256: String,
}

impl BackupService {
    /// Snapshot live files for `agent`.
    ///
    /// - Candidate paths come only from the registered adapter.
    /// - Only existing regular files are snapshotted.
    /// - Zero existing files → [`AppError::NotFound`], no DB row.
    /// - Identical content reuses the newest matching historical snapshot.
    /// - Writes `manifest.json` mapping stored basenames → live sources
    ///   (and SHA-256 when available).
    /// - DB index is written only after every copy/hardlink succeeds.
    /// - On failure: no DB row; best-effort removal of the incomplete snapshot
    ///   directory when it is exactly under `backups_root`.
    pub fn snapshot(
        &self,
        agent: AgentId,
        kind: BackupKind,
        note: Option<&str>,
    ) -> Result<BackupRecord> {
        self.snapshot_inner(agent, kind, note)
    }

    /// Snapshot while a larger live-write saga already holds the authority.
    pub fn snapshot_with_guard(
        &self,
        guard: &LiveWriteGuard,
        agent: AgentId,
        kind: BackupKind,
        note: Option<&str>,
    ) -> Result<BackupRecord> {
        self.authority.validate_guard(guard, agent)?;
        self.snapshot_inner(agent, kind, note)
    }

    /// Incremental snapshot:
    /// - If this agent's newest matching snapshot has the same live source set
    ///   and SHA-256 content, bump `created_at` and reuse that row/dir.
    /// - Otherwise create a new id/dir/row; unchanged bytes are hardlinked
    ///   from a prior snapshot when the hash is already stored, else copied.
    ///   Hardlink failure falls back to `std::fs::copy` (never symlinks).
    fn snapshot_inner(
        &self,
        agent: AgentId,
        kind: BackupKind,
        note: Option<&str>,
    ) -> Result<BackupRecord> {
        let started = Instant::now();
        let result = (|| {
            let adapter = self.registry.get(agent).ok_or_else(|| {
                AppError::NotFound(format!(
                    "no adapter registered for agent {}",
                    agent.as_str()
                ))
            })?;

            let candidates = adapter.live_backup_paths();
            let mut sources: Vec<PathBuf> = Vec::new();
            for path in candidates {
                if !is_safe_path(&path) {
                    return Err(AppError::InvalidArg(format!(
                        "unsafe backup source path: {}",
                        path.display()
                    )));
                }
                // Only regular files that currently exist (skip dirs/symlinks).
                match classify_path(&path)? {
                    PathClass::RegularFile => sources.push(path),
                    PathClass::Missing
                    | PathClass::Directory
                    | PathClass::Symlink
                    | PathClass::Other => {}
                }
            }

            if sources.is_empty() {
                return Err(AppError::NotFound(format!(
                    "no backupable live files for agent {}",
                    agent.as_str()
                )));
            }

            let (planned, total_size) = plan_snapshot_entries(&sources)?;
            if let Some(existing) = self.find_identical_snapshot(agent, &planned, total_size)? {
                let now = Utc::now().to_rfc3339();
                return self.repo.touch_created_at(&existing.id, &now);
            }

            let mut hash_index = self.content_index_for_agent(agent)?;

            let id = Uuid::new_v4().to_string();
            let snapshot_dir = self
                .backups_root
                .join("live")
                .join(agent.as_str())
                .join(&id);

            if !is_path_inside(&snapshot_dir, &self.backups_root) {
                return Err(AppError::message(
                    "backup.path",
                    format!(
                        "snapshot path escapes backups_root: {}",
                        snapshot_dir.display()
                    ),
                ));
            }

            if let Err(e) = std::fs::create_dir_all(&snapshot_dir) {
                return Err(AppError::from(e));
            }

            let copy_result = self.materialize_sources(&planned, &snapshot_dir, &mut hash_index);
            let (files, size, manifest) = match copy_result {
                Ok(v) => v,
                Err(e) => {
                    self.best_effort_remove_snapshot(&snapshot_dir);
                    return Err(e);
                }
            };

            if let Err(e) = write_manifest(&snapshot_dir, &manifest) {
                self.best_effort_remove_snapshot(&snapshot_dir);
                return Err(e);
            }

            let record = BackupRecord {
                id: id.clone(),
                agent_id: Some(agent),
                kind,
                path: snapshot_dir.display().to_string(),
                files,
                size,
                note: note.map(|s| s.to_string()).filter(|s| !s.is_empty()),
                created_at: Utc::now().to_rfc3339(),
            };

            if let Err(e) = self.repo.insert(&record) {
                self.best_effort_remove_snapshot(&snapshot_dir);
                return Err(e);
            }

            Ok(record)
        })();

        match &result {
            Ok(record) => {
                tracing::info!(
                    module = targets::BACKUP,
                    op = "snapshot",
                    id = %record.id,
                    kind = kind.as_str(),
                    agent = agent.as_str(),
                    files = record.files.len(),
                    size = record.size,
                    elapsed_ms = elapsed_ms(started),
                    "snapshot ok"
                );
            }
            Err(e) => {
                logging::log_app_error_agent(targets::BACKUP, "snapshot", agent.as_str(), e);
            }
        }
        result
    }
    fn materialize_sources(
        &self,
        planned: &[PlannedEntry],
        snapshot_dir: &Path,
        hash_index: &mut HashMap<String, PathBuf>,
    ) -> Result<(Vec<String>, u64, BackupManifest)> {
        let mut files: Vec<String> = Vec::with_capacity(planned.len());
        let mut total_size: u64 = 0;
        let mut entries: Vec<ManifestEntry> = Vec::with_capacity(planned.len());

        for entry in planned {
            let dest = snapshot_dir.join(&entry.stored);
            if !is_path_inside(&dest, snapshot_dir) {
                return Err(AppError::InvalidArg(format!(
                    "destination escapes snapshot dir: {}",
                    entry.stored
                )));
            }
            ensure_regular_file(&entry.source)?;

            let hash_key = entry.sha256.to_ascii_lowercase();
            let reused = hash_index.get(&hash_key).cloned();
            let linked = reused.as_ref().is_some_and(|existing| {
                ensure_regular_file(existing).is_ok()
                    && sha256_file(existing).ok().as_deref() == Some(hash_key.as_str())
                    && std::fs::hard_link(existing, &dest).is_ok()
            });
            if !linked {
                std::fs::copy(&entry.source, &dest)?;
            }
            ensure_regular_file(&dest)?;
            verify_sha256(&dest, &entry.sha256)?;
            let len = std::fs::metadata(&dest)?.len();
            total_size = total_size.saturating_add(len);
            files.push(entry.stored.clone());
            entries.push(ManifestEntry {
                stored: entry.stored.clone(),
                source: entry.source.display().to_string(),
                sha256: Some(entry.sha256.clone()),
            });
            hash_index.entry(hash_key).or_insert(dest);
        }

        Ok((
            files,
            total_size,
            BackupManifest {
                version: MANIFEST_VERSION,
                entries,
            },
        ))
    }

    /// Best-effort cleanup of an incomplete snapshot. Only removes when the
    /// path is exactly a directory under `backups_root` (lexical containment).
    fn best_effort_remove_snapshot(&self, snapshot_dir: &Path) {
        if !is_path_strictly_inside(snapshot_dir, &self.backups_root) {
            return;
        }
        if matches!(classify_path(snapshot_dir), Ok(PathClass::Directory)) {
            let _ = std::fs::remove_dir_all(snapshot_dir);
        }
    }
}

fn plan_snapshot_entries(sources: &[PathBuf]) -> Result<(Vec<PlannedEntry>, u64)> {
    let mut occupied: HashSet<String> = HashSet::new();
    let mut planned = Vec::with_capacity(sources.len());
    let mut total_size: u64 = 0;
    for src in sources {
        let dest_name = allocate_dest_name(src, &mut occupied)?;
        ensure_regular_file(src)?;
        let sha256 = sha256_file(src)?;
        let len = std::fs::metadata(src)?.len();
        total_size = total_size.saturating_add(len);
        planned.push(PlannedEntry {
            stored: dest_name,
            source: src.clone(),
            sha256,
        });
    }
    Ok((planned, total_size))
}

pub(super) fn planned_matches_manifest(
    planned: &[PlannedEntry],
    manifest: &BackupManifest,
    dir: &Path,
) -> bool {
    if planned.len() != manifest.entries.len() {
        return false;
    }
    for (p, e) in planned.iter().zip(&manifest.entries) {
        if p.stored != e.stored {
            return false;
        }
        if p.source.display().to_string() != e.source {
            return false;
        }
        let stored_path = dir.join(&e.stored);
        if !is_path_inside(&stored_path, dir) {
            return false;
        }
        if ensure_regular_file(&stored_path).is_err() {
            return false;
        }
        let hash = match sha256_file(&stored_path) {
            Ok(h) => h,
            Err(_) => return false,
        };
        if let Some(manifest_hash) = e.sha256.as_deref() {
            let Ok(manifest_hash) = normalize_sha256(manifest_hash) else {
                return false;
            };
            if !manifest_hash.eq_ignore_ascii_case(&p.sha256) {
                return false;
            }
        }
        if !hash.eq_ignore_ascii_case(&p.sha256) {
            return false;
        }
    }
    true
}

pub(super) fn index_stored_file(
    index: &mut HashMap<String, PathBuf>,
    dir: &Path,
    stored: &str,
    sha256: Option<String>,
) {
    let Ok(base) = sanitize_basename(stored) else {
        return;
    };
    if base != stored {
        return;
    }
    let stored_path = dir.join(&base);
    if !is_path_inside(&stored_path, dir) {
        return;
    }
    if ensure_regular_file(&stored_path).is_err() {
        return;
    }
    let actual_hash = match sha256_file(&stored_path) {
        Ok(h) => h,
        Err(_) => return,
    };
    if let Some(declared) = sha256.as_deref() {
        let Ok(declared) = normalize_sha256(declared) else {
            return;
        };
        if !declared.eq_ignore_ascii_case(&actual_hash) {
            return;
        }
    }
    index.entry(actual_hash).or_insert(stored_path);
}

fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_encode(hasher.finalize().as_slice()))
}

pub(super) fn normalize_sha256(raw: &str) -> Result<String> {
    if raw.len() != 64 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::InvalidArg(format!(
            "invalid SHA-256 digest in backup manifest: {raw:?}"
        )));
    }
    Ok(raw.to_ascii_lowercase())
}

pub(super) fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    let actual = sha256_file(path)?;
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(AppError::InvalidArg(format!(
            "backup payload hash mismatch for {}",
            path.display()
        )));
    }
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

pub fn write_manifest(snapshot_dir: &Path, manifest: &BackupManifest) -> Result<()> {
    let path = snapshot_dir.join(MANIFEST_FILE);
    if !is_path_inside(&path, snapshot_dir) {
        return Err(AppError::message(
            "backup.manifest",
            "manifest path escapes snapshot dir",
        ));
    }
    let json = serde_json::to_vec_pretty(manifest)?;
    std::fs::write(&path, json)?;
    Ok(())
}

pub(super) fn read_manifest(snapshot_dir: &Path) -> Result<Option<BackupManifest>> {
    let path = snapshot_dir.join(MANIFEST_FILE);
    match classify_path(&path)? {
        PathClass::RegularFile => {
            let bytes = std::fs::read(&path)?;
            let m: BackupManifest = serde_json::from_slice(&bytes).map_err(|e| {
                AppError::InvalidArg(format!(
                    "corrupt backup manifest at {}: {e}",
                    path.display()
                ))
            })?;
            if m.version != MANIFEST_VERSION {
                return Err(AppError::InvalidArg(format!(
                    "unsupported backup manifest version: {}",
                    m.version
                )));
            }
            Ok(Some(m))
        }
        PathClass::Missing => Ok(None),
        PathClass::Symlink => Err(AppError::InvalidArg(format!(
            "backup manifest is a symlink: {}",
            path.display()
        ))),
        PathClass::Directory | PathClass::Other => Err(AppError::InvalidArg(format!(
            "backup manifest is not a regular file: {}",
            path.display()
        ))),
    }
}
