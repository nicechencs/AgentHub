//! Agent skill projection targets (P12 / P1-3).
//!
//! An [`AgentSkillTarget`] describes where a skill may be projected for one
//! agent. Production builtins register [`StaticSkillTarget`] from path roots
//! without requiring a full [`crate::adapters::AdapterRegistry`]. Unsupported
//! agents are omitted (no silent port). Unmanaged directories are never
//! auto-deleted.
//!
//! [`AdapterSkillTarget`] remains available as a compatibility wrapper.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::adapters::{AdapterRegistry, AgentAdapter};
use crate::error::{AppError, Result};
use crate::models::{AgentId, Capability};
use crate::platform::AgentKey;

/// Describes one agent's skill projection root.
pub trait AgentSkillTarget: Send + Sync {
    fn agent_key(&self) -> AgentKey;

    /// Absolute skills root for this agent, when known.
    fn skills_root(&self) -> Option<PathBuf>;

    /// Whether the agent can accept skill projections.
    fn supports_skills(&self) -> bool;
}

/// Adapter-backed skill target (compatibility / transition path).
#[derive(Clone)]
pub struct AdapterSkillTarget {
    adapter: Arc<dyn AgentAdapter>,
}

impl AdapterSkillTarget {
    pub fn new(adapter: Arc<dyn AgentAdapter>) -> Self {
        Self { adapter }
    }
}

impl AgentSkillTarget for AdapterSkillTarget {
    fn agent_key(&self) -> AgentKey {
        AgentKey::from_agent_id(self.adapter.id())
    }

    fn skills_root(&self) -> Option<PathBuf> {
        self.adapter.skills_dir()
    }

    fn supports_skills(&self) -> bool {
        self.adapter.capability(Capability::Skills).is_usable()
            && self.adapter.skills_dir().is_some()
    }
}

/// Explicit target — production builtins, tests, and demos (no adapter required).
#[derive(Debug, Clone)]
pub struct StaticSkillTarget {
    pub agent_key: AgentKey,
    pub skills_root: Option<PathBuf>,
    pub supports: bool,
}

impl AgentSkillTarget for StaticSkillTarget {
    fn agent_key(&self) -> AgentKey {
        self.agent_key.clone()
    }

    fn skills_root(&self) -> Option<PathBuf> {
        self.skills_root.clone()
    }

    fn supports_skills(&self) -> bool {
        self.supports && self.skills_root.is_some()
    }
}

/// Registry of skill targets keyed by [`AgentKey`].
#[derive(Default, Clone)]
pub struct SkillTargetRegistry {
    targets: BTreeMap<AgentKey, Arc<dyn AgentSkillTarget>>,
    registration_order: Vec<AgentKey>,
}

impl SkillTargetRegistry {
    pub fn new() -> Self {
        Self {
            targets: BTreeMap::new(),
            registration_order: Vec::new(),
        }
    }

    pub fn register(&mut self, target: Arc<dyn AgentSkillTarget>) -> Result<()> {
        let key = target.agent_key();
        if self.targets.contains_key(&key) {
            return Err(AppError::message(
                "skill.target_duplicate",
                format!("skill target already registered for agent {key}"),
            ));
        }
        self.registration_order.push(key.clone());
        self.targets.insert(key, target);
        Ok(())
    }

    pub fn get(&self, agent_key: &AgentKey) -> Option<&Arc<dyn AgentSkillTarget>> {
        self.targets.get(agent_key)
    }

    /// Targets in explicit registration order.
    pub fn all(&self) -> impl Iterator<Item = &Arc<dyn AgentSkillTarget>> {
        self.registration_order
            .iter()
            .filter_map(|key| self.targets.get(key))
    }

    pub fn contains_key(&self, agent_key: &AgentKey) -> bool {
        self.targets.contains_key(agent_key)
    }

    pub fn supported_agent_keys(&self) -> Vec<AgentKey> {
        self.registration_order.clone()
    }

    /// Build from [`AdapterRegistry`]: only agents with usable Skills + skills_dir.
    ///
    /// Compatibility path — prefer [`builtin_skill_target_registry`] in production.
    pub fn from_adapter_registry(registry: &AdapterRegistry) -> Result<Self> {
        let mut out = Self::new();
        for agent in AgentId::ALL {
            let Some(adapter) = registry.get(agent) else {
                continue;
            };
            if !adapter.capability(Capability::Skills).is_usable() {
                continue;
            }
            if adapter.skills_dir().is_none() {
                continue;
            }
            out.register(Arc::new(AdapterSkillTarget::new(Arc::clone(&adapter))))?;
        }
        Ok(out)
    }
}

/// Production skill targets from path contributions — no fat adapter registry.
///
/// Membership is filled by `integrations::register_integrations` (Kimi omitted).
pub fn build_builtin_skill_targets() -> Result<SkillTargetRegistry> {
    Ok(crate::integrations::production_integrations()
        .skills
        .clone())
}

/// Process-wide builtin skill target registry.
pub fn builtin_skill_target_registry() -> &'static SkillTargetRegistry {
    &crate::integrations::production_integrations().skills
}
