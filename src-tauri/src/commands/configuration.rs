//! Configuration schema / read / validate / apply Tauri commands.

use std::collections::BTreeMap;

use agenthub_core::{
    AgentConfigSchema, ConfigApplyResult, ConfigChangePlan, ConfigValidationResult,
    NormalizedConfigDocument,
};
use serde_json::Value;
use tauri::State;

use crate::commands::{map_err_string, parse_agent, with_hub_blocking};
use crate::state::AppState;

/// Invoke: `get_agent_config_schema`
#[tauri::command]
pub async fn get_agent_config_schema(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<AgentConfigSchema, String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    with_hub_blocking(hub, move |hub| {
        hub.configuration
            .schema(agent)
            .map_err(|e| map_err_string("get_agent_config_schema", e))
    })
    .await
}

/// Invoke: `read_agent_config`
#[tauri::command]
pub async fn read_agent_config(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<NormalizedConfigDocument, String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    with_hub_blocking(hub, move |hub| {
        hub.configuration
            .read(agent)
            .map_err(|e| map_err_string("read_agent_config", e))
    })
    .await
}

/// Invoke: `validate_agent_config`
#[tauri::command]
pub async fn validate_agent_config(
    state: State<'_, AppState>,
    agent_id: String,
    values: Value,
) -> Result<ConfigValidationResult, String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    with_hub_blocking(hub, move |hub| {
        hub.configuration
            .validate_value(agent, values)
            .map_err(|e| map_err_string("validate_agent_config", e))
    })
    .await
}

/// Invoke: `plan_agent_config`
#[tauri::command]
pub async fn plan_agent_config(
    state: State<'_, AppState>,
    agent_id: String,
    values: Value,
) -> Result<ConfigChangePlan, String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    with_hub_blocking(hub, move |hub| {
        let map = value_to_map(values)?;
        hub.configuration
            .plan_apply(agent, &map)
            .map_err(|e| map_err_string("plan_agent_config", e))
    })
    .await
}

/// Invoke: `apply_agent_config`
#[tauri::command]
pub async fn apply_agent_config(
    state: State<'_, AppState>,
    agent_id: String,
    values: Value,
) -> Result<ConfigApplyResult, String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    with_hub_blocking(hub, move |hub| {
        hub.configuration
            .apply_value(agent, values)
            .map_err(|e| map_err_string("apply_agent_config", e))
    })
    .await
}

/// Invoke: `materialize_agent_config` — pool settings_config without FS write.
#[tauri::command]
pub async fn materialize_agent_config(
    state: State<'_, AppState>,
    agent_id: String,
    values: Value,
    base_raw: Option<Value>,
) -> Result<Value, String> {
    let hub = state.hub_arc()?;
    let agent = parse_agent(&agent_id)?;
    with_hub_blocking(hub, move |hub| {
        hub.configuration
            .materialize_settings_config_value(agent, values, base_raw)
            .map_err(|e| map_err_string("materialize_agent_config", e))
    })
    .await
}

fn value_to_map(values: Value) -> Result<BTreeMap<String, Value>, String> {
    let obj = values.as_object().ok_or_else(|| {
        "config values must be a JSON object [invalid_arg]".to_string()
    })?;
    Ok(obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}
