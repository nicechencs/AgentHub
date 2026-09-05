//! Gateway usage repo unit tests (separate from the production module).

use super::*;
use crate::models::GatewayUsageQuery;

fn row(request_id: &str, ts: &str, latency_ms: i64) -> GatewayUsageRow {
    GatewayUsageRow {
        request_id: request_id.to_owned(),
        ts: ts.to_owned(),
        profile_id: "profile-a".to_owned(),
        surface: "responses".to_owned(),
        upstream_channel: Some("openai_chat".to_owned()),
        ticket_id: Some("account:conn".to_owned()),
        account_source_kind: Some("account".to_owned()),
        account_source_id: Some("conn".to_owned()),
        model: Some("test".to_owned()),
        upstream_model: Some("kimi-test".to_owned()),
        input_tokens: 7,
        output_tokens: 3,
        cached_input_tokens: Some(2),
        reasoning_tokens: Some(1),
        status: if request_id == "req-failed" {
            "failed".to_owned()
        } else {
            "ok".to_owned()
        },
        status_code: Some(200),
        error_class: None,
        latency_ms: Some(latency_ms),
        ttft_ms: Some(4),
        attempts: Some(1),
        session_id: Some("sess".to_owned()),
    }
}

fn open_db() -> (tempfile::TempDir, GatewayUsageRepo) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(&dir.path().join("gateway.db")).expect("open db");
    (dir, GatewayUsageRepo::new(db))
}

#[test]
fn insert_query_and_replay_are_idempotent() {
    let (_dir, repo) = open_db();
    let rows = vec![row("req-1", "2026-08-30T10:00:00+00:00", 120)];

    let inserted = repo.insert_batch(&rows).expect("insert batch");
    assert_eq!(inserted, 1);

    // Replay the same request_id: the PK conflict must be ignored, not doubled.
    let replayed = repo.insert_batch(&rows).expect("replay batch");
    assert_eq!(replayed, 0);
    let replayed_mixed = repo
        .insert_batch(&[
            rows[0].clone(),
            row("req-2", "2026-08-30T10:00:01+00:00", 60),
        ])
        .expect("mixed replay batch");
    assert_eq!(replayed_mixed, 1);

    let stored = repo
        .query(&GatewayUsageQuery::default())
        .expect("query all");
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].request_id, "req-2", "newest ts first");
    assert_eq!(stored[1].input_tokens, 7);
    assert_eq!(stored[1].cached_input_tokens, Some(2));
}

#[test]
fn query_filters_by_profile_and_time_range() {
    let (_dir, repo) = open_db();
    let mut other = row("req-2", "2026-08-30T10:00:00+00:00", 30);
    other.profile_id = "profile-b".to_owned();
    repo.insert_batch(&[
        row("req-1", "2026-08-30T10:00:00+00:00", 10),
        other,
        row("req-3", "2026-09-01T10:00:00+00:00", 20),
    ])
    .expect("insert");

    let profile_rows = repo
        .query(&GatewayUsageQuery {
            profile_id: Some("profile-a".to_owned()),
            ..GatewayUsageQuery::default()
        })
        .expect("query profile");
    assert_eq!(
        profile_rows
            .iter()
            .map(|r| r.request_id.as_str())
            .collect::<Vec<_>>(),
        vec!["req-3", "req-1"]
    );

    let window = repo
        .query(&GatewayUsageQuery {
            since: Some("2026-08-31T00:00:00+00:00".to_owned()),
            until: Some("2026-09-01T23:59:59+00:00".to_owned()),
            ..GatewayUsageQuery::default()
        })
        .expect("query window");
    assert_eq!(window.len(), 1);
    assert_eq!(window[0].request_id, "req-3");
}

#[test]
fn overview_sums_tokens_and_computes_nearest_rank_p95() {
    let (_dir, repo) = open_db();
    repo.insert_batch(&[
        row("req-1", "2026-08-30T10:00:00+00:00", 100),
        row("req-2", "2026-08-30T10:00:01+00:00", 200),
        row("req-3", "2026-08-30T10:00:02+00:00", 300),
        {
            let mut failed = row("req-failed", "2026-08-30T10:00:03+00:00", 50);
            failed.status = "failed".to_owned();
            failed.error_class = Some("stream_error".to_owned());
            failed.ttft_ms = None;
            failed
        },
    ])
    .expect("insert");

    let overview = repo
        .overview(&GatewayUsageQuery::default())
        .expect("overview");
    assert_eq!(overview.request_count, 4);
    assert_eq!(overview.ok_count, 3);
    assert_eq!(overview.failed_count, 1);
    assert_eq!(overview.input_tokens, 28);
    assert_eq!(overview.output_tokens, 12);
    assert_eq!(overview.cached_input_tokens, 8);
    assert_eq!(overview.reasoning_tokens, 4);
    // AVG over all rows including the failed one.
    assert_eq!(overview.avg_latency_ms, Some(162.5));
    // Nearest-rank p95 over 4 sorted samples: ceil(0.95 * 4) = 4th → 300.
    assert_eq!(overview.p95_latency_ms, Some(300));
    assert_eq!(overview.avg_ttft_ms, Some(4.0));

    // Empty window: aggregates are zero and percentiles are None.
    let empty = repo
        .overview(&GatewayUsageQuery {
            profile_id: Some("missing".to_owned()),
            ..GatewayUsageQuery::default()
        })
        .expect("empty overview");
    assert_eq!(empty.request_count, 0);
    assert_eq!(empty.p95_latency_ms, None);
    assert_eq!(empty.avg_latency_ms, None);
}

#[test]
fn cursor_roundtrip_and_removal() {
    let (_dir, repo) = open_db();
    let cursor = GatewaySpoolCursor {
        path: "/tmp/spool/gateway-20260830.jsonl".to_owned(),
        byte_offset: 128,
        file_mtime: 1_786_492_800,
        file_size: 128,
    };

    let inserted = repo
        .insert_batch_and_cursor(
            &[row("req-1", "2026-08-30T10:00:00+00:00", 10)],
            &cursor,
            false,
        )
        .expect("insert with cursor");
    assert_eq!(inserted, 1);
    let stored = repo.get_spool_cursor(&cursor.path).expect("get cursor");
    assert_eq!(stored.as_ref().map(|c| c.byte_offset), Some(128));
    assert_eq!(stored.as_ref().map(|c| c.file_mtime), Some(1_786_492_800));
    assert_eq!(stored.as_ref().map(|c| c.file_size), Some(128));

    // A zero-row advance still moves the cursor (e.g. all lines malformed).
    let advanced = GatewaySpoolCursor {
        byte_offset: 256,
        ..cursor.clone()
    };
    let inserted = repo
        .insert_batch_and_cursor(&[], &advanced, false)
        .expect("advance cursor");
    assert_eq!(inserted, 0);
    assert_eq!(
        repo.get_spool_cursor(&cursor.path)
            .expect("get advanced cursor")
            .expect("cursor present")
            .byte_offset,
        256
    );

    // Removal drops the row (used after the spool file is deleted).
    repo.insert_batch_and_cursor(&[], &advanced, true)
        .expect("remove cursor");
    assert!(repo
        .get_spool_cursor(&cursor.path)
        .expect("get removed cursor")
        .is_none());
}
