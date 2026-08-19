//! List / fingerprint / projection matrix (read path).

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::models::{
    AgentId, Capability, Skill, SkillLinkKind, SkillMapStatus, SkillMarkdownPreview,
    SkillProjection, SkillSyncState,
};
use crate::platform::skills::{
    classify_projection, collect_file_index, hash_skill_root_shallow, is_exact_child,
    read_skill_md_file, read_skill_metadata, validate_skill_id, validate_skills_root,
};

use super::{SkillListCache, SkillService};

impl SkillService {
    pub fn list_shared_ids(&self) -> Result<HashSet<String>> {
        // Prefer warm list cache — already paid for the scan.
        if let Ok(guard) = self.list_cache.lock() {
            if let Some(cache) = guard.as_ref() {
                if cache.fingerprint == self.list_fingerprint() {
                    return Ok(cache.skills.iter().map(|s| s.id.clone()).collect());
                }
            }
        }

        let root = &self.source_root;
        if !root.exists() {
            return Ok(HashSet::new());
        }
        let meta = fs::metadata(root)?;
        if !meta.is_dir() {
            return Err(AppError::InvalidArg(format!(
                "skill source root is not a directory: {}",
                root.display()
            )));
        }

        let mut ids = HashSet::new();
        for ent in fs::read_dir(root)? {
            let ent = ent?;
            let name = ent.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') {
                continue;
            }
            let file_type = match ent.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            let id = name_str.into_owned();
            if !id.is_empty() {
                ids.insert(id);
            }
        }
        Ok(ids)
    }

    /// Read `SKILL.md` for GUI markdown preview.
    ///
    /// - `private_agent == None` → shared source root (`~/.agents/skills/<id>`).
    /// - `private_agent == Some(agent)` → that agent's private skills dir.
    /// - Rejects path traversal / unsafe ids; never follows skill-root symlinks.
    /// - Body is capped at [`SKILL_MARKDOWN_PREVIEW_CHARS`].
    pub fn read_skill_markdown(
        &self,
        skill_id: &str,
        private_agent: Option<AgentId>,
    ) -> Result<SkillMarkdownPreview> {
        let skill_id = validate_skill_id(skill_id)?;
        let skill_dir = match private_agent {
            None => {
                let dir = self.source_root.join(skill_id);
                if !is_exact_child(&dir, &self.source_root, skill_id) {
                    return Err(AppError::InvalidArg(format!(
                        "skill source path escapes source root: {}",
                        dir.display()
                    )));
                }
                dir
            }
            Some(agent) => {
                let adapter = self.registry.require(agent, Capability::Skills)?;
                let skills_root = adapter.skills_dir().ok_or_else(|| {
                    AppError::Unsupported(format!(
                        "agent {} has no skills directory",
                        agent.as_str()
                    ))
                })?;
                validate_skills_root(&skills_root)?;
                let dir = skills_root.join(skill_id);
                if !is_exact_child(&dir, &skills_root, skill_id) {
                    return Err(AppError::InvalidArg(format!(
                        "skill target path escapes skills root: {}",
                        dir.display()
                    )));
                }
                dir
            }
        };

        // Skill directory itself must be a real directory (not a symlink root).
        if !skill_dir.is_dir() {
            return Err(AppError::NotFound(format!(
                "skill directory not found: {}",
                skill_dir.display()
            )));
        }
        let dir_meta = fs::symlink_metadata(&skill_dir)?;
        if dir_meta.file_type().is_symlink() {
            return Err(AppError::InvalidArg(format!(
                "refusing to read skill via symlink directory: {}",
                skill_dir.display()
            )));
        }

        let skill_md = skill_dir.join("SKILL.md");
        if skill_md.is_file() {
            return read_skill_md_file(skill_id, &skill_dir, &skill_md);
        }
        // Case-insensitive fallback common on Windows installs that wrote skill.md.
        let alt = skill_dir.join("skill.md");
        if alt.is_file() {
            return read_skill_md_file(skill_id, &skill_dir, &alt);
        }
        Err(AppError::NotFound(format!(
            "SKILL.md not found in {}",
            skill_dir.display()
        )))
    }

    /// List skills under `source_root` with a per-agent projection matrix.
    ///
    /// - Missing source root → empty list (not an error).
    /// - Source path exists but is not a directory → [`AppError::InvalidArg`].
    /// - Immediate children only: directories kept; regular files, dot-prefixed
    ///   names, and bookkeeping such as `.skill-lock.json` ignored.
    /// - Order is deterministic by stable skill id (directory name).
    /// - Results are process-cached until fingerprint changes or
    ///   [`invalidate_list_cache`] is called.
    pub fn list(&self) -> Result<Vec<Skill>> {
        let fingerprint = self.list_fingerprint();
        if let Ok(guard) = self.list_cache.lock() {
            if let Some(cache) = guard.as_ref() {
                if cache.fingerprint == fingerprint {
                    return Ok((*cache.skills).clone());
                }
            }
        }

        let skills = self.list_uncached()?;
        if let Ok(mut guard) = self.list_cache.lock() {
            *guard = Some(SkillListCache {
                fingerprint,
                skills: Arc::new(skills.clone()),
            });
        }
        Ok(skills)
    }

    pub(super) fn list_uncached(&self) -> Result<Vec<Skill>> {
        let root = &self.source_root;
        if !root.exists() {
            return Ok(Vec::new());
        }
        let meta = fs::metadata(root)?;
        if !meta.is_dir() {
            return Err(AppError::InvalidArg(format!(
                "skill source root is not a directory: {}",
                root.display()
            )));
        }

        let mut entries: Vec<PathBuf> = Vec::new();
        for ent in fs::read_dir(root)? {
            let ent = ent?;
            let name = ent.file_name();
            let name_str = name.to_string_lossy();
            // Ignore bookkeeping / hidden / non-skill noise at the top level.
            if name_str.starts_with('.') {
                continue;
            }
            let path = ent.path();
            let file_type = match ent.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            // Only immediate child directories; never follow links as skill roots.
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            entries.push(path);
        }

        entries.sort_by(|a, b| {
            let an = a.file_name().map(|s| s.to_os_string());
            let bn = b.file_name().map(|s| s.to_os_string());
            an.cmp(&bn)
        });

        let mut skills = Vec::with_capacity(entries.len());
        for dir in entries {
            let id = dir
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if id.is_empty() {
                continue;
            }
            let (name, description) = read_skill_metadata(&dir, &id);
            let projections = self.project_matrix(&id, &dir);
            skills.push(Skill {
                id,
                name,
                description,
                source_dir: dir,
                projections,
            });
        }
        Ok(skills)
    }

    /// Cheap fingerprint of shared + agent skill roots (shallow entries + SKILL.md).
    pub(super) fn list_fingerprint(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.source_root.hash(&mut hasher);
        hash_skill_root_shallow(&self.source_root, &mut hasher);
        for agent in AgentId::ALL {
            agent.as_str().hash(&mut hasher);
            let Some(adapter) = self.registry.get(agent) else {
                continue;
            };
            let Some(dir) = adapter.skills_dir() else {
                continue;
            };
            dir.hash(&mut hasher);
            hash_skill_root_shallow(&dir, &mut hasher);
        }
        hasher.finish()
    }

    /// Build projections for every agent in [`AgentId::ALL`] order.
    ///
    /// Source tree index is collected once and reused across agents that support
    /// skills (avoids re-reading the same source for Claude/Codex/Grok).
    pub(super) fn project_matrix(&self, skill_id: &str, source_dir: &Path) -> Vec<SkillProjection> {
        // Index only — path → size. Full bytes are never held for list().
        let source_index = collect_file_index(source_dir).ok();
        // Lazy content hashes when path+size matches (streamed, discarded after).
        let mut source_hashes: Option<BTreeMap<String, u64>> = None;

        AgentId::ALL
            .iter()
            .copied()
            .map(|agent| {
                self.project_one(
                    agent,
                    skill_id,
                    source_dir,
                    source_index.as_ref(),
                    &mut source_hashes,
                )
            })
            .collect()
    }

    pub(super) fn project_one(
        &self,
        agent: AgentId,
        skill_id: &str,
        source_dir: &Path,
        source_index: Option<&BTreeMap<String, u64>>,
        source_hashes: &mut Option<BTreeMap<String, u64>>,
    ) -> SkillProjection {
        let blocked = |map_status: SkillMapStatus| SkillProjection {
            agent,
            state: SkillSyncState::Unsupported,
            link_kind: SkillLinkKind::None,
            target_dir: None,
            resolved_target: None,
            map_status,
        };

        let Some(adapter) = self.registry.get(agent) else {
            return blocked(SkillMapStatus::AgentUnsupported);
        };

        if adapter.capability(Capability::Skills).is_blocked() {
            return blocked(SkillMapStatus::AgentUnsupported);
        }

        let Some(skills_root) = adapter.skills_dir() else {
            // Adapter claims skills support but provides no root.
            return blocked(SkillMapStatus::TargetUnavailable);
        };

        let target_dir = skills_root.join(skill_id);
        let (state, link_kind, resolved_target) =
            classify_projection(source_dir, source_index, source_hashes, &target_dir);
        let map_status = match state {
            SkillSyncState::Unsupported => SkillMapStatus::TargetUnavailable,
            SkillSyncState::Foreign | SkillSyncState::Conflict => SkillMapStatus::Conflict,
            SkillSyncState::Linked | SkillSyncState::Copied | SkillSyncState::Absent => {
                SkillMapStatus::Available
            }
        };
        SkillProjection {
            agent,
            state,
            link_kind,
            target_dir: Some(target_dir),
            resolved_target,
            map_status,
        }
    }
}
