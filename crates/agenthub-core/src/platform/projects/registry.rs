//! ProjectSource registry (filled by service-layer builtin registration).

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::platform::AgentKey;

use super::source::ProjectSource;

#[derive(Clone, Default)]
pub struct ProjectSourceRegistry {
    by_key: HashMap<AgentKey, Arc<dyn ProjectSource>>,
    order: Vec<AgentKey>,
}

impl ProjectSourceRegistry {
    pub fn new() -> Self {
        Self {
            by_key: HashMap::new(),
            order: Vec::new(),
        }
    }

    pub fn register(&mut self, source: Arc<dyn ProjectSource>) -> Result<()> {
        let key = source.agent_key();
        if self.by_key.contains_key(&key) {
            return Err(AppError::InvalidArg(format!(
                "project source already registered: {key}"
            )));
        }
        self.order.push(key.clone());
        self.by_key.insert(key, source);
        Ok(())
    }

    pub fn get(&self, key: &AgentKey) -> Option<Arc<dyn ProjectSource>> {
        self.by_key.get(key).cloned()
    }

    /// Compatibility façade for callers that still use the closed built-in enum.
    pub fn get_agent_id(&self, agent: AgentId) -> Option<Arc<dyn ProjectSource>> {
        let key = AgentKey::from_agent_id(agent);
        self.get(&key)
    }

    pub fn contains_key(&self, key: &AgentKey) -> bool {
        self.by_key.contains_key(key)
    }

    /// Compatibility façade for callers that still use the closed built-in enum.
    pub fn contains(&self, agent: AgentId) -> bool {
        let key = AgentKey::from_agent_id(agent);
        self.contains_key(&key)
    }

    /// Explicit registration order; callers must register built-ins in product order.
    pub fn all_ordered(&self) -> Vec<Arc<dyn ProjectSource>> {
        self.order
            .iter()
            .filter_map(|key| self.by_key.get(key).cloned())
            .collect()
    }

    pub fn supported_keys(&self) -> Vec<AgentKey> {
        self.order.clone()
    }

    /// Compatibility projection for the closed built-in enum.
    pub fn supported_agents(&self) -> Vec<AgentId> {
        AgentId::ALL
            .into_iter()
            .filter(|agent| self.contains(*agent))
            .collect()
    }
}

/// Empty registry (tests / before builtin registration).
pub fn empty_registry() -> ProjectSourceRegistry {
    ProjectSourceRegistry::new()
}
