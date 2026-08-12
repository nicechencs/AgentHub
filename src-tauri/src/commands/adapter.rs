//! Read-only Adapter route analysis and plan commands.

use agenthub_core::models::{
    AdapterApplyPlan, AdapterApplyRequest, AdapterApplyResult, AdapterProfile,
    AdapterRouteAnalysis, AdapterRouteRequest, AdapterSourceKind,
};
use tauri::State;

use crate::adapter_bridge_controller::{
    apply_local_bridge, local_bridge_status, remove_adapter_with_bridge_cleanup,
    set_local_bridge_auto_start, start_local_bridge, stop_local_bridge, AdapterBridgeStatusDto,
};
use crate::commands::{map_err_string, parse_agent, with_hub_blocking};
use crate::state::AppState;

/// Preview a supported connection route. This command never applies a config or starts a bridge.
#[tauri::command]
pub async fn analyze_adapter(
    state: State<'_, AppState>,
    source_kind: String,
    source_id: String,
    target_agent_id: String,
) -> Result<AdapterRouteAnalysis, String> {
    let hub = state.hub_arc()?;
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
}

/// Preview the configuration fields an eventual apply would change. This is
/// intentionally read-only: `canApply` is false and secrets remain references.
#[tauri::command]
pub async fn plan_adapter(
    state: State<'_, AppState>,
    source_kind: String,
    source_id: String,
    target_agent_id: String,
) -> Result<AdapterApplyPlan, String> {
    let hub = state.hub_arc()?;
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
}

/// List credential-free, persisted adapter profiles. All filters are optional.
#[tauri::command]
pub async fn list_adapter_profiles(
    state: State<'_, AppState>,
    source_kind: Option<String>,
    source_id: Option<String>,
    target_agent_id: Option<String>,
) -> Result<Vec<AdapterProfile>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        list_adapter_profiles_inner(
            hub,
            source_kind.as_deref(),
            source_id.as_deref(),
            target_agent_id.as_deref(),
        )
    })
    .await
}

/// Apply a supported Adapter route.
///
/// Kimi membership -> Claude remains a direct native-endpoint projection.
/// Kimi membership -> Codex runs a local Responses-to-Chat bridge owned by
/// the GUI process, then projects its loopback endpoint through ProviderService.
#[tauri::command]
pub async fn apply_adapter(
    state: State<'_, AppState>,
    source_kind: String,
    source_id: String,
    target_agent_id: String,
) -> Result<AdapterApplyResult, String> {
    let source_kind_parsed = parse_source_kind(&source_kind)?;
    let target_agent_parsed = parse_agent(&target_agent_id)?;
    if target_agent_parsed == agenthub_core::models::AgentId::Codex {
        return apply_local_bridge(
            state.hub_arc()?,
            state.bridge_host(),
            state.bridge_saga_coordinator(),
            state.lifecycle_shutdown_barrier(),
            agenthub_core::services::AdapterBridgePrepareRequest {
                source_kind: source_kind_parsed,
                source_id,
                target_agent_id: target_agent_parsed,
                auto_start: true,
            },
        )
        .await;
    }
    let hub = state.hub_arc()?;
    let _target_guard = state
        .bridge_saga_coordinator()
        .lock_target(target_agent_parsed)
        .await;
    with_hub_blocking(hub, move |hub| {
        apply_adapter_inner(hub, &source_kind, &source_id, &target_agent_id)
    })
    .await
}

/// Start an already-created local bridge by profile id.
#[tauri::command]
pub async fn start_adapter_bridge(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<AdapterBridgeStatusDto, String> {
    start_local_bridge(
        state.hub_arc()?,
        state.bridge_host(),
        state.bridge_saga_coordinator(),
        state.lifecycle_shutdown_barrier(),
        profile_id,
    )
    .await
}

/// Stop one local bridge. Calling stop for an already-stopped profile is safe.
#[tauri::command]
pub async fn stop_adapter_bridge(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<AdapterBridgeStatusDto, String> {
    stop_local_bridge(
        state.hub_arc()?,
        state.bridge_host(),
        state.bridge_saga_coordinator(),
        state.lifecycle_shutdown_barrier(),
        profile_id,
    )
    .await
}

/// Get a credential-free local listener state for one bridge profile.
#[tauri::command]
pub async fn get_adapter_bridge_status(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<AdapterBridgeStatusDto, String> {
    local_bridge_status(state.hub_arc()?, state.bridge_host(), profile_id).await
}

/// Enable or disable background restore for an existing local bridge.
#[tauri::command]
pub async fn set_adapter_bridge_auto_start(
    state: State<'_, AppState>,
    profile_id: String,
    auto_start: bool,
) -> Result<AdapterProfile, String> {
    set_local_bridge_auto_start(state.hub_arc()?, profile_id, auto_start).await
}

/// Remove an adapter profile and its generated provider when it is not current.
#[tauri::command]
pub async fn remove_adapter(state: State<'_, AppState>, profile_id: String) -> Result<(), String> {
    remove_adapter_with_bridge_cleanup(
        state.hub_arc()?,
        state.bridge_host(),
        state.bridge_saga_coordinator(),
        state.lifecycle_shutdown_barrier(),
        profile_id,
    )
    .await
}

// ---------------------------------------------------------------------------
// Testable inner implementations (take &AgentHub, no Tauri State)
// ---------------------------------------------------------------------------

fn list_adapter_profiles_inner(
    hub: &agenthub_core::AgentHub,
    source_kind: Option<&str>,
    source_id: Option<&str>,
    target_agent_id: Option<&str>,
) -> Result<Vec<AdapterProfile>, String> {
    let source_kind = parse_source_kind_opt(source_kind)?;
    let target_agent_id = target_agent_id.map(parse_agent).transpose()?;
    hub.adapter_apply
        .list(source_kind, source_id, target_agent_id)
        .map_err(|err| map_err_string("list_adapter_profiles", err))
}

fn apply_adapter_inner(
    hub: &agenthub_core::AgentHub,
    source_kind: &str,
    source_id: &str,
    target_agent_id: &str,
) -> Result<AdapterApplyResult, String> {
    let source_kind = parse_source_kind(source_kind)?;
    let target_agent_id = parse_agent(target_agent_id)?;
    hub.adapter_apply
        .apply(&AdapterApplyRequest {
            source_kind,
            source_id: source_id.into(),
            target_agent_id,
        })
        .map_err(|err| map_err_string("apply_adapter", err))
}

fn parse_source_kind(source_kind: &str) -> Result<AdapterSourceKind, String> {
    AdapterSourceKind::parse(source_kind)
        .ok_or_else(|| "invalid adapter source kind, expected: account|provider".to_string())
}

fn parse_source_kind_opt(source_kind: Option<&str>) -> Result<Option<AdapterSourceKind>, String> {
    source_kind.map(parse_source_kind).transpose()
}

#[cfg(test)]
mod tests;
