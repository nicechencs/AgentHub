//! Skill package validation and atomic placement under a skills root.
//!
//! Owns staging → validate → write tree → backup/rename swap. Used by install,
//! update, sync, and copy-mode projection.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{AppError, Result};
use crate::logging::{self, targets};

use super::fs_safe::{
    collect_regular_files, ensure_no_symlink_in_existing_prefix, inspect_projection_target,
    is_exact_child, is_link_or_reparse, is_path_inside, paths_equal_lexical,
    validate_safe_path_component, validate_skills_root, validate_tree_entries_safe, TargetPresence,
};

/// Owns package tree validation and atomic materialization into a skill root.
#[derive(Debug, Default, Clone, Copy)]
pub struct SkillPackageService;

impl SkillPackageService {
    pub fn new() -> Self {
        Self
    }

    pub fn validate_and_collect(
        &self,
        source: &Path,
        skill_id: &str,
    ) -> Result<BTreeMap<String, Vec<u8>>> {
        validate_and_collect_source(source, skill_id)
    }

    /// Atomically place collected files at `target` under `skills_root`.
    pub fn place(
        &self,
        skills_root: &Path,
        skill_id: &str,
        target: &Path,
        files: &BTreeMap<String, Vec<u8>>,
        existing_target: Option<&Path>,
    ) -> Result<()> {
        materialize_projection(skills_root, skill_id, target, files, existing_target)
    }
}

pub(crate) fn validate_and_collect_source(source: &Path, skill_id: &str) -> Result<BTreeMap<String, Vec<u8>>> {
    let meta = match fs::symlink_metadata(source) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(AppError::NotFound(format!(
                "skill source not found: {skill_id}"
            )));
        }
        Err(e) => return Err(AppError::from(e)),
    };
    if is_link_or_reparse(&meta) {
        return Err(AppError::InvalidArg(format!(
            "skill source must not be a symlink or reparse point: {}",
            source.display()
        )));
    }
    if !meta.is_dir() {
        return Err(AppError::InvalidArg(format!(
            "skill source is not a directory: {}",
            source.display()
        )));
    }

    validate_tree_entries_safe(source, "skill source")?;

    collect_regular_files(source).map_err(|()| {
        AppError::InvalidArg(format!(
            "skill source tree contains symlink or special/non-regular entry: {}",
            source.display()
        ))
    })
}

/// Build staging under `skills_root`, write files, then place at `target`.
///
/// When `existing_target` is `Some`, the old target is moved aside only after
/// staging is fully built, then staging is renamed into place. On failure,
/// staging is cleaned and the old target is restored best-effort.
pub(crate) fn materialize_projection(
    skills_root: &Path,
    skill_id: &str,
    target: &Path,
    files: &BTreeMap<String, Vec<u8>>,
    existing_target: Option<&Path>,
) -> Result<()> {
    // Ensure skills_root exists as a real directory (ancestors already validated).
    if !skills_root.exists() {
        ensure_no_symlink_in_existing_prefix(skills_root)?;
        fs::create_dir_all(skills_root)?;
    }
    validate_skills_root(skills_root)?;

    let staging = create_staging_dir(skills_root, skill_id)?;
    if let Err(e) = write_skill_tree(&staging, files) {
        best_effort_remove_dir(&staging, skills_root);
        return Err(e);
    }

    // Re-validate the adapter-owned root after staging: another process must not
    // be able to swap it for a link while a potentially long source copy runs.
    if let Err(e) = validate_skills_root(skills_root) {
        best_effort_remove_dir(&staging, skills_root);
        return Err(e);
    }

    // Staging is complete. Install into the exact target path.
    if let Some(old) = existing_target {
        replace_target_with_staging(skills_root, skill_id, old, target, &staging)
    } else {
        // `rename` is not a portable no-replace primitive for directories. A
        // last-moment check keeps an observed conflict intact on every platform
        // (the OS-level race that remains requires a per-agent process lock).
        let target_state = match inspect_projection_target(target) {
            Ok(state) => state,
            Err(e) => {
                best_effort_remove_dir(&staging, skills_root);
                return Err(e);
            }
        };
        if !matches!(target_state, TargetPresence::Missing) {
            best_effort_remove_dir(&staging, skills_root);
            return Err(AppError::message(
                "skill.conflict",
                format!(
                    "skill target appeared while syncing; refusing to replace it: {}",
                    target.display()
                ),
            ));
        }
        match fs::rename(&staging, target) {
            Ok(()) => Ok(()),
            Err(e) => {
                best_effort_remove_dir(&staging, skills_root);
                Err(AppError::from(e))
            }
        }
    }
}

pub(crate) fn write_skill_tree(dest_root: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    for (rel, bytes) in files {
        let dest = join_normalized_rel(dest_root, rel)?;
        if !is_path_inside(&dest, dest_root) {
            return Err(AppError::InvalidArg(format!(
                "skill file path escapes destination: {rel}"
            )));
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, bytes)?;
    }
    Ok(())
}

/// Join a portable `a/b/c` key under `root`, rejecting traversal / unsafe segments.
pub(crate) fn join_normalized_rel(root: &Path, rel: &str) -> Result<PathBuf> {
    let mut out = root.to_path_buf();
    if rel.is_empty() {
        return Err(AppError::InvalidArg(
            "skill relative path must not be empty".into(),
        ));
    }
    for part in rel.split('/') {
        validate_safe_path_component(part).map_err(|_| {
            AppError::InvalidArg(format!("unsafe skill relative path segment in {rel:?}"))
        })?;
        out.push(part);
    }
    Ok(out)
}

pub(crate) fn create_staging_dir(skills_root: &Path, skill_id: &str) -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for i in 0u32..1024 {
        let name = format!(".agenthub-stage-{skill_id}-{nanos}-{i}");
        // Name is a single component we control; still verify containment.
        let path = skills_root.join(&name);
        if !is_path_inside(&path, skills_root) {
            return Err(AppError::InvalidArg(format!(
                "staging path escapes skills root: {}",
                path.display()
            )));
        }
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(AppError::from(e)),
        }
    }
    Err(AppError::message(
        "skill.staging",
        "could not allocate unique skill staging directory",
    ))
}

pub(crate) fn allocate_backup_path(skills_root: &Path, skill_id: &str) -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for i in 0u32..1024 {
        let name = format!(".agenthub-bak-{skill_id}-{nanos}-{i}");
        let path = skills_root.join(&name);
        if !is_path_inside(&path, skills_root) {
            return Err(AppError::InvalidArg(format!(
                "backup path escapes skills root: {}",
                path.display()
            )));
        }
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(path),
            Ok(_) => continue,
            // An unreadable candidate is not evidence that the name is free.
            Err(e) => return Err(AppError::from(e)),
        }
    }
    Err(AppError::message(
        "skill.backup",
        "could not allocate unique skill backup path",
    ))
}

/// Result of placing staging at live while retaining the old tree for later
/// finalize/rollback (install/update main commit).
#[derive(Debug)]
pub(crate) struct RetainedLiveSwap {
    /// Old live tree moved aside; `None` on first install.
    pub backup: Option<PathBuf>,
    /// True when no prior live directory existed.
    pub first_install: bool,
}

/// Move `existing` aside, place `staging` at `target`, then drop the backup.
///
/// On failure after moving the old target: restore best-effort, clean staging.
pub(crate) fn replace_target_with_staging(
    skills_root: &Path,
    skill_id: &str,
    existing: &Path,
    target: &Path,
    staging: &Path,
) -> Result<()> {
    let swap = swap_staging_keep_backup(skills_root, skill_id, Some(existing), target, staging)?;
    finalize_retained_backup(skills_root, swap);
    Ok(())
}

/// Place validated `staging` at `target`, keeping any previous live as backup.
///
/// Caller must later [`finalize_retained_backup`] (success) or
/// [`rollback_retained_swap`] (failure). Staging is consumed on success; on
/// pre-rename failure staging is cleaned when it uses helper naming.
pub(crate) fn swap_staging_keep_backup(
    skills_root: &Path,
    skill_id: &str,
    existing: Option<&Path>,
    target: &Path,
    staging: &Path,
) -> Result<RetainedLiveSwap> {
    if let Err(e) = validate_skills_root(skills_root) {
        best_effort_remove_dir(staging, skills_root);
        return Err(e);
    }
    if !is_exact_child(target, skills_root, skill_id) {
        best_effort_remove_dir(staging, skills_root);
        return Err(AppError::InvalidArg(format!(
            "replacement target is not the exact skill projection: {}",
            target.display()
        )));
    }

    match existing {
        None => {
            let target_state = match inspect_projection_target(target) {
                Ok(state) => state,
                Err(e) => {
                    best_effort_remove_dir(staging, skills_root);
                    return Err(e);
                }
            };
            if !matches!(target_state, TargetPresence::Missing) {
                best_effort_remove_dir(staging, skills_root);
                return Err(AppError::message(
                    "skill.conflict",
                    format!(
                        "skill target appeared while installing; refusing to replace it: {}",
                        target.display()
                    ),
                ));
            }
            match fs::rename(staging, target) {
                Ok(()) => Ok(RetainedLiveSwap {
                    backup: None,
                    first_install: true,
                }),
                Err(e) => {
                    best_effort_remove_dir(staging, skills_root);
                    Err(AppError::from(e))
                }
            }
        }
        Some(existing) => {
            if !paths_equal_lexical(existing, target) {
                best_effort_remove_dir(staging, skills_root);
                return Err(AppError::InvalidArg(format!(
                    "replacement target is not the exact skill projection: {}",
                    target.display()
                )));
            }
            let target_state = match inspect_projection_target(existing) {
                Ok(state) => state,
                Err(e) => {
                    best_effort_remove_dir(staging, skills_root);
                    return Err(e);
                }
            };
            match target_state {
                TargetPresence::Directory => {
                    if let Err(e) = validate_tree_entries_safe(existing, "skill target") {
                        best_effort_remove_dir(staging, skills_root);
                        return Err(e);
                    }
                }
                TargetPresence::Link { kind } => {
                    // Shared source + projection replace helpers never delete a
                    // link and pretend first install. Callers that own link
                    // removal (clear_managed / unproject) must do it first.
                    best_effort_remove_dir(staging, skills_root);
                    return Err(AppError::message(
                        "skill.conflict",
                        format!(
                            "refusing to replace skill target that is a {} (symlink/junction/reparse): {}",
                            kind.as_str(),
                            target.display()
                        ),
                    ));
                }
                TargetPresence::Missing => {
                    best_effort_remove_dir(staging, skills_root);
                    return Err(AppError::message(
                        "skill.conflict",
                        format!(
                            "skill target disappeared while syncing; refusing replacement: {}",
                            target.display()
                        ),
                    ));
                }
                TargetPresence::Dangerous { kind } => {
                    best_effort_remove_dir(staging, skills_root);
                    return Err(AppError::InvalidArg(format!(
                        "skill target changed to unsafe {kind} while syncing: {}",
                        target.display()
                    )));
                }
            }

            let backup = match allocate_backup_path(skills_root, skill_id) {
                Ok(path) => path,
                Err(e) => {
                    best_effort_remove_dir(staging, skills_root);
                    return Err(e);
                }
            };

            if let Err(e) = fs::rename(existing, &backup) {
                best_effort_remove_dir(staging, skills_root);
                return Err(AppError::from(e));
            }

            if let Err(e) = fs::rename(staging, target) {
                // Restore old target; clean leftover staging if still present.
                // If restore fails, keep backup path in the error — never delete
                // the only remaining old copy.
                match fs::rename(&backup, target) {
                    Ok(()) => {
                        best_effort_remove_dir(staging, skills_root);
                        Err(AppError::message(
                            "skill.swap",
                            format!("failed to place staged skill at {}: {e}", target.display()),
                        ))
                    }
                    Err(re) => {
                        best_effort_remove_dir(staging, skills_root);
                        Err(AppError::message(
                            "skill.swap",
                            format!(
                                "failed to place staged skill at {}: {e}; restore backup also failed: {re}; backup retained at {}",
                                target.display(),
                                backup.display()
                            ),
                        ))
                    }
                }
            } else {
                Ok(RetainedLiveSwap {
                    backup: Some(backup),
                    first_install: false,
                })
            }
        }
    }
}

/// Drop retained backup after lock + package metadata both committed.
///
/// Cleanup failure does **not** roll back committed metadata; it only warns so
/// operators can remove the leftover helper directory.
pub(crate) fn finalize_retained_backup(skills_root: &Path, swap: RetainedLiveSwap) {
    if let Some(backup) = swap.backup {
        if !try_remove_helper_dir(&backup, skills_root) {
            logging::log_warn(
                targets::SKILL,
                "finalize_backup",
                &format!(
                    "skill package metadata committed but backup cleanup failed; \
                     backup retained for manual cleanup: backup={} skills_root={}",
                    backup.display(),
                    skills_root.display()
                ),
            );
        }
    }
}

/// Restore live from retained backup (or remove first-install live).
///
/// Rollback step failures are returned (not swallowed with `let _`).
pub(crate) fn rollback_retained_swap(
    skills_root: &Path,
    target: &Path,
    swap: RetainedLiveSwap,
) -> Result<()> {
    let RetainedLiveSwap {
        backup,
        first_install,
    } = swap;

    if first_install {
        // First install: remove the new live tree if present.
        if target.exists() {
            let meta = fs::symlink_metadata(target)?;
            if is_link_or_reparse(&meta) {
                return Err(AppError::InvalidArg(format!(
                    "refusing to roll back link at skill source: {}",
                    target.display()
                )));
            }
            if meta.is_dir() {
                validate_tree_entries_safe(target, "skill source")?;
                fs::remove_dir_all(target)?;
            } else {
                fs::remove_file(target)?;
            }
        }
        // Unexpected leftover backup on first install — still try to drop it.
        if let Some(backup) = backup {
            best_effort_remove_dir(&backup, skills_root);
        }
        return Ok(());
    }

    let Some(backup) = backup else {
        // Overwrite path without backup should not happen; leave target as-is.
        return Err(AppError::message(
            "skill.commit",
            format!(
                "cannot roll back skill overwrite: no backup retained for {}",
                target.display()
            ),
        ));
    };

    // Remove failed new live if present, then restore backup.
    if target.exists() {
        let meta = fs::symlink_metadata(target)?;
        if is_link_or_reparse(&meta) {
            return Err(AppError::message(
                "skill.commit",
                format!(
                    "cannot restore skill backup; live is a link: {}",
                    target.display()
                ),
            ));
        }
        if meta.is_dir() {
            validate_tree_entries_safe(target, "skill source")?;
            fs::remove_dir_all(target)?;
        } else {
            fs::remove_file(target)?;
        }
    }
    fs::rename(&backup, target).map_err(|e| {
        AppError::message(
            "skill.commit",
            format!(
                "failed to restore skill backup {} → {}: {e}",
                backup.display(),
                target.display()
            ),
        )
    })?;
    Ok(())
}

pub(crate) fn best_effort_remove_dir(path: &Path, root: &Path) {
    let _ = try_remove_helper_dir(path, root);
}

/// Remove a helper dir (staging/backup) under `root`. Returns `true` when the
/// path is gone or was never a removable helper; `false` when removal failed.
fn try_remove_helper_dir(path: &Path, root: &Path) -> bool {
    let Ok(root_meta) = fs::symlink_metadata(root) else {
        return true;
    };
    if is_link_or_reparse(&root_meta) || !root_meta.is_dir() {
        return true;
    }
    let Some(parent) = path.parent() else {
        return true;
    };
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    if !paths_equal_lexical(parent, root)
        || !(name.starts_with(".agenthub-stage-") || name.starts_with(".agenthub-bak-"))
    {
        return true;
    }
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return true,
        Err(_) => return false,
    };
    if is_link_or_reparse(&meta) || !meta.is_dir() {
        return false;
    }
    if validate_tree_entries_safe(path, "skill helper").is_err() {
        return false;
    }
    match fs::remove_dir_all(path) {
        Ok(()) => true,
        Err(e) if e.kind() == io::ErrorKind::NotFound => true,
        Err(_) => false,
    }
}

