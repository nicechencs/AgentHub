//! Read-only MCP inventory — thin wrapper over core scanner.

use agenthub_core::services::list_mcp_inventory as list_mcp_inventory_impl;
use agenthub_core::services::McpInventory;

/// Invoke: `list_mcp_inventory` — scan known agent MCP config files (read-only).
#[tauri::command]
pub async fn list_mcp_inventory() -> Result<McpInventory, String> {
    // Pure filesystem scan; no hub lock required.
    tauri::async_runtime::spawn_blocking(list_mcp_inventory_impl)
        .await
        .map_err(|e| format!("list_mcp_inventory join error: {e}"))
}

/// Compatibility alias for `list_mcp_inventory` (pre-rename IPC).
#[tauri::command]
pub async fn list_mcp_inventory_cmd() -> Result<McpInventory, String> {
    list_mcp_inventory().await
}
