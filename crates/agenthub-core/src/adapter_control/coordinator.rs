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
    local_gateway: Arc<tokio::sync::Mutex<()>>,
    local_gateway_restart_pending: Mutex<HashMap<String, LocalGatewayRestartPending>>,
}

#[derive(Clone)]
struct LocalGatewayRestartPending {
    pool_id: String,
    mutation_kind: LocalGatewayMutationKind,
    mutation_committed: bool,
    restart_required: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalGatewayMutationKind {
    Create,
    Set,
    Delete,
}

impl AdapterSagaCoordinator {
    pub fn new() -> Self {
        Self {
            profiles: Mutex::new(HashMap::new()),
            targets: Mutex::new(HashMap::new()),
            local_gateway: Arc::new(tokio::sync::Mutex::new(())),
            local_gateway_restart_pending: Mutex::new(HashMap::new()),
        }
    }

    /// Lock one durable bridge / adapter profile for its lifecycle saga.
    pub async fn lock_profile(&self, profile_id: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut profiles = self
                .profiles
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            // Keep only entries still cloned by a holder or waiter (map Arc is otherwise the last).
            profiles.retain(|_, lock| Arc::strong_count(lock) > 1);
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

    /// Serialize mutations that can change the shared listener configuration,
    /// its accepted entry keys, or a generated provider pointing at it. Always
    /// acquire this before a profile or target lock.
    pub async fn lock_local_gateway(&self) -> tokio::sync::OwnedMutexGuard<()> {
        Arc::clone(&self.local_gateway).lock_owned().await
    }

    /// Remember a stopped edge until a later Key operation has published the
    /// persisted bearer table and restarted it. This is process-local recovery
    /// state; normal application restore remains the cross-process fallback.
    pub fn mark_local_gateway_restart_pending(
        &self,
        operation_id: &str,
        pool_id: &str,
        mutation_kind: LocalGatewayMutationKind,
        mutation_committed: bool,
        restart_required: bool,
    ) {
        self.local_gateway_restart_pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                operation_id.to_owned(),
                LocalGatewayRestartPending {
                    pool_id: pool_id.to_owned(),
                    mutation_kind,
                    mutation_committed,
                    restart_required,
                },
            );
    }

    pub fn mark_local_gateway_mutation_committed(&self, operation_id: &str) {
        if let Some(pending) = self
            .local_gateway_restart_pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_mut(operation_id)
        {
            pending.mutation_committed = true;
        }
    }

    pub fn local_gateway_restart_pending(
        &self,
    ) -> Vec<(String, String, LocalGatewayMutationKind, bool, bool)> {
        self.local_gateway_restart_pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .map(|(operation_id, pending)| {
                (
                    operation_id.clone(),
                    pending.pool_id.clone(),
                    pending.mutation_kind,
                    pending.mutation_committed,
                    pending.restart_required,
                )
            })
            .collect()
    }

    pub fn clear_local_gateway_restart_pending_for_pool(&self, pool_id: &str) {
        self.local_gateway_restart_pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, pending| pending.pool_id != pool_id);
    }

    #[cfg(test)]
    pub(crate) fn profile_lock_count(&self) -> usize {
        self.profiles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}

impl Default for AdapterSagaCoordinator {
    fn default() -> Self {
        Self::new()
    }
}
