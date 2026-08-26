//! Read-only vendor plugin / extension pack inventory — thin wrapper over core.

use agenthub_core::services::PluginInventory;
use agenthub_core::services::list_plugin_inventory;

/// Invoke: `list_plugin_inventory` — Claude/Grok plugin packs (not MCP).
#[tauri::command]
pub async fn list_plugin_inventory_cmd() -> Result<PluginInventory, String> {
    tauri::async_runtime::spawn_blocking(list_plugin_inventory)
        .await
        .map_err(|e| format!("list_plugin_inventory join error: {e}"))
}
