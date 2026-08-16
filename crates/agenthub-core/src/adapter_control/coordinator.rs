//! Process-local profile and target-agent saga gates.
//!
//! Owned by hosts (desktop AppState today; sidecar later). Not a global, and
//! intentionally free of Tauri types so GUI and CLI can share the same gate.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::models::AgentId;

/// Serializes adapter / local_bridge mutations per profile and per target Agent.
///
/// Every operation for one profile takes the same lock. Provider-changing
/// stages also serialize against other projections for that target Agent
/// before a live config snapshot is captured.
pub struct AdapterSagaCoordinator {
    profiles: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    targets: Mutex<HashMap<AgentId, Arc<tokio::sync::Mutex<()>>>>,
}

impl AdapterSagaCoordinator {
    pub fn new() -> Self {
        Self {
            profiles: Mutex::new(HashMap::new()),
            targets: Mutex::new(HashMap::new()),
        }
    }

    /// Lock one durable bridge / adapter profile for its lifecycle saga.
    pub async fn lock_profile(&self, profile_id: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut profiles = self
                .profiles
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Arc::clone(
                profiles
                    .entry(profile_id.to_owned())
                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
            )
        };
        lock.lock_owned().await
    }

    /// The single authority for mutations that can change one target agent's
    /// live configuration or authentication. The lock is per-agent so a Claude
    /// operation never unnecessarily blocks Codex, while all Codex paths share
    /// exactly the same authority as a bridge saga.
    pub async fn lock_target(&self, agent: AgentId) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut targets = self
                .targets
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Arc::clone(
                targets
                    .entry(agent)
                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
            )
        };
        lock.lock_owned().await
    }
}

impl Default for AdapterSagaCoordinator {
    fn default() -> Self {
        Self::new()
    }
}
