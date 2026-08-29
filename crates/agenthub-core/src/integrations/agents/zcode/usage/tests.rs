use std::path::Path;

use rusqlite::Connection;

use super::collect_zcode_usage;

fn seed_model_usage(home: &Path, rows: &[(&str, &str, &str, i64, i64, i64, i64, i64, &str)]) {
    let dir = home.join("cli").join("db");
    std::fs::create_dir_all(&dir).unwrap();
    let conn = Connection::open(dir.join("db.sqlite")).unwrap();
    conn.execute_batch(
        "CREATE TABLE model_usage (
            id TEXT,
            session_id TEXT,
            model_id TEXT,
            input_tokens INTEGER,
            output_tokens INTEGER,
            cache_creation_input_tokens INTEGER,
            cache_read_input_tokens INTEGER,
            started_at INTEGER,
            completed_at INTEGER,
            status TEXT
        );",
    )
    .unwrap();
    for (id, sess, model, input, output, cache_c, cache_r, ts, status) in rows {
        conn.execute(
            "INSERT INTO model_usage (
                id, session_id, model_id, input_tokens, output_tokens,
                cache_creation_input_tokens, cache_read_input_tokens,
                started_at, completed_at, status
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9)",
            rusqlite::params![id, sess, model, input, output, cache_c, cache_r, ts, status],
        )
        .unwrap();
    }
}

#[test]
fn harvests_completed_rows_and_peels_cache_from_input() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    seed_model_usage(
        home,
        &[
            (
                "u1",
                "sess_a",
                "GLM-5.3",
                29429,
                28,
                0,
                26432,
                1_787_916_973_108,
                "completed",
            ),
            (
                "u2",
                "sess_a",
                "GLM-5.3",
                10,
                2,
                0,
                0,
                1_787_916_973_200,
                "cancelled",
            ),
            (
                "u3",
                "sess_b",
                "GLM-5.3-Flash",
                0,
                0,
                0,
                0,
                1_787_916_973_300,
                "completed",
            ),
        ],
    );

    let events = collect_zcode_usage(home);
    assert_eq!(events.len(), 1);
    let ev = &events[0];
    assert_eq!(ev.model, "GLM-5.3");
    assert_eq!(ev.input_tokens, 29429 - 26432);
    assert_eq!(ev.cache_read_tokens, 26432);
    assert_eq!(ev.output_tokens, 28);
    assert_eq!(ev.session_id.as_deref(), Some("sess_a"));
    assert_eq!(ev.raw_hash, "zcode:u1");
    assert!(ev.ts.starts_with("2026-"));
}

#[test]
fn missing_db_is_empty() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(collect_zcode_usage(tmp.path()).is_empty());
}

#[test]
fn collect_pipeline_harvests_registered_source() {
    use crate::models::AgentId;
    use crate::platform::usage::collect_for_agent_id;
    use crate::storage::{Database, UsageRepo};
    use crate::utils::test_env::{lock_test_env, EnvVarGuard};

    let _lock = lock_test_env();
    let tmp = tempfile::tempdir().unwrap();
    seed_model_usage(
        tmp.path(),
        &[(
            "u1",
            "sess_a",
            "GLM-5.3",
            100,
            20,
            0,
            10,
            1_787_916_973_108,
            "completed",
        )],
    );
    let _home = EnvVarGuard::set("ZCODE_HOME", tmp.path());
    let dbdir = tempfile::tempdir().unwrap();
    let db = Database::open(&dbdir.path().join("u.db")).unwrap();
    let repo = UsageRepo::new(db);
    let stats = collect_for_agent_id(AgentId::Zcode, &repo).unwrap();
    assert_eq!(stats.events.len(), 1);
    assert_eq!(stats.events[0].input_tokens, 90);
    assert_eq!(stats.events[0].cache_read_tokens, 10);
}
