//! Skill source materialization (local dir / zip / git clone into temp).
//!
//! Network-facing git clone for *install* still runs here; *update* of git
//! sources uses [`super::git_update::atomic_git_skill_update`] (no live pull).

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};

/// Owns install-source materialization (path / zip / git → package dir).
#[derive(Debug, Default, Clone, Copy)]
pub struct SkillSourceService;

impl SkillSourceService {
    pub fn new() -> Self {
        Self
    }

    /// Returns `(package_dir, optional_cleanup_dir, kind, locator)`.
    pub fn materialize(&self, source: &str) -> Result<(PathBuf, Option<PathBuf>, String, String)> {
        materialize_install_package(source)
    }

    pub fn ensure_skill_md(&self, dir: &Path) -> Result<()> {
        ensure_skill_md(dir)
    }

    pub fn infer_skill_id(&self, package_dir: &Path, source: &str) -> Result<String> {
        infer_skill_id(package_dir, source)
    }
}

pub(crate) fn ensure_skill_md(dir: &Path) -> Result<()> {
    if dir.join("SKILL.md").is_file() {
        Ok(())
    } else {
        Err(AppError::InvalidArg(format!(
            "skill package must contain SKILL.md: {}",
            dir.display()
        )))
    }
}

pub(crate) fn infer_skill_id(package_dir: &Path, source: &str) -> Result<String> {
    if let Some(name) = package_dir.file_name().and_then(|s| s.to_str()) {
        if !name.is_empty() && name != "." && name != ".." {
            return Ok(name.to_string());
        }
    }
    // Fall back to last path segment of the source string.
    let trimmed = source.trim_end_matches(['/', '\\']);
    let base = trimmed
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("skill")
        .trim_end_matches(".git");
    if base.is_empty() {
        return Err(AppError::InvalidArg(
            "could not infer skill id from install source".into(),
        ));
    }
    Ok(base.to_string())
}

/// Returns (package_dir, optional_cleanup_dir, kind, locator).
pub(crate) fn materialize_install_package(
    source: &str,
) -> Result<(PathBuf, Option<PathBuf>, String, String)> {
    let path = PathBuf::from(source);
    if path.is_dir() {
        return Ok((
            path.canonicalize().unwrap_or(path),
            None,
            "local".into(),
            source.to_string(),
        ));
    }
    if path.is_file() {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "zip" {
            let tmp = tempfile::tempdir()
                .map_err(|e| AppError::message("skill.install", format!("tempdir failed: {e}")))?;
            let dest = tmp.path().to_path_buf();
            extract_zip_file(&path, &dest)?;
            // If zip has a single top-level dir, use it as package root.
            let package = single_child_dir(&dest).unwrap_or(dest.clone());
            // Leak tempdir into PathBuf ownership via keep; caller cleans.
            let cleanup = dest;
            std::mem::forget(tmp);
            return Ok((package, Some(cleanup), "zip".into(), source.to_string()));
        }
        return Err(AppError::InvalidArg(format!(
            "unsupported install file (expected directory or .zip): {source}"
        )));
    }

    // git URL heuristic
    let looks_git = source.starts_with("git@")
        || source.starts_with("http://")
        || source.starts_with("https://")
        || source.ends_with(".git")
        || source.contains("github.com")
        || source.contains("gitlab.com");
    if looks_git {
        let tmp = tempfile::tempdir()
            .map_err(|e| AppError::message("skill.install", format!("tempdir failed: {e}")))?;
        let dest = tmp.path().join("repo");
        let status = std::process::Command::new("git")
            .args(["clone", "--depth", "1", source])
            .arg(&dest)
            .status()
            .map_err(|e| {
                AppError::message(
                    "skill.install",
                    format!(
                        "git clone failed (is git installed? doctor / Agents 页运行环境可检测并安装 Git): {e}"
                    ),
                )
            })?;
        if !status.success() {
            return Err(AppError::message(
                "skill.install",
                format!(
                    "git clone failed for {source} (check network, or install Git via env/runtime)"
                ),
            ));
        }
        let cleanup = tmp.path().to_path_buf();
        std::mem::forget(tmp);
        return Ok((dest, Some(cleanup), "git".into(), source.to_string()));
    }

    Err(AppError::InvalidArg(format!(
        "install source not found (path/zip/git): {source}"
    )))
}

pub(crate) fn single_child_dir(root: &Path) -> Option<PathBuf> {
    let mut dirs = Vec::new();
    for ent in fs::read_dir(root).ok()? {
        let ent = ent.ok()?;
        let p = ent.path();
        if p.file_name()
            .map(|n| n.to_string_lossy().starts_with('.'))
            .unwrap_or(true)
        {
            continue;
        }
        if p.is_dir() {
            dirs.push(p);
        } else {
            return None;
        }
    }
    if dirs.len() == 1 {
        Some(dirs.remove(0))
    } else {
        None
    }
}

pub(crate) fn extract_zip_file(zip: &Path, dest: &Path) -> Result<()> {
    super::zip_extract::extract_zip_file(zip, dest)
}
