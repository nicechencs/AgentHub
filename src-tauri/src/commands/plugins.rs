//! Vendor plugin / extension pack inventory and enable/disable — thin wrappers over core.

use agenthub_core::services::{
    disable_plugin, enable_plugin, list_plugin_inventory, PluginInventory,
};

use super::parse_agent;

/// Invoke: `list_plugin_inventory` — Claude/Grok plugin packs (not MCP).
#[tauri::command]
pub async fn list_plugin_inventory_cmd() -> Result<PluginInventory, String> {
    tauri::async_runtime::spawn_blocking(list_plugin_inventory)
        .await
        .map_err(|e| format!("list_plugin_inventory join error: {e}"))
}

/// Invoke: `enable_plugin` — official `claude plugin enable` / `grok plugin enable`.
#[tauri::command]
pub async fn enable_plugin_cmd(
    agent: String,
    name: String,
    marketplace: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        enable_plugin(agent, &name, marketplace.as_deref())
    })
    .await
    .map_err(|e| format!("enable_plugin join error: {e}"))?
}

/// Invoke: `disable_plugin` — official `claude plugin disable` / `grok plugin disable`.
#[tauri::command]
pub async fn disable_plugin_cmd(
    agent: String,
    name: String,
    marketplace: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        disable_plugin(agent, &name, marketplace.as_deref())
    })
    .await
    .map_err(|e| format!("disable_plugin join error: {e}"))?
}
