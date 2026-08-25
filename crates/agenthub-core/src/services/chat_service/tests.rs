use super::*;
use crate::adapters::{AdapterRegistry, AgentAdapter};
use crate::utils::process::{ProcessRunner, RecordingProcessRunner, StreamingProcessRunner};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tempfile::tempdir;

/// Test runner that returns a canned native session id on top of [`RecordingProcessRunner`].
struct SidProcessRunner {
    inner: RecordingProcessRunner,
    session_id: String,
}

impl SidProcessRunner {
    fn new(session_id: impl Into<String>) -> Self {
        Self {
            inner: RecordingProcessRunner::new(),
            session_id: session_id.into(),
        }
    }

    fn with_status(session_id: impl Into<String>, status: RunStatus) -> Self {
        Self {
            inner: RecordingProcessRunner::with_status(status),
            session_id: session_id.into(),
        }
    }
}

impl ProcessRunner for SidProcessRunner {
    fn run(
        &self,
        spec: &crate::models::RunSpec,
        timeout: Duration,
        max_output_bytes: usize,
    ) -> AgentRunResult {
        let mut result = self.inner.run(spec, timeout, max_output_bytes);
        result.native_session_id = Some(self.session_id.clone());
        result
    }
}

impl StreamingProcessRunner for SidProcessRunner {
    fn run_streaming(
        &self,
        spec: &crate::models::RunSpec,
        timeout: Duration,
        max_output_bytes: usize,
        cancel: &crate::utils::process::CancelToken,
        on_chunk: &(dyn Fn(OutputStream, &str) + Send + Sync),
    ) -> AgentRunResult {
        let mut result =
            self.inner
                .run_streaming(spec, timeout, max_output_bytes, cancel, on_chunk);
        result.native_session_id = Some(self.session_id.clone());
        result
    }
}

struct DeterministicAgentAdapter {
    id: AgentId,
    fail_spec: bool,
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
            notes: Vec::new(), extra_copies: Vec::new(),
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
        if self.fail_spec {
            return Err(crate::error::AppError::InvalidArg(
                "forced spec failure".into(),
            ));
        }
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
    deterministic_registry_with_fail_spec(false)
}

fn deterministic_registry_with_fail_spec(fail_spec: bool) -> AdapterRegistry {
    let mut registry = AdapterRegistry::new();
    for id in AgentId::ALL {
        registry.register(Arc::new(DeterministicAgentAdapter { id, fail_spec }));
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
fn create_dedupes_same_agent_and_rejects_multiple() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::new()),
    ));
    let chat = ChatService::new(db, run);
    let conv = chat
        .create_conversation(vec![AgentId::Claude, AgentId::Claude], None)
        .unwrap();
    assert_eq!(conv.agent_ids, vec![AgentId::Claude]);

    let err = chat
        .create_conversation(vec![AgentId::Claude, AgentId::Codex], None)
        .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(err.to_string().contains("only one agent"));
}

#[test]
fn ensure_default_does_not_merge_titled_or_used_conversations() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::new()),
    ));
    let chat = ChatService::new(db.clone(), run);

    let titled = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    chat.update_conversation(
        &titled.id,
        Some("real conversation".into()),
        None,
        None,
        None,
    )
    .unwrap();

    let used = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    let repo = ChatRepo::new(db);
    repo.insert_message(&ChatMessage {
        id: "used-message".into(),
        conversation_id: used.id.clone(),
        turn: 1,
        role: ChatRole::User,
        agent_id: None,
        content: "already used".into(),
        status: ChatMessageStatus::Ok,
        exit_code: None,
        duration_ms: 0,
        error: None,
        created_at: Utc::now().to_rfc3339(),
    })
    .unwrap();

    let ensured = chat
        .ensure_default_conversation(vec![AgentId::Codex], None)
        .unwrap();
    assert_ne!(ensured.id, titled.id);
    assert_ne!(ensured.id, used.id);
    assert_eq!(chat.list_conversations().unwrap().len(), 3);

    let repeated = chat
        .ensure_default_conversation(vec![AgentId::Claude], None)
        .unwrap();
    assert_eq!(repeated.id, ensured.id);
    assert_eq!(chat.list_conversations().unwrap().len(), 3);
}

#[test]
fn ensure_default_is_idempotent_under_concurrent_service_calls() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::new()),
    ));
    let chat = Arc::new(ChatService::new(db, run));
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let mut handles = Vec::new();

    for _ in 0..8 {
        let chat = Arc::clone(&chat);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            chat.ensure_default_conversation(vec![AgentId::Claude], None)
                .unwrap()
        }));
    }

    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert!(results.windows(2).all(|rows| rows[0].id == rows[1].id));
    assert_eq!(chat.list_conversations().unwrap().len(), 1);

    // The explicit API remains an always-insert operation for user-requested
    // new chats; only the dedicated ensure API is idempotent.
    let explicit_a = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    let explicit_b = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    assert_ne!(explicit_a.id, explicit_b.id);
    assert_eq!(chat.list_conversations().unwrap().len(), 3);
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
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();

    // First turn
    let events = Mutex::new(Vec::new());
    chat.send(&conv.id, "first question", &|ev| {
        events.lock().unwrap().push(ev);
    })
    .unwrap();

    let msgs = chat.list_messages(&conv.id).unwrap();
    assert!(msgs.iter().any(|m| m.role == ChatRole::User));
    assert_eq!(msgs.iter().filter(|m| m.role == ChatRole::Agent).count(), 1);
    assert!(msgs.iter().all(|m| m.status != ChatMessageStatus::Running));

    // Second turn: RecordingProcessRunner returns mock:{agent}; verify history used.
    calls.lock().unwrap().clear();
    chat.send(&conv.id, "second", &|_| {}).unwrap();
    let specs = calls.lock().unwrap().clone();
    assert_eq!(specs.len(), 1, "the selected agent must reach the runner");
    assert_eq!(specs[0].agent, AgentId::Claude);
    let joined = specs[0].args.join(" ");
    assert!(
        joined.contains("second") || joined.contains("first question"),
        "unexpected args for claude: {joined}"
    );

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
        .create_conversation(vec![AgentId::Claude], None)
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
    assert_eq!(agents, vec![AgentId::Claude]);

    for e in &events {
        match e {
            ChatEvent::AgentStarted { turn: t, .. }
            | ChatEvent::AgentChunk { turn: t, .. }
            | ChatEvent::AgentFinished { turn: t, .. } => {
                assert_eq!(*t, turn);
            }
            ChatEvent::Finished {
                turn: t,
                ok,
                cancelled,
            } => {
                assert_eq!(*t, turn);
                assert!(*ok);
                assert!(!*cancelled);
            }
            _ => {}
        }
    }

    let msgs = chat.list_messages(&conv.id).unwrap();
    assert!(msgs.iter().all(|m| m.status != ChatMessageStatus::Running));
    assert_eq!(msgs.iter().filter(|m| m.role == ChatRole::Agent).count(), 1);
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
            Some(vec![AgentId::Grok, AgentId::Grok]),
            Some(Some(cwd.to_string_lossy().into_owned())),
            Some(true),
        )
        .unwrap();
    assert_eq!(updated.title, "renamed");
    assert_eq!(updated.agent_ids, vec![AgentId::Grok]);
    assert_eq!(updated.cwd.as_deref(), Some(cwd.to_string_lossy().as_ref()));
    assert!(updated.allow_dangerous);

    let multi = chat
        .update_conversation(
            &conv.id,
            None,
            Some(vec![AgentId::Claude, AgentId::Grok]),
            None,
            None,
        )
        .unwrap_err();
    assert_eq!(multi.code(), "invalid_arg");

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

#[test]
fn print_resume_sends_only_the_new_user_turn() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let recorder = RecordingProcessRunner::new();
    let calls = Arc::clone(&recorder.calls);
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(recorder),
    ));
    let chat = ChatService::new(db.clone(), run);
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    chat.send(&conv.id, "first question", &|_| {}).unwrap();

    let repo = crate::storage::ChatRepo::new(db);
    let mut stored = repo.get_conversation(&conv.id).unwrap().unwrap();
    stored.native_session_id = Some("sess-1".into());
    repo.update_conversation(&stored).unwrap();

    calls.lock().unwrap().clear();
    chat.send(&conv.id, "second", &|_| {}).unwrap();
    let specs = calls.lock().unwrap().clone();
    assert_eq!(specs.len(), 1);
    let joined = specs[0].args.join(" ");
    assert!(joined.contains("second"), "unexpected args: {joined}");
    assert!(
        !joined.contains("first question") && !joined.contains("[用户]"),
        "resume must not resend Hub history: {joined}"
    );
}

#[test]
fn resume_hard_failure_clears_native_session_id() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::with_status(RunStatus::Failed)),
    ));
    let chat = ChatService::new(db.clone(), run);
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    let repo = crate::storage::ChatRepo::new(db);
    let mut stored = repo.get_conversation(&conv.id).unwrap().unwrap();
    stored.native_session_id = Some("dead-sid".into());
    repo.update_conversation(&stored).unwrap();

    chat.send(&conv.id, "resume fail", &|_| {}).unwrap();
    let after = chat.get_conversation(&conv.id).unwrap();
    assert!(
        after.native_session_id.is_none(),
        "hard failure after resume must clear sid, got {:?}",
        after.native_session_id
    );
}

#[test]
fn resume_timeout_clears_native_session_id() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::with_status(RunStatus::Timeout)),
    ));
    let chat = ChatService::new(db.clone(), run);
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    let repo = crate::storage::ChatRepo::new(db);
    let mut stored = repo.get_conversation(&conv.id).unwrap().unwrap();
    stored.native_session_id = Some("dead-sid".into());
    repo.update_conversation(&stored).unwrap();

    chat.send(&conv.id, "resume timeout", &|_| {}).unwrap();
    let after = chat.get_conversation(&conv.id).unwrap();
    assert!(
        after.native_session_id.is_none(),
        "Timeout is a hard failure and must clear sid, got {:?}",
        after.native_session_id
    );
}

#[test]
fn resume_cancelled_keeps_native_session_id() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::with_status(RunStatus::Cancelled)),
    ));
    let chat = ChatService::new(db.clone(), run);
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    let repo = crate::storage::ChatRepo::new(db);
    let mut stored = repo.get_conversation(&conv.id).unwrap().unwrap();
    stored.native_session_id = Some("dead-sid".into());
    repo.update_conversation(&stored).unwrap();

    chat.send(&conv.id, "resume cancel", &|_| {}).unwrap();
    let after = chat.get_conversation(&conv.id).unwrap();
    assert_eq!(
        after.native_session_id.as_deref(),
        Some("dead-sid"),
        "Cancelled must not clear the native session id"
    );
}

#[test]
fn resume_run_error_clears_native_session_id() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry_with_fail_spec(true),
        Arc::new(RecordingProcessRunner::new()),
    ));
    let chat = ChatService::new(db.clone(), run);
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    let repo = crate::storage::ChatRepo::new(db);
    let mut stored = repo.get_conversation(&conv.id).unwrap().unwrap();
    stored.native_session_id = Some("dead-sid".into());
    repo.update_conversation(&stored).unwrap();

    let err = chat
        .send(&conv.id, "resume spec fail", &|_| {})
        .expect_err("build_run_spec failure must fail the send");
    assert!(format!("{err}").contains("forced spec failure"));
    let after = chat.get_conversation(&conv.id).unwrap();
    assert!(
        after.native_session_id.is_none(),
        "run_each Err after resume must clear sid, got {:?}",
        after.native_session_id
    );
}

#[test]
fn resume_hard_failure_discards_new_sid_from_results() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(SidProcessRunner::with_status(
            "fresh-sid",
            RunStatus::Failed,
        )),
    ));
    let chat = ChatService::new(db.clone(), run);
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    let repo = crate::storage::ChatRepo::new(db);
    let mut stored = repo.get_conversation(&conv.id).unwrap().unwrap();
    stored.native_session_id = Some("dead-sid".into());
    repo.update_conversation(&stored).unwrap();

    chat.send(&conv.id, "resume fail with fresh sid", &|_| {})
        .unwrap();
    let after = chat.get_conversation(&conv.id).unwrap();
    assert!(
        after.native_session_id.is_none(),
        "hard failure must clear sid even if results carry a new one, got {:?}",
        after.native_session_id
    );
}

#[test]
fn persist_native_session_id_when_cwd_and_agent_unchanged() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(SidProcessRunner::new("fresh-sid")),
    ));
    let chat = ChatService::new(db, run);
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    chat.send(&conv.id, "first", &|_| {}).unwrap();
    let after = chat.get_conversation(&conv.id).unwrap();
    assert_eq!(after.native_session_id.as_deref(), Some("fresh-sid"));
}

#[test]
fn persist_skips_native_session_when_cwd_changes_during_send() {
    let dir = tempdir().unwrap();
    let work_a = dir.path().join("a");
    let work_b = dir.path().join("b");
    std::fs::create_dir_all(&work_a).unwrap();
    std::fs::create_dir_all(&work_b).unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(SidProcessRunner::new("sid-from-old-cwd")),
    ));
    let chat = Arc::new(ChatService::new(db, run));
    let cwd_a = work_a.to_string_lossy().into_owned();
    let cwd_b = work_b.to_string_lossy().into_owned();
    let conv = chat
        .create_conversation(vec![AgentId::Claude], Some(cwd_a))
        .unwrap();

    let chat2 = Arc::clone(&chat);
    let id = conv.id.clone();
    let cwd_b_cb = cwd_b.clone();
    chat.send(&conv.id, "hello", &move |ev| {
        if matches!(ev, ChatEvent::AgentStarted { .. }) {
            chat2
                .update_conversation(&id, None, None, Some(Some(cwd_b_cb.clone())), None)
                .unwrap();
        }
    })
    .unwrap();

    let after = chat.get_conversation(&conv.id).unwrap();
    assert_eq!(after.cwd.as_deref(), Some(cwd_b.as_str()));
    assert!(
        after.native_session_id.is_none(),
        "sid from old cwd must not be persisted, got {:?}",
        after.native_session_id
    );
}

#[test]
fn persist_skips_native_session_when_agent_changes_during_send() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(SidProcessRunner::new("sid-from-old-agent")),
    ));
    let chat = Arc::new(ChatService::new(db, run));
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();

    let chat2 = Arc::clone(&chat);
    let id = conv.id.clone();
    chat.send(&conv.id, "hello", &move |ev| {
        if matches!(ev, ChatEvent::AgentStarted { .. }) {
            chat2
                .update_conversation(&id, None, Some(vec![AgentId::Codex]), None, None)
                .unwrap();
        }
    })
    .unwrap();

    let after = chat.get_conversation(&conv.id).unwrap();
    assert_eq!(after.agent_ids, vec![AgentId::Codex]);
    assert!(
        after.native_session_id.is_none(),
        "sid from old agent must not be persisted, got {:?}",
        after.native_session_id
    );
}

fn insert_running_agent_message(repo: &crate::storage::ChatRepo, conversation_id: &str) {
    repo.insert_message(&ChatMessage {
        id: format!("m-running-{conversation_id}"),
        conversation_id: conversation_id.into(),
        turn: 1,
        role: ChatRole::Agent,
        agent_id: Some(AgentId::Claude),
        content: String::new(),
        status: ChatMessageStatus::Running,
        exit_code: None,
        duration_ms: 0,
        error: None,
        created_at: Utc::now().to_rfc3339(),
    })
    .unwrap();
}

#[test]
fn interrupt_stale_running_marks_cancelled_and_keeps_native_session() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = crate::storage::ChatRepo::new(db.clone());
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::new()),
    ));
    let chat = ChatService::new(db, run);
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();
    let mut stored = repo.get_conversation(&conv.id).unwrap().unwrap();
    stored.native_session_id = Some("keep-sid".into());
    repo.update_conversation(&stored).unwrap();
    insert_running_agent_message(&repo, &conv.id);

    let n = repo.interrupt_stale_running().unwrap();
    assert_eq!(n, 1);

    let agents: Vec<_> = chat
        .list_messages(&conv.id)
        .unwrap()
        .into_iter()
        .filter(|m| m.role == ChatRole::Agent)
        .collect();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].status, ChatMessageStatus::Cancelled);
    assert!(agents[0].error.is_none());

    let after = chat.get_conversation(&conv.id).unwrap();
    assert_eq!(after.native_session_id.as_deref(), Some("keep-sid"));
}

#[test]
fn agent_hub_open_repairs_stale_running_messages() {
    let dir = tempdir().unwrap();
    let conv_id = {
        let db = Database::open(&crate::utils::paths::db_path(dir.path())).unwrap();
        let repo = crate::storage::ChatRepo::new(db.clone());
        let run = Arc::new(RunService::with_runner(
            deterministic_registry(),
            Arc::new(RecordingProcessRunner::new()),
        ));
        let chat = ChatService::new(db, run);
        let conv = chat
            .create_conversation(vec![AgentId::Claude], None)
            .unwrap();
        insert_running_agent_message(&repo, &conv.id);
        conv.id
    };

    let hub = crate::AgentHub::open(Some(dir.path())).expect("open hub");
    let agents: Vec<_> = hub
        .chat
        .list_messages(&conv_id)
        .unwrap()
        .into_iter()
        .filter(|m| m.role == ChatRole::Agent)
        .collect();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].status, ChatMessageStatus::Cancelled);
}

#[test]
fn list_conversations_sending_tracks_running_until_interrupt() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = crate::storage::ChatRepo::new(db.clone());
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(RecordingProcessRunner::new()),
    ));
    let chat = ChatService::new(db, run);
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();

    let listed = chat.list_conversations().unwrap();
    let row = listed.iter().find(|c| c.id == conv.id).expect("listed");
    assert!(!row.sending);
    assert!(!chat.get_conversation(&conv.id).unwrap().sending);

    insert_running_agent_message(&repo, &conv.id);
    let listed = chat.list_conversations().unwrap();
    let row = listed.iter().find(|c| c.id == conv.id).expect("listed");
    assert!(row.sending);
    assert!(chat.get_conversation(&conv.id).unwrap().sending);

    assert_eq!(repo.interrupt_stale_running().unwrap(), 1);
    let listed = chat.list_conversations().unwrap();
    let row = listed.iter().find(|c| c.id == conv.id).expect("listed");
    assert!(!row.sending);
    assert!(!chat.get_conversation(&conv.id).unwrap().sending);
}

#[test]
fn finished_event_cancelled_true_and_ok_true() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let run = Arc::new(RunService::with_runner(
        deterministic_registry(),
        Arc::new(SidProcessRunner::with_status("sid", RunStatus::Cancelled)),
    ));
    let chat = ChatService::new(db, run);
    let conv = chat
        .create_conversation(vec![AgentId::Claude], None)
        .unwrap();

    let events = Mutex::new(Vec::new());
    chat.send(&conv.id, "cancel path", &|ev| {
        events.lock().unwrap().push(ev);
    })
    .unwrap();

    let events = events.into_inner().unwrap();
    let finished = events.iter().find_map(|e| match e {
        ChatEvent::Finished {
            turn,
            ok,
            cancelled,
        } => Some((*turn, *ok, *cancelled)),
        _ => None,
    });
    let (turn, ok, cancelled) = finished.expect("Finished event");
    assert_eq!(turn, 1);
    assert!(ok, "Cancelled is not a hard failure");
    assert!(cancelled);
}
