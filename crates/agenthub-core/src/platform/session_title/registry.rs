//! SessionTitleSource registry (filled by platform builtin sources).

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::platform::AgentKey;

use super::source::SessionTitleSource;

#[derive(Clone, Default)]
pub struct SessionTitleRegistry {
    by_key: HashMap<AgentKey, Arc<dyn SessionTitleSource>>,
    order: Vec<AgentKey>,
}

impl SessionTitleRegistry {
    pub fn new() -> Self {
        Self {
            by_key: HashMap::new(),
            order: Vec::new(),
        }
    }

    pub fn register(&mut self, source: Arc<dyn SessionTitleSource>) -> Result<()> {
        let key = source.agent_key();
        if self.by_key.contains_key(&key) {
            return Err(AppError::InvalidArg(format!(
                "session title source already registered: {key}"
            )));
        }
        self.order.push(key.clone());
        self.by_key.insert(key, source);
        Ok(())
    }

    pub fn get(&self, key: &AgentKey) -> Option<Arc<dyn SessionTitleSource>> {
        self.by_key.get(key).cloned()
    }

    /// Compatibility façade for callers that still use the closed built-in enum.
    pub fn get_agent_id(&self, agent: AgentId) -> Option<Arc<dyn SessionTitleSource>> {
        self.get(&AgentKey::from_agent_id(agent))
    }

    /// Explicit registration order; callers must register built-ins in product order.
    pub fn supported_keys(&self) -> Vec<AgentKey> {
        self.order.clone()
    }
}

/// Empty registry (tests / before builtin registration).
pub fn empty_registry() -> SessionTitleRegistry {
    SessionTitleRegistry::new()
}
