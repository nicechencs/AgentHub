//! `.skill-lock.json` read/write (install origin records).
//!
//! Format is intentionally compatible with existing SkillService lock files.
//! Read and JSON parse failures are surfaced — never treated as an empty map.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::models::SkillSourceRecord;
use crate::utils::atomic::atomic_write;

pub(crate) fn skill_lock_file(root: &Path) -> PathBuf {
    root.join(".skill-lock.json")
}

/// Load the skill lock map.
///
/// - Missing file → empty map.
/// - Unreadable file or invalid JSON → error (not empty).
pub(crate) fn skill_lock_load(root: &Path) -> Result<BTreeMap<String, SkillSourceRecord>> {
    let path = skill_lock_file(root);
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => {
            return Err(AppError::message(
                "skill.lock",
                format!("read .skill-lock.json failed: {e}"),
            ));
        }
    };
    serde_json::from_str::<BTreeMap<String, SkillSourceRecord>>(&raw).map_err(|e| {
        AppError::message(
            "skill.lock",
            format!("parse .skill-lock.json failed: {e}"),
        )
    })
}

pub(crate) fn skill_lock_save(root: &Path, map: &BTreeMap<String, SkillSourceRecord>) -> Result<()> {
    if !root.exists() {
        fs::create_dir_all(root)?;
    }
    let path = skill_lock_file(root);
    let json = serde_json::to_string_pretty(map)
        .map_err(|e| AppError::message("skill.lock", format!("serialize .skill-lock.json: {e}")))?;
    atomic_write(&path, json.as_bytes())
}

pub(crate) fn skill_lock_upsert(root: &Path, id: &str, record: SkillSourceRecord) -> Result<()> {
    let mut map = skill_lock_load(root)?;
    map.insert(id.to_string(), record);
    skill_lock_save(root, &map)
}

pub(crate) fn skill_lock_remove(root: &Path, id: &str) -> Result<()> {
    let mut map = skill_lock_load(root)?;
    map.remove(id);
    skill_lock_save(root, &map)
}

/// Replace the entire lock map with `map` (used by commit compensation).
pub(crate) fn skill_lock_replace_map(
    root: &Path,
    map: &BTreeMap<String, SkillSourceRecord>,
) -> Result<()> {
    skill_lock_save(root, map)
}
