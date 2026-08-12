use super::*;
use crate::storage::Database;
use crate::utils::test_temp::real_tempdir;

#[test]
fn file_database_lock_dir_is_sibling_locks_folder() {
    // SQLite reports the real absolute path; on macOS that resolves through
    // /var -> /private/var. real_tempdir keeps the fixture root link-free and
    // already canonical so sibling lock_dir assertions stay stable.
    let dir = real_tempdir();
    let db_path = dir.path().join("live-write.db");
    let db = Database::open(&db_path).unwrap();
    let authority = LiveWriteAuthority::try_from_database(&db).unwrap();
    let expected_root = std::fs::canonicalize(dir.path()).unwrap_or_else(|_| dir.path().to_path_buf());
    assert_eq!(authority.lock_dir(), expected_root.join("locks"));
    assert_eq!(authority.data_root(), expected_root.as_path());
}

#[test]
fn from_database_matches_try_from_database_for_file_db() {
    let dir = real_tempdir();
    let db = Database::open(&dir.path().join("live-write.db")).unwrap();
    let a = LiveWriteAuthority::from_database(&db);
    let b = LiveWriteAuthority::try_from_database(&db).unwrap();
    assert_eq!(a.lock_dir(), b.lock_dir());
}

#[test]
fn lock_dir_matches_sqlite_reported_parent_even_when_open_path_has_symlink_prefix() {
    // tempfile defaults to /var/folders on macOS while SQLite's database_list
    // returns the resolved /private/var path. Authority must follow SQLite so
    // all composers share one locks/ sibling of the real data root.
    let dir = tempfile::tempdir().unwrap();
    let open_path = dir.path().join("live-write.db");
    let db = Database::open(&open_path).unwrap();
    let authority = LiveWriteAuthority::try_from_database(&db).unwrap();

    let sqlite_path = db
        .with_conn(|conn| {
            conn.query_row(
                "SELECT file FROM pragma_database_list WHERE name = 'main'",
                [],
                |row| row.get::<_, String>(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    let sqlite_parent = std::path::Path::new(&sqlite_path)
        .parent()
        .expect("sqlite file has a parent");
    assert_eq!(authority.lock_dir(), sqlite_parent.join("locks"));
    assert_eq!(authority.data_root(), sqlite_parent);
}
