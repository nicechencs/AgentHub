//! One `codex app-server` process per conversation, kept across turns.
//!
//! A fake app-server echoes request ids, logs every spawn and method, and
//! answers `turn/start` with `turn-N` plus a delta and `turn/completed`.
//! Control files in its directory change its behavior per test.

#![cfg(unix)]

use super::*;

use crate::adapters::AdapterRegistry;
use crate::models::{AgentId, ChatMessageStatus, Conversation};
use crate::services::RunService;
use crate::storage::{ChatRepo, Database};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const FAKE_CODEX: &str = r##"#!/bin/sh
dir="$(dirname "$0")"
printf 'spawn %s\n' "$$" >> "$dir/spawns.log"
loaded=0
turn=""
reply() {
  printf '%s\n' "$1"
}
while IFS= read -r line; do
  id=$(printf '%s' "$line" | grep -o '"id":[0-9]*' | head -1 | cut -d: -f2)
  method=$(printf '%s' "$line" | grep -o '"method":"[^"]*"' | head -1 | cut -d'"' -f4)
  if [ -z "$method" ]; then
    printf 'response %s\n' "$line" >> "$dir/methods.log"
    continue
  fi
  printf '%s\n' "$method" >> "$dir/methods.log"
  case "$method" in
    initialize)
      reply "{\"id\":$id,\"result\":{}}"
      ;;
    initialized)
      ;;
    thread/start)
      loaded=1
      reply "{\"id\":$id,\"result\":{\"thread\":{\"id\":\"thread-1\"}}}"
      ;;
    thread/resume)
      loaded=1
      reply "{\"id\":$id,\"result\":{\"thread\":{\"id\":\"thread-1\"}}}"
      ;;
    turn/start)
      printf '%s\n' "$line" >> "$dir/turn-starts.log"
      if [ "$loaded" != 1 ]; then
        reply "{\"id\":$id,\"error\":{\"code\":-32600,\"message\":\"thread not found: thread-1\"}}"
        continue
      fi
      n=$(cat "$dir/turn-count" 2>/dev/null || echo 0)
      n=$((n + 1))
      printf '%s\n' "$n" > "$dir/turn-count"
      turn="turn-$n"
      reply "{\"id\":$id,\"result\":{\"turn\":{\"id\":\"$turn\",\"status\":\"inProgress\"}}}"
      reply "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thread-1\",\"turn\":{\"id\":\"$turn\",\"status\":\"inProgress\"}}}"
      if [ -f "$dir/hold" ]; then
        continue
      fi
      reply "{\"method\":\"item/agentMessage/delta\",\"params\":{\"threadId\":\"thread-1\",\"turnId\":\"$turn\",\"itemId\":\"m\",\"delta\":\"reply-$n\"}}"
      reply "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thread-1\",\"turn\":{\"id\":\"$turn\",\"status\":\"completed\"}}}"
      if [ -f "$dir/late-after-turn" ]; then
        reply "{\"method\":\"item/agentMessage/delta\",\"params\":{\"threadId\":\"thread-1\",\"turnId\":\"$turn\",\"itemId\":\"m\",\"delta\":\"late\"}}"
        reply "{\"id\":\"late-$n\",\"method\":\"item/commandExecution/requestApproval\",\"params\":{\"threadId\":\"thread-1\",\"turnId\":\"$turn\",\"itemId\":\"c\",\"command\":\"ls\"}}"
      fi
      if [ -f "$dir/close-after-turn" ]; then
        rm -f "$dir/close-after-turn"
        loaded=0
        reply "{\"method\":\"thread/closed\",\"params\":{\"threadId\":\"thread-1\"}}"
      fi
      if [ -f "$dir/exit-after-turn" ]; then
        rm -f "$dir/exit-after-turn"
        exit 0
      fi
      ;;
    turn/interrupt)
      reply "{\"id\":$id,\"result\":{}}"
      if [ ! -f "$dir/ignore-interrupt" ]; then
        reply "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thread-1\",\"turn\":{\"id\":\"$turn\",\"status\":\"interrupted\"}}}"
      fi
      ;;
    *)
      if [ -n "$id" ]; then
        reply "{\"id\":$id,\"result\":{}}"
      fi
      ;;
  esac
done
"##;

struct FakeCodex {
    directory: tempfile::TempDir,
    program: PathBuf,
}

impl FakeCodex {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join("fake-codex");
        std::fs::write(&program, FAKE_CODEX).unwrap();
        let mut permissions = std::fs::metadata(&program).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&program, permissions).unwrap();
        Self { directory, program }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }

    fn flag(&self, name: &str) {
        std::fs::write(self.path(name), "1").unwrap();
    }

    fn clear(&self, name: &str) {
        let _ = std::fs::remove_file(self.path(name));
    }

    fn lines(&self, name: &str) -> Vec<String> {
        std::fs::read_to_string(self.path(name))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn spawns(&self) -> usize {
        self.lines("spawns.log").len()
    }

    fn method_count(&self, method: &str) -> usize {
        self.lines("methods.log")
            .iter()
            .filter(|line| line.as_str() == method)
            .count()
    }
}

fn codex_conversation(db: &Database, id: &str, cwd: &Path) {
    ChatRepo::new(db.clone())
        .create_conversation(&Conversation {
            id: id.into(),
            title: String::new(),
            agent_ids: vec![AgentId::Codex],
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

fn test_models() -> Vec<RuntimeModelOption> {
    vec![
        RuntimeModelOption {
            id: "gpt-test".into(),
            efforts: vec!["low".into(), "high".into()],
            default_effort: Some("low".into()),
        },
        RuntimeModelOption {
            id: "gpt-other".into(),
            efforts: vec!["low".into()],
            default_effort: Some("low".into()),
        },
    ]
}

fn runtime_with(fake: &FakeCodex, id: &str) -> (Arc<ChatRuntime>, tempfile::TempDir) {
    let cwd = tempfile::tempdir().unwrap();
    let db = Database::open_in_memory().unwrap();
    codex_conversation(&db, id, cwd.path());
    let runtime = Arc::new(ChatRuntime::new(
        db,
        Arc::new(RunService::new(AdapterRegistry::default())),
    ));
    runtime.store.enable_if_new(id).unwrap();
    runtime.set_codex_program_for_test(fake.program.clone());
    runtime.seed_catalog_cache_for_test(id, test_models(), vec![]);
    (runtime, cwd)
}

fn wait_phase(runtime: &ChatRuntime, id: &str, wanted: &[RuntimePhase]) -> RuntimeSnapshot {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = runtime.snapshot(id, None).unwrap();
        if wanted.contains(&snapshot.phase) {
            return snapshot;
        }
        if Instant::now() > deadline {
            panic!("phase stayed {:?}, wanted {wanted:?}", snapshot.phase);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        if Instant::now() > deadline {
            panic!("timed out waiting for {what}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn run_turn(runtime: &Arc<ChatRuntime>, id: &str, client: &str) -> RuntimeSnapshot {
    runtime
        .start(id, "hello", client, RuntimeStartExtras::default())
        .unwrap();
    wait_phase(runtime, id, &[RuntimePhase::Completed])
}

fn live_process_count(fake: &FakeCodex) -> usize {
    fake.lines("spawns.log")
        .iter()
        .filter_map(|line| line.strip_prefix("spawn "))
        .filter(|pid| Path::new(&format!("/proc/{pid}")).exists())
        .count()
}

// T1
#[test]
fn codex_reuses_one_process_across_turns() {
    let fake = FakeCodex::new();
    let (runtime, _cwd) = runtime_with(&fake, "live-reuse");
    let first = run_turn(&runtime, "live-reuse", "c-1");
    assert_eq!(first.current_message.unwrap().content, "reply-1");
    run_turn(&runtime, "live-reuse", "c-2");
    // A model change travels with `turn/start`; it never restarts Codex.
    runtime
        .store
        .set_turn_settings(
            "live-reuse",
            &RuntimeTurnSettings {
                model: Some("gpt-other".into()),
                effort: Some("low".into()),
            },
        )
        .unwrap();
    let third = run_turn(&runtime, "live-reuse", "c-3");
    assert_eq!(third.current_message.unwrap().content, "reply-3");

    assert_eq!(fake.spawns(), 1, "one process for three turns");
    assert_eq!(fake.method_count("thread/start"), 1);
    assert_eq!(fake.method_count("thread/resume"), 0);
    assert_eq!(fake.method_count("turn/start"), 3);
    let starts = fake.lines("turn-starts.log");
    assert!(
        starts[2].contains("\"model\":\"gpt-other\""),
        "{}",
        starts[2]
    );
    assert_eq!(live_process_count(&fake), 1);
    runtime.shutdown("live-reuse");
}

// T2
#[test]
fn codex_respawns_and_resumes_after_the_process_exits() {
    let fake = FakeCodex::new();
    let (runtime, _cwd) = runtime_with(&fake, "live-exit");
    fake.flag("exit-after-turn");
    run_turn(&runtime, "live-exit", "c-1");
    wait_until("the first process to exit", || {
        live_process_count(&fake) == 0
    });
    let second = run_turn(&runtime, "live-exit", "c-2");
    assert_eq!(second.current_message.unwrap().content, "reply-2");
    assert_eq!(fake.spawns(), 2);
    assert_eq!(fake.method_count("thread/start"), 1);
    assert_eq!(fake.method_count("thread/resume"), 1);
    runtime.shutdown("live-exit");
}

#[test]
fn codex_resumes_after_thread_closed_without_a_new_process() {
    let fake = FakeCodex::new();
    let (runtime, _cwd) = runtime_with(&fake, "live-closed");
    fake.flag("close-after-turn");
    run_turn(&runtime, "live-closed", "c-1");
    // Give the idle poll a moment to read `thread/closed`.
    std::thread::sleep(Duration::from_millis(300));
    run_turn(&runtime, "live-closed", "c-2");
    assert_eq!(fake.spawns(), 1);
    assert_eq!(fake.method_count("thread/resume"), 1);
    assert_eq!(fake.method_count("turn/start"), 2);
    runtime.shutdown("live-closed");
}

#[test]
fn codex_turn_start_on_an_unloaded_thread_resumes_and_retries_once() {
    let fake = FakeCodex::new();
    let db = Database::open_in_memory().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    codex_conversation(&db, "w-unloaded", cwd.path());
    let mut worker = test_worker(&db, "w-unloaded");
    worker.store.enable_if_new("w-unloaded").unwrap();
    begin_running_turn(&mut worker);
    *worker.codex_program_override.lock().unwrap() = Some(fake.program.clone());
    // The actor believes the thread is loaded, but this process never saw
    // it (as after a missed `thread/closed`): only `turn/start` tells.
    attach_live(&mut worker, &fake);
    worker
        .codex_connect_and_start("hello", "c-2", &RuntimeStartExtras::default())
        .unwrap();
    assert_eq!(fake.spawns(), 1);
    assert_eq!(
        fake.lines("methods.log")
            .into_iter()
            .filter(|line| !line.starts_with("response")
                && line != "initialize"
                && line != "initialized")
            .collect::<Vec<_>>(),
        vec!["turn/start", "thread/resume", "turn/start"]
    );
    assert_eq!(worker.turn_id.as_deref(), Some("turn-1"));
    assert!(worker
        .codex_live
        .as_ref()
        .is_some_and(|live| live.thread_loaded));
}

// T3 / T4: late or foreign notifications against a kept process.
#[test]
fn late_codex_events_after_completion_do_not_reopen_the_turn() {
    let fake = FakeCodex::new();
    let (runtime, _cwd) = runtime_with(&fake, "live-late");
    fake.flag("late-after-turn");
    let done = run_turn(&runtime, "live-late", "c-1");
    wait_until("the late approval to be answered", || {
        fake.lines("methods.log")
            .iter()
            .any(|line| line.starts_with("response") && line.contains("late-1"))
    });
    let after = runtime.snapshot("live-late", None).unwrap();
    assert_eq!(after.phase, RuntimePhase::Completed);
    assert!(after.pending_requests.is_empty());
    let message = after.current_message.unwrap();
    assert_eq!(message.content, "reply-1");
    assert_eq!(message.status, ChatMessageStatus::Ok);
    assert_eq!(after.last_sequence, done.last_sequence);
    let response = fake
        .lines("methods.log")
        .into_iter()
        .find(|line| line.starts_with("response") && line.contains("late-1"))
        .unwrap();
    assert!(
        response.contains("\"error\""),
        "an idle approval is cancelled, never allowed: {response}"
    );
    runtime.shutdown("live-late");
}

// T5: worker level — an approval for another thread is refused.
#[test]
fn codex_approval_for_another_thread_is_refused() {
    let fake = FakeCodex::new();
    let db = Database::open_in_memory().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    codex_conversation(&db, "w-foreign", cwd.path());
    let mut worker = test_worker(&db, "w-foreign");
    worker.store.enable_if_new("w-foreign").unwrap();
    begin_running_turn(&mut worker);
    worker.transport = Some(CodexTransport::spawn(&fake.program, fake.directory.path()).unwrap());
    worker
        .server_request(
            json!("foreign-1"),
            "item/commandExecution/requestApproval",
            &json!({"threadId": "thread-9", "turnId": "run-1", "command": "ls"}),
        )
        .unwrap();
    let snapshot = worker.store.snapshot("w-foreign", None).unwrap();
    assert!(snapshot.pending_requests.is_empty());
    assert_eq!(snapshot.phase, RuntimePhase::Running);
    wait_until("the foreign approval to be refused", || {
        fake.lines("methods.log").iter().any(|line| {
            line.starts_with("response") && line.contains("foreign-1") && line.contains("error")
        })
    });
}

#[test]
fn idle_codex_approval_is_cancelled_without_waiting() {
    let fake = FakeCodex::new();
    let db = Database::open_in_memory().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    codex_conversation(&db, "w-idle", cwd.path());
    let mut worker = test_worker(&db, "w-idle");
    worker.store.enable_if_new("w-idle").unwrap();
    begin_running_turn(&mut worker);
    worker
        .turn_completed(
            &json!({"threadId": "thread-1", "turn": {"id": "run-1", "status": "completed"}}),
        )
        .unwrap();
    worker.transport = Some(CodexTransport::spawn(&fake.program, fake.directory.path()).unwrap());
    worker
        .server_request(
            json!("idle-1"),
            "item/fileChange/requestApproval",
            &json!({"threadId": "thread-1", "turnId": "run-1", "changes": []}),
        )
        .unwrap();
    let snapshot = worker.store.snapshot("w-idle", None).unwrap();
    assert_eq!(snapshot.phase, RuntimePhase::Completed);
    assert!(snapshot.pending_requests.is_empty());
    wait_until("the idle approval to be answered", || {
        fake.lines("methods.log")
            .iter()
            .any(|line| line.starts_with("response") && line.contains("idle-1"))
    });
}

// T6
#[test]
fn login_change_restarts_the_idle_codex_process() {
    let fake = FakeCodex::new();
    let (runtime, _cwd) = runtime_with(&fake, "live-login");
    run_turn(&runtime, "live-login", "c-1");
    runtime.invalidate_catalogs();
    wait_until("the old process to stop", || live_process_count(&fake) == 0);
    run_turn(&runtime, "live-login", "c-2");
    assert_eq!(fake.spawns(), 2);
    assert_eq!(fake.method_count("thread/resume"), 1);
    assert_eq!(live_process_count(&fake), 1);
    runtime.shutdown("live-login");
}

#[test]
fn login_change_mid_turn_restarts_after_the_turn() {
    let fake = FakeCodex::new();
    let (runtime, _cwd) = runtime_with(&fake, "live-login-busy");
    fake.flag("hold");
    runtime
        .start(
            "live-login-busy",
            "hello",
            "c-1",
            RuntimeStartExtras::default(),
        )
        .unwrap();
    wait_phase(&runtime, "live-login-busy", &[RuntimePhase::Running]);
    runtime.invalidate_catalogs();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        live_process_count(&fake),
        1,
        "a running turn keeps its process"
    );
    let run_id = runtime
        .snapshot("live-login-busy", None)
        .unwrap()
        .run_id
        .unwrap();
    runtime.cancel("live-login-busy", &run_id).unwrap();
    wait_phase(&runtime, "live-login-busy", &[RuntimePhase::Cancelled]);
    wait_until("the process to stop after the turn", || {
        live_process_count(&fake) == 0
    });
    fake.clear("hold");
    run_turn(&runtime, "live-login-busy", "c-2");
    assert_eq!(fake.spawns(), 2);
    runtime.shutdown("live-login-busy");
}

#[test]
fn cwd_change_restarts_codex() {
    let fake = FakeCodex::new();
    let (runtime, _cwd) = runtime_with(&fake, "live-cwd");
    run_turn(&runtime, "live-cwd", "c-1");
    let other = tempfile::tempdir().unwrap();
    let mut conversation = runtime.repo.get_conversation("live-cwd").unwrap().unwrap();
    conversation.cwd = Some(other.path().to_string_lossy().into_owned());
    runtime.repo.update_conversation(&conversation).unwrap();
    run_turn(&runtime, "live-cwd", "c-2");
    assert_eq!(fake.spawns(), 2);
    assert_eq!(fake.method_count("thread/resume"), 1);
    assert_eq!(live_process_count(&fake), 1);
    runtime.shutdown("live-cwd");
}

// T7
#[test]
fn clearing_always_allow_restarts_the_idle_codex_process() {
    let fake = FakeCodex::new();
    let (runtime, _cwd) = runtime_with(&fake, "live-clear");
    run_turn(&runtime, "live-clear", "c-1");
    assert_eq!(live_process_count(&fake), 1);
    runtime.clear_session_allow_always("live-clear").unwrap();
    wait_until("the process to stop", || live_process_count(&fake) == 0);
    run_turn(&runtime, "live-clear", "c-2");
    assert_eq!(fake.spawns(), 2);
    assert_eq!(fake.method_count("thread/resume"), 1);
    runtime.shutdown("live-clear");
}

#[test]
fn clearing_always_allow_mid_turn_restarts_after_the_turn() {
    let fake = FakeCodex::new();
    let db = Database::open_in_memory().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    codex_conversation(&db, "w-clear", cwd.path());
    let mut worker = test_worker(&db, "w-clear");
    worker.store.enable_if_new("w-clear").unwrap();
    begin_running_turn(&mut worker);
    attach_live(&mut worker, &fake);
    worker.forget_session_allow_always();
    assert!(
        worker
            .transport
            .as_ref()
            .is_some_and(CodexTransport::is_open),
        "a running turn keeps its process"
    );
    worker
        .turn_completed(
            &json!({"threadId": "thread-1", "turn": {"id": "run-1", "status": "completed"}}),
        )
        .unwrap();
    worker.reap_codex_process();
    assert!(worker.transport.is_none(), "restart once the turn is over");
}

// T8
#[test]
fn idle_codex_process_is_reclaimed_after_the_timeout() {
    let fake = FakeCodex::new();
    let db = Database::open_in_memory().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    codex_conversation(&db, "w-idle-reap", cwd.path());
    let mut worker = test_worker(&db, "w-idle-reap");
    worker.store.enable_if_new("w-idle-reap").unwrap();
    begin_running_turn(&mut worker);
    attach_live(&mut worker, &fake);
    worker.codex_idle_timeout = Duration::from_millis(50);
    std::thread::sleep(Duration::from_millis(80));
    worker.reap_codex_process();
    assert!(
        worker.transport.is_some(),
        "a running turn is never reclaimed"
    );
    worker
        .turn_completed(
            &json!({"threadId": "thread-1", "turn": {"id": "run-1", "status": "completed"}}),
        )
        .unwrap();
    worker.reap_codex_process();
    assert!(worker.transport.is_some(), "not idle long enough yet");
    std::thread::sleep(Duration::from_millis(80));
    worker.reap_codex_process();
    assert!(worker.transport.is_none());
    assert!(worker.codex_live.is_none());
}

// T9
#[test]
fn shutdown_kills_the_kept_codex_process() {
    let fake = FakeCodex::new();
    let (runtime, _cwd) = runtime_with(&fake, "live-shutdown");
    run_turn(&runtime, "live-shutdown", "c-1");
    assert_eq!(live_process_count(&fake), 1);
    runtime.shutdown("live-shutdown");
    wait_until("the process to be killed", || {
        live_process_count(&fake) == 0
    });
}

// T10 / T11: soft stop keeps the process.
#[test]
fn codex_stop_interrupts_and_keeps_the_process() {
    let fake = FakeCodex::new();
    let (runtime, _cwd) = runtime_with(&fake, "live-stop");
    fake.flag("hold");
    runtime
        .start("live-stop", "hello", "c-1", RuntimeStartExtras::default())
        .unwrap();
    let running = wait_phase(&runtime, "live-stop", &[RuntimePhase::Running]);
    runtime
        .cancel("live-stop", running.run_id.as_deref().unwrap())
        .unwrap();
    let stopped = wait_phase(&runtime, "live-stop", &[RuntimePhase::Cancelled]);
    assert_eq!(
        stopped.current_message.unwrap().status,
        ChatMessageStatus::Cancelled
    );
    assert_eq!(fake.method_count("turn/interrupt"), 1);
    assert_eq!(live_process_count(&fake), 1, "stop keeps Codex running");
    fake.clear("hold");
    let next = run_turn(&runtime, "live-stop", "c-2");
    assert_eq!(next.current_message.unwrap().content, "reply-2");
    assert_eq!(fake.spawns(), 1);
    assert_eq!(fake.method_count("thread/resume"), 0);
    runtime.shutdown("live-stop");
}

#[test]
fn codex_stop_without_completion_kills_after_the_deadline() {
    let fake = FakeCodex::new();
    let db = Database::open_in_memory().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    codex_conversation(&db, "w-stop-deadline", cwd.path());
    let mut worker = test_worker(&db, "w-stop-deadline");
    worker.store.enable_if_new("w-stop-deadline").unwrap();
    begin_running_turn(&mut worker);
    attach_live(&mut worker, &fake);
    fake.flag("ignore-interrupt");
    worker.cancel("run-1").unwrap();
    assert!(worker.cancel_deadline.is_some());
    assert!(worker
        .transport
        .as_ref()
        .is_some_and(CodexTransport::is_open));
    worker.cancel_deadline = Some(Instant::now());
    worker.check_cancel_deadline().unwrap();
    assert!(worker.transport.is_none());
    let snapshot = worker.store.snapshot("w-stop-deadline", None).unwrap();
    assert_eq!(snapshot.phase, RuntimePhase::Interrupted);
}

fn test_worker(db: &Database, id: &str) -> ActorWorker {
    let (_tx, rx) = std::sync::mpsc::sync_channel(1);
    ActorWorker {
        conversation_id: id.into(),
        rx,
        store: store::RuntimeStore::new(db.clone()),
        repo: ChatRepo::new(db.clone()),
        run: Arc::new(RunService::new(AdapterRegistry::default())),
        catalogs: Arc::new(std::sync::Mutex::new(HashMap::new())),
        host_terminals: super::host_terminal::HostedTerminals::new(
            id.into(),
            Arc::new(std::sync::Mutex::new(HashMap::new())),
        ),
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
        allow_always_grants: Default::default(),
        pending_fs_writes: HashMap::new(),
        pending_grants: HashMap::new(),
        claude_dedup: Default::default(),
        thinking_open: false,
        codex_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        codex_live: None,
        codex_restart_pending: false,
        codex_idle_timeout: CODEX_IDLE_TIMEOUT,
    }
}

fn begin_running_turn(worker: &mut ActorWorker) {
    let now = "2026-01-01T00:00:00Z".to_string();
    let mut user = ChatMessage {
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
    let mut agent = ChatMessage {
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
    let turn = worker
        .store
        .begin_turn(
            &worker.conversation_id,
            &mut user,
            &mut agent,
            "run-1",
            Some("thread-1"),
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
    worker.run_id = Some("run-1".into());
    worker.thread_id = Some("thread-1".into());
    worker.turn_id = Some("run-1".into());
    worker
        .store
        .set_state(
            &worker.conversation_id,
            RuntimePhase::Running,
            Some("run-1"),
            Some("thread-1"),
            Some("run-1"),
            Some(turn),
            worker.message_id.as_deref(),
        )
        .unwrap();
}

/// A kept process that already has `thread-1` loaded.
fn attach_live(worker: &mut ActorWorker, fake: &FakeCodex) {
    let program = fake.program.clone();
    let cwd = worker.conversation_cwd().unwrap();
    worker.transport = Some(CodexTransport::spawn(&program, &cwd).unwrap());
    worker.codex_live = Some(CodexLive {
        program,
        cwd,
        generation: 0,
        thread_loaded: true,
        idle_since: None,
    });
}
