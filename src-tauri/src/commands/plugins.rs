//! Vendor plugin / extension pack inventory, enable/disable, install/uninstall.

use agenthub_core::services::{
    disable_plugin as disable_plugin_impl, enable_plugin as enable_plugin_impl,
    install_plugin as install_plugin_impl, list_available_plugins as list_available_plugins_impl,
    list_plugin_inventory as list_plugin_inventory_impl,
    preview_plugin_install as preview_plugin_install_impl,
    uninstall_plugin as uninstall_plugin_impl, PluginEntry, PluginInstallOptions, PluginInventory,
    PluginUninstallOptions,
};

use super::parse_agent;

/// Invoke: `list_plugin_inventory` — Claude/Grok/Pi plugin packs (not MCP).
#[tauri::command]
pub async fn list_plugin_inventory() -> Result<PluginInventory, String> {
    tauri::async_runtime::spawn_blocking(list_plugin_inventory_impl)
        .await
        .map_err(|e| format!("list_plugin_inventory join error: {e}"))
}

/// Invoke: `enable_plugin` — official `claude plugin enable` / `grok plugin enable`.
#[tauri::command]
pub async fn enable_plugin(
    agent: String,
    name: String,
    marketplace: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        enable_plugin_impl(agent, &name, marketplace.as_deref())
    })
    .await
    .map_err(|e| format!("enable_plugin join error: {e}"))?
}

/// Invoke: `disable_plugin` — official `claude plugin disable` / `grok plugin disable`.
#[tauri::command]
pub async fn disable_plugin(
    agent: String,
    name: String,
    marketplace: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
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
    agent: String,
    source: String,
    confirmed: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        install_plugin_impl(agent, &source, PluginInstallOptions { confirmed })
    })
    .await
    .map_err(|e| format!("install_plugin join error: {e}"))?
}

/// Invoke: `uninstall_plugin` — official CLI. Default keeps plugin data.
#[tauri::command]
pub async fn uninstall_plugin(
    agent: String,
    name: String,
    marketplace: Option<String>,
    keep_data: Option<bool>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        uninstall_plugin_impl(
            agent,
            &name,
            marketplace.as_deref(),
            PluginUninstallOptions {
                keep_data: keep_data.unwrap_or(true),
            },
        )
    })
    .await
    .map_err(|e| format!("uninstall_plugin join error: {e}"))?
}
