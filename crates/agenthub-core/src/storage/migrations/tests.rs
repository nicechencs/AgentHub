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
