//! Usage Tauri commands — collect / query / trend / models / health.

use agenthub_core::models::{CollectResult, ParserHealth, UsageQuery, UsageRecord};
use serde_json::Value;
use tauri::State;

use crate::commands::{map_err_string, parse_agent_opt, with_hub_blocking};
use crate::state::AppState;

#[tauri::command]
pub async fn usage_get_availability(state: State<'_, AppState>) -> Result<Value, String> {
    let _hub = state.hub_arc()?;
    // Wired in core; always available in Tauri desktop.
    Ok(serde_json::json!({ "status": "available" }))
}

#[tauri::command]
pub async fn usage_collect(
    state: State<'_, AppState>,
    agent_id: Option<String>,
) -> Result<CollectResult, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        let filter = parse_agent_opt(agent_id.as_deref())?;
        hub.usage
            .collect(filter)
            .map_err(|e| map_err_string("usage_collect", e))
    })
    .await
}

#[tauri::command]
pub async fn usage_query(
    state: State<'_, AppState>,
    days: u32,
    agent_id: Option<String>,
    model: Option<String>,
) -> Result<Vec<UsageRecord>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        let agent = parse_agent_opt(agent_id.as_deref())?;
        let model = model.filter(|m| !m.is_empty() && m != "all");
        hub.usage
            .query(UsageQuery {
                days: days.max(1),
                agent_id: agent,
                model,
            })
            .map_err(|e| map_err_string("usage_query", e))
    })
    .await
}

#[tauri::command]
pub async fn usage_trend(
    state: State<'_, AppState>,
    days: u32,
    agent_id: Option<String>,
) -> Result<Vec<Value>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        let agent = parse_agent_opt(agent_id.as_deref())?;
        let points = hub
            .usage
            .trend(days.max(1), agent)
            .map_err(|e| map_err_string("usage_trend", e))?;
        Ok(points.into_iter().map(|p| Value::Object(p.0)).collect())
    })
    .await
}

#[tauri::command]
pub async fn usage_list_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.usage
            .list_models()
            .map_err(|e| map_err_string("usage_list_models", e))
    })
    .await
}

#[tauri::command]
pub async fn usage_parser_health(state: State<'_, AppState>) -> Result<Vec<ParserHealth>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.usage
            .parser_health()
            .map_err(|e| map_err_string("usage_parser_health", e))
    })
    .await
}

/// Models in recent usage_records lacking embedded pricing.
#[tauri::command]
pub async fn usage_missing_pricing(
    state: State<'_, AppState>,
    days: Option<u32>,
) -> Result<Vec<String>, String> {
    let hub = state.hub_arc()?;
    let d = days.unwrap_or(30);
    with_hub_blocking(hub, move |hub| {
        hub.usage
            .missing_pricing_models(d)
            .map_err(|e| map_err_string("usage_missing_pricing", e))
    })
    .await
}
