//! Desktop host implementation of [`agenthub_core::adapter_control::AdapterControl`].
//!
//! Commands parse wire input and call this façade. Reshape bind/unbind go through
//! core [`TicketBindService`]; `local_bridge` still runs the in-process saga in
//! [`crate::adapter_bridge_controller`].

use std::sync::Arc;

use agenthub_core::adapter_control::{
    resolve_bind_action, resolve_unbind_action, AdapterBridgeSagaCoordinator, AdapterBridgeStatus,
    AdapterControl, BindAction,
};
use agenthub_core::bridge::BridgeRuntimeHost;
use agenthub_core::error::AppError;
use agenthub_core::models::{
    AdapterApplyResult, AdapterProfile, AgentId, TicketBinding, TicketUnbindRequest,
};
use agenthub_core::services::ticket_binding_from_apply;
use agenthub_core::AgentHub;

use crate::adapter_bridge_controller::{
    apply_local_bridge, local_bridge_status, remove_adapter_with_bridge_cleanup,
    set_local_bridge_auto_start, start_local_bridge, stop_local_bridge, unbind_local_bridge,
};
use crate::commands::{map_err_string, with_hub_blocking};
use crate::exit_coordinator::LifecycleShutdownBarrier;

/// In-process desktop AdapterControl: TicketBindService + bridge saga.
pub(crate) struct DesktopAdapterControl {
    hub: Arc<AgentHub>,
    host: Arc<BridgeRuntimeHost>,
    coordinator: Arc<AdapterBridgeSagaCoordinator>,
    lifecycle_barrier: Arc<LifecycleShutdownBarrier>,
}

impl DesktopAdapterControl {
    pub(crate) fn new(
        hub: Arc<AgentHub>,
        host: Arc<BridgeRuntimeHost>,
        coordinator: Arc<AdapterBridgeSagaCoordinator>,
        lifecycle_barrier: Arc<LifecycleShutdownBarrier>,
    ) -> Self {
        Self {
            hub,
            host,
            coordinator,
            lifecycle_barrier,
        }
    }
}

impl AdapterControl for DesktopAdapterControl {
    async fn bind(
        &self,
        ticket_id: String,
        target_agent_id: AgentId,
    ) -> Result<TicketBinding, String> {
        let action = {
            let hub = Arc::clone(&self.hub);
            let ticket_id = ticket_id.clone();
            with_hub_blocking(hub, move |hub| {
                resolve_bind_action(hub, &ticket_id, target_agent_id).map_err(|err| match err {
                    // Preserve bracketed adapter codes from plan.reason for the GUI.
                    AppError::Unsupported(reason) => reason,
                    other => map_err_string("bind_ticket", other),
                })
            })
            .await?
        };
        match action {
            BindAction::LocalBridge(request) => {
                let result = apply_local_bridge(
                    Arc::clone(&self.hub),
                    Arc::clone(&self.host),
                    Arc::clone(&self.coordinator),
                    Arc::clone(&self.lifecycle_barrier),
                    request,
                )
                .await?;
                Ok(ticket_binding_from_apply(&ticket_id, &result))
            }
            BindAction::Reshape(request) => {
                let _target_guard = self.coordinator.lock_target(target_agent_id).await;
                let hub = Arc::clone(&self.hub);
                with_hub_blocking(hub, move |hub| {
                    hub.ticket_bind()
                        .bind(&request)
                        .map_err(|err| map_err_string("bind_ticket", err))
                })
                .await
            }
        }
    }

    async fn unbind(&self, ticket_id: String, agent_id: AgentId) -> Result<(), String> {
        let action = {
            let hub = Arc::clone(&self.hub);
            let ticket_id = ticket_id.clone();
            with_hub_blocking(hub, move |hub| {
                resolve_unbind_action(hub, &ticket_id, agent_id)
                    .map_err(|err| map_err_string("unbind_ticket", err))
            })
            .await?
        };
        let request = TicketUnbindRequest {
            ticket_id: action.request.ticket_id,
            agent_id: action.request.agent_id,
        };
        if let Some(profile_id) = action.stop_bridge_profile_id {
            return unbind_local_bridge(
                Arc::clone(&self.hub),
                Arc::clone(&self.host),
                Arc::clone(&self.coordinator),
                Arc::clone(&self.lifecycle_barrier),
                profile_id,
                request,
            )
            .await;
        }
        let _target_guard = match action.lock_target {
            Some(target) => Some(self.coordinator.lock_target(target).await),
            None => None,
        };
        let hub = Arc::clone(&self.hub);
        with_hub_blocking(hub, move |hub| {
            hub.ticket_bind()
                .unbind(&request)
                .map_err(|err| map_err_string("unbind_ticket", err))
        })
        .await
    }

    async fn start_bridge(&self, profile_id: String) -> Result<AdapterBridgeStatus, String> {
        start_local_bridge(
            Arc::clone(&self.hub),
            Arc::clone(&self.host),
            Arc::clone(&self.coordinator),
            Arc::clone(&self.lifecycle_barrier),
            profile_id,
        )
        .await
    }

    async fn stop_bridge(&self, profile_id: String) -> Result<AdapterBridgeStatus, String> {
        stop_local_bridge(
            Arc::clone(&self.hub),
            Arc::clone(&self.host),
            Arc::clone(&self.coordinator),
            Arc::clone(&self.lifecycle_barrier),
            profile_id,
        )
        .await
    }

    async fn bridge_status(&self, profile_id: String) -> Result<AdapterBridgeStatus, String> {
        local_bridge_status(Arc::clone(&self.hub), Arc::clone(&self.host), profile_id).await
    }

    async fn remove(&self, profile_id: String) -> Result<(), String> {
        remove_adapter_with_bridge_cleanup(
            Arc::clone(&self.hub),
            Arc::clone(&self.host),
            Arc::clone(&self.coordinator),
            Arc::clone(&self.lifecycle_barrier),
            profile_id,
        )
        .await
    }

    async fn set_auto_start(
        &self,
        profile_id: String,
        auto_start: bool,
    ) -> Result<AdapterProfile, String> {
        set_local_bridge_auto_start(Arc::clone(&self.hub), profile_id, auto_start).await
    }
}

/// Build an [`AdapterApplyResult`] from a successful bind (compat for apply_adapter).
pub(crate) fn apply_result_from_binding(
    hub: &AgentHub,
    binding: &TicketBinding,
) -> Result<AdapterApplyResult, String> {
    let profile_id = binding.profile_id.as_deref().ok_or_else(|| {
        "bind did not persist an adapter profile [adapter.profile_missing]".to_string()
    })?;
    let profile = hub
        .adapter_apply()
        .list(None, None, Some(binding.agent_id))
        .map_err(|err| map_err_string("apply_adapter", err))?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("adapter profile not found: {profile_id}"))?;
    let provider_id = profile.generated_provider_id.clone().ok_or_else(|| {
        "bind did not persist a generated provider [adapter.provider_missing]".to_string()
    })?;
    let provider = hub
        .providers()
        .get(&provider_id, Some(binding.agent_id))
        .map_err(|err| map_err_string("apply_adapter", err))?;
    Ok(AdapterApplyResult {
        profile,
        provider: provider.redacted(),
    })
}
