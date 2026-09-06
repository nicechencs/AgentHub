use super::types::RuntimeStartExtras;
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
    assert!(store
        .snapshot("c2", None)
        .unwrap()
        .pending_requests
        .is_empty());
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
            RuntimeStartExtras::default(),
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
            RuntimeStartExtras::default(),
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
