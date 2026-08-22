//! Durable journal for the shared skill package commit protocol.
//!
//! A commit crosses three independently durable stores (the live directory,
//! `.skill-lock.json`, and the package row).  The journal is written before
//! the first directory rename and is advanced after every durable step.  A
//! process which starts later can therefore finish or undo an interrupted
//! commit while holding the source-root lock.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::models::SkillSourceRecord;
use crate::platform::skills::fs_safe::{is_link_or_reparse, validate_skill_id};
use crate::storage::{SkillPackageRow, SkillRepo};
use crate::utils::atomic::atomic_write;

pub(crate) const SKILL_COMMIT_JOURNAL_SCHEMA: u32 = 1;
pub(crate) const SKILL_COMMIT_JOURNAL_FILE: &str = ".skill-commit-journal.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SkillCommitPhase {
    Prepared,
    LiveSwapped,
    LockCommitted,
    PackageCommitted,
    RollbackLiveRestored,
    RollbackLockRestored,
    RollbackPackageRestored,
    RollbackHelpersCleaned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SkillPackageSnapshot {
    pub id: String,
    pub source_kind: String,
    pub locator: String,
    pub revision: String,
    pub manifest_json: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&SkillPackageRow> for SkillPackageSnapshot {
    fn from(row: &SkillPackageRow) -> Self {
        Self {
            id: row.id.clone(),
            source_kind: row.source_kind.clone(),
            locator: row.locator.clone(),
            revision: row.revision.clone(),
            manifest_json: row.manifest_json.clone(),
            created_at: row.created_at.clone(),
            updated_at: row.updated_at.clone(),
        }
    }
}

impl From<SkillPackageSnapshot> for SkillPackageRow {
    fn from(row: SkillPackageSnapshot) -> Self {
        Self {
            id: row.id,
            source_kind: row.source_kind,
            locator: row.locator,
            revision: row.revision,
            manifest_json: row.manifest_json,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillCommitJournal {
    pub schema: u32,
    pub skill: String,
    pub target: PathBuf,
    pub had_live: bool,
    pub staging: PathBuf,
    pub backup: Option<PathBuf>,
    pub old_lock: BTreeMap<String, SkillSourceRecord>,
    pub had_lock_file: bool,
    pub old_package: Option<SkillPackageSnapshot>,
    pub has_package_repo: bool,
    pub phase: SkillCommitPhase,
}

pub(crate) fn journal_path(root: &Path) -> PathBuf {
    root.join(SKILL_COMMIT_JOURNAL_FILE)
}

pub(crate) fn write_journal(root: &Path, journal: &SkillCommitJournal) -> Result<()> {
    validate_journal_paths(root, journal)?;
    let bytes = serde_json::to_vec_pretty(journal).map_err(|e| {
        AppError::message("skill.commit_journal", format!("serialize journal: {e}"))
    })?;
    atomic_write(&journal_path(root), &bytes)?;
    sync_directory(root);
    Ok(())
}

pub(crate) fn load_journal(root: &Path) -> Result<Option<SkillCommitJournal>> {
    let path = journal_path(root);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(AppError::message(
                "skill.commit_journal",
                format!("read commit journal failed: {e}"),
            ));
        }
    };
    let journal: SkillCommitJournal = serde_json::from_str(&raw).map_err(|e| {
        AppError::message(
            "skill.commit_journal",
            format!("parse commit journal failed: {e}"),
        )
    })?;
    if journal.schema != SKILL_COMMIT_JOURNAL_SCHEMA {
        return Err(AppError::message(
            "skill.commit_journal",
            format!("unsupported commit journal schema: {}", journal.schema),
        ));
    }
    Ok(Some(journal))
}

pub(crate) fn clear_journal(root: &Path) -> Result<()> {
    match fs::remove_file(journal_path(root)) {
        Ok(()) => {
            sync_directory(root);
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::message(
            "skill.commit_journal",
            format!("remove commit journal failed: {e}"),
        )),
    }
}

/// Validate paths read from a journal before using them for any destructive
/// operation.  Only exact skill children and the helper directories allocated
/// by the commit protocol are accepted.
pub(crate) fn validate_journal_paths(root: &Path, journal: &SkillCommitJournal) -> Result<()> {
    if validate_skill_id(&journal.skill).is_err()
        || journal.skill.chars().any(|ch| ch == '/' || ch == '\\')
    {
        return Err(AppError::message(
            "skill.commit_journal",
            "commit journal contains an invalid skill id",
        ));
    }
    let target = root.join(&journal.skill);
    if !paths_equal_lexical(&journal.target, &target) {
        return Err(AppError::message(
            "skill.commit_journal",
            "commit journal target does not match skill root",
        ));
    }
    if journal
        .old_package
        .as_ref()
        .is_some_and(|package| package.id != journal.skill)
    {
        return Err(AppError::message(
            "skill.commit_journal",
            "commit journal package snapshot does not match skill",
        ));
    }
    if !journal.has_package_repo && journal.old_package.is_some() {
        return Err(AppError::message(
            "skill.commit_journal",
            "commit journal contains a package snapshot without a database",
        ));
    }
    validate_helper_path(root, &journal.staging, ".agenthub-stage-")?;
    if let Some(backup) = journal.backup.as_ref() {
        validate_helper_path(root, backup, ".agenthub-bak-")?;
    }
    if journal.had_live != journal.backup.is_some() {
        return Err(AppError::message(
            "skill.commit_journal",
            "commit journal live/backup state is inconsistent",
        ));
    }
    Ok(())
}

fn validate_helper_path(root: &Path, path: &Path, prefix: &str) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Err(AppError::message(
            "skill.commit_journal",
            "helper has no parent",
        ));
    };
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Err(AppError::message(
            "skill.commit_journal",
            "helper has invalid name",
        ));
    };
    if !paths_equal_lexical(parent, root) || !name.starts_with(prefix) {
        return Err(AppError::message(
            "skill.commit_journal",
            format!(
                "commit journal references an unsafe helper path: {}",
                path.display()
            ),
        ));
    }
    Ok(())
}

/// Remove only a helper path named by a validated journal.  This deliberately
/// does not scan the root or infer paths from a prefix.
pub(crate) fn remove_journal_helper(root: &Path, path: &Path) -> Result<()> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| AppError::message("skill.commit_journal", "helper has invalid name"))?;
    let prefix = if name.starts_with(".agenthub-stage-") {
        ".agenthub-stage-"
    } else if name.starts_with(".agenthub-bak-") {
        ".agenthub-bak-"
    } else {
        return Err(AppError::message(
            "skill.commit_journal",
            format!("refusing to remove non-helper path: {}", path.display()),
        ));
    };
    validate_helper_path(root, path, prefix)?;
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(AppError::from(e)),
    };
    if !meta.is_dir() || is_link_or_reparse(&meta) {
        return Err(AppError::message(
            "skill.commit_journal",
            format!("refusing to remove unsafe helper path: {}", path.display()),
        ));
    }
    fs::remove_dir_all(path)?;
    sync_directory(root);
    Ok(())
}

pub(crate) fn restore_package(
    repo: Option<&SkillRepo>,
    journal: &SkillCommitJournal,
) -> Result<()> {
    if !journal.has_package_repo {
        return Ok(());
    }
    let repo = repo.ok_or_else(|| {
        AppError::message(
            "skill.commit_journal",
            "database is required to recover the package row",
        )
    })?;
    match (&journal.old_package, repo.get_package(&journal.skill)?) {
        (Some(old), Some(current)) if SkillPackageSnapshot::from(&current) == *old => Ok(()),
        (Some(old), _) => {
            repo.upsert_package(&old.clone().into())?;
            Ok(())
        }
        (None, None) => Ok(()),
        (None, Some(_)) => match repo.delete_package(&journal.skill) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == "not_found" => Ok(()),
            Err(e) => Err(e),
        },
    }
}

fn paths_equal_lexical(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

fn sync_directory(path: &Path) {
    // Directory fsync is not supported on all Windows filesystems.  The
    // atomic file write is still durable; this best-effort step closes the
    // parent-directory durability gap where the platform permits it.
    if let Ok(dir) = fs::File::open(path) {
        let _ = dir.sync_all();
    }
}
