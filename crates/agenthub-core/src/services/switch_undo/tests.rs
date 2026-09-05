use super::*;
use crate::models::{Account, AccountKind, AgentId};
use crate::services::ConnectionService;
use crate::storage::AccountRepo;
use serde_json::json;
use tempfile::tempdir;

fn account(id: &str, current: bool, updated: &str) -> Account {
    Account {
        id: id.into(),
        agent_id: AgentId::Claude,
        kind: AccountKind::ApiKey,
        label: id.into(),
        credentials: json!({ "key": "x" }),
        extra: json!({}),
        status: "active".into(),
        is_current: current,
        created_at: updated.into(),
        updated_at: updated.into(),
    }
}

#[test]
fn undo_slot_roundtrip_and_take_clears() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("undo.db")).unwrap();
    record_switch_undo(&db, PROVIDER_UNDO_PREFIX, AgentId::Claude, "a", "b").unwrap();
    assert_eq!(
        peek_switch_undo(&db, PROVIDER_UNDO_PREFIX, AgentId::Claude).unwrap(),
        Some("a".into())
    );
    assert_eq!(
        peek_switch_undo(&db, PROVIDER_UNDO_PREFIX, AgentId::Claude).unwrap(),
        Some("a".into()),
        "peek must not clear"
    );
    clear_switch_undo(&db, PROVIDER_UNDO_PREFIX, AgentId::Claude).unwrap();
    assert_eq!(
        peek_switch_undo(&db, PROVIDER_UNDO_PREFIX, AgentId::Claude).unwrap(),
        None
    );
}

#[test]
fn undo_conn_helpers_roundtrip_on_open_connection() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("undo.db")).unwrap();
    db.with_conn(|conn| {
        record_switch_undo_conn(conn, ACCOUNT_UNDO_PREFIX, AgentId::Claude, "from", "to")
    })
    .unwrap();
    assert_eq!(
        peek_switch_undo(&db, ACCOUNT_UNDO_PREFIX, AgentId::Claude).unwrap(),
        Some("from".into())
    );
    db.with_conn(|conn| clear_switch_undo_conn(conn, ACCOUNT_UNDO_PREFIX, AgentId::Claude))
        .unwrap();
    assert_eq!(
        peek_switch_undo(&db, ACCOUNT_UNDO_PREFIX, AgentId::Claude).unwrap(),
        None
    );
}

#[test]
fn activate_account_with_undo_writes_slot_in_same_transaction() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("undo.db")).unwrap();
    let accounts = AccountRepo::new(db.clone());
    let from = accounts.create(&account("acc-from", true, "t1")).unwrap();
    let to = accounts.create(&account("acc-to", false, "t2")).unwrap();
    let connections = ConnectionService::new(db.clone());
    connections
        .activate_account_with_undo(
            AgentId::Claude,
            &to.id,
            &to.updated_at,
            "t3",
            Some((ACCOUNT_UNDO_PREFIX, Some(from.id.as_str()))),
        )
        .unwrap();
    assert_eq!(
        peek_switch_undo(&db, ACCOUNT_UNDO_PREFIX, AgentId::Claude).unwrap(),
        Some(from.id)
    );
    let active = connections.get_active(AgentId::Claude).unwrap().unwrap();
    assert_eq!(active.account_id.as_deref(), Some("acc-to"));
}

#[test]
fn activate_account_with_undo_clears_slot_when_from_matches_target() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("undo.db")).unwrap();
    record_switch_undo(&db, ACCOUNT_UNDO_PREFIX, AgentId::Claude, "old", "acc-same").unwrap();
    let accounts = AccountRepo::new(db.clone());
    let row = accounts.create(&account("acc-same", false, "t1")).unwrap();
    ConnectionService::new(db.clone())
        .activate_account_with_undo(
            AgentId::Claude,
            &row.id,
            &row.updated_at,
            "t2",
            Some((ACCOUNT_UNDO_PREFIX, Some(row.id.as_str()))),
        )
        .unwrap();
    assert_eq!(
        peek_switch_undo(&db, ACCOUNT_UNDO_PREFIX, AgentId::Claude).unwrap(),
        None
    );
}

#[test]
fn extract_probe_url_prefers_common_keys() {
    let settings = json!({
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "sk-secret",
            "ANTHROPIC_BASE_URL": "https://api.example.com/v1"
        }
    });
    assert_eq!(
        extract_probe_url(&settings).as_deref(),
        Some("https://api.example.com/v1")
    );
    assert!(extract_probe_url(&json!({"api_key": "sk"})).is_none());
}

#[test]
fn extract_probe_url_ignores_json_schema_and_uses_env_base() {
    let settings = json!({
        "$schema": "https://json.schemastore.org/claude-code-settings.json",
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "sk-secret",
            "ANTHROPIC_BASE_URL": "https://mytokens.cc"
        }
    });
    assert_eq!(
        extract_probe_url(&settings).as_deref(),
        Some("https://mytokens.cc")
    );
    assert_eq!(
        extract_probe_url(&json!({
            "env": { "ANTHROPIC_BASE_URL": "https://mytokens.cc" }
        }))
        .as_deref(),
        Some("https://mytokens.cc")
    );
}
