//! OAuth PKCE + device-code Tauri commands.

use agenthub_core::models::Account;
use agenthub_core::oauth::{
    self, DeviceOAuthPoll, DeviceOAuthStart, OAuthLoginOption, OAuthSessionInfo, StartOAuthResult,
};
use tauri::State;

use crate::commands::{map_err_string, parse_agent, with_hub_blocking};
use crate::state::AppState;

/// Invoke: `oauth_list_options`
#[tauri::command]
pub async fn oauth_list_options(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<Vec<OAuthLoginOption>, String> {
    let _hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    Ok(oauth::list_oauth_options(agent))
}

/// Invoke: `oauth_start`
#[tauri::command]
pub async fn oauth_start(
    state: State<'_, AppState>,
    agent_id: String,
    open_browser: Option<bool>,
    provider_key: Option<String>,
) -> Result<StartOAuthResult, String> {
    let hub = state.hub_arc()?;
    let open = open_browser.unwrap_or(true);
    with_hub_blocking(hub, move |_hub| {
        let agent = parse_agent(&agent_id)?;
        oauth::start_oauth(agent, open, provider_key.as_deref())
            .map_err(|e| map_err_string("oauth_start", e))
    })
    .await
}

/// Invoke: `oauth_wait`
#[tauri::command]
pub async fn oauth_wait(
    state: State<'_, AppState>,
    oauth_state: String,
    timeout_secs: Option<u64>,
) -> Result<OAuthSessionInfo, String> {
    let _hub = state.hub_arc()?;
    let timeout = timeout_secs.unwrap_or(120);
    // Blocking wait on async pool
    tauri::async_runtime::spawn_blocking(move || {
        oauth::wait_oauth(&oauth_state, timeout).map_err(|e| map_err_string("oauth_wait", e))
    })
    .await
    .map_err(|e| format!("oauth_wait join error: {e}"))?
}

/// Invoke: `oauth_complete` — exchange code and store account.
#[tauri::command]
pub async fn oauth_complete(
    state: State<'_, AppState>,
    oauth_state: String,
) -> Result<Account, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        oauth::complete_oauth(&hub.accounts, &oauth_state)
            .map(|a| a.redacted())
            .map_err(|e| map_err_string("oauth_complete", e))
    })
    .await
}

/// Invoke: `oauth_supported`
#[tauri::command]
pub async fn oauth_supported(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<bool, String> {
    let _hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    Ok(oauth::oauth_supported(agent))
}

/// Invoke: `oauth_device_start`
#[tauri::command]
pub async fn oauth_device_start(
    state: State<'_, AppState>,
    agent_id: String,
    provider_key: String,
) -> Result<DeviceOAuthStart, String> {
    let _hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    oauth::start_device_oauth(agent, &provider_key).map_err(|e| map_err_string("oauth_device_start", e))
}

/// Invoke: `oauth_device_poll`
#[tauri::command]
pub async fn oauth_device_poll(
    state: State<'_, AppState>,
    oauth_state: String,
) -> Result<DeviceOAuthPoll, String> {
    let _hub = state.hub_arc()?;
    oauth::poll_device_oauth(&oauth_state).map_err(|e| map_err_string("oauth_device_poll", e))
}

/// Invoke: `oauth_device_complete`
#[tauri::command]
pub async fn oauth_device_complete(
    state: State<'_, AppState>,
    oauth_state: String,
) -> Result<Account, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        oauth::complete_device_oauth(&hub.accounts, &oauth_state)
            .map(|a| a.redacted())
            .map_err(|e| map_err_string("oauth_device_complete", e))
    })
    .await
}
