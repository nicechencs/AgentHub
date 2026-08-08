//! Skill assignment reconciler (P12).
//!
//! Drives desired → observed projection. On failure keeps desired + last_error
//! and never claims `applied`. Unmanaged targets return conflict and are not
//! overwritten without `force`. Delete requires ownership proof (R03).

use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::platform::skills::fs_safe::{
    ensure_no_symlink_in_ancestors, ensure_no_symlink_in_existing_prefix, is_exact_child,
    reject_source_target_overlap, validate_skill_id, validate_skills_root,
};
use crate::platform::skills::ownership::{project_copy_with_ownership, unproject_with_ownership};
use crate::platform::skills::packages::validate_and_collect_source;
use crate::platform::skills::target::SkillTargetRegistry;
use crate::platform::AgentKey;
use crate::storage::SkillRepo;

/// Observed status strings stored in `skill_assignments.observed_status`.
pub mod observed {
    pub const PENDING: &str = "pending";
    pub const APPLIED: &str = "applied";
    pub const ABSENT: &str = "absent";
    pub const CONFLICT: &str = "conflict";
    pub const ERROR: &str = "error";
    pub const UNSUPPORTED: &str = "unsupported";
}

/// Reconciles one skill assignment (or all agents for a skill) against FS.
pub struct SkillReconciler {
    source_root: PathBuf,
    targets: SkillTargetRegistry,
    repo: SkillRepo,
}

impl SkillReconciler {
    pub fn new(source_root: PathBuf, targets: SkillTargetRegistry, repo: SkillRepo) -> Self {
        Self {
            source_root,
            targets,
            repo,
        }
    }

    pub fn source_root(&self) -> &Path {
        &self.source_root
    }

    pub fn targets(&self) -> &SkillTargetRegistry {
        &self.targets
    }

    pub fn repo(&self) -> &SkillRepo {
        &self.repo
    }

    /// Reconcile one (skill, agent) pair according to the assignment row.
    ///
    /// Missing assignment → no-op success.
    /// Target FS / conflict / unsupported failures are returned as `Err` after
    /// observed is written. Repo / `update_observed` failures also return `Err`
    /// (callers that need to distinguish use [`Self::reconcile_skill`]).
    pub fn reconcile_one(
        &self,
        skill_id: &str,
        agent_key: &AgentKey,
        force: bool,
        now: &str,
    ) -> Result<()> {
        self.reconcile_one_classified(skill_id, agent_key, force, now)?
    }

    /// Compatibility façade for callers that still use the built-in enum.
    pub fn reconcile_one_for_agent(
        &self,
        skill_id: &str,
        agent: AgentId,
        force: bool,
        now: &str,
    ) -> Result<()> {
        let agent_key = AgentKey::from_agent_id(agent);
        self.reconcile_one(skill_id, &agent_key, force, now)
    }

    /// Reconcile every assignment for a skill package.
    ///
    /// - Target filesystem / conflict / unsupported → per-agent `Err` in the
    ///   returned vec (observed already written).
    /// - Repo query or `update_observed` persistence failure → outer `Err`
    ///   (SkillService maps this to `skill.reconcile_partial`).
    pub fn reconcile_skill(
        &self,
        skill_id: &str,
        force: bool,
        now: &str,
    ) -> Result<Vec<(AgentKey, Result<()>)>> {
        let skill_id = validate_skill_id(skill_id)?;
        let assignments = self.repo.list_assignments_for_skill(skill_id)?;
        let mut out = Vec::new();
        for a in assignments {
            let agent_key = AgentKey::parse(a.agent_key.clone()).map_err(|err| {
                AppError::message(
                    "skill.assignment_data",
                    format!(
                        "invalid agent_key '{}' in skill assignment for '{}': {err}",
                        a.agent_key, skill_id
                    ),
                )
            })?;
            match self.reconcile_one_classified(skill_id, &agent_key, force, now) {
                Ok(target_outcome) => out.push((agent_key, target_outcome)),
                Err(infra) => return Err(infra),
            }
        }
        Ok(out)
    }

    /// Private classifier: outer `Err` = infrastructure; inner = target outcome.
    fn reconcile_one_classified(
        &self,
        skill_id: &str,
        agent_key: &AgentKey,
        force: bool,
        now: &str,
    ) -> Result<Result<()>> {
        let skill_id = validate_skill_id(skill_id)?;
        let Some(assignment) = self.repo.get_assignment(skill_id, agent_key.as_str())? else {
            return Ok(Ok(()));
        };

        let Some(target) = self.targets.get(agent_key) else {
            let outcome = Err(AppError::Unsupported(format!(
                "agent {} has no skill target",
                agent_key.as_str()
            )));
            return self.persist_observed(
                skill_id,
                agent_key.as_str(),
                observed::UNSUPPORTED,
                assignment.applied_revision.as_deref(),
                Some("agent has no skill target"),
                now,
                outcome,
            );
        };

        if !target.supports_skills() {
            let outcome = Err(AppError::Unsupported(format!(
                "agent {} does not support skills",
                agent_key.as_str()
            )));
            return self.persist_observed(
                skill_id,
                agent_key.as_str(),
                observed::UNSUPPORTED,
                assignment.applied_revision.as_deref(),
                Some("agent does not support skills"),
                now,
                outcome,
            );
        }

        let package = self.repo.get_package(skill_id)?;
        let revision = package.as_ref().map(|p| p.revision.as_str()).unwrap_or("1");

        if assignment.desired_enabled {
            match self.project_copy(skill_id, agent_key, force) {
                Ok(()) => self.persist_observed(
                    skill_id,
                    agent_key.as_str(),
                    observed::APPLIED,
                    Some(revision),
                    None,
                    now,
                    Ok(()),
                ),
                Err(e) if e.code() == "skill.conflict" => {
                    let msg = e.to_string();
                    self.persist_observed(
                        skill_id,
                        agent_key.as_str(),
                        observed::CONFLICT,
                        None,
                        Some(msg.as_str()),
                        now,
                        Err(e),
                    )
                }
                Err(e) => {
                    let msg = e.to_string();
                    self.persist_observed(
                        skill_id,
                        agent_key.as_str(),
                        observed::ERROR,
                        None,
                        Some(msg.as_str()),
                        now,
                        Err(e),
                    )
                }
            }
        } else {
            match self.unproject(skill_id, agent_key) {
                Ok(()) => self.persist_observed(
                    skill_id,
                    agent_key.as_str(),
                    observed::ABSENT,
                    None,
                    None,
                    now,
                    Ok(()),
                ),
                Err(e) if e.code() == "skill.conflict" => {
                    let msg = e.to_string();
                    self.persist_observed(
                        skill_id,
                        agent_key.as_str(),
                        observed::CONFLICT,
                        assignment.applied_revision.as_deref(),
                        Some(msg.as_str()),
                        now,
                        Err(e),
                    )
                }
                Err(e) => {
                    let msg = e.to_string();
                    self.persist_observed(
                        skill_id,
                        agent_key.as_str(),
                        observed::ERROR,
                        assignment.applied_revision.as_deref(),
                        Some(msg.as_str()),
                        now,
                        Err(e),
                    )
                }
            }
        }
    }

    /// Write observed, then return the target outcome.
    ///
    /// `update_observed` failure is infrastructure (`Err` outer).
    fn persist_observed(
        &self,
        skill_id: &str,
        agent_key: &str,
        status: &str,
        applied_revision: Option<&str>,
        last_error: Option<&str>,
        now: &str,
        outcome: Result<()>,
    ) -> Result<Result<()>> {
        self.repo.update_observed(
            skill_id,
            agent_key,
            status,
            applied_revision,
            last_error,
            now,
        )?;
        Ok(outcome)
    }

    /// Project source skill onto agent skills root as a copy (existing sync semantics).
    pub fn project_copy(&self, skill_id: &str, agent_key: &AgentKey, force: bool) -> Result<()> {
        let skill_id = validate_skill_id(skill_id)?;
        let (source_dir, skills_root, target_dir) =
            self.resolve_projection_paths(skill_id, agent_key)?;

        let source_files = validate_and_collect_source(&source_dir, skill_id)?;
        let package = self.repo.get_package(skill_id)?;
        let revision = package.as_ref().map(|p| p.revision.as_str()).unwrap_or("1");

        project_copy_with_ownership(
            &skills_root,
            skill_id,
            &source_dir,
            &target_dir,
            &source_files,
            force,
            revision,
            agent_key,
        )
    }

    /// Compatibility façade for callers that still use the built-in enum.
    pub fn project_copy_for_agent(
        &self,
        skill_id: &str,
        agent: AgentId,
        force: bool,
    ) -> Result<()> {
        let agent_key = AgentKey::from_agent_id(agent);
        self.project_copy(skill_id, &agent_key, force)
    }

    /// Remove projected skill directory for one agent (ownership-gated).
    pub fn unproject(&self, skill_id: &str, agent_key: &AgentKey) -> Result<()> {
        let skill_id = validate_skill_id(skill_id)?;
        let (source_dir, skills_root, target_dir) =
            self.resolve_projection_paths(skill_id, agent_key)?;

        unproject_with_ownership(&skills_root, skill_id, &source_dir, &target_dir, agent_key)
    }

    /// Compatibility façade for callers that still use the built-in enum.
    pub fn unproject_for_agent(&self, skill_id: &str, agent: AgentId) -> Result<()> {
        let agent_key = AgentKey::from_agent_id(agent);
        self.unproject(skill_id, &agent_key)
    }

    fn resolve_projection_paths(
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

        ensure_no_symlink_in_existing_prefix(&source_dir)?;
        ensure_no_symlink_in_ancestors(&target_dir)?;
        reject_source_target_overlap(&self.source_root, &source_dir, &skills_root, &target_dir)?;

        Ok((source_dir, skills_root, target_dir))
    }
}
