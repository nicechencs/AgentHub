//! MCP inventory + catalog / probe / write / enable.

use agenthub_core::services::{
    list_mcp_catalog as list_mcp_catalog_impl, list_mcp_inventory as list_mcp_inventory_impl,
    probe_mcp_server as probe_mcp_server_impl, set_mcp_server_enabled as set_mcp_server_enabled_impl,
    upsert_mcp_server as upsert_mcp_server_impl, McpCatalogEntry, McpInventory, McpProbeResult,
    McpServerSpec, McpWriteResult,
};

use super::parse_agent;

/// Invoke: `list_mcp_inventory` — scan known agent MCP config files.
#[tauri::command]
pub async fn list_mcp_inventory() -> Result<McpInventory, String> {
    tauri::async_runtime::spawn_blocking(list_mcp_inventory_impl)
        .await
        .map_err(|e| format!("list_mcp_inventory join error: {e}"))
}

/// Invoke: `list_mcp_catalog` — built-in local templates (no remote marketplace).
#[tauri::command]
pub async fn list_mcp_catalog() -> Result<Vec<McpCatalogEntry>, String> {
    Ok(list_mcp_catalog_impl())
}

/// Invoke: `probe_mcp_server` — stdio PATH / HTTP connect check (no OAuth).
#[tauri::command]
pub async fn probe_mcp_server(spec: McpServerSpec) -> Result<McpProbeResult, String> {
    tauri::async_runtime::spawn_blocking(move || probe_mcp_server_impl(&spec).map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("probe_mcp_server join error: {e}"))?
}

/// Invoke: `upsert_mcp_server` — write MCP into a supported Agent config.
#[tauri::command]
pub async fn upsert_mcp_server(agent: String, spec: McpServerSpec) -> Result<McpWriteResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        upsert_mcp_server_impl(agent, &spec).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("upsert_mcp_server join error: {e}"))?
}

/// Invoke: `set_mcp_server_enabled` — toggle enabled (Codex disable removes the entry).
#[tauri::command]
pub async fn set_mcp_server_enabled(
    agent: String,
    name: String,
    enabled: bool,
) -> Result<McpWriteResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let agent = parse_agent(&agent)?;
        set_mcp_server_enabled_impl(agent, &name, enabled).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("set_mcp_server_enabled join error: {e}"))?
}
