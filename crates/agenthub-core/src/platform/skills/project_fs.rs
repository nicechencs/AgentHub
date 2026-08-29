//! Project-workspace skill roots (`.agents/skills` plus known per-agent folders).
//!
//! Canonical writes go to `<workspace>/.agents/skills`. Listing also discovers
//! skills already sitting in well-known project folders such as `.claude/skills`.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::{InstalledSkill, SkillMapStatus};

use super::fs_safe::{is_link_or_reparse, resolve_link_path, validate_skill_id};
use super::yaml::read_skill_metadata;

/// Portable project skill library (Agent Skills spec + AgentHub user library counterpart).
pub(crate) const PROJECT_SKILLS_CANONICAL_ORIGIN: &str = ".agents/skills";

/// Relative skill roots scanned under a workspace. First entry is the write target.
pub(crate) const PROJECT_SKILL_RELATIVE_ROOTS: &[&str] = &[
    PROJECT_SKILLS_CANONICAL_ORIGIN,
    ".claude/skills",
    ".codex/skills",
    ".cursor/skills",
    ".cursor/skills-cursor",
    ".grok/skills",
    ".dsh/skills",
    ".zcode/skills",
];

/// Normalize an absolute existing workspace directory. Does not follow the path
/// through `canonicalize` (Windows `\\?\` prefixes break later containment checks).
pub(crate) fn resolve_project_workspace(raw: &str) -> Result<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(AppError::InvalidArg(
            "workspace path must not be empty".into(),
        ));
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(AppError::InvalidArg(
            "workspace path must be absolute".into(),
        ));
    }
    let meta = fs::metadata(&path)
        .map_err(|_| AppError::NotFound(format!("workspace path not found: {raw}")))?;
    if !meta.is_dir() {
        return Err(AppError::InvalidArg(
            "workspace path is not a directory".into(),
        ));
    }
    Ok(normalize_existing_dir(&path))
}

pub(crate) fn canonical_project_skills_root(workspace: &Path) -> PathBuf {
    workspace.join(".agents").join("skills")
}

pub(crate) fn project_skill_root(workspace: &Path, origin: &str) -> Result<PathBuf> {
    let origin = normalize_origin(origin)?;
    let mut dir = workspace.to_path_buf();
    for part in origin.split('/') {
        dir.push(part);
    }
    Ok(dir)
}

pub(crate) fn normalize_origin(origin: &str) -> Result<&str> {
    let origin = origin.trim();
    if PROJECT_SKILL_RELATIVE_ROOTS.contains(&origin) {
        return Ok(origin);
    }
    Err(AppError::InvalidArg(format!(
        "unknown project skill origin: {origin}"
    )))
}

pub(crate) fn list_project_workspace_skills(workspace: &Path) -> Result<Vec<InstalledSkill>> {
    let mut out = Vec::new();
    for origin in PROJECT_SKILL_RELATIVE_ROOTS {
        let root = project_skill_root(workspace, origin)?;
        out.extend(scan_project_skill_root(&root, origin)?);
    }
    Ok(out)
}

pub(crate) fn scan_project_skill_root(root: &Path, origin: &str) -> Result<Vec<InstalledSkill>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let meta = match fs::symlink_metadata(root) {
        Ok(m) => m,
        Err(_) => return Ok(Vec::new()),
    };
    if !meta.is_dir() {
        return Ok(Vec::new());
    }
    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    for ent in entries {
        let ent = match ent {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = ent.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if validate_skill_id(&name).is_err() {
            continue;
        }
        let path = ent.path();
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_dirish = meta.is_dir() || is_link_or_reparse(&meta) || meta.file_type().is_symlink();
        if !is_dirish {
            continue;
        }
        let skill_md_ok = path.join("SKILL.md").is_file()
            || resolve_link_path(&path)
                .map(|resolved| resolved.join("SKILL.md").is_file())
                .unwrap_or(false);
        if !skill_md_ok {
            continue;
        }
        let (display, description) = read_skill_metadata(&path, &name);
        out.push(InstalledSkill {
            id: name,
            name: display,
            description,
            source_dir: path,
            root_label: origin.to_string(),
            root_dir: root.to_path_buf(),
            origin: origin.to_string(),
            projectable: false,
            map_status: SkillMapStatus::Available,
            content_hash: None,
            source: None,
            projections: vec![],
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.origin.cmp(&b.origin)));
    Ok(out)
}

fn normalize_existing_dir(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(c.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(s) => out.push(s),
        }
    }
    out
}

#[cfg(test)]
mod tests;
