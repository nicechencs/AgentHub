//! Restore / delete transactions over indexed snapshots.
//!
//! Restore always takes a PreRestore snapshot (via `snapshot_with_guard`) before
//! overwriting live files. Partial live writes roll back; `backup.rollback`
//! then retries from that PreRestore. Delete does not take the live-write lock.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::models::{AgentId, BackupKind, BackupRecord};
use crate::services::LiveWriteGuard;
use crate::utils::paths::is_safe_path;

use super::path_safety::{
    allocate_dest_name, classify_path, ensure_regular_file, is_path_inside, normalize_components,
    sanitize_basename, PathClass,
};
use super::snapshot::{
    normalize_sha256, read_manifest, verify_sha256, BackupManifest, MANIFEST_FILE,
};
use super::{elapsed_ms, BackupService, RestoreResult};

#[derive(Debug, Clone)]
pub struct RestoreItem {
    pub stored_path: PathBuf,
    pub dest: PathBuf,
    /// Hash recorded by a v1 manifest. Legacy manifests omit this field.
    pub expected_sha256: Option<String>,
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

impl BackupService {
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
pub fn apply_restore_plan(plan: &[RestoreItem]) -> Result<()> {
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
