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
use crate::platform::skills::journal::{
    clear_journal, load_journal, remove_journal_helper, restore_package, validate_journal_paths,
    write_journal, SkillCommitJournal, SkillCommitPhase, SkillPackageSnapshot,
};
use crate::platform::skills::lockfile::{
    skill_lock_file, skill_lock_load, skill_lock_replace_map, skill_lock_upsert,
};
use crate::platform::skills::packages::{
    allocate_backup_path, create_staging_dir, finalize_retained_backup, rollback_retained_swap,
    rollback_retained_swap_idempotent, swap_staging_keep_backup_at, write_skill_tree,
    RetainedLiveSwap,
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

    // Never overwrite evidence from an interrupted commit.  Startup or the
    // explicit SkillService bootstrap must recover it under the root lock.
    if load_journal(skills_root)?.is_some() {
        return Err(AppError::message(
            "skill.commit_recovery",
            "an interrupted skill commit requires bootstrap recovery",
        ));
    }

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

    // Allocate every helper path before the first rename, then persist the
    // complete old-state snapshot.  A later bootstrap can now recover even if
    // this process disappears between any two metadata writes.
    let backup_path = if had_live {
        match allocate_backup_path(skills_root, skill_id) {
            Ok(path) => Some(path),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(e);
            }
        }
    } else {
        None
    };
    let mut journal = SkillCommitJournal {
        schema: crate::platform::skills::journal::SKILL_COMMIT_JOURNAL_SCHEMA,
        skill: skill_id.to_string(),
        target: dest.clone(),
        had_live,
        staging: staging.clone(),
        backup: backup_path.clone(),
        old_lock: old_lock.clone(),
        had_lock_file,
        old_package: old_package.as_ref().map(SkillPackageSnapshot::from),
        has_package_repo: repo.is_some(),
        phase: SkillCommitPhase::Prepared,
    };
    if let Err(e) = write_journal(skills_root, &journal) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }

    // --- live swap (retain backup) ---
    let existing = if had_live { Some(dest.as_path()) } else { None };
    let swap = match swap_staging_keep_backup_at(
        skills_root,
        skill_id,
        existing,
        &dest,
        &staging,
        backup_path,
    ) {
        Ok(s) => s,
        Err(e) => {
            // A retained backup means the helper crossed the first rename and
            // must remain journaled for bootstrap recovery.  Otherwise the
            // helper guarantees that no live state changed, so the prepared
            // record can be removed.
            let helper_retained = journal.staging.exists()
                || journal.backup.as_ref().is_some_and(|path| path.exists());
            if !helper_retained {
                let _ = clear_journal(skills_root);
            }
            return Err(e);
        }
    };

    journal.phase = SkillCommitPhase::LiveSwapped;
    if let Err(e) = write_journal(skills_root, &journal) {
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
        let compensate = if compensate.is_ok() {
            clear_journal(skills_root)
        } else {
            compensate
        };
        return Err(log_commit_failure(skill_id, e, compensate));
    }

    // From here: live is new; backup retained until metadata commits.
    let commit_result = finish_metadata_commit(
        skills_root,
        skill_id,
        &record,
        repo,
        now,
        faults,
        &mut journal,
    );

    match commit_result {
        Ok(()) => {
            // Backup cleanup failure does not roll back committed metadata,
            // but it must retain the journal with the concrete inspection or
            // removal error so bootstrap can retry without losing the helper.
            if let Err(e) = finalize_retained_backup(skills_root, swap) {
                logging::log_app_error(targets::SKILL, "commit_journal_cleanup", &e);
                tracing::warn!(
                    module = targets::SKILL,
                    op = "commit_journal_cleanup",
                    skill_id = skill_id,
                    code = e.code(),
                    error = %e,
                    "skill commit completed but backup cleanup failed; durable journal retained for bootstrap"
                );
            } else if let Err(e) = clear_journal(skills_root) {
                logging::log_app_error(targets::SKILL, "commit_journal_cleanup", &e);
                tracing::warn!(
                    module = targets::SKILL,
                    op = "commit_journal_cleanup",
                    skill_id = skill_id,
                    code = e.code(),
                    "skill commit completed but durable journal cleanup failed; bootstrap will retry"
                );
            }
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
            let compensate = if compensate.is_ok() {
                clear_journal(skills_root)
            } else {
                compensate
            };
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
    journal: &mut SkillCommitJournal,
) -> Result<()> {
    if faults.fail_before_lock {
        return Err(AppError::message(
            "skill.commit",
            "injected failure before lock write",
        ));
    }

    skill_lock_upsert(skills_root, skill_id, record.clone())?;

    journal.phase = SkillCommitPhase::LockCommitted;
    write_journal(skills_root, journal)?;

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
    journal.phase = SkillCommitPhase::PackageCommitted;
    write_journal(skills_root, journal)?;
    Ok(())
}

/// Recover one interrupted package commit.  The caller must hold the shared
/// source-root lock.  Every mutation is idempotent and the journal is removed
/// only after live, lock, package, and helper cleanup all succeed.
pub(crate) fn recover_skill_commit_journal(
    skills_root: &Path,
    repo: Option<&SkillRepo>,
) -> Result<bool> {
    let Some(journal) = load_journal(skills_root)? else {
        return Ok(false);
    };
    validate_journal_paths(skills_root, &journal)?;

    if journal.phase == SkillCommitPhase::Prepared {
        // Prepared may mean either "before the first rename" or that the
        // process crossed a rename and died before recording LiveSwapped.
        // Run the same idempotent rollback state machine in both cases so a
        // stale Prepared journal cannot hide a failed metadata compensation.
        remove_journal_helper(skills_root, &journal.staging)?;
        recover_after_live_swap(skills_root, &journal, repo)?;
        return Ok(true);
    }

    if journal.phase == SkillCommitPhase::PackageCommitted {
        // New metadata and live content are committed.  Finish cleanup only;
        // never roll back a successful package commit.
        let target_meta = fs::symlink_metadata(&journal.target).map_err(|e| {
            AppError::message(
                "skill.commit_recovery",
                format!("committed skill target is unavailable: {e}"),
            )
        })?;
        if !target_meta.is_dir()
            || crate::platform::skills::fs_safe::is_link_or_reparse(&target_meta)
        {
            return Err(AppError::message(
                "skill.commit_recovery",
                format!(
                    "refusing to remove backup while committed target is unsafe: {}",
                    journal.target.display()
                ),
            ));
        }
        validate_tree_entries_safe(&journal.target, "skill source")?;
        remove_journal_helper(skills_root, &journal.staging)?;
        if let Some(backup) = journal.backup.as_ref() {
            remove_journal_helper(skills_root, backup)?;
        }
        clear_journal(skills_root)?;
        return Ok(true);
    }

    recover_after_live_swap(skills_root, &journal, repo)?;
    Ok(true)
}

fn recover_after_live_swap(
    skills_root: &Path,
    journal: &SkillCommitJournal,
    repo: Option<&SkillRepo>,
) -> Result<()> {
    let mut journal = journal.clone();

    // Persist each completed compensation step before proceeding to the next
    // store. If the process dies after a rename/write but before this journal
    // update, the step is still safe to retry: the live rollback helper never
    // deletes a target when its backup has already disappeared.
    if matches!(
        journal.phase,
        SkillCommitPhase::Prepared
            | SkillCommitPhase::LiveSwapped
            | SkillCommitPhase::LockCommitted
            | SkillCommitPhase::PackageCommitted
    ) {
        let swap = RetainedLiveSwap {
            backup: journal.backup.clone(),
            first_install: !journal.had_live,
        };
        rollback_retained_swap_idempotent(skills_root, &journal.target, swap).map_err(|e| {
            AppError::message("skill.commit_recovery", format!("live restore failed: {e}"))
        })?;
        journal.phase = SkillCommitPhase::RollbackLiveRestored;
        write_journal(skills_root, &journal)?;
    }

    if journal.phase == SkillCommitPhase::RollbackLiveRestored {
        if !journal.had_lock_file {
            match fs::remove_file(skill_lock_file(skills_root)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(AppError::message(
                        "skill.commit_recovery",
                        format!("lock restore failed: {e}"),
                    ));
                }
            }
        } else if let Err(e) = skill_lock_replace_map(skills_root, &journal.old_lock) {
            return Err(AppError::message(
                "skill.commit_recovery",
                format!("lock restore failed: {e}"),
            ));
        }
        journal.phase = SkillCommitPhase::RollbackLockRestored;
        write_journal(skills_root, &journal)?;
    }

    if journal.phase == SkillCommitPhase::RollbackLockRestored {
        restore_package(repo, &journal).map_err(|e| {
            AppError::message(
                "skill.commit_recovery",
                format!("package restore failed: {e}"),
            )
        })?;
        journal.phase = SkillCommitPhase::RollbackPackageRestored;
        write_journal(skills_root, &journal)?;
    }

    if journal.phase == SkillCommitPhase::RollbackPackageRestored {
        remove_journal_helper(skills_root, &journal.staging).map_err(|e| {
            AppError::message(
                "skill.commit_recovery",
                format!("staging cleanup failed: {e}"),
            )
        })?;
        if let Some(backup) = journal.backup.as_ref() {
            match fs::symlink_metadata(backup) {
                Ok(_) => {
                    return Err(AppError::message(
                        "skill.commit_recovery",
                        format!(
                            "live restore left backup helper in place: {}",
                            backup.display()
                        ),
                    ));
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(AppError::message(
                        "skill.commit_recovery",
                        format!("backup helper state could not be verified: {e}"),
                    ));
                }
            }
        }
        journal.phase = SkillCommitPhase::RollbackHelpersCleaned;
        write_journal(skills_root, &journal)?;
    }

    if journal.phase == SkillCommitPhase::RollbackHelpersCleaned {
        clear_journal(skills_root)?;
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
