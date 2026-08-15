//! Provider pool Tauri commands — thin wrappers over agenthub-core.
//!
//! All responses that may contain credentials are redacted before return.
//! Upsert merges preserved secrets when the client sends the redaction marker
//! `"***"` so a read-edit-save cycle does not wipe real keys.

use agenthub_core::models::{Provider, ProviderInput, ProviderPreset, ProviderSwitchResult};
use agenthub_core::presets;
use agenthub_core::utils::redact::is_secret_key;
use agenthub_core::AgentHub;
use serde::Serialize;
use serde_json::{Map, Value};
use tauri::State;

use crate::commands::{map_err_string, parse_agent, parse_agent_opt, with_hub_blocking};
use crate::state::AppState;

const REDACTED_MARKER: &str = "***";

/// Switch confirmation payload for the GUI dialog (no live writes).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchPreview {
    pub backfill_summary: String,
    pub backup_path: String,
    pub process_warning: Option<String>,
}

/// Invoke: `list_provider_presets`
#[tauri::command]
pub fn list_provider_presets(agent_id: Option<String>) -> Result<Vec<ProviderPreset>, String> {
    let filter = parse_agent_opt(agent_id.as_deref())?;
    Ok(presets::list(filter))
}

/// Invoke: `list_providers` — redacted rows.
#[tauri::command]
pub async fn list_providers(
    state: State<'_, AppState>,
    agent_id: Option<String>,
) -> Result<Vec<Provider>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        list_providers_inner(hub, agent_id.as_deref())
    })
    .await
}

/// Invoke: `get_provider` — redacted single row.
#[tauri::command]
pub async fn get_provider(
    state: State<'_, AppState>,
    id_or_name: String,
    agent_id: Option<String>,
) -> Result<Provider, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        get_provider_inner(hub, &id_or_name, agent_id.as_deref())
    })
    .await
}

/// Invoke: `upsert_provider` — create or update with secret-preserving merge.
#[tauri::command]
pub async fn upsert_provider(
    state: State<'_, AppState>,
    input: ProviderInput,
) -> Result<Provider, String> {
    let hub = state.hub_arc()?;
    let _target_guard = state
        .bridge_saga_coordinator()
        .lock_target(input.agent_id)
        .await;
    with_hub_blocking(hub, move |hub| upsert_provider_inner(hub, input)).await
}

/// Invoke: `delete_provider`
#[tauri::command]
pub async fn delete_provider(
    state: State<'_, AppState>,
    agent_id: String,
    provider_id: String,
) -> Result<(), String> {
    let agent = parse_agent(&agent_id)?;
    let hub = state.hub_arc()?;
    let _target_guard = state.bridge_saga_coordinator().lock_target(agent).await;
    with_hub_blocking(hub, move |hub| {
        delete_provider_inner(hub, &agent_id, &provider_id)
    })
    .await
}

/// Invoke: `import_provider_live` — capture live agent config into the pool.
#[tauri::command]
pub async fn import_provider_live(
    state: State<'_, AppState>,
    agent_id: String,
    name: Option<String>,
) -> Result<Provider, String> {
    let agent = parse_agent(&agent_id)?;
    let hub = state.hub_arc()?;
    let _target_guard = state.bridge_saga_coordinator().lock_target(agent).await;
    with_hub_blocking(hub, move |hub| {
        import_provider_live_inner(hub, &agent_id, name.as_deref())
    })
    .await
}

/// Invoke: `switch_provider` — backfill → backup → live write → DB select.
#[tauri::command]
pub async fn switch_provider(
    state: State<'_, AppState>,
    agent_id: String,
    id_or_name: String,
) -> Result<ProviderSwitchResult, String> {
    let agent = parse_agent(&agent_id)?;
    let hub = state.hub_arc()?;
    let _target_guard = state.bridge_saga_coordinator().lock_target(agent).await;
    with_hub_blocking(hub, move |hub| {
        switch_provider_inner(hub, &agent_id, &id_or_name)
    })
    .await
}

/// Invoke: `switch_provider_preview` — read-only dialog summary (no writes).
#[tauri::command]
pub async fn switch_provider_preview(
    state: State<'_, AppState>,
    agent_id: String,
    id_or_name: String,
) -> Result<SwitchPreview, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        switch_provider_preview_inner(hub, &agent_id, &id_or_name)
    })
    .await
}

/// Invoke: `undo_switch_provider` — re-apply the previous provider after a switch.
#[tauri::command]
pub async fn undo_switch_provider(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<bool, String> {
    let agent = parse_agent(&agent_id)?;
    let hub = state.hub_arc()?;
    let _target_guard = state.bridge_saga_coordinator().lock_target(agent).await;
    with_hub_blocking(hub, move |hub| {
        hub.providers
            .undo_switch(agent)
            .map_err(|e| map_err_string("undo_switch_provider", e))
    })
    .await
}

/// Invoke: `test_provider_latency` — probe Base URL RTT in milliseconds.
#[tauri::command]
pub async fn test_provider_latency(
    state: State<'_, AppState>,
    agent_id: String,
    provider_id: String,
) -> Result<u64, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        let agent = parse_agent(&agent_id)?;
        hub.providers
            .test_latency(agent, &provider_id)
            .map_err(|e| map_err_string("test_provider_latency", e))
    })
    .await
}

// ---------------------------------------------------------------------------
// Testable inner implementations (take &AgentHub, no Tauri State)
// ---------------------------------------------------------------------------

fn list_providers_inner(hub: &AgentHub, agent_id: Option<&str>) -> Result<Vec<Provider>, String> {
    let filter = parse_agent_opt(agent_id)?;
    let items = hub
        .providers
        .list(filter)
        .map_err(|e| map_err_string("list_providers", e))?;
    Ok(items.into_iter().map(|p| p.redacted()).collect())
}

fn get_provider_inner(
    hub: &AgentHub,
    id_or_name: &str,
    agent_id: Option<&str>,
) -> Result<Provider, String> {
    let filter = parse_agent_opt(agent_id)?;
    let item = hub
        .providers
        .get(id_or_name, filter)
        .map_err(|e| map_err_string("get_provider", e))?;
    Ok(item.redacted())
}

fn upsert_provider_inner(hub: &AgentHub, mut input: ProviderInput) -> Result<Provider, String> {
    // Merge secrets from the stored row when the client re-sends "***".
    if let Ok(existing) = hub.providers.get(&input.id, Some(input.agent_id)) {
        input.settings_config =
            merge_preserving_secrets(&existing.settings_config, &input.settings_config);
        input.meta = merge_preserving_secrets(&existing.meta, &input.meta);
    }
    let saved = hub
        .providers
        .upsert(&input)
        .map_err(|e| map_err_string("upsert_provider", e))?;
    Ok(saved.redacted())
}

fn delete_provider_inner(hub: &AgentHub, agent_id: &str, provider_id: &str) -> Result<(), String> {
    let agent = parse_agent(agent_id)?;
    hub.providers
        .delete(provider_id, agent)
        .map_err(|e| map_err_string("delete_provider", e))
}

fn import_provider_live_inner(
    hub: &AgentHub,
    agent_id: &str,
    name: Option<&str>,
) -> Result<Provider, String> {
    let agent = parse_agent(agent_id)?;
    let item = hub
        .providers
        .import_live(agent, name)
        .map_err(|e| map_err_string("import_provider_live", e))?;
    Ok(item.redacted())
}

fn switch_provider_inner(
    hub: &AgentHub,
    agent_id: &str,
    id_or_name: &str,
) -> Result<ProviderSwitchResult, String> {
    let agent = parse_agent(agent_id)?;
    let result = hub
        .providers
        .switch(id_or_name, agent)
        .map_err(|e| map_err_string("switch_provider", e))?;
    Ok(result.redacted())
}

fn switch_provider_preview_inner(
    hub: &AgentHub,
    agent_id: &str,
    id_or_name: &str,
) -> Result<SwitchPreview, String> {
    let agent = parse_agent(agent_id)?;
    // Validate target exists and belongs to agent (no write).
    let _target = hub
        .providers
        .get(id_or_name, Some(agent))
        .map_err(|e| map_err_string("switch_provider_preview", e))?;
    let current = hub
        .providers
        .list(Some(agent))
        .map_err(|e| map_err_string("switch_provider_preview", e))?
        .into_iter()
        .find(|p| p.is_current);

    let backfill_summary = match current {
        Some(c) => format!("当前生效配置将回存为「{}」", c.name),
        None => "尚无生效配置，将直接写入本机".into(),
    };

    let backup_path = hub
        .backups
        .backups_root()
        .join("live")
        .join(agent.as_str())
        .display()
        .to_string();

    Ok(SwitchPreview {
        backfill_summary,
        backup_path,
        process_warning: None,
    })
}

/// When `new` carries the redaction marker at secret keys (or opaque TOML
/// content), keep the corresponding value from `old`.
fn merge_preserving_secrets(old: &Value, new: &Value) -> Value {
    match (old, new) {
        (Value::Object(old_map), Value::Object(new_map)) => {
            let opaque_toml = new_map.get("format").and_then(Value::as_str) == Some("toml")
                && new_map.get("content").is_some_and(Value::is_string);
            let mut out = Map::new();
            for (k, new_v) in new_map {
                let keep_old_secret = is_redacted_leaf(new_v)
                    && (is_secret_key(k) || (opaque_toml && k == "content"));
                if keep_old_secret {
                    if let Some(old_v) = old_map.get(k) {
                        out.insert(k.clone(), old_v.clone());
                    }
                    // No stored secret: omit the marker rather than write "***".
                    continue;
                }
                if let Some(old_v) = old_map.get(k) {
                    if new_v.is_object() && old_v.is_object() {
                        out.insert(k.clone(), merge_preserving_secrets(old_v, new_v));
                        continue;
                    }
                }
                out.insert(k.clone(), new_v.clone());
            }
            Value::Object(out)
        }
        (_, new_v) => new_v.clone(),
    }
}

fn is_redacted_leaf(value: &Value) -> bool {
    matches!(value, Value::String(s) if s == REDACTED_MARKER)
}

#[cfg(test)]
mod tests;
