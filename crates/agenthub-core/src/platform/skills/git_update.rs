//! Atomic git skill update: fetch into staging, never `git pull` on live.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{AppError, Result};
use crate::platform::skills::fs_safe::{
    ensure_no_symlink_in_existing_prefix, validate_skills_root, validate_tree_entries_safe,
};
use crate::utils::redact::redact_url_userinfo;

/// Update a git-backed skill without mutating the live tree until ready.
///
/// Strategy:
/// 1. Clone the remote URL into a sibling staging directory (from recorded locator).
/// 2. Validate staging contains `SKILL.md`.
/// 3. Atomically replace live dir with staging (backup + rename).
///
/// Public behaviour is unchanged: clone → validate → swap → drop backup.
/// Install/update main-commit paths that need retained backup should use
/// [`prepare_git_skill_staging`] + the package commit coordinator instead.
///
/// `locator` is typically a git URL or `url#ref` recorded in `.skill-lock.json`.
pub fn atomic_git_skill_update(
    skills_root: &Path,
    skill_id: &str,
    live_dest: &Path,
    locator: &str,
) -> Result<()> {
    let staging = prepare_git_skill_staging(skills_root, skill_id, locator)?;
    // Replace live with staging via backup rename (Windows-safe order).
    replace_dir_atomic(skills_root, skill_id, live_dest, &staging)
}

/// Clone `locator` into a unique staging directory under `skills_root`.
///
/// Does **not** touch the live skill tree. Strips `.git`, requires `SKILL.md`,
/// and validates the whole tree with the shared FS safety helpers before return.
/// On any pre-swap failure, cleans the staging directory (no leak).
///
/// Crate-internal helper for the install/update coordinator (tests must not
/// hit the network — use local fixtures / fault injectors instead).
pub(crate) fn prepare_git_skill_staging(
    skills_root: &Path,
    skill_id: &str,
    locator: &str,
) -> Result<PathBuf> {
    // Validate root before create/write so pre-swap errors never leave
    // helper dirs behind and never traverse a linked skills_root.
    if !skills_root.exists() {
        ensure_no_symlink_in_existing_prefix(skills_root)?;
        std::fs::create_dir_all(skills_root)?;
    }
    validate_skills_root(skills_root)?;

    let staging = unique_staging(skills_root, skill_id)?;
    let (url, branch) = parse_git_locator(locator);

    let clone_status = if let Some(branch) = branch.as_deref() {
        Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                "--branch",
                branch,
                &url,
                &staging.to_string_lossy(),
            ])
            .status()
    } else {
        Command::new("git")
            .args(["clone", "--depth", "1", &url, &staging.to_string_lossy()])
            .status()
    }
    .map_err(|e| {
        let _ = std::fs::remove_dir_all(&staging);
        AppError::message("skill.update", format!("git clone failed: {e}"))
    })?;

    if !clone_status.success() {
        let _ = std::fs::remove_dir_all(&staging);
        // Never put raw locator/userinfo into AppError (may reach ERROR logs).
        let safe_url = redact_url_userinfo(&url);
        return Err(AppError::message(
            "skill.update",
            format!("git clone failed for skill '{skill_id}' from {safe_url}"),
        ));
    }

    // Drop .git from staging so the skill tree is a plain package (matches install).
    // Failure must surface and clean staging — never enter live with a partial tree.
    let git_dir = staging.join(".git");
    if git_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&git_dir) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(AppError::message(
                "skill.update",
                format!("failed to remove .git from staging for skill '{skill_id}': {e}"),
            ));
        }
    }

    if !staging.join("SKILL.md").is_file() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(AppError::InvalidArg(format!(
            "updated skill '{skill_id}' is missing SKILL.md"
        )));
    }

    // Full tree safety before any live swap (symlink/reparse/special entries).
    if let Err(e) = validate_tree_entries_safe(&staging, "skill staging") {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }

    Ok(staging)
}

pub(crate) fn parse_git_locator(locator: &str) -> (String, Option<String>) {
    // url#ref or plain url
    if let Some((url, rev)) = locator.rsplit_once('#') {
        if !url.is_empty() && !rev.is_empty() && !rev.contains('/') {
            return (url.to_string(), Some(rev.to_string()));
        }
    }
    (locator.to_string(), None)
}

fn unique_staging(skills_root: &Path, skill_id: &str) -> Result<PathBuf> {
    for i in 0..50 {
        let name = if i == 0 {
            format!(".staging-{skill_id}")
        } else {
            format!(".staging-{skill_id}-{i}")
        };
        let path = skills_root.join(name);
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(AppError::message(
        "skill.staging",
        "could not allocate unique skill staging directory",
    ))
}

fn replace_dir_atomic(
    skills_root: &Path,
    skill_id: &str,
    target: &Path,
    staging: &Path,
) -> Result<()> {
    let backup = skills_root.join(format!(".backup-{skill_id}"));
    if backup.exists() {
        let _ = std::fs::remove_dir_all(&backup);
    }

    if target.exists() {
        std::fs::rename(target, &backup).map_err(|e| {
            let _ = std::fs::remove_dir_all(staging);
            AppError::message(
                "skill.update",
                format!("failed to move live skill aside: {e}"),
            )
        })?;
    }

    match std::fs::rename(staging, target) {
        Ok(()) => {
            let _ = std::fs::remove_dir_all(&backup);
            Ok(())
        }
        Err(e) => {
            // Restore backup — report restore failure if it happens.
            if backup.exists() {
                if let Err(re) = std::fs::rename(&backup, target) {
                    let _ = std::fs::remove_dir_all(staging);
                    return Err(AppError::message(
                        "skill.update",
                        format!(
                            "failed to place updated skill: {e}; restore backup also failed: {re}"
                        ),
                    ));
                }
            }
            let _ = std::fs::remove_dir_all(staging);
            Err(AppError::message(
                "skill.update",
                format!("failed to place updated skill: {e}"),
            ))
        }
    }
}
