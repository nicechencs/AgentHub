//! Install / uninstall / update / project / import orchestration.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Instant;

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{
    AgentId, Capability, Skill, SkillLinkKind, SkillProjectMode, SkillProjectResult,
    SkillSourceRecord,
};
use crate::platform::skills::{
    acquire_skill_lock, acquire_skill_root_lock, chrono_now, clear_managed_target_for_reproject,
    commit_skill_package, create_projection_link, ensure_no_symlink_in_existing_prefix,
    ensure_skill_md, finalize_link_projection_ownership, inspect_projection_target, is_exact_child,
    is_link_or_reparse, materialize_projection, paths_equal_lexical, prepare_git_skill_staging,
    read_skill_metadata, record_copy_ownership, recycle_skill_dir, remove_projection_link,
    resolve_link_path, skill_lock_load, skill_lock_remove, skill_lock_upsert,
    validate_and_collect_source, validate_skill_id, validate_skills_root,
    validate_tree_entries_safe, PreparedSkillTree, SkillCommitFaults, SkillPackageService,
    SkillSourceService, TargetPresence,
};
use crate::platform::AgentKey;
use crate::storage::SkillRepo;
use crate::utils::agent_lock::AgentWriteLock;

use super::{elapsed_ms, log_skill_write_result, SkillService};

impl SkillService {
    // -----------------------------------------------------------------------
    // Install / uninstall / update / project (Phase B)
    // -----------------------------------------------------------------------

    /// Install a skill into the shared source root from a local path, zip, or git URL.
    ///
    /// Atomic: materialize + validate under staging, then commit live + lock +
    /// package revision together (backup retained until metadata succeeds).
    /// Requires `SKILL.md` in the package root. Same-name install without
    /// `overwrite` is rejected.
    pub fn install_skill(&self, source: &str, overwrite: bool) -> Result<Skill> {
        let started = Instant::now();
        let result = (|| {
            let source = source.trim();
            if source.is_empty() {
                return Err(AppError::InvalidArg(
                    "install source must not be empty".into(),
                ));
            }
            let _global = acquire_skill_root_lock(&self.source_root)?;

            let sources = SkillSourceService::new();
            let packages = SkillPackageService::new();
            // 1–2: materialize + validate **before** touching live.
            let (package_dir, cleanup, source_kind, locator) = sources.materialize(source)?;
            let install_result = (|| {
                sources.ensure_skill_md(&package_dir)?;
                let skill_id = sources.infer_skill_id(&package_dir, source)?;
                let skill_id = validate_skill_id(&skill_id)?.to_string();
                let _skill_lock = acquire_skill_lock(&self.source_root, &skill_id)?;

                let dest = self.source_root.join(&skill_id);
                if dest.exists() && !overwrite {
                    return Err(AppError::InvalidArg(format!(
                        "skill '{skill_id}' already exists (pass overwrite to replace)"
                    )));
                }

                if !self.source_root.exists() {
                    fs::create_dir_all(&self.source_root)?;
                }
                ensure_no_symlink_in_existing_prefix(&self.source_root)?;

                if dest.exists() {
                    // Replace existing real directory only (never follow a link root).
                    let meta = fs::symlink_metadata(&dest)?;
                    if is_link_or_reparse(&meta) {
                        return Err(AppError::InvalidArg(format!(
                            "refusing to overwrite link at skill source: {}",
                            dest.display()
                        )));
                    }
                }

                let files = packages.validate_and_collect(&package_dir, &skill_id)?;
                let now = chrono_now();
                let record = SkillSourceRecord {
                    kind: source_kind,
                    locator: locator.clone(),
                    version: None,
                    installed_at: now.clone(),
                    updated_at: None,
                };

                let repo = self.db.as_ref().map(|db| SkillRepo::new(db.clone()));
                let committed = commit_skill_package(
                    &self.source_root,
                    &skill_id,
                    PreparedSkillTree::Files(&files),
                    record,
                    repo.as_ref(),
                    &now,
                    SkillCommitFaults::default(),
                )?;

                // Reconcile after main commit (no package rollback on target errors).
                self.reconcile_after_package_commit(&skill_id)?;

                let (name, description) = read_skill_metadata(&committed.dest, &committed.skill_id);
                let projections = self.project_matrix(&committed.skill_id, &committed.dest);
                Ok(Skill {
                    id: committed.skill_id,
                    name,
                    description,
                    source_dir: committed.dest,
                    projections,
                })
            })();

            if let Some(dir) = cleanup {
                let _ = fs::remove_dir_all(dir);
            }
            install_result
        })();

        match &result {
            Ok(skill) => {
                self.invalidate_list_cache();
                log_skill_write_result("install", &skill.id, None, started, true, None)
            }
            // Avoid logging raw source (may contain path/URL credentials); AppError is redacted.
            Err(e) => log_skill_write_result("install", "-", None, started, false, Some(e)),
        }
        result
    }

    /// Uninstall a shared source skill: snapshot, remove all agent projections,
    /// delete the source directory, drop lock entry.
    ///
    /// Agent-private skills (not under source root) are removed only from their
    /// own root when `skill_id` is found via [`list_installed`] with origin.
    pub fn uninstall_skill(
        &self,
        skill_id: &str,
        backup: Option<&crate::services::BackupService>,
    ) -> Result<()> {
        let started = Instant::now();
        let result = (|| {
            let skill_id = validate_skill_id(skill_id)?.to_string();
            let _global = acquire_skill_root_lock(&self.source_root)?;
            let _skill_lock = acquire_skill_lock(&self.source_root, &skill_id)?;

            let source_dir = self.source_root.join(&skill_id);
            if !source_dir.exists() {
                return Err(AppError::NotFound(format!(
                    "skill '{skill_id}' is not installed in the shared source root"
                )));
            }

            // Validate lock is readable *before* any projection/source delete.
            // A corrupt `.skill-lock.json` must not cause uninstall side effects.
            let _lock = skill_lock_load(&self.source_root)?;

            // Best-effort pre-uninstall snapshot of agent live files (not the skill
            // tree itself — backup_service snapshots adapter live paths). Callers
            // that hold BackupService should pass it; we still proceed without.
            if let Some(backups) = backup {
                for agent in AgentId::ALL {
                    if self
                        .registry
                        .get(agent)
                        .is_some_and(|a| a.capability(Capability::Skills).is_usable())
                    {
                        let _ = backups.snapshot(
                            agent,
                            crate::models::BackupKind::PreSkillUninstall,
                            Some(&format!("pre-skill-uninstall:{skill_id}")),
                        );
                    }
                }
            }

            // Build the complete key-native target set before mutating anything.
            // Assignments for a missing/disabled contribution fail closed: the
            // source must remain until that target can be safely reconciled.
            let target_keys: Vec<AgentKey> = self
                .targets
                .all()
                .filter(|target| target.supports_skills() && target.skills_root().is_some())
                .map(|target| target.agent_key())
                .collect();
            if let Some(db) = self.db.as_ref() {
                let repo = SkillRepo::new(db.clone());
                for assignment in repo.list_assignments_for_skill(&skill_id)? {
                    let key = AgentKey::parse(assignment.agent_key.clone()).map_err(|err| {
                        AppError::message(
                            "skill.assignment_data",
                            format!(
                                "invalid agent_key '{}' in skill assignment for '{}': {err}",
                                assignment.agent_key, skill_id
                            ),
                        )
                    })?;
                    if !target_keys.contains(&key) {
                        return Err(AppError::message(
                            "skill.target_unavailable",
                            format!(
                                "cannot uninstall skill '{skill_id}': target for assigned agent '{}' is not registered and usable",
                                key.as_str()
                            ),
                        ));
                    }
                }
            }

            // Remove projections first. Every lock, ownership, marker and target
            // failure propagates; source/lock deletion is strictly after all keys.
            let lock_dir = self.source_root.join(".locks");
            for key in target_keys {
                let _agent_lock = AgentWriteLock::acquire_key(&lock_dir, &key)?;
                self.disable_key(&skill_id, &key)?;
            }

            // Remove source directory (must be a real dir).
            let meta = fs::symlink_metadata(&source_dir)?;
            if is_link_or_reparse(&meta) {
                return Err(AppError::InvalidArg(format!(
                    "refusing to uninstall: source is a link: {}",
                    source_dir.display()
                )));
            }
            validate_tree_entries_safe(&source_dir, "skill source")?;
            recycle_skill_dir(&source_dir)?;
            skill_lock_remove(&self.source_root, &skill_id)?;
            Ok(())
        })();
        match &result {
            Ok(()) => {
                self.invalidate_list_cache();
                log_skill_write_result("uninstall", skill_id, None, started, true, None)
            }
            Err(e) => log_skill_write_result("uninstall", skill_id, None, started, false, Some(e)),
        }
        result
    }

    /// Uninstall a skill from an agent-private root (not projectable).
    pub fn uninstall_private_skill(&self, skill_id: &str, agent: AgentId) -> Result<()> {
        let started = Instant::now();
        let result = (|| {
            let skill_id = validate_skill_id(skill_id)?.to_string();
            let adapter = self.registry.get(agent).ok_or_else(|| {
                AppError::Unsupported(format!("agent {} has no adapter", agent.as_str()))
            })?;
            let skills_root = adapter.skills_dir().ok_or_else(|| {
                AppError::Unsupported(format!("agent {} has no skills directory", agent.as_str()))
            })?;
            let target = skills_root.join(&skill_id);
            // Must not be the shared source.
            if paths_equal_lexical(&skills_root, &self.source_root) {
                return self.uninstall_skill(&skill_id, None);
            }
            let lock_dir = self.source_root.join(".locks");
            let _agent_lock = AgentWriteLock::acquire(&lock_dir, agent)?;
            match inspect_projection_target(&target)? {
                TargetPresence::Missing => Ok(()),
                TargetPresence::Link { .. } => remove_projection_link(&target),
                TargetPresence::Directory => {
                    validate_tree_entries_safe(&target, "private skill")?;
                    recycle_skill_dir(&target)?;
                    Ok(())
                }
                TargetPresence::Dangerous { kind } => Err(AppError::InvalidArg(format!(
                    "refusing to uninstall private skill ({kind}): {}",
                    target.display()
                ))),
            }
        })();
        // Shared-root path already logs inside uninstall_skill; still record agent context.
        match &result {
            Ok(()) => {
                self.invalidate_list_cache();
                log_skill_write_result(
                    "uninstall_private",
                    skill_id,
                    Some(agent.as_str()),
                    started,
                    true,
                    None,
                )
            }
            Err(e) => log_skill_write_result(
                "uninstall_private",
                skill_id,
                Some(agent.as_str()),
                started,
                false,
                Some(e),
            ),
        }
        result
    }

    /// Update a skill from its recorded source (git: staging clone + commit;
    /// local/zip/market: re-fetch + commit). Never `git pull` on the live tree.
    pub fn update_skill(&self, skill_id: &str) -> Result<Skill> {
        let started = Instant::now();
        let result = (|| {
            let skill_id = validate_skill_id(skill_id)?.to_string();
            let _global = acquire_skill_root_lock(&self.source_root)?;
            let _skill_lock = acquire_skill_lock(&self.source_root, &skill_id)?;

            let dest = self.source_root.join(&skill_id);
            // Refuse link/junction roots (do not follow into foreign trees).
            match fs::symlink_metadata(&dest) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    return Err(AppError::NotFound(format!(
                        "skill '{skill_id}' not found in source root"
                    )));
                }
                Err(e) => return Err(AppError::from(e)),
                Ok(meta) if is_link_or_reparse(&meta) => {
                    return Err(AppError::InvalidArg(format!(
                        "refusing to update link at skill source: {}",
                        dest.display()
                    )));
                }
                Ok(meta) if !meta.is_dir() => {
                    return Err(AppError::NotFound(format!(
                        "skill '{skill_id}' not found in source root"
                    )));
                }
                Ok(_) => {}
            }
            // Main commit must surface lock read/parse errors (not empty map).
            let lock = skill_lock_load(&self.source_root)?;
            let record = lock.get(&skill_id).cloned().ok_or_else(|| {
                AppError::InvalidArg(format!(
                    "skill '{skill_id}' has no recorded source in .skill-lock.json; reinstall to track origin"
                ))
            })?;

            let now = chrono_now();
            let mut updated = record.clone();
            updated.updated_at = Some(now.clone());

            // 1–2: materialize + validate fully before live swap.
            let prepared = match record.kind.as_str() {
                "git" => {
                    // Clone to staging only — no live pull.
                    let staging =
                        prepare_git_skill_staging(&self.source_root, &skill_id, &record.locator)?;
                    PreparedSkillTree::StagingDir(staging)
                }
                "local" | "zip" | "market" => {
                    let sources = SkillSourceService::new();
                    let packages = SkillPackageService::new();
                    let (package_dir, cleanup, _, _) = sources.materialize(&record.locator)?;
                    let files_result = (|| {
                        sources.ensure_skill_md(&package_dir)?;
                        packages.validate_and_collect(&package_dir, &skill_id)
                    })();
                    if let Some(dir) = cleanup {
                        let _ = fs::remove_dir_all(dir);
                    }
                    let files = files_result?;
                    // Leak files into owned map for commit (PreparedSkillTree::Files borrows).
                    // Commit immediately in this arm instead.
                    let repo = self.db.as_ref().map(|db| SkillRepo::new(db.clone()));
                    let committed = commit_skill_package(
                        &self.source_root,
                        &skill_id,
                        PreparedSkillTree::Files(&files),
                        updated.clone(),
                        repo.as_ref(),
                        &now,
                        SkillCommitFaults::default(),
                    )?;
                    self.reconcile_after_package_commit(&skill_id)?;
                    let (name, description) =
                        read_skill_metadata(&committed.dest, &committed.skill_id);
                    return Ok(Skill {
                        id: committed.skill_id,
                        name,
                        description,
                        source_dir: committed.dest,
                        projections: self.project_matrix(&skill_id, &dest),
                    });
                }
                other => {
                    return Err(AppError::InvalidArg(format!(
                        "cannot update skill with source kind '{other}'"
                    )));
                }
            };

            let repo = self.db.as_ref().map(|db| SkillRepo::new(db.clone()));
            let committed = commit_skill_package(
                &self.source_root,
                &skill_id,
                prepared,
                updated,
                repo.as_ref(),
                &now,
                SkillCommitFaults::default(),
            )?;

            // assignment_stack / ensure_package errors are part of main commit
            // (via commit_skill_package). Reconcile is post-commit only.
            self.reconcile_after_package_commit(&skill_id)?;

            let (name, description) = read_skill_metadata(&committed.dest, &committed.skill_id);
            Ok(Skill {
                id: committed.skill_id,
                name,
                description,
                source_dir: committed.dest,
                projections: self.project_matrix(&skill_id, &dest),
            })
        })();
        match &result {
            Ok(skill) => {
                self.invalidate_list_cache();
                log_skill_write_result("update", &skill.id, None, started, true, None)
            }
            Err(e) => log_skill_write_result("update", skill_id, None, started, false, Some(e)),
        }
        result
    }

    /// Project a source skill onto an agent as link or copy.
    ///
    /// Windows link mode: junction → symlink → copy.
    /// Unix link mode: symlink → copy.
    pub fn project_skill(
        &self,
        skill_id: &str,
        agent: AgentId,
        mode: SkillProjectMode,
    ) -> Result<SkillProjectResult> {
        let started = Instant::now();
        let result = (|| {
            let skill_id = validate_skill_id(skill_id)?.to_string();
            let lock_dir = self.source_root.join(".locks");
            let _agent_lock = AgentWriteLock::acquire(&lock_dir, agent)?;
            let _skill_lock = acquire_skill_lock(&self.source_root, &skill_id)?;

            let (source_dir, skills_root, target_dir) =
                self.resolve_projection_paths(&skill_id, agent)?;
            let agent_key = AgentKey::from_agent_id(agent);
            ensure_skill_md(&source_dir).map_err(|_| {
                AppError::NotFound(format!(
                    "skill source not found or missing SKILL.md: {skill_id}"
                ))
            })?;

            // Only clear targets we can prove are platform-managed (R03).
            // Unmanaged user directories / foreign links → skill.conflict.
            clear_managed_target_for_reproject(
                &skills_root,
                &skill_id,
                &source_dir,
                &target_dir,
                &agent_key,
            )?;

            if !skills_root.exists() {
                ensure_no_symlink_in_existing_prefix(&skills_root)?;
                fs::create_dir_all(&skills_root)?;
            }
            validate_skills_root(&skills_root)?;

            let revision = self.projection_revision(&skill_id);
            let (applied, fell_back) = match mode {
                SkillProjectMode::Copy => {
                    let files = validate_and_collect_source(&source_dir, &skill_id)?;
                    materialize_projection(&skills_root, &skill_id, &target_dir, &files, None)?;
                    // Marker write failure fails the op; assignment path would
                    // not claim applied. Here project_skill returns Err.
                    record_copy_ownership(&skills_root, &skill_id, &target_dir, &revision)?;
                    (SkillLinkKind::None, false)
                }
                SkillProjectMode::Link => {
                    let (applied, fell_back) = create_projection_link(&source_dir, &target_dir)?;
                    // True link: clear stale marker (errors must not be swallowed).
                    // SkillLinkKind::None = copy fallback → record ownership marker.
                    finalize_link_projection_ownership(
                        &skills_root,
                        &skill_id,
                        &target_dir,
                        applied == SkillLinkKind::None,
                        &revision,
                    )?;
                    (applied, fell_back)
                }
            };

            Ok(SkillProjectResult {
                skill_id,
                agent,
                requested_mode: mode,
                applied_link_kind: applied,
                fell_back,
                target_dir,
            })
        })();
        match &result {
            Ok(r) => {
                self.invalidate_list_cache();
                tracing::info!(
                    module = targets::SKILL,
                    op = "project",
                    skill_id = %r.skill_id,
                    agent = agent.as_str(),
                    mode = mode.as_str(),
                    applied = r.applied_link_kind.as_str(),
                    fell_back = r.fell_back,
                    elapsed_ms = elapsed_ms(started),
                    "ok"
                );
            }
            Err(e) => log_skill_write_result(
                "project",
                skill_id,
                Some(agent.as_str()),
                started,
                false,
                Some(e),
            ),
        }
        result
    }

    /// Copy an agent-private skill into the shared source root.
    ///
    /// - Default is **copy** (never moves / deletes the private skill).
    /// - Validates skill id, path safety, and `SKILL.md`.
    /// - Without `overwrite`, refuses if the shared root already has the same id.
    /// - Reuses the same locks / staging / tree validation as [`install_skill`].
    pub fn import_private_to_shared(
        &self,
        skill_id: &str,
        agent: AgentId,
        overwrite: bool,
    ) -> Result<Skill> {
        let started = Instant::now();
        let result = (|| {
            let skill_id = validate_skill_id(skill_id)?.to_string();
            let adapter = self.registry.require(agent, Capability::Skills)?;
            let skills_root = adapter.skills_dir().ok_or_else(|| {
                AppError::Unsupported(format!("agent {} has no skills directory", agent.as_str()))
            })?;

            // Private root must not be the shared source.
            if paths_equal_lexical(&skills_root, &self.source_root) {
                return Err(AppError::InvalidArg(format!(
                    "skill '{skill_id}' is already under the shared source root"
                )));
            }

            let private_dir = skills_root.join(&skill_id);
            if !is_exact_child(&private_dir, &skills_root, &skill_id) {
                return Err(AppError::InvalidArg(format!(
                    "private skill path escapes agent skills root: {}",
                    private_dir.display()
                )));
            }

            // Resolve real package directory (follow link only for reading content).
            let package_dir = match inspect_projection_target(&private_dir)? {
                TargetPresence::Missing => {
                    return Err(AppError::NotFound(format!(
                        "private skill '{skill_id}' not found for {}",
                        agent.as_str()
                    )));
                }
                TargetPresence::Link { .. } => {
                    resolve_link_path(&private_dir).ok_or_else(|| {
                        AppError::InvalidArg(format!(
                            "private skill link is unresolvable: {}",
                            private_dir.display()
                        ))
                    })?
                }
                TargetPresence::Directory => private_dir.clone(),
                TargetPresence::Dangerous { kind } => {
                    return Err(AppError::InvalidArg(format!(
                        "refusing to import unsafe private skill ({kind}): {}",
                        private_dir.display()
                    )));
                }
            };

            // Do not allow importing the shared root itself via a private alias.
            if paths_equal_lexical(&package_dir, &self.source_root.join(&skill_id)) {
                return Err(AppError::InvalidArg(format!(
                    "private skill resolves to the shared source entry for '{skill_id}'"
                )));
            }

            ensure_skill_md(&package_dir)?;
            let files = validate_and_collect_source(&package_dir, &skill_id)?;

            let _global = acquire_skill_root_lock(&self.source_root)?;
            let _skill_lock = acquire_skill_lock(&self.source_root, &skill_id)?;

            let dest = self.source_root.join(&skill_id);
            if dest.exists() && !overwrite {
                return Err(AppError::message(
                    "skill.conflict",
                    format!(
                        "skill '{skill_id}' already exists in shared source (pass overwrite to replace)"
                    ),
                ));
            }

            if !self.source_root.exists() {
                fs::create_dir_all(&self.source_root)?;
            }
            ensure_no_symlink_in_existing_prefix(&self.source_root)?;

            if dest.exists() {
                let meta = fs::symlink_metadata(&dest)?;
                if is_link_or_reparse(&meta) {
                    return Err(AppError::InvalidArg(format!(
                        "refusing to overwrite link at skill source: {}",
                        dest.display()
                    )));
                }
                materialize_projection(&self.source_root, &skill_id, &dest, &files, Some(&dest))?;
            } else {
                materialize_projection(&self.source_root, &skill_id, &dest, &files, None)?;
            }

            let now = chrono_now();
            let record = SkillSourceRecord {
                kind: "local".into(),
                locator: private_dir.to_string_lossy().into_owned(),
                version: None,
                installed_at: now,
                updated_at: None,
            };
            skill_lock_upsert(&self.source_root, &skill_id, record)?;

            // Private original must remain after copy.
            if !private_dir.exists() {
                return Err(AppError::message(
                    "skill.import",
                    format!(
                        "import unexpectedly removed private skill at {}",
                        private_dir.display()
                    ),
                ));
            }

            let (name, description) = read_skill_metadata(&dest, &skill_id);
            let projections = self.project_matrix(&skill_id, &dest);
            Ok(Skill {
                id: skill_id,
                name,
                description,
                source_dir: dest,
                projections,
            })
        })();

        match &result {
            Ok(skill) => {
                self.invalidate_list_cache();
                log_skill_write_result(
                    "import_private",
                    &skill.id,
                    Some(agent.as_str()),
                    started,
                    true,
                    None,
                )
            }
            Err(e) => log_skill_write_result(
                "import_private",
                skill_id,
                Some(agent.as_str()),
                started,
                false,
                Some(e),
            ),
        }
        result
    }

    pub fn skill_lock_path(&self) -> PathBuf {
        self.source_root.join(".skill-lock.json")
    }
}
