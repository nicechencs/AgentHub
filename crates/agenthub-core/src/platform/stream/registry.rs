//! Compile-time StreamParser registry.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use crate::models::AgentId;
use crate::platform::AgentKey;

use super::parser::{StreamParseError, StreamParser};
use super::sources;

#[derive(Clone, Default)]
pub struct StreamParserRegistry {
    by_key: HashMap<AgentKey, Arc<dyn StreamParser>>,
    registration_order: Vec<AgentKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamParserRegistryError {
    pub agent_key: AgentKey,
}

impl std::fmt::Display for StreamParserRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "duplicate stream parser key: {}", self.agent_key)
    }
}

impl std::error::Error for StreamParserRegistryError {}

impl StreamParserRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        parser: Arc<dyn StreamParser>,
    ) -> Result<(), StreamParserRegistryError> {
        let key = parser.agent_key();
        if self.by_key.contains_key(&key) {
            return Err(StreamParserRegistryError { agent_key: key });
        }
        self.registration_order.push(key.clone());
        self.by_key.insert(key, parser);
        Ok(())
    }

    pub fn get(&self, key: &AgentKey) -> Option<Arc<dyn StreamParser>> {
        self.by_key.get(key).cloned()
    }

    /// Compatibility alias for the former key-specific lookup name.
    pub fn get_key(&self, key: &AgentKey) -> Option<Arc<dyn StreamParser>> {
        self.get(key)
    }

    /// Compatibility façade for callers that still use the closed built-in id.
    pub fn get_agent_id(&self, agent: AgentId) -> Option<Arc<dyn StreamParser>> {
        self.get(&AgentKey::from_agent_id(agent))
    }

    /// Require a parser by its native key or return typed unsupported.
    pub fn require_key(&self, key: &AgentKey) -> Result<Arc<dyn StreamParser>, StreamParseError> {
        self.get(key)
            .ok_or_else(|| StreamParseError::unsupported_key(key))
    }

    /// Compatibility façade for callers that still use the closed built-in id.
    pub fn require(&self, agent: AgentId) -> Result<Arc<dyn StreamParser>, StreamParseError> {
        self.require_key(&AgentKey::from_agent_id(agent))
    }

    pub fn contains_key(&self, key: &AgentKey) -> bool {
        self.by_key.contains_key(key)
    }

    /// Compatibility façade for callers that still use the closed built-in id.
    pub fn contains(&self, agent: AgentId) -> bool {
        self.contains_key(&AgentKey::from_agent_id(agent))
    }

    /// Native parser keys in explicit registration order.
    pub fn supported_keys(&self) -> Vec<AgentKey> {
        self.registration_order.clone()
    }

    /// Compatibility projection containing only known built-in identities.
    pub fn supported_agents(&self) -> Vec<AgentId> {
        self.registration_order
            .iter()
            .filter_map(|key| {
                AgentId::ALL
                    .iter()
                    .copied()
                    .find(|agent| agent.as_str() == key.as_str())
            })
            .collect()
    }
}

pub fn builtin_stream_registry() -> &'static StreamParserRegistry {
    static REG: OnceLock<StreamParserRegistry> = OnceLock::new();
    REG.get_or_init(sources::build_registry)
}
