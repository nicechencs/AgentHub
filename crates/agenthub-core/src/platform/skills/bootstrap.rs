//! Idempotent import of skill packages / managed assignments from lock + FS (P12).
//!
//! Migration only creates tables. This helper reads `.skill-lock.json` and
//! existing projections: only imports assignments when the projection is
//! platform-managed with certainty. Uncertain content is left untouched
//! (no assignment, or observed conflict) — never deleted.

use std::fs;
use std::path::Path;

use crate::error::Result;
use crate::platform::skills::assignment::SkillAssignmentService;
use crate::platform::skills::commit::recover_skill_commit_journal;
use crate::platform::skills::fs_safe::{
    inspect_projection_target, validate_skill_id, TargetPresence,
};
use crate::platform::skills::lockfile::skill_lock_load;
use crate::platform::skills::ownership::is_managed_projection;
use crate::platform::skills::scoped_lock::acquire_skill_root_lock;
use crate::platform::skills::target::SkillTargetRegistry;
use crate::storage::{SkillAssignmentRow, SkillRepo};

/// Summary of a bootstrap pass (for tests / diagnostics).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillBootstrapReport {
    pub packages_ensured: usize,
    pub assignments_imported: usize,
    pub conflicts_noted: usize,
    pub skipped_uncertain: usize,
}

/// Idempotent bootstrap from source root lock + current projections.
///
/// - Ensures `skill_packages` for each lock record (and on-disk skill dirs that
///   appear in the lock).
/// - For each target agent: if projection is Linked/Copied with managed
///   certainty, set assignment `desired_enabled=true` and
///   `applied_revision` / `observed_status=applied`.
/// - If target exists but is not managed → leave assignment absent (do not
///   delete FS). Optionally note conflict when an assignment already exists.
pub fn bootstrap_skill_assignments(
    source_root: &Path,
    targets: &SkillTargetRegistry,
    repo: &SkillRepo,
    now: &str,
) -> Result<SkillBootstrapReport> {
    // Startup recovery and import must observe one coherent source root.  The
    // same lock is held by install/update/uninstall before any rename.
    let _root_lock = acquire_skill_root_lock(source_root)?;
    recover_skill_commit_journal(source_root, Some(repo))?;

    let mut report = SkillBootstrapReport::default();
    let assign = SkillAssignmentService::new(repo.clone());
    let lock = skill_lock_load(source_root)?;

    // Packages from lock file.
    for (skill_id, record) in &lock {
        if validate_skill_id(skill_id).is_err() {
            continue;
        }
        assign.ensure_package(skill_id, Some(record), now)?;
        report.packages_ensured += 1;
    }

    // Also ensure packages for on-disk skill dirs that have lock entries only
    // (already covered). For dirs without lock: do not invent packages here
    // unless we need assignment import — managed projections still need a package.
    if source_root.is_dir() {
        if let Ok(entries) = fs::read_dir(source_root) {
            for ent in entries.flatten() {
                let name = ent.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                if validate_skill_id(&name).is_err() {
                    continue;
                }
                let path = ent.path();
                let Ok(meta) = fs::symlink_metadata(&path) else {
                    continue;
                };
                if !meta.is_dir() {
                    continue;
                }
                if lock.contains_key(&name) {
                    continue; // already ensured
                }
                // No lock record: only create package if a managed projection exists
                // for some agent (so we can import assignment).
                let source_dir = path;
                let mut needs_package = false;
                for target in targets.all() {
                    if !target.supports_skills() {
                        continue;
                    }
                    let Some(root) = target.skills_root() else {
                        continue;
                    };
                    let target_dir = root.join(&name);
                    // No package revision yet — only import certainty from link
                    // or a marker that validates against live content.
                    if is_managed_projection(&source_dir, root.as_path(), &name, &target_dir, None)
                    {
                        needs_package = true;
                        break;
                    }
                }
                if needs_package {
                    assign.ensure_package(&name, None, now)?;
                    report.packages_ensured += 1;
                }
            }
        }
    }

    // Import managed assignments for every known package × target.
    let packages = repo.list_packages()?;
    for package in packages {
        let skill_id = package.id.as_str();
        let source_dir = source_root.join(skill_id);
        if !source_dir.is_dir() {
            continue;
        }
        let revision = package.revision.clone();

        for target in targets.all() {
            if !target.supports_skills() {
                continue;
            }
            let agent_key = target.agent_key();
            let Some(skills_root) = target.skills_root() else {
                continue;
            };
            let target_dir = skills_root.join(skill_id);

            let existing = repo.get_assignment(skill_id, agent_key.as_str())?;
            if existing
                .as_ref()
                .is_some_and(|a| a.desired_enabled || a.observed_status == "applied")
            {
                // Already imported / managed — idempotent skip.
                continue;
            }

            // Bootstrap requires marker+revision+fingerprint (or valid link).
            if is_managed_projection(
                &source_dir,
                skills_root.as_path(),
                skill_id,
                &target_dir,
                Some(revision.as_str()),
            ) {
                let mode = match inspect_projection_target(&target_dir) {
                    Ok(TargetPresence::Link { .. }) => "link",
                    _ => "copy",
                };
                let row = SkillAssignmentRow {
                    skill_package_id: skill_id.to_string(),
                    agent_key: agent_key.to_string(),
                    desired_enabled: true,
                    projection_mode: mode.into(),
                    applied_revision: Some(revision.clone()),
                    observed_status: "applied".into(),
                    last_error: None,
                    updated_at: now.to_string(),
                };
                repo.upsert_assignment(&row)?;
                report.assignments_imported += 1;
                continue;
            }

            // Target present but unmanaged → do not delete; leave assignment absent.
            match inspect_projection_target(&target_dir) {
                Ok(TargetPresence::Missing) => {}
                Ok(TargetPresence::Link { .. }) | Ok(TargetPresence::Directory) => {
                    report.skipped_uncertain += 1;
                    if let Some(prev) = existing {
                        // Note conflict on existing pending assignment only.
                        if prev.desired_enabled {
                            repo.update_observed(
                                skill_id,
                                agent_key.as_str(),
                                "conflict",
                                None,
                                Some("unmanaged projection present; not overwritten by bootstrap"),
                                now,
                            )?;
                            report.conflicts_noted += 1;
                        }
                    }
                }
                _ => {
                    report.skipped_uncertain += 1;
                }
            }
        }
    }

    Ok(report)
}
