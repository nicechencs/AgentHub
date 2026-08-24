//! Skills platform: source materialization, package placement, lockfile, git update,
//! assignment + reconcile (P12), plus pure list/classify/hash/yaml helpers (P2-6).
//!
//! [`crate::services::SkillService`] remains the public façade; install/update
//! (and package place used by sync) own their FS work here so live trees are
//! never mutated via `git pull` and package swaps share one atomic path.

mod assignment;
mod bootstrap;
mod catalog;
mod classify;
mod commit;
mod fs_index;
mod fs_safe;
mod git_update;
mod hash;
mod journal;
mod lockfile;
mod ownership;
mod packages;
mod projection_link;
mod reconcile;
mod scoped_lock;
mod sources;
mod target;
mod time;
mod yaml;

pub use assignment::SkillAssignmentService;
pub use bootstrap::{bootstrap_skill_assignments, SkillBootstrapReport};
pub use git_update::atomic_git_skill_update;
pub use ownership::is_managed_projection;
pub use packages::SkillPackageService;
pub use reconcile::{observed as skill_observed, SkillReconciler};
pub use sources::SkillSourceService;
pub use target::{
    build_builtin_skill_targets, builtin_skill_target_registry, AdapterSkillTarget,
    AgentSkillTarget, SkillTargetRegistry, StaticSkillTarget,
};

// Re-exports used by SkillService façade and its tests.
pub(crate) use assignment::package_revision;
pub(crate) use catalog::{
    for_each_agent_skill_dir, installed_skill_from_agent, installed_skill_from_shared,
    map_status_agent_vs_shared,
};
pub(crate) use classify::classify_projection;
pub(crate) use commit::{
    commit_skill_package, recover_skill_commit_journal, PreparedSkillTree, SkillCommitFaults,
};
pub(crate) use fs_index::collect_file_index;
// collect_regular_files / detect_link_kind / normalize_rel_path are re-exported
// for skill_service tests.
#[cfg(test)]
pub(crate) use fs_safe::{collect_regular_files, detect_link_kind, normalize_rel_path};
pub(crate) use fs_safe::{
    ensure_no_symlink_in_ancestors, ensure_no_symlink_in_existing_prefix,
    inspect_projection_target, is_exact_child, is_link_or_reparse, link_resolves_to_source,
    paths_equal_lexical, reject_source_target_overlap, remove_projection_link, resolve_link_path,
    resolve_readable_skill_dir, validate_skill_id, validate_skills_root,
    validate_tree_entries_safe, TargetPresence,
};
pub(crate) use git_update::prepare_git_skill_staging;
pub(crate) use hash::{fingerprint_skill_tree, hash_skill_root_shallow};
pub(crate) use journal::journal_path as skill_commit_journal_path;
pub(crate) use lockfile::{skill_lock_load, skill_lock_remove, skill_lock_upsert};
pub(crate) use ownership::{
    clear_managed_target_for_reproject, finalize_link_projection_ownership,
    project_copy_with_ownership, record_copy_ownership, recycle_skill_dir,
    unproject_with_ownership,
};
#[cfg(test)]
pub(crate) use packages::write_skill_tree;
pub(crate) use packages::{materialize_projection, validate_and_collect_source};
pub(crate) use projection_link::create_projection_link;
pub(crate) use scoped_lock::{acquire_skill_lock, acquire_skill_root_lock};
pub(crate) use time::chrono_now;
pub(crate) use yaml::{parse_skill_frontmatter, read_skill_md_file, read_skill_metadata};

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

#[cfg(test)]
mod reconcile_link_tests;
