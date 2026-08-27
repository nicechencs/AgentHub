//! Adapter registry: behavior owner (detect / install / run / capability / require).
//!
//! [`register_all`] is the builtin adapter-set source of truth.
//! `shared_registry()` is that same set for capability hot paths — not a second
//! product table. Catalog snapshots are built only via
//! [`crate::platform::AgentCatalogService::from_registry`]. CLI capability
//! matrix uses this registry; the UI product directory uses catalog. Catalog
//! must not run detect/install; this registry is not the UI directory source of
//! truth.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

use crate::error::{AppError, Result};
use crate::models::{AgentId, Capability, CapabilityLevel, CapabilityState};

use super::AgentAdapter;

/// Process-wide adapter registry (same set as [`register_all`]).
///
/// Used by hot paths that only need capability lookups without rebuilding the map.
fn shared_registry() -> &'static AdapterRegistry {
    static REGISTRY: OnceLock<AdapterRegistry> = OnceLock::new();
    REGISTRY.get_or_init(register_all)
}

/// Whether this agent supports structured stream output (matrix cell).
pub fn supports_structured_stream(agent: AgentId) -> bool {
    shared_registry()
        .get(agent)
        .map(|a| a.capability(Capability::StructuredStream).is_usable())
        .unwrap_or(false)
}

/// ProcessMode + capability matrix lookup (models cannot import adapters).
pub fn wants_structured_for(mode: crate::models::ProcessMode, agent: AgentId) -> bool {
    mode.wants_structured(supports_structured_stream(agent))
}

#[derive(Clone)]
pub struct AdapterRegistry {
    adapters: HashMap<AgentId, Arc<dyn AgentAdapter>>,
    /// Insertion order for registered adapters (catalog / open-key composition).
    ///
    /// Kept as [`AgentId`] so this layer stays free of `platform::AgentKey`.
    /// Callers that need open identity convert via `AgentKey::from_agent_id`.
    registration_order: Vec<AgentId>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
            registration_order: Vec::new(),
        }
    }

    pub fn register(&mut self, adapter: Arc<dyn AgentAdapter>) {
        let id = adapter.id();
        let is_new = !self.adapters.contains_key(&id);
        self.adapters.insert(id, adapter);
        if is_new {
            self.registration_order.push(id);
        }
    }

    pub fn get(&self, id: AgentId) -> Option<Arc<dyn AgentAdapter>> {
        self.adapters.get(&id).cloned()
    }

    /// Registered agents in insertion order (not [`AgentId::ALL`]).
    pub fn registered_agents(&self) -> &[AgentId] {
        &self.registration_order
    }

    pub fn all(&self) -> Vec<Arc<dyn AgentAdapter>> {
        // Compatibility: still filter through AgentId::ALL for callers that lock
        // product order to that closed list (usage/matrix/etc.). Catalog uses
        // [`Self::registered_agents`] instead.
        AgentId::ALL
            .iter()
            .filter_map(|id| self.adapters.get(id).cloned())
            .collect()
    }

    /// Global capability matrix for GUI / CLI / docs.
    pub fn matrix(&self) -> BTreeMap<AgentId, BTreeMap<Capability, CapabilityState>> {
        let mut out = BTreeMap::new();
        for adapter in self.all() {
            let mut row = BTreeMap::new();
            for cap in Capability::ALL {
                row.insert(cap, adapter.capability(cap));
            }
            out.insert(adapter.id(), row);
        }
        out
    }

    /// Gate a call site on a declared capability. Partial is allowed through.
    pub fn require(&self, agent: AgentId, cap: Capability) -> Result<Arc<dyn AgentAdapter>> {
        use crate::logging::targets;

        let adapter = self.get(agent).ok_or_else(|| {
            AppError::NotFound(format!("adapter not registered: {}", agent.as_str()))
        })?;
        let state = adapter.capability(cap);
        match state.level {
            CapabilityLevel::Full => Ok(adapter),
            CapabilityLevel::Partial => {
                tracing::debug!(
                    target: targets::CAPABILITY,
                    module = targets::CAPABILITY,
                    op = "require",
                    agent = agent.as_str(),
                    capability = cap.as_str(),
                    level = "partial",
                    reason = state.reason.unwrap_or(""),
                    "capability allowed with degradation"
                );
                Ok(adapter)
            }
            CapabilityLevel::Unsupported => {
                let reason = state.reason.unwrap_or("未提供原因");
                tracing::warn!(
                    target: targets::CAPABILITY,
                    module = targets::CAPABILITY,
                    op = "require",
                    agent = agent.as_str(),
                    capability = cap.as_str(),
                    level = "unsupported",
                    reason,
                    "capability blocked"
                );
                Err(AppError::Unsupported(format!(
                    "{} 不支持{}：{}",
                    agent.display_name(),
                    cap.label(),
                    reason
                )))
            }
            CapabilityLevel::Planned => {
                let reason = state.reason.unwrap_or("路线图项");
                tracing::info!(
                    target: targets::CAPABILITY,
                    module = targets::CAPABILITY,
                    op = "require",
                    agent = agent.as_str(),
                    capability = cap.as_str(),
                    level = "planned",
                    reason,
                    "capability not wired yet"
                );
                Err(AppError::Unsupported(format!(
                    "{}的{}尚未接入 AgentHub：{}",
                    agent.display_name(),
                    cap.label(),
                    reason
                )))
            }
        }
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        register_all()
    }
}

pub fn register_all() -> AdapterRegistry {
    let mut reg = AdapterRegistry::new();
    reg.register(Arc::new(super::claude::ClaudeAdapter));
    reg.register(Arc::new(super::codex::CodexAdapter));
    reg.register(Arc::new(super::kimi::KimiAdapter));
    reg.register(Arc::new(super::grok::GrokAdapter));
    reg.register(Arc::new(super::pi::PiAdapter));
    reg.register(Arc::new(super::workbuddy::WorkBuddyAdapter));
    reg.register(Arc::new(super::cursor::CursorAdapter));
    reg.register(Arc::new(super::dsh::DshAdapter));
    reg
}
