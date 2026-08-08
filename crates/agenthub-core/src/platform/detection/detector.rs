//! Agent installation detection port.

use std::sync::Arc;

use crate::adapters::AgentAdapter;
use crate::platform::lifecycle::InstallationObserved;
use crate::platform::AgentKey;

/// Narrow platform capability for observing one agent's installed state.
pub trait AgentDetector: Send + Sync {
    fn agent_key(&self) -> AgentKey;
    fn detect(&self) -> InstallationObserved;
}

/// Compatibility wrapper for built-in agents that still use the legacy adapter.
pub struct AdapterDetector {
    adapter: Arc<dyn AgentAdapter>,
}

impl AdapterDetector {
    pub fn new(adapter: Arc<dyn AgentAdapter>) -> Self {
        Self { adapter }
    }
}

impl AgentDetector for AdapterDetector {
    fn agent_key(&self) -> AgentKey {
        AgentKey::from_agent_id(self.adapter.id())
    }

    fn detect(&self) -> InstallationObserved {
        self.adapter.detect().into()
    }
}
