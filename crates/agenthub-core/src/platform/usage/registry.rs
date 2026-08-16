//! Compile-time registry of optional UsageSource contributions.

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::platform::AgentKey;

use super::source::UsageSource;

/// Lookup table keyed by the open AgentKey identity.
#[derive(Clone, Default)]
pub struct UsageSourceRegistry {
    by_key: HashMap<AgentKey, Arc<dyn UsageSource>>,
    registration_order: Vec<AgentKey>,
}

impl UsageSourceRegistry {
    pub fn new() -> Self {
        Self {
            by_key: HashMap::new(),
            registration_order: Vec::new(),
        }
    }

    pub fn register(&mut self, source: Arc<dyn UsageSource>) -> Result<()> {
        let key = source.agent_key();
        if self.by_key.contains_key(&key) {
            return Err(AppError::InvalidArg(format!(
                "duplicate usage source key: {key}"
            )));
        }
        self.registration_order.push(key.clone());
        self.by_key.insert(key, source);
        Ok(())
    }

    pub fn get(&self, key: &AgentKey) -> Option<Arc<dyn UsageSource>> {
        self.by_key.get(key).cloned()
    }

    /// Compatibility alias for the former key lookup name.
    pub fn get_key(&self, key: &AgentKey) -> Option<Arc<dyn UsageSource>> {
        self.get(key)
    }

    /// Compatibility lookup for callers that still receive the closed AgentId DTO.
    pub fn get_agent_id(&self, agent: AgentId) -> Option<Arc<dyn UsageSource>> {
        self.get(&AgentKey::from_agent_id(agent))
    }

    /// Keys in explicit registration order.
    pub fn supported_agent_keys(&self) -> Vec<AgentKey> {
        self.registration_order.clone()
    }

    /// Legacy closed identities in product order.
    pub fn supported_agents(&self) -> Vec<AgentId> {
        AgentId::ALL
            .iter()
            .copied()
            .filter(|agent| self.contains_agent_id(*agent))
            .collect()
    }

    pub fn all_ordered(&self) -> Vec<Arc<dyn UsageSource>> {
        self.registration_order
            .iter()
            .filter_map(|key| self.get(key))
            .collect()
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
}

/// Process-wide builtin sources (filled by `integrations::register_integrations`).
pub fn builtin_usage_registry() -> &'static UsageSourceRegistry {
    &crate::integrations::production_integrations().usage
}
