//! Projection matrix classification (source vs agent target).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::{SkillLinkKind, SkillSyncState};

use super::fs_index::collect_file_index;
use super::fs_safe::{detect_link_kind, is_link_or_reparse, link_resolves_to_source, resolve_link_path};
use super::hash::hash_tree_files;

/// Classify source vs target for the projection matrix (list path).
///
/// Returns `(state, link_kind, resolved_target)`.
///
/// - Missing → [`SkillSyncState::Absent`]
/// - Link resolving to source → [`SkillSyncState::Linked`] (+ kind + resolved)
/// - Link resolving elsewhere / unresolvable → [`SkillSyncState::Foreign`]
/// - Real directory, regular-file trees identical → [`SkillSyncState::Copied`]
/// - Real directory, content differs → [`SkillSyncState::Foreign`]
/// - File / special / unreadable / nested unsafe → [`SkillSyncState::Conflict`]
pub(crate) fn classify_projection(
    source: &Path,
    source_index: Option<&BTreeMap<String, u64>>,
    source_hashes: &mut Option<BTreeMap<String, u64>>,
    target: &Path,
) -> (SkillSyncState, SkillLinkKind, Option<PathBuf>) {
    let meta = match fs::symlink_metadata(target) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (SkillSyncState::Absent, SkillLinkKind::None, None);
        }
        Err(_) => return (SkillSyncState::Conflict, SkillLinkKind::None, None),
        Ok(m) => m,
    };

    // Projection link: the skill directory itself may be a junction/symlink.
    if is_link_or_reparse(&meta) {
        let kind = detect_link_kind(target, &meta);
        let resolved = resolve_link_path(target);
        // Containment: resolved path must not escape... we report foreign if it
        // doesn't match source; callers that write still validate ancestors.
        if let Some(ref resolved_path) = resolved {
            if link_resolves_to_source(target, source) {
                return (SkillSyncState::Linked, kind, Some(resolved_path.clone()));
            }
            return (SkillSyncState::Foreign, kind, Some(resolved_path.clone()));
        }
        return (SkillSyncState::Foreign, kind, None);
    }

    if !meta.is_dir() {
        // Regular file or special where a skill dir is expected.
        return (SkillSyncState::Conflict, SkillLinkKind::None, None);
    }

    let Some(source_index) = source_index else {
        return (SkillSyncState::Conflict, SkillLinkKind::None, None);
    };

    let target_index = match collect_file_index(target) {
        Ok(t) => t,
        // Nested symlink / special / unreadable inside a real dir → conflict.
        Err(_) => return (SkillSyncState::Conflict, SkillLinkKind::None, None),
    };

    // Path set + size: content mismatches → foreign (not conflict).
    let mut size_match = source_index.len() == target_index.len();
    if size_match {
        for (path, src_size) in source_index {
            match target_index.get(path) {
                Some(tgt_size) if tgt_size == src_size => {}
                _ => {
                    size_match = false;
                    break;
                }
            }
        }
    }
    if !size_match {
        return (SkillSyncState::Foreign, SkillLinkKind::None, None);
    }

    // Same paths and sizes — stream-hash content (no full-byte retention).
    if source_hashes.is_none() {
        match hash_tree_files(source, source_index) {
            Ok(h) => *source_hashes = Some(h),
            Err(()) => return (SkillSyncState::Conflict, SkillLinkKind::None, None),
        }
    }
    let Some(src_hashes) = source_hashes.as_ref() else {
        return (SkillSyncState::Conflict, SkillLinkKind::None, None);
    };
    let tgt_hashes = match hash_tree_files(target, &target_index) {
        Ok(h) => h,
        Err(()) => return (SkillSyncState::Conflict, SkillLinkKind::None, None),
    };

    if src_hashes == &tgt_hashes {
        (SkillSyncState::Copied, SkillLinkKind::None, None)
    } else {
        (SkillSyncState::Foreign, SkillLinkKind::None, None)
    }
}
