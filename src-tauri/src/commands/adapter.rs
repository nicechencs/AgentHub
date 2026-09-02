//! Read-only Adapter route analysis and plan commands + thin control delegates.
//!
//! Mutation / local_bridge lifecycle goes through
//! [`agenthub_core::adapter_control::AdapterControl`] (desktop host impl).

use agenthub_core::adapter_control::{
    resolve_bind_action, AdapterControl, BindAction, LocalEntryStatus,
};
use agenthub_core::bridge::BridgeRuntimeHost;
use agenthub_core::models::{
    ticket_id, AdapterApplyPlan, AdapterApplyResult, AdapterProfile, AdapterProfileFilter,
    AdapterProfileMode, AdapterRoute, AdapterRouteAnalysis, AdapterRouteRequest, AdapterSourceKind,
    AgentId, DefaultRoutePoolList, DefaultRoutePoolOverview, LocalTokenRecord,
    RouteDownstreamSurface, SyncConnectionAuthorizationsResult, TicketBinding, TicketBindingRoute,
    TicketPlanRequest, TicketWallet,
};
use agenthub_core::utils::upstream_model_catalog::SourceModelCatalog;
use agenthub_core::AgentHub;
use tauri::{AppHandle, State};

use crate::adapter_bridge_controller::{
    local_entry_status as read_local_entry_status, start_local_entry as start_shared_local_entry,
    stop_local_entry as stop_shared_local_entry, AdapterBridgeStatusDto,
};
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
        hub.adapter_routes()
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
        hub.adapter_routes()
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
            .tickets()
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
        hub.tickets()
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
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    let preflight_ticket = ticket.clone();
    let preflight_target = parse_agent(&target_agent_id).map_err(adapter_error_from_string)?;
    let action = with_hub_blocking(hub.clone(), move |hub| {
        resolve_bind_action(hub, &preflight_ticket, preflight_target)
            .map_err(|error| map_err_string("apply_adapter", error))
    })
    .await
    .map_err(adapter_error_from_string)?;
    if matches!(action, BindAction::NativeSelf(_)) {
        return Err(adapter_error_from_string(
            "这类登录由 Agent 自己管理，不能生成本机路由配置 [adapter.native_self]".into(),
        ));
    }
    let control = state.adapter_control().map_err(adapter_error_from_string)?;
    let target_agent_id = preflight_target;
    let binding = control
        .bind(ticket, target_agent_id)
        .await
        .map_err(adapter_error_from_string)?;
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

/// Start the shared local relay (loopback listener). Does not bind logins to Agents.
#[tauri::command]
pub async fn start_local_entry(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<LocalEntryStatus, GuiError> {
    start_shared_local_entry(
        state.hub_arc().map_err(adapter_error_from_string)?,
        state.bridge_host(),
        state.bridge_saga_coordinator(),
        state.lifecycle_shutdown_barrier(),
        state.local_entry_restarting(),
        app,
        true,
    )
    .await
    .map_err(adapter_error_from_string)
}

/// Stop the shared local relay.
#[tauri::command]
pub async fn stop_local_entry(state: State<'_, AppState>) -> Result<LocalEntryStatus, GuiError> {
    stop_shared_local_entry(
        state.hub_arc().map_err(adapter_error_from_string)?,
        state.bridge_host(),
        state.bridge_saga_coordinator(),
        state.lifecycle_shutdown_barrier(),
        state.local_entry_restarting(),
    )
    .await
    .map_err(adapter_error_from_string)
}

/// Credential-free relay status for the board switch.
#[tauri::command]
pub async fn get_local_entry_status(
    state: State<'_, AppState>,
) -> Result<LocalEntryStatus, GuiError> {
    read_local_entry_status(&state.bridge_host(), &state.local_entry_restarting())
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

/// Kimi and DSH share one chat-completions token, or keep separate keys.
#[tauri::command]
pub async fn set_chat_completions_shared(
    state: State<'_, AppState>,
    shared: bool,
) -> Result<DefaultRoutePoolList, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        hub.route_pools()
            .set_chat_completions_shared(shared)
            .map_err(|err| map_err_string("set_chat_completions_shared", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Default RoutePool overview for the Routes page. Hub token is never serialized.
#[tauri::command]
pub async fn list_default_route_pools(
    state: State<'_, AppState>,
) -> Result<DefaultRoutePoolList, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        hub.route_pools()
            .list_default_overviews()
            .map_err(|err| map_err_string("list_default_route_pools", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Loopback bearers for the tokens page.
#[tauri::command]
pub async fn list_local_tokens(
    state: State<'_, AppState>,
) -> Result<Vec<LocalTokenRecord>, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        hub.route_pools()
            .list_local_tokens()
            .map_err(|err| map_err_string("list_local_tokens", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Ensure the local entry is up, then `POST` a tiny request on the row path.
#[tauri::command]
pub async fn test_local_token(
    app: AppHandle,
    state: State<'_, AppState>,
    endpoint: String,
    token: String,
    path: String,
    model: Option<String>,
) -> Result<agenthub_core::utils::local_token_probe::LocalTokenProbeResult, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    let host = state.bridge_host();
    let status = match read_local_entry_status(&host, &state.local_entry_restarting()) {
        Ok(status) if status.running => status,
        _ => start_shared_local_entry(
            hub.clone(),
            host.clone(),
            state.bridge_saga_coordinator(),
            state.lifecycle_shutdown_barrier(),
            state.local_entry_restarting(),
            app,
            false,
        )
        .await
        .map_err(adapter_error_from_string)?,
    };
    let live_endpoint = status
        .port
        .map(|port| format!("127.0.0.1:{port}"))
        .unwrap_or(endpoint);
    let chosen = model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let lookup_token = token.clone();
    let model = if chosen.is_some() {
        chosen
    } else {
        with_hub_blocking(hub, move |hub| {
            Ok(lookup_local_token_test_model(hub, &lookup_token))
        })
        .await
        .unwrap_or(None)
    };
    tauri::async_runtime::spawn_blocking(move || {
        agenthub_core::utils::local_token_probe::probe_local_token(
            &live_endpoint,
            &token,
            &path,
            model.as_deref(),
        )
    })
    .await
    .map_err(|err| adapter_error_from_string(format!("command join error: {err}")))
}

fn lookup_local_token_test_model(hub: &AgentHub, token: &str) -> Option<String> {
    list_models_for_local_token(hub, token)
        .into_iter()
        .find(|model| !model.trim().is_empty())
}

fn list_models_for_local_token(hub: &AgentHub, token: &str) -> Vec<String> {
    let Ok(records) = hub.route_pools().list_local_tokens() else {
        return Vec::new();
    };
    let Some(pool_id) = records
        .into_iter()
        .find(|record| record.token == token)
        .map(|record| record.pool_id)
    else {
        return Vec::new();
    };
    hub.route_pools()
        .list_upstream_models_for_pool(&pool_id)
        .unwrap_or_default()
}

/// Cached model list for one connection-pool login. Fetches once until URL/key/login changes.
#[tauri::command]
pub async fn ensure_source_model_catalog(
    state: State<'_, AppState>,
    source_kind: String,
    source_id: String,
) -> Result<SourceModelCatalog, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        let kind = parse_source_kind(&source_kind)?;
        hub.route_pools()
            .ensure_source_model_catalog(kind, &source_id)
            .map_err(|err| map_err_string("ensure_source_model_catalog", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Replace the cached list with a user-supplied model list for routing.
#[tauri::command]
pub async fn set_source_custom_models(
    state: State<'_, AppState>,
    source_kind: String,
    source_id: String,
    models: Vec<String>,
) -> Result<SourceModelCatalog, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        let kind = parse_source_kind(&source_kind)?;
        hub.route_pools()
            .set_source_custom_models(kind, &source_id, models)
            .map_err(|err| map_err_string("set_source_custom_models", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Save a custom model list for the token's pool logins.
#[tauri::command]
pub async fn set_local_token_custom_models(
    state: State<'_, AppState>,
    token: String,
    models: Vec<String>,
) -> Result<Vec<String>, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        hub.route_pools()
            .set_local_token_custom_models(&token, models)
            .map_err(|err| map_err_string("set_local_token_custom_models", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Live model ids for the tokens-page test dropdown.
#[tauri::command]
pub async fn list_local_token_models(
    state: State<'_, AppState>,
    token: String,
) -> Result<Vec<String>, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| Ok(list_models_for_local_token(hub, &token)))
        .await
        .map_err(adapter_error_from_string)
}

/// Re-read models supported by the token's pool logins.
#[tauri::command]
pub async fn refresh_local_token_models(
    state: State<'_, AppState>,
    token: String,
) -> Result<Vec<String>, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        hub.route_pools()
            .refresh_local_token_models(&token)
            .map_err(|err| map_err_string("refresh_local_token_models", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Replace one default-pool loopback bearer. Restarts that edge if it is live.
#[tauri::command]
pub async fn set_local_token(
    state: State<'_, AppState>,
    pool_id: String,
    token: String,
) -> Result<LocalTokenRecord, GuiError> {
    crate::adapter_bridge_controller::set_local_entry_token(
        state.hub_arc().map_err(adapter_error_from_string)?,
        state.bridge_host(),
        state.bridge_saga_coordinator(),
        state.lifecycle_shutdown_barrier(),
        pool_id,
        token,
    )
    .await
    .map_err(adapter_error_from_string)
}

/// Enroll a newly added authorization into the default auth pool and mark it
/// pool-owned so it does not appear on the Connections list.
#[tauri::command]
pub async fn attach_pool_owned_authorization(
    state: State<'_, AppState>,
    source_kind: String,
    source_id: String,
    target_agent_id: String,
    surface: String,
) -> Result<DefaultRoutePoolOverview, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        let source_kind = parse_source_kind(&source_kind)?;
        let target_agent_id = parse_agent(&target_agent_id)?;
        let surface = RouteDownstreamSurface::parse(&surface).ok_or_else(|| {
            "invalid route pool surface, expected: messages|responses|chat_completions".to_string()
        })?;
        hub.route_pools()
            .attach_pool_owned_authorization(
                target_agent_id,
                surface,
                source_kind,
                &source_id,
            )
            .map_err(|err| map_err_string("attach_pool_owned_authorization", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Enable or disable every default-pool membership of one login.
#[tauri::command]
pub async fn set_route_authorization_enabled(
    state: State<'_, AppState>,
    source_kind: String,
    source_id: String,
    enabled: bool,
) -> Result<u32, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        let source_kind = parse_source_kind(&source_kind)?;
        hub.route_pools()
            .set_authorization_enabled(source_kind, &source_id, enabled)
            .map_err(|err| map_err_string("set_route_authorization_enabled", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Set priority on every default-pool membership of one login.
#[tauri::command]
pub async fn set_route_authorization_priority(
    state: State<'_, AppState>,
    source_kind: String,
    source_id: String,
    priority: i64,
) -> Result<u32, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        let source_kind = parse_source_kind(&source_kind)?;
        hub.route_pools()
            .set_authorization_priority(source_kind, &source_id, priority)
            .map_err(|err| map_err_string("set_route_authorization_priority", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Remove every default-pool membership of one login.
///
/// This only removes the route-pool authorization reference. The underlying
/// account/provider deletion commands keep their existing behavior.
#[tauri::command]
pub async fn remove_route_authorization(
    state: State<'_, AppState>,
    source_kind: String,
    source_id: String,
) -> Result<u32, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        let source_kind = parse_source_kind(&source_kind)?;
        hub.route_pools()
            .remove_route_authorization(source_kind, &source_id)
            .map_err(|err| map_err_string("remove_route_authorization", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Move a Connections-managed pool member into the pool recycle bin.
/// Leaves the Connections login in place.
#[tauri::command]
pub async fn recycle_route_membership(
    state: State<'_, AppState>,
    source_kind: String,
    source_id: String,
) -> Result<u32, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        let source_kind = parse_source_kind(&source_kind)?;
        hub.route_pools()
            .recycle_route_membership(source_kind, &source_id)
            .map_err(|err| map_err_string("recycle_route_membership", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Enroll existing Connections authorizations into default auth pools.
/// Does not remove them from Connections.
#[tauri::command]
pub async fn sync_connection_authorizations(
    state: State<'_, AppState>,
    request: Option<agenthub_core::models::SyncConnectionAuthorizationsRequest>,
) -> Result<SyncConnectionAuthorizationsResult, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    with_hub_blocking(hub, move |hub| {
        let result = match request.as_ref() {
            Some(request) => hub
                .route_pools()
                .sync_connection_authorizations_selected(Some(&request.sources)),
            None => hub.route_pools().sync_connection_authorizations(),
        };
        result
            .map_err(|err| map_err_string("sync_connection_authorizations", err))
    })
    .await
    .map_err(adapter_error_from_string)
}

/// Convert a native_endpoint / config_sync login into the target Agent default
/// local-bridge pool. Binds first; occupancy / bind failure does not enroll.
#[tauri::command]
pub async fn enroll_native_to_gateway(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<DefaultRoutePoolOverview, GuiError> {
    let hub = state.hub_arc().map_err(adapter_error_from_string)?;
    let control = state.adapter_control().map_err(adapter_error_from_string)?;
    let host = state.bridge_host();
    let (ticket, target) = {
        let profile_id = profile_id.clone();
        with_hub_blocking(hub.clone(), move |hub| {
            prepare_enroll_native(hub, &profile_id)
        })
        .await
        .map_err(adapter_error_from_string)?
    };
    let binding = match control.bind(ticket, target).await {
        Ok(binding) => binding,
        Err(error) => return Err(adapter_error_from_string(error)),
    };
    with_hub_blocking(hub, move |hub| {
        persist_enroll_native_if_bound(hub, host.as_ref(), Ok(binding))
    })
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
    hub.adapter_apply()
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

fn prepare_enroll_native(hub: &AgentHub, profile_id: &str) -> Result<(String, AgentId), String> {
    let profile = hub
        .route_pools()
        .get_adapter_profile(profile_id)
        .map_err(|err| map_err_string("enroll_native_to_gateway", err))?
        .ok_or_else(|| format!("adapter profile not found: {profile_id}"))?;
    hub.route_pools()
        .evaluate_enroll_native(&profile)
        .map_err(|err| map_err_string("enroll_native_to_gateway", err))?;
    Ok((
        ticket_id(profile.source_kind, &profile.source_id),
        profile.target_agent_id,
    ))
}

/// Enroll only after a successful bind. Occupancy / bind errors must not call this
/// with `Ok`; passing `Err` leaves the pool unenrolled.
pub(crate) fn persist_enroll_native_if_bound(
    hub: &AgentHub,
    host: &BridgeRuntimeHost,
    bind: Result<TicketBinding, String>,
) -> Result<DefaultRoutePoolOverview, String> {
    let binding = bind?;
    let bound_id = binding.profile_id.clone().ok_or_else(|| {
        "bind did not persist an adapter profile [adapter.profile_missing]".to_string()
    })?;
    let port = match host.status(&bound_id) {
        Ok(Some(status)) if status.port > 0 => status.port,
        _ => binding
            .bridge
            .as_ref()
            .and_then(|bridge| bridge.port)
            .unwrap_or(0),
    };
    if port == 0 {
        return Err("local route has no port after bind [adapter.bridge_start]".into());
    }
    let profile = hub
        .route_pools()
        .get_adapter_profile(&bound_id)
        .map_err(|err| map_err_string("enroll_native_to_gateway", err))?
        .ok_or_else(|| format!("adapter profile not found: {bound_id}"))?;
    hub.route_pools()
        .persist_enroll_after_native_bind(&profile, port)
        .map_err(|err| map_err_string("enroll_native_to_gateway", err))
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
