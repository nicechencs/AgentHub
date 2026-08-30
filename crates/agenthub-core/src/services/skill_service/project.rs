//! Project-workspace skill list / install / uninstall / preview.
//!
//! Isolated from the user shared library (`~/.agents/skills`). Writes go to
//! `<workspace>/.agents/skills`. Listing also surfaces skills already in known
//! per-agent project folders.

use std::fs;
use std::time::Instant;

use crate::error::{AppError, Result};
use crate::models::{InstalledSkill, Skill, SkillMarkdownPreview, SkillSourceRecord};
use crate::platform::skills::{
    acquire_skill_lock, acquire_skill_root_lock, canonical_project_skills_root, chrono_now,
    commit_skill_package, ensure_no_symlink_in_existing_prefix, inspect_projection_target,
    is_exact_child, is_link_or_reparse, list_project_workspace_skills, normalize_origin,
    project_skill_root, read_skill_md_file, read_skill_metadata, recycle_skill_dir,
    remove_projection_link, resolve_project_workspace, resolve_readable_skill_dir,
    validate_skill_id, validate_skills_root, validate_tree_entries_safe, PreparedSkillTree,
    SkillCommitFaults, SkillPackageService, SkillSourceService, TargetPresence,
    PROJECT_SKILLS_CANONICAL_ORIGIN,
};

use super::{log_skill_write_result, SkillService};

impl SkillService {
    /// List skills under a workspace (canonical `.agents/skills` plus known extras).
    pub fn list_project_skills(&self, workspace: &str) -> Result<Vec<InstalledSkill>> {
        let workspace = resolve_project_workspace(workspace)?;
        list_project_workspace_skills(&workspace)
    }

    /// Install a skill into `<workspace>/.agents/skills`. Does not touch the user
    /// shared library or agent projections.
    pub fn install_project_skill(
        &self,
        workspace: &str,
        source: &str,
        overwrite: bool,
    ) -> Result<Skill> {
        let started = Instant::now();
        let result = (|| {
            let source = source.trim();
            if source.is_empty() {
                return Err(AppError::InvalidArg(
                    "install source must not be empty".into(),
                ));
            }
            let workspace = resolve_project_workspace(workspace)?;
            let skills_root = canonical_project_skills_root(&workspace);
            let _global = acquire_skill_root_lock(&skills_root)?;

            let sources = SkillSourceService::new();
            let packages = SkillPackageService::new();
            let (package_dir, cleanup, source_kind, locator) = sources.materialize(source)?;
            let install_result = (|| {
                sources.ensure_skill_md(&package_dir)?;
                let skill_id = sources.infer_skill_id(&package_dir, source)?;
                let skill_id = validate_skill_id(&skill_id)?.to_string();
                let _skill_lock = acquire_skill_lock(&skills_root, &skill_id)?;

                let dest = skills_root.join(&skill_id);
                if dest.exists() && !overwrite {
                    return Err(AppError::InvalidArg(format!(
                        "skill '{skill_id}' already exists (pass overwrite to replace)"
                    )));
                }

                if !skills_root.exists() {
                    fs::create_dir_all(&skills_root)?;
                }
                ensure_no_symlink_in_existing_prefix(&skills_root)?;

                if dest.exists() {
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

                let committed = commit_skill_package(
                    &skills_root,
                    &skill_id,
                    PreparedSkillTree::Files(&files),
                    record,
                    None,
                    &now,
                    SkillCommitFaults::default(),
                )?;

                let (name, description) = read_skill_metadata(&committed.dest, &committed.skill_id);
                Ok(Skill {
                    id: committed.skill_id,
                    name,
                    description,
                    source_dir: committed.dest,
                    projections: vec![],
                })
            })();

            if let Some(dir) = cleanup {
                let _ = fs::remove_dir_all(dir);
            }
            install_result
        })();

        match &result {
            Ok(skill) => {
                log_skill_write_result("install_project", &skill.id, None, started, true, None)
            }
            Err(e) => log_skill_write_result("install_project", "-", None, started, false, Some(e)),
        }
        result
    }

    /// Recycle a project skill directory. `origin` defaults to `.agents/skills`.
    pub fn uninstall_project_skill(
        &self,
        workspace: &str,
        skill_id: &str,
        origin: Option<&str>,
    ) -> Result<()> {
        let started = Instant::now();
        let result = (|| {
            let workspace = resolve_project_workspace(workspace)?;
            let origin = normalize_origin(origin.unwrap_or(PROJECT_SKILLS_CANONICAL_ORIGIN))?;
            let skills_root = project_skill_root(&workspace, origin)?;
            validate_skills_root(&skills_root)?;
            let skill_id = validate_skill_id(skill_id)?.to_string();
            let target = skills_root.join(&skill_id);
            if !is_exact_child(&target, &skills_root, &skill_id) {
                return Err(AppError::InvalidArg(format!(
                    "skill target path escapes skills root: {}",
                    target.display()
                )));
            }
            let _global = acquire_skill_root_lock(&skills_root)?;
            let _skill_lock = acquire_skill_lock(&skills_root, &skill_id)?;
            match inspect_projection_target(&target)? {
                TargetPresence::Missing => Ok(()),
                TargetPresence::Link { .. } => remove_projection_link(&target),
                TargetPresence::Directory => {
                    validate_tree_entries_safe(&target, "project skill")?;
                    recycle_skill_dir(&target)?;
                    Ok(())
                }
                TargetPresence::Dangerous { kind } => Err(AppError::InvalidArg(format!(
                    "refusing to uninstall project skill ({kind}): {}",
                    target.display()
                ))),
            }
        })();
        match &result {
            Ok(()) => {
                log_skill_write_result("uninstall_project", skill_id, None, started, true, None)
            }
            Err(e) => {
                log_skill_write_result("uninstall_project", skill_id, None, started, false, Some(e))
            }
        }
        result
    }

    /// Read `SKILL.md` for a project skill. `origin` defaults to `.agents/skills`.
    pub fn read_project_skill_markdown(
        &self,
        workspace: &str,
        skill_id: &str,
        origin: Option<&str>,
    ) -> Result<SkillMarkdownPreview> {
        let workspace = resolve_project_workspace(workspace)?;
        let origin = normalize_origin(origin.unwrap_or(PROJECT_SKILLS_CANONICAL_ORIGIN))?;
        let skills_root = project_skill_root(&workspace, origin)?;
        validate_skills_root(&skills_root)?;
        let skill_id = validate_skill_id(skill_id)?;
        let skill_dir = skills_root.join(skill_id);
        if !is_exact_child(&skill_dir, &skills_root, skill_id) {
            return Err(AppError::InvalidArg(format!(
                "skill target path escapes skills root: {}",
                skill_dir.display()
            )));
        }
        let read_dir = resolve_readable_skill_dir(&skill_dir)?;
        let skill_md = read_dir.join("SKILL.md");
        if skill_md.is_file() {
            return read_skill_md_file(skill_id, &read_dir, &skill_md);
        }
        let alt = read_dir.join("skill.md");
        if alt.is_file() {
            return read_skill_md_file(skill_id, &read_dir, &alt);
        }
        Err(AppError::NotFound(format!(
            "SKILL.md not found in {}",
            skill_dir.display()
        )))
    }
}

#[cfg(test)]
mod tests;
