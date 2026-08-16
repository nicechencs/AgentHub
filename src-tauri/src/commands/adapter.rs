//! Read-only Adapter route analysis and plan commands + thin control delegates.
//!
//! Mutation / local_bridge lifecycle goes through
//! [`agenthub_core::adapter_control::AdapterControl`] (desktop host impl).

use agenthub_core::adapter_control::AdapterControl;
use agenthub_core::models::{
    ticket_id, AdapterApplyPlan, AdapterApplyResult, AdapterProfile, AdapterProfileFilter,
    AdapterProfileMode, AdapterRoute, AdapterRouteAnalysis, AdapterRouteRequest, AdapterSourceKind,
    TicketBinding, TicketBindingRoute, TicketPlanRequest, TicketWallet,
};
use tauri::State;

use crate::adapter_bridge_controller::AdapterBridgeStatusDto;
use crate::adapter_control_host::apply_result_from_binding;
use crate::commands::{
    adapter_error_from_string, map_err_string, parse_agent, with_hub_blocking, GuiError,
};
use crate::state::AppState;

/// Preview a supported connection route. This command never applies a config or starts a bridge.
#[tauri::command]
pub async fn analyze_adapter(
    state: State<'_, AppState>,
    source_kind: String,
    source_id: String,
    target_agent_id: String,
) -> Result<AdapterRouteAnalysis, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        let source_kind = parse_source_kind(&source_kind)?;
        let target_agent_id = parse_agent(&target_agent_id)?;
        hub.adapter_routes
            .analyze(&AdapterRouteRequest {
                source_kind,
                source_id,
                target_agent_id,
            })
            .map_err(|err| map_err_string("analyze_adapter", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Preview the configuration fields an eventual apply would change. This is
/// intentionally read-only: `canApply` is false and secrets remain references.
#[tauri::command]
pub async fn plan_adapter(
    state: State<'_, AppState>,
    source_kind: String,
    source_id: String,
    target_agent_id: String,
) -> Result<AdapterApplyPlan, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        let source_kind = parse_source_kind(&source_kind)?;
        let target_agent_id = parse_agent(&target_agent_id)?;
        hub.adapter_routes
            .plan(&AdapterRouteRequest {
                source_kind,
                source_id,
                target_agent_id,
            })
            .map_err(|err| map_err_string("plan_adapter", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// List the read-only Ticket / Binding wallet (generated projections excluded).
#[tauri::command]
pub async fn list_ticket_wallet(state: State<'_, AppState>) -> Result<TicketWallet, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    let host = state.bridge_host();
    with_hub_blocking(hub, move |hub| {
        let mut wallet = hub
            .tickets
            .list_wallet()
            .map_err(|err| map_err_string("list_ticket_wallet", err))?;
        enrich_bridge_running(&host, &mut wallet);
        Ok(wallet)
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Plan an adapter route from a ticket id (`account:<id>` / `provider:<id>`).
///
/// Wire shape matches [`plan_adapter`] exactly.
#[tauri::command]
pub async fn plan_ticket(
    state: State<'_, AppState>,
    ticket_id: String,
    target_agent_id: String,
) -> Result<AdapterApplyPlan, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        let target_agent_id = parse_agent(&target_agent_id)?;
        hub.tickets
            .plan(&TicketPlanRequest {
                ticket_id,
                target_agent_id,
            })
            .map_err(|err| map_err_string("plan_ticket", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// List credential-free, persisted adapter profiles. All filters are optional.
#[tauri::command]
pub async fn list_adapter_profiles(
    state: State<'_, AppState>,
    source_kind: Option<String>,
    source_id: Option<String>,
    target_agent_id: Option<String>,
    mode: Option<String>,
    route: Option<String>,
) -> Result<Vec<AdapterProfile>, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        list_adapter_profiles_inner(
            hub,
            source_kind.as_deref(),
            source_id.as_deref(),
            target_agent_id.as_deref(),
            mode.as_deref(),
            route.as_deref(),
        )
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Bind a ticket to an Agent. Local-bridge targets use the desktop host saga.
#[tauri::command]
pub async fn bind_ticket(
    state: State<'_, AppState>,
    ticket_id: String,
    target_agent_id: String,
) -> Result<TicketBinding, GuiError> {
    let control = state.adapter_control().map_err(adapter_error_from_string)?;
    let target_agent_id = parse_agent(&target_agent_id).map_err(adapter_error_from_string)?;
    control
        .bind(ticket_id, target_agent_id)
        .await
        .map_err(adapter_error_from_string)
}

/// Unbind a ticket from an Agent. Stops a bridge first, then restores previous
/// live and deletes the generated projection. The source ticket remains.
#[tauri::command]
pub async fn unbind_ticket(
    state: State<'_, AppState>,
    ticket_id: String,
    agent_id: String,
) -> Result<(), GuiError> {
    let control = state.adapter_control().map_err(adapter_error_from_string)?;
    let agent_id = parse_agent(&agent_id).map_err(adapter_error_from_string)?;
    control
        .unbind(ticket_id, agent_id)
        .await
        .map_err(adapter_error_from_string)
}

/// Thin compatibility delegate to [`bind_ticket`]. Prefer bind as the write API.
///
/// Kimi membership -> Codex still runs the desktop host saga.
#[tauri::command]
pub async fn apply_adapter(
    state: State<'_, AppState>,
    source_kind: String,
    source_id: String,
    target_agent_id: String,
) -> Result<AdapterApplyResult, GuiError> {
    let source_kind_parsed = parse_source_kind(&source_kind).map_err(adapter_error_from_string)?;
    let ticket = ticket_id(source_kind_parsed, &source_id);
    let control = state.adapter_control().map_err(adapter_error_from_string)?;
    let target_agent_id = parse_agent(&target_agent_id).map_err(adapter_error_from_string)?;
    let binding = control
        .bind(ticket, target_agent_id)
        .await
        .map_err(adapter_error_from_string)?;
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| apply_result_from_binding(hub, &binding))
        .await
        .map_err(adapter_error_from_string)
}

/// Start an already-created local bridge by profile id.
#[tauri::command]
pub async fn start_adapter_bridge(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<AdapterBridgeStatusDto, GuiError> {
    state
        .adapter_control()
        .map_err(adapter_error_from_string)?
        .start_bridge(profile_id)
        .await
        .map_err(adapter_error_from_string)
}

/// Stop one local bridge. Calling stop for an already-stopped profile is safe.
#[tauri::command]
pub async fn stop_adapter_bridge(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<AdapterBridgeStatusDto, GuiError> {
    state
        .adapter_control()
        .map_err(adapter_error_from_string)?
        .stop_bridge(profile_id)
        .await
        .map_err(adapter_error_from_string)
}

/// Get a credential-free local listener state for one bridge profile.
#[tauri::command]
pub async fn get_adapter_bridge_status(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<AdapterBridgeStatusDto, GuiError> {
    state
        .adapter_control()
        .map_err(adapter_error_from_string)?
        .bridge_status(profile_id)
        .await
        .map_err(adapter_error_from_string)
}

/// Enable or disable background restore for an existing local bridge.
#[tauri::command]
pub async fn set_adapter_bridge_auto_start(
    state: State<'_, AppState>,
    profile_id: String,
    auto_start: bool,
) -> Result<AdapterProfile, GuiError> {
    state
        .adapter_control()
        .map_err(adapter_error_from_string)?
        .set_auto_start(profile_id, auto_start)
        .await
        .map_err(adapter_error_from_string)
}

/// Remove an adapter profile and its generated provider when it is not current.
#[tauri::command]
pub async fn remove_adapter(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), GuiError> {
    state
        .adapter_control()
        .map_err(adapter_error_from_string)?
        .remove(profile_id)
        .await
        .map_err(adapter_error_from_string)
}

// ---------------------------------------------------------------------------
// Testable helpers (no Tauri State)
// ---------------------------------------------------------------------------

fn list_adapter_profiles_inner(
    hub: &agenthub_core::AgentHub,
    source_kind: Option<&str>,
    source_id: Option<&str>,
    target_agent_id: Option<&str>,
    mode: Option<&str>,
    route: Option<&str>,
) -> Result<Vec<AdapterProfile>, String> {
    let source_kind = parse_source_kind_opt(source_kind)?;
    let target_agent_id = target_agent_id.map(parse_agent).transpose()?;
    let mode = parse_mode_opt(mode)?;
    let route = parse_route_opt(route)?;
    hub.adapter_apply
        .list_filtered(&AdapterProfileFilter {
            source_kind,
            source_id: source_id.map(str::to_owned),
            target_agent_id,
            mode,
            route,
            ..AdapterProfileFilter::default()
        })
        .map_err(|err| map_err_string("list_adapter_profiles", err))
}

fn parse_source_kind(source_kind: &str) -> Result<AdapterSourceKind, String> {
    AdapterSourceKind::parse(source_kind)
        .ok_or_else(|| "invalid adapter source kind, expected: account|provider".to_string())
}

fn parse_source_kind_opt(source_kind: Option<&str>) -> Result<Option<AdapterSourceKind>, String> {
    source_kind.map(parse_source_kind).transpose()
}

fn parse_mode(mode: &str) -> Result<AdapterProfileMode, String> {
    AdapterProfileMode::parse(mode)
        .ok_or_else(|| "invalid adapter mode, expected: api|oauth".to_string())
}

fn parse_mode_opt(mode: Option<&str>) -> Result<Option<AdapterProfileMode>, String> {
    mode.map(parse_mode).transpose()
}

fn parse_route(route: &str) -> Result<AdapterRoute, String> {
    AdapterRoute::parse(route)
        .filter(|value| value.is_profile_supported())
        .ok_or_else(|| {
            "invalid adapter route, expected: config_sync|native_endpoint|local_bridge".to_string()
        })
}

fn parse_route_opt(route: Option<&str>) -> Result<Option<AdapterRoute>, String> {
    route.map(parse_route).transpose()
}

/// Best-effort fill of `bridge.running` from the process-local listener host.
/// Core leaves `running=false`; this stays in the GUI command layer to avoid
/// a core → host dependency.
fn enrich_bridge_running(
    host: &agenthub_core::bridge::BridgeRuntimeHost,
    wallet: &mut TicketWallet,
) {
    for binding in &mut wallet.bindings {
        if binding.route != TicketBindingRoute::Bridge {
            continue;
        }
        let Some(profile_id) = binding.profile_id.as_deref() else {
            continue;
        };
        let Ok(Some(status)) = host.status(profile_id) else {
            continue;
        };
        if let Some(bridge) = binding.bridge.as_mut() {
            bridge.running = status.running;
            if status.running {
                bridge.port = Some(status.port);
            }
        }
    }
}

#[cfg(test)]
mod tests;
