//! Built-in agent detectors registered without a full [`crate::adapters::AdapterRegistry`].
//!
//! Each entry is a [`super::detector::FnDetector`] that calls a free
//! `detect_*_installation` helper re-exported from `adapters`. Adapters still
//! implement `AgentAdapter::detect` by delegating to the same helper (transition).

use std::sync::Arc;

use crate::models::AgentId;
use crate::platform::AgentKey;

use super::detector::{AgentDetector, FnDetector};
use super::registry::DetectorRegistry;

fn key(agent: AgentId) -> AgentKey {
    AgentKey::from_agent_id(agent)
}

fn register_observe(
    registry: &mut DetectorRegistry,
    agent: AgentId,
    observe: fn() -> crate::models::DetectResult,
) {
    let agent_key = key(agent);
    registry
        .register(Arc::new(FnDetector::new(agent_key, move || {
            observe().into()
        })) as Arc<dyn AgentDetector>)
        .expect("unique built-in detector");
}

/// Build the production detector set in [`AgentId::ALL`] order.
///
/// Does **not** call [`crate::adapters::register_all`].
pub fn build_registry() -> DetectorRegistry {
    let mut registry = DetectorRegistry::new();
    register_observe(
        &mut registry,
        AgentId::Claude,
        crate::adapters::detect_claude_installation,
    );
    register_observe(
        &mut registry,
        AgentId::Codex,
        crate::adapters::detect_codex_installation,
    );
    register_observe(
        &mut registry,
        AgentId::Kimi,
        crate::adapters::detect_kimi_installation,
    );
    register_observe(
        &mut registry,
        AgentId::Grok,
        crate::adapters::detect_grok_installation,
    );
    register_observe(
        &mut registry,
        AgentId::Pi,
        crate::adapters::detect_pi_installation,
    );
    register_observe(
        &mut registry,
        AgentId::WorkBuddy,
        crate::adapters::detect_workbuddy_installation,
    );
    register_observe(
        &mut registry,
        AgentId::Cursor,
        crate::adapters::detect_cursor_installation,
    );
    register_observe(
        &mut registry,
        AgentId::Dsh,
        crate::adapters::detect_dsh_installation,
    );
    registry
}
