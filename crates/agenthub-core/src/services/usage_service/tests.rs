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
        cache_creation_1h_tokens: 0,
        cache_read_tokens: cache_read,
        session_id: Some("sess".into()),
        ts: "2025-06-15T00:00:00+00:00".into(),
        raw_hash: "h".into(),
        cost_usd,
        fast: false,
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
    let ev = grok_event("grok-4.5-build", 100_000, 0, 0, 0, None);
    let cost = cost_for_event(&ev);
    // grok-4.5 input is $2 / 1M; 100k stays below the 200k long-context tier.
    assert!((cost - 0.2).abs() < 0.01, "got {cost}");
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

fn codex_event(model: &str, ts: &str) -> ParsedUsageEvent {
    ParsedUsageEvent {
        agent_id: AgentId::Codex,
        model: model.into(),
        input_tokens: 100_000,
        output_tokens: 0,
        cache_creation_tokens: 0,
        cache_creation_1h_tokens: 0,
        cache_read_tokens: 0,
        session_id: Some("sess".into()),
        ts: ts.into(),
        raw_hash: "h".into(),
        cost_usd: None,
        fast: false,
    }
}

#[test]
fn auto_review_uses_published_backend_and_is_not_missing_pricing() {
    let luna = codex_event("gpt-5.6-luna", "2026-08-26T00:00:00Z");
    let review = codex_event("codex-auto-review", "2026-08-26T00:00:00Z");
    let old = codex_event("codex-auto-review", "2026-07-01T00:00:00Z");
    assert!(!event_missing_pricing(&review));
    assert!(!event_missing_pricing(&old));
    let luna_cost = cost_for_event(&luna);
    assert!(
        (cost_for_event(&review) - luna_cost).abs() < 1e-12,
        "current auto-review must bill as luna"
    );
    assert!(
        (cost_for_event(&old) - 0.25).abs() < 1e-9,
        "pre-luna auto-review bills as gpt-5.4"
    );
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
        extra_copies: Vec::new(),
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

#[test]
fn collect_ingests_gateway_spool_rows_into_the_separate_table() {
    use crate::bridge::usage_capture::GatewayUsageEvent;
    use crate::models::GatewayUsageQuery;

    let spool_event = |request_id: &str, input: u64| GatewayUsageEvent {
        request_id: request_id.to_owned(),
        ts: "2026-08-30T10:00:00+00:00".to_owned(),
        profile_id: "profile-a".to_owned(),
        surface: "responses".to_owned(),
        upstream_channel: Some("openai_chat".to_owned()),
        ticket_id: Some("account:conn".to_owned()),
        account_source_kind: Some("account".to_owned()),
        account_source_id: Some("conn".to_owned()),
        model: Some("test".to_owned()),
        upstream_model: None,
        input_tokens: input,
        output_tokens: 3,
        cached_input_tokens: None,
        reasoning_tokens: None,
        status: "ok".to_owned(),
        status_code: Some(200),
        error_class: None,
        latency_ms: Some(12),
        ttft_ms: None,
        attempts: Some(1),
        session_id: None,
    };

    let root = tempfile::tempdir().unwrap();
    let spool = tempfile::tempdir().unwrap();
    let mut fixture = String::new();
    fixture.push_str(&serde_json::to_string(&spool_event("req-1", 7)).unwrap());
    fixture.push('\n');
    fixture.push_str("{not json\n");
    fixture.push_str(&serde_json::to_string(&spool_event("req-2", 9)).unwrap());
    fixture.push('\n');
    std::fs::write(spool.path().join("gateway-20260830.jsonl"), fixture).unwrap();

    let db = crate::storage::Database::open(&root.path().join("usage.db")).unwrap();
    let service = crate::services::UsageService::with_registry(
        db,
        crate::platform::usage::UsageSourceRegistry::new(),
    )
    .with_gateway_spool_dir(spool.path().to_path_buf());

    // collect() drains the spool; gateway counts stay out of CollectResult.
    let result = service.collect(None).unwrap();
    assert_eq!(result.inserted, 0);

    let rows = service
        .gateway_usage_query(GatewayUsageQuery::default())
        .unwrap();
    assert_eq!(rows.len(), 2, "malformed line skipped, both events stored");
    assert_eq!(rows[0].request_id, "req-2");
    assert_eq!(rows[0].input_tokens, 9);

    // Replay is idempotent: a second collect must not duplicate rows.
    service.collect(None).unwrap();
    assert_eq!(
        service
            .gateway_usage_query(GatewayUsageQuery::default())
            .unwrap()
            .len(),
        2
    );

    // Overview aggregates the ingested window.
    let overview = service
        .gateway_usage_overview(GatewayUsageQuery::default())
        .unwrap();
    assert_eq!(overview.request_count, 2);
    assert_eq!(overview.ok_count, 2);
    assert_eq!(overview.input_tokens, 16);
}

#[test]
fn gateway_usage_query_ingests_spool_without_a_prior_collect() {
    use crate::bridge::usage_capture::GatewayUsageEvent;
    use crate::models::GatewayUsageQuery;

    let spool_event = |request_id: &str, input: u64| GatewayUsageEvent {
        request_id: request_id.to_owned(),
        ts: "2026-08-30T10:00:00+00:00".to_owned(),
        profile_id: "profile-a".to_owned(),
        surface: "responses".to_owned(),
        upstream_channel: None,
        ticket_id: None,
        account_source_kind: None,
        account_source_id: None,
        model: Some("test".to_owned()),
        upstream_model: None,
        input_tokens: input,
        output_tokens: 1,
        cached_input_tokens: None,
        reasoning_tokens: None,
        status: "ok".to_owned(),
        status_code: Some(200),
        error_class: None,
        latency_ms: Some(8),
        ttft_ms: None,
        attempts: Some(1),
        session_id: None,
    };

    let root = tempfile::tempdir().unwrap();
    let spool = tempfile::tempdir().unwrap();
    std::fs::write(
        spool.path().join("gateway-20260830.jsonl"),
        format!(
            "{}\n",
            serde_json::to_string(&spool_event("req-board", 4)).unwrap()
        ),
    )
    .unwrap();

    let db = crate::storage::Database::open(&root.path().join("usage.db")).unwrap();
    let service = crate::services::UsageService::with_registry(
        db,
        crate::platform::usage::UsageSourceRegistry::new(),
    )
    .with_gateway_spool_dir(spool.path().to_path_buf());

    let rows = service
        .gateway_usage_query(GatewayUsageQuery::default())
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].request_id, "req-board");
    assert_eq!(rows[0].input_tokens, 4);
}
