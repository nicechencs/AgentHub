use super::*;
use crate::storage::Database;

fn tmp() -> (tempfile::TempDir, LiveFingerprintRepo) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("fingerprints.db")).unwrap();
    let repo = LiveFingerprintRepo::new(db);
    (dir, repo)
}

#[test]
fn upsert_then_get_roundtrip() {
    let (_dir, repo) = tmp();
    repo.upsert("claude", "/home/.claude/settings.json", "abc123", "t0")
        .unwrap();
    assert_eq!(
        repo.get("claude", "/home/.claude/settings.json").unwrap(),
        Some("abc123".to_string())
    );
    // Different agent or path → no row.
    assert_eq!(
        repo.get("codex", "/home/.claude/settings.json").unwrap(),
        None
    );
    assert_eq!(
        repo.get("claude", "/home/.codex/config.toml").unwrap(),
        None
    );
}

#[test]
fn upsert_overwrites_previous_fingerprint() {
    let (_dir, repo) = tmp();
    repo.upsert("claude", "/settings.json", "old", "t0")
        .unwrap();
    repo.upsert("claude", "/settings.json", "new", "t1")
        .unwrap();
    assert_eq!(
        repo.get("claude", "/settings.json").unwrap(),
        Some("new".to_string())
    );
}

#[test]
fn delete_for_agent_removes_only_that_agent() {
    let (_dir, repo) = tmp();
    repo.upsert("claude", "/a.json", "h1", "t0").unwrap();
    repo.upsert("claude", "/b.json", "h2", "t0").unwrap();
    repo.upsert("codex", "/a.json", "h3", "t0").unwrap();

    let removed = repo.delete_for_agent("claude").unwrap();
    assert_eq!(removed, 2);
    assert_eq!(repo.get("claude", "/a.json").unwrap(), None);
    assert_eq!(repo.get("claude", "/b.json").unwrap(), None);
    assert_eq!(
        repo.get("codex", "/a.json").unwrap(),
        Some("h3".to_string())
    );

    assert_eq!(repo.delete_for_agent("grok").unwrap(), 0);
}
