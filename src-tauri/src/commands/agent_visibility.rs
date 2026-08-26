//! Agent visibility (soft-hide) commands — thin wrappers over agenthub-core.

use tauri::State;

use crate::commands::{map_err_string, parse_agent, with_hub_blocking};
use crate::state::AppState;

/// Invoke: `list_hidden_agents`
#[tauri::command]
pub async fn list_hidden_agents(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.agent_visibility()
            .list_hidden_agents()
            .map_err(|e| map_err_string("list_hidden_agents", e))
    })
    .await
}

/// Invoke: `set_agent_hidden`
#[tauri::command]
pub async fn set_agent_hidden(
    state: State<'_, AppState>,
    agent_id: String,
    hidden: bool,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        let id = parse_agent(&agent_id)?;
        hub.agent_visibility()
            .set_agent_hidden(id, hidden)
            .map_err(|e| map_err_string("set_agent_hidden", e))
    })
    .await
}
