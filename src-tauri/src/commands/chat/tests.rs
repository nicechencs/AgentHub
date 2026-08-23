use super::*;
use agenthub_core::models::AgentId;
use tempfile::tempdir;

#[test]
fn create_list_send_and_delete() {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let conv = create_conversation_inner(&hub, vec!["claude".into()], None).unwrap();
    assert!(!conv.id.is_empty());
    let list = list_conversations_inner(&hub).unwrap();
    assert_eq!(list.len(), 1);

    hub.chat
        .send(&conv.id, "hello from test", &|_ev| {})
        .unwrap();
    let msgs = list_chat_messages_inner(&hub, &conv.id).unwrap();
    assert!(msgs
        .iter()
        .any(|m| matches!(m.role, agenthub_core::models::ChatRole::User)));
    delete_conversation_inner(&hub, &conv.id).unwrap();
    assert!(list_conversations_inner(&hub).unwrap().is_empty());
}

#[test]
fn parse_agent_ids_dedupes_and_rejects_empty() {
    let ids = parse_agent_ids(vec!["claude".into(), "claude".into(), "codex".into()]).unwrap();
    assert_eq!(ids, vec![AgentId::Claude, AgentId::Codex]);

    let err = parse_agent_ids(vec![]).unwrap_err();
    assert!(err.contains("empty"));

    let err = parse_agent_ids(vec!["nope".into()]).unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn create_dedupes_agents_and_update_roundtrip() {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let conv =
        create_conversation_inner(&hub, vec!["claude".into(), "claude".into()], None).unwrap();
    assert_eq!(conv.agent_ids, vec![AgentId::Claude]);

    let err =
        create_conversation_inner(&hub, vec!["claude".into(), "grok".into()], None).unwrap_err();
    assert!(err.contains("only one agent"));

    let updated = update_conversation_inner(
        &hub,
        &conv.id,
        Some("title".into()),
        Some(vec!["codex".into()]),
        Some(String::new()), // clear cwd
        Some(true),
    )
    .unwrap();
    assert_eq!(updated.title, "title");
    assert_eq!(updated.agent_ids, vec![AgentId::Codex]);
    assert!(updated.cwd.is_none());
    assert!(updated.allow_dangerous);
}

#[test]
fn ensure_default_is_idempotent_but_create_remains_explicit() {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let first = ensure_default_conversation_inner(&hub, vec!["claude".into()], None).unwrap();
    let second = ensure_default_conversation_inner(&hub, vec!["claude".into()], None).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(list_conversations_inner(&hub).unwrap().len(), 1);

    let explicit_a = create_conversation_inner(&hub, vec!["claude".into()], None).unwrap();
    let explicit_b = create_conversation_inner(&hub, vec!["claude".into()], None).unwrap();
    assert_ne!(explicit_a.id, explicit_b.id);
    assert_eq!(list_conversations_inner(&hub).unwrap().len(), 3);
}

#[test]
fn cancel_is_noop_without_inflight_send() {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let conv = create_conversation_inner(&hub, vec!["claude".into()], None).unwrap();
    chat_cancel_inner(&hub, &conv.id).unwrap();
}

#[test]
fn empty_agent_list_rejected_on_create() {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let err = create_conversation_inner(&hub, vec![], None).unwrap_err();
    assert!(err.contains("empty") || err.contains("at least"));
}
