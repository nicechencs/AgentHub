use super::types::{RuntimeLocalImage, RuntimeStartExtras};
use super::*;
use crate::models::{AgentId, ChatEvent, ChatRole, Conversation};
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
fn begin_turn_bumps_conversation_sort_time_without_renaming() {
    let db = Database::open_in_memory().unwrap();
    let repo = ChatRepo::new(db.clone());
    let older = Conversation {
        id: "older".into(),
        title: "kept title".into(),
        agent_ids: vec![AgentId::Codex],
        cwd: Some(std::env::temp_dir().to_string_lossy().into_owned()),
        allow_dangerous: false,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
        native_session_id: None,
        sending: false,
    };
    let newer = Conversation {
        id: "newer".into(),
        title: "other".into(),
        agent_ids: vec![AgentId::Codex],
        cwd: older.cwd.clone(),
        allow_dangerous: false,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-06-01T00:00:00Z".into(),
        native_session_id: None,
        sending: false,
    };
    repo.create_conversation(&older).unwrap();
    repo.create_conversation(&newer).unwrap();
    let store = super::store::RuntimeStore::new(db);
    store.enable_if_new("older").unwrap();
    store.enable_if_new("newer").unwrap();
    let listed = repo.list_conversations().unwrap();
    assert_eq!(listed[0].id, "newer");
    assert_eq!(listed[1].id, "older");

    let now = "2026-01-01T00:00:00Z".to_string();
    let mut user = crate::models::ChatMessage {
        id: "user-1".into(),
        conversation_id: "older".into(),
        turn: 0,
        role: ChatRole::User,
        agent_id: None,
        content: "hello from older".into(),
        status: crate::models::ChatMessageStatus::Ok,
        exit_code: None,
        duration_ms: 0,
        error: None,
        created_at: now.clone(),
    };
    let mut agent = crate::models::ChatMessage {
        id: "agent-1".into(),
        conversation_id: "older".into(),
        turn: 0,
        role: ChatRole::Agent,
        agent_id: Some(AgentId::Codex),
        content: String::new(),
        status: crate::models::ChatMessageStatus::Running,
        exit_code: None,
        duration_ms: 0,
        error: None,
        created_at: now,
    };
    store
        .begin_turn("older", &mut user, &mut agent, "run-1", None, |turn| {
            vec![ChatEvent::Started {
                turn,
                agents: vec![AgentId::Codex],
            }]
        })
        .unwrap();

    let listed = repo.list_conversations().unwrap();
    assert_eq!(listed[0].id, "older");
    assert_eq!(listed[0].title, "kept title");
    assert_eq!(listed[1].id, "newer");
}

#[test]
fn begin_turn_sets_title_when_empty_and_still_bumps_sort_time() {
    let db = Database::open_in_memory().unwrap();
    let repo = ChatRepo::new(db.clone());
    conversation(&db, "untitled", false);
    conversation(&db, "other", false);
    repo.update_conversation(&Conversation {
        id: "other".into(),
        title: "named".into(),
        agent_ids: vec![AgentId::Codex],
        cwd: Some(std::env::temp_dir().to_string_lossy().into_owned()),
        allow_dangerous: false,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-06-01T00:00:00Z".into(),
        native_session_id: None,
        sending: false,
    })
    .unwrap();
    let store = super::store::RuntimeStore::new(db);
    store.enable_if_new("untitled").unwrap();
    let now = "2026-01-01T00:00:00Z".to_string();
    let mut user = crate::models::ChatMessage {
        id: "user-1".into(),
        conversation_id: "untitled".into(),
        turn: 0,
        role: ChatRole::User,
        agent_id: None,
        content: "first prompt for title".into(),
        status: crate::models::ChatMessageStatus::Ok,
        exit_code: None,
        duration_ms: 0,
        error: None,
        created_at: now.clone(),
    };
    let mut agent = crate::models::ChatMessage {
        id: "agent-1".into(),
        conversation_id: "untitled".into(),
        turn: 0,
        role: ChatRole::Agent,
        agent_id: Some(AgentId::Codex),
        content: String::new(),
        status: crate::models::ChatMessageStatus::Running,
        exit_code: None,
        duration_ms: 0,
        error: None,
        created_at: now,
    };
    store
        .begin_turn("untitled", &mut user, &mut agent, "run-1", None, |turn| {
            vec![ChatEvent::Started {
                turn,
                agents: vec![AgentId::Codex],
            }]
        })
        .unwrap();
    let untitled = repo.get_conversation("untitled").unwrap().unwrap();
    assert_eq!(untitled.title, "first prompt for title");
    let listed = repo.list_conversations().unwrap();
    assert_eq!(listed[0].id, "untitled");
}

#[test]
fn empty_grok_conversation_enables_runtime() {
    let db = Database::open_in_memory().unwrap();
    let now = "2026-01-01T00:00:00Z".to_string();
    let empty = Conversation {
        id: "grok-empty".into(),
        title: String::new(),
        agent_ids: vec![AgentId::Grok],
        cwd: Some(std::env::temp_dir().to_string_lossy().into_owned()),
        allow_dangerous: false,
        created_at: now.clone(),
        updated_at: now.clone(),
        native_session_id: None,
        sending: false,
    };
    let legacy = Conversation {
        id: "grok-legacy".into(),
        title: String::new(),
        agent_ids: vec![AgentId::Grok],
        cwd: empty.cwd.clone(),
        allow_dangerous: false,
        created_at: now.clone(),
        updated_at: now,
        native_session_id: None,
        sending: false,
    };
    let repo = ChatRepo::new(db.clone());
    repo.create_conversation(&empty).unwrap();
    repo.create_conversation(&legacy).unwrap();
    repo.insert_message(&crate::models::ChatMessage {
        id: "legacy-user".into(),
        conversation_id: "grok-legacy".into(),
        turn: 1,
        role: crate::models::ChatRole::User,
        agent_id: None,
        content: "legacy".into(),
        status: crate::models::ChatMessageStatus::Ok,
        exit_code: None,
        duration_ms: 0,
        error: None,
        created_at: "2026-01-01T00:00:00Z".into(),
    })
    .unwrap();
    let store = super::store::RuntimeStore::new(db);
    store.enable_if_new("grok-empty").unwrap();
    assert!(store.snapshot("grok-empty", None).unwrap().enabled);
    assert!(store.enable_if_new("grok-legacy").is_err());
}

#[test]
fn empty_kiro_conversation_enables_runtime() {
    let db = Database::open_in_memory().unwrap();
    let now = "2026-01-01T00:00:00Z".to_string();
    let empty = Conversation {
        id: "kiro-empty".into(),
        title: String::new(),
        agent_ids: vec![AgentId::Kiro],
        cwd: Some(std::env::temp_dir().to_string_lossy().into_owned()),
        allow_dangerous: false,
        created_at: now.clone(),
        updated_at: now.clone(),
        native_session_id: None,
        sending: false,
    };
    let legacy = Conversation {
        id: "kiro-legacy".into(),
        title: String::new(),
        agent_ids: vec![AgentId::Kiro],
        cwd: empty.cwd.clone(),
        allow_dangerous: false,
        created_at: now.clone(),
        updated_at: now,
        native_session_id: None,
        sending: false,
    };
    let repo = ChatRepo::new(db.clone());
    repo.create_conversation(&empty).unwrap();
    repo.create_conversation(&legacy).unwrap();
    repo.insert_message(&crate::models::ChatMessage {
        id: "legacy-user".into(),
        conversation_id: "kiro-legacy".into(),
        turn: 1,
        role: crate::models::ChatRole::User,
        agent_id: None,
        content: "legacy".into(),
        status: crate::models::ChatMessageStatus::Ok,
        exit_code: None,
        duration_ms: 0,
        error: None,
        created_at: "2026-01-01T00:00:00Z".into(),
    })
    .unwrap();
    let store = super::store::RuntimeStore::new(db);
    store.enable_if_new("kiro-empty").unwrap();
    assert!(store.snapshot("kiro-empty", None).unwrap().enabled);
    assert!(store.enable_if_new("kiro-legacy").is_err());
}

#[test]
fn empty_claude_conversation_enables_runtime() {
    let db = Database::open_in_memory().unwrap();
    let now = "2026-01-01T00:00:00Z".to_string();
    let empty = Conversation {
        id: "claude-empty".into(),
        title: String::new(),
        agent_ids: vec![AgentId::Claude],
        cwd: Some(std::env::temp_dir().to_string_lossy().into_owned()),
        allow_dangerous: false,
        created_at: now.clone(),
        updated_at: now.clone(),
        native_session_id: None,
        sending: false,
    };
    let legacy = Conversation {
        id: "claude-legacy".into(),
        title: String::new(),
        agent_ids: vec![AgentId::Claude],
        cwd: empty.cwd.clone(),
        allow_dangerous: false,
        created_at: now.clone(),
        updated_at: now,
        native_session_id: None,
        sending: false,
    };
    let repo = ChatRepo::new(db.clone());
    repo.create_conversation(&empty).unwrap();
    repo.create_conversation(&legacy).unwrap();
    repo.insert_message(&crate::models::ChatMessage {
        id: "legacy-user".into(),
        conversation_id: "claude-legacy".into(),
        turn: 1,
        role: crate::models::ChatRole::User,
        agent_id: None,
        content: "legacy".into(),
        status: crate::models::ChatMessageStatus::Ok,
        exit_code: None,
        duration_ms: 0,
        error: None,
        created_at: "2026-01-01T00:00:00Z".into(),
    })
    .unwrap();
    let store = super::store::RuntimeStore::new(db);
    store.enable_if_new("claude-empty").unwrap();
    assert!(store.snapshot("claude-empty", None).unwrap().enabled);
    assert!(store.enable_if_new("claude-legacy").is_err());
}

#[test]
fn grok_legacy_continue_requires_session_and_keeps_print_path_otherwise() {
    let db = Database::open_in_memory().unwrap();
    let now = "2026-01-01T00:00:00Z".to_string();
    let with_session = Conversation {
        id: "grok-resume".into(),
        title: String::new(),
        agent_ids: vec![AgentId::Grok],
        cwd: Some(std::env::temp_dir().to_string_lossy().into_owned()),
        allow_dangerous: false,
        created_at: now.clone(),
        updated_at: now.clone(),
        native_session_id: Some("sess-legacy-1".into()),
        sending: false,
    };
    let no_session = Conversation {
        id: "grok-nosess".into(),
        title: String::new(),
        agent_ids: vec![AgentId::Grok],
        cwd: with_session.cwd.clone(),
        allow_dangerous: false,
        created_at: now.clone(),
        updated_at: now.clone(),
        native_session_id: None,
        sending: false,
    };
    let codex = Conversation {
        id: "codex-legacy".into(),
        title: String::new(),
        agent_ids: vec![AgentId::Codex],
        cwd: with_session.cwd.clone(),
        allow_dangerous: false,
        created_at: now.clone(),
        updated_at: now.clone(),
        native_session_id: Some("thread-1".into()),
        sending: false,
    };
    let kiro = Conversation {
        id: "kiro-legacy".into(),
        title: String::new(),
        agent_ids: vec![AgentId::Kiro],
        cwd: with_session.cwd.clone(),
        allow_dangerous: false,
        created_at: now.clone(),
        updated_at: now,
        native_session_id: Some("sess-kiro-1".into()),
        sending: false,
    };
    let repo = ChatRepo::new(db.clone());
    repo.create_conversation(&with_session).unwrap();
    repo.create_conversation(&no_session).unwrap();
    repo.create_conversation(&codex).unwrap();
    repo.create_conversation(&kiro).unwrap();
    for id in ["grok-resume", "grok-nosess", "codex-legacy", "kiro-legacy"] {
        repo.insert_message(&crate::models::ChatMessage {
            id: format!("{id}-user"),
            conversation_id: id.into(),
            turn: 1,
            role: crate::models::ChatRole::User,
            agent_id: None,
            content: "hi".into(),
            status: crate::models::ChatMessageStatus::Ok,
            exit_code: None,
            duration_ms: 0,
            error: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        })
        .unwrap();
    }
    let runtime = ChatRuntime::new(
        db.clone(),
        Arc::new(RunService::new(AdapterRegistry::default())),
    );
    let snapshot = runtime.continue_legacy("grok-resume").unwrap();
    assert!(snapshot.enabled);
    assert_eq!(
        super::store::RuntimeStore::new(db)
            .record("grok-resume")
            .unwrap()
            .unwrap()
            .thread_id
            .as_deref(),
        Some("sess-legacy-1")
    );
    let missing = runtime.continue_legacy("grok-nosess").unwrap_err();
    assert!(missing.to_string().contains("请新建对话"), "{missing}");
    assert!(!runtime.snapshot("grok-nosess", None).unwrap().enabled);
    assert!(runtime.continue_legacy("codex-legacy").is_err());
    let kiro_err = runtime.continue_legacy("kiro-legacy").unwrap_err();
    assert!(
        kiro_err.to_string().contains("不能切换聊天方式"),
        "{kiro_err}"
    );
    assert!(!runtime.snapshot("kiro-legacy", None).unwrap().enabled);
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
fn idle_enabled_runtime_allows_agent_and_cwd_changes() {
    let dir = tempdir().unwrap();
    let work_a = dir.path().join("a");
    let work_b = dir.path().join("b");
    std::fs::create_dir_all(&work_a).unwrap();
    std::fs::create_dir_all(&work_b).unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let chat = ChatService::new(db.clone(), run);
    let conv = chat
        .create_conversation(
            vec![AgentId::Codex],
            Some(work_a.to_string_lossy().into_owned()),
        )
        .unwrap();
    let store = super::store::RuntimeStore::new(db);
    store.enable_if_new(&conv.id).unwrap();
    assert!(store.persisted_enabled(&conv.id).unwrap());

    let moved = chat
        .update_conversation(
            &conv.id,
            None,
            None,
            Some(Some(work_b.to_string_lossy().into_owned())),
            None,
        )
        .unwrap();
    assert_eq!(
        moved.cwd.as_deref(),
        Some(work_b.to_string_lossy().as_ref())
    );
    assert!(store.persisted_enabled(&conv.id).unwrap());

    let switched = chat
        .update_conversation(&conv.id, None, Some(vec![AgentId::Pi]), None, None)
        .unwrap();
    assert_eq!(switched.agent_ids, vec![AgentId::Pi]);
    assert!(!store.persisted_enabled(&conv.id).unwrap());
    let snapshot = chat.runtime().snapshot(&conv.id, None).unwrap();
    assert!(!snapshot.enabled);
    let options = chat.runtime().options(&conv.id).unwrap();
    assert!(options.models.is_empty());
    assert!(!options.image_input);
    assert!(!options.steer);
    assert!(!store.persisted_enabled(&conv.id).unwrap());
}

#[test]
fn options_for_pi_are_empty_and_do_not_enable_runtime() {
    let db = Database::open_in_memory().unwrap();
    let now = "2026-01-01T00:00:00Z".to_string();
    ChatRepo::new(db.clone())
        .create_conversation(&Conversation {
            id: "pi-empty".into(),
            title: String::new(),
            agent_ids: vec![AgentId::Pi],
            cwd: Some(std::env::temp_dir().to_string_lossy().into_owned()),
            allow_dangerous: false,
            created_at: now.clone(),
            updated_at: now,
            native_session_id: None,
            sending: false,
        })
        .unwrap();
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let runtime = Arc::new(ChatRuntime::new(db.clone(), run));
    let options = runtime.options("pi-empty").unwrap();
    assert_eq!(options.conversation_id, "pi-empty");
    assert!(options.models.is_empty());
    assert!(options.extensions.is_empty());
    assert!(!options.image_input);
    assert!(!options.steer);
    assert!(!runtime.store.persisted_enabled("pi-empty").unwrap());
    assert!(!runtime.snapshot("pi-empty", None).unwrap().enabled);
}

#[test]
fn started_runtime_rejects_agent_and_cwd_changes() {
    let dir = tempdir().unwrap();
    let work = dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let chat = ChatService::new(db.clone(), run);
    let conv = chat
        .create_conversation(
            vec![AgentId::Codex],
            Some(work.to_string_lossy().into_owned()),
        )
        .unwrap();
    let store = super::store::RuntimeStore::new(db);
    store.enable_if_new(&conv.id).unwrap();
    store
        .commit_event(
            &conv.id,
            RuntimePhase::Running,
            Some("run-1"),
            &ChatEvent::Error {
                message: "started".into(),
            },
        )
        .unwrap();

    let cwd_err = chat
        .update_conversation(
            &conv.id,
            None,
            None,
            Some(Some(work.to_string_lossy().into_owned())),
            None,
        )
        .unwrap_err();
    assert_eq!(cwd_err.code(), "invalid_arg");
    assert!(cwd_err.to_string().contains("新建会话"));
    assert!(
        !cwd_err.to_string().contains("invalid argument"),
        "GUI toast should not prefix this lock error, got {cwd_err}"
    );

    let agent_err = chat
        .update_conversation(&conv.id, None, Some(vec![AgentId::Claude]), None, None)
        .unwrap_err();
    assert_eq!(agent_err.code(), "invalid_arg");
    assert!(store.persisted_enabled(&conv.id).unwrap());

    let renamed = chat
        .update_conversation(&conv.id, Some("keep title".into()), None, None, None)
        .unwrap();
    assert_eq!(renamed.title, "keep title");
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
        permission_options: Vec::new(),
        file_changes: Vec::new(),
    };
    store
        .add_request("c2", &request, "item/commandExecution/requestApproval", "7")
        .unwrap();
    let snapshot = store.snapshot("c2", None).unwrap();
    assert_eq!(snapshot.phase, RuntimePhase::Waiting);
    assert_eq!(snapshot.pending_requests, vec![request]);
    assert!(store.remove_request("c2", "req-1").unwrap());
    assert!(store
        .snapshot("c2", None)
        .unwrap()
        .pending_requests
        .is_empty());
}

#[test]
fn persisted_request_round_trips_allow_always_options() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "c-always", false);
    let store = super::store::RuntimeStore::new(db);
    store.enable_if_new("c-always").unwrap();
    let request = RuntimeRequest {
        id: "req-always".into(),
        run_id: "run-1".into(),
        kind: RuntimeRequestKind::Command,
        title: "执行命令".into(),
        detail: "printf always".into(),
        questions: Vec::new(),
        permission_options: vec![super::types::RuntimePermissionOption {
            id: "always".into(),
            kind: "allow_always".into(),
        }],
        file_changes: Vec::new(),
    };
    store
        .add_request("c-always", &request, "session/request_permission", "8")
        .unwrap();
    assert_eq!(
        store.snapshot("c-always", None).unwrap().pending_requests,
        vec![request]
    );
}

#[test]
fn persisted_request_round_trips_file_change_preview() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "c-file", false);
    let store = super::store::RuntimeStore::new(db);
    store.enable_if_new("c-file").unwrap();
    let request = RuntimeRequest {
        id: "req-file".into(),
        run_id: "run-1".into(),
        kind: RuntimeRequestKind::File,
        title: "修改文件".into(),
        detail: "/tmp/example.txt".into(),
        questions: Vec::new(),
        permission_options: Vec::new(),
        file_changes: vec![super::types::RuntimeFileChange {
            path: "/tmp/example.txt".into(),
            kind: Some("add".into()),
            preview: Some("ok".into()),
        }],
    };
    store
        .add_request("c-file", &request, "applyPatchApproval", "9")
        .unwrap();
    assert_eq!(
        store.snapshot("c-file", None).unwrap().pending_requests,
        vec![request]
    );
}

fn require_real_codex_opt_in() {
    assert_eq!(
        std::env::var("AGENTHUB_RUN_CODEX_RUNTIME_TEST").ok().as_deref(),
        Some("1"),
        "set AGENTHUB_RUN_CODEX_RUNTIME_TEST=1 to use the existing Codex login; no live test was run"
    );
}

fn open_real_chat(root: &tempfile::TempDir) -> (ChatService, String, std::path::PathBuf) {
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
    (chat, conversation.id, data_dir)
}

fn reopen_real_chat(data_dir: &std::path::Path) -> ChatService {
    let db = Database::open(&data_dir.join("agenthub.sqlite")).unwrap();
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    ChatService::new(db, run)
}

fn wait_for_snapshot(
    chat: &ChatService,
    conversation_id: &str,
    after: i64,
    timeout: Duration,
    mut pred: impl FnMut(&RuntimeSnapshot) -> bool,
) -> RuntimeSnapshot {
    let deadline = Instant::now() + timeout;
    let mut sequence = after;
    loop {
        let snapshot = chat
            .runtime()
            .snapshot(conversation_id, Some(sequence))
            .unwrap();
        sequence = snapshot.last_sequence;
        if pred(&snapshot) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "Codex runtime condition not met before timeout (phase={:?})",
            snapshot.phase
        );
        std::thread::sleep(Duration::from_millis(400));
    }
}

fn wait_for_terminal(chat: &ChatService, conversation_id: &str, after: i64) -> RuntimeSnapshot {
    wait_for_snapshot(chat, conversation_id, after, Duration::from_secs(90), |s| {
        matches!(
            s.phase,
            RuntimePhase::Completed
                | RuntimePhase::Failed
                | RuntimePhase::Cancelled
                | RuntimePhase::Interrupted
        )
    })
}

fn wait_for_waiting(chat: &ChatService, conversation_id: &str, after: i64) -> RuntimeSnapshot {
    wait_for_snapshot(chat, conversation_id, after, Duration::from_secs(90), |s| {
        (matches!(s.phase, RuntimePhase::Waiting) && !s.pending_requests.is_empty())
            || matches!(
                s.phase,
                RuntimePhase::Completed
                    | RuntimePhase::Failed
                    | RuntimePhase::Cancelled
                    | RuntimePhase::Interrupted
            )
    })
}

fn last_agent_content(chat: &ChatService, conversation_id: &str) -> String {
    chat.list_messages(conversation_id)
        .unwrap()
        .into_iter()
        .filter(|m| m.role == ChatRole::Agent)
        .last()
        .map(|m| m.content)
        .unwrap_or_default()
}

fn snapshot_error(snapshot: &RuntimeSnapshot) -> String {
    snapshot
        .current_message
        .as_ref()
        .and_then(|message| message.error.clone())
        .unwrap_or_default()
}

fn is_usage_limited(snapshot: &RuntimeSnapshot) -> bool {
    let hay = snapshot_error(snapshot).to_ascii_lowercase();
    hay.contains("usagelimitexceeded") || hay.contains("usage limit")
}

fn abort_if_limited(snapshot: &RuntimeSnapshot) -> bool {
    if !is_usage_limited(snapshot) {
        return false;
    }
    eprintln!("skip: Codex usage limit");
    true
}

/// Real Codex app-server smoke test.  It is deliberately ignored: the caller
/// must opt in with `AGENTHUB_RUN_CODEX_RUNTIME_TEST=1`.  Only the AgentHub
/// database and working directory are temporary; the caller's existing Codex
/// login is used without printing or changing it.
#[test]
#[ignore = "uses the caller's Codex login and creates native sessions; explicit opt-in required"]
fn real_codex_runtime_start_and_resume() {
    require_real_codex_opt_in();
    let root = tempdir().unwrap();
    let (chat, conversation_id, data_dir) = open_real_chat(&root);
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let first = chat
        .runtime()
        .start(
            &conversation_id,
            &format!("Remember this random marker for our next turn: AGENTHUB_RUNTIME_{nonce}. Reply with exactly that marker. Do not call any tools."),
            "real-1",
            RuntimeStartExtras::default(),
        )
        .unwrap();
    let first_done = wait_for_terminal(&chat, &conversation_id, first.last_sequence);
    if abort_if_limited(&first_done) {
        return;
    }
    assert_eq!(
        first_done.phase,
        RuntimePhase::Completed,
        "first turn error: {}",
        snapshot_error(&first_done)
    );

    drop(chat);
    let chat = reopen_real_chat(&data_dir);
    let second = chat
        .runtime()
        .start(
            &conversation_id,
            "What was the exact random marker I asked you to remember in my previous message? Reply only with that marker. Do not call any tools.",
            "real-2",
            RuntimeStartExtras::default(),
        )
        .unwrap();
    let second_done = wait_for_terminal(&chat, &conversation_id, second.last_sequence);
    let second_err = second_done
        .current_message
        .as_ref()
        .and_then(|m| m.error.clone())
        .unwrap_or_default();
    assert_eq!(
        second_done.phase,
        RuntimePhase::Completed,
        "resume turn ended as {:?} err={second_err}",
        second_done.phase
    );
    assert!(
        last_agent_content(&chat, &conversation_id).contains(&nonce),
        "resumed turn did not remember the previous marker"
    );
}

/// Product path: command approval allow + deny via ChatRuntime reply.
#[test]
#[ignore = "uses the caller's Codex login and creates native sessions; explicit opt-in required"]
fn real_codex_runtime_command_approval_allow_and_deny() {
    require_real_codex_opt_in();
    let root = tempdir().unwrap();
    let (chat, conversation_id, _) = open_real_chat(&root);

    // Deny path first: request a harmless command and decline it.
    let deny_start = chat
        .runtime()
        .start(
            &conversation_id,
            "Run this exact shell command once and show its stdout: printf 'AGENTHUB_DENY_PROBE'. Do not invent the output; you must execute the command.",
            "real-deny-1",
            RuntimeStartExtras::default(),
        )
        .unwrap();
    let waiting = wait_for_waiting(&chat, &conversation_id, deny_start.last_sequence);
    if abort_if_limited(&waiting) {
        return;
    }
    assert_eq!(
        waiting.phase,
        RuntimePhase::Waiting,
        "expected command approval before tools run; got {:?} ({})",
        waiting.phase,
        snapshot_error(&waiting)
    );
    let request = waiting
        .pending_requests
        .iter()
        .find(|r| matches!(r.kind, RuntimeRequestKind::Command))
        .cloned()
        .expect("expected a command approval request");
    chat.runtime()
        .reply(RuntimeReply {
            conversation_id: conversation_id.clone(),
            run_id: request.run_id.clone(),
            request_id: request.id.clone(),
            client_request_id: "real-deny-reply".into(),
            decision: Some(RuntimeDecision::Deny),
            answers: None,
        })
        .unwrap();
    let denied = wait_for_terminal(&chat, &conversation_id, waiting.last_sequence);
    assert!(
        matches!(
            denied.phase,
            RuntimePhase::Completed | RuntimePhase::Failed | RuntimePhase::Cancelled
        ),
        "deny path ended unexpectedly: {:?}",
        denied.phase
    );

    // Allow path: request another harmless command and accept it.
    let allow_start = chat
        .runtime()
        .start(
            &conversation_id,
            "Run this exact shell command once and reply with only its stdout: printf 'AGENTHUB_ALLOW_OK'. Do not invent the output.",
            "real-allow-1",
            RuntimeStartExtras::default(),
        )
        .unwrap();
    let waiting = wait_for_waiting(&chat, &conversation_id, allow_start.last_sequence);
    assert_eq!(
        waiting.phase,
        RuntimePhase::Waiting,
        "expected command approval before tools run; got {:?}",
        waiting.phase
    );
    let request = waiting
        .pending_requests
        .iter()
        .find(|r| matches!(r.kind, RuntimeRequestKind::Command))
        .cloned()
        .expect("expected a command approval request");
    chat.runtime()
        .reply(RuntimeReply {
            conversation_id: conversation_id.clone(),
            run_id: request.run_id.clone(),
            request_id: request.id.clone(),
            client_request_id: "real-allow-reply".into(),
            decision: Some(RuntimeDecision::Allow),
            answers: None,
        })
        .unwrap();
    let allowed = wait_for_terminal(&chat, &conversation_id, waiting.last_sequence);
    assert_eq!(allowed.phase, RuntimePhase::Completed);
    assert!(
        last_agent_content(&chat, &conversation_id).contains("AGENTHUB_ALLOW_OK"),
        "allow path did not surface command output"
    );
}

/// Product path: mid-turn steer + interrupt.
#[test]
#[ignore = "uses the caller's Codex login and creates native sessions; explicit opt-in required"]
fn real_codex_runtime_steer_and_interrupt() {
    require_real_codex_opt_in();
    let root = tempdir().unwrap();
    let (chat, conversation_id, _) = open_real_chat(&root);
    let steer_marker = format!("AGENTHUB_STEER_{}", uuid::Uuid::new_v4().simple());

    let started = chat
        .runtime()
        .start(
            &conversation_id,
            "Write a long numbered list from 1 to 80, one number per line, with a short adjective after each number. Do not call any tools. Keep writing until you finish.",
            "real-steer-1",
            RuntimeStartExtras::default(),
        )
        .unwrap();
    // Wait until the turn is visibly running (or already waiting/terminal).
    let live = wait_for_snapshot(
        &chat,
        &conversation_id,
        started.last_sequence,
        Duration::from_secs(45),
        |s| {
            matches!(
                s.phase,
                RuntimePhase::Running
                    | RuntimePhase::Waiting
                    | RuntimePhase::Completed
                    | RuntimePhase::Failed
                    | RuntimePhase::Cancelled
                    | RuntimePhase::Interrupted
            )
        },
    );
    if abort_if_limited(&live) {
        return;
    }
    assert!(
        matches!(live.phase, RuntimePhase::Running | RuntimePhase::Waiting),
        "steer needs an active turn; got {:?}",
        live.phase
    );
    let run_id = live.run_id.clone().expect("active turn must expose runId");
    chat.runtime()
        .steer(
            &conversation_id,
            &run_id,
            &format!(
                "Stop the list. Reply with exactly this marker and nothing else: {steer_marker}"
            ),
            "real-steer-client",
        )
        .unwrap();
    let steered = wait_for_terminal(&chat, &conversation_id, live.last_sequence);
    if abort_if_limited(&steered) {
        return;
    }
    assert_eq!(
        steered.phase,
        RuntimePhase::Completed,
        "steer error: {}",
        snapshot_error(&steered)
    );
    assert!(
        last_agent_content(&chat, &conversation_id).contains(&steer_marker),
        "steered turn did not include the marker"
    );

    let interrupt_start = chat
        .runtime()
        .start(
            &conversation_id,
            "Write a very long essay about rivers, at least 40 paragraphs. Do not call any tools. Keep writing until finished.",
            "real-interrupt-1",
            RuntimeStartExtras::default(),
        )
        .unwrap();
    let live = wait_for_snapshot(
        &chat,
        &conversation_id,
        interrupt_start.last_sequence,
        Duration::from_secs(45),
        |s| {
            matches!(
                s.phase,
                RuntimePhase::Running
                    | RuntimePhase::Waiting
                    | RuntimePhase::Completed
                    | RuntimePhase::Failed
                    | RuntimePhase::Cancelled
                    | RuntimePhase::Interrupted
            )
        },
    );
    let run_id = live.run_id.clone().expect("active turn must expose runId");
    // Give the model a moment to produce output before interrupting.
    std::thread::sleep(Duration::from_secs(2));
    chat.runtime().cancel(&conversation_id, &run_id).unwrap();
    let stopped = wait_for_terminal(&chat, &conversation_id, live.last_sequence);
    assert!(
        matches!(
            stopped.phase,
            RuntimePhase::Cancelled | RuntimePhase::Interrupted | RuntimePhase::Completed
        ),
        "interrupt ended unexpectedly: {:?}",
        stopped.phase
    );
}

/// Product path: catalog over-reports effort → fail once → learn → persist across reopen.
#[test]
#[ignore = "uses the caller's Codex login and creates native sessions; explicit opt-in required"]
fn real_codex_runtime_learn_denied_effort() {
    require_real_codex_opt_in();
    let root = tempdir().unwrap();
    let (chat, conversation_id, data_dir) = open_real_chat(&root);

    let options = chat.runtime().options(&conversation_id).unwrap();
    let spark = options
        .models
        .iter()
        .find(|m| m.id == "gpt-5.3-codex-spark")
        .cloned();
    let Some(spark) = spark else {
        eprintln!("skip: gpt-5.3-codex-spark not in live model/list");
        return;
    };
    // Prefer the historically broken pair when the live catalog still offers it.
    let can_request_medium = spark.efforts.iter().any(|e| e == "medium");
    if can_request_medium {
        chat.runtime()
            .set_settings(
                &conversation_id,
                RuntimeTurnSettings {
                    model: Some(spark.id.clone()),
                    effort: Some("medium".into()),
                },
            )
            .unwrap();
    } else {
        // Catalog already dropped medium; seed the deny table and verify persistence/retry.
        chat.runtime()
            .note_thinking_failure(
                &conversation_id,
                RuntimeTurnSettings {
                    model: Some(spark.id.clone()),
                    effort: Some("medium".into()),
                },
                "does not support parameter reasoningEffort=medium",
            )
            .unwrap();
        chat.runtime()
            .set_settings(
                &conversation_id,
                RuntimeTurnSettings {
                    model: Some(spark.id.clone()),
                    effort: spark.default_effort.clone(),
                },
            )
            .unwrap();
    }

    if can_request_medium {
        let start = chat.runtime().start(
            &conversation_id,
            "Reply with exactly: AGENTHUB_EFFORT_PROBE. Do not call any tools.",
            "real-effort-1",
            RuntimeStartExtras::default(),
        );
        match start {
            Ok(snapshot) => {
                let done = wait_for_terminal(&chat, &conversation_id, snapshot.last_sequence);
                if matches!(done.phase, RuntimePhase::Completed) {
                    eprintln!("note: spark+medium completed; denied-effort learning not exercised");
                    return;
                }
                assert_eq!(
                    done.phase,
                    RuntimePhase::Failed,
                    "unexpected phase after spark+medium: {:?}",
                    done.phase
                );
                let err = done
                    .current_message
                    .as_ref()
                    .and_then(|m| m.error.clone())
                    .unwrap_or_default();
                assert!(
                    super::ops::looks_like_thinking_unsupported(&err),
                    "expected thinking-unsupported failure, got: {err}"
                );
            }
            Err(error) => {
                assert!(
                    error.to_string().contains("不支持思考强度")
                        || super::ops::looks_like_thinking_unsupported(&error.to_string()),
                    "unexpected start error: {error}"
                );
                chat.runtime()
                    .note_thinking_failure(
                        &conversation_id,
                        RuntimeTurnSettings {
                            model: Some(spark.id.clone()),
                            effort: Some("medium".into()),
                        },
                        "does not support parameter reasoningEffort=medium",
                    )
                    .unwrap();
            }
        }
    }

    let after = chat.runtime().options(&conversation_id).unwrap();
    let spark_after = after
        .models
        .iter()
        .find(|m| m.id == "gpt-5.3-codex-spark")
        .expect("spark should remain listed");
    assert!(
        !spark_after.efforts.iter().any(|e| e == "medium"),
        "medium should be filtered after learning"
    );
    assert_ne!(after.settings.effort.as_deref(), Some("medium"));

    drop(chat);
    let chat = reopen_real_chat(&data_dir);
    let persisted = chat.runtime().options(&conversation_id).unwrap();
    let spark_persisted = persisted
        .models
        .iter()
        .find(|m| m.id == "gpt-5.3-codex-spark")
        .expect("spark should remain listed after reopen");
    assert!(
        !spark_persisted.efforts.iter().any(|e| e == "medium"),
        "medium should stay filtered after sqlite reopen"
    );

    let retry = chat
        .runtime()
        .start(
            &conversation_id,
            "Reply with exactly: AGENTHUB_EFFORT_OK. Do not call any tools.",
            "real-effort-2",
            RuntimeStartExtras::default(),
        )
        .unwrap();
    let done = wait_for_terminal(&chat, &conversation_id, retry.last_sequence);
    assert_eq!(done.phase, RuntimePhase::Completed);
    assert!(
        last_agent_content(&chat, &conversation_id).contains("AGENTHUB_EFFORT_OK"),
        "retry after effort coerce did not complete cleanly"
    );
}

/// Product path: worker shutdown mid-turn, reopen sqlite, continue the native thread.
#[test]
#[ignore = "uses the caller's Codex login and creates native sessions; explicit opt-in required"]
fn real_codex_runtime_crash_reopen_then_continue() {
    require_real_codex_opt_in();
    let root = tempdir().unwrap();
    let (chat, conversation_id, data_dir) = open_real_chat(&root);
    let started = chat
        .runtime()
        .start(
            &conversation_id,
            "Write a very long essay about rivers, at least 40 paragraphs. Do not call any tools. Keep writing until finished.",
            "real-crash-1",
            RuntimeStartExtras::default(),
        )
        .unwrap();
    let live = wait_for_snapshot(
        &chat,
        &conversation_id,
        started.last_sequence,
        Duration::from_secs(45),
        |s| {
            matches!(
                s.phase,
                RuntimePhase::Running
                    | RuntimePhase::Waiting
                    | RuntimePhase::Completed
                    | RuntimePhase::Failed
                    | RuntimePhase::Cancelled
                    | RuntimePhase::Interrupted
            )
        },
    );
    if abort_if_limited(&live) {
        return;
    }
    assert!(
        matches!(live.phase, RuntimePhase::Running | RuntimePhase::Waiting),
        "crash reopen needs an active turn; got {:?}",
        live.phase
    );
    chat.runtime().shutdown(&conversation_id);
    drop(chat);

    let chat = reopen_real_chat(&data_dir);
    let recovered = chat.runtime().snapshot(&conversation_id, None).unwrap();
    assert!(
        !matches!(
            recovered.phase,
            RuntimePhase::Starting
                | RuntimePhase::Running
                | RuntimePhase::Waiting
                | RuntimePhase::Cancelling
        ),
        "stale active phase after reopen: {:?}",
        recovered.phase
    );
    let retry = chat
        .runtime()
        .start(
            &conversation_id,
            "Reply with exactly: AGENTHUB_RECOVER_OK. Do not call any tools.",
            "real-crash-2",
            RuntimeStartExtras::default(),
        )
        .unwrap();
    let done = wait_for_terminal(&chat, &conversation_id, retry.last_sequence);
    if abort_if_limited(&done) {
        return;
    }
    assert_eq!(
        done.phase,
        RuntimePhase::Completed,
        "recover continue error: {}",
        snapshot_error(&done)
    );
    assert!(
        last_agent_content(&chat, &conversation_id).contains("AGENTHUB_RECOVER_OK"),
        "continue after reopen did not complete cleanly"
    );
}

#[test]
fn oversized_stdout_line_is_a_picture_too_large_error() {
    let error = super::codex_transport::CodexTransportError::Protocol(
        "stdout JSON line exceeds 1048576 bytes".into(),
    );
    assert_eq!(
        super::transport_user_message(crate::models::AgentId::Codex, &error),
        "图片太大，请换一张更小的图"
    );
    let other = super::codex_transport::CodexTransportError::Protocol("missing field".into());
    assert!(
        super::transport_user_message(crate::models::AgentId::Codex, &other)
            .contains("missing field"),
        "{}",
        super::transport_user_message(crate::models::AgentId::Codex, &other)
    );
    assert_eq!(
        super::transport_user_message(
            crate::models::AgentId::Grok,
            &super::codex_transport::CodexTransportError::Exited
        ),
        "Grok 已退出"
    );
}

/// Product path: localImage extra is accepted on turn/start.
#[test]
#[ignore = "uses the caller's Codex login and creates native sessions; explicit opt-in required"]
fn real_codex_runtime_local_image_turn() {
    require_real_codex_opt_in();
    let root = tempdir().unwrap();
    let (chat, conversation_id, _) = open_real_chat(&root);
    let png = root.path().join("workspace").join("probe.png");
    std::fs::write(
        &png,
        [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x05, 0xFE,
            0xD4, 0xEF, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ],
    )
    .unwrap();
    let started = chat
        .runtime()
        .start(
            &conversation_id,
            "If you received an image, reply with exactly AGENTHUB_IMAGE_OK. Do not call any tools.",
            "real-image-1",
            RuntimeStartExtras {
                images: vec![RuntimeLocalImage {
                    path: png.to_string_lossy().into_owned(),
                }],
                ..Default::default()
            },
        )
        .unwrap();
    let done = wait_for_terminal(&chat, &conversation_id, started.last_sequence);
    if abort_if_limited(&done) {
        return;
    }
    assert_eq!(
        done.phase,
        RuntimePhase::Completed,
        "image turn error: {}",
        snapshot_error(&done)
    );
    assert!(
        last_agent_content(&chat, &conversation_id).contains("AGENTHUB_IMAGE_OK"),
        "image turn did not acknowledge the attachment"
    );
}

#[test]
fn frozen_options_serve_warmed_catalog_without_refetch() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "warm", false);
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let runtime = Arc::new(ChatRuntime::new(db, run));
    runtime.store.enable_if_new("warm").unwrap();
    runtime.seed_catalog_cache_for_test(
        "warm",
        vec![super::types::RuntimeModelOption {
            id: "gpt-warm".into(),
            efforts: vec!["low".into()],
            default_effort: Some("low".into()),
        }],
        vec![super::types::RuntimeExtensionItem {
            id: "/skills/demo/SKILL.md".into(),
            name: "demo".into(),
            kind: super::types::RuntimeExtensionKind::Skill,
            installed: true,
            enabled: true,
            loaded: false,
            callable: true,
            path: Some("/skills/demo/SKILL.md".into()),
        }],
    );
    runtime
        .store
        .commit_event(
            "warm",
            RuntimePhase::Running,
            Some("run-warm"),
            &ChatEvent::Error {
                message: "marker".into(),
            },
        )
        .unwrap();

    let options = runtime.options("warm").unwrap();
    assert!(options.settings_frozen);
    assert_eq!(options.models.len(), 1);
    assert_eq!(options.models[0].id, "gpt-warm");
    assert_eq!(options.extensions.len(), 1);
    assert!(options.models_from_codex);
}

#[test]
fn frozen_options_stay_empty_when_catalog_never_warmed() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "cold", false);
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let runtime = Arc::new(ChatRuntime::new(db, run));
    runtime.store.enable_if_new("cold").unwrap();
    runtime
        .store
        .commit_event(
            "cold",
            RuntimePhase::Running,
            Some("run-cold"),
            &ChatEvent::Error {
                message: "marker".into(),
            },
        )
        .unwrap();

    let options = runtime.options("cold").unwrap();
    assert!(options.settings_frozen);
    assert!(options.models.is_empty());
    assert!(options.extensions.is_empty());
    assert!(!options.models_from_codex);
}

#[test]
fn invalidate_catalogs_drops_warmed_idle_cache() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "stale", false);
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let runtime = Arc::new(ChatRuntime::new(db, run));
    runtime.store.enable_if_new("stale").unwrap();
    runtime.seed_catalog_cache_for_test(
        "stale",
        vec![super::types::RuntimeModelOption {
            id: "gpt-old-login".into(),
            efforts: vec!["low".into()],
            default_effort: Some("low".into()),
        }],
        vec![],
    );
    assert!(runtime.has_catalog_cache_for_test("stale"));
    runtime.invalidate_catalogs();
    assert!(!runtime.has_catalog_cache_for_test("stale"));
}

#[test]
fn refresh_options_skips_warmed_catalog_when_idle() {
    let db = Database::open_in_memory().unwrap();
    let now = "2026-01-01T00:00:00Z".to_string();
    ChatRepo::new(db.clone())
        .create_conversation(&Conversation {
            id: "no-cwd".into(),
            title: String::new(),
            agent_ids: vec![AgentId::Codex],
            cwd: None,
            allow_dangerous: false,
            created_at: now.clone(),
            updated_at: now,
            native_session_id: None,
            sending: false,
        })
        .unwrap();
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let runtime = Arc::new(ChatRuntime::new(db, run));
    runtime.store.enable_if_new("no-cwd").unwrap();
    runtime.seed_catalog_cache_for_test(
        "no-cwd",
        vec![super::types::RuntimeModelOption {
            id: "gpt-old-login".into(),
            efforts: vec!["low".into()],
            default_effort: Some("low".into()),
        }],
        vec![],
    );
    assert_eq!(
        runtime.options("no-cwd").unwrap().models[0].id,
        "gpt-old-login"
    );
    let refreshed = runtime.refresh_options("no-cwd").unwrap();
    assert!(refreshed.models.is_empty());
}

#[test]
fn idle_options_replace_model_missing_from_new_login_catalog() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "switch-login", false);
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let runtime = Arc::new(ChatRuntime::new(db, run));
    runtime.store.enable_if_new("switch-login").unwrap();
    runtime
        .store
        .set_turn_settings(
            "switch-login",
            &super::types::RuntimeTurnSettings {
                model: Some("gpt-old-login".into()),
                effort: Some("xhigh".into()),
            },
        )
        .unwrap();
    runtime.seed_catalog_cache_for_test(
        "switch-login",
        vec![super::types::RuntimeModelOption {
            id: "gpt-new-login".into(),
            efforts: vec!["low".into(), "high".into()],
            default_effort: Some("high".into()),
        }],
        vec![],
    );

    let options = runtime.options("switch-login").unwrap();
    assert_eq!(options.settings.model.as_deref(), Some("gpt-new-login"));
    assert_eq!(options.settings.effort.as_deref(), Some("high"));
}

#[test]
fn idle_options_fill_first_catalog_model_when_unset() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "unset", false);
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let runtime = Arc::new(ChatRuntime::new(db, run));
    runtime.store.enable_if_new("unset").unwrap();
    runtime.seed_catalog_cache_for_test(
        "unset",
        vec![super::types::RuntimeModelOption {
            id: "gpt-first".into(),
            efforts: vec!["low".into(), "high".into()],
            default_effort: Some("high".into()),
        }],
        vec![],
    );

    let options = runtime.options("unset").unwrap();
    assert_eq!(options.settings.model.as_deref(), Some("gpt-first"));
    assert_eq!(options.settings.effort.as_deref(), Some("high"));
    let persisted = runtime.store.turn_settings("unset").unwrap();
    assert_eq!(persisted.model.as_deref(), Some("gpt-first"));
}

#[test]
fn idle_options_reconcile_unsupported_effort_to_model_default() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "spark", false);
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let runtime = Arc::new(ChatRuntime::new(db, run));
    runtime.store.enable_if_new("spark").unwrap();
    runtime.seed_catalog_cache_for_test(
        "spark",
        vec![super::types::RuntimeModelOption {
            id: "gpt-5.3-codex-spark".into(),
            efforts: vec!["low".into(), "high".into()],
            default_effort: Some("low".into()),
        }],
        vec![],
    );
    runtime
        .store
        .set_turn_settings(
            "spark",
            &super::types::RuntimeTurnSettings {
                model: Some("gpt-5.3-codex-spark".into()),
                effort: Some("medium".into()),
            },
        )
        .unwrap();

    let options = runtime.options("spark").unwrap();
    assert!(!options.settings_frozen);
    assert_eq!(
        options.settings.model.as_deref(),
        Some("gpt-5.3-codex-spark")
    );
    assert_eq!(options.settings.effort.as_deref(), Some("low"));
}

#[test]
fn frozen_options_keep_effective_unsupported_effort_pair() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "frozen-spark", false);
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let runtime = Arc::new(ChatRuntime::new(db, run));
    runtime.store.enable_if_new("frozen-spark").unwrap();
    runtime.seed_catalog_cache_for_test(
        "frozen-spark",
        vec![super::types::RuntimeModelOption {
            id: "gpt-5.3-codex-spark".into(),
            efforts: vec!["low".into()],
            default_effort: Some("low".into()),
        }],
        vec![],
    );
    runtime
        .store
        .set_turn_settings(
            "frozen-spark",
            &super::types::RuntimeTurnSettings {
                model: Some("gpt-5.3-codex-spark".into()),
                effort: Some("medium".into()),
            },
        )
        .unwrap();
    runtime
        .store
        .commit_event(
            "frozen-spark",
            RuntimePhase::Running,
            Some("run-frozen"),
            &ChatEvent::Error {
                message: "marker".into(),
            },
        )
        .unwrap();

    let options = runtime.options("frozen-spark").unwrap();
    assert!(options.settings_frozen);
    assert_eq!(options.settings.effort.as_deref(), Some("medium"));
}

#[test]
fn start_rejects_unsupported_model_effort_pair() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "start-spark", false);
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let runtime = Arc::new(ChatRuntime::new(db, run));
    runtime.store.enable_if_new("start-spark").unwrap();
    runtime.seed_catalog_cache_for_test(
        "start-spark",
        vec![super::types::RuntimeModelOption {
            id: "gpt-5.3-codex-spark".into(),
            efforts: vec!["low".into(), "high".into()],
            default_effort: Some("low".into()),
        }],
        vec![],
    );
    runtime
        .store
        .set_turn_settings(
            "start-spark",
            &super::types::RuntimeTurnSettings {
                model: Some("gpt-5.3-codex-spark".into()),
                effort: Some("medium".into()),
            },
        )
        .unwrap();

    let err = runtime
        .start(
            "start-spark",
            "ping",
            "client-spark-1",
            RuntimeStartExtras::default(),
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("不支持思考强度"),
        "unexpected error: {err}"
    );
}

#[test]
fn set_settings_resets_effort_when_model_changes_without_effort() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "switch", false);
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let runtime = Arc::new(ChatRuntime::new(db, run));
    runtime.store.enable_if_new("switch").unwrap();
    runtime.seed_catalog_cache_for_test(
        "switch",
        vec![
            super::types::RuntimeModelOption {
                id: "gpt-full".into(),
                efforts: vec!["low".into(), "medium".into(), "high".into()],
                default_effort: Some("medium".into()),
            },
            super::types::RuntimeModelOption {
                id: "gpt-5.3-codex-spark".into(),
                efforts: vec!["low".into(), "high".into()],
                default_effort: Some("low".into()),
            },
        ],
        vec![],
    );
    let first = runtime
        .set_settings(
            "switch",
            super::types::RuntimeTurnSettings {
                model: Some("gpt-full".into()),
                effort: Some("medium".into()),
            },
        )
        .unwrap();
    assert_eq!(first.effort.as_deref(), Some("medium"));

    let switched = runtime
        .set_settings(
            "switch",
            super::types::RuntimeTurnSettings {
                model: Some("gpt-5.3-codex-spark".into()),
                effort: None,
            },
        )
        .unwrap();
    assert_eq!(switched.model.as_deref(), Some("gpt-5.3-codex-spark"));
    assert_eq!(switched.effort.as_deref(), Some("low"));

    let rejected = runtime
        .set_settings(
            "switch",
            super::types::RuntimeTurnSettings {
                model: Some("gpt-5.3-codex-spark".into()),
                effort: Some("medium".into()),
            },
        )
        .unwrap_err();
    assert!(rejected.to_string().contains("不支持思考强度"));
}

#[test]
fn learn_from_thinking_unsupported_filters_over_reported_catalog() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "learn-spark", false);
    let run = Arc::new(RunService::new(AdapterRegistry::default()));
    let runtime = Arc::new(ChatRuntime::new(db, run));
    runtime.store.enable_if_new("learn-spark").unwrap();
    runtime.seed_catalog_cache_for_test(
        "learn-spark",
        vec![super::types::RuntimeModelOption {
            id: "gpt-5.3-codex-spark".into(),
            efforts: vec!["low".into(), "medium".into(), "high".into(), "xhigh".into()],
            default_effort: Some("high".into()),
        }],
        vec![],
    );
    runtime
        .store
        .set_turn_settings(
            "learn-spark",
            &super::types::RuntimeTurnSettings {
                model: Some("gpt-5.3-codex-spark".into()),
                effort: Some("medium".into()),
            },
        )
        .unwrap();

    // Before learning, over-reported medium is still offered.
    let before = runtime.options("learn-spark").unwrap();
    assert_eq!(
        before.models[0].efforts,
        vec!["low", "medium", "high", "xhigh"]
    );

    runtime
        .note_thinking_failure(
            "learn-spark",
            super::types::RuntimeTurnSettings {
                model: Some("gpt-5.3-codex-spark".into()),
                effort: Some("medium".into()),
            },
            "OpenAI API error (400): does not support parameter reasoningEffort=medium",
        )
        .unwrap();

    let after = runtime.options("learn-spark").unwrap();
    assert_eq!(after.models[0].efforts, vec!["low", "high", "xhigh"]);
    assert_eq!(after.settings.effort.as_deref(), Some("high"));

    let rejected = runtime
        .set_settings(
            "learn-spark",
            super::types::RuntimeTurnSettings {
                model: Some("gpt-5.3-codex-spark".into()),
                effort: Some("medium".into()),
            },
        )
        .unwrap_err();
    assert!(rejected.to_string().contains("不支持思考强度"));
}

#[test]
fn start_empty_prompt_logs_send_fail() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "empty-prompt", false);
    let runtime = Arc::new(ChatRuntime::new(
        db,
        Arc::new(RunService::new(AdapterRegistry::default())),
    ));
    let (result, logs) = crate::logging::with_captured_logs(|| {
        runtime.start(
            "empty-prompt",
            "   ",
            "client-empty",
            RuntimeStartExtras::default(),
        )
    });
    assert!(result.is_err());
    assert!(logs.contains("core.chat"), "logs:\n{logs}");
    assert!(logs.contains("op=\"send_fail\""), "logs:\n{logs}");
    assert!(
        !logs.contains("   "),
        "must not log prompt whitespace as body"
    );
}

#[test]
fn cancel_without_actor_logs_stop_fail() {
    let db = Database::open_in_memory().unwrap();
    conversation(&db, "no-actor", false);
    let runtime = ChatRuntime::new(db, Arc::new(RunService::new(AdapterRegistry::default())));
    let (result, logs) =
        crate::logging::with_captured_logs(|| runtime.cancel("no-actor", "run-missing"));
    assert!(result.is_err());
    assert!(logs.contains("core.chat"), "logs:\n{logs}");
    assert!(logs.contains("op=\"stop_fail\""), "logs:\n{logs}");
}
