use super::*;
use serde_json::json;
use tempfile::tempdir;

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
