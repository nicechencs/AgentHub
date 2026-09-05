use super::*;
use crate::models::{AgentId, ChatEvent, Conversation};
use crate::storage::{ChatRepo, Database};
use crate::{
    adapters::AdapterRegistry,
    services::{ChatService, RunService},
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::tempdir;

fn conversation(db: &Database, id: &str, messages: bool) -> Conversation {
    let now = "2026-01-01T00:00:00Z".to_string();
    let value = Conversation {
        id: id.to_string(),
        title: String::new(),
        agent_ids: vec![AgentId::Codex],
        cwd: Some(std::env::temp_dir().to_string_lossy().into_owned()),
        allow_dangerous: false,
        created_at: now.clone(),
        updated_at: now,
        native_session_id: None,
        sending: false,
    };
    let repo = ChatRepo::new(db.clone());
    repo.create_conversation(&value).unwrap();
    if messages {
        let user = crate::models::ChatMessage {
            id: "legacy-user".into(),
            conversation_id: id.into(),
            turn: 1,
            role: crate::models::ChatRole::User,
            agent_id: None,
            content: "legacy".into(),
            status: crate::models::ChatMessageStatus::Ok,
            exit_code: None,
            duration_ms: 0,
            error: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        repo.insert_message(&user).unwrap();
    }
    value
}

#[test]
fn empty_codex_conversation_enables_runtime_but_legacy_stays_legacy() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "empty", false);
    conversation(&db, "legacy", true);
    let store = super::store::RuntimeStore::new(db);

    store.enable_if_new("empty").unwrap();
    let snapshot = store.snapshot("empty", None).unwrap();
    assert!(snapshot.enabled);
    assert_eq!(snapshot.phase, RuntimePhase::Idle);
    assert!(store.enable_if_new("legacy").is_err());
}

#[test]
fn public_snapshot_advertises_new_codex_conversations_as_runtime_enabled() {
    let db = Database::open_in_memory().unwrap();
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let chat = ChatService::new(db, run);
    let conversation = chat
        .create_conversation(
            vec![AgentId::Codex],
            Some(std::env::temp_dir().display().to_string()),
        )
        .unwrap();
    let snapshot = chat.runtime().snapshot(&conversation.id, None).unwrap();
    assert!(snapshot.enabled);
    assert_eq!(snapshot.phase, RuntimePhase::Idle);
}

#[test]
fn persisted_events_are_replayed_after_the_requested_sequence() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "c1", false);
    let store = super::store::RuntimeStore::new(db);
    store.enable_if_new("c1").unwrap();
    store
        .commit_event(
            "c1",
            RuntimePhase::Running,
            Some("run-1"),
            &ChatEvent::Error {
                message: "safe".into(),
            },
        )
        .unwrap();
    let all = store.snapshot("c1", None).unwrap();
    assert_eq!(all.last_sequence, 1);
    assert_eq!(all.events.len(), 1);
    let replay = store.snapshot("c1", Some(0)).unwrap();
    assert_eq!(replay.events.len(), 1);
    assert_eq!(replay.events[0].sequence, all.events[0].sequence);
    assert!(store.snapshot("c1", Some(1)).unwrap().events.is_empty());
}

#[test]
fn persisted_request_is_removed_only_after_explicit_resolution() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "c2", false);
    let store = super::store::RuntimeStore::new(db);
    store.enable_if_new("c2").unwrap();
    let request = RuntimeRequest {
        id: "req-1".into(),
        run_id: "run-1".into(),
        kind: RuntimeRequestKind::Command,
        title: "执行命令".into(),
        detail: "printf safe".into(),
        questions: Vec::new(),
    };
    store
        .add_request("c2", &request, "item/commandExecution/requestApproval", "7")
        .unwrap();
    let snapshot = store.snapshot("c2", None).unwrap();
    assert_eq!(snapshot.phase, RuntimePhase::Waiting);
    assert_eq!(snapshot.pending_requests, vec![request]);
    assert!(store.remove_request("c2", "req-1").unwrap());
    assert!(
        store
            .snapshot("c2", None)
            .unwrap()
            .pending_requests
            .is_empty()
    );
}

/// Real Codex app-server smoke test.  It is deliberately ignored: the caller
/// must opt in with `AGENTHUB_RUN_CODEX_RUNTIME_TEST=1`.  Only the AgentHub
/// database and working directory are temporary; the caller's existing Codex
/// login is used without printing or changing it.
#[test]
#[ignore = "uses the caller's Codex login and creates native sessions; explicit opt-in required"]
fn real_codex_runtime_start_and_resume() {
    assert_eq!(
        std::env::var("AGENTHUB_RUN_CODEX_RUNTIME_TEST").ok().as_deref(),
        Some("1"),
        "set AGENTHUB_RUN_CODEX_RUNTIME_TEST=1 to use the existing Codex login; no live test was run"
    );
    let root = tempdir().unwrap();
    let data_dir = root.path().join("agenthub");
    let cwd = root.path().join("workspace");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&cwd).unwrap();

    let db = Database::open(&data_dir.join("agenthub.sqlite")).unwrap();
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let chat = ChatService::new(db, run);
    let conversation = chat
        .create_conversation(
            vec![AgentId::Codex],
            Some(cwd.to_string_lossy().into_owned()),
        )
        .unwrap();
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let first = chat
        .runtime()
        .start(
            &conversation.id,
            &format!("Remember this random marker for our next turn: AGENTHUB_RUNTIME_{nonce}. Reply with exactly that marker. Do not call any tools."),
            "real-1",
        )
        .unwrap();
    let first_done = wait_for_terminal(&chat, &conversation.id, first.last_sequence);
    assert!(matches!(first_done.phase, RuntimePhase::Completed));

    drop(chat);
    let db = Database::open(&data_dir.join("agenthub.sqlite")).unwrap();
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let chat = ChatService::new(db, run);
    let second = chat
        .runtime()
        .start(
            &conversation.id,
            "What was the exact random marker I asked you to remember in my previous message? Reply only with that marker. Do not call any tools.",
            "real-2",
        )
        .unwrap();
    let second_done = wait_for_terminal(&chat, &conversation.id, second.last_sequence);
    assert!(matches!(second_done.phase, RuntimePhase::Completed));
    let messages = chat.list_messages(&conversation.id).unwrap();
    let answer = messages
        .iter()
        .filter(|m| m.role == ChatRole::Agent)
        .last()
        .unwrap();
    assert!(
        answer.content.contains(&nonce),
        "resumed turn did not remember the previous marker"
    );
}

fn wait_for_terminal(chat: &ChatService, conversation_id: &str, after: i64) -> RuntimeSnapshot {
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut sequence = after;
    loop {
        let snapshot = chat
            .runtime()
            .snapshot(conversation_id, Some(sequence))
            .unwrap();
        sequence = snapshot.last_sequence;
        if matches!(
            snapshot.phase,
            RuntimePhase::Completed
                | RuntimePhase::Failed
                | RuntimePhase::Cancelled
                | RuntimePhase::Interrupted
        ) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "Codex runtime did not reach a terminal phase"
        );
        std::thread::sleep(Duration::from_millis(400));
    }
}
