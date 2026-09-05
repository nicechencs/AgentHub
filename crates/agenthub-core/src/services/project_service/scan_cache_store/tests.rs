use super::*;
use crate::models::AgentId;
use rusqlite::{params, Connection};
use tempfile::tempdir;

fn sample_entry() -> IndexEntry {
    IndexEntry {
        mtime_ms: 100,
        size: 10,
        project_key: "cwd/D:/work".into(),
        cwd: Some("D:/work".into()),
        title: "t".into(),
        preview: Some("p".into()),
        message_count: Some(2),
        updated_at: "t0".into(),
        session_id: Some("parent-1".into()),
        parent_session_id: Some("parent-1".into()),
        thread_kind: Some("subagent".into()),
        agent_role: Some("explorer".into()),
    }
}

#[test]
fn put_and_get_fresh_roundtrip() {
    let dir = tempdir().unwrap();
    let mut store = SessionIndexStore::load(dir.path()).expect("open");
    store.put(AgentId::Codex, "sessions/a.jsonl", sample_entry());
    store.save_if_dirty();
    assert!(dir.path().join("scan-cache.db").exists());

    let store = SessionIndexStore::load(dir.path()).expect("reopen");
    let hit = store
        .get_fresh(AgentId::Codex, "sessions/a.jsonl", 10, 100)
        .expect("fresh");
    assert_eq!(hit.session_id.as_deref(), Some("parent-1"));
    assert_eq!(hit.parent_session_id.as_deref(), Some("parent-1"));
    assert_eq!(hit.thread_kind.as_deref(), Some("subagent"));
    assert_eq!(hit.agent_role.as_deref(), Some("explorer"));
    assert!(store
        .get_fresh(AgentId::Codex, "sessions/a.jsonl", 11, 100)
        .is_none());
}

#[test]
fn stale_parser_version_is_a_miss() {
    let dir = tempdir().unwrap();
    let mut store = SessionIndexStore::load(dir.path()).expect("open");
    store.put(AgentId::Codex, "sessions/a.jsonl", sample_entry());
    store.save_if_dirty();

    let conn = Connection::open(dir.path().join("scan-cache.db")).expect("db");
    conn.execute(
        "UPDATE scan_entries SET parser_version = ?1",
        params![super::PARSER_SESSIONS as i64 - 1],
    )
    .unwrap();
    drop(conn);

    let store = SessionIndexStore::load(dir.path()).expect("reopen");
    assert!(
        store
            .get_fresh(AgentId::Codex, "sessions/a.jsonl", 10, 100)
            .is_none(),
        "rows written before parent/thread/role fields must re-parse"
    );
}

#[test]
fn path_cache_roundtrip() {
    let dir = tempdir().unwrap();
    let mut store = SessionIndexStore::load(dir.path()).expect("open");
    store.put_path(AgentId::Cursor, "d-demo", r"D:\demo");
    store.save_if_dirty();
    let store = SessionIndexStore::load(dir.path()).expect("reopen");
    assert_eq!(
        store.cached_path(AgentId::Cursor, "d-demo").as_deref(),
        Some(r"D:\demo")
    );
}

#[test]
fn discards_legacy_json_when_parser_version_differs() {
    let dir = tempdir().unwrap();
    let json = serde_json::json!({
        "version": 2,
        "agents": {
            "codex": {
                "files": {
                    "sessions/old.jsonl": {
                        "mtimeMs": 5,
                        "size": 9,
                        "projectKey": "cwd/D:/old",
                        "title": "legacy",
                        "updatedAt": "t0"
                    }
                }
            }
        }
    });
    fs::write(
        dir.path().join("project_session_index.json"),
        serde_json::to_vec(&json).unwrap(),
    )
    .unwrap();

    let store = SessionIndexStore::load(dir.path()).expect("open");
    assert!(
        store
            .get_fresh(AgentId::Codex, "sessions/old.jsonl", 9, 5)
            .is_none(),
        "pre-subagent JSON must not be imported as current parser rows"
    );
    assert!(!dir.path().join("project_session_index.json").exists());
}

#[test]
fn imports_legacy_json_at_current_parser_version() {
    let dir = tempdir().unwrap();
    let json = serde_json::json!({
        "version": super::PARSER_SESSIONS,
        "agents": {
            "codex": {
                "files": {
                    "sessions/old.jsonl": {
                        "mtimeMs": 5,
                        "size": 9,
                        "projectKey": "cwd/D:/old",
                        "title": "legacy",
                        "updatedAt": "t0",
                        "sessionId": "parent-1",
                        "parentSessionId": "parent-1",
                        "threadKind": "subagent",
                        "agentRole": "explorer"
                    }
                }
            }
        }
    });
    fs::write(
        dir.path().join("project_session_index.json"),
        serde_json::to_vec(&json).unwrap(),
    )
    .unwrap();

    let store = SessionIndexStore::load(dir.path()).expect("open");
    let hit = store
        .get_fresh(AgentId::Codex, "sessions/old.jsonl", 9, 5)
        .expect("imported");
    assert_eq!(hit.title, "legacy");
    assert_eq!(hit.parent_session_id.as_deref(), Some("parent-1"));
    assert_eq!(hit.thread_kind.as_deref(), Some("subagent"));
    assert_eq!(hit.agent_role.as_deref(), Some("explorer"));
    assert!(!dir.path().join("project_session_index.json").exists());
}
