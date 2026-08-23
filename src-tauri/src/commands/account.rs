//! Account Tauri commands — thin wrappers over agenthub-core.
//!
//! All responses that may contain credentials are redacted before return.

use agenthub_core::models::{Account, AccountSwitchResult, AuthState};
use agenthub_core::AgentHub;
use tauri::State;

use crate::commands::{map_err_string, parse_agent, with_hub_blocking};
use crate::state::AppState;

/// Invoke: `list_accounts`
#[tauri::command]
pub async fn list_accounts(
    state: State<'_, AppState>,
    agent_id: Option<String>,
) -> Result<Vec<Account>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        list_accounts_inner(hub, agent_id.as_deref())
    })
    .await
}

/// Invoke: `probe_live_auth` — read-only, redacted authentication status.
#[tauri::command]
pub async fn probe_live_auth(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<AuthState, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| probe_live_auth_inner(hub, &agent_id)).await
}

/// Invoke: `import_account_live`
#[tauri::command]
pub async fn import_account_live(
    state: State<'_, AppState>,
    agent_id: String,
    name: Option<String>,
) -> Result<Account, String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    let _target_guard = state.bridge_saga_coordinator().lock_target(agent).await;
    with_hub_blocking(hub, move |hub| {
        import_account_live_inner(hub, &agent_id, name.as_deref())
    })
    .await
}

/// Invoke: `add_api_key_account`
#[tauri::command]
pub async fn add_api_key_account(
    state: State<'_, AppState>,
    agent_id: String,
    key: String,
    label: Option<String>,
    env_key: Option<String>,
    product_marker: Option<String>,
) -> Result<Account, String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    let _target_guard = state.bridge_saga_coordinator().lock_target(agent).await;
    with_hub_blocking(hub, move |hub| {
        add_api_key_account_inner_with_marker(
            hub,
            &agent_id,
            &key,
            label.as_deref(),
            env_key.as_deref(),
            product_marker.as_deref(),
        )
    })
    .await
}

/// Invoke: `update_api_key_account`
#[tauri::command]
pub async fn update_api_key_account(
    state: State<'_, AppState>,
    agent_id: String,
    id_or_label: String,
    label: Option<String>,
    key: Option<String>,
) -> Result<Account, String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    let _target_guard = state.bridge_saga_coordinator().lock_target(agent).await;
    with_hub_blocking(hub, move |hub| {
        update_api_key_account_inner(
            hub,
            &agent_id,
            &id_or_label,
            label.as_deref(),
            key.as_deref(),
        )
    })
    .await
}

/// Invoke: `switch_account`
#[tauri::command]
pub async fn switch_account(
    state: State<'_, AppState>,
    agent_id: String,
    id_or_label: String,
) -> Result<AccountSwitchResult, String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    let _target_guard = state.bridge_saga_coordinator().lock_target(agent).await;
    with_hub_blocking(hub, move |hub| {
        switch_account_inner(hub, &agent_id, &id_or_label)
    })
    .await
}

/// Invoke: `undo_switch_account` — re-apply the previous account after a switch.
#[tauri::command]
pub async fn undo_switch_account(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<bool, String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    let _target_guard = state.bridge_saga_coordinator().lock_target(agent).await;
    with_hub_blocking(hub, move |hub| {
        hub.accounts
            .undo_switch(agent)
            .map_err(|e| map_err_string("undo_switch_account", e))
    })
    .await
}

/// Invoke: `delete_account`
#[tauri::command]
pub async fn delete_account(
    state: State<'_, AppState>,
    agent_id: String,
    id_or_label: String,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    let _target_guard = state.bridge_saga_coordinator().lock_target(agent).await;
    with_hub_blocking(hub, move |hub| {
        delete_account_inner(hub, &agent_id, &id_or_label)
    })
    .await
}

/// Invoke: `refresh_account_token`
#[tauri::command]
pub async fn refresh_account_token(
    state: State<'_, AppState>,
    agent_id: String,
    id_or_label: String,
) -> Result<Account, String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    let _target_guard = state.bridge_saga_coordinator().lock_target(agent).await;
    with_hub_blocking(hub, move |hub| {
        hub.accounts
            .refresh_token(&id_or_label, agent)
            .map(|a| a.redacted())
            .map_err(|e| map_err_string("refresh_account_token", e))
    })
    .await
}

/// Invoke: `refresh_account_quota` — force 5h/7d upstream quota probe for OAuth.
#[tauri::command]
pub async fn refresh_account_quota(
    state: State<'_, AppState>,
    agent_id: String,
    id_or_label: String,
) -> Result<Account, String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    let _target_guard = state.bridge_saga_coordinator().lock_target(agent).await;
    with_hub_blocking(hub, move |hub| {
        hub.accounts
            .refresh_quota(&id_or_label, agent)
            .map(|a| a.redacted())
            .map_err(|e| map_err_string("refresh_account_quota", e))
    })
    .await
}

fn list_accounts_inner(hub: &AgentHub, agent_id: Option<&str>) -> Result<Vec<Account>, String> {
    let filter = match agent_id {
        None => None,
        Some(s) if s.trim().is_empty() => None,
        Some(s) => Some(parse_agent(s)?),
    };
    let items = hub
        .accounts
        .list(filter)
        .map_err(|e| map_err_string("list_accounts", e))?;
    Ok(items.into_iter().map(|a| a.redacted()).collect())
}

fn probe_live_auth_inner(hub: &AgentHub, agent_id: &str) -> Result<AuthState, String> {
    let agent = parse_agent(agent_id)?;
    let adapter = hub
        .registry
        .get(agent)
        .ok_or_else(|| format!("adapter not registered: {}", agent.as_str()))?;
    let mut state = adapter
        .read_auth()
        .map_err(|e| map_err_string("probe_live_auth", e))?;
    if hub
        .accounts
        .live_is_adapter_projection(agent)
        .unwrap_or(false)
        && !state
            .also_present
            .iter()
            .any(|kind| kind == agenthub_core::services::ADAPTER_PROJECTION_KIND)
    {
        state
            .also_present
            .push(agenthub_core::services::ADAPTER_PROJECTION_KIND.to_owned());
    }
    Ok(state)
}

fn import_account_live_inner(
    hub: &AgentHub,
    agent_id: &str,
    name: Option<&str>,
) -> Result<Account, String> {
    let agent = parse_agent(agent_id)?;
    let item = hub
        .accounts
        .import_live(agent, name)
        .map_err(|e| map_err_string("import_account_live", e))?;
    Ok(item.redacted())
}

fn add_api_key_account_inner(
    hub: &AgentHub,
    agent_id: &str,
    key: &str,
    label: Option<&str>,
    env_key: Option<&str>,
) -> Result<Account, String> {
    add_api_key_account_inner_with_marker(hub, agent_id, key, label, env_key, None)
}

fn add_api_key_account_inner_with_marker(
    hub: &AgentHub,
    agent_id: &str,
    key: &str,
    label: Option<&str>,
    env_key: Option<&str>,
    product_marker: Option<&str>,
) -> Result<Account, String> {
    let agent = parse_agent(agent_id)?;
    let item = hub
        .accounts
        .add_api_key_with_env_and_marker(agent, label, key, env_key, product_marker)
        .map_err(|e| map_err_string("add_api_key_account", e))?;
    Ok(item.redacted())
}

fn update_api_key_account_inner(
    hub: &AgentHub,
    agent_id: &str,
    id_or_label: &str,
    label: Option<&str>,
    key: Option<&str>,
) -> Result<Account, String> {
    let agent = parse_agent(agent_id)?;
    let item = hub
        .accounts
        .update_api_key(agent, id_or_label, label, key)
        .map_err(|e| map_err_string("update_api_key_account", e))?;
    Ok(item.redacted())
}

fn switch_account_inner(
    hub: &AgentHub,
    agent_id: &str,
    id_or_label: &str,
) -> Result<AccountSwitchResult, String> {
    let agent = parse_agent(agent_id)?;
    let result = hub
        .accounts
        .switch(id_or_label, agent)
        .map_err(|e| map_err_string("switch_account", e))?;
    Ok(result.redacted())
}

fn delete_account_inner(hub: &AgentHub, agent_id: &str, id_or_label: &str) -> Result<(), String> {
    let agent = parse_agent(agent_id)?;
    hub.accounts
        .delete(id_or_label, agent)
        .map_err(|e| map_err_string("delete_account", e))
}

#[cfg(test)]
mod tests;
