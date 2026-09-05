use super::*;
use crate::models::{AgentId, UsageQuery, UsageRecord};
use crate::storage::connection_usage::{ConnectionUsageEvent, ConnectionUsageStore};
use crate::storage::{Database, UsageRepo};

fn usage_row(id: &str, hash: &str) -> UsageRecord {
    UsageRecord {
        id: id.into(),
        agent_id: AgentId::Codex,
        account_id: None,
        model: "gpt-5".into(),
        input_tokens: 12,
        output_tokens: 3,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        cost_usd: Some(0.01),
        session_id: Some("s1".into()),
        ts: "2026-09-01T00:00:00Z".into(),
        raw_hash: Some(hash.into()),
        fast: false,
    }
}

fn table_exists(db: &Database, name: &str) -> bool {
    db.with_conn(|conn| {
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |row| row.get(0),
        )?;
        Ok(n > 0)
    })
    .unwrap_or(false)
}

fn remove_sqlite(path: &Path) {
    let _ = fs::remove_file(path);
    for extra in ["-wal", "-shm", "-journal"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(extra);
        let _ = fs::remove_file(PathBuf::from(name));
    }
}

#[test]
fn isolate_moves_usage_out_of_product_db() {
    let dir = tempfile::tempdir().unwrap();
    let main_path = dir.path().join("agenthub.db");
    let cache_path = dir.path().join("cache.db");
    let main = Database::open(&main_path).unwrap();
    UsageRepo::new(main.clone())
        .insert_batch(&[usage_row("u1", "h1")])
        .unwrap();
    main.set_setting("usage_token_layout", "5").unwrap();
    main.set_setting("theme", "dark").unwrap();

    let cache = open_cache(&cache_path);
    isolate_usage_cache(&main, &cache, &main_path, dir.path());

    assert!(!table_exists(&main, "usage_records"));
    assert!(!table_exists(&main, "usage_cursors"));
    assert!(!table_exists(&main, "gateway_usage"));
    assert_eq!(main.get_setting("usage_token_layout").unwrap(), None);
    assert_eq!(main.get_setting("theme").unwrap().as_deref(), Some("dark"));

    let rows = UsageRepo::new(cache.clone())
        .query(&UsageQuery {
            days: 30,
            agent_id: Some(AgentId::Codex),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "u1");
    assert_eq!(
        cache.get_setting("usage_token_layout").unwrap().as_deref(),
        Some("5")
    );
}

#[test]
fn isolate_folds_legacy_connection_usage_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let main_path = dir.path().join("agenthub.db");
    let cache_path = dir.path().join("cache.db");
    let sidecar = dir.path().join("connection_usage.db");
    let main = Database::open(&main_path).unwrap();
    ConnectionUsageStore::open(sidecar.clone()).record(&[ConnectionUsageEvent {
        event_key: "log:a".into(),
        ticket_id: "account:1".into(),
        input_tokens: 4,
        output_tokens: 1,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        ts: "t0".into(),
    }]);

    let cache = open_cache(&cache_path);
    isolate_usage_cache(&main, &cache, &main_path, dir.path());

    let rows = ConnectionUsageStore::from_database(cache).list_summaries();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].ticket_id, "account:1");
    assert_eq!(rows[0].input_tokens, 4);
    assert!(!sidecar.exists());
}

#[test]
fn missing_or_corrupt_cache_file_still_opens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let db = open_cache(&path);
    db.ping().unwrap();
    assert!(table_exists(&db, "usage_records"));
    drop(db);

    std::fs::write(&path, "not sqlite").unwrap();
    let db = open_cache(&path);
    db.ping().unwrap();
    UsageRepo::new(db)
        .insert_batch(&[usage_row("u2", "h2")])
        .unwrap();
}

#[test]
fn deleted_cache_file_reopens_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    {
        let db = open_cache(&path);
        UsageRepo::new(db)
            .insert_batch(&[usage_row("u3", "h3")])
            .unwrap();
    }
    remove_sqlite(&path);
    let db = open_cache(&path);
    let rows = UsageRepo::new(db)
        .query(&UsageQuery {
            days: 30,
            ..Default::default()
        })
        .unwrap();
    assert!(rows.is_empty());
}
