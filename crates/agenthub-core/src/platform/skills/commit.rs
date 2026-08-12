//! Narrow install/update main-commit coordinator for shared skill packages.
//!
//! Ordering (after staging is fully built and validated by the caller):
//! 1. Snapshot old lock entry + optional package row
//! 2. live → backup, staging → live (**keep** backup)
//! 3. Atomic write `.skill-lock.json`
//! 4. Upsert `skill_packages` when a repo is provided
//! 5. On any failure: restore live, lock, package; merge compensation errors
//! 6. Only after lock (+ package) succeed: drop backup
//!
//! Projection reconcile is **not** part of this commit — callers run it after.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::logging::{self, targets};
use crate::models::SkillSourceRecord;
use crate::platform::skills::assignment::SkillAssignmentService;
use crate::platform::skills::fs_safe::{
    ensure_no_symlink_in_existing_prefix, validate_skills_root, validate_tree_entries_safe,
};
use crate::platform::skills::lockfile::{
    skill_lock_file, skill_lock_load, skill_lock_replace_map, skill_lock_upsert,
};
use crate::platform::skills::packages::{
    create_staging_dir, finalize_retained_backup, rollback_retained_swap, swap_staging_keep_backup,
    write_skill_tree, RetainedLiveSwap,
};
use crate::storage::{SkillPackageRow, SkillRepo};

/// Prepared skill tree ready to swap into the live source root.
pub(crate) enum PreparedSkillTree<'a> {
    /// In-memory files (local / zip / market re-fetch). Written to staging here.
    Files(&'a BTreeMap<String, Vec<u8>>),
    /// Already-materialized staging directory (e.g. git clone). Consumed by rename.
    StagingDir(PathBuf),
}

/// Test-only fault points for compensation coverage (no generic executor trait).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SkillCommitFaults {
    /// Fail after live swap, before lock write.
    pub fail_before_lock: bool,
    /// Fail after lock write, before package write.
    pub fail_before_package: bool,
}

/// Result of a successful live + lock (+ package) commit.
#[derive(Debug)]
pub(crate) struct SkillPackageCommit {
    pub skill_id: String,
    pub dest: PathBuf,
}

/// Commit a prepared skill into live source, lock, and optional package row.
///
/// `repo == None` → filesystem + lock only (unit tests / FS-only service).
///
/// Pre-swap failures (including snapshot / tree validation) always clean
/// staging. Successful metadata commit finalizes backup without rolling back
/// committed rows if backup cleanup fails.
pub(crate) fn commit_skill_package(
    skills_root: &Path,
    skill_id: &str,
    prepared: PreparedSkillTree<'_>,
    record: SkillSourceRecord,
    repo: Option<&SkillRepo>,
    now: &str,
    faults: SkillCommitFaults,
) -> Result<SkillPackageCommit> {
    if !skills_root.exists() {
        ensure_no_symlink_in_existing_prefix(skills_root)?;
        std::fs::create_dir_all(skills_root)?;
    }
    validate_skills_root(skills_root)?;

    let dest = skills_root.join(skill_id);
    let had_live = dest.exists();

    // Materialize / accept staging first so every later pre-swap failure can clean it
    // (including StagingDir from git prepare, which would otherwise leak on snapshot err).
    let staging = match prepared {
        PreparedSkillTree::Files(files) => {
            let staging = create_staging_dir(skills_root, skill_id)?;
            if let Err(e) = write_skill_tree(&staging, files) {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(e);
            }
            staging
        }
        PreparedSkillTree::StagingDir(path) => path,
    };

    if !staging.join("SKILL.md").is_file() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(AppError::InvalidArg(format!(
            "skill package must contain SKILL.md before live swap: {skill_id}"
        )));
    }

    // Whole-tree safety for both Files and StagingDir paths before any swap.
    if let Err(e) = validate_tree_entries_safe(&staging, "skill staging") {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }

    // --- snapshots (old state boundary); failure cleans staging ---
    let had_lock_file = skill_lock_file(skills_root).is_file();
    let old_lock = match skill_lock_load(skills_root) {
        Ok(m) => m,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
    };
    let old_package = match repo {
        Some(r) => match r.get_package(skill_id) {
            Ok(p) => p,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(e);
            }
        },
        None => None,
    };

    // --- live swap (retain backup) ---
    let existing = if had_live { Some(dest.as_path()) } else { None };
    let swap = match swap_staging_keep_backup(skills_root, skill_id, existing, &dest, &staging) {
        Ok(s) => s,
        Err(e) => {
            // Staging cleaned by swap helper when named helper-style; also try raw path.
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
    };

    // From here: live is new; backup retained until metadata commits.
    let commit_result = finish_metadata_commit(skills_root, skill_id, &record, repo, now, faults);

    match commit_result {
        Ok(()) => {
            // Backup cleanup failure does not roll back committed metadata.
            finalize_retained_backup(skills_root, swap);
            Ok(SkillPackageCommit {
                skill_id: skill_id.to_string(),
                dest,
            })
        }
        Err(primary) => {
            let compensate = compensate_failed_commit(
                skills_root,
                skill_id,
                &dest,
                swap,
                &old_lock,
                had_lock_file,
                repo,
                old_package.as_ref(),
            );
            Err(log_commit_failure(skill_id, primary, compensate))
        }
    }
}

fn finish_metadata_commit(
    skills_root: &Path,
    skill_id: &str,
    record: &SkillSourceRecord,
    repo: Option<&SkillRepo>,
    now: &str,
    faults: SkillCommitFaults,
) -> Result<()> {
    if faults.fail_before_lock {
        return Err(AppError::message(
            "skill.commit",
            "injected failure before lock write",
        ));
    }

    skill_lock_upsert(skills_root, skill_id, record.clone())?;

    if faults.fail_before_package {
        return Err(AppError::message(
            "skill.commit",
            "injected failure after lock write before package row",
        ));
    }

    if let Some(r) = repo {
        let assign = SkillAssignmentService::new(r.clone());
        assign.ensure_package(skill_id, Some(record), now)?;
    }
    Ok(())
}

/// Best-effort reverse of live + lock + package. Runs all three steps and
/// aggregates failures (does not short-circuit after the first error).
fn compensate_failed_commit(
    skills_root: &Path,
    skill_id: &str,
    dest: &Path,
    swap: RetainedLiveSwap,
    old_lock: &BTreeMap<String, SkillSourceRecord>,
    had_lock_file: bool,
    repo: Option<&SkillRepo>,
    old_package: Option<&SkillPackageRow>,
) -> Result<()> {
    let mut errors: Vec<String> = Vec::new();

    // 1) Restore live tree.
    if let Err(e) = rollback_retained_swap(skills_root, dest, swap) {
        errors.push(format!("live restore failed: {e}"));
    }

    // 2) Restore lock. First install with no prior lock file must not leave
    //    an empty `.skill-lock.json` behind.
    if !had_lock_file {
        let path = skill_lock_file(skills_root);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => errors.push(format!("lock cleanup failed: {e}")),
        }
    } else if let Err(e) = skill_lock_replace_map(skills_root, old_lock) {
        errors.push(format!("lock restore failed: {e}"));
    }

    // 3) Restore package row when DB is in play. Always read current first:
    //    - equal to old snapshot → skip upsert (avoids re-firing fail triggers)
    //    - old empty and current empty → skip delete
    if let Some(r) = repo {
        match r.get_package(skill_id) {
            Err(e) => errors.push(format!("package read failed: {e}")),
            Ok(current) => match (old_package, current) {
                (Some(pkg), Some(ref cur)) if cur == pkg => {}
                (Some(pkg), _) => {
                    if let Err(e) = r.upsert_package(pkg) {
                        errors.push(format!("package restore failed: {e}"));
                    }
                }
                (None, None) => {}
                (None, Some(_)) => match r.delete_package(skill_id) {
                    Ok(()) => {}
                    Err(e) if e.code() == "not_found" => {}
                    Err(e) => errors.push(format!("package delete failed: {e}")),
                },
            },
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::message("skill.commit", errors.join("; ")))
    }
}

fn merge_primary_and_compensate(primary: AppError, compensate: Result<()>) -> AppError {
    match compensate {
        Ok(()) => primary,
        Err(rb) => AppError::message(
            "skill.commit",
            format!("{primary}; compensation also failed: {rb}"),
        ),
    }
}

/// Merge primary + compensation errors and emit structured skill logs.
fn log_commit_failure(skill_id: &str, primary: AppError, compensate: Result<()>) -> AppError {
    let dual_failure = compensate.is_err();
    let err = merge_primary_and_compensate(primary, compensate);
    if dual_failure {
        // Dual-failure may leave filesystem/DB drift; highest-severity commit outcome.
        logging::log_app_error(targets::SKILL, "commit_compensate", &err);
        tracing::warn!(
            module = targets::SKILL,
            op = "commit_compensate",
            skill_id = skill_id,
            code = err.code(),
            "skill commit failed and compensation also failed; manual cleanup may be required"
        );
    } else {
        logging::log_app_error(targets::SKILL, "commit", &err);
        tracing::debug!(
            module = targets::SKILL,
            op = "commit",
            skill_id = skill_id,
            code = err.code(),
            "skill commit failed; compensation restored prior state"
        );
    }
    err
}
