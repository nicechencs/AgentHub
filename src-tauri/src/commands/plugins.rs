//! Vendor plugin / extension pack inventory and supported package operations.

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

/// Invoke: `list_plugin_inventory` — Claude/Codex/Grok/Pi plugin packs (not MCP).
#[tauri::command]
pub async fn list_plugin_inventory() -> Result<PluginInventory, String> {
    tauri::async_runtime::spawn_blocking(list_plugin_inventory_impl)
        .await
        .map_err(|e| format!("list_plugin_inventory join error: {e}"))
}

/// Invoke: `enable_plugin` — supported CLI/config enablement for the selected Agent.
#[tauri::command]
pub async fn enable_plugin(
    state: State<'_, AppState>,
    agent: String,
    name: String,
    marketplace: Option<String>,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = hub
            .backups()
            .acquire_live_write(agent)
            .map_err(|error| error.to_string())?;
        enable_plugin_impl(agent, &name, marketplace.as_deref())
    })
    .await
    .map_err(|e| format!("enable_plugin join error: {e}"))?
}

/// Invoke: `disable_plugin` — supported CLI/config disablement for the selected Agent.
#[tauri::command]
pub async fn disable_plugin(
    state: State<'_, AppState>,
    agent: String,
    name: String,
    marketplace: Option<String>,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = hub
            .backups()
            .acquire_live_write(agent)
            .map_err(|error| error.to_string())?;
        disable_plugin_impl(agent, &name, marketplace.as_deref())
    })
    .await
    .map_err(|e| format!("disable_plugin join error: {e}"))?
}

/// Invoke: `list_available_plugins` — marketplace rows, not installed inventory.
#[tauri::command]
pub async fn list_available_plugins(agent: String) -> Result<Vec<PluginEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        list_available_plugins_impl(agent)
    })
    .await
    .map_err(|e| format!("list_available_plugins join error: {e}"))?
}

/// Invoke: `preview_plugin_install` — component list before confirm.
#[tauri::command]
pub async fn preview_plugin_install(agent: String, source: String) -> Result<PluginEntry, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        preview_plugin_install_impl(agent, &source)
    })
    .await
    .map_err(|e| format!("preview_plugin_install join error: {e}"))?
}

/// Invoke: `install_plugin` — official CLI after UI confirm (`--trust` / `-y`).
#[tauri::command]
pub async fn install_plugin(
    state: State<'_, AppState>,
    agent: String,
    source: String,
    confirmed: bool,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = hub
            .backups()
            .acquire_live_write(agent)
            .map_err(|error| error.to_string())?;
        install_plugin_impl(agent, &source, PluginInstallOptions { confirmed })
    })
    .await
    .map_err(|e| format!("install_plugin join error: {e}"))?
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
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = hub
            .backups()
            .acquire_live_write(agent)
            .map_err(|error| error.to_string())?;
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

/// Invoke: `refresh_plugin_marketplace` — refresh Claude/Codex/Grok catalogs.
#[tauri::command]
pub async fn refresh_plugin_marketplace(
    state: State<'_, AppState>,
    agent: String,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = hub
            .backups()
            .acquire_live_write(agent)
            .map_err(|error| error.to_string())?;
        refresh_plugin_marketplace_impl(agent)
    })
    .await
    .map_err(|e| format!("refresh_plugin_marketplace join error: {e}"))?
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
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        let _live_write = hub
            .backups()
            .acquire_live_write(agent)
            .map_err(|error| error.to_string())?;
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

/// Invoke: `update_pi_plugins` — update eligible Pi extensions after confirm.
#[tauri::command]
pub async fn update_pi_plugins(state: State<'_, AppState>, confirmed: bool) -> Result<(), String> {
    let hub = state.hub_arc()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _live_write = hub
            .backups()
            .acquire_live_write(agenthub_core::models::AgentId::Pi)
            .map_err(|error| error.to_string())?;
        update_pi_plugins_impl(PluginUpdateOptions { confirmed })
    })
    .await
    .map_err(|e| format!("update_pi_plugins join error: {e}"))?
}
