use super::*;
use crate::models::AgentId;
use tempfile::tempdir;

#[test]
fn put_and_get_fresh_roundtrip() {
    let dir = tempdir().unwrap();
    let mut store = SessionIndexStore::load(dir.path()).expect("open");
    store.put(
        AgentId::Codex,
        "sessions/a.jsonl",
        IndexEntry {
            mtime_ms: 100,
            size: 10,
            project_key: "cwd/D:/work".into(),
            cwd: Some("D:/work".into()),
            title: "t".into(),
            preview: Some("p".into()),
            message_count: Some(2),
            updated_at: "t0".into(),
            session_id: Some("sid-1".into()),
        },
    );
    store.save_if_dirty();
    assert!(dir.path().join("scan-cache.db").exists());

    let store = SessionIndexStore::load(dir.path()).expect("reopen");
    assert!(store
        .get_fresh(AgentId::Codex, "sessions/a.jsonl", 10, 100)
        .is_some());
    assert!(store
        .get_fresh(AgentId::Codex, "sessions/a.jsonl", 11, 100)
        .is_none());
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
fn imports_legacy_json_then_removes_it() {
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
    let hit = store
        .get_fresh(AgentId::Codex, "sessions/old.jsonl", 9, 5)
        .expect("imported");
    assert_eq!(hit.title, "legacy");
    assert!(!dir.path().join("project_session_index.json").exists());
}
