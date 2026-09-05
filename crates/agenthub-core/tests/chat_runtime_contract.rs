//! Public-contract checks for the durable chat runtime.
//!
//! These tests deliberately do not install or spawn Codex.  They exercise the
//! public ChatService/runtime boundary against a migrated temporary database.

use std::sync::Arc;

use agenthub_core::adapters::AdapterRegistry;
use agenthub_core::models::{AgentId, ChatEvent, ChatMessage, ChatMessageStatus, ChatRole};
use agenthub_core::services::chat_runtime::{
    RuntimeDecision, RuntimeEvent, RuntimePhase, RuntimeReply, RuntimeSnapshot,
};
use agenthub_core::services::{ChatService, RunService};
use agenthub_core::storage::{ChatRepo, Database};
use tempfile::tempdir;

fn chat() -> (tempfile::TempDir, Database, ChatService) {
    let dir = tempdir().expect("tempdir");
    let db = Database::open(&dir.path().join("chat-runtime.db")).expect("migrated db");
    let run = Arc::new(RunService::new(AdapterRegistry::new()));
    let chat = ChatService::new(db.clone(), run);
    (dir, db, chat)
}

fn conversation(chat: &ChatService, agent: AgentId, cwd: Option<String>) -> String {
    chat.create_conversation(vec![agent], cwd)
        .expect("create conversation")
        .id
}

#[test]
fn empty_codex_snapshot_enables_runtime_before_the_frontend_chooses_a_transport() {
    let (_dir, _db, chat) = chat();
    let id = conversation(
        &chat,
        AgentId::Codex,
        Some(std::env::temp_dir().display().to_string()),
    );

    let snapshot = chat
        .runtime()
        .snapshot(&id, None)
        .expect("runtime snapshot");
    assert!(
        snapshot.enabled,
        "an empty Codex conversation must select runtime, never legacy"
    );
    assert_eq!(snapshot.phase, RuntimePhase::Idle);
}

#[test]
fn non_codex_and_legacy_conversations_remain_disabled() {
    let (_dir, db, chat) = chat();
    let non_codex = conversation(&chat, AgentId::Claude, None);
    let legacy = conversation(&chat, AgentId::Codex, None);
    ChatRepo::new(db)
        .insert_message(&ChatMessage {
            id: "legacy-message".into(),
            conversation_id: legacy.clone(),
            turn: 1,
            role: ChatRole::User,
            agent_id: None,
            content: "legacy history".into(),
            status: ChatMessageStatus::Ok,
            exit_code: None,
            duration_ms: 0,
            error: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        })
        .expect("insert legacy history");

    assert!(!chat.runtime().snapshot(&non_codex, None).unwrap().enabled);
    assert!(!chat.runtime().snapshot(&legacy, None).unwrap().enabled);
}

#[test]
fn missing_cwd_start_failure_cannot_leave_a_running_snapshot_or_accept_an_old_reply() {
    let (_dir, _db, chat) = chat();
    let id = conversation(&chat, AgentId::Codex, None);

    assert!(chat.runtime().start(&id, "hello", "start-1").is_err());
    let snapshot = chat
        .runtime()
        .snapshot(&id, None)
        .expect("snapshot after failed start");
    assert!(
        !matches!(
            snapshot.phase,
            RuntimePhase::Starting
                | RuntimePhase::Running
                | RuntimePhase::Waiting
                | RuntimePhase::Cancelling
        ),
        "a start failure must be terminal, got {:?}",
        snapshot.phase
    );

    let reply = RuntimeReply {
        conversation_id: id,
        run_id: "old-run".into(),
        request_id: "old-request".into(),
        client_request_id: "reply-1".into(),
        decision: Some(RuntimeDecision::Allow),
        answers: None,
    };
    assert!(
        chat.runtime().reply(reply).is_err(),
        "old run replies must be rejected"
    );
}

#[test]
fn persisted_running_state_rejects_a_second_start_without_spawning_codex() {
    let (_dir, db, chat) = chat();
    let id = conversation(&chat, AgentId::Codex, None);
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO chat_runtime (conversation_id, enabled, run_id, phase, last_sequence, updated_at) VALUES (?1, 1, 'run-live', 'running', 0, ?2)",
            rusqlite::params![id, "2026-01-01T00:00:00Z"],
        )?;
        Ok(())
    })
    .expect("seed running runtime");

    let error = chat
        .runtime()
        .start(&id, "must not spawn", "start-2")
        .unwrap_err();
    assert!(error.to_string().contains("active runtime turn"));
    let snapshot = chat.runtime().snapshot(&id, None).unwrap();
    assert_eq!(
        snapshot.phase,
        RuntimePhase::Running,
        "rejecting a second start must preserve the first run"
    );
    assert_eq!(snapshot.run_id.as_deref(), Some("run-live"));
}

#[test]
fn runtime_dtos_use_the_public_camel_case_wire_contract() {
    let snapshot = RuntimeSnapshot {
        conversation_id: "conversation-1".into(),
        enabled: true,
        run_id: Some("run-1".into()),
        phase: RuntimePhase::Waiting,
        last_sequence: 7,
        events: vec![RuntimeEvent {
            sequence: 7,
            event: ChatEvent::Error {
                message: "safe".into(),
            },
        }],
        pending_requests: Vec::new(),
        current_message: None,
        gap: false,
    };
    let value = serde_json::to_value(&snapshot).expect("serialize snapshot");
    assert_eq!(value["conversationId"], "conversation-1");
    assert_eq!(value["runId"], "run-1");
    assert_eq!(value["lastSequence"], 7);
    assert!(value["currentMessage"].is_null());
    assert!(value.get("conversation_id").is_none());
    let restored: RuntimeSnapshot = serde_json::from_value(value).expect("round trip snapshot");
    assert_eq!(restored.run_id.as_deref(), Some("run-1"));

    let reply = RuntimeReply {
        conversation_id: "conversation-1".into(),
        run_id: "run-1".into(),
        request_id: "request-1".into(),
        client_request_id: "client-1".into(),
        decision: Some(RuntimeDecision::Deny),
        answers: None,
    };
    let reply_value = serde_json::to_value(reply).expect("serialize reply");
    assert_eq!(reply_value["clientRequestId"], "client-1");
    assert_eq!(reply_value["decision"], "deny");
}
