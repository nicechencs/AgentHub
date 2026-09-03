//! Spool writer unit tests (separate from the production module).

use std::fs;

use serde_json::Value;

use super::*;

fn event(request_id: &str, ts: &str) -> GatewayUsageEvent {
    GatewayUsageEvent {
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
        reasoning_tokens: Some(0),
        status: "ok".to_owned(),
        status_code: Some(200),
        error_class: None,
        latency_ms: Some(12),
        ttft_ms: Some(4),
        attempts: Some(1),
        session_id: Some("sess".to_owned()),
    }
}

fn spool_lines(dir: &std::path::Path, day: &str) -> Vec<Value> {
    let raw = fs::read_to_string(dir.join(format!("gateway-{day}.jsonl"))).expect("spool file");
    raw.lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

#[test]
fn spool_writes_one_json_object_per_line_and_flushes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let spool = UsageSpool::new(dir.path().to_path_buf());

    spool.record(&event("req-1", "2026-08-30T10:00:00+00:00"));
    spool.record(&event("req-2", "2026-08-30T10:00:01+00:00"));

    let lines = spool_lines(dir.path(), "20260830");
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["request_id"], "req-1");
    assert_eq!(lines[1]["request_id"], "req-2");
    assert_eq!(lines[0]["input_tokens"], 7);
    assert_eq!(lines[0]["cached_input_tokens"], 2);
    assert_eq!(lines[0]["status"], "ok");
    // Round-trip: the collector parses the same shape back.
    let parsed: GatewayUsageEvent =
        serde_json::from_value(lines[0].clone()).expect("event round-trip");
    assert_eq!(parsed, event("req-1", "2026-08-30T10:00:00+00:00"));
}

#[test]
fn spool_rotates_per_day_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let spool = UsageSpool::new(dir.path().to_path_buf());

    spool.record(&event("req-1", "2026-08-30T23:59:59+00:00"));
    spool.record(&event("req-2", "2026-08-31T00:00:00+00:00"));
    spool.record(&event("req-3", "2026-08-31T00:00:01+00:00"));

    assert_eq!(spool_lines(dir.path(), "20260830").len(), 1);
    let late = spool_lines(dir.path(), "20260831");
    assert_eq!(late.len(), 2);
    assert_eq!(late[0]["request_id"], "req-2");
}

#[test]
fn spool_write_failure_is_swallowed_and_recovery_reopens_the_file() {
    // A file where the spool directory should be makes create_dir_all fail.
    let dir = tempfile::tempdir().expect("tempdir");
    let blocker = dir.path().join("blocker");
    fs::write(&blocker, b"not a dir").expect("blocker file");
    let spool = UsageSpool::new(blocker.join("spool"));

    // Must not panic; the failure is only logged.
    spool.record(&event("req-bad", "2026-08-30T10:00:00+00:00"));

    // Once the directory becomes creatable, later records succeed again.
    fs::remove_file(&blocker).expect("remove blocker");
    spool.record(&event("req-ok", "2026-08-30T10:00:01+00:00"));
    assert_eq!(spool_lines(&blocker.join("spool"), "20260830").len(), 1);
}

#[test]
fn spool_slot_is_set_once_and_defaults_to_noop() {
    let slot = UsageSpoolSlot::default();
    assert!(slot.get().is_none());

    let dir = tempfile::tempdir().expect("tempdir");
    assert!(slot.set(Arc::new(UsageSpool::new(dir.path().to_path_buf()))));
    assert!(!slot.set(Arc::new(UsageSpool::new(dir.path().to_path_buf()))));
    assert!(slot.get().is_some());

    // Unset slots never reach a spool: emit is a plain no-op.
    emit(
        &UsageSpoolSlot::default(),
        event("req-none", "2026-08-30T10:00:00+00:00"),
    );
}
