use crate::storage::{Database, LocalEntryKey, LocalEntryKeyRepo};

fn tmp() -> (tempfile::TempDir, LocalEntryKeyRepo) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("entry-keys.db")).unwrap();
    (dir, LocalEntryKeyRepo::new(db))
}

fn row(id: &str, pool_id: &str, name: &str, token: &str) -> LocalEntryKey {
    LocalEntryKey {
        id: id.into(),
        pool_id: pool_id.into(),
        name: name.into(),
        token: token.into(),
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

#[test]
fn migration_marker_exists() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("entry-keys.db")).unwrap();
    db.with_conn(|conn| {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = '00029_local_entry_keys'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn inserts_lists_updates_and_deletes_named_keys() {
    let (_dir, repo) = tmp();
    repo.insert(&row("k1", "pool-a", "工作电脑", "ahb_one"))
        .unwrap();
    repo.insert(&row("k2", "pool-a", "默认名", ""))
        .unwrap();
    let listed = repo.list().unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].name, "工作电脑");
    let updated = repo
        .update(&LocalEntryKey {
            name: "家里".into(),
            updated_at: "t1".into(),
            ..listed[0].clone()
        })
        .unwrap();
    assert_eq!(updated.name, "家里");
    repo.delete("k1").unwrap();
    assert_eq!(repo.list().unwrap().len(), 1);
    assert!(repo.get("k1").unwrap().is_none());
}

#[test]
fn extra_tokens_are_unique_empty_names_are_not() {
    let (_dir, repo) = tmp();
    repo.insert(&row("n1", "pool-a", "A", "")).unwrap();
    repo.insert(&row("n2", "pool-b", "B", "")).unwrap();
    repo.insert(&row("k1", "pool-a", "one", "ahb_same"))
        .unwrap();
    let error = repo
        .insert(&row("k2", "pool-b", "two", "ahb_same"))
        .unwrap_err();
    assert_eq!(error.code(), "invalid_arg");
}
