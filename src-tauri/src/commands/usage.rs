//! Usage Tauri commands — collect / query / trend / models / health.

use agenthub_core::models::{
    AgentId, CollectResult, GatewayUsageOverview, GatewayUsageQuery, GatewayUsageRow, ParserHealth,
    UsageOverview, UsageQuery, UsageRecord,
};
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
        hub.usage()
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
    limit: Option<u32>,
    since: Option<String>,
    exclude_agent_ids: Option<Vec<String>>,
    until: Option<String>,
) -> Result<Vec<UsageRecord>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        let agent = parse_agent_opt(agent_id.as_deref())?;
        let model = model.filter(|m| !m.is_empty() && m != "all");
        let since = since.filter(|s| !s.is_empty());
        let until = until.filter(|s| !s.is_empty());
        let exclude_agent_ids = parse_exclude_agent_ids(exclude_agent_ids);
        hub.usage()
            .query(UsageQuery {
                days: days.max(1),
                agent_id: agent,
                model,
                limit,
                since,
                until,
                exclude_agent_ids,
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
    model: Option<String>,
    since: Option<String>,
    exclude_agent_ids: Option<Vec<String>>,
    group_by: Option<String>,
    until: Option<String>,
) -> Result<Vec<Value>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        let agent = parse_agent_opt(agent_id.as_deref())?;
        let model = model.filter(|m| !m.is_empty() && m != "all");
        let since = since.filter(|s| !s.is_empty());
        let until = until.filter(|s| !s.is_empty());
        let exclude = parse_exclude_agent_ids(exclude_agent_ids);
        let by_model = group_by.as_deref() == Some("model");
        let points = if by_model {
            hub.usage().trend_by_model(
                days.max(1),
                agent,
                model.as_deref(),
                since.as_deref(),
                &exclude,
                until.as_deref(),
            )
        } else {
            hub.usage().trend(
                days.max(1),
                agent,
                model.as_deref(),
                since.as_deref(),
                &exclude,
                until.as_deref(),
            )
        }
        .map_err(|e| map_err_string("usage_trend", e))?;
        Ok(points.into_iter().map(|p| Value::Object(p.0)).collect())
    })
    .await
}

#[tauri::command]
pub async fn usage_overview(
    state: State<'_, AppState>,
    days: u32,
    agent_id: Option<String>,
    model: Option<String>,
    since: Option<String>,
    exclude_agent_ids: Option<Vec<String>>,
    until: Option<String>,
) -> Result<UsageOverview, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        let agent = parse_agent_opt(agent_id.as_deref())?;
        let model = model.filter(|m| !m.is_empty() && m != "all");
        let since = since.filter(|s| !s.is_empty());
        let until = until.filter(|s| !s.is_empty());
        let exclude = parse_exclude_agent_ids(exclude_agent_ids);
        hub.usage()
            .overview(
                days.max(1),
                agent,
                model.as_deref(),
                since.as_deref(),
                &exclude,
                until.as_deref(),
            )
            .map_err(|e| map_err_string("usage_overview", e))
    })
    .await
}

fn parse_exclude_agent_ids(ids: Option<Vec<String>>) -> Vec<AgentId> {
    ids.unwrap_or_default()
        .into_iter()
        .filter_map(|s| AgentId::parse(&s))
        .collect()
}

#[tauri::command]
pub async fn usage_list_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.usage()
            .list_models()
            .map_err(|e| map_err_string("usage_list_models", e))
    })
    .await
}

#[tauri::command]
pub async fn usage_parser_health(state: State<'_, AppState>) -> Result<Vec<ParserHealth>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.usage()
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
        hub.usage()
            .missing_pricing_models(d)
            .map_err(|e| map_err_string("usage_missing_pricing", e))
    })
    .await
}

/// Per-request usage observed by the local bridge (separate `gateway_usage`
/// table; never merged into `usage_records`).
#[tauri::command]
pub async fn gateway_usage_query(
    state: State<'_, AppState>,
    since: Option<String>,
    until: Option<String>,
    profile_id: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<GatewayUsageRow>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.usage()
            .gateway_usage_query(GatewayUsageQuery {
                since: since.filter(|s| !s.is_empty()),
                until: until.filter(|s| !s.is_empty()),
                profile_id: profile_id.filter(|p| !p.is_empty()),
                limit,
            })
            .map_err(|e| map_err_string("gateway_usage_query", e))
    })
    .await
}

/// Aggregated overview over the `gateway_usage` table for a time window.
#[tauri::command]
pub async fn gateway_usage_overview(
    state: State<'_, AppState>,
    since: Option<String>,
    until: Option<String>,
    profile_id: Option<String>,
) -> Result<GatewayUsageOverview, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.usage()
            .gateway_usage_overview(GatewayUsageQuery {
                since: since.filter(|s| !s.is_empty()),
                until: until.filter(|s| !s.is_empty()),
                profile_id: profile_id.filter(|p| !p.is_empty()),
                limit: None,
            })
            .map_err(|e| map_err_string("gateway_usage_overview", e))
    })
    .await
}
