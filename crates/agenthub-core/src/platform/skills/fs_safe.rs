//! Shared filesystem safety helpers for skills (package place + projection).
//!
//! Extracted from SkillService so package ownership can live in `packages`
//! without depending on the service façade.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::SkillLinkKind;

pub(crate) fn validate_skill_id(skill_id: &str) -> Result<&str> {
    if skill_id.is_empty() {
        return Err(AppError::InvalidArg("skill id must not be empty".into()));
    }
    if skill_id == "." || skill_id == ".." {
        return Err(AppError::InvalidArg(format!(
            "invalid skill id: {skill_id:?}"
        )));
    }
    if skill_id.contains('\0') {
        return Err(AppError::InvalidArg("skill id must not contain NUL".into()));
    }
    if skill_id.contains('/') || skill_id.contains('\\') {
        return Err(AppError::InvalidArg(format!(
            "skill id must be a single path component (no separators): {skill_id:?}"
        )));
    }

    validate_safe_path_component(skill_id)?;

    let path = Path::new(skill_id);
    let mut comps = path.components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(name)), None) => {
            let name = name.to_string_lossy();
            if name != skill_id {
                return Err(AppError::InvalidArg(format!(
                    "invalid skill id: {skill_id:?}"
                )));
            }
            Ok(skill_id)
        }
        (Some(Component::Prefix(_) | Component::RootDir), _) => Err(AppError::InvalidArg(format!(
            "skill id must not be absolute or rooted: {skill_id:?}"
        ))),
        (Some(Component::ParentDir | Component::CurDir), _) => Err(AppError::InvalidArg(format!(
            "invalid skill id: {skill_id:?}"
        ))),
        _ => Err(AppError::InvalidArg(format!(
            "invalid skill id: {skill_id:?}"
        ))),
    }
}

/// Portable single-component safety (Windows reserved names + forbidden chars).
pub(crate) fn validate_safe_path_component(name: &str) -> Result<()> {
    if name.is_empty() || name == "." || name == ".." {
        return Err(AppError::InvalidArg(format!(
            "invalid path component: {name:?}"
        )));
    }
    if name.chars().any(|c| c.is_control() || c == '\0') {
        return Err(AppError::InvalidArg(format!(
            "path component must not contain control characters: {name:?}"
        )));
    }
    // Colon (ADS), angle brackets, quote, pipe, wildcards, and other Windows-illegal chars.
    if name
        .chars()
        .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\'))
    {
        return Err(AppError::InvalidArg(format!(
            "path component contains reserved or non-portable characters: {name:?}"
        )));
    }
    // Trailing dot/space are stripped/aliased on Windows.
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(AppError::InvalidArg(format!(
            "path component must not end with '.' or space: {name:?}"
        )));
    }
    // Reserved device basenames (also with extension: CON.txt → stem CON).
    let stem = name.split_once('.').map(|(s, _)| s).unwrap_or(name);
    if is_windows_reserved_device(stem) {
        return Err(AppError::InvalidArg(format!(
            "path component uses reserved device name: {name:?}"
        )));
    }
    Ok(())
}

pub(crate) fn is_windows_reserved_device(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

/// Treat every link-like Windows reparse point as unsafe, not only symbolic links.
///
/// Junctions and other name-surrogate reparse points can redirect traversal even
/// when `FileType::is_symlink()` is false. On Unix, the ordinary symlink bit is
/// the complete link-like classification exposed by `symlink_metadata`.
pub(crate) fn is_link_or_reparse(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }

    #[cfg(not(windows))]
    {
        false
    }
}

/// Lexical containment: `child` equals `root` or is a strict descendant.
pub(crate) fn is_path_inside(child: &Path, root: &Path) -> bool {
    let child_c = normalize_path_components(child);
    let root_c = normalize_path_components(root);
    if root_c.is_empty() {
        return false;
    }
    child_c.starts_with(&root_c)
}

pub(crate) fn paths_equal_lexical(a: &Path, b: &Path) -> bool {
    normalize_path_components(a) == normalize_path_components(b)
}

/// Resolve a skill package directory for reading.
///
/// The **leaf** may be a junction/symlink (catalog scan and Windows projections
/// already treat those as installed). Missing / non-dir → NotFound.
/// Dangling link → InvalidArg. Callers must still ensure `skill_dir` is an
/// exact child of the skills root so `skill_id` cannot traverse.
pub(crate) fn resolve_readable_skill_dir(skill_dir: &Path) -> Result<PathBuf> {
    let meta = match fs::symlink_metadata(skill_dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(AppError::NotFound(format!(
                "skill directory not found: {}",
                skill_dir.display()
            )));
        }
        Err(e) => return Err(AppError::from(e)),
        Ok(m) => m,
    };
    if is_link_or_reparse(&meta) {
        return resolve_link_path(skill_dir).ok_or_else(|| {
            AppError::InvalidArg(format!(
                "skill directory link is unresolvable: {}",
                skill_dir.display()
            ))
        });
    }
    if meta.is_dir() {
        return Ok(skill_dir.to_path_buf());
    }
    Err(AppError::NotFound(format!(
        "skill directory not found: {}",
        skill_dir.display()
    )))
}

/// `child` must be exactly `root/skill_id` (one Normal component under root).
pub(crate) fn is_exact_child(child: &Path, root: &Path, skill_id: &str) -> bool {
    if !is_path_inside(child, root) {
        return false;
    }
    let child_c = normalize_path_components(child);
    let root_c = normalize_path_components(root);
    if child_c.len() != root_c.len() + 1 {
        return false;
    }
    child_c
        .last()
        .is_some_and(|c| c.to_string_lossy() == skill_id)
}

pub(crate) fn normalize_path_components(path: &Path) -> Vec<std::ffi::OsString> {
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

/// Reject skills_root that is a symlink or non-directory; validate ancestors.
pub(crate) fn validate_skills_root(skills_root: &Path) -> Result<()> {
    ensure_no_symlink_in_existing_prefix(skills_root)?;
    match fs::symlink_metadata(skills_root) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // Missing root is OK (created later). Ancestors already checked.
            Ok(())
        }
        Err(e) => Err(AppError::from(e)),
        Ok(meta) => {
            if is_link_or_reparse(&meta) {
                return Err(AppError::InvalidArg(format!(
                    "skills_dir must not be a symlink or reparse point: {}",
                    skills_root.display()
                )));
            }
            if !meta.is_dir() {
                return Err(AppError::InvalidArg(format!(
                    "skills_dir must be a directory: {}",
                    skills_root.display()
                )));
            }
            Ok(())
        }
    }
}

/// Walk existing path prefixes and refuse any symlink / reparse component.
///
/// Stops at the first missing component (remaining path cannot traverse a link).
pub(crate) fn ensure_no_symlink_in_existing_prefix(path: &Path) -> Result<()> {
    let mut acc = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Prefix(p) => {
                acc.push(p.as_os_str());
            }
            Component::RootDir => {
                acc.push(Component::RootDir.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                acc.pop();
            }
            Component::Normal(s) => {
                acc.push(s);
                match fs::symlink_metadata(&acc) {
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {
                        // Remainder does not exist — no further traversal possible.
                        return Ok(());
                    }
                    Err(e) => {
                        return Err(AppError::InvalidArg(format!(
                            "unreadable path component {}: {e}",
                            acc.display()
                        )));
                    }
                    Ok(meta) if is_link_or_reparse(&meta) => {
                        return Err(AppError::InvalidArg(format!(
                            "path must not traverse a symlink or reparse point: {}",
                            acc.display()
                        )));
                    }
                    Ok(_) => {}
                }
            }
        }
    }
    Ok(())
}

/// Like [`ensure_no_symlink_in_existing_prefix`], but skips the final path component.
///
/// Used for projection targets where the leaf skill directory itself may be a
/// junction/symlink pointing at the source skill.
pub(crate) fn ensure_no_symlink_in_ancestors(path: &Path) -> Result<()> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            ensure_no_symlink_in_existing_prefix(parent)
        }
        _ => Ok(()),
    }
}

/// Classify a path's link kind (none / symlink / junction / hardlink).
pub(crate) fn detect_link_kind(path: &Path, meta: &fs::Metadata) -> SkillLinkKind {
    if !is_link_or_reparse(meta) {
        return SkillLinkKind::None;
    }

    #[cfg(windows)]
    {
        match windows_reparse_tag(path) {
            Some(IO_REPARSE_TAG_MOUNT_POINT) => SkillLinkKind::Junction,
            Some(IO_REPARSE_TAG_SYMLINK) => SkillLinkKind::Symlink,
            // Name-surrogate reparse we cannot classify: prefer junction (Windows
            // projection default) over treating as generic symlink.
            _ => SkillLinkKind::Junction,
        }
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        SkillLinkKind::Symlink
    }
}

#[cfg(windows)]
pub(crate) const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
#[cfg(windows)]
pub(crate) const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;

/// Read the Windows reparse tag without following the link. `None` on failure.
#[cfg(windows)]
pub(crate) fn windows_reparse_tag(path: &Path) -> Option<u32> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

    #[repr(C)]
    struct ReparseDataBufferHeader {
        reparse_tag: u32,
        reparse_data_length: u16,
        reserved: u16,
    }

    const FSCTL_GET_REPARSE_POINT: u32 = 0x0009_00A8;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const OPEN_EXISTING: u32 = 3;
    const FILE_SHARE_READ: u32 = 0x1;
    const FILE_SHARE_WRITE: u32 = 0x2;
    const FILE_SHARE_DELETE: u32 = 0x4;
    const GENERIC_READ: u32 = 0x8000_0000;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileW(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security: *mut core::ffi::c_void,
            creation_disposition: u32,
            flags_and_attrs: u32,
            template: *mut core::ffi::c_void,
        ) -> *mut core::ffi::c_void;
        fn DeviceIoControl(
            handle: *mut core::ffi::c_void,
            io_control_code: u32,
            in_buffer: *mut core::ffi::c_void,
            in_buffer_size: u32,
            out_buffer: *mut core::ffi::c_void,
            out_buffer_size: u32,
            bytes_returned: *mut u32,
            overlapped: *mut core::ffi::c_void,
        ) -> i32;
    }

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: path is NUL-terminated; handle closed via OwnedHandle.
    let raw = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if raw == (-1isize as *mut core::ffi::c_void) || raw.is_null() {
        return None;
    }
    // SAFETY: valid handle from CreateFileW.
    let handle = unsafe { OwnedHandle::from_raw_handle(raw as _) };
    let mut buf = [0u8; 16 * 1024];
    let mut returned = 0u32;
    let ok = unsafe {
        DeviceIoControl(
            handle.as_raw_handle() as *mut _,
            FSCTL_GET_REPARSE_POINT,
            std::ptr::null_mut(),
            0,
            buf.as_mut_ptr() as *mut _,
            buf.len() as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 || returned < std::mem::size_of::<ReparseDataBufferHeader>() as u32 {
        return None;
    }
    // SAFETY: buffer filled with at least the header.
    let header = unsafe { &*(buf.as_ptr() as *const ReparseDataBufferHeader) };
    Some(header.reparse_tag)
}

/// Resolve a link (symlink/junction) to its final path without requiring the
/// path to already be known as a link.
pub(crate) fn resolve_link_path(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok()
}

/// Canonicalize a real (non-link) path when present.
pub(crate) fn try_canonicalize_real(path: &Path) -> Option<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !is_link_or_reparse(&meta) => fs::canonicalize(path).ok(),
        // Source may itself be opened via canonicalize even if intermediate
        // resolution is needed — prefer plain canonicalize when real.
        Ok(_) => None,
        Err(_) => fs::canonicalize(path).ok(),
    }
}

/// Whether a projection link resolves to the same directory as `source`.
pub(crate) fn link_resolves_to_source(link: &Path, source: &Path) -> bool {
    let Some(resolved) = resolve_link_path(link) else {
        return false;
    };
    let Some(src) = try_canonicalize_real(source).or_else(|| fs::canonicalize(source).ok()) else {
        return false;
    };
    paths_equal_os(&resolved, &src)
}

/// OS-aware path equality (case-insensitive + separator fold on Windows).
pub(crate) fn paths_equal_os(a: &Path, b: &Path) -> bool {
    if paths_equal_lexical(a, b) {
        return true;
    }
    #[cfg(windows)]
    {
        let norm = |p: &Path| {
            p.to_string_lossy()
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_ascii_lowercase()
        };
        norm(a) == norm(b)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Remove a projection **link** without following it into the source tree.
///
/// - Unix: `remove_file` unlinks the symlink inode.
/// - Windows: `remove_dir` removes junctions / directory symlinks; fall back to
///   `remove_file` for file symlinks. Never uses `remove_dir_all` on a link root.
pub(crate) fn remove_projection_link(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(AppError::from)?;
    if !is_link_or_reparse(&meta) {
        return Err(AppError::InvalidArg(format!(
            "expected a link projection at {}",
            path.display()
        )));
    }

    #[cfg(windows)]
    {
        // Directory junctions and directory symlinks: RemoveDirectory.
        match fs::remove_dir(path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(_) => {}
        }
        // File symlink fallback.
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::from(e)),
        }
    }

    #[cfg(not(windows))]
    {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::from(e)),
        }
    }
}

/// Reject configs where target/source alias or overlap could mutate the source.
pub(crate) fn reject_source_target_overlap(
    source_root: &Path,
    source_dir: &Path,
    skills_root: &Path,
    target_dir: &Path,
) -> Result<()> {
    // Lexical defenses (work when paths are missing).
    if paths_equal_lexical(source_dir, target_dir) {
        return Err(AppError::InvalidArg(format!(
            "skill target coincides with source skill path: {}",
            target_dir.display()
        )));
    }
    if paths_equal_lexical(source_root, skills_root) {
        return Err(AppError::InvalidArg(format!(
            "skills_dir must not be the same as skill source root: {}",
            skills_root.display()
        )));
    }
    if is_path_inside(target_dir, source_dir) {
        return Err(AppError::InvalidArg(format!(
            "skill target is inside source skill tree: {}",
            target_dir.display()
        )));
    }
    if is_path_inside(source_dir, target_dir) {
        return Err(AppError::InvalidArg(format!(
            "skill source is inside target tree: {}",
            source_dir.display()
        )));
    }
    // Projecting into the source tree (or sourcing from skills root) can delete/overwrite truth.
    if is_path_inside(target_dir, source_root) {
        return Err(AppError::InvalidArg(format!(
            "skill target is inside skill source root: {}",
            target_dir.display()
        )));
    }
    if is_path_inside(source_dir, skills_root) {
        return Err(AppError::InvalidArg(format!(
            "skill source is inside skills_dir: {}",
            source_dir.display()
        )));
    }
    if is_path_inside(skills_root, source_dir) {
        return Err(AppError::InvalidArg(format!(
            "skills_dir is inside source skill tree: {}",
            skills_root.display()
        )));
    }
    if is_path_inside(source_root, target_dir) {
        return Err(AppError::InvalidArg(format!(
            "skill source root is inside target tree: {}",
            source_root.display()
        )));
    }

    // Canonical checks when paths already exist (resolves same-file aliases).
    reject_canonical_overlap(source_dir, target_dir, "source skill", "target")?;
    if let (Ok(sr), Ok(sk)) = (
        try_canonicalize_existing(source_root),
        try_canonicalize_existing(skills_root),
    ) {
        if paths_equal_lexical(&sr, &sk) {
            return Err(AppError::InvalidArg(format!(
                "skills_dir canonical path equals skill source root: {}",
                sk.display()
            )));
        }
        // skills_root inside a source skill directory
        if let Ok(sd) = try_canonicalize_existing(source_dir) {
            if is_path_inside(&sk, &sd) {
                return Err(AppError::InvalidArg(format!(
                    "skills_dir is inside canonical source skill tree: {}",
                    sk.display()
                )));
            }
        }
        // target under source root canonically
        if let Ok(td) = try_canonicalize_existing(target_dir) {
            if is_path_inside(&td, &sr) {
                return Err(AppError::InvalidArg(format!(
                    "skill target is inside canonical source root: {}",
                    td.display()
                )));
            }
        }
    }

    Ok(())
}

pub(crate) fn reject_canonical_overlap(
    a: &Path,
    b: &Path,
    a_label: &str,
    b_label: &str,
) -> Result<()> {
    let (Ok(ca), Ok(cb)) = (try_canonicalize_existing(a), try_canonicalize_existing(b)) else {
        return Ok(());
    };
    if paths_equal_lexical(&ca, &cb) {
        return Err(AppError::InvalidArg(format!(
            "{a_label} and {b_label} resolve to the same path: {}",
            ca.display()
        )));
    }
    if is_path_inside(&cb, &ca) {
        return Err(AppError::InvalidArg(format!(
            "{b_label} is inside {a_label} tree: {}",
            cb.display()
        )));
    }
    if is_path_inside(&ca, &cb) {
        return Err(AppError::InvalidArg(format!(
            "{a_label} is inside {b_label} tree: {}",
            ca.display()
        )));
    }
    Ok(())
}

pub(crate) fn try_canonicalize_existing(path: &Path) -> std::result::Result<PathBuf, ()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !is_link_or_reparse(&meta) => fs::canonicalize(path).map_err(|_| ()),
        // Do not canonicalize through symlinks for safety comparisons.
        _ => Err(()),
    }
}

#[derive(Debug)]
pub(crate) enum TargetPresence {
    Missing,
    /// Real (non-link) directory.
    Directory,
    /// Projection link (symlink / junction); safe to remove without following.
    #[allow(dead_code)]
    Link {
        kind: SkillLinkKind,
    },
    Dangerous {
        kind: &'static str,
    },
}

pub(crate) fn inspect_projection_target(target: &Path) -> Result<TargetPresence> {
    match fs::symlink_metadata(target) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(TargetPresence::Missing),
        Err(e) => Err(AppError::from(e)),
        Ok(meta) => {
            if is_link_or_reparse(&meta) {
                let kind = detect_link_kind(target, &meta);
                return Ok(TargetPresence::Link { kind });
            }
            let ft = meta.file_type();
            if ft.is_dir() {
                return Ok(TargetPresence::Directory);
            }
            if ft.is_file() {
                return Ok(TargetPresence::Dangerous {
                    kind: "regular file",
                });
            }
            Ok(TargetPresence::Dangerous {
                kind: "special/non-regular entry",
            })
        }
    }
}

/// Walk an existing directory tree and reject symlink / special / unreadable entries.
///
/// Does not follow directory symlinks (they fail as symlink entries).
pub(crate) fn validate_tree_entries_safe(root: &Path, label: &str) -> Result<()> {
    let meta = fs::symlink_metadata(root).map_err(|e| {
        AppError::InvalidArg(format!(
            "{label} root is unreadable ({}): {e}",
            root.display()
        ))
    })?;
    if is_link_or_reparse(&meta) || !meta.is_dir() {
        return Err(AppError::InvalidArg(format!(
            "{label} root is not a safe directory: {}",
            root.display()
        )));
    }
    validate_tree_entries_safe_rec(root, label)
}

pub(crate) fn validate_tree_entries_safe_rec(dir: &Path, label: &str) -> Result<()> {
    let entries = fs::read_dir(dir).map_err(|e| {
        AppError::InvalidArg(format!(
            "{label} directory is unreadable ({}): {e}",
            dir.display()
        ))
    })?;
    for ent in entries {
        let ent = ent.map_err(|e| {
            AppError::InvalidArg(format!(
                "{label} entry is unreadable under {}: {e}",
                dir.display()
            ))
        })?;
        let path = ent.path();
        let meta = fs::symlink_metadata(&path).map_err(|e| {
            AppError::InvalidArg(format!(
                "{label} entry is unreadable ({}): {e}",
                path.display()
            ))
        })?;
        let ft = meta.file_type();
        if is_link_or_reparse(&meta) {
            return Err(AppError::InvalidArg(format!(
                "{label} tree contains a symlink or reparse point: {}",
                path.display()
            )));
        }
        if ft.is_dir() {
            validate_tree_entries_safe_rec(&path, label)?;
            continue;
        }
        if !ft.is_file() {
            return Err(AppError::InvalidArg(format!(
                "{label} tree contains a special/non-regular entry: {}",
                path.display()
            )));
        }
        // Ensure regular files are readable (permission / lock failures).
        fs::File::open(&path).map_err(|e| {
            AppError::InvalidArg(format!(
                "{label} tree contains an unreadable file ({}): {e}",
                path.display()
            ))
        })?;
    }
    Ok(())
}

pub(crate) fn collect_regular_files(
    root: &Path,
) -> std::result::Result<BTreeMap<String, Vec<u8>>, ()> {
    let root_meta = fs::symlink_metadata(root).map_err(|_| ())?;
    if is_link_or_reparse(&root_meta) || !root_meta.is_dir() {
        return Err(());
    }

    let mut out = BTreeMap::new();
    collect_regular_files_rec(root, root, &mut out)?;

    // A source tree can be projected to agents on case-insensitive filesystems.
    // Reject portable-name aliases up front instead of silently overwriting one
    // file while materializing the projection.
    let mut portable_keys = BTreeMap::new();
    for key in out.keys() {
        let folded = key.to_lowercase();
        if portable_keys.insert(folded, key).is_some() {
            return Err(());
        }
    }

    Ok(out)
}

pub(crate) fn collect_regular_files_rec(
    root: &Path,
    dir: &Path,
    out: &mut BTreeMap<String, Vec<u8>>,
) -> std::result::Result<(), ()> {
    let entries = fs::read_dir(dir).map_err(|_| ())?;
    for ent in entries {
        let ent = ent.map_err(|_| ())?;
        let path = ent.path();

        // Prefer symlink_metadata so we never follow links outside the root.
        let meta = fs::symlink_metadata(&path).map_err(|_| ())?;
        let ft = meta.file_type();

        if is_link_or_reparse(&meta) {
            // Symlinks are unsafe for equality — conservative conflict.
            return Err(());
        }
        if ft.is_dir() {
            collect_regular_files_rec(root, &path, out)?;
            continue;
        }
        if !ft.is_file() {
            // Sockets, devices, etc. — equality not safe.
            return Err(());
        }

        let rel = path.strip_prefix(root).map_err(|_| ())?;
        let key = normalize_rel_path(rel)?;
        let bytes = fs::read(&path).map_err(|_| ())?;
        if out.insert(key, bytes).is_some() {
            return Err(());
        }
    }
    Ok(())
}

/// Normalize a relative path to a portable comparison key (`a/b/c`).
pub(crate) fn normalize_rel_path(path: &Path) -> std::result::Result<String, ()> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => {
                let name = name.to_str().ok_or(())?;
                validate_safe_path_component(name).map_err(|_| ())?;
                parts.push(name.to_owned());
            }
            Component::CurDir => {}
            // These cannot be represented safely beneath the destination root.
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return Err(()),
        }
    }
    if parts.is_empty() {
        return Err(());
    }
    Ok(parts.join("/"))
}
