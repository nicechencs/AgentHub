use super::*;
use agenthub_core::models::AccountKind;
use serde_json::json;

fn sample() -> Account {
    Account {
        id: "a1".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::ApiKey,
        label: "xai-••••1234".into(),
        credentials: json!({"format": "api_key", "api_key": "secret-should-not-leak"}),
        extra: json!({}),
        status: "active".into(),
        is_current: true,
        created_at: "t0".into(),
        updated_at: "t1".into(),
    }
}

#[test]
fn emit_list_json_redacts_secrets() {
    let items = vec![sample()];
    let redacted = items[0].redacted();
    let s = serde_json::to_string(&redacted).unwrap();
    assert!(!s.contains("secret-should-not-leak"));
    assert!(s.contains("***"));
}

#[test]
fn write_operations_require_agent() {
    assert_eq!(
        require_agent(None, "switch").unwrap_err().code(),
        "invalid_arg"
    );
    assert_eq!(
        require_agent(None, "undo").unwrap_err().code(),
        "invalid_arg"
    );
    assert_eq!(require_agent(Some("grok"), "undo").unwrap(), AgentId::Grok);
}

#[test]
fn read_key_arg_rejects_empty() {
    assert_eq!(read_key_arg("").unwrap_err().code(), "invalid_arg");
    assert_eq!(read_key_arg("   ").unwrap_err().code(), "invalid_arg");
    assert_eq!(read_key_arg("sk-ok").unwrap(), "sk-ok");
}

#[test]
fn switch_confirm_prompt_mentions_three_elements() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let prompt = switch_confirm_prompt(&hub, AgentId::Grok, "acct-1");
    assert!(prompt.contains("backfill:"));
    assert!(prompt.contains("backup:"));
    assert!(prompt.contains("process:"));
}

#[test]
fn emit_one_and_list_quiet_ok() {
    emit_list(&[sample()], OutputFormat::Quiet).unwrap();
    emit_one(&sample(), OutputFormat::Quiet).unwrap();
}
