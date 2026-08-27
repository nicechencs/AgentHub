//! Path identity and containment guards shared by catalog, snapshot, restore, and delete.
//!
//! Lexical checks (`is_path_inside`) never follow symlinks. Canonical strict
//! containment (`ensure_existing_path_strictly_inside`) is required before
//! restore/delete mutate an on-disk snapshot.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::BackupRecord;
use crate::utils::paths::is_safe_path;

use super::BackupService;

#[derive(Debug)]
pub enum PathClass {
    RegularFile,
    Directory,
    Symlink,
    Missing,
    Other,
}

pub fn classify_path(path: &Path) -> Result<PathClass> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Ok(PathClass::Symlink),
        Ok(meta) if meta.is_file() => Ok(PathClass::RegularFile),
        Ok(meta) if meta.is_dir() => Ok(PathClass::Directory),
        Ok(_) => Ok(PathClass::Other),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(PathClass::Missing),
        Err(e) => Err(AppError::from(e)),
    }
}

pub fn ensure_regular_file(path: &Path) -> Result<()> {
    match classify_path(path)? {
        PathClass::RegularFile => Ok(()),
        PathClass::Missing => Err(AppError::NotFound(format!(
            "expected regular file missing: {}",
            path.display()
        ))),
        PathClass::Symlink => Err(AppError::InvalidArg(format!(
            "refusing symlink where regular file expected: {}",
            path.display()
        ))),
        PathClass::Directory => Err(AppError::InvalidArg(format!(
            "refusing directory where regular file expected: {}",
            path.display()
        ))),
        PathClass::Other => Err(AppError::InvalidArg(format!(
            "unsupported file type: {}",
            path.display()
        ))),
    }
}

/// Canonical strict containment for existing paths. This closes
/// ancestor-symlink escapes that lexical component checks cannot detect and
/// prevents callers from ever treating `root` itself as a snapshot.
pub fn ensure_existing_path_strictly_inside(
    path: &Path,
    root: &Path,
    code: &'static str,
) -> Result<()> {
    let canonical_root = std::fs::canonicalize(root)?;
    let canonical_path = std::fs::canonicalize(path)?;
    if !is_path_strictly_inside(&canonical_path, &canonical_root) {
        return Err(AppError::message(
            code,
            format!("path resolves outside backups_root: {}", path.display()),
        ));
    }
    Ok(())
}

/// Lexical containment: `child` equals `root` or is a strict descendant.
/// Does not follow symlinks; used as a safety guard before write/delete.
pub fn is_path_inside(child: &Path, root: &Path) -> bool {
    let child_c = normalize_components(child);
    let root_c = normalize_components(root);
    if root_c.is_empty() {
        return false;
    }
    child_c.starts_with(&root_c)
}

/// Exact lexical identity. This deliberately does not canonicalize or
/// normalize `.`/`..`: an indexed path must be the path AgentHub generated.
fn paths_equal_exact(left: &Path, right: &Path) -> bool {
    left.as_os_str() == right.as_os_str()
}

pub fn is_path_strictly_inside(child: &Path, root: &Path) -> bool {
    let child_c = normalize_components(child);
    let root_c = normalize_components(root);
    !root_c.is_empty() && child_c.len() > root_c.len() && child_c.starts_with(&root_c)
}

/// Reject symlinks in every existing component from `root` to `path`.
fn ensure_path_components_not_symlinks(path: &Path, root: &Path) -> Result<()> {
    let relative = path.strip_prefix(root).map_err(|_| {
        AppError::InvalidArg(format!(
            "backup path is outside backups_root: {}",
            path.display()
        ))
    })?;

    let mut current = root.to_path_buf();
    for component in std::iter::once(None).chain(relative.components().map(Some)) {
        if let Some(component) = component {
            current.push(component.as_os_str());
        }
        let metadata = std::fs::symlink_metadata(&current)?;
        if metadata.file_type().is_symlink() {
            return Err(AppError::InvalidArg(format!(
                "backup path contains a symlink component: {}",
                current.display()
            )));
        }
    }
    Ok(())
}

/// Refuse snapshots containing symlinks or special filesystem entries before
/// recursive restore/delete operations can traverse them.
fn ensure_tree_has_no_symlinks(root: &Path) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(AppError::InvalidArg(format!(
                    "backup snapshot contains a symlink: {}",
                    path.display()
                )));
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if !metadata.is_file() {
                return Err(AppError::InvalidArg(format!(
                    "backup snapshot contains an unsupported filesystem entry: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

pub fn normalize_components(path: &Path) -> Vec<std::ffi::OsString> {
    use std::path::Component;
    let mut out = Vec::new();
    for c in path.components() {
        match c {
            Component::Prefix(p) => out.push(p.as_os_str().to_os_string()),
            Component::RootDir => out.push(std::ffi::OsString::from("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(s) => out.push(s.to_os_string()),
        }
    }
    out
}

/// Build a collision-safe destination basename from a source path.
/// Rejects empty / traversal / unsafe names.
pub fn allocate_dest_name(src: &Path, occupied: &mut HashSet<String>) -> Result<String> {
    let raw = src.file_name().and_then(|s| s.to_str()).ok_or_else(|| {
        AppError::InvalidArg(format!(
            "backup source has no valid file name: {}",
            src.display()
        ))
    })?;

    let base = sanitize_basename(raw)?;
    if occupied.insert(base.clone()) {
        return Ok(base);
    }

    // Collision: settings.json → settings__2.json, settings__3.json, ...
    let (stem, ext) = split_stem_ext(&base);
    for n in 2u32.. {
        let candidate = if ext.is_empty() {
            format!("{stem}__{n}")
        } else {
            format!("{stem}__{n}.{ext}")
        };
        if !is_safe_path(Path::new(&candidate)) {
            continue;
        }
        if occupied.insert(candidate.clone()) {
            return Ok(candidate);
        }
    }
    Err(AppError::message(
        "backup.name",
        "could not allocate unique backup file name",
    ))
}

pub fn sanitize_basename(raw: &str) -> Result<String> {
    if raw.is_empty() || raw == "." || raw == ".." {
        return Err(AppError::InvalidArg(format!(
            "invalid backup file name: {raw:?}"
        )));
    }
    if raw.contains('/') || raw.contains('\\') || raw.contains('\0') {
        return Err(AppError::InvalidArg(format!(
            "backup file name must not contain path separators: {raw:?}"
        )));
    }
    if !is_safe_path(Path::new(raw)) {
        return Err(AppError::InvalidArg(format!(
            "unsafe backup file name: {raw:?}"
        )));
    }
    Ok(raw.to_string())
}

fn split_stem_ext(name: &str) -> (&str, &str) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() && !stem.contains('.') => {
            (stem, ext)
        }
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => (stem, ext),
        _ => (name, ""),
    }
}

impl BackupService {
    /// Derive the only legal on-disk location for a live backup:
    /// `backups_root/live/<agent>/<id>`.
    pub(super) fn expected_snapshot_dir(&self, record: &BackupRecord) -> Result<PathBuf> {
        let agent = record.agent_id.ok_or_else(|| {
            AppError::InvalidArg(format!(
                "backup {} has no agent_id; cannot resolve snapshot path",
                record.id
            ))
        })?;
        let id = sanitize_basename(&record.id)?;
        if id != record.id {
            return Err(AppError::InvalidArg(format!(
                "backup id must be a plain safe basename: {:?}",
                record.id
            )));
        }
        Ok(self
            .backups_root
            .join("live")
            .join(agent.as_str())
            .join(&id))
    }

    /// Lexical identity only: path string must equal the exact expected location.
    /// Rejects backups_root, category/agent parents, sibling backups, and escapes.
    pub(super) fn validate_snapshot_identity(&self, record: &BackupRecord) -> Result<PathBuf> {
        let expected = self.expected_snapshot_dir(record)?;
        let snapshot_dir = PathBuf::from(&record.path);

        if !is_safe_path(&snapshot_dir) {
            return Err(AppError::InvalidArg(format!(
                "unsafe backup path in index: {}",
                snapshot_dir.display()
            )));
        }
        if !paths_equal_exact(&snapshot_dir, &expected) {
            return Err(AppError::message(
                "backup.path",
                format!(
                    "backup path does not match expected snapshot location: {} (expected {})",
                    snapshot_dir.display(),
                    expected.display()
                ),
            ));
        }
        // Defense in depth: exact child of backups_root, never the root itself.
        if !is_path_strictly_inside(&snapshot_dir, &self.backups_root) {
            return Err(AppError::message(
                "backup.path",
                format!(
                    "backup path is not strictly inside backups_root: {}",
                    snapshot_dir.display()
                ),
            ));
        }
        Ok(snapshot_dir)
    }

    /// Existing directory checks for restore/delete: type, no symlink components
    /// under backups_root, no symlink descendants, strict canonical containment.
    pub(super) fn ensure_snapshot_safe_for_mutation(&self, snapshot_dir: &Path) -> Result<()> {
        match classify_path(snapshot_dir)? {
            PathClass::Directory => {}
            PathClass::Symlink => {
                return Err(AppError::InvalidArg(format!(
                    "backup path is a symlink: {}",
                    snapshot_dir.display()
                )));
            }
            PathClass::RegularFile => {
                return Err(AppError::InvalidArg(format!(
                    "backup path is a regular file: {}",
                    snapshot_dir.display()
                )));
            }
            PathClass::Missing => {
                return Err(AppError::NotFound(format!(
                    "backup snapshot directory missing: {}",
                    snapshot_dir.display()
                )));
            }
            PathClass::Other => {
                return Err(AppError::InvalidArg(format!(
                    "backup path is not a directory: {}",
                    snapshot_dir.display()
                )));
            }
        }

        ensure_path_components_not_symlinks(snapshot_dir, &self.backups_root)?;
        ensure_tree_has_no_symlinks(snapshot_dir)?;
        ensure_existing_path_strictly_inside(snapshot_dir, &self.backups_root, "backup.path")?;
        Ok(())
    }

    pub(super) fn validate_snapshot_dir(&self, record: &BackupRecord) -> Result<PathBuf> {
        let snapshot_dir = self.validate_snapshot_identity(record)?;
        self.ensure_snapshot_safe_for_mutation(&snapshot_dir)?;
        Ok(snapshot_dir)
    }
}
