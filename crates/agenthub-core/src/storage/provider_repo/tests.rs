use super::*;
use serde_json::json;
use tempfile::tempdir;

fn sample(id: &str, agent: AgentId, name: &str, current: bool) -> Provider {
    Provider {
        id: id.into(),
        agent_id: agent,
        name: name.into(),
        settings_config: json!({"api_key": "sk-test", "base_url": "https://x"}),
        meta: json!({}),
        is_current: current,
        created_at: "2026-01-01 00:00:00".into(),
        updated_at: "2026-01-01 00:00:00".into(),
    }
}

fn repo() -> (tempfile::TempDir, ProviderRepo) {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    (dir, ProviderRepo::new(db))
}

#[test]
fn upsert_list_get_by_id_and_name() {
    let (_dir, repo) = repo();

    repo.upsert(&sample("id-a", AgentId::Claude, "Alpha", true))
        .unwrap();
    repo.upsert(&sample("id-b", AgentId::Codex, "Alpha", false))
        .unwrap();
    repo.upsert(&sample("id-c", AgentId::Claude, "Beta", false))
        .unwrap();

    let all = repo.list(None).unwrap();
    assert_eq!(all.len(), 3);

    let claude = repo.list(Some(AgentId::Claude)).unwrap();
    assert_eq!(claude.len(), 2);
    assert!(claude.iter().all(|p| p.agent_id == AgentId::Claude));

    let by_id = repo.get_by_id("id-a").unwrap().expect("found");
    assert_eq!(by_id.name, "Alpha");
    assert!(by_id.is_current);
    assert_eq!(by_id.settings_config["api_key"], "sk-test");

    assert!(repo.get_by_id("missing").unwrap().is_none());

    let by_name = repo.list_by_name("Alpha", None).unwrap();
    assert_eq!(by_name.len(), 2);

    let by_name_agent = repo.list_by_name("Alpha", Some(AgentId::Codex)).unwrap();
    assert_eq!(by_name_agent.len(), 1);
    assert_eq!(by_name_agent[0].id, "id-b");
}

#[test]
fn create_duplicate_is_invalid_arg_and_non_mutating() {
    let (_dir, repo) = repo();
    let p = sample("dup", AgentId::Claude, "A", false);
    repo.create(&p).unwrap();

    let mut again = p.clone();
    again.name = "Changed".into();
    again.updated_at = "2026-02-02 00:00:00".into();
    let err = repo.create(&again).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");

    let stored = repo.get_by_id("dup").unwrap().unwrap();
    assert_eq!(stored.name, "A");
    assert_eq!(stored.updated_at, "2026-01-01 00:00:00");
}

#[test]
fn update_missing_is_not_found() {
    let (_dir, repo) = repo();
    let err = repo
        .update(&sample("missing", AgentId::Claude, "A", false))
        .unwrap_err();
    assert_eq!(err.code(), "not_found");
}

#[test]
fn delete_missing_is_not_found() {
    let (_dir, repo) = repo();
    let err = repo.delete("missing").unwrap_err();
    assert_eq!(err.code(), "not_found");
}

#[test]
fn update_and_upsert_preserve_created_at() {
    let (_dir, repo) = repo();
    let mut p = sample("p1", AgentId::Claude, "A", false);
    p.created_at = "2026-01-01 00:00:00".into();
    p.updated_at = "2026-01-01 00:00:00".into();
    repo.create(&p).unwrap();

    p.name = "B".into();
    p.created_at = "2099-01-01 00:00:00".into();
    p.updated_at = "2026-06-01 00:00:00".into();
    let updated = repo.update(&p).unwrap();
    assert_eq!(updated.created_at, "2026-01-01 00:00:00");
    assert_eq!(updated.name, "B");
    assert_eq!(updated.updated_at, "2026-06-01 00:00:00");

    p.name = "C".into();
    p.created_at = "2099-12-31 00:00:00".into();
    p.updated_at = "2026-07-01 00:00:00".into();
    let upserted = repo.upsert(&p).unwrap();
    assert_eq!(upserted.created_at, "2026-01-01 00:00:00");
    assert_eq!(upserted.name, "C");
}

#[test]
fn update_and_upsert_reject_agent_id_change_without_mutation() {
    let (_dir, repo) = repo();
    let p = sample("p1", AgentId::Claude, "A", true);
    repo.create(&p).unwrap();

    let mut bad = p.clone();
    bad.agent_id = AgentId::Codex;
    bad.name = "Hijacked".into();
    let err = repo.update(&bad).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");

    let err = repo.upsert(&bad).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");

    let stored = repo.get_by_id("p1").unwrap().unwrap();
    assert_eq!(stored.agent_id, AgentId::Claude);
    assert_eq!(stored.name, "A");
    assert!(stored.is_current);
}

#[test]
fn is_current_uniqueness_per_agent_on_create_update_upsert() {
    let (_dir, repo) = repo();
    repo.create(&sample("c1", AgentId::Claude, "One", true))
        .unwrap();
    repo.create(&sample("c2", AgentId::Claude, "Two", false))
        .unwrap();
    // Different agent can also be current.
    repo.create(&sample("x1", AgentId::Codex, "X", true))
        .unwrap();

    assert!(repo.get_by_id("c1").unwrap().unwrap().is_current);
    assert!(!repo.get_by_id("c2").unwrap().unwrap().is_current);
    assert!(repo.get_by_id("x1").unwrap().unwrap().is_current);

    // Promote c2 → only c2 current among Claude.
    let mut c2 = sample("c2", AgentId::Claude, "Two", true);
    c2.updated_at = "2026-03-01 00:00:00".into();
    repo.update(&c2).unwrap();
    assert!(!repo.get_by_id("c1").unwrap().unwrap().is_current);
    assert!(repo.get_by_id("c2").unwrap().unwrap().is_current);
    assert!(repo.get_by_id("x1").unwrap().unwrap().is_current);

    // Upsert another current Claude.
    repo.upsert(&sample("c3", AgentId::Claude, "Three", true))
        .unwrap();
    let currents: Vec<_> = repo
        .list(Some(AgentId::Claude))
        .unwrap()
        .into_iter()
        .filter(|p| p.is_current)
        .map(|p| p.id)
        .collect();
    assert_eq!(currents, vec!["c3".to_string()]);
    assert!(repo.get_by_id("x1").unwrap().unwrap().is_current);
}

#[test]
fn crud_roundtrip_and_delete() {
    let (_dir, repo) = repo();
    let created = repo
        .create(&sample("p1", AgentId::Grok, "G", false))
        .unwrap();
    assert_eq!(created.id, "p1");

    let mut p = created;
    p.name = "G2".into();
    p.settings_config = json!({"k": 1});
    p.meta = json!({"m": true});
    p.updated_at = "2026-04-01 00:00:00".into();
    let updated = repo.update(&p).unwrap();
    assert_eq!(updated.name, "G2");
    assert_eq!(updated.settings_config["k"], 1);
    assert_eq!(updated.meta["m"], true);

    repo.delete("p1").unwrap();
    assert!(repo.get_by_id("p1").unwrap().is_none());
    assert!(repo.list(None).unwrap().is_empty());
}

#[test]
fn failed_update_does_not_clear_current() {
    // If agent_id change is rejected mid-transaction, existing currents stay put.
    let (_dir, repo) = repo();
    repo.create(&sample("c1", AgentId::Claude, "One", true))
        .unwrap();
    repo.create(&sample("c2", AgentId::Claude, "Two", false))
        .unwrap();

    let mut bad = sample("c2", AgentId::Codex, "Two", true);
    bad.name = "Nope".into();
    let err = repo.update(&bad).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");

    assert!(repo.get_by_id("c1").unwrap().unwrap().is_current);
    let c2 = repo.get_by_id("c2").unwrap().unwrap();
    assert!(!c2.is_current);
    assert_eq!(c2.name, "Two");
    assert_eq!(c2.agent_id, AgentId::Claude);
}

#[test]
fn scoped_delete_rejects_cross_agent_without_mutation() {
    let (_dir, repo) = repo();
    repo.create(&sample("p1", AgentId::Claude, "One", true))
        .unwrap();

    let error = repo.delete_for_agent("p1", AgentId::Codex).unwrap_err();
    assert_eq!(error.code(), "not_found");
    assert!(repo.get_by_id("p1").unwrap().unwrap().is_current);

    repo.delete_for_agent("p1", AgentId::Claude).unwrap();
    assert!(repo.get_by_id("p1").unwrap().is_none());
}

#[test]
fn switch_current_backfills_and_is_scoped_per_agent() {
    let (_dir, repo) = repo();
    repo.create(&sample("c1", AgentId::Claude, "One", true))
        .unwrap();
    repo.create(&sample("c2", AgentId::Claude, "Two", false))
        .unwrap();
    repo.create(&sample("x1", AgentId::Codex, "X", true))
        .unwrap();

    let live = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "live-secret"}});
    let selected = repo
        .switch_current(
            "c2",
            AgentId::Claude,
            "2026-01-01 00:00:00",
            Some(("c1", "2026-01-01 00:00:00", &live)),
            "2026-08-01 00:00:00",
        )
        .unwrap();
    assert_eq!(selected.id, "c2");
    assert!(selected.is_current);

    let old = repo.get_by_id("c1").unwrap().unwrap();
    assert!(!old.is_current);
    assert_eq!(old.settings_config, live);
    assert_eq!(old.updated_at, "2026-08-01 00:00:00");
    assert!(repo.get_by_id("x1").unwrap().unwrap().is_current);

    let error = repo
        .switch_current(
            "x1",
            AgentId::Claude,
            "2026-01-01 00:00:00",
            None,
            "2026-08-02 00:00:00",
        )
        .unwrap_err();
    assert_eq!(error.code(), "not_found");
    assert!(repo.get_by_id("c2").unwrap().unwrap().is_current);
    assert!(repo.get_by_id("x1").unwrap().unwrap().is_current);
}

#[test]
fn switch_current_rolls_back_backfill_and_flags_on_sql_failure() {
    let (_dir, repo) = repo();
    repo.create(&sample("c1", AgentId::Claude, "One", true))
        .unwrap();
    repo.create(&sample("c2", AgentId::Claude, "Two", false))
        .unwrap();
    let original = repo.get_by_id("c1").unwrap().unwrap();

    repo.db
        .with_conn(|conn| {
            conn.execute_batch(
                r#"
                CREATE TRIGGER fail_provider_select
                BEFORE UPDATE OF is_current ON providers
                WHEN NEW.id = 'c2' AND NEW.is_current = 1
                BEGIN
                    SELECT RAISE(ABORT, 'injected switch failure');
                END;
                "#,
            )?;
            Ok(())
        })
        .unwrap();

    let live = json!({"api_key": "must-rollback"});
    let error = repo
        .switch_current(
            "c2",
            AgentId::Claude,
            "2026-01-01 00:00:00",
            Some(("c1", "2026-01-01 00:00:00", &live)),
            "2026-08-01 00:00:00",
        )
        .unwrap_err();
    assert_eq!(error.code(), "db");

    assert_eq!(repo.get_by_id("c1").unwrap().unwrap(), original);
    assert!(!repo.get_by_id("c2").unwrap().unwrap().is_current);
}

#[test]
fn multiple_current_rows_fail_closed() {
    let (_dir, repo) = repo();
    repo.create(&sample("c1", AgentId::Claude, "One", true))
        .unwrap();
    repo.create(&sample("c2", AgentId::Claude, "Two", false))
        .unwrap();
    repo.db
        .with_conn(|conn| {
            conn.execute("UPDATE providers SET is_current = 1 WHERE id = 'c2'", [])?;
            Ok(())
        })
        .unwrap();

    let error = repo.get_current(AgentId::Claude).unwrap_err();
    assert_eq!(error.code(), "provider.state");
}
