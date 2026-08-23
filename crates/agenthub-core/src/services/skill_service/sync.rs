//! Sync / disable / assignment reconcile orchestration.

use std::path::PathBuf;
use std::time::Instant;

use crate::error::{AppError, Result};
use crate::logging::targets;
use crate::models::{
    AgentId, SkillAction, SkillFailure, SkillProjectMode, SkillSyncReport, SkillSyncState,
};
use crate::platform::skills::{
    chrono_now, ensure_no_symlink_in_ancestors, ensure_no_symlink_in_existing_prefix,
    inspect_projection_target, is_exact_child, link_resolves_to_source, package_revision,
    project_copy_with_ownership, reject_source_target_overlap, skill_lock_load,
    unproject_with_ownership, validate_and_collect_source, validate_skill_id, validate_skills_root,
    SkillAssignmentService, SkillReconciler, TargetPresence,
};
use crate::platform::AgentKey;
use crate::storage::SkillRepo;

use super::{log_skill_write_result, SkillService};

impl SkillService {
    /// Project one source skill onto a single agent's `skills_dir/<skill_id>` as a copy.
    ///
    /// - `force == false`: create when absent; no-op when already linked to source
    ///   or byte-identical copy; reject foreign/conflict without mutation.
    /// - `force == true`: build a sibling staging directory, then replace the
    ///   exact target (including removing a prior projection link without following
    ///   it). Old target is preserved until staging is complete; on replace failure
    ///   staging is cleaned and the old target restored best-effort. Sibling skills
    ///   are never mutated.
    /// - Existing real-directory targets with nested symlink / special / unreadable
    ///   entries are always rejected with [`AppError::InvalidArg`] (including force).
    ///
    /// When a database is attached ([`with_db`]), desired assignment is set first
    /// and projection runs through [`SkillReconciler`] (failure keeps desired +
    /// `last_error`, never claims applied).
    pub fn sync(&self, skill_id: &str, agent: AgentId, force: bool) -> Result<()> {
        self.sync_with_mode(skill_id, agent, force, None)
    }

    /// Like [`Self::sync`], but may overwrite `projection_mode`.
    ///
    /// `None` keeps the stored mode (new rows still default to copy). `Some(Link)`
    /// / `Some(Copy)` persist that mode and reconcile. Copy on a correct source
    /// link removes the link first so the user-requested copy is not a no-op.
    pub fn sync_with_mode(
        &self,
        skill_id: &str,
        agent: AgentId,
        force: bool,
        mode: Option<SkillProjectMode>,
    ) -> Result<()> {
        let started = Instant::now();
        let result = (|| {
            let skill_id = validate_skill_id(skill_id)?;
            if self.db.is_some() {
                self.sync_via_assignment(skill_id, agent, force, mode)
            } else if mode == Some(SkillProjectMode::Link) {
                self.project_skill(skill_id, agent, SkillProjectMode::Link)
                    .map(|_| ())
            } else {
                if mode == Some(SkillProjectMode::Copy) {
                    self.replace_correct_link_with_copy_if_needed(skill_id, agent)?;
                }
                self.sync_projection(skill_id, agent, force)
            }
        })();
        match &result {
            Ok(()) => {
                self.invalidate_list_cache();
                log_skill_write_result("sync", skill_id, Some(agent.as_str()), started, true, None)
            }
            Err(e) => log_skill_write_result(
                "sync",
                skill_id,
                Some(agent.as_str()),
                started,
                false,
                Some(e),
            ),
        }
        result
    }

    /// Sync every listed skill onto `targets`.
    ///
    /// Individual `sync` errors are collected into [`SkillSyncReport::failed`]
    /// rather than aborting the batch. `list()` failures still propagate.
    ///
    /// When `skip_unsupported` is true, a skill whose projection state for that
    /// agent is [`SkillSyncState::Unsupported`] is recorded as skipped instead
    /// of calling `sync`. CLI `--all` and GUI "all agents" pass `true`; a
    /// single-agent target passes `false` so unsupported still surfaces as
    /// `failed`.
    pub fn sync_targets(
        &self,
        targets: &[AgentId],
        force: bool,
        skip_unsupported: bool,
    ) -> Result<SkillSyncReport> {
        let skills = self.list()?;
        let mut report = SkillSyncReport::default();

        for skill in &skills {
            for &agent in targets {
                let action = SkillAction {
                    skill: skill.id.clone(),
                    agent,
                };
                if skip_unsupported && skill.state_for(agent) == Some(SkillSyncState::Unsupported) {
                    report.skipped.push(action);
                    continue;
                }
                match self.sync(&skill.id, agent, force) {
                    Ok(()) => report.synced.push(action),
                    Err(error) => report.failed.push(SkillFailure {
                        skill: action.skill,
                        agent,
                        code: error.code().to_string(),
                        error: error.to_string(),
                    }),
                }
            }
        }
        Ok(report)
    }

    /// Remove the projected skill directory for one agent.
    ///
    /// Only the exact validated `skills_dir/<skill_id>` target is removed.
    /// Missing target is an idempotent success.
    ///
    /// **Link targets** (symlink / junction): remove the link entry itself via
    /// [`remove_projection_link`] — never follow into the source tree.
    ///
    /// Real directories with nested unsafe entries, special/non-dir targets, and
    /// containment violations are rejected without mutation. Source tree is not
    /// modified.
    ///
    /// With a database, sets `desired_enabled=false` then reconciles.
    pub fn disable(&self, skill_id: &str, agent: AgentId) -> Result<()> {
        self.disable_key(skill_id, &AgentKey::from_agent_id(agent))
    }

    /// Key-native disable path used by shared uninstall and future Agent targets.
    pub fn disable_key(&self, skill_id: &str, agent_key: &AgentKey) -> Result<()> {
        let started = Instant::now();
        let result = (|| {
            let skill_id = validate_skill_id(skill_id)?;
            if self.db.is_some() {
                self.disable_via_assignment_key(skill_id, agent_key)
            } else {
                self.disable_projection_key(skill_id, agent_key)
            }
        })();
        match &result {
            Ok(()) => {
                self.invalidate_list_cache();
                log_skill_write_result(
                    "disable",
                    skill_id,
                    Some(agent_key.as_str()),
                    started,
                    true,
                    None,
                )
            }
            Err(e) => log_skill_write_result(
                "disable",
                skill_id,
                Some(agent_key.as_str()),
                started,
                false,
                Some(e),
            ),
        }
        result
    }

    /// FS-only project (used when no DB). Ownership rules match
    /// [`SkillReconciler::project_copy`] via shared `project_copy_with_ownership`.
    pub(super) fn sync_projection(
        &self,
        skill_id: &str,
        agent: AgentId,
        force: bool,
    ) -> Result<()> {
        let (source_dir, skills_root, target_dir) =
            self.resolve_projection_paths(skill_id, agent)?;
        let agent_key = AgentKey::from_agent_id(agent);

        // Validate source tree up front (no staging until source is safe).
        let source_files = validate_and_collect_source(&source_dir, skill_id)?;
        let revision = self.projection_revision(skill_id);

        project_copy_with_ownership(
            &skills_root,
            skill_id,
            &source_dir,
            &target_dir,
            &source_files,
            force,
            &revision,
            &agent_key,
        )
    }

    /// Resolve ownership revision from lock record, then package row, else `"1"`.
    pub(super) fn projection_revision(&self, skill_id: &str) -> String {
        if let Ok(lock) = skill_lock_load(&self.source_root) {
            if let Some(rec) = lock.get(skill_id) {
                return package_revision(rec);
            }
        }
        if let Some(db) = self.db.as_ref() {
            if let Ok(Some(pkg)) = SkillRepo::new(db.clone()).get_package(skill_id) {
                if !pkg.revision.trim().is_empty() {
                    return pkg.revision;
                }
            }
        }
        "1".into()
    }

    /// After a successful package commit: reconcile desired assignments.
    ///
    /// Single-target failures keep the shared package and rely on the reconciler
    /// observed error; infrastructure failures return a partial-failure error
    /// (package is **not** rolled back).
    pub(super) fn reconcile_after_package_commit(&self, skill_id: &str) -> Result<()> {
        let Some(_) = self.db.as_ref() else {
            return Ok(());
        };
        let now = chrono_now();
        let partial = |e: &AppError| {
            let msg = crate::utils::redact::redact_text(&e.to_string());
            AppError::message(
                "skill.reconcile_partial",
                format!(
                    "shared package for '{skill_id}' was updated but projection reconcile failed: {msg}"
                ),
            )
        };
        let (_assign, reconciler) = self.assignment_stack().map_err(|e| partial(&e))?;
        match reconciler.reconcile_skill(skill_id, true, &now) {
            Ok(outcomes) => {
                let mut failed = 0u32;
                for (agent, r) in &outcomes {
                    if let Err(e) = r {
                        failed += 1;
                        let msg = crate::utils::redact::redact_text(&e.to_string());
                        tracing::warn!(
                            module = targets::SKILL,
                            op = "reconcile_after_commit",
                            skill_id = skill_id,
                            agent = agent.as_str(),
                            code = e.code(),
                            "shared package committed; target reconcile failed: {msg}"
                        );
                    }
                }
                if failed > 0 {
                    tracing::warn!(
                        module = targets::SKILL,
                        op = "reconcile_after_commit",
                        skill_id = skill_id,
                        failed_targets = failed,
                        total_targets = outcomes.len(),
                        "shared package committed; {failed}/{} target reconcile(s) failed",
                        outcomes.len()
                    );
                }
                Ok(())
            }
            // Repo / update_observed infrastructure errors → skill.reconcile_partial.
            Err(e) => Err(partial(&e)),
        }
    }

    /// FS-only unproject — ownership proof required (same as reconciler).
    pub(super) fn disable_projection_key(
        &self,
        skill_id: &str,
        agent_key: &AgentKey,
    ) -> Result<()> {
        let (source_dir, skills_root, target_dir) =
            self.resolve_projection_paths_key(skill_id, agent_key)?;

        unproject_with_ownership(&skills_root, skill_id, &source_dir, &target_dir, agent_key)
    }

    pub(super) fn assignment_stack(&self) -> Result<(SkillAssignmentService, SkillReconciler)> {
        let db = self.db.as_ref().ok_or_else(|| {
            AppError::message("skill.db", "skill assignment database is not configured")
        })?;
        let repo = SkillRepo::new(db.clone());
        let assign = SkillAssignmentService::new(repo.clone());
        let reconciler = SkillReconciler::new(self.source_root.clone(), self.targets.clone(), repo);
        Ok((assign, reconciler))
    }

    pub(super) fn sync_via_assignment(
        &self,
        skill_id: &str,
        agent: AgentId,
        force: bool,
        mode: Option<SkillProjectMode>,
    ) -> Result<()> {
        let now = chrono_now();
        let (assign, reconciler) = self.assignment_stack()?;
        let lock = skill_lock_load(&self.source_root)?;
        let record = lock.get(skill_id);
        let agent_key = AgentKey::from_agent_id(agent);
        assign.ensure_package(skill_id, record, &now)?;
        if mode == Some(SkillProjectMode::Copy) {
            self.replace_correct_link_with_copy_if_needed(skill_id, agent)?;
        }
        assign.set_desired_enabled(skill_id, &agent_key, true, mode.map(|m| m.as_str()), &now)?;
        reconciler.reconcile_one(skill_id, &agent_key, force, &now)
    }

    /// Copy mode otherwise no-ops a correct source link; explicit copy must replace it.
    fn replace_correct_link_with_copy_if_needed(&self, skill_id: &str, agent: AgentId) -> Result<()> {
        let (source_dir, skills_root, target_dir) =
            self.resolve_projection_paths(skill_id, agent)?;
        match inspect_projection_target(&target_dir)? {
            TargetPresence::Link { .. } if link_resolves_to_source(&target_dir, &source_dir) => {
                let agent_key = AgentKey::from_agent_id(agent);
                unproject_with_ownership(
                    &skills_root,
                    skill_id,
                    &source_dir,
                    &target_dir,
                    &agent_key,
                )
            }
            _ => Ok(()),
        }
    }

    pub(super) fn disable_via_assignment_key(
        &self,
        skill_id: &str,
        agent_key: &AgentKey,
    ) -> Result<()> {
        let now = chrono_now();
        let (assign, reconciler) = self.assignment_stack()?;
        let lock = skill_lock_load(&self.source_root)?;
        let record = lock.get(skill_id);
        // Package may already exist; ensure so FK for assignment is satisfied.
        assign.ensure_package(skill_id, record, &now)?;
        assign.set_desired_enabled(skill_id, agent_key, false, None, &now)?;
        reconciler.reconcile_one(skill_id, agent_key, false, &now)
    }

    /// Resolve source + skills_root + target for a validated skill id / agent.
    pub(super) fn resolve_projection_paths(
        &self,
        skill_id: &str,
        agent: AgentId,
    ) -> Result<(PathBuf, PathBuf, PathBuf)> {
        let agent_key = AgentKey::from_agent_id(agent);
        if self.targets.get(&agent_key).is_none() && self.registry.get(agent).is_none() {
            return Err(AppError::NotFound(format!(
                "agent {} has no adapter",
                agent.as_str()
            )));
        }
        self.resolve_projection_paths_key(skill_id, &agent_key)
    }

    pub(super) fn resolve_projection_paths_key(
        &self,
        skill_id: &str,
        agent_key: &AgentKey,
    ) -> Result<(PathBuf, PathBuf, PathBuf)> {
        let target = self.targets.get(agent_key).ok_or_else(|| {
            AppError::Unsupported(format!("agent {} has no skill target", agent_key.as_str()))
        })?;
        if !target.supports_skills() {
            return Err(AppError::Unsupported(format!(
                "agent {} does not support skills",
                agent_key.as_str()
            )));
        }
        let skills_root = target.skills_root().ok_or_else(|| {
            AppError::Unsupported(format!(
                "agent {} has no skills directory",
                agent_key.as_str()
            ))
        })?;

        // skills_root must not be a symlink or non-directory; ancestors must not
        // be symlinks either (prevents target resolution via adapter-supplied links).
        validate_skills_root(&skills_root)?;

        let source_dir = self.source_root.join(skill_id);
        let target_dir = skills_root.join(skill_id);

        if !is_exact_child(&source_dir, &self.source_root, skill_id) {
            return Err(AppError::InvalidArg(format!(
                "skill source path escapes source root: {}",
                source_dir.display()
            )));
        }
        if !is_exact_child(&target_dir, &skills_root, skill_id) {
            return Err(AppError::InvalidArg(format!(
                "skill target path escapes skills root: {}",
                target_dir.display()
            )));
        }

        // Source path (including leaf) must not traverse links — truth is a real dir.
        ensure_no_symlink_in_existing_prefix(&source_dir)?;
        // Target skill directory itself may be a projection link/reparse point;
        // only ancestors of the leaf are forbidden to be links.
        ensure_no_symlink_in_ancestors(&target_dir)?;

        reject_source_target_overlap(&self.source_root, &source_dir, &skills_root, &target_dir)?;

        Ok((source_dir, skills_root, target_dir))
    }
}
