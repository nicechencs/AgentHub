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

/// Standalone detector: `AgentKey` + detect closure — no `AgentAdapter` required.
///
/// Production builtins and test-only agents both use this shape so detect can be
/// registered without implementing the full adapter surface.
pub struct FnDetector {
    key: AgentKey,
    detect_fn: Arc<dyn Fn() -> InstallationObserved + Send + Sync>,
}

impl FnDetector {
    pub fn new(
        key: AgentKey,
        detect_fn: impl Fn() -> InstallationObserved + Send + Sync + 'static,
    ) -> Self {
        Self {
            key,
            detect_fn: Arc::new(detect_fn),
        }
    }
}

impl AgentDetector for FnDetector {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn detect(&self) -> InstallationObserved {
        (self.detect_fn)()
    }
}
