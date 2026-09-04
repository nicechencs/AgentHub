use tauri::State;

use crate::commands::{map_err_string, with_hub_blocking};
use crate::state::AppState;

const REMEMBERED_VAULT_SETTING_KEY: &str = "sub2api_remembered_password_vault";

/// Invoke: `sub2api_remembered_vault_get` — JSON map accountId -> secret.
/// Never log the returned value.
#[tauri::command]
pub async fn sub2api_remembered_vault_get(
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, |hub| {
        hub.db()
            .get_setting(REMEMBERED_VAULT_SETTING_KEY)
            .map_err(|e| map_err_string("sub2api_remembered_vault_get", e))
    })
    .await
}

/// Invoke: `sub2api_remembered_vault_set` — replace vault JSON (may be `{}`).
/// Never log `json`.
#[tauri::command]
pub async fn sub2api_remembered_vault_set(
    state: State<'_, AppState>,
    json: String,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.db()
            .set_setting(REMEMBERED_VAULT_SETTING_KEY, &json)
            .map_err(|e| map_err_string("sub2api_remembered_vault_set", e))
    })
    .await
}
