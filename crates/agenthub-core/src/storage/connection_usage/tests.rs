use super::*;
use crate::models::{Account, AccountKind, AgentId, GatewayUsageRow};
use crate::storage::{AccountRepo, Database};
use serde_json::json;

fn event(key: &str, ticket: &str, input: i64, output: i64, ts: &str) -> ConnectionUsageEvent {
    ConnectionUsageEvent {
        event_key: key.into(),
        ticket_id: ticket.into(),
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        ts: ts.into(),
    }
}

#[test]
fn records_and_lists_per_ticket_and_dedupes() {
    let dir = tempfile::tempdir().unwrap();
    let store = ConnectionUsageStore::open(dir.path().join("connection_usage.db"));
    store.record(&[
        event("log:a", "account:1", 10, 2, "2026-09-01T00:00:00Z"),
        event("log:b", "account:1", 5, 1, "2026-09-01T01:00:00Z"),
        event("log:a", "account:1", 99, 99, "2026-09-01T02:00:00Z"),
    ]);
    let rows = store.list_summaries();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].ticket_id, "account:1");
    assert_eq!(rows[0].input_tokens, 15);
    assert_eq!(rows[0].output_tokens, 3);
    assert_eq!(
        rows[0].last_used_at.as_deref(),
        Some("2026-09-01T01:00:00Z")
    );
}

#[test]
fn deleted_sidecar_does_not_break_a_new_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("connection_usage.db");
    {
        let store = ConnectionUsageStore::open(path.clone());
        assert!(store.list_summaries().is_empty());
        store.record(&[event("log:a", "account:1", 1, 0, "t0")]);
        assert_eq!(store.list_summaries().len(), 1);
    }
    let _ = std::fs::remove_file(&path);
    let store = ConnectionUsageStore::open(path);
    assert!(store.list_summaries().is_empty());
    store.record(&[event("log:b", "account:2", 4, 1, "t1")]);
    let rows = store.list_summaries();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].ticket_id, "account:2");
    assert_eq!(rows[0].input_tokens, 4);
}

#[test]
fn disabled_store_is_a_no_op() {
    let store = ConnectionUsageStore::disabled();
    store.record(&[event("log:a", "account:1", 1, 0, "t0")]);
    assert!(store.list_summaries().is_empty());
}

#[test]
fn shared_cache_database_records_and_lists() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("cache.db")).unwrap();
    let store = ConnectionUsageStore::from_database(db);
    store.record(&[event("log:a", "account:1", 8, 2, "t0")]);
    let rows = store.list_summaries();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].input_tokens, 8);
    assert_eq!(rows[0].output_tokens, 2);
}

#[test]
fn gateway_ticket_prefers_ticket_id_then_source() {
    let with_ticket = GatewayUsageRow {
        request_id: "r1".into(),
        ts: "t".into(),
        profile_id: "p".into(),
        surface: "responses".into(),
        upstream_channel: None,
        ticket_id: Some("account:abc".into()),
        account_source_kind: Some("account".into()),
        account_source_id: Some("other".into()),
        model: None,
        upstream_model: None,
        input_tokens: 1,
        output_tokens: 0,
        cached_input_tokens: None,
        reasoning_tokens: None,
        status: "ok".into(),
        status_code: Some(200),
        error_class: None,
        latency_ms: None,
        ttft_ms: None,
        attempts: None,
        session_id: None,
    };
    assert_eq!(
        ticket_id_from_gateway(&with_ticket).as_deref(),
        Some("account:abc")
    );
    let from_source = GatewayUsageRow {
        ticket_id: None,
        account_source_kind: Some("provider".into()),
        account_source_id: Some("prov-1".into()),
        ..with_ticket.clone()
    };
    assert_eq!(
        ticket_id_from_gateway(&from_source).as_deref(),
        Some("provider:prov-1")
    );
}

#[test]
fn current_ticket_follows_current_account() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("main.db")).unwrap();
    let repo = AccountRepo::new(db.clone());
    repo.create(&Account {
        id: "acc-1".into(),
        agent_id: AgentId::Codex,
        kind: AccountKind::ApiKey,
        label: "k".into(),
        credentials: json!({"key": "sk-test"}),
        extra: json!({}),
        status: "active".into(),
        is_current: true,
        created_at: "t".into(),
        updated_at: "t".into(),
    })
    .unwrap();
    assert_eq!(
        current_ticket_id_for_agent(&db, AgentId::Codex).as_deref(),
        Some("account:acc-1")
    );
    assert_eq!(current_ticket_id_for_agent(&db, AgentId::Grok), None);
}
