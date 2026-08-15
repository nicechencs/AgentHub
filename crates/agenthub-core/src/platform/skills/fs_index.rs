//! Regular-file path → size index for skill trees (list / classify path).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::fs_safe::{is_link_or_reparse, normalize_rel_path};

/// Collect normalized relative path → file size for all regular files under root.
///
/// Same safety rules as [`collect_regular_files`]: no symlink follow, non-regular
/// → error, case-fold portable-name collisions → error. Does **not** read bytes.
pub(crate) fn collect_file_index(root: &Path) -> std::result::Result<BTreeMap<String, u64>, ()> {
    let root_meta = fs::symlink_metadata(root).map_err(|_| ())?;
    if is_link_or_reparse(&root_meta) || !root_meta.is_dir() {
        return Err(());
    }

    let mut out = BTreeMap::new();
    collect_file_index_rec(root, root, &mut out)?;

    let mut portable_keys = BTreeMap::new();
    for key in out.keys() {
        let folded = key.to_lowercase();
        if portable_keys.insert(folded, key).is_some() {
            return Err(());
        }
    }

    Ok(out)
}

pub(crate) fn collect_file_index_rec(
    root: &Path,
    dir: &Path,
    out: &mut BTreeMap<String, u64>,
) -> std::result::Result<(), ()> {
    let entries = fs::read_dir(dir).map_err(|_| ())?;
    for ent in entries {
        let ent = ent.map_err(|_| ())?;
        let path = ent.path();

        let meta = fs::symlink_metadata(&path).map_err(|_| ())?;
        let ft = meta.file_type();

        if is_link_or_reparse(&meta) {
            return Err(());
        }
        if ft.is_dir() {
            collect_file_index_rec(root, &path, out)?;
            continue;
        }
        if !ft.is_file() {
            return Err(());
        }

        let rel = path.strip_prefix(root).map_err(|_| ())?;
        let key = normalize_rel_path(rel)?;
        let size = meta.len();
        if out.insert(key, size).is_some() {
            return Err(());
        }
    }
    Ok(())
}

pub(crate) fn join_normalized(root: &Path, rel: &str) -> std::result::Result<PathBuf, ()> {
    let mut path = root.to_path_buf();
    for part in rel.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(());
        }
        path.push(part);
    }
    Ok(path)
}
