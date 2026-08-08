//! Active binding repository tests (separate file).

use crate::storage::{ActiveBindingRepo, ActiveBindingRow, Database};

fn tmp_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    (dir, db)
}

#[test]
fn upsert_get_clear_roundtrip() {
    let (_dir, db) = tmp_db();
    let repo = ActiveBindingRepo::new(db);
    let row = ActiveBindingRow {
        agent_key: "claude".into(),
        account_id: Some("a1".into()),
        provider_id: None,
        model_id: None,
        config_profile_id: None,
        revision: 1,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };
    let saved = repo.upsert(&row).unwrap();
    assert_eq!(saved.account_id.as_deref(), Some("a1"));
    assert_eq!(repo.get("claude").unwrap().unwrap().revision, 1);

    let next = repo
        .set_refs("claude", None, Some("p1".into()), None, "t1")
        .unwrap();
    assert_eq!(next.revision, 2);
    assert_eq!(next.provider_id.as_deref(), Some("p1"));
    assert!(next.account_id.is_none());

    repo.clear("claude").unwrap();
    assert!(repo.get("claude").unwrap().is_none());
}

#[test]
fn rejects_invalid_agent_key() {
    let (_dir, db) = tmp_db();
    let repo = ActiveBindingRepo::new(db);
    let row = ActiveBindingRow {
        agent_key: "NOT VALID".into(),
        account_id: None,
        provider_id: None,
        model_id: None,
        config_profile_id: None,
        revision: 1,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };
    assert_eq!(repo.upsert(&row).unwrap_err().code(), "invalid_arg");
}
