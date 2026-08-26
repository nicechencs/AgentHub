//! Read-only Agent Catalog Tauri commands — thin wrappers over core.

use agenthub_core::AgentDescriptor;
use tauri::State;

use crate::commands::{map_err_string, with_hub_blocking};
use crate::state::AppState;

/// Invoke: `list_agent_catalog`
///
/// Full agent directory (keys, display names, capabilities, install channels).
/// Deterministic product order from core [`agenthub_core::AgentCatalogService`].
#[tauri::command]
pub async fn list_agent_catalog(
    state: State<'_, AppState>,
) -> Result<Vec<AgentDescriptor>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| Ok(hub.catalog().list_owned())).await
}

/// Invoke: `get_agent_catalog_entry`
///
/// Lookup one agent by string key. Invalid format → `invalid_arg`;
/// unknown key → `not_found` (no fallback to a known agent).
#[tauri::command]
pub async fn get_agent_catalog_entry(
    state: State<'_, AppState>,
    key: String,
) -> Result<AgentDescriptor, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.catalog()
            .get_str(&key)
            .map(|d| d.clone())
            .map_err(|e| map_err_string("get_agent_catalog_entry", e))
    })
    .await
}
