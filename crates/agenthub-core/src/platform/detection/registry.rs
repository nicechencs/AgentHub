//! Agent detector registry keyed by open AgentKey identity.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use crate::error::{AppError, Result};
use crate::models::AgentId;
use crate::platform::AgentKey;

use super::detector::{AdapterDetector, AgentDetector};

#[derive(Clone, Default)]
pub struct DetectorRegistry {
    by_key: HashMap<AgentKey, Arc<dyn AgentDetector>>,
    registration_order: Vec<AgentKey>,
}

impl DetectorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, detector: Arc<dyn AgentDetector>) -> Result<()> {
        let key = detector.agent_key();
        if self.by_key.contains_key(&key) {
            return Err(AppError::InvalidArg(format!(
                "duplicate agent detector key: {key}"
            )));
        }
        self.registration_order.push(key.clone());
        self.by_key.insert(key, detector);
        Ok(())
    }

    pub fn get(&self, key: &AgentKey) -> Option<Arc<dyn AgentDetector>> {
        self.by_key.get(key).cloned()
    }

    /// Compatibility lookup for callers that still receive the closed AgentId DTO.
    pub fn get_agent_id(&self, agent: AgentId) -> Option<Arc<dyn AgentDetector>> {
        self.get(&AgentKey::from_agent_id(agent))
    }

    pub fn contains_key(&self, key: &AgentKey) -> bool {
        self.by_key.contains_key(key)
    }

    pub fn supported_agent_keys(&self) -> Vec<AgentKey> {
        self.registration_order.clone()
    }

    pub fn all_ordered(&self) -> Vec<Arc<dyn AgentDetector>> {
        self.registration_order
            .iter()
            .filter_map(|key| self.get(key))
            .collect()
    }
}

fn build_registry() -> DetectorRegistry {
    let adapters = crate::adapters::register_all();
    let mut registry = DetectorRegistry::new();
    for adapter in adapters.all() {
        registry
            .register(Arc::new(AdapterDetector::new(adapter)))
            .expect("unique built-in detector");
    }
    registry
}

pub fn builtin_detector_registry() -> &'static DetectorRegistry {
    static REGISTRY: OnceLock<DetectorRegistry> = OnceLock::new();
    REGISTRY.get_or_init(build_registry)
}
