//! Skills platform: source materialization, package placement, lockfile, git update,
//! assignment + reconcile (P12).
//!
//! [`crate::services::SkillService`] remains the public façade; install/update
//! (and package place used by sync) own their FS work here so live trees are
//! never mutated via `git pull` and package swaps share one atomic path.

mod assignment;
mod bootstrap;
mod commit;
mod fs_safe;
mod git_update;
mod lockfile;
mod ownership;
mod packages;
mod reconcile;
mod sources;
mod target;

pub use assignment::SkillAssignmentService;
pub use bootstrap::{bootstrap_skill_assignments, SkillBootstrapReport};
pub use git_update::atomic_git_skill_update;
pub use ownership::is_managed_projection;
pub use packages::SkillPackageService;
pub use reconcile::{observed as skill_observed, SkillReconciler};
pub use sources::SkillSourceService;
pub use target::{AdapterSkillTarget, AgentSkillTarget, SkillTargetRegistry, StaticSkillTarget};

// Re-exports used by SkillService façade and its tests.
pub(crate) use assignment::package_revision;
pub(crate) use commit::{commit_skill_package, PreparedSkillTree, SkillCommitFaults};
pub(crate) use fs_safe::{
    collect_regular_files, detect_link_kind, ensure_no_symlink_in_ancestors,
    ensure_no_symlink_in_existing_prefix, inspect_projection_target, is_exact_child,
    is_link_or_reparse, link_resolves_to_source, normalize_rel_path, paths_equal_lexical,
    reject_source_target_overlap, remove_projection_link, resolve_link_path, validate_skill_id,
    validate_skills_root, validate_tree_entries_safe, TargetPresence,
};
pub(crate) use git_update::prepare_git_skill_staging;
pub(crate) use lockfile::{skill_lock_load, skill_lock_remove, skill_lock_upsert};
pub(crate) use ownership::{
    clear_managed_target_for_reproject, finalize_link_projection_ownership,
    project_copy_with_ownership, record_copy_ownership, unproject_with_ownership,
};
pub(crate) use packages::{materialize_projection, validate_and_collect_source, write_skill_tree};

// Used by SkillService tests (projection atomicity); keep re-export for façade tests.
#[cfg(test)]
pub(crate) use packages::replace_target_with_staging;
pub(crate) use sources::ensure_skill_md;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod assignment_tests;

#[cfg(test)]
mod ownership_tests;

#[cfg(test)]
mod commit_tests;
