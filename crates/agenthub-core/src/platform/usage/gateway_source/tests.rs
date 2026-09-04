//! Gateway spool ingest unit tests (temp-dir fixtures, cursor replay,
//! malformed-line tolerance).

use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;
use crate::storage::Database;

fn event(request_id: &str, ts: &str, input: u64) -> GatewayUsageEvent {
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
        input_tokens: input,
        output_tokens: 3,
        cached_input_tokens: Some(2),
        reasoning_tokens: None,
        status: "ok".to_owned(),
        status_code: Some(200),
        error_class: None,
        latency_ms: Some(12),
        ttft_ms: Some(4),
        attempts: Some(1),
        session_id: None,
    }
}

fn write_spool(dir: &Path, day: &str, lines: &[String]) -> PathBuf {
    let path = dir.join(format!("gateway-{day}.jsonl"));
    fs::write(&path, lines.join("\n") + "\n").expect("write spool fixture");
    path
}

fn spool_line(event: &GatewayUsageEvent) -> String {
    serde_json::to_string(event).expect("serialize event")
}

fn open_repo() -> (tempfile::TempDir, GatewayUsageRepo) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(&dir.path().join("gateway.db")).expect("open db");
    (dir, GatewayUsageRepo::new(db))
}

fn backdate(path: &Path, days_ago: u64) {
    let past = SystemTime::now() - Duration::from_secs(days_ago * 24 * 60 * 60);
    // std File::set_modified needs a write handle; append preserves content.
    let file = File::options().append(true).open(path).expect("open spool");
    file.set_modified(past).expect("backdate file");
}

#[test]
fn ingest_reads_all_files_oldest_first_and_advances_cursors() {
    let (_root, repo) = open_repo();
    let spool = tempfile::tempdir().expect("spool tempdir");
    write_spool(
        spool.path(),
        "20260830",
        &[spool_line(&event(
            "req-old",
            "2026-08-30T10:00:00+00:00",
            5,
        ))],
    );
    write_spool(
        spool.path(),
        "20260831",
        &[
            spool_line(&event("req-new", "2026-08-31T10:00:00+00:00", 7)),
            spool_line(&event("req-new2", "2026-08-31T10:00:01+00:00", 9)),
        ],
    );

    let outcome = ingest_spool_dir(&repo, spool.path()).expect("ingest");
    assert_eq!(outcome.files, 2);
    assert_eq!(outcome.inserted, 3);
    assert_eq!(outcome.malformed, 0);
    assert_eq!(outcome.deleted_files, 0);

    let rows = repo
        .query(&crate::models::GatewayUsageQuery::default())
        .expect("query rows");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].request_id, "req-new2");

    // Re-ingest is a no-op: cursors are at EOF and rows dedupe by request_id.
    let replay = ingest_spool_dir(&repo, spool.path()).expect("replay ingest");
    assert_eq!(replay.inserted, 0);
    assert_eq!(
        repo.query(&crate::models::GatewayUsageQuery::default())
            .expect("query rows")
            .len(),
        3
    );
}

#[test]
fn cursor_replay_continues_from_a_partially_consumed_file() {
    let (_root, repo) = open_repo();
    let spool = tempfile::tempdir().expect("spool tempdir");
    let lines = vec![
        spool_line(&event("req-1", "2026-08-30T10:00:00+00:00", 5)),
        spool_line(&event("req-2", "2026-08-30T10:00:01+00:00", 7)),
        spool_line(&event("req-3", "2026-08-30T10:00:02+00:00", 9)),
    ];
    let path = write_spool(spool.path(), "20260830", &lines);
    let raw = fs::read_to_string(&path).expect("read fixture");

    // Simulate a crash after the first line was ingested and the cursor saved.
    let first_line_end = raw.find('\n').expect("line break") as i64 + 1;
    let mtime = fs::metadata(&path)
        .expect("meta")
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    repo.insert_batch_and_cursor(
        &[gateway_row(event("req-1", "2026-08-30T10:00:00+00:00", 5))],
        &GatewaySpoolCursor {
            path: path.to_string_lossy().to_string(),
            byte_offset: first_line_end,
            file_mtime: mtime,
        },
        false,
    )
    .expect("seed cursor");

    let outcome = ingest_spool_dir(&repo, spool.path()).expect("ingest rest");
    assert_eq!(outcome.inserted, 2, "only req-2 and req-3 are new");
    let rows = repo
        .query(&crate::models::GatewayUsageQuery::default())
        .expect("query rows");
    assert_eq!(rows.len(), 3, "no duplicate for req-1");
}

#[test]
fn malformed_lines_are_skipped_without_blocking_their_neighbors() {
    let (_root, repo) = open_repo();
    let spool = tempfile::tempdir().expect("spool tempdir");
    write_spool(
        spool.path(),
        "20260830",
        &[
            "{not json".to_owned(),
            spool_line(&event("req-ok", "2026-08-30T10:00:00+00:00", 5)),
            "[]".to_owned(),
            String::new(),
        ],
    );

    let outcome = ingest_spool_dir(&repo, spool.path()).expect("ingest");
    assert_eq!(outcome.inserted, 1);
    assert_eq!(outcome.malformed, 2);
    // The cursor advanced past every line, so a replay inserts nothing.
    let replay = ingest_spool_dir(&repo, spool.path()).expect("replay");
    assert_eq!(replay.inserted, 0);
    assert_eq!(replay.malformed, 0);
}

#[test]
fn fully_ingested_old_files_are_deleted_and_their_cursors_removed() {
    let (_root, repo) = open_repo();
    let spool = tempfile::tempdir().expect("spool tempdir");
    let old_path = write_spool(
        spool.path(),
        "20260801",
        &[spool_line(&event(
            "req-old",
            "2026-08-01T10:00:00+00:00",
            5,
        ))],
    );
    let fresh_path = write_spool(
        spool.path(),
        "20260831",
        &[spool_line(&event(
            "req-new",
            "2026-08-31T10:00:00+00:00",
            7,
        ))],
    );
    backdate(&old_path, SPOOL_RETENTION_DAYS + 1);

    let outcome = ingest_spool_dir(&repo, spool.path()).expect("ingest");
    assert_eq!(outcome.inserted, 2);
    assert_eq!(outcome.deleted_files, 1, "only the expired file is deleted");
    assert!(!old_path.exists(), "expired spool file is removed");
    assert!(fresh_path.exists(), "fresh spool file stays");
    assert!(repo
        .get_spool_cursor(&old_path.to_string_lossy())
        .expect("old cursor gone")
        .is_none());
    assert!(repo
        .get_spool_cursor(&fresh_path.to_string_lossy())
        .expect("fresh cursor")
        .is_some());

    // A stale mid-file cursor (e.g. crash before the last byte) still ends at
    // EOF after the remaining bytes are consumed, so the expired file is
    // deleted and the surviving rows stay deduplicated.
    let partial_path = spool.path().join("gateway-20260802.jsonl");
    fs::write(
        &partial_path,
        spool_line(&event("req-partial", "2026-08-02T10:00:00+00:00", 5)),
    )
    .expect("write partial fixture");
    backdate(&partial_path, SPOOL_RETENTION_DAYS + 1);
    let len = fs::metadata(&partial_path).expect("len").len() as i64;
    let mtime = fs::metadata(&partial_path)
        .expect("meta")
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    repo.insert_batch_and_cursor(
        &[],
        &GatewaySpoolCursor {
            path: partial_path.to_string_lossy().to_string(),
            byte_offset: len - 1,
            file_mtime: mtime,
        },
        false,
    )
    .expect("seed partial cursor");
    let second = ingest_spool_dir(&repo, spool.path()).expect("second ingest");
    assert_eq!(second.inserted, 0, "no new rows from the consumed tail");
    assert_eq!(second.deleted_files, 1);
    assert!(!partial_path.exists());
}

#[test]
fn missing_spool_dir_is_an_empty_outcome() {
    let (_root, repo) = open_repo();
    let missing = tempfile::tempdir().expect("tempdir");
    let dir = missing.path().join("does-not-exist");
    let outcome = ingest_spool_dir(&repo, &dir).expect("empty outcome");
    assert_eq!(outcome, GatewaySpoolOutcome::default());
}
