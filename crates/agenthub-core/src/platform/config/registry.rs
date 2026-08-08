//! Compile-time registry of optional AgentConfigProjector contributions.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::platform::AgentKey;

use super::projector::AgentConfigProjector;
use super::sources;

/// Lookup table keyed by the open AgentKey identity.
#[derive(Clone, Default)]
pub struct ConfigProjectorRegistry {
    by_key: HashMap<AgentKey, Arc<dyn AgentConfigProjector>>,
    registration_order: Vec<AgentKey>,
}

impl ConfigProjectorRegistry {
    pub fn new() -> Self {
        Self {
            by_key: HashMap::new(),
            registration_order: Vec::new(),
        }
    }

    pub fn register(&mut self, projector: Arc<dyn AgentConfigProjector>) -> Result<()> {
        let key = projector.agent_key();
        if self.by_key.contains_key(&key) {
            return Err(AppError::InvalidArg(format!(
                "duplicate config projector key: {key}"
            )));
        }
        self.registration_order.push(key.clone());
        self.by_key.insert(key, projector);
        Ok(())
    }

    pub fn get(&self, key: &AgentKey) -> Option<Arc<dyn AgentConfigProjector>> {
        self.by_key.get(key).cloned()
    }

    /// Compatibility alias for the former key lookup name.
    pub fn get_key(&self, key: &AgentKey) -> Option<Arc<dyn AgentConfigProjector>> {
        self.get(key)
    }

    /// Compatibility lookup for callers that still receive the closed AgentId DTO.
    pub fn get_agent_id(&self, agent: AgentId) -> Option<Arc<dyn AgentConfigProjector>> {
        self.get(&AgentKey::from_agent_id(agent))
    }

    pub fn contains(&self, agent: AgentId) -> bool {
        self.contains_key(&AgentKey::from_agent_id(agent))
    }

    pub fn contains_key(&self, key: &AgentKey) -> bool {
        self.by_key.contains_key(key)
    }

    pub fn supported_agent_keys(&self) -> Vec<AgentKey> {
        self.registration_order.clone()
    }

    pub fn supported_agents(&self) -> Vec<AgentId> {
        AgentId::ALL
            .iter()
            .copied()
            .filter(|agent| self.contains(*agent))
            .collect()
    }
}

fn build_registry() -> ConfigProjectorRegistry {
    let mut reg = ConfigProjectorRegistry::new();
    sources::register_all(&mut reg);
    reg
}

/// Process-wide builtin projectors (Claude / Codex / Kimi / Grok).
pub fn builtin_config_registry() -> &'static ConfigProjectorRegistry {
    static REG: OnceLock<ConfigProjectorRegistry> = OnceLock::new();
    REG.get_or_init(build_registry)
}
