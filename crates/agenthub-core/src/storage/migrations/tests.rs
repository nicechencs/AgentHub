use super::*;
use crate::storage::Database;
use std::sync::Arc;
use std::thread;

fn prepare_migration_table(conn: &Connection) {
    conn.execute_batch(
        r#"
        CREATE TABLE schema_migrations (
            version TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .unwrap();
}

fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [table],
        |row| row.get(0),
    )
    .unwrap()
}

fn migration_exists(conn: &Connection, version: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
        [version],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn failing_migration_script_rolls_back_its_schema_and_marker() {
    let conn = Connection::open_in_memory().unwrap();
    prepare_migration_table(&conn);

    assert!(apply_migration(
        &conn,
        "test_failing_script",
        "CREATE TABLE migration_probe (id INTEGER); INSERT INTO no_such_table VALUES (1);",
    )
    .is_err());

    assert!(!table_exists(&conn, "migration_probe"));
    assert!(!migration_exists(&conn, "test_failing_script"));
}

#[test]
fn marker_insert_failure_rolls_back_the_migration_schema() {
    let conn = Connection::open_in_memory().unwrap();
    prepare_migration_table(&conn);
    conn.execute(
        "INSERT INTO schema_migrations (version) VALUES (?1)",
        ["test_duplicate_marker"],
    )
    .unwrap();

    assert!(apply_migration(
        &conn,
        "test_duplicate_marker",
        "CREATE TABLE migration_marker_probe (id INTEGER);",
    )
    .is_err());

    assert!(!table_exists(&conn, "migration_marker_probe"));
    assert!(migration_exists(&conn, "test_duplicate_marker"));
}

#[test]
fn migration_batch_failure_rolls_back_schema_and_all_markers() {
    let conn = Connection::open_in_memory().unwrap();
    let migrations = [
        (
            "test_batch_first",
            "CREATE TABLE migration_batch_probe (id INTEGER);",
        ),
        (
            "test_batch_failing",
            "CREATE TABLE migration_batch_second (id INTEGER); INSERT INTO no_such_table VALUES (1);",
        ),
    ];

    assert!(run_once(&conn, &migrations).is_err());
    assert!(!table_exists(&conn, "schema_migrations"));
    assert!(!table_exists(&conn, "migration_batch_probe"));
    assert!(!table_exists(&conn, "migration_batch_second"));
}

#[test]
fn concurrent_database_open_serializes_migrations() {
    let dir = tempfile::tempdir().unwrap();
    let path = Arc::new(dir.path().join("concurrent-open.db"));
    let handles = (0..4)
        .map(|_| {
            let path = Arc::clone(&path);
            thread::spawn(move || Database::open(path.as_ref()).is_ok())
        })
        .collect::<Vec<_>>();

    for handle in handles {
        assert!(handle.join().unwrap());
    }

    let conn = Connection::open(path.as_ref()).unwrap();
    for (version, _) in MIGRATIONS {
        assert!(migration_exists(&conn, version));
    }
}

fn create_0001_usage_records(conn: &Connection) {
    conn.execute_batch(
        r#"
        CREATE TABLE usage_records (
            id          TEXT PRIMARY KEY,
            agent_id    TEXT NOT NULL,
            account_id  TEXT,
            model       TEXT,
            input_tokens  INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            cache_tokens  INTEGER NOT NULL DEFAULT 0,
            cost_cny    REAL,
            session_id  TEXT,
            ts          TEXT NOT NULL,
            raw_hash    TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .unwrap();
}

fn apply_00021(conn: &Connection) {
    let sql = MIGRATIONS
        .iter()
        .find(|(version, _)| *version == "00021_usage_dedup_nulls")
        .expect("00021_usage_dedup_nulls is registered")
        .1;
    apply_migration(conn, "00021_usage_dedup_nulls", sql).unwrap();
}

fn usage_row_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM usage_records", [], |row| row.get(0))
        .unwrap()
}

fn usage_raw_hash(conn: &Connection, id: &str) -> String {
    conn.query_row(
        "SELECT raw_hash FROM usage_records WHERE id = ?1",
        [id],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn usage_dedup_00021_keeps_unidentified_orphan_rows() {
    let conn = Connection::open_in_memory().unwrap();
    prepare_migration_table(&conn);
    create_0001_usage_records(&conn);

    conn.execute(
        "INSERT INTO usage_records (id, agent_id, input_tokens, ts, session_id, raw_hash)
         VALUES ('orphan-a', 'codex', 10, '2026-01-01T00:00:00Z', NULL, NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO usage_records (id, agent_id, input_tokens, ts, session_id, raw_hash)
         VALUES ('orphan-b', 'codex', 20, '2026-01-01T00:00:00Z', NULL, NULL)",
        [],
    )
    .unwrap();

    apply_00021(&conn);

    assert_eq!(usage_row_count(&conn), 2);
    assert_eq!(usage_raw_hash(&conn, "orphan-a"), "orphan:orphan-a");
    assert_eq!(usage_raw_hash(&conn, "orphan-b"), "orphan:orphan-b");
}

#[test]
fn usage_dedup_00021_collapses_true_session_hash_duplicates() {
    let conn = Connection::open_in_memory().unwrap();
    prepare_migration_table(&conn);
    create_0001_usage_records(&conn);

    conn.execute(
        "INSERT INTO usage_records (id, agent_id, input_tokens, ts, session_id, raw_hash)
         VALUES ('dup-a', 'codex', 30, '2026-01-01T00:00:00Z', 'sess-1', 'hash-1')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO usage_records (id, agent_id, input_tokens, ts, session_id, raw_hash)
         VALUES ('dup-b', 'codex', 40, '2026-01-01T00:00:00Z', 'sess-1', 'hash-1')",
        [],
    )
    .unwrap();

    apply_00021(&conn);

    assert_eq!(usage_row_count(&conn), 1);
    let remaining: (String, String) = conn
        .query_row(
            "SELECT session_id, raw_hash FROM usage_records",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(remaining, ("sess-1".into(), "hash-1".into()));
}
