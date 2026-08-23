//! Installed / catalog list orchestration.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::error::Result;
use crate::models::{InstalledSkill, SkillMapStatus};
use crate::platform::skills::{
    fingerprint_skill_tree, for_each_agent_skill_dir, installed_skill_from_agent,
    installed_skill_from_shared, map_status_agent_vs_shared, skill_lock_load,
};

use super::SkillService;

impl SkillService {
    /// List skills from the shared source **and** agent-private roots.
    pub fn list_installed(&self) -> Result<Vec<InstalledSkill>> {
        let mut out: Vec<InstalledSkill> = Vec::new();
        let lock = skill_lock_load(&self.source_root)?;

        // Shared source skills (projectable).
        for skill in self.list()? {
            let source = lock.get(&skill.id).cloned();
            out.push(installed_skill_from_shared(
                &self.source_root,
                skill,
                source,
            ));
        }

        let shared_ids: HashSet<String> = out.iter().map(|s| s.id.clone()).collect();

        // Per shared skill: (file index, lazy content hashes) reused across agents.
        let mut shared_cmp_cache: HashMap<
            String,
            (Option<BTreeMap<String, u64>>, Option<BTreeMap<String, u64>>),
        > = HashMap::new();

        // Agent workspace roots: every on-disk skill under each agent, including
        // projections/copies of shared ids.
        // - Not in shared → PrivateSource (可加入共享库)
        // - Same id + content equal / link to shared → Available (已在共享库)
        // - Same id + content differs → Conflict (有冲突，可覆盖加入)
        let source_root = &self.source_root;
        for_each_agent_skill_dir(&self.registry, |entry| {
            let map_status = if shared_ids.contains(&entry.skill_id) {
                let shared_dir = source_root.join(&entry.skill_id);
                map_status_agent_vs_shared(
                    &entry.skill_id,
                    &shared_dir,
                    &entry.path,
                    &mut shared_cmp_cache,
                )
            } else {
                SkillMapStatus::PrivateSource
            };
            out.push(installed_skill_from_agent(entry, map_status, None));
        });

        out.sort_by(|a, b| a.id.cmp(&b.id).then(a.origin.cmp(&b.origin)));
        Ok(out)
    }

    /// Catalog view: shared library rows plus agent-private skills that are **not**
    /// already in the shared library (id match only — no cross-agent merge here).
    ///
    /// - Shared rows match [`Self::list`] content: `origin=shared`, projectable, full projections.
    /// - Agent-only ids emit one row per agent directory (`projections` is empty).
    /// - Same id under two agents (and not in shared) stays two rows; private rows
    ///   carry `content_hash` so the GUI can merge identical copies after visibility
    ///   filtering.
    /// - Agent copies / conflicts of a shared id are omitted.
    pub fn list_catalog(&self) -> Result<Vec<InstalledSkill>> {
        let mut out: Vec<InstalledSkill> = Vec::new();
        let lock = skill_lock_load(&self.source_root)?;

        for skill in self.list()? {
            let source = lock.get(&skill.id).cloned();
            out.push(installed_skill_from_shared(
                &self.source_root,
                skill,
                source,
            ));
        }

        let shared_ids: HashSet<String> = out.iter().map(|s| s.id.clone()).collect();

        for_each_agent_skill_dir(&self.registry, |entry| {
            if shared_ids.contains(&entry.skill_id) {
                return;
            }
            let content_hash = fingerprint_skill_tree(&entry.path);
            out.push(installed_skill_from_agent(
                entry,
                SkillMapStatus::PrivateSource,
                content_hash,
            ));
        });

        out.sort_by(|a, b| a.id.cmp(&b.id).then(a.origin.cmp(&b.origin)));
        Ok(out)
    }
}
