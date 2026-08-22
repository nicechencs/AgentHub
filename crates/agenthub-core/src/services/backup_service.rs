//! Live backup snapshot / restore / delete service.
//!
//! Incremental live snapshots:
//! - Same agent + same live source set + same SHA-256 content reuses the
//!   newest matching historical snapshot (`created_at` bumped; no new
//!   directory or DB row).
//! - A partial change creates a new `backups_root/live/<agent>/<id>/`
//!   snapshot. Unchanged file bytes are hardlinked from a prior snapshot
//!   when the hash is already stored; `std::fs::copy` is the fallback
//!   (never symlinks).
//!
//! Restore destinations are always derived from the registered adapter's
//! `live_backup_paths` — never from untrusted absolute paths alone.
//! Delete removes the snapshot directory (when contained) and the index row.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::adapters::AdapterRegistry;
use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::models::{AgentId, BackupKind, BackupRecord};
use crate::services::{LiveWriteAuthority, LiveWriteGuard};
use crate::storage::{BackupRepo, Database};
use crate::utils::paths::is_safe_path;

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// On-disk mapping written next to snapshot files.
///
/// `source` records the live path at snapshot time for deterministic pairing
/// with today's `live_backup_paths`; restore still rejects any source that is
/// not present on the current adapter allow-list.
const MANIFEST_FILE: &str = "manifest.json";
const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct BackupManifest {
    version: u32,
    entries: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ManifestEntry {
    /// Basename stored inside the snapshot directory.
    stored: String,
    /// Absolute live path that was copied (identity only; must match adapter).
    source: String,
    /// SHA-256 hex of file bytes at snapshot time. Absent on legacy manifests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
}

/// One live file about to be snapshotted (allocated dest name + content hash).
struct PlannedEntry {
    stored: String,
    source: PathBuf,
    sha256: String,
}

/// Result of a successful restore.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    /// The backup that was applied.
    pub restored: BackupRecord,
    /// Automatic PreRestore snapshot of live files before overwrite, if any
    /// backupable live files existed.
    pub pre_restore: Option<BackupRecord>,
    /// Live destinations that received a restored file.
    pub restored_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct RestoreItem {
    stored_path: PathBuf,
    dest: PathBuf,
    /// Hash recorded by a v1 manifest. Legacy manifests omit this field.
    expected_sha256: Option<String>,
}

#[derive(Debug)]
enum PathClass {
    RegularFile,
    Directory,
    Symlink,
    Missing,
    Other,
}

/// One applied live write, for failure rollback.
enum AppliedOp {
    /// Destination did not exist; created by restore.
    Created { dest: PathBuf },
    /// Destination existed; previous bytes saved under `backup_of_old`.
    Replaced {
        dest: PathBuf,
        backup_of_old: PathBuf,
    },
}

/// Orchestrates live file snapshots + backup index rows + restore/delete.
#[derive(Clone)]
pub struct BackupService {
    repo: BackupRepo,
    registry: AdapterRegistry,
    backups_root: PathBuf,
    authority: LiveWriteAuthority,
}

impl BackupService {
    /// Explicit dependencies — no implicit home/data-dir resolution.
    pub fn new(db: Database, registry: AdapterRegistry, backups_root: PathBuf) -> Self {
        Self {
            repo: BackupRepo::new(db.clone()),
            registry,
            backups_root,
            authority: LiveWriteAuthority::from_database(&db),
        }
    }

    pub fn backups_root(&self) -> &Path {
        &self.backups_root
    }

    pub fn repo(&self) -> &BackupRepo {
        &self.repo
    }

    /// List indexed backups newest-first; optional agent filter.
    pub fn list(&self, agent: Option<AgentId>) -> Result<Vec<BackupRecord>> {
        self.repo.list(agent)
    }

    /// Fetch a single backup by id.
    pub fn get_by_id(&self, id: &str) -> Result<BackupRecord> {
        self.repo
            .get_by_id(id)?
            .ok_or_else(|| AppError::NotFound(format!("backup not found: {id}")))
    }

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

    /// Restore live files from an indexed backup.
    ///
    /// Security:
    /// - Snapshot directory must stay inside `backups_root`.
    /// - Stored entries must be regular files (no symlinks / directories).
    /// - Agent must be registered; destinations come only from
    ///   `adapter.live_backup_paths()`.
    /// - Manifest (or legacy deterministic mapping) never grants destinations
    ///   outside the adapter allow-list.
    /// - Creates a `PreRestore` snapshot before overwriting any live file.
    /// - Staged replacement with rollback of partial applies on failure.
    pub fn restore(&self, id: &str) -> Result<RestoreResult> {
        let started = Instant::now();
        let result = (|| {
            let record = self.get_by_id(id)?;
            let agent = record.agent_id.ok_or_else(|| {
                AppError::InvalidArg(format!(
                    "backup {id} has no agent_id; cannot restore live files"
                ))
            })?;
            let guard = self.authority.acquire(agent)?;
            self.restore_with_guard(&guard, id)
        })();

        match &result {
            Ok(restored) => {
                let agent = restored
                    .restored
                    .agent_id
                    .map(|a| a.as_str())
                    .unwrap_or("-");
                tracing::info!(
                    module = targets::BACKUP,
                    op = "restore",
                    id = id,
                    kind = restored.restored.kind.as_str(),
                    agent = agent,
                    paths = restored.restored_paths.len(),
                    elapsed_ms = elapsed_ms(started),
                    "restore ok"
                );
            }
            Err(e) => {
                logging::log_app_error(targets::BACKUP, "restore", e);
            }
        }
        result
    }

    /// Restore while an enclosing provider or bridge saga already holds the
    /// same database-derived per-agent authority.
    pub fn restore_with_guard(&self, guard: &LiveWriteGuard, id: &str) -> Result<RestoreResult> {
        let record = self.get_by_id(id)?;
        let agent = record.agent_id.ok_or_else(|| {
            AppError::InvalidArg(format!(
                "backup {id} has no agent_id; cannot restore live files"
            ))
        })?;
        self.authority.validate_guard(guard, agent)?;
        self.restore_record(guard, id, record, agent)
    }

    fn restore_record(
        &self,
        guard: &LiveWriteGuard,
        id: &str,
        record: BackupRecord,
        agent: AgentId,
    ) -> Result<RestoreResult> {
        let adapter = self.registry.get(agent).ok_or_else(|| {
            AppError::NotFound(format!(
                "no adapter registered for agent {}",
                agent.as_str()
            ))
        })?;

        let ordered_live = adapter.live_backup_paths();
        let allowed = allowed_path_map(&ordered_live)?;
        let snapshot_dir = self.validate_snapshot_dir(&record)?;
        let plan = build_restore_plan(&record, &snapshot_dir, &ordered_live, &allowed)?;

        if plan.is_empty() {
            return Err(AppError::NotFound(format!(
                "backup {id} has no restorable files matching adapter live paths"
            )));
        }

        // PreRestore of current live — soft-skip when nothing exists yet.
        let pre_restore = match self.snapshot_with_guard(
            guard,
            agent,
            BackupKind::PreRestore,
            Some(&format!("auto before restore of {id}")),
        ) {
            Ok(pre) => Some(pre),
            Err(e) if e.code() == "not_found" => None,
            Err(e) => return Err(e),
        };

        if let Err(e) = apply_restore_plan(&plan) {
            // apply_restore_plan rolls back partial live writes and surfaces
            // backup.rollback when that compensation fails. Only then try
            // PreRestore as a second-chance recovery of live files.
            if e.code() == "backup.rollback" {
                if let Some(ref pre) = pre_restore {
                    if let Err(re) = self.reapply_snapshot_files(pre, &ordered_live, &allowed) {
                        return Err(AppError::message(
                            "backup.rollback",
                            format!("{e}; PreRestore recovery also failed: {re}"),
                        ));
                    }
                }
            }
            return Err(e);
        }

        Ok(RestoreResult {
            restored: record,
            pre_restore,
            restored_paths: plan.iter().map(|p| p.dest.clone()).collect(),
        })
    }

    /// Delete an indexed backup with compensatable steps:
    /// validate exact snapshot identity → tombstone-rename → delete DB row →
    /// remove tombstone. On any late failure, restore directory and/or row
    /// where possible and report `backup.rollback`.
    pub fn delete(&self, id: &str) -> Result<()> {
        let started = Instant::now();
        let mut agent_for_log: Option<AgentId> = None;
        let mut kind_for_log: Option<BackupKind> = None;
        let result = (|| {
            let record = self.get_by_id(id)?;
            agent_for_log = record.agent_id;
            kind_for_log = Some(record.kind);
            let snapshot_dir = self.validate_snapshot_identity(&record)?;

            match classify_path(&snapshot_dir)? {
                PathClass::Missing => {
                    // Orphan index: path identity was valid but on-disk data is gone.
                    if self.repo.delete(id)? {
                        return Ok(());
                    }
                    Err(AppError::NotFound(format!("backup not found: {id}")))
                }
                PathClass::RegularFile => Err(AppError::InvalidArg(format!(
                    "backup path is a regular file, refusing delete: {}",
                    snapshot_dir.display()
                ))),
                PathClass::Symlink => Err(AppError::InvalidArg(format!(
                    "backup path is a symlink, refusing delete: {}",
                    snapshot_dir.display()
                ))),
                PathClass::Other => Err(AppError::InvalidArg(format!(
                    "backup path is not a directory: {}",
                    snapshot_dir.display()
                ))),
                PathClass::Directory => {
                    self.ensure_snapshot_safe_for_mutation(&snapshot_dir)?;

                    let parent = snapshot_dir.parent().ok_or_else(|| {
                        AppError::InvalidArg("backup snapshot has no parent directory".into())
                    })?;
                    let tombstone =
                        parent.join(format!(".agenthub-delete-{}", Uuid::new_v4().simple()));
                    std::fs::rename(&snapshot_dir, &tombstone)?;

                    match self.repo.delete(id) {
                        Ok(true) => match std::fs::remove_dir_all(&tombstone) {
                            Ok(()) => Ok(()),
                            Err(cleanup_err) => {
                                // Compensating transaction: put directory back, then row.
                                match std::fs::rename(&tombstone, &snapshot_dir) {
                                    Ok(()) => {
                                        if let Err(db_err) = self.repo.insert(&record) {
                                            return Err(AppError::message(
                                                "backup.rollback",
                                                format!(
                                                    "delete cleanup failed ({cleanup_err}); \
                                                     restored directory but failed to restore index: {db_err}"
                                                ),
                                            ));
                                        }
                                        Err(AppError::message(
                                            "backup.rollback",
                                            format!(
                                                "delete cleanup failed ({cleanup_err}); \
                                                 directory and index restored"
                                            ),
                                        ))
                                    }
                                    Err(restore_err) => Err(AppError::message(
                                        "backup.rollback",
                                        format!(
                                            "delete cleanup failed ({cleanup_err}); \
                                             also failed to restore directory ({restore_err}); \
                                             index row already removed"
                                        ),
                                    )),
                                }
                            }
                        },
                        Ok(false) => {
                            if let Err(re) = std::fs::rename(&tombstone, &snapshot_dir) {
                                return Err(AppError::message(
                                    "backup.rollback",
                                    format!(
                                        "backup not found during delete: {id}; \
                                         also failed to restore directory: {re}"
                                    ),
                                ));
                            }
                            Err(AppError::NotFound(format!("backup not found: {id}")))
                        }
                        Err(e) => {
                            if let Err(re) = std::fs::rename(&tombstone, &snapshot_dir) {
                                return Err(AppError::message(
                                    "backup.rollback",
                                    format!(
                                        "db delete failed ({e}); also failed to restore directory: {re}"
                                    ),
                                ));
                            }
                            Err(e)
                        }
                    }
                }
            }
        })();

        match &result {
            Ok(()) => {
                let kind = kind_for_log.map(|k| k.as_str()).unwrap_or("-");
                let agent = agent_for_log.map(|a| a.as_str()).unwrap_or("-");
                tracing::info!(
                    module = targets::BACKUP,
                    op = "delete",
                    id = id,
                    kind = kind,
                    agent = agent,
                    elapsed_ms = elapsed_ms(started),
                    "delete ok"
                );
            }
            Err(e) => {
                if let Some(agent) = agent_for_log {
                    logging::log_app_error_agent(targets::BACKUP, "delete", agent.as_str(), e);
                } else {
                    logging::log_app_error(targets::BACKUP, "delete", e);
                }
            }
        }
        result
    }

    /// Newest-first scan: reuse a completed snapshot whose stored basenames,
    /// live sources, and content hashes match `planned`.
    fn find_identical_snapshot(
        &self,
        agent: AgentId,
        planned: &[PlannedEntry],
        total_size: u64,
    ) -> Result<Option<BackupRecord>> {
        let records = self.repo.list(Some(agent))?;
        for rec in records {
            if rec.size != total_size || rec.files.len() != planned.len() {
                continue;
            }
            if !rec
                .files
                .iter()
                .map(String::as_str)
                .eq(planned.iter().map(|e| e.stored.as_str()))
            {
                continue;
            }
            let dir = match self.validate_snapshot_dir(&rec) {
                Ok(dir) => dir,
                Err(_) => continue,
            };
            let Ok(Some(manifest)) = read_manifest(&dir) else {
                continue;
            };
            if planned_matches_manifest(planned, &manifest, &dir) {
                return Ok(Some(rec));
            }
        }
        Ok(None)
    }

    /// Hash → stored regular file in a prior snapshot for this agent.
    /// Newest snapshot wins when the same hash appears more than once.
    fn content_index_for_agent(&self, agent: AgentId) -> Result<HashMap<String, PathBuf>> {
        let mut index = HashMap::new();
        let records = self.repo.list(Some(agent))?;
        for rec in records {
            let dir = match self.validate_snapshot_dir(&rec) {
                Ok(dir) => dir,
                Err(_) => continue,
            };
            match read_manifest(&dir) {
                Ok(Some(manifest)) => {
                    for entry in manifest.entries {
                        index_stored_file(&mut index, &dir, &entry.stored, entry.sha256);
                    }
                }
                Ok(None) => {
                    for name in &rec.files {
                        index_stored_file(&mut index, &dir, name, None);
                    }
                }
                Err(_) => continue,
            }
        }
        Ok(index)
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

    /// Derive the only legal on-disk location for a live backup:
    /// `backups_root/live/<agent>/<id>`.
    fn expected_snapshot_dir(&self, record: &BackupRecord) -> Result<PathBuf> {
        let agent = record.agent_id.ok_or_else(|| {
            AppError::InvalidArg(format!(
                "backup {} has no agent_id; cannot resolve snapshot path",
                record.id
            ))
        })?;
        let id = sanitize_basename(&record.id)?;
        if id != record.id {
            return Err(AppError::InvalidArg(format!(
                "backup id must be a plain safe basename: {:?}",
                record.id
            )));
        }
        Ok(self
            .backups_root
            .join("live")
            .join(agent.as_str())
            .join(&id))
    }

    /// Lexical identity only: path string must equal the exact expected location.
    /// Rejects backups_root, category/agent parents, sibling backups, and escapes.
    fn validate_snapshot_identity(&self, record: &BackupRecord) -> Result<PathBuf> {
        let expected = self.expected_snapshot_dir(record)?;
        let snapshot_dir = PathBuf::from(&record.path);

        if !is_safe_path(&snapshot_dir) {
            return Err(AppError::InvalidArg(format!(
                "unsafe backup path in index: {}",
                snapshot_dir.display()
            )));
        }
        if !paths_equal_exact(&snapshot_dir, &expected) {
            return Err(AppError::message(
                "backup.path",
                format!(
                    "backup path does not match expected snapshot location: {} (expected {})",
                    snapshot_dir.display(),
                    expected.display()
                ),
            ));
        }
        // Defense in depth: exact child of backups_root, never the root itself.
        if !is_path_strictly_inside(&snapshot_dir, &self.backups_root) {
            return Err(AppError::message(
                "backup.path",
                format!(
                    "backup path is not strictly inside backups_root: {}",
                    snapshot_dir.display()
                ),
            ));
        }
        Ok(snapshot_dir)
    }

    /// Existing directory checks for restore/delete: type, no symlink components
    /// under backups_root, no symlink descendants, strict canonical containment.
    fn ensure_snapshot_safe_for_mutation(&self, snapshot_dir: &Path) -> Result<()> {
        match classify_path(snapshot_dir)? {
            PathClass::Directory => {}
            PathClass::Symlink => {
                return Err(AppError::InvalidArg(format!(
                    "backup path is a symlink: {}",
                    snapshot_dir.display()
                )));
            }
            PathClass::RegularFile => {
                return Err(AppError::InvalidArg(format!(
                    "backup path is a regular file: {}",
                    snapshot_dir.display()
                )));
            }
            PathClass::Missing => {
                return Err(AppError::NotFound(format!(
                    "backup snapshot directory missing: {}",
                    snapshot_dir.display()
                )));
            }
            PathClass::Other => {
                return Err(AppError::InvalidArg(format!(
                    "backup path is not a directory: {}",
                    snapshot_dir.display()
                )));
            }
        }

        ensure_path_components_not_symlinks(snapshot_dir, &self.backups_root)?;
        ensure_tree_has_no_symlinks(snapshot_dir)?;
        ensure_existing_path_strictly_inside(snapshot_dir, &self.backups_root, "backup.path")?;
        Ok(())
    }

    fn validate_snapshot_dir(&self, record: &BackupRecord) -> Result<PathBuf> {
        let snapshot_dir = self.validate_snapshot_identity(record)?;
        self.ensure_snapshot_safe_for_mutation(&snapshot_dir)?;
        Ok(snapshot_dir)
    }

    fn reapply_snapshot_files(
        &self,
        record: &BackupRecord,
        ordered_live: &[PathBuf],
        allowed: &HashMap<String, PathBuf>,
    ) -> Result<()> {
        let snapshot_dir = self.validate_snapshot_dir(record)?;
        let plan = build_restore_plan(record, &snapshot_dir, ordered_live, allowed)?;
        apply_restore_plan(&plan)?;
        Ok(())
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

fn planned_matches_manifest(
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

fn index_stored_file(
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

fn normalize_sha256(raw: &str) -> Result<String> {
    if raw.len() != 64 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::InvalidArg(format!(
            "invalid SHA-256 digest in backup manifest: {raw:?}"
        )));
    }
    Ok(raw.to_ascii_lowercase())
}

fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
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

fn classify_path(path: &Path) -> Result<PathClass> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Ok(PathClass::Symlink),
        Ok(meta) if meta.is_file() => Ok(PathClass::RegularFile),
        Ok(meta) if meta.is_dir() => Ok(PathClass::Directory),
        Ok(_) => Ok(PathClass::Other),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(PathClass::Missing),
        Err(e) => Err(AppError::from(e)),
    }
}

fn ensure_regular_file(path: &Path) -> Result<()> {
    match classify_path(path)? {
        PathClass::RegularFile => Ok(()),
        PathClass::Missing => Err(AppError::NotFound(format!(
            "expected regular file missing: {}",
            path.display()
        ))),
        PathClass::Symlink => Err(AppError::InvalidArg(format!(
            "refusing symlink where regular file expected: {}",
            path.display()
        ))),
        PathClass::Directory => Err(AppError::InvalidArg(format!(
            "refusing directory where regular file expected: {}",
            path.display()
        ))),
        PathClass::Other => Err(AppError::InvalidArg(format!(
            "unsupported file type: {}",
            path.display()
        ))),
    }
}

fn write_manifest(snapshot_dir: &Path, manifest: &BackupManifest) -> Result<()> {
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

fn read_manifest(snapshot_dir: &Path) -> Result<Option<BackupManifest>> {
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

fn allowed_path_map(ordered_live: &[PathBuf]) -> Result<HashMap<String, PathBuf>> {
    let mut allowed = HashMap::new();
    for p in ordered_live {
        if !is_safe_path(p) {
            return Err(AppError::InvalidArg(format!(
                "unsafe live backup path from adapter: {}",
                p.display()
            )));
        }
        allowed.insert(p.display().to_string(), p.clone());
        allowed.insert(path_key(p), p.clone());
    }
    Ok(allowed)
}

fn path_key(path: &Path) -> String {
    normalize_components(path)
        .into_iter()
        .map(|c| c.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\0")
}

fn resolve_allowed_dest(source: &str, allowed: &HashMap<String, PathBuf>) -> Option<PathBuf> {
    if let Some(p) = allowed.get(source) {
        return Some(p.clone());
    }
    let as_path = PathBuf::from(source);
    allowed.get(&path_key(&as_path)).cloned()
}

fn build_restore_plan(
    record: &BackupRecord,
    snapshot_dir: &Path,
    ordered_live: &[PathBuf],
    allowed: &HashMap<String, PathBuf>,
) -> Result<Vec<RestoreItem>> {
    let mut stored_ok: HashSet<String> = HashSet::new();
    for name in &record.files {
        let base = sanitize_basename(name)?;
        if base != *name {
            return Err(AppError::InvalidArg(format!(
                "backup file entry must be a plain basename: {name:?}"
            )));
        }
        if base == MANIFEST_FILE {
            return Err(AppError::InvalidArg(format!(
                "reserved name cannot be a backup payload: {base}"
            )));
        }
        let full = snapshot_dir.join(&base);
        if !is_path_inside(&full, snapshot_dir) {
            return Err(AppError::InvalidArg(format!(
                "backup file escapes snapshot dir: {name}"
            )));
        }
        ensure_regular_file(&full)?;
        if !stored_ok.insert(base.clone()) {
            return Err(AppError::InvalidArg(format!(
                "duplicate backup file entry: {base}"
            )));
        }
    }

    if let Some(manifest) = read_manifest(snapshot_dir)? {
        plan_from_manifest(manifest, snapshot_dir, ordered_live, allowed, &stored_ok)
    } else {
        // Backward-compatible mapping for pre-manifest snapshots: replay the
        // same allocate_dest_name order against current adapter live paths.
        plan_from_legacy(snapshot_dir, ordered_live, &stored_ok)
    }
}

fn plan_from_manifest(
    manifest: BackupManifest,
    snapshot_dir: &Path,
    ordered_live: &[PathBuf],
    allowed: &HashMap<String, PathBuf>,
    stored_ok: &HashSet<String>,
) -> Result<Vec<RestoreItem>> {
    let mut plan = Vec::new();
    let mut seen_stored = HashSet::new();
    let mut seen_dest = HashSet::new();
    // dest path_key -> stored basename, for swap/order checks
    let mut dest_to_stored: HashMap<String, String> = HashMap::new();

    for entry in manifest.entries {
        let stored = sanitize_basename(&entry.stored)?;
        if stored != entry.stored {
            return Err(AppError::InvalidArg(format!(
                "manifest stored name must be a plain basename: {:?}",
                entry.stored
            )));
        }
        if stored == MANIFEST_FILE {
            return Err(AppError::InvalidArg(
                "reserved manifest.json cannot appear as a manifest payload entry".into(),
            ));
        }
        // Extra entries (not in DB index) are always rejected.
        if !stored_ok.contains(&stored) {
            return Err(AppError::InvalidArg(format!(
                "manifest entry not listed in backup index: {stored}"
            )));
        }
        if !seen_stored.insert(stored.clone()) {
            return Err(AppError::InvalidArg(format!(
                "duplicate manifest stored name: {stored}"
            )));
        }

        // Destination must be on the adapter allow-list — never trust raw source
        // as an arbitrary write target.
        let dest = resolve_allowed_dest(&entry.source, allowed).ok_or_else(|| {
            AppError::InvalidArg(format!(
                "manifest source is not in adapter live_backup_paths: {}",
                entry.source
            ))
        })?;
        if !is_safe_path(&dest) {
            return Err(AppError::InvalidArg(format!(
                "unsafe restore destination: {}",
                dest.display()
            )));
        }
        validate_dest_type(&dest)?;
        let dest_key = path_key(&dest);
        if !seen_dest.insert(dest_key.clone()) {
            return Err(AppError::InvalidArg(format!(
                "multiple manifest entries target the same live path: {}",
                dest.display()
            )));
        }

        let stored_path = snapshot_dir.join(&stored);
        ensure_regular_file(&stored_path)?;
        let expected_sha256 = match entry.sha256.as_deref() {
            Some(raw) => {
                let expected = normalize_sha256(raw)?;
                verify_sha256(&stored_path, &expected)?;
                Some(expected)
            }
            None => None,
        };
        dest_to_stored.insert(dest_key, stored.clone());
        plan.push(RestoreItem {
            stored_path,
            dest,
            expected_sha256,
        });
    }

    if seen_stored != *stored_ok {
        return Err(AppError::InvalidArg(
            "backup manifest does not map every indexed file exactly once".into(),
        ));
    }

    // Reject swapped mappings among allow-listed destinations: replaying
    // allocate_dest_name over adapter order for destinations present in the
    // plan must reproduce the stored basenames.
    let mut occupied = HashSet::new();
    for live in ordered_live {
        let key = path_key(live);
        if !dest_to_stored.contains_key(&key) {
            continue;
        }
        let expected_stored = allocate_dest_name(live, &mut occupied)?;
        let actual_stored = dest_to_stored.get(&key).ok_or_else(|| {
            AppError::InvalidArg(format!(
                "manifest mapping missing for adapter path: {}",
                live.display()
            ))
        })?;
        if actual_stored != &expected_stored {
            return Err(AppError::InvalidArg(format!(
                "manifest stored/source mapping inconsistent with adapter order: \
                 {} expected stored {expected_stored}, got {actual_stored}",
                live.display()
            )));
        }
    }

    Ok(plan)
}

fn plan_from_legacy(
    snapshot_dir: &Path,
    ordered_live: &[PathBuf],
    stored_ok: &HashSet<String>,
) -> Result<Vec<RestoreItem>> {
    if stored_ok.contains(MANIFEST_FILE) {
        return Err(AppError::InvalidArg(format!(
            "reserved name cannot be a backup payload: {MANIFEST_FILE}"
        )));
    }

    // Full allocate_dest_name replay over current adapter paths (as if all exist).
    let mut occupied = HashSet::new();
    // original basename -> allocated (stored_name, dest) in adapter order
    let mut groups: HashMap<String, Vec<(String, PathBuf)>> = HashMap::new();
    let mut expected: HashMap<String, PathBuf> = HashMap::new();

    for live in ordered_live {
        if !is_safe_path(live) {
            return Err(AppError::InvalidArg(format!(
                "unsafe live backup path from adapter: {}",
                live.display()
            )));
        }
        let raw = live.file_name().and_then(|s| s.to_str()).ok_or_else(|| {
            AppError::InvalidArg(format!(
                "live backup path has no valid file name: {}",
                live.display()
            ))
        })?;
        let base = sanitize_basename(raw)?;
        let stored_name = allocate_dest_name(live, &mut occupied)?;
        if expected.insert(stored_name.clone(), live.clone()).is_some() {
            return Err(AppError::InvalidArg(format!(
                "duplicate allocated backup name: {stored_name}"
            )));
        }
        groups
            .entry(base)
            .or_default()
            .push((stored_name, live.clone()));
    }

    // Pre-manifest backups did not persist the source mapping. Duplicate live
    // basenames therefore cannot be restored safely: adapter order or the set
    // of files may have changed since the snapshot was created.
    for (base, members) in &groups {
        let present = members
            .iter()
            .filter(|(name, _)| stored_ok.contains(name))
            .count();
        if present > 0 && members.len() > 1 {
            return Err(AppError::InvalidArg(format!(
                "legacy backup has ambiguous collision group for basename {base:?}: \
                 indexed {present} of {} collision members",
                members.len()
            )));
        }
    }

    let mut plan = Vec::new();
    let mut seen_dest = HashSet::new();
    for stored in stored_ok {
        let live = expected.get(stored).ok_or_else(|| {
            AppError::InvalidArg(format!(
                "legacy backup file cannot be mapped to an adapter live path: {stored}"
            ))
        })?;
        if !seen_dest.insert(path_key(live)) {
            return Err(AppError::InvalidArg(format!(
                "legacy backup maps multiple files to the same live path: {}",
                live.display()
            )));
        }
        let stored_path = snapshot_dir.join(stored);
        if !is_path_inside(&stored_path, snapshot_dir) {
            return Err(AppError::InvalidArg(format!(
                "backup file escapes snapshot dir: {stored}"
            )));
        }
        ensure_regular_file(&stored_path)?;
        validate_dest_type(live)?;
        plan.push(RestoreItem {
            stored_path,
            dest: live.clone(),
            expected_sha256: None,
        });
    }
    plan.sort_by(|a, b| a.stored_path.cmp(&b.stored_path));
    Ok(plan)
}

fn validate_dest_type(dest: &Path) -> Result<()> {
    match classify_path(dest)? {
        PathClass::Missing | PathClass::RegularFile => Ok(()),
        PathClass::Directory => Err(AppError::InvalidArg(format!(
            "restore destination is a directory: {}",
            dest.display()
        ))),
        PathClass::Symlink => Err(AppError::InvalidArg(format!(
            "restore destination is a symlink: {}",
            dest.display()
        ))),
        PathClass::Other => Err(AppError::InvalidArg(format!(
            "restore destination has unsupported type: {}",
            dest.display()
        ))),
    }
}

/// Staged/atomic-ish replace for each plan item; rolls back prior items on error.
/// On rollback failure returns `backup.rollback` carrying both failures.
fn apply_restore_plan(plan: &[RestoreItem]) -> Result<()> {
    let mut applied: Vec<AppliedOp> = Vec::new();
    let staging_root = tempfile::tempdir().map_err(AppError::from)?;

    for (idx, item) in plan.iter().enumerate() {
        match stage_replace_one(item, staging_root.path(), idx) {
            Ok(op) => applied.push(op),
            Err(apply_err) => {
                return match rollback_ops(&applied) {
                    Ok(()) => Err(apply_err),
                    Err(rollback_err) => Err(AppError::message(
                        "backup.rollback",
                        format!(
                            "restore apply failed ({apply_err}); rollback also failed ({rollback_err})"
                        ),
                    )),
                };
            }
        }
    }
    // Staging dir drop cleans residual old-file copies.
    Ok(())
}

fn stage_replace_one(item: &RestoreItem, staging_root: &Path, idx: usize) -> Result<AppliedOp> {
    ensure_regular_file(&item.stored_path)?;
    if let Some(expected) = item.expected_sha256.as_deref() {
        verify_sha256(&item.stored_path, expected)?;
    }
    validate_dest_type(&item.dest)?;

    if let Some(parent) = item.dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let staged = staging_root.join(format!("new-{idx}"));
    std::fs::copy(&item.stored_path, &staged)?;
    if let Some(expected) = item.expected_sha256.as_deref() {
        verify_sha256(&staged, expected)?;
    }

    match classify_path(&item.dest)? {
        PathClass::Missing => {
            replace_file(&staged, &item.dest)?;
            Ok(AppliedOp::Created {
                dest: item.dest.clone(),
            })
        }
        PathClass::RegularFile => {
            let old_backup = staging_root.join(format!("old-{idx}"));
            std::fs::copy(&item.dest, &old_backup)?;
            // replace_file preserves an existing destination on commit failure
            // (moves dest aside and renames it back if install fails), so a
            // second recovery attempt here is redundant.
            replace_file(&staged, &item.dest)?;
            Ok(AppliedOp::Replaced {
                dest: item.dest.clone(),
                backup_of_old: old_backup,
            })
        }
        other => Err(AppError::InvalidArg(format!(
            "restore destination became unsafe ({other:?}): {}",
            item.dest.display()
        ))),
    }
}

/// Replace `dest` with the contents at `staged` using same-dir temp + rename.
///
/// Never deletes `dest` before the new file is installed. On platforms where
/// rename-over-existing fails (Windows), the existing regular file is renamed
/// aside first; if installing the new file then fails, the old file is renamed
/// back. Symlink / directory / special destinations are rejected.
fn replace_file(staged: &Path, dest: &Path) -> Result<()> {
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    let tmp_name = format!(".agenthub-restore-{}.tmp", Uuid::new_v4().simple());
    let tmp = parent.join(&tmp_name);
    std::fs::copy(staged, &tmp)?;

    match std::fs::rename(&tmp, dest) {
        Ok(()) => Ok(()),
        Err(first_err) => match classify_path(dest)? {
            PathClass::Missing => {
                let _ = std::fs::remove_file(&tmp);
                Err(AppError::from(first_err))
            }
            PathClass::RegularFile => {
                // Move dest aside, then install new. Never remove_file(dest).
                let old_name = format!(".agenthub-restore-old-{}.tmp", Uuid::new_v4().simple());
                let old_tmp = parent.join(&old_name);

                if let Err(e) = std::fs::rename(dest, &old_tmp) {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(AppError::from(e));
                }

                match std::fs::rename(&tmp, dest) {
                    Ok(()) => {
                        // Best-effort cleanup of the aside copy.
                        let _ = std::fs::remove_file(&old_tmp);
                        Ok(())
                    }
                    Err(install_err) => {
                        // Put the previous contents back under dest.
                        match std::fs::rename(&old_tmp, dest) {
                            Ok(()) => {
                                let _ = std::fs::remove_file(&tmp);
                                Err(AppError::from(install_err))
                            }
                            Err(recover_err) => {
                                // Leave old_tmp in place so bytes are not lost;
                                // clean residual staged temp best-effort.
                                let _ = std::fs::remove_file(&tmp);
                                Err(AppError::message(
                                    "backup.rollback",
                                    format!(
                                        "replace install failed ({install_err}); \
                                         also failed to restore previous file ({recover_err})"
                                    ),
                                ))
                            }
                        }
                    }
                }
            }
            PathClass::Symlink => {
                let _ = std::fs::remove_file(&tmp);
                Err(AppError::InvalidArg(format!(
                    "replace destination is a symlink: {}",
                    dest.display()
                )))
            }
            PathClass::Directory => {
                let _ = std::fs::remove_file(&tmp);
                Err(AppError::InvalidArg(format!(
                    "replace destination is a directory: {}",
                    dest.display()
                )))
            }
            PathClass::Other => {
                let _ = std::fs::remove_file(&tmp);
                Err(AppError::InvalidArg(format!(
                    "replace destination has unsupported type: {}",
                    dest.display()
                )))
            }
        },
    }
}

fn rollback_ops(applied: &[AppliedOp]) -> Result<()> {
    let mut failures = Vec::new();
    for op in applied.iter().rev() {
        let result = match op {
            AppliedOp::Created { dest } => match classify_path(dest)? {
                // Already gone — nothing to undo.
                PathClass::Missing => Ok(()),
                PathClass::RegularFile => std::fs::remove_file(dest).map_err(AppError::from),
                other => Err(AppError::InvalidArg(format!(
                    "rollback cannot safely undo created path ({other:?}): {}",
                    dest.display()
                ))),
            },
            AppliedOp::Replaced {
                dest,
                backup_of_old,
            } => {
                ensure_regular_file(backup_of_old).and_then(|()| replace_file(backup_of_old, dest))
            }
        };
        if let Err(error) = result {
            failures.push(error.to_string());
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(AppError::message("backup.rollback", failures.join("; ")))
    }
}

/// Canonical strict containment for existing paths. This closes
/// ancestor-symlink escapes that lexical component checks cannot detect and
/// prevents callers from ever treating `root` itself as a snapshot.
fn ensure_existing_path_strictly_inside(
    path: &Path,
    root: &Path,
    code: &'static str,
) -> Result<()> {
    let canonical_root = std::fs::canonicalize(root)?;
    let canonical_path = std::fs::canonicalize(path)?;
    if !is_path_strictly_inside(&canonical_path, &canonical_root) {
        return Err(AppError::message(
            code,
            format!("path resolves outside backups_root: {}", path.display()),
        ));
    }
    Ok(())
}

/// Lexical containment: `child` equals `root` or is a strict descendant.
/// Does not follow symlinks; used as a safety guard before write/delete.
fn is_path_inside(child: &Path, root: &Path) -> bool {
    let child_c = normalize_components(child);
    let root_c = normalize_components(root);
    if root_c.is_empty() {
        return false;
    }
    child_c.starts_with(&root_c)
}

/// Exact lexical identity. This deliberately does not canonicalize or
/// normalize `.`/`..`: an indexed path must be the path AgentHub generated.
fn paths_equal_exact(left: &Path, right: &Path) -> bool {
    left.as_os_str() == right.as_os_str()
}

fn is_path_strictly_inside(child: &Path, root: &Path) -> bool {
    let child_c = normalize_components(child);
    let root_c = normalize_components(root);
    !root_c.is_empty() && child_c.len() > root_c.len() && child_c.starts_with(&root_c)
}

/// Reject symlinks in every existing component from `root` to `path`.
fn ensure_path_components_not_symlinks(path: &Path, root: &Path) -> Result<()> {
    let relative = path.strip_prefix(root).map_err(|_| {
        AppError::InvalidArg(format!(
            "backup path is outside backups_root: {}",
            path.display()
        ))
    })?;

    let mut current = root.to_path_buf();
    for component in std::iter::once(None).chain(relative.components().map(Some)) {
        if let Some(component) = component {
            current.push(component.as_os_str());
        }
        let metadata = std::fs::symlink_metadata(&current)?;
        if metadata.file_type().is_symlink() {
            return Err(AppError::InvalidArg(format!(
                "backup path contains a symlink component: {}",
                current.display()
            )));
        }
    }
    Ok(())
}

/// Refuse snapshots containing symlinks or special filesystem entries before
/// recursive restore/delete operations can traverse them.
fn ensure_tree_has_no_symlinks(root: &Path) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(AppError::InvalidArg(format!(
                    "backup snapshot contains a symlink: {}",
                    path.display()
                )));
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if !metadata.is_file() {
                return Err(AppError::InvalidArg(format!(
                    "backup snapshot contains an unsupported filesystem entry: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn normalize_components(path: &Path) -> Vec<std::ffi::OsString> {
    use std::path::Component;
    let mut out = Vec::new();
    for c in path.components() {
        match c {
            Component::Prefix(p) => out.push(p.as_os_str().to_os_string()),
            Component::RootDir => out.push(std::ffi::OsString::from("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(s) => out.push(s.to_os_string()),
        }
    }
    out
}

/// Build a collision-safe destination basename from a source path.
/// Rejects empty / traversal / unsafe names.
fn allocate_dest_name(src: &Path, occupied: &mut HashSet<String>) -> Result<String> {
    let raw = src.file_name().and_then(|s| s.to_str()).ok_or_else(|| {
        AppError::InvalidArg(format!(
            "backup source has no valid file name: {}",
            src.display()
        ))
    })?;

    let base = sanitize_basename(raw)?;
    if occupied.insert(base.clone()) {
        return Ok(base);
    }

    // Collision: settings.json → settings__2.json, settings__3.json, ...
    let (stem, ext) = split_stem_ext(&base);
    for n in 2u32.. {
        let candidate = if ext.is_empty() {
            format!("{stem}__{n}")
        } else {
            format!("{stem}__{n}.{ext}")
        };
        if !is_safe_path(Path::new(&candidate)) {
            continue;
        }
        if occupied.insert(candidate.clone()) {
            return Ok(candidate);
        }
    }
    Err(AppError::message(
        "backup.name",
        "could not allocate unique backup file name",
    ))
}

fn sanitize_basename(raw: &str) -> Result<String> {
    if raw.is_empty() || raw == "." || raw == ".." {
        return Err(AppError::InvalidArg(format!(
            "invalid backup file name: {raw:?}"
        )));
    }
    if raw.contains('/') || raw.contains('\\') || raw.contains('\0') {
        return Err(AppError::InvalidArg(format!(
            "backup file name must not contain path separators: {raw:?}"
        )));
    }
    if !is_safe_path(Path::new(raw)) {
        return Err(AppError::InvalidArg(format!(
            "unsafe backup file name: {raw:?}"
        )));
    }
    Ok(raw.to_string())
}

fn split_stem_ext(name: &str) -> (&str, &str) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() && !stem.contains('.') => {
            (stem, ext)
        }
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => (stem, ext),
        _ => (name, ""),
    }
}

#[cfg(test)]
mod tests;
