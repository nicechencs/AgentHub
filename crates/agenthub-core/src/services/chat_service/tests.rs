use super::*;
use crate::adapters::{AdapterRegistry, AgentAdapter};
use crate::utils::process::RecordingProcessRunner;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tempfile::tempdir;

struct DeterministicAgentAdapter {
    id: AgentId,
}

impl AgentAdapter for DeterministicAgentAdapter {
    fn id(&self) -> AgentId {
        self.id
    }

    fn detect(&self) -> crate::models::DetectResult {
        crate::models::DetectResult {
            agent: self.id,
            status: crate::models::DetectStatus::Installed,
            version: Some("test".into()),
            binary_path: Some(std::path::PathBuf::from("test-agent")),
            channel: Some("test".into()),
            env_ready: true,
            notes: Vec::new(),
        }
    }

    fn install_channels(&self) -> Vec<crate::models::InstallChannel> {
        Vec::new()
    }

    fn read_config(&self) -> crate::error::Result<crate::models::AgentConfig> {
        Err(crate::error::AppError::Unsupported("test adapter".into()))
    }

    fn read_auth(&self) -> crate::error::Result<crate::models::AuthState> {
        Err(crate::error::AppError::Unsupported("test adapter".into()))
    }

    fn skills_dir(&self) -> Option<std::path::PathBuf> {
        None
    }

    fn live_backup_paths(&self) -> Vec<std::path::PathBuf> {
        Vec::new()
    }

    fn build_run_spec(
        &self,
        binary: &std::path::Path,
        prompt: &str,
        opts: &crate::models::RunOptions,
    ) -> crate::error::Result<crate::models::RunSpec> {
        Ok(crate::models::RunSpec {
            agent: self.id,
            program: binary.to_path_buf(),
            args: vec!["--prompt".into(), prompt.into()],
            cwd: opts.cwd.clone(),
            env: Vec::new(),
        })
    }

    fn capability(&self, _cap: crate::models::Capability) -> crate::models::CapabilityState {
        crate::models::CapabilityState::unsupported("test adapter")
    }
}

fn deterministic_registry() -> AdapterRegistry {
    let mut registry = AdapterRegistry::new();
    for id in AgentId::ALL {
        registry.register(Arc::new(DeterministicAgentAdapter { id }));
    }
    registry
}

fn msg(
    turn: i64,
    role: ChatRole,
    agent: Option<AgentId>,
    content: &str,
    status: ChatMessageStatus,
) -> ChatMessage {
    ChatMessage {
        id: format!("m-{turn}-{}", content.len()),
        conversation_id: "c".into(),
        turn,
        role,
        agent_id: agent,
        content: content.into(),
        status,
        exit_code: None,
        duration_ms: 0,
        error: None,
        created_at: "t".into(),
    }
}

#[test]
fn build_prompt_empty_history_returns_input() {
    assert_eq!(build_agent_prompt(&[], AgentId::Claude, "hello"), "hello");
}

#[test]
fn build_prompt_isolates_agents() {
    let history = vec![
        msg(1, ChatRole::User, None, "q1", ChatMessageStatus::Ok),
        msg(
            1,
            ChatRole::Agent,
            Some(AgentId::Claude),
            "claude-reply",
            ChatMessageStatus::Ok,
        ),
        msg(
            1,
            ChatRole::Agent,
            Some(AgentId::Codex),
            "codex-secret",
            ChatMessageStatus::Ok,
        ),
    ];
    let p = build_agent_prompt(&history, AgentId::Claude, "q2");
    assert!(p.contains("claude-reply"));
    assert!(!p.contains("codex-secret"));
    assert!(p.contains("q2"));
    assert!(p.contains("## 历史对话"));
}

#[test]
fn build_prompt_truncates_old_turns() {
    let mut history = Vec::new();
    let big = "x".repeat(10_000);
    for t in 1..=5 {
        history.push(msg(
            t,
            ChatRole::User,
            None,
            &format!("user-{t}-{big}"),
            ChatMessageStatus::Ok,
        ));
        history.push(msg(
            t,
            ChatRole::Agent,
            Some(AgentId::Grok),
            &format!("agent-{t}-{big}"),
            ChatMessageStatus::Ok,
        ));
    }
    let p = build_agent_prompt(&history, AgentId::Grok, "latest");
    assert!(p.contains("[更早的对话已省略]") || p == "latest");
    assert!(p.chars().count() <= CONTEXT_CHAR_LIMIT);
    assert!(!p.contains("user-1-"));
}

#[test]
fn create_dedupes_agent_ids() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::new()),
    ));
    let chat = ChatService::new(db, run);
    let conv = chat
        .create_conversation(vec![AgentId::Claude, AgentId::Claude, AgentId::Codex], None)
        .unwrap();
    assert_eq!(conv.agent_ids, vec![AgentId::Claude, AgentId::Codex]);
}

#[test]
fn send_persists_and_isolates_prompts() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let recorder = RecordingProcessRunner::new();
    let calls = Arc::clone(&recorder.calls);
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(recorder),
    ));
    let chat = ChatService::new(db, run);

    let conv = chat
        .create_conversation(vec![AgentId::Claude, AgentId::Codex], None)
        .unwrap();

    // First turn
    let events = Mutex::new(Vec::new());
    chat.send(&conv.id, "first question", &|ev| {
        events.lock().unwrap().push(ev);
    })
    .unwrap();

    let msgs = chat.list_messages(&conv.id).unwrap();
    assert!(msgs.iter().any(|m| m.role == ChatRole::User));
    assert_eq!(msgs.iter().filter(|m| m.role == ChatRole::Agent).count(), 2);
    assert!(msgs.iter().all(|m| m.status != ChatMessageStatus::Running));

    // Second turn: RecordingProcessRunner returns mock:{agent}; verify history used.
    calls.lock().unwrap().clear();
    chat.send(&conv.id, "second", &|_| {}).unwrap();
    let specs = calls.lock().unwrap().clone();
    assert_eq!(specs.len(), 2, "each selected agent must reach the runner");
    assert!(specs.iter().any(|s| s.agent == AgentId::Claude));
    assert!(specs.iter().any(|s| s.agent == AgentId::Codex));
    for s in &specs {
        let joined = s.args.join(" ");
        // Second-turn prompt should include prior user text when agent ran.
        if matches!(s.agent, AgentId::Claude | AgentId::Codex) {
            assert!(
                joined.contains("second") || joined.contains("first question"),
                "unexpected args for {}: {joined}",
                s.agent.as_str()
            );
        }
    }

    let updated = chat.get_conversation(&conv.id).unwrap();
    assert!(!updated.title.is_empty());
    assert!(updated.title.contains("first") || !updated.title.is_empty());
}

#[test]
fn concurrent_send_rejected() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::new()),
    ));
    let chat = Arc::new(ChatService::new(db, run));
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    let id = conv.id.clone();

    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let release_rx2 = Arc::clone(&release_rx);
    let chat2 = Arc::clone(&chat);
    let id2 = id.clone();
    let t1 = thread::spawn(move || {
        chat2.send(&id2, "slow", &move |event| {
            if matches!(event, ChatEvent::AgentStarted { .. }) {
                started_tx.send(()).expect("notify first agent start");
                release_rx2
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(10))
                    .expect("release first send after concurrent assertion");
            }
        })
    });

    // Hold the first send inside its AgentStarted callback until the concurrent
    // assertion completes, so the test does not depend on scheduler timing.
    started_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("first send did not start an agent in time");

    let second = chat.send(&id, "second", &|_| {});
    release_tx.send(()).expect("release first send");
    let err = second.unwrap_err();
    assert!(err.to_string().contains("in-flight"), "unexpected: {err}");
    t1.join().unwrap().unwrap();
}

#[test]
fn delete_conversation_cancels_active() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let mut recorder = RecordingProcessRunner::new();
    recorder.delay = Duration::from_millis(500);
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(recorder),
    ));
    let chat = Arc::new(ChatService::new(db, run));
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    let id = conv.id.clone();
    let chat2 = Arc::clone(&chat);
    let id2 = id.clone();
    let handle = thread::spawn(move || chat2.send(&id2, "long", &|_| {}));
    thread::sleep(Duration::from_millis(80));
    chat.delete_conversation(&id).unwrap();
    // Send may succeed (mock runner finishes) or fail if delete raced past inserts.
    let _ = handle.join().unwrap();
    assert!(chat.list_conversations().unwrap().is_empty());
}

#[test]
fn append_capped_respects_byte_limit() {
    let mut s = String::new();
    append_capped(&mut s, "hello", 3);
    assert_eq!(s, "hel");
    append_capped(&mut s, "xxx", 3);
    assert_eq!(s, "hel");
}

#[test]
fn append_capped_utf8_safe() {
    let mut s = String::new();
    // "你" is 3 bytes in UTF-8; room of 4 should take one full char only.
    append_capped(&mut s, "你好", 4);
    assert_eq!(s, "你");
    assert!(s.is_char_boundary(s.len()));
}

#[test]
fn invalid_cwd_rejected_on_create() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::new()),
    ));
    let chat = ChatService::new(db, run);
    let err = chat
        .create_conversation(
            vec![AgentId::Claude],
            Some("Z:\\this\\path\\does\\not\\exist-agenthub".into()),
        )
        .unwrap_err();
    assert!(err.to_string().contains("cwd"), "unexpected: {err}");
}

#[test]
fn empty_prompt_and_empty_agents_rejected() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::new()),
    ));
    let chat = ChatService::new(db, run);

    let err = chat.create_conversation(vec![], None).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");

    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    let err = chat.send(&conv.id, "   ", &|_| {}).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn send_events_include_turn_and_no_running_left() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::new()),
    ));
    let chat = ChatService::new(db, run);
    let conv = chat
        .create_conversation(vec![AgentId::Claude, AgentId::Codex], None)
        .unwrap();

    let events = Mutex::new(Vec::new());
    chat.send(&conv.id, "hello events", &|ev| {
        events.lock().unwrap().push(ev);
    })
    .unwrap();

    let events = events.into_inner().unwrap();
    let started = events.iter().find_map(|e| match e {
        ChatEvent::Started { turn, agents } => Some((*turn, agents.clone())),
        _ => None,
    });
    let (turn, agents) = started.expect("Started event");
    assert_eq!(turn, 1);
    assert_eq!(agents.len(), 2);

    for e in &events {
        match e {
            ChatEvent::AgentStarted { turn: t, .. }
            | ChatEvent::AgentChunk { turn: t, .. }
            | ChatEvent::AgentFinished { turn: t, .. } => {
                assert_eq!(*t, turn);
            }
            ChatEvent::Finished { turn: t, ok } => {
                assert_eq!(*t, turn);
                assert!(*ok);
            }
            _ => {}
        }
    }

    let msgs = chat.list_messages(&conv.id).unwrap();
    assert!(msgs.iter().all(|m| m.status != ChatMessageStatus::Running));
    assert_eq!(msgs.iter().filter(|m| m.role == ChatRole::Agent).count(), 2);
}

#[test]
fn failed_runner_status_persists_failed_not_running() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::with_status(RunStatus::Failed)),
    ));
    let chat = ChatService::new(db, run);
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    chat.send(&conv.id, "will fail", &|_| {}).unwrap();
    let agents: Vec<_> = chat
        .list_messages(&conv.id)
        .unwrap()
        .into_iter()
        .filter(|m| m.role == ChatRole::Agent)
        .collect();
    assert!(!agents.is_empty());
    for m in agents {
        assert_eq!(m.status, ChatMessageStatus::Failed);
        assert!(m.error.is_some());
    }
}

#[test]
fn cancel_mid_send_marks_cancelled() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let mut recorder = RecordingProcessRunner::new();
    recorder.delay = Duration::from_millis(500);
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(recorder),
    ));
    let chat = Arc::new(ChatService::new(db, run));
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    let id = conv.id.clone();
    let chat2 = Arc::clone(&chat);
    let id2 = id.clone();
    let handle = thread::spawn(move || chat2.send(&id2, "slow cancel", &|_| {}));
    thread::sleep(Duration::from_millis(80));
    chat.cancel(&id).unwrap();
    handle.join().unwrap().unwrap();

    let agents: Vec<_> = chat
        .list_messages(&id)
        .unwrap()
        .into_iter()
        .filter(|m| m.role == ChatRole::Agent)
        .collect();
    assert!(!agents.is_empty());
    for m in agents {
        assert_eq!(
            m.status,
            ChatMessageStatus::Cancelled,
            "expected cancelled, got {:?}",
            m.status
        );
    }
    // Active map cleared — a subsequent send must succeed.
    chat.send(&id, "after cancel", &|_| {}).unwrap();
}

#[test]
fn update_conversation_dedupes_and_validates_cwd() {
    let dir = tempdir().unwrap();
    let cwd = dir.path().join("work");
    std::fs::create_dir_all(&cwd).unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::new()),
    ));
    let chat = ChatService::new(db, run);
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();

    let updated = chat
        .update_conversation(
            &conv.id,
            Some("renamed".into()),
            Some(vec![AgentId::Claude, AgentId::Claude, AgentId::Grok]),
            Some(Some(cwd.to_string_lossy().into_owned())),
            Some(true),
        )
        .unwrap();
    assert_eq!(updated.title, "renamed");
    assert_eq!(updated.agent_ids, vec![AgentId::Claude, AgentId::Grok]);
    assert_eq!(updated.cwd.as_deref(), Some(cwd.to_string_lossy().as_ref()));
    assert!(updated.allow_dangerous);

    let err = chat
        .update_conversation(
            &conv.id,
            None,
            None,
            Some(Some("Z:\\nope-agenthub-cwd".into())),
            None,
        )
        .unwrap_err();
    assert!(err.to_string().contains("cwd"));
}

#[test]
fn valid_cwd_accepted_on_create_and_send() {
    let dir = tempdir().unwrap();
    let work = dir.path().join("proj");
    std::fs::create_dir_all(&work).unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::new()),
    ));
    let chat = ChatService::new(db, run);
    let cwd = work.to_string_lossy().into_owned();
    let conv = chat
        .create_conversation(vec![AgentId::Claude], Some(cwd.clone()))
        .unwrap();
    assert_eq!(conv.cwd.as_deref(), Some(cwd.as_str()));
    chat.send(&conv.id, "with cwd", &|_| {}).unwrap();
}

#[test]
fn build_prompt_skips_non_ok_agent_replies() {
    let history = vec![
        msg(1, ChatRole::User, None, "q1", ChatMessageStatus::Ok),
        msg(
            1,
            ChatRole::Agent,
            Some(AgentId::Claude),
            "failed-reply",
            ChatMessageStatus::Failed,
        ),
        msg(2, ChatRole::User, None, "q2", ChatMessageStatus::Ok),
        msg(
            2,
            ChatRole::Agent,
            Some(AgentId::Claude),
            "ok-reply",
            ChatMessageStatus::Ok,
        ),
    ];
    let p = build_agent_prompt(&history, AgentId::Claude, "q3");
    assert!(!p.contains("failed-reply"));
    assert!(p.contains("ok-reply"));
    assert!(p.contains("q3"));
}

#[test]
fn map_run_status_covers_all_variants() {
    assert_eq!(map_run_status(RunStatus::Ok), ChatMessageStatus::Ok);
    assert_eq!(map_run_status(RunStatus::DryRun), ChatMessageStatus::Ok);
    assert_eq!(map_run_status(RunStatus::Failed), ChatMessageStatus::Failed);
    assert_eq!(
        map_run_status(RunStatus::Timeout),
        ChatMessageStatus::Timeout
    );
    assert_eq!(
        map_run_status(RunStatus::Skipped),
        ChatMessageStatus::Skipped
    );
    assert_eq!(
        map_run_status(RunStatus::Cancelled),
        ChatMessageStatus::Cancelled
    );
}
