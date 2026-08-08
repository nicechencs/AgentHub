//! Install contribution registry.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::platform::AgentKey;

use super::contribution::InstallContribution;
use super::sources;

#[derive(Clone, Default)]
pub struct InstallContributionRegistry {
    by_key: HashMap<AgentKey, Arc<dyn InstallContribution>>,
    registration_order: Vec<AgentKey>,
}

impl InstallContributionRegistry {
    pub fn new() -> Self {
        Self {
            by_key: HashMap::new(),
            registration_order: Vec::new(),
        }
    }

    pub fn register(&mut self, contrib: Arc<dyn InstallContribution>) -> Result<()> {
        let key = contrib.agent_key();
        if self.by_key.contains_key(&key) {
            return Err(AppError::InvalidArg(format!(
                "duplicate install contribution key: {key}"
            )));
        }
        self.registration_order.push(key.clone());
        self.by_key.insert(key, contrib);
        Ok(())
    }

    pub fn get(&self, key: &AgentKey) -> Option<Arc<dyn InstallContribution>> {
        self.by_key.get(key).cloned()
    }

    /// Compatibility alias for the former key-specific lookup name.
    pub fn get_key(&self, key: &AgentKey) -> Option<Arc<dyn InstallContribution>> {
        self.get(key)
    }

    /// Compatibility lookup for callers that still receive the closed AgentId DTO.
    pub fn get_agent_id(&self, agent: AgentId) -> Option<Arc<dyn InstallContribution>> {
        self.get(&AgentKey::from_agent_id(agent))
    }

    pub fn contains_key(&self, key: &AgentKey) -> bool {
        self.by_key.contains_key(key)
    }

    /// Compatibility helper for callers that still receive the closed AgentId DTO.
    pub fn contains_agent_id(&self, agent: AgentId) -> bool {
        self.contains_key(&AgentKey::from_agent_id(agent))
    }

    /// Compatibility name retained for existing AgentId callers.
    pub fn contains(&self, agent: AgentId) -> bool {
        self.contains_agent_id(agent)
    }

    pub fn supported_agent_keys(&self) -> Vec<AgentKey> {
        self.registration_order.clone()
    }

    pub fn all_ordered(&self) -> Vec<Arc<dyn InstallContribution>> {
        self.registration_order
            .iter()
            .filter_map(|key| self.get(key))
            .collect()
    }
}

pub fn builtin_install_registry() -> &'static InstallContributionRegistry {
    static REG: OnceLock<InstallContributionRegistry> = OnceLock::new();
    REG.get_or_init(sources::build_registry)
}
