//! Path contribution registry.

use std::collections::HashMap;
use std::sync::Arc;

use crate::models::AgentId;

use super::contribution::AgentPathContribution;

#[derive(Clone, Default)]
pub struct AgentPathRegistry {
    by_id: HashMap<AgentId, Arc<dyn AgentPathContribution>>,
}

impl AgentPathRegistry {
    pub fn new() -> Self {
        Self {
            by_id: HashMap::new(),
        }
    }

    pub fn register(&mut self, c: Arc<dyn AgentPathContribution>) {
        self.by_id.insert(c.agent_id(), c);
    }

    pub fn get(&self, agent: AgentId) -> Option<Arc<dyn AgentPathContribution>> {
        self.by_id.get(&agent).cloned()
    }

    pub fn contains(&self, agent: AgentId) -> bool {
        self.by_id.contains_key(&agent)
    }
}

pub fn builtin_path_registry() -> &'static AgentPathRegistry {
    &crate::integrations::production_integrations().paths
}
