//! Catalog / installed-list helpers used by the SkillService façade.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::adapters::AdapterRegistry;
use crate::models::{
    AgentId, Capability, InstalledSkill, Skill, SkillMapStatus, SkillSourceRecord, SkillSyncState,
};

use super::classify::classify_projection;
use super::fs_index::collect_file_index;
use super::fs_safe::{is_link_or_reparse, resolve_link_path};
use super::yaml::read_skill_metadata;

/// One on-disk skill directory discovered under an agent's skills root.
pub(crate) struct AgentSkillDirEntry {
    pub agent: AgentId,
    pub skill_id: String,
    pub path: PathBuf,
    pub skills_root: PathBuf,
    pub display: String,
    pub description: String,
}

/// Walk every agent skills root with the same discovery rules as
/// [`SkillService::list_installed`] / [`SkillService::list_catalog`].
pub(crate) fn for_each_agent_skill_dir(
    registry: &AdapterRegistry,
    mut visit: impl FnMut(AgentSkillDirEntry),
) {
    for agent in AgentId::ALL {
        let Some(adapter) = registry.get(agent) else {
            continue;
        };
        if adapter.capability(Capability::Skills).is_blocked() {
            continue;
        }
        let Some(skills_root) = adapter.skills_dir() else {
            continue;
        };
        if !skills_root.is_dir() {
            continue;
        }
        let entries = match fs::read_dir(&skills_root) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for ent in entries {
            let ent = match ent {
                Ok(e) => e,
                Err(_) => continue,
            };
            let name = ent.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = ent.path();
            let meta = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            // Accept real dirs and projection links as "installed" for the agent.
            let is_dirish =
                meta.is_dir() || is_link_or_reparse(&meta) || meta.file_type().is_symlink();
            if !is_dirish {
                continue;
            }
            // Prefer entries that look like skills (SKILL.md) when resolvable.
            let skill_md_ok = path.join("SKILL.md").is_file()
                || resolve_link_path(&path)
                    .map(|r| r.join("SKILL.md").is_file())
                    .unwrap_or(false);
            if !skill_md_ok {
                continue;
            }
            let (display, description) = read_skill_metadata(&path, &name);
            visit(AgentSkillDirEntry {
                agent,
                skill_id: name,
                path,
                skills_root: skills_root.clone(),
                display,
                description,
            });
        }
    }
}

pub(crate) fn installed_skill_from_shared(
    source_root: &Path,
    skill: Skill,
    source: Option<SkillSourceRecord>,
) -> InstalledSkill {
    InstalledSkill {
        id: skill.id.clone(),
        name: skill.name,
        description: skill.description,
        source_dir: skill.source_dir.clone(),
        root_label: short_root_label(source_root),
        root_dir: source_root.to_path_buf(),
        origin: "shared".into(),
        projectable: true,
        map_status: SkillMapStatus::Available,
        source,
        projections: skill.projections,
    }
}

pub(crate) fn installed_skill_from_agent(
    entry: AgentSkillDirEntry,
    map_status: SkillMapStatus,
) -> InstalledSkill {
    InstalledSkill {
        id: entry.skill_id,
        name: entry.display,
        description: entry.description,
        source_dir: entry.path,
        root_label: short_root_label(&entry.skills_root),
        root_dir: entry.skills_root,
        origin: entry.agent.as_str().to_string(),
        projectable: false,
        map_status,
        source: None,
        projections: vec![],
    }
}

/// Compare an agent-root skill directory against the shared library copy.
///
/// Uses the same path/size + stream-hash pipeline as the projection matrix.
pub(crate) fn map_status_agent_vs_shared(
    skill_id: &str,
    shared_dir: &Path,
    agent_path: &Path,
    cache: &mut HashMap<String, (Option<BTreeMap<String, u64>>, Option<BTreeMap<String, u64>>)>,
) -> SkillMapStatus {
    let entry = cache.entry(skill_id.to_string()).or_insert_with(|| {
        let index = collect_file_index(shared_dir).ok();
        (index, None)
    });
    let source_index = entry.0.as_ref();
    let (state, _, _) = classify_projection(shared_dir, source_index, &mut entry.1, agent_path);
    match state {
        SkillSyncState::Linked | SkillSyncState::Copied => SkillMapStatus::Available,
        SkillSyncState::Foreign | SkillSyncState::Conflict => SkillMapStatus::Conflict,
        // Shared id listed but shared dir missing/unreadable — treat as local-only.
        SkillSyncState::Absent => SkillMapStatus::PrivateSource,
        SkillSyncState::Unsupported => SkillMapStatus::Conflict,
    }
}

pub(crate) fn short_root_label(root: &Path) -> String {
    if let Ok(home) = crate::utils::paths::home_dir() {
        if let Ok(rel) = root.strip_prefix(&home) {
            return format!("~/{}", rel.to_string_lossy().replace('\\', "/"));
        }
    }
    root.display().to_string()
}
