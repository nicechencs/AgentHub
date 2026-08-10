use super::*;
use serde_json::json;
use tempfile::tempdir;

fn sample(id: &str, agent: AgentId, label: &str, current: bool) -> Account {
    Account {
        id: id.into(),
        agent_id: agent,
        kind: AccountKind::ApiKey,
        label: label.into(),
        credentials: json!({"format": "api_key", "api_key": "sk-secret"}),
        extra: json!({}),
        status: "active".into(),
        is_current: current,
        created_at: "2026-01-01 00:00:00".into(),
        updated_at: "2026-01-01 00:00:00".into(),
    }
}

fn repo() -> (tempfile::TempDir, AccountRepo) {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    (dir, AccountRepo::new(db))
}

#[test]
fn create_list_get_and_single_current() {
    let (_dir, repo) = repo();
    repo.create(&sample("a1", AgentId::Grok, "key-a", true))
        .unwrap();
    repo.create(&sample("a2", AgentId::Grok, "key-b", true))
        .unwrap();

    let list = repo.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(list.len(), 2);
    let current = repo.get_current(AgentId::Grok).unwrap().unwrap();
    assert_eq!(current.id, "a2");
    assert_eq!(list.iter().filter(|a| a.is_current).count(), 1);
}

#[test]
fn select_current_enforces_single_current() {
    let (_dir, repo) = repo();
    repo.create(&sample("a1", AgentId::Codex, "one", true))
        .unwrap();
    repo.create(&sample("a2", AgentId::Codex, "two", false))
        .unwrap();
    let selected = repo
        .select_current(
            "a2",
            AgentId::Codex,
            "2026-01-01 00:00:00",
            "2026-02-01 00:00:00",
        )
        .unwrap();
    assert!(selected.is_current);
    assert_eq!(selected.id, "a2");
    let a1 = repo.get_by_id("a1").unwrap().unwrap();
    assert!(!a1.is_current);
}

#[test]
fn delete_missing_is_not_found() {
    let (_dir, repo) = repo();
    assert_eq!(repo.delete("missing").unwrap_err().code(), "not_found");
}

#[test]
fn healed_field_update_is_cas_and_preserves_current_flag() {
    let (_dir, repo) = repo();
    let mut account = sample("a1", AgentId::Codex, "one", true);
    let stored = repo.create(&account).unwrap();
    account = stored.clone();
    account.label = "healed".into();
    account.credentials = json!({"access_token": "new"});
    account.extra = json!({"quota": {"used": 1}});
    account.status = "active".into();

    let updated = repo
        .update_healed_fields(&account, &stored.updated_at, "2026-02-01 00:00:00")
        .unwrap();
    assert!(updated.is_current);
    assert_eq!(updated.label, "healed");

    let stale = repo
        .update_healed_fields(&account, &stored.updated_at, "2026-03-01 00:00:00")
        .unwrap_err();
    assert_eq!(stale.code(), "account.conflict");
    assert!(repo.get_by_id("a1").unwrap().unwrap().is_current);
}
