use std::collections::HashSet;

use crate::models::{AgentId, DetectResult, DetectStatus, ParsedUsageEvent};
use crate::services::AgentVisibilityService;

use super::{cost_for_event, event_missing_pricing, visible_installed_agent_ids};

fn grok_event(
    model: &str,
    input: i64,
    output: i64,
    cache_create: i64,
    cache_read: i64,
    cost_usd: Option<f64>,
) -> ParsedUsageEvent {
    ParsedUsageEvent {
        agent_id: AgentId::Grok,
        model: model.into(),
        input_tokens: input,
        output_tokens: output,
        cache_creation_tokens: cache_create,
        cache_read_tokens: cache_read,
        session_id: Some("sess".into()),
        ts: "2025-06-15T00:00:00+00:00".into(),
        raw_hash: "h".into(),
        cost_usd,
    }
}

#[test]
fn cost_for_event_prefers_recorded_ticks() {
    let ev = grok_event("grok-4.5-build", 7180, 130, 0, 11264, Some(0.0185192));
    let cost = cost_for_event(&ev);
    assert!(
        (cost - 0.0185192).abs() < 1e-6,
        "invoice ticks must win over the pricing table, got {cost}"
    );
}

#[test]
fn cost_for_event_prices_grok_build_via_stripped_candidate() {
    let ev = grok_event("grok-4.5-build", 1_000_000, 0, 0, 0, None);
    let cost = cost_for_event(&ev);
    // grok-4.5 input is $2 / 1M
    assert!((cost - 2.0).abs() < 0.01, "got {cost}");
}

#[test]
fn cost_for_event_is_zero_for_unpriced_grok_model() {
    let ev = grok_event("grok-never-priced-build", 1_000_000, 1_000_000, 0, 0, None);
    let cost = cost_for_event(&ev);
    assert!((cost - 0.0).abs() < 1e-12, "got {cost}");
}

#[test]
fn event_missing_pricing_uses_grok_candidates() {
    let priced = grok_event("grok-4.5-build", 10, 1, 0, 0, None);
    let unpriced = grok_event("grok-never-priced-build", 10, 1, 0, 0, None);
    assert!(!event_missing_pricing(&priced));
    assert!(event_missing_pricing(&unpriced));
}

#[test]
fn event_missing_pricing_does_not_fire_when_ticks_are_absent_but_table_matches() {
    // Collect only flags missing pricing when cost_usd is none AND no table row.
    // grok-4.5-build resolves via the stripped candidate.
    let ev = grok_event("grok-4.5-build", 10, 1, 0, 0, None);
    assert!(!event_missing_pricing(&ev));
}

fn detect_row(agent: AgentId, installed: bool) -> DetectResult {
    DetectResult {
        agent,
        status: if installed {
            DetectStatus::Installed
        } else {
            DetectStatus::NotFound
        },
        version: None,
        binary_path: None,
        channel: None,
        env_ready: installed,
        notes: Vec::new(),
    }
}

#[test]
fn visible_installed_agent_ids_drops_hidden_and_uninstalled() {
    let detect = vec![
        detect_row(AgentId::Claude, false),
        detect_row(AgentId::Codex, true),
        detect_row(AgentId::Grok, true),
        detect_row(AgentId::Pi, true),
    ];
    let ids = visible_installed_agent_ids(&["codex".into(), "pi".into()], &detect);
    assert_eq!(ids, HashSet::from([AgentId::Grok]));
}

#[test]
fn visibility_service_hidden_ids_feed_collect_targets() {
    let dir = tempfile::tempdir().unwrap();
    let vis = AgentVisibilityService::new(dir.path().to_path_buf());
    vis.set_agent_hidden(AgentId::Claude, true).unwrap();
    let hidden = vis.list_hidden_agents().unwrap();
    let detect = vec![
        detect_row(AgentId::Claude, true),
        detect_row(AgentId::Grok, true),
        detect_row(AgentId::Codex, false),
    ];
    let ids = visible_installed_agent_ids(&hidden, &detect);
    assert_eq!(ids, HashSet::from([AgentId::Grok]));
}
