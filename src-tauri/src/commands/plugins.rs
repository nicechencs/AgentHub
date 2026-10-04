//! Vendor plugin / extension pack inventory and supported package operations.

use agenthub_core::models::AgentId;
use agenthub_core::services::{
    disable_plugin as disable_plugin_impl, enable_plugin as enable_plugin_impl,
    install_plugin as install_plugin_impl, list_available_plugins as list_available_plugins_impl,
    list_plugin_inventory as list_plugin_inventory_impl,
    preview_plugin_install as preview_plugin_install_impl,
    refresh_plugin_marketplace as refresh_plugin_marketplace_impl,
    uninstall_plugin as uninstall_plugin_impl, update_pi_plugins as update_pi_plugins_impl,
    update_plugin as update_plugin_impl, PluginEntry, PluginInstallOptions, PluginInventory,
    PluginUninstallOptions, PluginUpdateOptions,
};
use tauri::State;

use super::parse_agent;
use crate::state::AppState;

/// Shared desktop boundary for inventory reads. Keep reads available even when
/// the AgentHub database failed to open; only writes need its live-write lock.
pub(crate) async fn list_plugin_inventory_inner(
    _state: &AppState,
) -> Result<PluginInventory, String> {
    tauri::async_runtime::spawn_blocking(list_plugin_inventory_impl)
        .await
        .map_err(|e| format!("list_plugin_inventory join error: {e}"))
}

/// Invoke: `list_plugin_inventory` — Claude/Codex/Grok/Pi plugin packs (not MCP).
#[tauri::command]
pub async fn list_plugin_inventory(state: State<'_, AppState>) -> Result<PluginInventory, String> {
    list_plugin_inventory_inner(&state).await
}

pub(crate) async fn enable_plugin_inner(
    state: &AppState,
    agent: String,
    name: String,
    marketplace: Option<String>,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = acquire_plugin_write(&hub, agent)?;
        enable_plugin_impl(agent, &name, marketplace.as_deref())
    })
    .await
    .map_err(|e| format!("enable_plugin join error: {e}"))?
}

/// Invoke: `enable_plugin` — supported CLI/config enablement for the selected Agent.
#[tauri::command]
pub async fn enable_plugin(
    state: State<'_, AppState>,
    agent: String,
    name: String,
    marketplace: Option<String>,
) -> Result<(), String> {
    enable_plugin_inner(&state, agent, name, marketplace).await
}

pub(crate) async fn disable_plugin_inner(
    state: &AppState,
    agent: String,
    name: String,
    marketplace: Option<String>,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = acquire_plugin_write(&hub, agent)?;
        disable_plugin_impl(agent, &name, marketplace.as_deref())
    })
    .await
    .map_err(|e| format!("disable_plugin join error: {e}"))?
}

/// Invoke: `disable_plugin` — supported CLI/config disablement for the selected Agent.
#[tauri::command]
pub async fn disable_plugin(
    state: State<'_, AppState>,
    agent: String,
    name: String,
    marketplace: Option<String>,
) -> Result<(), String> {
    disable_plugin_inner(&state, agent, name, marketplace).await
}

pub(crate) async fn list_available_plugins_inner(
    _state: &AppState,
    agent: String,
) -> Result<Vec<PluginEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        list_available_plugins_impl(agent)
    })
    .await
    .map_err(|e| format!("list_available_plugins join error: {e}"))?
}

/// Invoke: `list_available_plugins` — marketplace rows, not installed inventory.
#[tauri::command]
pub async fn list_available_plugins(
    state: State<'_, AppState>,
    agent: String,
) -> Result<Vec<PluginEntry>, String> {
    list_available_plugins_inner(&state, agent).await
}

pub(crate) async fn preview_plugin_install_inner(
    _state: &AppState,
    agent: String,
    source: String,
) -> Result<PluginEntry, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        preview_plugin_install_impl(agent, &source)
    })
    .await
    .map_err(|e| format!("preview_plugin_install join error: {e}"))?
}

/// Invoke: `preview_plugin_install` — component list before confirm.
#[tauri::command]
pub async fn preview_plugin_install(
    state: State<'_, AppState>,
    agent: String,
    source: String,
) -> Result<PluginEntry, String> {
    preview_plugin_install_inner(&state, agent, source).await
}

pub(crate) async fn install_plugin_inner(
    state: &AppState,
    agent: String,
    source: String,
    confirmed: bool,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = acquire_plugin_write(&hub, agent)?;
        install_plugin_impl(agent, &source, PluginInstallOptions { confirmed })
    })
    .await
    .map_err(|e| format!("install_plugin join error: {e}"))?
}

/// Invoke: `install_plugin` — official CLI after UI confirm (`--trust` / `-y`).
#[tauri::command]
pub async fn install_plugin(
    state: State<'_, AppState>,
    agent: String,
    source: String,
    confirmed: bool,
) -> Result<(), String> {
    install_plugin_inner(&state, agent, source, confirmed).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn uninstall_plugin_inner(
    state: &AppState,
    agent: String,
    name: String,
    marketplace: Option<String>,
    install_source: Option<String>,
    keep_data: Option<bool>,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = acquire_plugin_write(&hub, agent)?;
        uninstall_plugin_impl(
            agent,
            &name,
            marketplace.as_deref(),
            install_source.as_deref(),
            PluginUninstallOptions {
                keep_data: keep_data.unwrap_or(true),
            },
        )
    })
    .await
    .map_err(|e| format!("uninstall_plugin join error: {e}"))?
}

/// Invoke: `uninstall_plugin` — official CLI. Default keeps plugin data.
#[tauri::command]
pub async fn uninstall_plugin(
    state: State<'_, AppState>,
    agent: String,
    name: String,
    marketplace: Option<String>,
    install_source: Option<String>,
    keep_data: Option<bool>,
) -> Result<(), String> {
    uninstall_plugin_inner(&state, agent, name, marketplace, install_source, keep_data).await
}

pub(crate) async fn refresh_plugin_marketplace_inner(
    state: &AppState,
    agent: String,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = acquire_plugin_write(&hub, agent)?;
        refresh_plugin_marketplace_impl(agent)
    })
    .await
    .map_err(|e| format!("refresh_plugin_marketplace join error: {e}"))?
}

/// Invoke: `refresh_plugin_marketplace` — refresh Claude/Codex/Grok catalogs.
#[tauri::command]
pub async fn refresh_plugin_marketplace(
    state: State<'_, AppState>,
    agent: String,
) -> Result<(), String> {
    refresh_plugin_marketplace_inner(&state, agent).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn update_plugin_inner(
    state: &AppState,
    agent: String,
    name: String,
    marketplace: Option<String>,
    scope: Option<String>,
    confirmed: bool,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = acquire_plugin_write(&hub, agent)?;
        update_plugin_impl(
            agent,
            &name,
            marketplace.as_deref(),
            scope.as_deref(),
            PluginUpdateOptions { confirmed },
        )
    })
    .await
    .map_err(|e| format!("update_plugin join error: {e}"))?
}

/// Invoke: `update_plugin` — update one installed Claude/Grok pack after confirm.
#[tauri::command]
pub async fn update_plugin(
    state: State<'_, AppState>,
    agent: String,
    name: String,
    marketplace: Option<String>,
    scope: Option<String>,
    confirmed: bool,
) -> Result<(), String> {
    update_plugin_inner(&state, agent, name, marketplace, scope, confirmed).await
}

pub(crate) async fn update_pi_plugins_inner(
    state: &AppState,
    confirmed: bool,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _live_write = acquire_plugin_write(&hub, AgentId::Pi)?;
        update_pi_plugins_impl(PluginUpdateOptions { confirmed })
    })
    .await
    .map_err(|e| format!("update_pi_plugins join error: {e}"))?
}

/// Invoke: `update_pi_plugins` — update eligible Pi extensions after confirm.
#[tauri::command]
pub async fn update_pi_plugins(state: State<'_, AppState>, confirmed: bool) -> Result<(), String> {
    update_pi_plugins_inner(&state, confirmed).await
}

fn acquire_plugin_write(
    hub: &agenthub_core::AgentHub,
    agent: AgentId,
) -> Result<agenthub_core::services::LiveWriteGuard, String> {
    hub.backups()
        .acquire_live_write(agent)
        .map_err(|error| error.to_string())
}
