//! Create projection links with platform-specific fallbacks (junction / symlink / copy).

use std::fs;
use std::path::Path;

use crate::error::{AppError, Result};
use crate::models::SkillLinkKind;

use super::fs_safe::collect_regular_files;
use super::packages::write_skill_tree;

/// Create a projection link with platform fallbacks.
/// Returns (applied_kind, fell_back).
pub(crate) fn create_projection_link(
    source: &Path,
    target: &Path,
) -> Result<(SkillLinkKind, bool)> {
    let source = fs::canonicalize(source).map_err(|e| {
        AppError::InvalidArg(format!(
            "cannot canonicalize skill source {}: {e}",
            source.display()
        ))
    })?;

    #[cfg(windows)]
    {
        // 1) Junction (no admin)
        if create_windows_junction_runtime(&target, &source).is_ok() {
            return Ok((SkillLinkKind::Junction, false));
        }
        // 2) Directory symlink
        if std::os::windows::fs::symlink_dir(&source, target).is_ok() {
            return Ok((SkillLinkKind::Symlink, true));
        }
        // 3) Copy fallback
        let files = collect_regular_files(&source).map_err(|()| {
            AppError::InvalidArg("skill source tree is unsafe for copy fallback".into())
        })?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        write_skill_tree(target, &files)?;
        Ok((SkillLinkKind::None, true))
    }

    #[cfg(not(windows))]
    {
        if std::os::unix::fs::symlink(&source, target).is_ok() {
            return Ok((SkillLinkKind::Symlink, false));
        }
        let files = collect_regular_files(&source).map_err(|()| {
            AppError::InvalidArg("skill source tree is unsafe for copy fallback".into())
        })?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        write_skill_tree(target, &files)?;
        Ok((SkillLinkKind::None, true))
    }
}

#[cfg(windows)]
pub(crate) fn create_windows_junction_runtime(link: &Path, target: &Path) -> std::io::Result<()> {
    use std::process::Command;
    use crate::utils::process::apply_no_window;
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)?;
    }
    let target_s = target.to_string_lossy().to_string();
    let link_arg = link.to_string_lossy().to_string();
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", "mklink", "/J", &link_arg, &target_s]);
    apply_no_window(&mut cmd);
    let status = cmd.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "mklink /J failed",
        ))
    }
}
