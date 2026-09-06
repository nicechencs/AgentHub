use super::*;
use tempfile::tempdir;

fn previous_database(path: &Path) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA wal_autocheckpoint=0;
         CREATE TABLE schema_migrations(version TEXT PRIMARY KEY);
         INSERT INTO schema_migrations VALUES ('0002_chat');
         CREATE TABLE messages(body TEXT);
         INSERT INTO messages VALUES ('中文历史');",
    )
    .unwrap();
    conn
}

#[test]
fn backup_includes_committed_wal_and_restores_independently() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("中文 会话.sqlite");
    let conn = previous_database(&path);
    conn.execute("INSERT INTO messages VALUES ('latest WAL message')", [])
        .unwrap();
    let backup = before_upgrade(&conn, &path).unwrap().unwrap();
    let restored = Connection::open(&backup).unwrap();
    let rows: i64 = restored
        .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 2);
    let integrity: String = restored
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
    conn.execute("DELETE FROM messages", []).unwrap();
    assert_eq!(
        restored
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn new_and_already_upgraded_databases_do_not_create_backups() {
    let dir = tempdir().unwrap();
    let empty = dir.path().join("empty.sqlite");
    let conn = Connection::open(&empty).unwrap();
    assert!(before_upgrade(&conn, &empty).unwrap().is_none());
    let old = dir.path().join("upgraded.sqlite");
    let conn = previous_database(&old);
    conn.execute(
        "INSERT INTO schema_migrations VALUES ('00031_chat_runtime')",
        [],
    )
    .unwrap();
    assert!(before_upgrade(&conn, &old).unwrap().is_none());
}

#[test]
fn failed_backup_leaves_source_and_prior_backups_untouched() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("chat.sqlite");
    let conn = previous_database(&path);
    let prior = before_upgrade(&conn, &path).unwrap().unwrap();
    let original = std::fs::read(&prior).unwrap();
    // SQLite forbids VACUUM from inside a transaction. This deterministically
    // exercises cleanup of a reserved output after a database-side failure.
    conn.execute_batch("BEGIN").unwrap();
    assert!(before_upgrade(&conn, &path).is_err());
    conn.execute_batch("ROLLBACK").unwrap();
    assert_eq!(std::fs::read(&prior).unwrap(), original);
    let count = std::fs::read_dir(dir.path())
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("before-chat-runtime")
        })
        .count();
    assert_eq!(count, 1);
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn public_open_backs_up_before_marking_migration_and_does_not_repeat_after_success() {
    use crate::storage::Database;
    let dir = tempdir().unwrap();
    let path = dir.path().join("chat.sqlite");
    let db = Database::open(&path).unwrap();
    db.with_conn(|conn| {
        conn.execute_batch(
            "DELETE FROM schema_migrations WHERE version = '00031_chat_runtime';
            CREATE TABLE backup_probe(body TEXT);
            INSERT INTO backup_probe VALUES ('committed before upgrade');",
        )?;
        Ok(())
    })
    .unwrap();
    // Leave the first connection open so the backup must include its WAL.
    let upgraded = Database::open(&path).unwrap();
    upgraded
        .with_conn(|conn| {
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*) FROM schema_migrations WHERE version = '00031_chat_runtime'",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                1
            );
            Ok(())
        })
        .unwrap();
    drop(upgraded);
    let reopened = Database::open(&path).unwrap();
    drop(reopened);
    let backups: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .contains("before-chat-runtime")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    let restored = Connection::open(&backups[0]).unwrap();
    assert_eq!(
        restored
            .query_row("SELECT body FROM backup_probe", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "committed before upgrade"
    );
    assert_eq!(
        restored
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = '00031_chat_runtime'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[cfg(unix)]
#[test]
fn public_open_backup_path_failure_does_not_apply_migration() {
    use crate::storage::Database;
    use std::os::unix::fs::PermissionsExt;
    // Root bypasses directory permissions, so this failure injection is not
    // applicable when a CI runner deliberately executes as root.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let dir = tempdir().unwrap();
    let path = dir.path().join("chat.sqlite");
    let conn = previous_database(&path);
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let opened = Database::open(&path);
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        opened.is_err(),
        "backup creation should fail without directory write access"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = '00031_chat_runtime'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='chat_runtime'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
