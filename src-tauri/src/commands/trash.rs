use agenthub_core::models::ConnectionTrashItem;
use tauri::State;

use super::{map_err_string, parse_agent_opt, with_hub_blocking};
use crate::state::AppState;

/// List deleted connection records. Secrets stay redacted at the Tauri boundary.
#[tauri::command]
pub async fn list_connection_trash(
    state: State<'_, AppState>,
    agent_id: Option<String>,
    home: Option<String>,
) -> Result<Vec<ConnectionTrashItem>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        let agent = parse_agent_opt(agent_id.as_deref())?;
        let home = match home.as_deref() {
            None => None,
            Some("connections") | Some("route_pool") => home.as_deref(),
            Some(other) => return Err(format!("invalid trash home: {other}")),
        };
        hub.connections()
            .list_trash_filtered(agent, home)
            .map(|items| items.into_iter().map(|item| item.redacted()).collect())
            .map_err(|err| map_err_string("list_connection_trash", err))
    })
    .await
}

#[tauri::command]
pub async fn restore_connection_trash(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.restore_connection_trash(&id)
            .map_err(|err| map_err_string("restore_connection_trash", err))
    })
    .await
}

#[tauri::command]
pub async fn delete_connection_trash(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.connections()
            .delete_trash(&id)
            .map_err(|err| map_err_string("delete_connection_trash", err))
    })
    .await
}
