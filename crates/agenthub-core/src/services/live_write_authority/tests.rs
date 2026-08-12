use super::*;
use crate::storage::Database;

#[test]
fn file_database_lock_dir_is_sibling_locks_folder() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("live-write.db");
    let db = Database::open(&db_path).unwrap();
    let authority = LiveWriteAuthority::try_from_database(&db).unwrap();
    assert_eq!(authority.lock_dir(), dir.path().join("locks"));
    assert_eq!(authority.data_root(), dir.path());
}

#[test]
fn from_database_matches_try_from_database_for_file_db() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("live-write.db")).unwrap();
    let a = LiveWriteAuthority::from_database(&db);
    let b = LiveWriteAuthority::try_from_database(&db).unwrap();
    assert_eq!(a.lock_dir(), b.lock_dir());
}
