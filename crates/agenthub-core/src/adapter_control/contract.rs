//! Application contract for adapter bind / bridge lifecycle.
//!
//! Reshape bind/unbind planning lives here as pure helpers. Hosts execute
//! [`BindAction::LocalBridge`] via their listener saga; reshape calls
//! [`crate::services::TicketBindService`].

use crate::error::{AppError, Result};
use crate::models::{
    parse_ticket_id, AdapterProfile, AdapterRoute, AgentId, TicketBinding, TicketPlanRequest,
    TicketUnbindRequest,
};
use crate::services::AdapterBridgePrepareRequest;
use crate::AgentHub;

use super::status::AdapterBridgeStatus;

type ControlResult<T> = std::result::Result<T, String>;

/// Tauri-neutral control surface for ticket bind and local_bridge lifecycle.
///
/// Desktop implements this by composing [`crate::services::TicketBindService`]
/// with the in-process bridge saga. A future sidecar client would implement the
/// same methods over IPC without changing shell commands.
pub trait AdapterControl: Send + Sync {
    fn bind(
        &self,
        ticket_id: String,
        target_agent_id: AgentId,
    ) -> impl std::future::Future<Output = ControlResult<TicketBinding>> + Send;

    fn unbind(
        &self,
        ticket_id: String,
        agent_id: AgentId,
    ) -> impl std::future::Future<Output = ControlResult<()>> + Send;

    fn start_bridge(
        &self,
        profile_id: String,
    ) -> impl std::future::Future<Output = ControlResult<AdapterBridgeStatus>> + Send;

    fn stop_bridge(
        &self,
        profile_id: String,
    ) -> impl std::future::Future<Output = ControlResult<AdapterBridgeStatus>> + Send;

    fn bridge_status(
        &self,
        profile_id: String,
    ) -> impl std::future::Future<Output = ControlResult<AdapterBridgeStatus>> + Send;

    fn remove(
        &self,
        profile_id: String,
    ) -> impl std::future::Future<Output = ControlResult<()>> + Send;

    fn set_auto_start(
        &self,
        profile_id: String,
        auto_start: bool,
    ) -> impl std::future::Future<Output = ControlResult<AdapterProfile>> + Send;
}

/// Host-facing decision for one product bind.
#[derive(Debug, Clone)]
pub enum BindAction {
    /// Config-sync / native-endpoint reshape; host locks target then calls
    /// [`crate::services::TicketBindService::bind`].
    Reshape(TicketPlanRequest),
    /// Local bridge; host runs the listener + projection saga.
    LocalBridge(AdapterBridgePrepareRequest),
    /// Native account switch; no adapter profile is persisted for this path.
    ///
    /// This explicit variant lets compatibility callers preflight the action
    /// before attempting to construct an [`AdapterApplyResult`], which cannot
    /// represent a native self-bind without a generated profile.
    NativeSelf(TicketPlanRequest),
}

/// Host-facing decision for one product unbind.
#[derive(Debug, Clone)]
pub struct UnbindAction {
    pub request: TicketUnbindRequest,
    /// When set, host must stop this bridge profile before core unbind.
    pub stop_bridge_profile_id: Option<String>,
    /// Target agent whose live gate must be held around core unbind when a
    /// profile exists. Absent when there is no profile (core unbind fails).
    pub lock_target: Option<AgentId>,
}

/// Plan bind: validate ticket + route, then dispatch native self, reshape, or
/// local_bridge.
pub fn resolve_bind_action(
    hub: &AgentHub,
    ticket_id: &str,
    target_agent_id: AgentId,
) -> Result<BindAction> {
    let (source_kind, source_id) = hub.tickets.parse_bindable_ticket(ticket_id)?;
    let plan = hub.tickets.plan(&TicketPlanRequest {
        ticket_id: ticket_id.to_owned(),
        target_agent_id,
    })?;
    if !plan.can_apply {
        return Err(AppError::Unsupported(plan.reason));
    }
    if source_kind == crate::models::AdapterSourceKind::Account
        && target_agent_id == AgentId::Codex
        && plan.analysis.rule_id.as_deref()
            == Some(crate::models::CODEX_SUBSCRIPTION_TO_CODEX_RULE_ID)
    {
        return Ok(BindAction::NativeSelf(TicketPlanRequest {
            ticket_id: ticket_id.to_owned(),
            target_agent_id,
        }));
    }
    if plan.analysis.route == AdapterRoute::LocalBridge {
        return Ok(BindAction::LocalBridge(AdapterBridgePrepareRequest {
            source_kind,
            source_id,
            target_agent_id,
            auto_start: true,
        }));
    }
    Ok(BindAction::Reshape(TicketPlanRequest {
        ticket_id: ticket_id.to_owned(),
        target_agent_id,
    }))
}

/// Plan unbind: locate profile, require bridge stop when route is local_bridge.
pub fn resolve_unbind_action(
    hub: &AgentHub,
    ticket_id: &str,
    agent_id: AgentId,
) -> Result<UnbindAction> {
    let (source_kind, source_id) = parse_ticket_id(ticket_id).map_err(AppError::InvalidArg)?;
    let profiles = hub
        .adapter_apply
        .list(Some(source_kind), Some(&source_id), Some(agent_id))?;
    let profile = match profiles.as_slice() {
        [] => None,
        [profile] => Some(profile.clone()),
        _ => {
            return Err(AppError::message(
                "adapter.profile_conflict",
                format!(
                    "multiple adapter profiles found for {}:{} → {}; remove the duplicate profiles before unbinding",
                    source_kind.as_str(),
                    source_id,
                    agent_id.as_str()
                ),
            ));
        }
    };
    let stop_bridge_profile_id = profile
        .as_ref()
        .filter(|profile| profile.route == AdapterRoute::LocalBridge)
        .map(|profile| profile.id.clone());
    let lock_target = profile.as_ref().map(|profile| profile.target_agent_id);
    Ok(UnbindAction {
        request: TicketUnbindRequest {
            ticket_id: ticket_id.to_owned(),
            agent_id,
        },
        stop_bridge_profile_id,
        lock_target,
    })
}
