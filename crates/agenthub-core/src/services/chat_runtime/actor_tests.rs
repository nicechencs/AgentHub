use super::*;

use crate::adapters::AdapterRegistry;
use crate::error::AppError;
use crate::logging::with_captured_logs;
use crate::models::{AgentId, ChatEvent, ChatMessageStatus, ChatRole, Conversation};
use crate::services::RunService;
use crate::storage::{ChatRepo, Database};
use serde_json::json;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn conversation(db: &Database, id: &str) {
    conversation_with(db, id, AgentId::Codex, &std::env::temp_dir());
}

fn conversation_with(db: &Database, id: &str, agent: AgentId, cwd: &std::path::Path) {
    ChatRepo::new(db.clone())
        .create_conversation(&Conversation {
            id: id.into(),
            title: String::new(),
            agent_ids: vec![agent],
            cwd: Some(cwd.to_string_lossy().into_owned()),
            allow_dangerous: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            native_session_id: None,
            sending: false,
            first_user_content: None,
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
        catalogs: Arc::new(std::sync::Mutex::new(HashMap::new())),
        codex_program_override: Arc::new(std::sync::Mutex::new(None)),
        abort: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        agent: AgentId::Codex,
        transport: None,
        thread_id: None,
        turn_id: None,
        chat_turn: None,
        message_id: None,
        run_id: None,
        last_start_request: None,
        pending_prompt_id: None,
        permission_options: HashMap::new(),
        file_change_items: HashMap::new(),
        cancel_deadline: None,
        session_model: None,
        session_effort: None,
        session_trust_all: None,
        session_allow_always: false,
        pending_fs_writes: HashMap::new(),
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
                permission_options: Vec::new(),
                file_changes: Vec::new(),
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
    assert!(snapshot
        .events
        .iter()
        .any(|event| matches!(event.event, ChatEvent::AgentFinished { .. })));
    assert!(snapshot
        .events
        .iter()
        .any(|event| matches!(event.event, ChatEvent::Finished { ok: false, .. })));
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
    assert!(snapshot
        .events
        .iter()
        .any(|event| matches!(event.event, ChatEvent::Error { .. })));
}

#[test]
fn thread_token_usage_updated_emits_turn_and_session() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "usage");
    let mut worker = worker(&db, "usage");
    worker.store.enable_if_new("usage").unwrap();
    start_placeholder(&mut worker);

    worker
        .notification(
            "thread/tokenUsage/updated",
            &json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "tokenUsage": {
                    "last": {
                        "inputTokens": 100,
                        "cachedInputTokens": 20,
                        "cacheWriteInputTokens": 0,
                        "outputTokens": 10,
                        "reasoningOutputTokens": 5,
                        "totalTokens": 110
                    },
                    "total": {
                        "inputTokens": 400,
                        "cachedInputTokens": 80,
                        "cacheWriteInputTokens": 0,
                        "outputTokens": 40,
                        "reasoningOutputTokens": 15,
                        "totalTokens": 440
                    },
                    "modelContextWindow": 258400
                }
            }),
        )
        .unwrap();

    let snapshot = worker.store.snapshot("usage", None).unwrap();
    let rows: Vec<_> = snapshot
        .events
        .iter()
        .filter_map(|event| match &event.event {
            ChatEvent::AgentProcess {
                step:
                    crate::models::ProcessStep::Usage {
                        scope,
                        input,
                        output,
                        total,
                        context_window,
                        ..
                    },
                ..
            } => Some((scope.clone(), *input, *output, *total, *context_window)),
            _ => None,
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            (Some("turn".into()), Some(100), Some(10), Some(110), None),
            (
                Some("session".into()),
                Some(400),
                Some(40),
                Some(440),
                Some(258400)
            ),
        ]
    );
}

#[test]
fn grok_turn_completed_usage_emits_process_step() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "grok-usage");
    let mut worker = worker(&db, "grok-usage");
    worker.agent = AgentId::Grok;
    worker.store.enable_if_new("grok-usage").unwrap();
    start_placeholder(&mut worker);

    worker
        .notification(
            "session/update",
            &json!({
                "update": {
                    "sessionUpdate": "turn_completed",
                    "usage": {
                        "inputTokens": 50,
                        "outputTokens": 5,
                        "cachedReadTokens": 10
                    }
                }
            }),
        )
        .unwrap();

    let snapshot = worker.store.snapshot("grok-usage", None).unwrap();
    assert!(snapshot.events.iter().any(|event| matches!(
        &event.event,
        ChatEvent::AgentProcess {
            step: crate::models::ProcessStep::Usage {
                input: Some(50),
                output: Some(5),
                cache_read: Some(10),
                ..
            },
            ..
        }
    )));
}

#[cfg(unix)]
fn fake_transport() -> (tempfile::TempDir, CodexTransport, std::path::PathBuf) {
    fake_transport_with(None)
}

#[cfg(unix)]
fn fake_transport_with(
    abort: Option<Arc<std::sync::atomic::AtomicBool>>,
) -> (tempfile::TempDir, CodexTransport, std::path::PathBuf) {
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
    *'"decision":"acceptForSession"'*)
      printf '%s\n' acceptForSession >> "$log"
      ;;
    *'"decision":"accept"'*)
      printf '%s\n' accept >> "$log"
      ;;
    *'"optionId":'*)
      printf '%s\n' "$line" >> "$log"
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
    let transport = match abort {
        Some(flag) => {
            CodexTransport::spawn_interruptible(&program, directory.path(), flag).unwrap()
        }
        None => CodexTransport::spawn(&program, directory.path()).unwrap(),
    };
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
        permission_options: Vec::new(),
        file_changes: Vec::new(),
    };
    let (_directory, transport, log) = fake_transport();
    first_worker.transport = Some(transport);
    first_worker
        .store
        .add_request("stop-first", &request, "approval", "server-stop")
        .unwrap();
    first_worker.cancel("run-1").unwrap();
    assert!(first_worker
        .reply(RuntimeReply {
            conversation_id: "stop-first".into(),
            run_id: "run-1".into(),
            request_id: "request-1".into(),
            client_request_id: "late-allow".into(),
            decision: Some(RuntimeDecision::Allow),
            answers: None,
        })
        .is_err());
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
fn empty_approval_answers_are_treated_as_absent() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "empty-answers");
    let mut worker = worker(&db, "empty-answers");
    worker.store.enable_if_new("empty-answers").unwrap();
    start_placeholder(&mut worker);
    worker
        .store
        .add_request(
            "empty-answers",
            &RuntimeRequest {
                id: "request-1".into(),
                run_id: "run-1".into(),
                kind: RuntimeRequestKind::Command,
                title: "执行命令".into(),
                detail: "safe".into(),
                questions: Vec::new(),
                permission_options: Vec::new(),
                file_changes: Vec::new(),
            },
            "session/request_permission",
            "server-1",
        )
        .unwrap();
    let empty = worker
        .reply(RuntimeReply {
            conversation_id: "empty-answers".into(),
            run_id: "run-1".into(),
            request_id: "request-1".into(),
            client_request_id: "allow-empty".into(),
            decision: Some(RuntimeDecision::Allow),
            answers: Some(std::collections::BTreeMap::new()),
        })
        .unwrap_err();
    assert_ne!(empty.code(), "invalid_arg");
    let filled = worker
        .reply(RuntimeReply {
            conversation_id: "empty-answers".into(),
            run_id: "run-1".into(),
            request_id: "request-1".into(),
            client_request_id: "allow-filled".into(),
            decision: Some(RuntimeDecision::Allow),
            answers: Some(std::collections::BTreeMap::from([(
                "q".into(),
                vec!["x".into()],
            )])),
        })
        .unwrap_err();
    assert_eq!(filled.code(), "invalid_arg");
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

#[test]
fn file_and_question_server_requests_become_pending_runtime_requests() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "requests");
    let mut worker = worker(&db, "requests");
    worker.store.enable_if_new("requests").unwrap();
    start_placeholder(&mut worker);

    worker
        .server_request(
            json!("file-1"),
            "item/fileChange/requestApproval",
            &json!({"turnId": "run-1", "reason": "edit readme"}),
        )
        .unwrap();
    worker
        .server_request(
            json!("q-1"),
            "item/tool/requestUserInput",
            &json!({
                "turnId": "run-1",
                "questions": [{
                    "id": "color",
                    "header": "Color",
                    "question": "Pick one",
                    "options": [{"label": "red", "description": ""}],
                    "isOther": false,
                    "isSecret": false
                }]
            }),
        )
        .unwrap();

    let snapshot = worker.store.snapshot("requests", None).unwrap();
    assert_eq!(snapshot.phase, RuntimePhase::Waiting);
    assert_eq!(snapshot.pending_requests.len(), 2);
    assert_eq!(snapshot.pending_requests[0].kind, RuntimeRequestKind::File);
    assert_eq!(snapshot.pending_requests[0].title, "修改文件");
    assert_eq!(snapshot.pending_requests[0].detail, "edit readme");
    assert!(snapshot.pending_requests[0].file_changes.is_empty());
    assert_eq!(
        snapshot.pending_requests[0].permission_options,
        vec![
            RuntimePermissionOption {
                id: "accept".into(),
                kind: "allow_once".into(),
            },
            RuntimePermissionOption {
                id: "accept_always".into(),
                kind: "allow_always".into(),
            },
            RuntimePermissionOption {
                id: "decline".into(),
                kind: "reject_once".into(),
            },
        ]
    );
    assert_eq!(
        snapshot.pending_requests[1].kind,
        RuntimeRequestKind::Question
    );
    assert_eq!(snapshot.pending_requests[1].questions[0].id, "color");
    assert_eq!(
        snapshot.pending_requests[1].questions[0].options[0].label,
        "red"
    );
}

#[test]
fn file_change_request_uses_item_started_paths_when_reason_is_empty() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "file-paths");
    let mut worker = worker(&db, "file-paths");
    worker.store.enable_if_new("file-paths").unwrap();
    start_placeholder(&mut worker);

    worker
        .notification(
            "item/started",
            &json!({
                "item": {
                    "type": "fileChange",
                    "id": "exec-1",
                    "status": "inProgress",
                    "changes": [{
                        "path": "/workspace/qa-codex-filechange-scratch/probe.txt",
                        "kind": { "type": "add" },
                        "diff": "FILECHANGE_OK\n"
                    }]
                }
            }),
        )
        .unwrap();
    worker
        .server_request(
            json!("file-1"),
            "item/fileChange/requestApproval",
            &json!({
                "turnId": "run-1",
                "itemId": "exec-1",
                "reason": null,
                "grantRoot": null
            }),
        )
        .unwrap();

    let snapshot = worker.store.snapshot("file-paths", None).unwrap();
    assert_eq!(snapshot.pending_requests.len(), 1);
    assert_eq!(snapshot.pending_requests[0].kind, RuntimeRequestKind::File);
    assert_eq!(snapshot.pending_requests[0].title, "修改文件");
    assert_eq!(
        snapshot.pending_requests[0].detail,
        "/workspace/qa-codex-filechange-scratch/probe.txt"
    );
    assert_eq!(snapshot.pending_requests[0].file_changes.len(), 1);
    assert_eq!(
        snapshot.pending_requests[0].file_changes[0].path,
        "/workspace/qa-codex-filechange-scratch/probe.txt"
    );
    assert_eq!(
        snapshot.pending_requests[0].file_changes[0].kind.as_deref(),
        Some("add")
    );
    assert_eq!(
        snapshot.pending_requests[0].file_changes[0]
            .preview
            .as_deref(),
        Some("FILECHANGE_OK\n")
    );
    assert!(snapshot.events.iter().any(|event| matches!(
        &event.event,
        ChatEvent::AgentProcess {
            step: crate::models::ProcessStep::Tool { name, status, .. },
            ..
        } if name == "fileChange" && status == "inProgress"
    )));
}

#[test]
fn apply_patch_approval_alias_becomes_file_request() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "legacy-patch");
    let mut worker = worker(&db, "legacy-patch");
    worker.store.enable_if_new("legacy-patch").unwrap();
    start_placeholder(&mut worker);

    worker
        .server_request(
            json!("patch-1"),
            "applyPatchApproval",
            &json!({
                "turnId": "run-1",
                "fileChanges": {
                    "/tmp/example.txt": { "type": "add", "content": "ok" }
                }
            }),
        )
        .unwrap();

    let snapshot = worker.store.snapshot("legacy-patch", None).unwrap();
    assert_eq!(snapshot.pending_requests[0].kind, RuntimeRequestKind::File);
    assert_eq!(snapshot.pending_requests[0].detail, "/tmp/example.txt");
    assert_eq!(snapshot.pending_requests[0].file_changes.len(), 1);
    assert_eq!(
        snapshot.pending_requests[0].file_changes[0]
            .preview
            .as_deref(),
        Some("ok")
    );
}

#[test]
fn file_change_path_only_payload_keeps_empty_preview() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "file-empty");
    let mut worker = worker(&db, "file-empty");
    worker.store.enable_if_new("file-empty").unwrap();
    start_placeholder(&mut worker);

    worker
        .notification(
            "item/started",
            &json!({
                "item": {
                    "type": "fileChange",
                    "id": "exec-empty",
                    "status": "inProgress",
                    "changes": [{
                        "path": "/workspace/notes.md",
                        "kind": { "type": "update" }
                    }]
                }
            }),
        )
        .unwrap();
    worker
        .server_request(
            json!("file-empty"),
            "item/fileChange/requestApproval",
            &json!({
                "turnId": "run-1",
                "itemId": "exec-empty",
                "reason": null,
                "grantRoot": null
            }),
        )
        .unwrap();

    let snapshot = worker.store.snapshot("file-empty", None).unwrap();
    assert_eq!(snapshot.pending_requests[0].detail, "/workspace/notes.md");
    assert_eq!(snapshot.pending_requests[0].file_changes.len(), 1);
    assert_eq!(snapshot.pending_requests[0].file_changes[0].preview, None);
}

#[test]
fn acp_permission_with_file_operation_keeps_protocol_diff() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "acp-file");
    let mut worker = worker(&db, "acp-file");
    worker.agent = AgentId::Grok;
    worker.store.enable_if_new("acp-file").unwrap();
    start_placeholder(&mut worker);

    worker
        .server_request(
            json!("perm-file"),
            "session/request_permission",
            &json!({
                "turnId": "run-1",
                "toolCall": {
                    "title": "edit",
                    "kind": "edit",
                    "rawInput": {
                        "operation": {
                            "type": "update_file",
                            "path": "README.md",
                            "diff": "@@ -1,2 +1,3 @@\n hello\n+world\n"
                        }
                    }
                },
                "options": [
                    {"optionId": "once", "kind": "allow_once"},
                    {"optionId": "reject", "kind": "reject_once"}
                ]
            }),
        )
        .unwrap();

    let snapshot = worker.store.snapshot("acp-file", None).unwrap();
    assert_eq!(snapshot.pending_requests[0].kind, RuntimeRequestKind::File);
    assert_eq!(snapshot.pending_requests[0].title, "修改文件");
    assert_eq!(snapshot.pending_requests[0].file_changes.len(), 1);
    assert_eq!(
        snapshot.pending_requests[0].file_changes[0].path,
        "README.md"
    );
    assert_eq!(
        snapshot.pending_requests[0].file_changes[0]
            .preview
            .as_deref(),
        Some("@@ -1,2 +1,3 @@\n hello\n+world\n")
    );
}

#[test]
fn acp_permission_locations_path_only_stays_file_request() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "acp-path-only");
    let mut worker = worker(&db, "acp-path-only");
    worker.agent = AgentId::Claude;
    worker.store.enable_if_new("acp-path-only").unwrap();
    start_placeholder(&mut worker);

    worker
        .server_request(
            json!("perm-path"),
            "session/request_permission",
            &json!({
                "turnId": "run-1",
                "toolCall": {
                    "title": "Edit notes.md",
                    "kind": "edit",
                    "locations": [{ "path": "/workspace/notes.md" }]
                },
                "options": [
                    {"optionId": "once", "kind": "allow_once"},
                    {"optionId": "always", "kind": "allow_always"},
                    {"optionId": "reject", "kind": "reject_once"}
                ]
            }),
        )
        .unwrap();

    let snapshot = worker.store.snapshot("acp-path-only", None).unwrap();
    assert_eq!(snapshot.pending_requests[0].kind, RuntimeRequestKind::File);
    assert_eq!(snapshot.pending_requests[0].title, "修改文件");
    assert_eq!(snapshot.pending_requests[0].detail, "/workspace/notes.md");
    assert_eq!(snapshot.pending_requests[0].file_changes.len(), 1);
    assert_eq!(
        snapshot.pending_requests[0].file_changes[0].path,
        "/workspace/notes.md"
    );
    assert_eq!(
        snapshot.pending_requests[0].file_changes[0].kind.as_deref(),
        Some("update")
    );
    assert_eq!(snapshot.pending_requests[0].file_changes[0].preview, None);
}

#[test]
fn acp_permission_snapshot_keeps_allow_always_option() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "always");
    let mut worker = worker(&db, "always");
    worker.agent = AgentId::Kiro;
    worker.store.enable_if_new("always").unwrap();
    start_placeholder(&mut worker);

    worker
        .server_request(
            json!("perm-1"),
            "session/request_permission",
            &json!({
                "turnId": "run-1",
                "toolCall": { "title": "写文件" },
                "options": [
                    {"optionId": "once", "kind": "allow_once"},
                    {"optionId": "always", "kind": "allow_always"},
                    {"optionId": "reject", "kind": "reject_once"}
                ]
            }),
        )
        .unwrap();

    let snapshot = worker.store.snapshot("always", None).unwrap();
    assert_eq!(snapshot.pending_requests.len(), 1);
    assert_eq!(
        snapshot.pending_requests[0].permission_options,
        vec![
            RuntimePermissionOption {
                id: "once".into(),
                kind: "allow_once".into(),
            },
            RuntimePermissionOption {
                id: "always".into(),
                kind: "allow_always".into(),
            },
            RuntimePermissionOption {
                id: "reject".into(),
                kind: "reject_once".into(),
            },
        ]
    );
}

#[test]
fn acp_permission_without_options_does_not_synthesize_allow_always() {
    for (id, agent) in [
        ("grok-no-synth", AgentId::Grok),
        ("kiro-no-synth", AgentId::Kiro),
    ] {
        let db = Database::open_in_memory().unwrap();
        conversation(&db, id);
        let mut worker = worker(&db, id);
        worker.agent = agent;
        worker.store.enable_if_new(id).unwrap();
        start_placeholder(&mut worker);

        worker
            .server_request(
                json!("perm-1"),
                "session/request_permission",
                &json!({
                    "turnId": "run-1",
                    "toolCall": { "title": "写文件" }
                }),
            )
            .unwrap();

        let snapshot = worker.store.snapshot(id, None).unwrap();
        assert_eq!(snapshot.pending_requests.len(), 1);
        assert!(
            snapshot.pending_requests[0].permission_options.is_empty(),
            "{agent:?} must not invent allow_always"
        );
    }
}

#[test]
fn acp_permission_uses_server_option_ids_without_auto_allow_always() {
    let options = vec![
        RuntimePermissionOption {
            id: "custom-allow".into(),
            kind: "allow_always".into(),
        },
        RuntimePermissionOption {
            id: "custom-once".into(),
            kind: "allow_once".into(),
        },
        RuntimePermissionOption {
            id: "custom-reject".into(),
            kind: "reject_once".into(),
        },
    ];
    assert_eq!(
        acp_permission_reply(&options, "accept").unwrap(),
        json!({"outcome":{"outcome":"selected","optionId":"custom-once"}})
    );
    assert_eq!(
        acp_permission_reply(&options, "accept_always").unwrap(),
        json!({"outcome":{"outcome":"selected","optionId":"custom-allow"}})
    );
    assert_eq!(
        acp_permission_reply(&options, "decline").unwrap(),
        json!({"outcome":{"outcome":"selected","optionId":"custom-reject"}})
    );
    assert!(acp_permission_reply(&[], "accept").is_err());
    assert!(acp_permission_reply(&[], "accept_always")
        .unwrap_err()
        .to_string()
        .contains("不能一直允许"));
    assert_eq!(
        acp_permission_reply(&[], "decline").unwrap(),
        json!({"outcome":{"outcome":"cancelled"}})
    );
}

#[test]
fn acp_permission_accepts_allow_always_tool_kinds() {
    assert!(is_acp_allow_always_kind("allow_always"));
    assert!(is_acp_allow_always_kind("allow_always_tool"));
    assert!(is_acp_allow_always_kind("allow_always_tool_args"));
    assert!(!is_acp_allow_always_kind("allow_once"));
    assert!(!is_acp_allow_always_kind("allow_edits_for_session"));
    let options = vec![
        RuntimePermissionOption {
            id: "once".into(),
            kind: "allow_once".into(),
        },
        RuntimePermissionOption {
            id: "tool".into(),
            kind: "allow_always_tool".into(),
        },
        RuntimePermissionOption {
            id: "reject".into(),
            kind: "reject_once".into(),
        },
    ];
    assert_eq!(
        acp_permission_reply(&options, "accept_always").unwrap(),
        json!({"outcome":{"outcome":"selected","optionId":"tool"}})
    );
    let args = vec![RuntimePermissionOption {
        id: "args".into(),
        kind: "allow_always_tool_args".into(),
    }];
    assert_eq!(
        acp_permission_reply(&args, "accept_always").unwrap(),
        json!({"outcome":{"outcome":"selected","optionId":"args"}})
    );
}

#[cfg(unix)]
#[test]
fn codex_allow_always_accepts_and_auto_approves_later_command() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "codex-always");
    let mut worker = worker(&db, "codex-always");
    worker.store.enable_if_new("codex-always").unwrap();
    start_placeholder(&mut worker);
    let (_directory, transport, log) = fake_transport();
    worker.transport = Some(transport);
    worker
        .server_request(
            json!("cmd-1"),
            "item/commandExecution/requestApproval",
            &json!({"turnId": "run-1", "command": "ls"}),
        )
        .unwrap();
    let first = worker.store.snapshot("codex-always", None).unwrap();
    assert!(first.pending_requests[0]
        .permission_options
        .iter()
        .any(|option| option.kind == "allow_always"));
    worker
        .reply(RuntimeReply {
            conversation_id: "codex-always".into(),
            run_id: "run-1".into(),
            request_id: first.pending_requests[0].id.clone(),
            client_request_id: "always-1".into(),
            decision: Some(RuntimeDecision::AllowAlways),
            answers: None,
        })
        .unwrap();
    assert!(worker
        .store
        .snapshot("codex-always", None)
        .unwrap()
        .pending_requests
        .is_empty());
    worker
        .server_request(
            json!("cmd-2"),
            "item/commandExecution/requestApproval",
            &json!({"turnId": "run-1", "command": "pwd"}),
        )
        .unwrap();
    assert!(worker
        .store
        .snapshot("codex-always", None)
        .unwrap()
        .pending_requests
        .is_empty());
    std::thread::sleep(Duration::from_millis(80));
    let wire = std::fs::read_to_string(log).unwrap();
    assert!(
        wire.lines().any(|line| line == "acceptForSession"),
        "Codex remember must send acceptForSession: {wire}"
    );
}

#[cfg(unix)]
#[test]
fn codex_allow_always_survives_turn_and_later_file_path() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "codex-file-always");
    let mut worker = worker(&db, "codex-file-always");
    worker.store.enable_if_new("codex-file-always").unwrap();
    start_placeholder(&mut worker);
    let (_directory, transport, log) = fake_transport();
    worker.transport = Some(transport);
    worker
        .server_request(
            json!("file-1"),
            "item/fileChange/requestApproval",
            &json!({
                "turnId": "run-1",
                "changes": [{"path": "/tmp/agenthub-always-allow-codex-347.txt"}]
            }),
        )
        .unwrap();
    let first = worker.store.snapshot("codex-file-always", None).unwrap();
    assert_eq!(first.pending_requests[0].kind, RuntimeRequestKind::File);
    worker
        .reply(RuntimeReply {
            conversation_id: "codex-file-always".into(),
            run_id: "run-1".into(),
            request_id: first.pending_requests[0].id.clone(),
            client_request_id: "always-file-1".into(),
            decision: Some(RuntimeDecision::AllowAlways),
            answers: None,
        })
        .unwrap();
    assert!(worker.session_allow_always);
    worker
        .turn_completed(&json!({"status": "completed"}))
        .unwrap();
    assert!(
        worker.session_allow_always,
        "Codex remember must survive a completed turn"
    );
    let (_directory2, transport2, log2) = fake_transport();
    worker.transport = Some(transport2);
    worker
        .store
        .set_state(
            "codex-file-always",
            RuntimePhase::Running,
            Some("run-2"),
            worker.thread_id.as_deref(),
            Some("turn-2"),
            worker.chat_turn,
            worker.message_id.as_deref(),
        )
        .unwrap();
    worker.run_id = Some("run-2".into());
    worker
        .server_request(
            json!("file-2"),
            "item/fileChange/requestApproval",
            &json!({
                "turnId": "run-2",
                "changes": [{"path": "/tmp/agenthub-always-allow-codex-347-b.txt"}]
            }),
        )
        .unwrap();
    assert!(
        worker
            .store
            .snapshot("codex-file-always", None)
            .unwrap()
            .pending_requests
            .is_empty(),
        "later out-of-cwd write must stay auto-approved"
    );
    std::thread::sleep(Duration::from_millis(80));
    let first_wire = std::fs::read_to_string(log).unwrap();
    let later_wire = std::fs::read_to_string(log2).unwrap();
    assert!(
        first_wire.lines().any(|line| line == "acceptForSession"),
        "first remember must send acceptForSession: {first_wire}"
    );
    assert!(
        later_wire.lines().any(|line| line == "acceptForSession"),
        "later auto-allow must send acceptForSession: {later_wire}"
    );
}

#[cfg(unix)]
#[test]
fn acp_allow_always_forwards_option_and_auto_approves_later_in_live_process() {
    for (id, agent, always_kind) in [
        ("grok-always", AgentId::Grok, "allow_always"),
        ("kiro-always", AgentId::Kiro, "allow_always_tool"),
    ] {
        let db = Database::open_in_memory().unwrap();
        conversation(&db, id);
        let mut worker = worker(&db, id);
        worker.agent = agent;
        worker.store.enable_if_new(id).unwrap();
        start_placeholder(&mut worker);
        let (_directory, transport, log) = fake_transport();
        worker.transport = Some(transport);
        worker
            .server_request(
                json!("perm-1"),
                "session/request_permission",
                &json!({
                    "turnId": "run-1",
                    "toolCall": { "title": "写文件" },
                    "options": [
                        {"optionId": "once", "kind": "allow_once"},
                        {"optionId": "always", "kind": always_kind},
                        {"optionId": "reject", "kind": "reject_once"}
                    ]
                }),
            )
            .unwrap();
        let first = worker.store.snapshot(id, None).unwrap();
        assert!(
            first.pending_requests[0]
                .permission_options
                .iter()
                .any(|option| option.kind == always_kind),
            "{agent:?} must keep server remember kind {always_kind}"
        );
        worker
            .reply(RuntimeReply {
                conversation_id: id.into(),
                run_id: "run-1".into(),
                request_id: first.pending_requests[0].id.clone(),
                client_request_id: "always-1".into(),
                decision: Some(RuntimeDecision::AllowAlways),
                answers: None,
            })
            .unwrap();
        assert!(worker.session_allow_always, "{agent:?}");
        assert!(worker
            .store
            .snapshot(id, None)
            .unwrap()
            .pending_requests
            .is_empty());

        worker
            .server_request(
                json!("perm-2"),
                "session/request_permission",
                &json!({
                    "turnId": "run-1",
                    "toolCall": { "title": "再写文件" },
                    "options": [
                        {"optionId": "once-2", "kind": "allow_once"},
                        {"optionId": "reject-2", "kind": "reject_once"}
                    ]
                }),
            )
            .unwrap();
        assert!(
            worker
                .store
                .snapshot(id, None)
                .unwrap()
                .pending_requests
                .is_empty(),
            "{agent:?} must auto-approve later permission in the live process"
        );

        worker
            .turn_completed(&json!({"stopReason": "end_turn"}))
            .unwrap();
        assert!(
            worker.session_allow_always,
            "{agent:?} remember must survive a completed ACP turn"
        );
        assert!(
            worker
                .transport
                .as_ref()
                .is_some_and(CodexTransport::is_open),
            "{agent:?}"
        );

        worker
            .store
            .set_state(
                id,
                RuntimePhase::Running,
                Some("run-1"),
                worker.thread_id.as_deref(),
                Some("turn-1"),
                worker.chat_turn,
                worker.message_id.as_deref(),
            )
            .unwrap();
        worker
            .server_request(
                json!("perm-3"),
                "session/request_permission",
                &json!({
                    "turnId": "run-1",
                    "toolCall": { "title": "下一轮" },
                    "options": [
                        {"optionId": "once-3", "kind": "allow_once"},
                        {"optionId": "always-3", "kind": always_kind}
                    ]
                }),
            )
            .unwrap();
        assert!(
            worker
                .store
                .snapshot(id, None)
                .unwrap()
                .pending_requests
                .is_empty(),
            "{agent:?} next-turn permission must stay auto-approved"
        );

        std::thread::sleep(Duration::from_millis(80));
        let wire = std::fs::read_to_string(&log).unwrap();
        assert!(
            wire.contains(r#""optionId":"always""#),
            "{agent:?} first reply must forward remember option: {wire}"
        );
        assert!(
            wire.contains(r#""optionId":"once-2""#),
            "{agent:?} later request without remember kind still auto-allows once: {wire}"
        );
        assert!(
            wire.contains(r#""optionId":"always-3""#),
            "{agent:?} later request prefers remember kind: {wire}"
        );
    }
}

#[cfg(unix)]
#[test]
fn acp_allow_once_does_not_remember_later_permissions() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "acp-once");
    let mut worker = worker(&db, "acp-once");
    worker.agent = AgentId::Kiro;
    worker.store.enable_if_new("acp-once").unwrap();
    start_placeholder(&mut worker);
    let (_directory, transport, _log) = fake_transport();
    worker.transport = Some(transport);
    worker
        .server_request(
            json!("perm-1"),
            "session/request_permission",
            &json!({
                "turnId": "run-1",
                "toolCall": { "title": "写文件" },
                "options": [
                    {"optionId": "once", "kind": "allow_once"},
                    {"optionId": "always", "kind": "allow_always_tool"},
                    {"optionId": "reject", "kind": "reject_once"}
                ]
            }),
        )
        .unwrap();
    let first = worker.store.snapshot("acp-once", None).unwrap();
    worker
        .reply(RuntimeReply {
            conversation_id: "acp-once".into(),
            run_id: "run-1".into(),
            request_id: first.pending_requests[0].id.clone(),
            client_request_id: "once-1".into(),
            decision: Some(RuntimeDecision::Allow),
            answers: None,
        })
        .unwrap();
    assert!(!worker.session_allow_always);
    worker
        .server_request(
            json!("perm-2"),
            "session/request_permission",
            &json!({
                "turnId": "run-1",
                "toolCall": { "title": "再写" },
                "options": [
                    {"optionId": "once-2", "kind": "allow_once"},
                    {"optionId": "always-2", "kind": "allow_always_tool"}
                ]
            }),
        )
        .unwrap();
    assert_eq!(
        worker
            .store
            .snapshot("acp-once", None)
            .unwrap()
            .pending_requests
            .len(),
        1
    );
}

#[cfg(unix)]
#[test]
fn acp_session_remember_does_not_invent_allow_when_request_has_no_allow_option() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "acp-no-allow");
    let mut worker = worker(&db, "acp-no-allow");
    worker.agent = AgentId::Grok;
    worker.store.enable_if_new("acp-no-allow").unwrap();
    start_placeholder(&mut worker);
    let (_directory, transport, _log) = fake_transport();
    worker.transport = Some(transport);
    worker.session_allow_always = true;
    worker
        .server_request(
            json!("perm-deny-only"),
            "session/request_permission",
            &json!({
                "turnId": "run-1",
                "toolCall": { "title": "危险操作" },
                "options": [{"optionId": "reject", "kind": "reject_once"}]
            }),
        )
        .unwrap();
    let snapshot = worker.store.snapshot("acp-no-allow", None).unwrap();
    assert_eq!(snapshot.pending_requests.len(), 1);
    assert!(snapshot.pending_requests[0]
        .permission_options
        .iter()
        .all(|option| option.kind != "allow_always"));
}

#[test]
fn acp_cancel_wins_over_late_end_turn() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "cancel-race");
    let mut worker = worker(&db, "cancel-race");
    worker.store.enable_if_new("cancel-race").unwrap();
    start_placeholder(&mut worker);
    worker.agent = AgentId::Kiro;
    worker.cancel_deadline = Some(Instant::now() + Duration::from_secs(10));
    worker
        .store
        .set_state(
            "cancel-race",
            RuntimePhase::Cancelling,
            Some("run-1"),
            Some("session-1"),
            Some("turn-1"),
            worker.chat_turn,
            worker.message_id.as_deref(),
        )
        .unwrap();
    worker
        .turn_completed(&json!({"stopReason":"end_turn"}))
        .unwrap();
    let snapshot = worker.store.snapshot("cancel-race", None).unwrap();
    assert_eq!(snapshot.phase, RuntimePhase::Cancelled);
    assert_eq!(
        snapshot.current_message.unwrap().status,
        ChatMessageStatus::Cancelled
    );
}

#[test]
fn acp_cancel_deadline_terminalizes_without_a_server_response() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "cancel-deadline");
    let mut worker = worker(&db, "cancel-deadline");
    worker.store.enable_if_new("cancel-deadline").unwrap();
    start_placeholder(&mut worker);
    worker.agent = AgentId::Kiro;
    worker.cancel_deadline = Some(Instant::now() - Duration::from_millis(1));
    worker
        .store
        .set_state(
            "cancel-deadline",
            RuntimePhase::Cancelling,
            Some("run-1"),
            Some("session-1"),
            Some("turn-1"),
            worker.chat_turn,
            worker.message_id.as_deref(),
        )
        .unwrap();
    worker.check_cancel_deadline().unwrap();
    let snapshot = worker.store.snapshot("cancel-deadline", None).unwrap();
    assert_eq!(snapshot.phase, RuntimePhase::Interrupted);
    assert!(snapshot.events.iter().any(|event| {
        matches!(
            &event.event,
            ChatEvent::Error { message } if message.contains("请新建对话")
        )
    }));
    assert!(!snapshot
        .events
        .iter()
        .any(|event| { matches!(event.event, ChatEvent::Finished { ok: true, .. }) }));
}

#[test]
fn late_acp_permission_after_completion_is_cancelled_without_waiting() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "late-permission");
    let mut worker = worker(&db, "late-permission");
    worker.store.enable_if_new("late-permission").unwrap();
    start_placeholder(&mut worker);
    worker.agent = AgentId::Kiro;
    worker
        .store
        .set_state(
            "late-permission",
            RuntimePhase::Completed,
            None,
            Some("session-1"),
            None,
            worker.chat_turn,
            worker.message_id.as_deref(),
        )
        .unwrap();
    worker
        .server_request(
            json!("late-1"),
            "session/request_permission",
            &json!({
                "sessionId": "session-1",
                "turnId": "run-1",
                "options": [{"optionId":"allow","kind":"allow_once"}]
            }),
        )
        .unwrap();
    assert!(worker
        .store
        .snapshot("late-permission", None)
        .unwrap()
        .pending_requests
        .is_empty());

    worker
        .store
        .set_state(
            "late-permission",
            RuntimePhase::Cancelling,
            Some("run-1"),
            Some("session-1"),
            Some("turn-1"),
            worker.chat_turn,
            worker.message_id.as_deref(),
        )
        .unwrap();
    worker
        .server_request(
            json!("late-2"),
            "session/request_permission",
            &json!({
                "sessionId": "session-1",
                "turnId": "run-1",
                "options": [{"optionId":"allow","kind":"allow_once"}]
            }),
        )
        .unwrap();
    assert!(worker
        .store
        .snapshot("late-permission", None)
        .unwrap()
        .pending_requests
        .is_empty());
}

#[test]
fn acp_stop_reasons_never_default_to_success() {
    for (index, reason) in ["max_tokens", "max_turn_requests", "refusal", "unknown"]
        .iter()
        .enumerate()
    {
        let id = format!("stop-reason-{index}");
        let db = Database::open_in_memory().unwrap();
        conversation(&db, &id);
        let mut worker = worker(&db, &id);
        worker.store.enable_if_new(&id).unwrap();
        start_placeholder(&mut worker);
        worker.agent = AgentId::Kiro;
        worker
            .turn_completed(&json!({"stopReason": reason}))
            .unwrap();
        let snapshot = worker.store.snapshot(&id, None).unwrap();
        assert_eq!(snapshot.phase, RuntimePhase::Failed, "{reason}");
        assert_eq!(
            snapshot.current_message.unwrap().status,
            ChatMessageStatus::Failed,
            "{reason}"
        );
        assert!(!snapshot
            .events
            .iter()
            .any(|event| { matches!(event.event, ChatEvent::Finished { ok: true, .. }) }));
    }
}

#[test]
fn dead_kiro_process_rejects_existing_session_without_replacing_thread_id() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "dead-kiro");
    let mut worker = worker(&db, "dead-kiro");
    worker.agent = AgentId::Kiro;
    worker.store.enable_if_new("dead-kiro").unwrap();
    worker.thread_id = Some("kiro-session-1".into());
    worker
        .store
        .set_state(
            "dead-kiro",
            RuntimePhase::Completed,
            None,
            worker.thread_id.as_deref(),
            None,
            None,
            None,
        )
        .unwrap();
    let error = worker.acp_connect_and_prompt(Vec::new()).unwrap_err();
    assert!(error.to_string().contains("新建对话"));
    assert_eq!(worker.thread_id.as_deref(), Some("kiro-session-1"));
    assert_eq!(
        worker
            .store
            .record("dead-kiro")
            .unwrap()
            .unwrap()
            .thread_id
            .as_deref(),
        Some("kiro-session-1")
    );
}

#[cfg(unix)]
fn write_fake_codex(directory: &std::path::Path, script: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let program = directory.join("fake-codex");
    std::fs::write(&program, script).unwrap();
    let mut permissions = std::fs::metadata(&program).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&program, permissions).unwrap();
    program
}

#[cfg(unix)]
fn wait_for_file(path: &std::path::Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.is_file() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {}", path.display());
}

#[cfg(unix)]
#[test]
fn poll_events_drains_a_burst_without_starving_later_commands() {
    let directory = tempfile::tempdir().unwrap();
    let ready = directory.path().join("ready");
    let ready_path = ready.display().to_string();
    let program = write_fake_codex(
        directory.path(),
        &format!(
            r##"#!/bin/sh
IFS= read -r initialize
printf '%s\n' '{{"id":1,"result":{{"initialized":true}}}}'
i=0
while [ "$i" -lt 200 ]; do
  printf '%s\n' '{{"method":"item/agentMessage/delta","params":{{"delta":"x"}}}}'
  i=$((i + 1))
done
printf '%s\n' ready > "{ready_path}"
while IFS= read -r _; do
  :
done
"##
        ),
    );
    let transport = CodexTransport::spawn(&program, directory.path()).unwrap();
    wait_for_file(&ready, Duration::from_secs(2));

    let db = Database::open_in_memory().unwrap();
    conversation(&db, "burst");
    let mut worker = worker(&db, "burst");
    worker.store.enable_if_new("burst").unwrap();
    start_placeholder(&mut worker);
    worker.transport = Some(transport);
    worker.poll_events().unwrap();
    let first = worker
        .store
        .snapshot("burst", None)
        .unwrap()
        .current_message
        .unwrap()
        .content
        .len();
    assert_eq!(
        first, 64,
        "one poll should take a 64-event batch, got {first}"
    );
    worker.poll_events().unwrap();
    let second = worker
        .store
        .snapshot("burst", None)
        .unwrap()
        .current_message
        .unwrap()
        .content
        .len();
    assert_eq!(second, 128);
    worker
        .abort
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let started = Instant::now();
    worker.poll_events().unwrap();
    assert!(started.elapsed() < Duration::from_millis(200));
}

#[cfg(unix)]
#[test]
fn shutdown_preempts_blocking_catalog_fetch() {
    let directory = tempfile::tempdir().unwrap();
    let ready = directory.path().join("blocked");
    let ready_path = ready.display().to_string();
    let program = write_fake_codex(
        directory.path(),
        &format!(
            r##"#!/bin/sh
if [ "$1" = "--version" ]; then
  printf '%s\n' 'codex 0.0.1'
  exit 0
fi
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' '{{"id":1,"result":{{"initialized":true}}}}'
      ;;
    *'"method":"model/list"'*)
      printf '%s\n' blocked > "{ready_path}"
      sleep 30
      printf '%s\n' '{{"id":2,"result":{{"data":[]}}}}'
      ;;
  esac
done
"##
        ),
    );
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "catalog-cancel");
    let runtime = Arc::new(ChatRuntime::new(
        db,
        Arc::new(RunService::new(AdapterRegistry::default())),
    ));
    runtime.store.enable_if_new("catalog-cancel").unwrap();
    runtime.set_codex_program_for_test(program);
    let started = Arc::clone(&runtime);
    let handle = std::thread::spawn(move || {
        started.start(
            "catalog-cancel",
            "hello",
            "client-catalog",
            RuntimeStartExtras::default(),
        )
    });
    wait_for_file(&ready, Duration::from_secs(3));
    let shutdown_started = Instant::now();
    runtime.shutdown("catalog-cancel");
    assert!(
        shutdown_started.elapsed() < Duration::from_secs(5),
        "shutdown waited {:?}",
        shutdown_started.elapsed()
    );
    let outcome = handle.join().expect("start thread");
    assert!(outcome.is_err(), "start should fail after shutdown");
}

#[cfg(unix)]
#[test]
fn cancel_preempts_blocking_turn_start() {
    let directory = tempfile::tempdir().unwrap();
    let ready = directory.path().join("blocked");
    let ready_path = ready.display().to_string();
    let program = write_fake_codex(
        directory.path(),
        &format!(
            r##"#!/bin/sh
if [ "$1" = "--version" ]; then
  printf '%s\n' 'codex 0.0.1'
  exit 0
fi
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' '{{"id":1,"result":{{"initialized":true}}}}'
      ;;
    *'"method":"thread/start"'*)
      printf '%s\n' '{{"id":2,"result":{{"thread":{{"id":"thread-1"}}}}}}'
      ;;
    *'"method":"turn/start"'*)
      printf '%s\n' blocked > "{ready_path}"
      sleep 30
      printf '%s\n' '{{"id":3,"result":{{"turn":{{"id":"turn-1"}}}}}}'
      ;;
  esac
done
"##
        ),
    );
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "turn-cancel");
    let runtime = Arc::new(ChatRuntime::new(
        db,
        Arc::new(RunService::new(AdapterRegistry::default())),
    ));
    runtime.store.enable_if_new("turn-cancel").unwrap();
    runtime.set_codex_program_for_test(program);
    runtime.seed_catalog_cache_for_test(
        "turn-cancel",
        vec![RuntimeModelOption {
            id: "gpt-test".into(),
            efforts: vec!["low".into()],
            default_effort: Some("low".into()),
        }],
        vec![],
    );
    let started = Arc::clone(&runtime);
    let handle = std::thread::spawn(move || {
        started.start(
            "turn-cancel",
            "hello",
            "client-turn",
            RuntimeStartExtras::default(),
        )
    });
    wait_for_file(&ready, Duration::from_secs(3));
    let cancel_started = Instant::now();
    runtime.cancel("turn-cancel", "").unwrap();
    assert!(
        cancel_started.elapsed() < Duration::from_secs(5),
        "cancel waited {:?}",
        cancel_started.elapsed()
    );
    let outcome = handle.join().expect("start thread");
    assert!(outcome.is_err(), "start should fail after cancel");
    let snapshot = runtime.snapshot("turn-cancel", None).unwrap();
    assert!(
        matches!(
            snapshot.phase,
            RuntimePhase::Cancelled | RuntimePhase::Interrupted
        ),
        "phase {:?}",
        snapshot.phase
    );
}

fn captured_has_op(logs: &str, op: &str) -> bool {
    logs.contains(&format!("op=\"{op}\""))
}

#[test]
fn runtime_logs_send_start_and_fail_without_codex() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "log-send");
    let mut worker = worker(&db, "log-send");
    worker.store.enable_if_new("log-send").unwrap();
    *worker.codex_program_override.lock().unwrap() =
        Some(std::path::PathBuf::from("/nonexistent-agenthub-codex"));
    let prompt = "secret-prompt-must-not-appear";
    let (result, logs) = with_captured_logs(|| {
        worker.start_turn(prompt, "client-log-send", &RuntimeStartExtras::default())
    });
    assert!(result.is_err(), "missing Codex must fail start: {result:?}");
    assert!(logs.contains("core.chat"), "logs:\n{logs}");
    assert!(logs.contains("send start"), "logs:\n{logs}");
    assert!(captured_has_op(&logs, "send"), "logs:\n{logs}");
    assert!(captured_has_op(&logs, "send_fail"), "logs:\n{logs}");
    assert!(logs.contains("log-send"), "logs:\n{logs}");
    assert!(logs.contains("codex"), "logs:\n{logs}");
    assert!(!logs.contains(prompt), "must not log prompt:\n{logs}");
}

#[test]
fn runtime_logs_send_ok_when_turn_completes() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "log-ok");
    let mut worker = worker(&db, "log-ok");
    worker.store.enable_if_new("log-ok").unwrap();
    start_placeholder(&mut worker);
    let (result, logs) = with_captured_logs(|| {
        worker.terminalize(
            ChatMessageStatus::Ok,
            None,
            RuntimePhase::Completed,
            true,
            false,
        )
    });
    result.unwrap();
    assert!(logs.contains("core.chat"), "logs:\n{logs}");
    assert!(logs.contains("send ok"), "logs:\n{logs}");
    assert!(captured_has_op(&logs, "send"), "logs:\n{logs}");
    assert!(!captured_has_op(&logs, "send_fail"), "logs:\n{logs}");
}

#[test]
fn runtime_logs_stop_when_cancel_has_no_transport() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "log-stop");
    let mut worker = worker(&db, "log-stop");
    worker.store.enable_if_new("log-stop").unwrap();
    start_placeholder(&mut worker);
    let (result, logs) = with_captured_logs(|| worker.cancel("run-1"));
    result.unwrap();
    assert!(logs.contains("core.chat"), "logs:\n{logs}");
    assert!(logs.contains("stop ok"), "logs:\n{logs}");
    assert!(captured_has_op(&logs, "stop"), "logs:\n{logs}");
    assert!(!captured_has_op(&logs, "stop_fail"), "logs:\n{logs}");
    assert!(!captured_has_op(&logs, "send_fail"), "logs:\n{logs}");
}

#[test]
fn runtime_logs_stop_fail_when_cancel_fails() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "log-stop-fail");
    let mut worker = worker(&db, "log-stop-fail");
    worker.store.enable_if_new("log-stop-fail").unwrap();
    start_placeholder(&mut worker);
    let (result, logs) = with_captured_logs(|| {
        worker.cancel_failed(AppError::message("chat.runtime", "interrupt failed"))
    });
    assert!(result.is_err());
    assert!(logs.contains("core.chat"), "logs:\n{logs}");
    assert!(captured_has_op(&logs, "stop_fail"), "logs:\n{logs}");
    assert!(!logs.contains("interrupt failed") || logs.contains("stop_fail"));
}

#[test]
fn abort_during_cancel_logs_stop_ok_not_transport_fail() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "log-stop-abort");
    let mut worker = worker(&db, "log-stop-abort");
    worker.store.enable_if_new("log-stop-abort").unwrap();
    start_placeholder(&mut worker);
    worker.abort.store(true, Ordering::SeqCst);
    let (result, logs) = with_captured_logs(|| {
        worker.complete_cancel(AppError::message(
            "chat.runtime.transport",
            "codex app-server request interrupted",
        ))
    });
    result.unwrap();
    assert!(logs.contains("core.chat"), "logs:\n{logs}");
    assert!(logs.contains("stop ok"), "logs:\n{logs}");
    assert!(captured_has_op(&logs, "stop"), "logs:\n{logs}");
    assert!(!captured_has_op(&logs, "stop_fail"), "logs:\n{logs}");
    assert!(!captured_has_op(&logs, "send_fail"), "logs:\n{logs}");
    let snapshot = worker.store.snapshot("log-stop-abort", None).unwrap();
    assert_eq!(snapshot.phase, RuntimePhase::Cancelled);
    assert_eq!(
        snapshot.current_message.unwrap().status,
        ChatMessageStatus::Cancelled
    );
}

#[test]
fn transport_interrupt_without_abort_still_logs_stop_fail() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "log-stop-transport");
    let mut worker = worker(&db, "log-stop-transport");
    worker.store.enable_if_new("log-stop-transport").unwrap();
    start_placeholder(&mut worker);
    let (result, logs) = with_captured_logs(|| {
        worker.complete_cancel(AppError::message(
            "chat.runtime.transport",
            "codex app-server I/O failed",
        ))
    });
    assert!(result.is_err());
    assert!(captured_has_op(&logs, "stop_fail"), "logs:\n{logs}");
    assert!(!logs.contains("stop ok"), "logs:\n{logs}");
}

#[cfg(unix)]
#[test]
fn stop_while_waiting_for_approval_logs_stop_ok() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "stop-waiting");
    let mut worker = worker(&db, "stop-waiting");
    worker.store.enable_if_new("stop-waiting").unwrap();
    start_placeholder(&mut worker);
    worker
        .store
        .set_state(
            "stop-waiting",
            RuntimePhase::Waiting,
            Some("run-1"),
            worker.thread_id.as_deref(),
            worker.turn_id.as_deref(),
            worker.chat_turn,
            worker.message_id.as_deref(),
        )
        .unwrap();
    worker
        .store
        .add_request(
            "stop-waiting",
            &RuntimeRequest {
                id: "request-1".into(),
                run_id: "run-1".into(),
                kind: RuntimeRequestKind::Command,
                title: "执行命令".into(),
                detail: "safe".into(),
                questions: Vec::new(),
                permission_options: Vec::new(),
                file_changes: Vec::new(),
            },
            "item/commandExecution/requestApproval",
            "server-wait",
        )
        .unwrap();
    let abort = Arc::clone(&worker.abort);
    let (_directory, transport, log) = fake_transport_with(Some(abort));
    worker.transport = Some(transport);
    worker.abort.store(true, Ordering::SeqCst);
    let (result, logs) = with_captured_logs(|| worker.cancel("run-1"));
    result.unwrap();
    assert!(logs.contains("stop ok"), "logs:\n{logs}");
    assert!(captured_has_op(&logs, "stop"), "logs:\n{logs}");
    assert!(!captured_has_op(&logs, "stop_fail"), "logs:\n{logs}");
    let snapshot = worker.store.snapshot("stop-waiting", None).unwrap();
    assert_eq!(snapshot.phase, RuntimePhase::Cancelled);
    assert!(snapshot.pending_requests.is_empty());
    assert_eq!(
        snapshot.current_message.unwrap().status,
        ChatMessageStatus::Cancelled
    );
    let wire = std::fs::read_to_string(log).unwrap_or_default();
    assert!(
        !wire.lines().any(|line| line == "accept"),
        "stop must not allow the pending command:\n{wire}"
    );
}

#[test]
fn claude_stream_result_completes_turn_and_keeps_session() {
    let db = Database::open_in_memory().unwrap();
    ChatRepo::new(db.clone())
        .create_conversation(&Conversation {
            id: "claude-stream".into(),
            title: String::new(),
            agent_ids: vec![AgentId::Claude],
            cwd: Some(std::env::temp_dir().to_string_lossy().into_owned()),
            allow_dangerous: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            native_session_id: None,
            sending: false,
            first_user_content: None,
        })
        .unwrap();
    let mut worker = worker(&db, "claude-stream");
    worker.agent = AgentId::Claude;
    worker.store.enable_if_new("claude-stream").unwrap();
    start_placeholder(&mut worker);
    worker.agent = AgentId::Claude;
    worker
        .notification(
            "claude/stream",
            &json!({
                "type": "system",
                "subtype": "init",
                "session_id": "claude-sess-1"
            }),
        )
        .unwrap();
    assert_eq!(worker.thread_id.as_deref(), Some("claude-sess-1"));
    worker
        .notification(
            "claude/stream",
            &json!({
                "type": "assistant",
                "session_id": "claude-sess-1",
                "message": {
                    "role": "assistant",
                    "content": [{"type": "text", "text": "PONG"}]
                }
            }),
        )
        .unwrap();
    worker
        .notification(
            "claude/stream",
            &json!({
                "type": "result",
                "subtype": "success",
                "is_error": false,
                "result": "PONG",
                "session_id": "claude-sess-1"
            }),
        )
        .unwrap();
    assert_eq!(worker.thread_id.as_deref(), Some("claude-sess-1"));
    let snapshot = worker.store.snapshot("claude-stream", None).unwrap();
    assert_eq!(snapshot.phase, RuntimePhase::Completed);
    let message = snapshot.current_message.unwrap();
    assert_eq!(message.status, ChatMessageStatus::Ok);
    assert!(
        message.content.contains("PONG"),
        "content={}",
        message.content
    );
}

#[cfg(unix)]
#[test]
fn grok_fs_write_outside_cwd_emits_card_then_writes_on_allow() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let write_path = outside.path().join("agenthub-always-allow-grok-347.txt");
    let db = Database::open_in_memory().unwrap();
    conversation_with(&db, "grok-fs-out", AgentId::Grok, project.path());
    let mut worker = worker(&db, "grok-fs-out");
    worker.agent = AgentId::Grok;
    worker.store.enable_if_new("grok-fs-out").unwrap();
    start_placeholder(&mut worker);
    let (_directory, transport, _log) = fake_transport();
    worker.transport = Some(transport);

    worker
        .server_request(
            json!("fs-1"),
            "fs/write_text_file",
            &json!({
                "sessionId": "thread-1",
                "path": write_path.to_string_lossy(),
                "content": "first-write"
            }),
        )
        .unwrap();
    assert!(
        !write_path.exists(),
        "out-of-cwd write must wait for the card"
    );
    let first = worker.store.snapshot("grok-fs-out", None).unwrap();
    assert_eq!(first.pending_requests.len(), 1);
    assert_eq!(first.pending_requests[0].kind, RuntimeRequestKind::File);
    assert_eq!(first.pending_requests[0].title, "修改文件");
    assert!(first.pending_requests[0]
        .permission_options
        .iter()
        .any(|option| option.kind == "allow_always"));

    worker
        .reply(RuntimeReply {
            conversation_id: "grok-fs-out".into(),
            run_id: "run-1".into(),
            request_id: first.pending_requests[0].id.clone(),
            client_request_id: "allow-fs-1".into(),
            decision: Some(RuntimeDecision::AllowAlways),
            answers: None,
        })
        .unwrap();
    assert_eq!(std::fs::read_to_string(&write_path).unwrap(), "first-write");
    assert!(worker.session_allow_always);

    let later = outside.path().join("agenthub-always-allow-grok-347-b.txt");
    worker
        .server_request(
            json!("fs-2"),
            "fs/write_text_file",
            &json!({
                "sessionId": "thread-1",
                "path": later.to_string_lossy(),
                "content": "second-write"
            }),
        )
        .unwrap();
    assert!(
        worker
            .store
            .snapshot("grok-fs-out", None)
            .unwrap()
            .pending_requests
            .is_empty(),
        "later out-of-cwd write must stay auto-approved"
    );
    assert_eq!(std::fs::read_to_string(&later).unwrap(), "second-write");
}

#[cfg(unix)]
#[test]
fn grok_fs_write_inside_cwd_writes_without_card() {
    let project = tempfile::tempdir().unwrap();
    let write_path = project.path().join("inside.txt");
    let db = Database::open_in_memory().unwrap();
    conversation_with(&db, "grok-fs-in", AgentId::Grok, project.path());
    let mut worker = worker(&db, "grok-fs-in");
    worker.agent = AgentId::Grok;
    worker.store.enable_if_new("grok-fs-in").unwrap();
    start_placeholder(&mut worker);
    let (_directory, transport, _log) = fake_transport();
    worker.transport = Some(transport);

    worker
        .server_request(
            json!("fs-in"),
            "fs/write_text_file",
            &json!({
                "sessionId": "thread-1",
                "path": write_path.to_string_lossy(),
                "content": "cwd-ok"
            }),
        )
        .unwrap();
    assert!(worker
        .store
        .snapshot("grok-fs-in", None)
        .unwrap()
        .pending_requests
        .is_empty());
    assert_eq!(std::fs::read_to_string(&write_path).unwrap(), "cwd-ok");
}

#[cfg(unix)]
#[test]
fn grok_fs_write_deny_does_not_create_file() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let write_path = outside.path().join("denied.txt");
    let db = Database::open_in_memory().unwrap();
    conversation_with(&db, "grok-fs-deny", AgentId::Grok, project.path());
    let mut worker = worker(&db, "grok-fs-deny");
    worker.agent = AgentId::Grok;
    worker.store.enable_if_new("grok-fs-deny").unwrap();
    start_placeholder(&mut worker);
    let (_directory, transport, _log) = fake_transport();
    worker.transport = Some(transport);

    worker
        .server_request(
            json!("fs-deny"),
            "fs/write_text_file",
            &json!({
                "sessionId": "thread-1",
                "path": write_path.to_string_lossy(),
                "content": "nope"
            }),
        )
        .unwrap();
    let first = worker.store.snapshot("grok-fs-deny", None).unwrap();
    worker
        .reply(RuntimeReply {
            conversation_id: "grok-fs-deny".into(),
            run_id: "run-1".into(),
            request_id: first.pending_requests[0].id.clone(),
            client_request_id: "deny-fs-1".into(),
            decision: Some(RuntimeDecision::Deny),
            answers: None,
        })
        .unwrap();
    assert!(!write_path.exists());
    assert!(!worker.session_allow_always);
}
