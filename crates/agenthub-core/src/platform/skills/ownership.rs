//! Skill projection ownership markers (R03).
//!
//! Platform may only delete projections it can prove it created and that have
//! not been tampered with. Byte-identical copies without a valid marker are
//! **not** managed (but may be removed as a legacy copy during disable).
//!
//! Marker path (per agent skills root):
//!   `<skills_root>/.agenthub/skill-ownership/<skill_id>.json`

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, Result};
use crate::models::SkillLinkKind;
use crate::platform::AgentKey;
use crate::utils::atomic::atomic_write;

use super::fs_safe::{
    collect_regular_files, ensure_no_symlink_in_existing_prefix, inspect_projection_target,
    is_exact_child, is_link_or_reparse, is_path_inside, link_resolves_to_source,
    remove_projection_link, validate_skill_id, validate_skills_root, validate_tree_entries_safe,
    TargetPresence,
};
use super::packages::{materialize_projection, validate_and_collect_source};
use super::projection_link::create_projection_link;

pub(crate) const OWNERSHIP_FORMAT_VERSION: u32 = 1;
pub(crate) const PROJECTION_MODE_COPY: &str = "copy";
pub(crate) const PROJECTION_MODE_LINK: &str = "link";

const OWNERSHIP_DIR_SEGMENTS: &[&str] = &[".agenthub", "skill-ownership"];

/// Sidecar ownership record for a copied projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillOwnershipMarker {
    pub format_version: u32,
    pub skill_id: String,
    pub target_relative_path: String,
    pub projection_mode: String,
    pub applied_revision: String,
    pub content_fingerprint: String,
}

// ---------------------------------------------------------------------------
// Paths + store safety
// ---------------------------------------------------------------------------

pub(crate) fn ownership_store_dir(skills_root: &Path) -> PathBuf {
    let mut p = skills_root.to_path_buf();
    for seg in OWNERSHIP_DIR_SEGMENTS {
        p.push(seg);
    }
    p
}

pub(crate) fn ownership_marker_path(skills_root: &Path, skill_id: &str) -> PathBuf {
    ownership_store_dir(skills_root).join(format!("{skill_id}.json"))
}

/// Validate skills_root and that ownership store path does not traverse links.
pub(crate) fn ensure_ownership_store_safe(skills_root: &Path) -> Result<()> {
    validate_skills_root(skills_root)?;
    let store = ownership_store_dir(skills_root);
    ensure_no_symlink_in_existing_prefix(&store)?;
    // If any ownership path component exists, it must not be a reparse/symlink.
    let mut acc = skills_root.to_path_buf();
    for seg in OWNERSHIP_DIR_SEGMENTS {
        acc.push(seg);
        match fs::symlink_metadata(&acc) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                return Err(AppError::InvalidArg(format!(
                    "ownership path unreadable ({}): {e}",
                    acc.display()
                )));
            }
            Ok(meta) if is_link_or_reparse(&meta) => {
                return Err(AppError::InvalidArg(format!(
                    "ownership path must not be a symlink or reparse point: {}",
                    acc.display()
                )));
            }
            Ok(meta) if !meta.is_dir() => {
                return Err(AppError::InvalidArg(format!(
                    "ownership path must be a directory: {}",
                    acc.display()
                )));
            }
            Ok(_) => {}
        }
    }
    Ok(())
}

fn ensure_ownership_store_dirs(skills_root: &Path) -> Result<()> {
    ensure_ownership_store_safe(skills_root)?;
    let store = ownership_store_dir(skills_root);
    if !store.exists() {
        fs::create_dir_all(&store)?;
        ensure_ownership_store_safe(skills_root)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Fingerprint
// ---------------------------------------------------------------------------

/// Stable content fingerprint over a validated regular-file tree map
/// (keys already sorted by [`BTreeMap`]).
pub(crate) fn fingerprint_file_map(files: &BTreeMap<String, Vec<u8>>) -> String {
    let mut hasher = Sha256::new();
    for (rel, bytes) in files {
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    hex_encode(&hasher.finalize())
}

/// Fingerprint the regular-file tree at `target_dir` after safety validation.
pub(crate) fn fingerprint_tree_at(target_dir: &Path) -> Result<String> {
    validate_tree_entries_safe(target_dir, "skill target")?;
    let files = collect_regular_files(target_dir).map_err(|()| {
        AppError::InvalidArg(format!(
            "skill target tree is unreadable or unsafe: {}",
            target_dir.display()
        ))
    })?;
    Ok(fingerprint_file_map(&files))
}

fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

// ---------------------------------------------------------------------------
// Marker read / write / clear
// ---------------------------------------------------------------------------

pub(crate) fn write_copy_ownership_marker(
    skills_root: &Path,
    skill_id: &str,
    applied_revision: &str,
    content_fingerprint: &str,
) -> Result<()> {
    let skill_id = validate_skill_id(skill_id)?;
    ensure_ownership_store_dirs(skills_root)?;
    let path = ownership_marker_path(skills_root, skill_id);
    // Final path must not be a link (atomic_write creates via temp in parent).
    ensure_no_symlink_in_existing_prefix(&path)?;
    if let Ok(meta) = fs::symlink_metadata(&path) {
        if is_link_or_reparse(&meta) {
            return Err(AppError::InvalidArg(format!(
                "ownership marker path is a symlink or reparse point: {}",
                path.display()
            )));
        }
    }

    let marker = SkillOwnershipMarker {
        format_version: OWNERSHIP_FORMAT_VERSION,
        skill_id: skill_id.to_string(),
        target_relative_path: skill_id.to_string(),
        projection_mode: PROJECTION_MODE_COPY.into(),
        applied_revision: applied_revision.to_string(),
        content_fingerprint: content_fingerprint.to_string(),
    };
    let json = serde_json::to_vec_pretty(&marker).map_err(|e| {
        AppError::message(
            "skill.ownership",
            format!("serialize ownership marker for '{skill_id}': {e}"),
        )
    })?;
    atomic_write(&path, &json)?;
    Ok(())
}

/// Clear ownership marker; errors if path is unsafe or removal fails.
pub(crate) fn clear_ownership_marker(skills_root: &Path, skill_id: &str) -> Result<()> {
    let skill_id = validate_skill_id(skill_id)?;
    ensure_ownership_store_safe(skills_root)?;
    let path = ownership_marker_path(skills_root, skill_id);
    ensure_no_symlink_in_existing_prefix(&path)?;
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::from(e)),
        Ok(meta) if is_link_or_reparse(&meta) => Err(AppError::InvalidArg(format!(
            "ownership marker is a symlink or reparse point: {}",
            path.display()
        ))),
        Ok(_) => fs::remove_file(&path).or_else(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(AppError::from(e))
            }
        }),
    }
}

fn read_ownership_marker_raw(
    skills_root: &Path,
    skill_id: &str,
) -> Result<Option<SkillOwnershipMarker>> {
    let skill_id = validate_skill_id(skill_id)?;
    ensure_ownership_store_safe(skills_root)?;
    let path = ownership_marker_path(skills_root, skill_id);
    ensure_no_symlink_in_existing_prefix(&path)?;
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(AppError::InvalidArg(format!(
                "ownership marker unreadable ({}): {e}",
                path.display()
            )));
        }
        Ok(meta) if is_link_or_reparse(&meta) => {
            return Err(AppError::InvalidArg(format!(
                "ownership marker is a symlink or reparse point: {}",
                path.display()
            )));
        }
        Ok(meta) if !meta.is_file() => {
            return Err(AppError::InvalidArg(format!(
                "ownership marker is not a regular file: {}",
                path.display()
            )));
        }
        Ok(_) => {}
    }
    let raw = fs::read(&path).map_err(|e| {
        AppError::InvalidArg(format!(
            "ownership marker unreadable ({}): {e}",
            path.display()
        ))
    })?;
    // Malformed JSON / field shape → skill.conflict (not a path hazard).
    let marker: SkillOwnershipMarker = serde_json::from_slice(&raw).map_err(|e| {
        ownership_conflict(
            skill_id,
            &path,
            &format!("malformed ownership marker JSON: {e}"),
        )
    })?;
    Ok(Some(marker))
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

/// Validate marker fields and live fingerprint against a real directory target.
///
/// `expected_revision`: when `Some`, marker.applied_revision must match
/// (bootstrap import). When `None`, any non-empty revision is accepted
/// (delete / managed checks).
pub(crate) fn verify_copy_ownership(
    skills_root: &Path,
    skill_id: &str,
    target_dir: &Path,
    expected_revision: Option<&str>,
) -> Result<SkillOwnershipMarker> {
    let skill_id = validate_skill_id(skill_id)?;
    if !is_exact_child(target_dir, skills_root, skill_id) {
        return Err(ownership_conflict(
            skill_id,
            target_dir,
            "target path is not exact skills_root/skill_id",
        ));
    }

    let Some(marker) = read_ownership_marker_raw(skills_root, skill_id)? else {
        return Err(ownership_conflict(
            skill_id,
            target_dir,
            "missing ownership marker",
        ));
    };

    if marker.format_version != OWNERSHIP_FORMAT_VERSION {
        return Err(ownership_conflict(
            skill_id,
            target_dir,
            "unsupported ownership marker format",
        ));
    }
    if marker.skill_id != skill_id {
        return Err(ownership_conflict(
            skill_id,
            target_dir,
            "ownership marker skill_id mismatch",
        ));
    }
    if marker.target_relative_path != skill_id {
        return Err(ownership_conflict(
            skill_id,
            target_dir,
            "ownership marker target path mismatch",
        ));
    }
    if marker.projection_mode != PROJECTION_MODE_COPY {
        return Err(ownership_conflict(
            skill_id,
            target_dir,
            "ownership marker is not a copy projection",
        ));
    }
    if marker.applied_revision.is_empty() {
        return Err(ownership_conflict(
            skill_id,
            target_dir,
            "ownership marker has empty revision",
        ));
    }
    if let Some(exp) = expected_revision {
        if marker.applied_revision != exp {
            return Err(ownership_conflict(
                skill_id,
                target_dir,
                "ownership marker revision mismatch",
            ));
        }
    }
    if marker.content_fingerprint.is_empty() {
        return Err(ownership_conflict(
            skill_id,
            target_dir,
            "ownership marker has empty fingerprint",
        ));
    }

    match inspect_projection_target(target_dir)? {
        TargetPresence::Directory => {}
        TargetPresence::Missing => {
            return Err(ownership_conflict(
                skill_id,
                target_dir,
                "ownership marker present but target missing",
            ));
        }
        TargetPresence::Link { .. } => {
            return Err(ownership_conflict(
                skill_id,
                target_dir,
                "ownership marker is for copy but target is a link",
            ));
        }
        TargetPresence::Dangerous { kind } => {
            return Err(AppError::InvalidArg(format!(
                "skill target is not a safe directory ({kind}): {}",
                target_dir.display()
            )));
        }
    }

    let live_fp = fingerprint_tree_at(target_dir)?;
    if live_fp != marker.content_fingerprint {
        return Err(ownership_conflict(
            skill_id,
            target_dir,
            "content fingerprint mismatch (projection modified or not platform-owned)",
        ));
    }
    Ok(marker)
}

fn ownership_conflict(skill_id: &str, target: &Path, detail: &str) -> AppError {
    AppError::message(
        "skill.conflict",
        format!(
            "skill '{skill_id}' is not a verified platform projection at {} ({detail})",
            target.display()
        ),
    )
}

/// Conflict for targets that are not verified platform projections.
/// Does **not** suggest force takeover of ordinary / foreign content.
pub(crate) fn conflict_error(skill_id: &str, agent: &AgentKey, target: &Path) -> AppError {
    AppError::message(
        "skill.conflict",
        format!(
            "skill '{skill_id}' conflicts at {} for agent {} (not a verified platform projection)",
            target.display(),
            agent.as_str()
        ),
    )
}

/// Managed copy exists but differs from current source; force may refresh.
pub(crate) fn conflict_stale_managed(skill_id: &str, agent: &AgentKey, target: &Path) -> AppError {
    AppError::message(
        "skill.conflict",
        format!(
            "skill '{skill_id}' managed projection at {} for agent {} differs from source (use force to refresh)",
            target.display(),
            agent.as_str()
        ),
    )
}

/// Whether a path is a platform-managed projection of `source_dir`.
///
/// - Link: only when it resolves to source (no marker required).
/// - Directory: only with valid ownership marker + matching fingerprint.
///   Byte-identical without marker is **not** managed.
///
/// When `expected_revision` is set (bootstrap), marker revision must match.
pub fn is_managed_projection(
    source_dir: &Path,
    skills_root: &Path,
    skill_id: &str,
    target_dir: &Path,
    expected_revision: Option<&str>,
) -> bool {
    match inspect_projection_target(target_dir) {
        Ok(TargetPresence::Link { .. }) => link_resolves_to_source(target_dir, source_dir),
        Ok(TargetPresence::Directory) => {
            verify_copy_ownership(skills_root, skill_id, target_dir, expected_revision).is_ok()
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Unified project / unproject FS paths
// ---------------------------------------------------------------------------

/// After a successful copy materialize, write ownership marker from live tree.
///
/// Failure here means the overall project operation failed (caller must not
/// record applied). The copy may remain on disk without a marker (unmanaged).
pub(crate) fn record_copy_ownership(
    skills_root: &Path,
    skill_id: &str,
    target_dir: &Path,
    applied_revision: &str,
) -> Result<()> {
    let fp = fingerprint_tree_at(target_dir)?;
    write_copy_ownership_marker(skills_root, skill_id, applied_revision, &fp)
}

/// Project source files as a copy, recording ownership. Shared by reconciler
/// and SkillService FS-only sync.
///
/// **Force semantics (R03 close-out):**
/// - Foreign link → always `skill.conflict` (force cannot remove/replace it).
/// - Real directory is platform-owned only when `verify_copy_ownership` succeeds.
/// - Verify failure → always `skill.conflict`; directory content is never mutated.
/// - Force only refreshes a verified managed copy whose live fingerprint differs
///   from the current source (source package update). Force cannot claim
///   unmarked or foreign directories.
pub(crate) fn project_copy_with_ownership(
    skills_root: &Path,
    skill_id: &str,
    source_dir: &Path,
    target_dir: &Path,
    source_files: &BTreeMap<String, Vec<u8>>,
    force: bool,
    applied_revision: &str,
    agent: &AgentKey,
) -> Result<()> {
    let skill_id = validate_skill_id(skill_id)?;
    let source_fp = fingerprint_file_map(source_files);

    let target_state = inspect_projection_target(target_dir)?;
    match target_state {
        TargetPresence::Missing => {
            materialize_projection(skills_root, skill_id, target_dir, source_files, None)?;
            record_copy_ownership(skills_root, skill_id, target_dir, applied_revision)?;
        }
        TargetPresence::Link { .. } => {
            if link_resolves_to_source(target_dir, source_dir) {
                // Correct managed link — leave as-is (link mode; no marker).
                return Ok(());
            }
            // Foreign link: never delete or replace, even with force.
            return Err(conflict_error(skill_id, agent, target_dir));
        }
        TargetPresence::Directory => {
            validate_tree_entries_safe(target_dir, "skill target")?;

            // Platform ownership requires marker + live fingerprint match.
            // Verify failure (missing/invalid/tampered marker) → conflict for any force.
            let marker = match verify_copy_ownership(skills_root, skill_id, target_dir, None) {
                Ok(m) => m,
                Err(e) if e.code() == "skill.conflict" => {
                    return Err(conflict_error(skill_id, agent, target_dir));
                }
                Err(e) => return Err(e),
            };

            if marker.content_fingerprint == source_fp {
                // Content matches source: refresh revision only when package moved.
                if marker.applied_revision != applied_revision {
                    write_copy_ownership_marker(
                        skills_root,
                        skill_id,
                        applied_revision,
                        &source_fp,
                    )?;
                }
                return Ok(());
            }

            // Verified managed copy, but source content differs.
            if !force {
                return Err(conflict_stale_managed(skill_id, agent, target_dir));
            }
            materialize_projection(
                skills_root,
                skill_id,
                target_dir,
                source_files,
                Some(target_dir),
            )?;
            record_copy_ownership(skills_root, skill_id, target_dir, applied_revision)?;
        }
        TargetPresence::Dangerous { kind } => {
            return Err(AppError::InvalidArg(format!(
                "skill target for '{skill_id}' on {} is not a safe directory ({kind}): {}",
                agent.as_str(),
                target_dir.display()
            )));
        }
    }
    Ok(())
}

/// Project source as a managed link. Shared by the reconciler link path.
///
/// **Force semantics:** same as copy — foreign links and unmanaged directories
/// are always `skill.conflict`. Force cannot claim them.
///
/// A verified managed copy is converted to a link by creating the link at a
/// sibling staging path first. If that fails (or falls back to a physical copy),
/// the original managed copy is left untouched.
pub(crate) fn project_link_with_ownership(
    skills_root: &Path,
    skill_id: &str,
    source_dir: &Path,
    target_dir: &Path,
    force: bool,
    applied_revision: &str,
    agent: &AgentKey,
) -> Result<()> {
    let _ = force;
    let skill_id = validate_skill_id(skill_id)?;
    validate_and_collect_source(source_dir, skill_id)?;

    if !skills_root.exists() {
        ensure_no_symlink_in_existing_prefix(skills_root)?;
        fs::create_dir_all(skills_root)?;
    }
    validate_skills_root(skills_root)?;

    match inspect_projection_target(target_dir)? {
        TargetPresence::Missing => {
            let (applied, _fell_back) = create_projection_link(source_dir, target_dir)?;
            finalize_link_projection_ownership(
                skills_root,
                skill_id,
                target_dir,
                applied == SkillLinkKind::None,
                applied_revision,
            )
        }
        TargetPresence::Link { .. } => {
            if !link_resolves_to_source(target_dir, source_dir) {
                return Err(conflict_error(skill_id, agent, target_dir));
            }
            // Correct managed link: drop a leftover copy marker so bootstrap
            // and verify_copy_ownership do not see a copy/link mismatch.
            clear_ownership_marker(skills_root, skill_id)
        }
        TargetPresence::Directory => {
            validate_tree_entries_safe(target_dir, "skill target")?;
            match verify_copy_ownership(skills_root, skill_id, target_dir, None) {
                Ok(_) => convert_managed_copy_to_link(
                    skills_root,
                    skill_id,
                    source_dir,
                    target_dir,
                    applied_revision,
                    agent,
                ),
                Err(e) if e.code() == "skill.conflict" => {
                    Err(conflict_error(skill_id, agent, target_dir))
                }
                Err(e) => Err(e),
            }
        }
        TargetPresence::Dangerous { kind } => Err(AppError::InvalidArg(format!(
            "skill target for '{skill_id}' on {} is not a safe directory ({kind}): {}",
            agent.as_str(),
            target_dir.display()
        ))),
    }
}

/// Create a real link at a sibling path, then swap it over a verified copy.
///
/// Link-create failure or copy fallback leaves the managed copy in place
/// (desired mode stays `link`; physical form is the fallback copy).
fn convert_managed_copy_to_link(
    skills_root: &Path,
    skill_id: &str,
    source_dir: &Path,
    target_dir: &Path,
    applied_revision: &str,
    agent: &AgentKey,
) -> Result<()> {
    let staging = match allocate_link_staging_path(skills_root, skill_id) {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };

    let (applied, _fell_back) = match create_projection_link(source_dir, &staging) {
        Ok(result) => result,
        Err(_) => {
            let _ = cleanup_link_staging(&staging, skills_root);
            return Ok(());
        }
    };

    if applied == SkillLinkKind::None {
        let _ = cleanup_link_staging(&staging, skills_root);
        return Ok(());
    }

    if let Err(e) = unproject_with_ownership(skills_root, skill_id, source_dir, target_dir, agent) {
        let _ = cleanup_link_staging(&staging, skills_root);
        return Err(e);
    }

    if let Err(e) = fs::rename(&staging, target_dir) {
        let _ = cleanup_link_staging(&staging, skills_root);
        return Err(AppError::message(
            "skill.swap",
            format!(
                "recycled managed copy for '{skill_id}' but failed to place link at {}: {e}",
                target_dir.display()
            ),
        ));
    }

    finalize_link_projection_ownership(skills_root, skill_id, target_dir, false, applied_revision)
}

fn allocate_link_staging_path(skills_root: &Path, skill_id: &str) -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for i in 0u32..1024 {
        let name = format!(".agenthub-link-{skill_id}-{nanos}-{i}");
        let path = skills_root.join(&name);
        if !is_path_inside(&path, skills_root) {
            return Err(AppError::InvalidArg(format!(
                "link staging path escapes skills root: {}",
                path.display()
            )));
        }
        if fs::symlink_metadata(&path).is_err() {
            return Ok(path);
        }
    }
    Err(AppError::message(
        "skill.staging",
        "could not allocate unique skill link staging path",
    ))
}

fn cleanup_link_staging(path: &Path, skills_root: &Path) -> Result<()> {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Ok(());
    };
    if !name.starts_with(".agenthub-link-") {
        return Ok(());
    }
    if path.parent() != Some(skills_root) {
        return Ok(());
    }
    match inspect_projection_target(path)? {
        TargetPresence::Missing => Ok(()),
        TargetPresence::Link { .. } => remove_projection_link(path),
        TargetPresence::Directory => {
            validate_tree_entries_safe(path, "skill helper")?;
            fs::remove_dir_all(path).or_else(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(AppError::from(e))
                }
            })
        }
        TargetPresence::Dangerous { kind } => Err(AppError::InvalidArg(format!(
            "refusing to clean unsafe link staging ({kind}): {}",
            path.display()
        ))),
    }
}

/// After `create_projection_link`: real links clear stale markers; copy fallback
/// (`SkillLinkKind::None`) must record ownership like an ordinary copy.
pub(crate) fn finalize_link_projection_ownership(
    skills_root: &Path,
    skill_id: &str,
    target_dir: &Path,
    applied_is_copy_fallback: bool,
    applied_revision: &str,
) -> Result<()> {
    if applied_is_copy_fallback {
        record_copy_ownership(skills_root, skill_id, target_dir, applied_revision)
    } else {
        // True link — no marker required; clear leftover copy marker (errors propagate).
        clear_ownership_marker(skills_root, skill_id)
    }
}

/// Move a real skill directory to the operating system recycle bin.
///
/// User-facing deletes (shared uninstall, private/conflict folder remove,
/// verified copy unproject) go through here. Links are unlinked, not recycled.
#[cfg(not(test))]
pub(crate) fn recycle_skill_dir(path: &Path) -> Result<()> {
    trash::delete(path).map_err(|e| {
        AppError::message(
            "skill.recycle",
            format!(
                "failed to move skill folder to the recycle bin at {}: {e}",
                path.display()
            ),
        )
    })
}

/// Unit tests must not add temporary fixtures to the user's real recycle bin.
#[cfg(test)]
pub(crate) fn recycle_skill_dir(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::from(e)),
    }
}

fn recycle_projection_dir(target_dir: &Path) -> Result<()> {
    recycle_skill_dir(target_dir)
}

/// Remove a projection only when ownership can be proven.
///
/// Directory copies are moved to the operating system recycle bin. A verified
/// marker is cleared before recycling so a successful recycle can never leave
/// stale ownership evidence; if recycling fails, the verified marker is
/// restored.
///
/// - Link → only if resolves to source; never follow.
/// - Directory with marker → marker + fingerprint must match, then recycle.
/// - Directory without marker → only a safe, byte-identical source copy is
///   accepted as a legacy AgentHub projection, then recycled.
/// - Missing → idempotent success; clear stale marker.
/// - force is not a parameter: ownership cannot be bypassed for delete.
/// - `clear_ownership_marker` errors are never swallowed.
pub(crate) fn unproject_with_ownership(
    skills_root: &Path,
    skill_id: &str,
    source_dir: &Path,
    target_dir: &Path,
    agent: &AgentKey,
) -> Result<()> {
    unproject_with_recycler(
        skills_root,
        skill_id,
        source_dir,
        target_dir,
        agent,
        recycle_projection_dir,
    )
}

/// Internal unproject implementation with an injected directory recycler.
/// Tests pass a temporary-directory cleanup closure so they never touch the
/// user's real operating system recycle bin.
pub(crate) fn unproject_with_recycler<F>(
    skills_root: &Path,
    skill_id: &str,
    source_dir: &Path,
    target_dir: &Path,
    agent: &AgentKey,
    recycler: F,
) -> Result<()>
where
    F: FnOnce(&Path) -> Result<()>,
{
    let skill_id = validate_skill_id(skill_id)?;
    if !is_exact_child(target_dir, skills_root, skill_id) {
        return Err(AppError::InvalidArg(format!(
            "skill target escapes skills root: {}",
            target_dir.display()
        )));
    }

    match inspect_projection_target(target_dir)? {
        TargetPresence::Missing => {
            // Idempotent: still clear a stale marker for this skill id.
            clear_ownership_marker(skills_root, skill_id)?;
            Ok(())
        }
        TargetPresence::Link { .. } => {
            if !link_resolves_to_source(target_dir, source_dir) {
                return Err(AppError::message(
                    "skill.conflict",
                    format!(
                        "skill '{skill_id}' foreign link at {} for agent {} (not removed)",
                        target_dir.display(),
                        agent.as_str()
                    ),
                ));
            }
            // Clear marker first so a clear failure cannot leave stale ownership
            // after the link is already gone.
            clear_ownership_marker(skills_root, skill_id)?;
            remove_projection_link(target_dir)?;
            Ok(())
        }
        TargetPresence::Directory => {
            validate_tree_entries_safe(target_dir, "skill target")?;
            let verified_marker = match read_ownership_marker_raw(skills_root, skill_id)? {
                Some(_) => {
                    // A present marker must pass every normal ownership check;
                    // malformed or mismatched markers never fall back to legacy.
                    Some(verify_copy_ownership(
                        skills_root,
                        skill_id,
                        target_dir,
                        None,
                    )?)
                }
                None => {
                    // Pre-marker AgentHub copies can be disabled only when both
                    // trees pass validation and are exactly content-identical.
                    let source_fp = fingerprint_tree_at(source_dir)?;
                    let target_fp = fingerprint_tree_at(target_dir)?;
                    if source_fp != target_fp {
                        return Err(ownership_conflict(
                            skill_id,
                            target_dir,
                            "missing ownership marker and content differs from source",
                        ));
                    }
                    None
                }
            };

            if let Some(marker) = verified_marker.as_ref() {
                clear_ownership_marker(skills_root, skill_id)?;
                if let Err(recycle_err) = recycler(target_dir) {
                    if let Err(restore_err) = write_copy_ownership_marker(
                        skills_root,
                        skill_id,
                        &marker.applied_revision,
                        &marker.content_fingerprint,
                    ) {
                        return Err(AppError::message(
                            "skill.recycle_rollback",
                            format!(
                                "failed to recycle verified skill projection ({recycle_err}); \
                                 restoring its ownership marker also failed ({restore_err})"
                            ),
                        ));
                    }
                    return Err(recycle_err);
                }
            } else {
                recycler(target_dir)?;
            }
            Ok(())
        }
        TargetPresence::Dangerous { kind } => Err(AppError::InvalidArg(format!(
            "refusing to disable skill '{skill_id}' on {}: target is {kind}: {}",
            agent.as_str(),
            target_dir.display()
        ))),
    }
}

/// Clear an existing target only when it is a verified managed projection
/// (used by `project_skill` before re-projecting). Unmanaged → conflict.
pub(crate) fn clear_managed_target_for_reproject(
    skills_root: &Path,
    skill_id: &str,
    source_dir: &Path,
    target_dir: &Path,
    agent: &AgentKey,
) -> Result<()> {
    unproject_with_ownership(skills_root, skill_id, source_dir, target_dir, agent)
}
