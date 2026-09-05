use super::*;

use crate::adapters::AdapterRegistry;
use crate::models::{AgentId, ChatEvent, ChatMessageStatus, ChatRole, Conversation};
use crate::services::RunService;
use crate::storage::{ChatRepo, Database};
use serde_json::json;
use std::sync::Arc;

fn conversation(db: &Database, id: &str) {
    ChatRepo::new(db.clone())
        .create_conversation(&Conversation {
            id: id.into(),
            title: String::new(),
            agent_ids: vec![AgentId::Codex],
            cwd: Some(std::env::temp_dir().to_string_lossy().into_owned()),
            allow_dangerous: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            native_session_id: None,
            sending: false,
        })
        .unwrap();
}

fn worker(db: &Database, id: &str) -> ActorWorker {
    let (_tx, rx) = std::sync::mpsc::sync_channel(1);
    ActorWorker {
        conversation_id: id.into(),
        rx,
        store: store::RuntimeStore::new(db.clone()),
        repo: ChatRepo::new(db.clone()),
        run: Arc::new(RunService::new(AdapterRegistry::default())),
        transport: None,
        thread_id: None,
        turn_id: None,
        chat_turn: None,
        message_id: None,
        run_id: None,
        last_start_request: None,
    }
}

fn start_placeholder(worker: &mut ActorWorker) {
    let now = "2026-01-01T00:00:00Z".to_string();
    let mut user = crate::models::ChatMessage {
        id: "user-1".into(),
        conversation_id: worker.conversation_id.clone(),
        turn: 0,
        role: ChatRole::User,
        agent_id: None,
        content: "hello".into(),
        status: ChatMessageStatus::Ok,
        exit_code: None,
        duration_ms: 0,
        error: None,
        created_at: now.clone(),
    };
    let mut agent = crate::models::ChatMessage {
        id: "agent-1".into(),
        conversation_id: worker.conversation_id.clone(),
        turn: 0,
        role: ChatRole::Agent,
        agent_id: Some(AgentId::Codex),
        content: String::new(),
        status: ChatMessageStatus::Running,
        exit_code: None,
        duration_ms: 0,
        error: None,
        created_at: now,
    };
    let run_id = "run-1";
    let turn = worker
        .store
        .begin_turn(
            &worker.conversation_id,
            &mut user,
            &mut agent,
            run_id,
            None,
            |turn| {
                vec![ChatEvent::Started {
                    turn,
                    agents: vec![AgentId::Codex],
                }]
            },
        )
        .unwrap();
    worker.chat_turn = Some(turn);
    worker.message_id = Some(agent.id);
    worker.run_id = Some(run_id.into());
    worker.thread_id = Some("thread-1".into());
    worker.turn_id = Some("turn-1".into());
    worker
        .store
        .set_state(
            &worker.conversation_id,
            RuntimePhase::Running,
            Some(run_id),
            None,
            Some("turn-1"),
            Some(turn),
            worker.message_id.as_deref(),
        )
        .unwrap();
}

#[test]
fn non_retryable_notification_error_terminalizes_message_and_controls() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "error");
    let mut worker = worker(&db, "error");
    worker.store.enable_if_new("error").unwrap();
    start_placeholder(&mut worker);
    worker
        .store
        .add_request(
            "error",
            &RuntimeRequest {
                id: "request-1".into(),
                run_id: "run-1".into(),
                kind: RuntimeRequestKind::Command,
                title: "执行命令".into(),
                detail: "safe".into(),
                questions: Vec::new(),
            },
            "item/commandExecution/requestApproval",
            "1",
        )
        .unwrap();

    worker
        .notification("error", &json!({"message": "fatal", "willRetry": false}))
        .unwrap();
    let snapshot = worker.store.snapshot("error", None).unwrap();
    assert_eq!(snapshot.phase, RuntimePhase::Failed);
    assert!(snapshot.pending_requests.is_empty());
    assert_eq!(
        snapshot.current_message.unwrap().status,
        ChatMessageStatus::Failed
    );
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| matches!(event.event, ChatEvent::AgentFinished { .. }))
    );
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| matches!(event.event, ChatEvent::Finished { ok: false, .. }))
    );
}

#[test]
fn retryable_notification_error_keeps_the_turn_alive() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "retry");
    let mut worker = worker(&db, "retry");
    worker.store.enable_if_new("retry").unwrap();
    start_placeholder(&mut worker);
    worker
        .notification("error", &json!({"message": "temporary", "willRetry": true}))
        .unwrap();
    let snapshot = worker.store.snapshot("retry", None).unwrap();
    assert_eq!(snapshot.phase, RuntimePhase::Running);
    assert_eq!(
        snapshot.current_message.unwrap().status,
        ChatMessageStatus::Running
    );
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| matches!(event.event, ChatEvent::Error { .. }))
    );
}

#[cfg(unix)]
fn fake_transport() -> (tempfile::TempDir, CodexTransport, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("fake-codex");
    std::fs::write(
        &program,
        r##"#!/bin/sh
log="$(dirname "$0")/wire.log"
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' '{"id":1,"result":{"initialized":true}}'
      ;;
    *'"method":"turn/interrupt"'*)
      printf '%s\n' interrupt >> "$log"
      printf '%s\n' '{"id":2,"result":{}}'
      printf '%s\n' '{"method":"turn/completed","params":{"status":"interrupted"}}'
      ;;
    *'"decision":"accept"'*)
      printf '%s\n' accept >> "$log"
      ;;
  esac
done
"##,
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&program).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&program, permissions).unwrap();
    let log = directory.path().join("wire.log");
    let transport = CodexTransport::spawn(&program, directory.path()).unwrap();
    (directory, transport, log)
}

#[cfg(unix)]
#[test]
fn stop_wins_over_late_allow_and_reply_before_stop_is_sent() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "stop-first");
    let mut first_worker = worker(&db, "stop-first");
    first_worker.store.enable_if_new("stop-first").unwrap();
    start_placeholder(&mut first_worker);
    let request = RuntimeRequest {
        id: "request-1".into(),
        run_id: "run-1".into(),
        kind: RuntimeRequestKind::Command,
        title: "执行命令".into(),
        detail: "safe".into(),
        questions: Vec::new(),
    };
    let (_directory, transport, log) = fake_transport();
    first_worker.transport = Some(transport);
    first_worker
        .store
        .add_request("stop-first", &request, "approval", "server-stop")
        .unwrap();
    first_worker.cancel("run-1").unwrap();
    assert!(
        first_worker
            .reply(RuntimeReply {
                conversation_id: "stop-first".into(),
                run_id: "run-1".into(),
                request_id: "request-1".into(),
                client_request_id: "late-allow".into(),
                decision: Some(RuntimeDecision::Allow),
                answers: None,
            })
            .is_err()
    );
    first_worker.poll_events().unwrap();
    assert_eq!(
        first_worker
            .store
            .snapshot("stop-first", None)
            .unwrap()
            .phase,
        RuntimePhase::Cancelled
    );
    let wire = std::fs::read_to_string(log).unwrap();
    assert!(!wire.lines().any(|line| line == "accept"));
    assert!(wire.lines().any(|line| line == "interrupt"));

    let db = Database::open_in_memory().unwrap();
    conversation(&db, "reply-first");
    let mut second_worker = worker(&db, "reply-first");
    second_worker.store.enable_if_new("reply-first").unwrap();
    start_placeholder(&mut second_worker);
    let (_directory, transport, log) = fake_transport();
    second_worker.transport = Some(transport);
    second_worker
        .store
        .add_request("reply-first", &request, "approval", "server-allow")
        .unwrap();
    second_worker
        .reply(RuntimeReply {
            conversation_id: "reply-first".into(),
            run_id: "run-1".into(),
            request_id: "request-1".into(),
            client_request_id: "allow-before-stop".into(),
            decision: Some(RuntimeDecision::Allow),
            answers: None,
        })
        .unwrap();
    second_worker.cancel("run-1").unwrap();
    second_worker.poll_events().unwrap();
    assert_eq!(
        second_worker
            .store
            .snapshot("reply-first", None)
            .unwrap()
            .phase,
        RuntimePhase::Cancelled
    );
    let lines = std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let accept = lines.iter().position(|line| line == "accept").unwrap();
    let interrupt = lines.iter().position(|line| line == "interrupt").unwrap();
    assert!(accept < interrupt);
}

#[test]
fn recover_active_terminalizes_old_agent_message() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "recover");
    let mut worker = worker(&db, "recover");
    worker.store.enable_if_new("recover").unwrap();
    start_placeholder(&mut worker);
    worker.store.recover_active().unwrap();
    let snapshot = worker.store.snapshot("recover", None).unwrap();
    assert_eq!(snapshot.phase, RuntimePhase::Interrupted);
    assert_eq!(
        snapshot.current_message.unwrap().status,
        ChatMessageStatus::Cancelled
    );
}

#[test]
fn operation_ledger_keeps_a_b_a_idempotency_history() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "ledger");
    let store = store::RuntimeStore::new(db);
    store.enable_if_new("ledger").unwrap();
    assert_eq!(
        store.begin_operation("ledger", "start", "a", None).unwrap(),
        store::OperationState::New
    );
    store
        .mark_operation("ledger", "start", "a", store::OperationState::Failed, None)
        .unwrap();
    assert_eq!(
        store.begin_operation("ledger", "start", "b", None).unwrap(),
        store::OperationState::New
    );
    store
        .mark_operation(
            "ledger",
            "start",
            "b",
            store::OperationState::Accepted,
            Some("run-b"),
        )
        .unwrap();
    assert_eq!(
        store.begin_operation("ledger", "start", "a", None).unwrap(),
        store::OperationState::Failed
    );
    assert_eq!(
        store.begin_operation("ledger", "start", "b", None).unwrap(),
        store::OperationState::Accepted
    );
}

#[test]
fn failed_old_run_reply_with_same_client_id_never_becomes_success() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "old-reply");
    store::RuntimeStore::new(db.clone())
        .enable_if_new("old-reply")
        .unwrap();
    let runtime = Arc::new(ChatRuntime::new(
        db,
        Arc::new(RunService::new(AdapterRegistry::default())),
    ));
    let reply = RuntimeReply {
        conversation_id: "old-reply".into(),
        run_id: "old-run".into(),
        request_id: "request-1".into(),
        client_request_id: "same-client-id".into(),
        decision: Some(RuntimeDecision::Allow),
        answers: None,
    };
    assert!(runtime.reply(reply.clone()).is_err());
    assert!(runtime.reply(reply).is_err());
}

#[test]
fn retained_events_report_a_gap_after_old_sequences_are_trimmed() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "gap");
    let store = store::RuntimeStore::new(db);
    store.enable_if_new("gap").unwrap();
    for index in 0..2_100 {
        store
            .commit_event(
                "gap",
                RuntimePhase::Running,
                Some("run-1"),
                &ChatEvent::Error {
                    message: format!("event-{index}"),
                },
            )
            .unwrap();
    }
    let snapshot = store.snapshot("gap", Some(0)).unwrap();
    assert!(snapshot.gap);
    assert!(snapshot.events.len() <= 2_048);
    assert!(snapshot.events.first().unwrap().sequence > 1);
}
