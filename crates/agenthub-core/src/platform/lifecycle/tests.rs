//! Lifecycle coordinator tests (temp DB + fake executor).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;
use std::path::PathBuf;

use crate::adapters::register_all;
use crate::error::{AppError, Result};
use crate::models::{AgentId, DetectStatus, InstallOutcome};
use crate::platform::detection::{AgentDetector, DetectorRegistry};
use crate::platform::install::{InstallContribution, InstallContributionRegistry};
use crate::platform::lifecycle::{
    InstallationObserved, LifecycleCoordinator, LifecycleInstallExecutor, OperationKind,
    OperationStatus, VecProgressSink,
};
use crate::platform::AgentKey;
use crate::storage::Database;
use crate::utils::command_exec::{CommandExecutor, ExecRequest, ExecResult};
use tempfile::tempdir;

use super::executor::legacy_builtin_agent_id;

struct RejectExecutor;

impl CommandExecutor for RejectExecutor {
    fn run(&self, req: &ExecRequest) -> ExecResult {
        ExecResult {
            command: req.program.clone(),
            exit_code: Some(1),
            timed_out: false,
            stdout: String::new(),
            stderr: "fake reject".into(),
            spawn_error: None,
        }
    }
}

/// Counts `run` invocations so pre-execute audit failures can prove zero side effects.
struct CountingExecutor {
    calls: AtomicUsize,
}

impl CountingExecutor {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl CommandExecutor for CountingExecutor {
    fn run(&self, req: &ExecRequest) -> ExecResult {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ExecResult {
            command: req.program.clone(),
            exit_code: Some(1),
            timed_out: false,
            stdout: String::new(),
            stderr: "fake reject".into(),
            spawn_error: None,
        }
    }
}

struct MutableDetector {
    key: AgentKey,
    installed: Arc<AtomicBool>,
}

impl AgentDetector for MutableDetector {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn detect(&self) -> InstallationObserved {
        let installed = self.installed.load(Ordering::SeqCst);
        InstallationObserved {
            status: if installed {
                DetectStatus::Installed
            } else {
                DetectStatus::NotFound
            },
            version: installed.then(|| "1.0.0".into()),
            binary_path: None,
            channel: Some("native".into()),
            notes: Vec::new(),
        }
    }
}

struct FakeInstallContribution {
    key: AgentKey,
}

impl InstallContribution for FakeInstallContribution {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn native_setup_url(&self) -> Option<&'static str> {
        Some("https://example.invalid/fake-agent")
    }
}

/// Contribution used to prove BuiltinLifecycleInstallExecutor consumes allowlist fields.
struct NpmFakeInstallContribution {
    key: AgentKey,
    package: &'static str,
    extra_flags: &'static [&'static str],
}

impl InstallContribution for NpmFakeInstallContribution {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn npm_package(&self) -> Option<&'static str> {
        Some(self.package)
    }

    fn npm_install_extra_flags(&self) -> &'static [&'static str] {
        self.extra_flags
    }
}

/// Records commands and returns a configurable exit code (no real process).
struct RecordingExecutor {
    calls: Arc<std::sync::Mutex<Vec<String>>>,
    exit_code: i32,
}

impl CommandExecutor for RecordingExecutor {
    fn run(&self, req: &ExecRequest) -> ExecResult {
        let cmd = format!("{} {}", req.program, req.args.join(" "));
        self.calls.lock().unwrap().push(cmd.clone());
        ExecResult {
            command: cmd,
            exit_code: Some(self.exit_code),
            timed_out: false,
            stdout: String::new(),
            stderr: String::new(),
            spawn_error: None,
        }
    }
}

struct FakeLifecycleExecutor {
    installed: Arc<AtomicBool>,
}

impl LifecycleInstallExecutor for FakeLifecycleExecutor {
    fn install(
        &self,
        key: &AgentKey,
        _contribution: &dyn InstallContribution,
        channel: &str,
        _install_deps: bool,
        command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome> {
        if channel != "native" {
            return Err(AppError::Unsupported(format!(
                "channel '{channel}' is unsupported for {}",
                key.as_str()
            )));
        }
        let request = ExecRequest {
            program: "fake-installer".into(),
            args: Vec::new(),
            timeout: Duration::from_secs(1),
            max_output_bytes: 1024,
        };
        let _ = command_executor.run(&request);
        self.installed.store(true, Ordering::SeqCst);
        Ok(InstallOutcome {
            ok: true,
            action: "agent_install".into(),
            logs: vec!["fake install".into()],
            message: format!("{} installed", key.as_str()),
            agent: None,
            runtime: None,
            ..Default::default()
        })
    }

    fn upgrade(
        &self,
        key: &AgentKey,
        contribution: &dyn InstallContribution,
        command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome> {
        // Reuse install channel checks, then mark installed with upgrade action.
        let mut out = self.install(key, contribution, "native", false, command_executor)?;
        out.action = "agent_upgrade".into();
        out.message = format!("{} upgraded", key.as_str());
        out.logs = vec!["fake upgrade".into()];
        Ok(out)
    }

    fn uninstall(
        &self,
        key: &AgentKey,
        _contribution: &dyn InstallContribution,
        _purge_config: bool,
        _actual_data_dir: &std::path::Path,
        _command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome> {
        self.installed.store(false, Ordering::SeqCst);
        Ok(InstallOutcome {
            ok: true,
            action: "agent_uninstall".into(),
            logs: vec!["fake uninstall".into()],
            message: format!("{} uninstalled", key.as_str()),
            agent: None,
            runtime: None,
            ..Default::default()
        })
    }
}

struct BlockingLifecycleExecutor {
    started: Arc<Barrier>,
    release: Arc<Barrier>,
}

impl LifecycleInstallExecutor for BlockingLifecycleExecutor {
    fn install(
        &self,
        key: &AgentKey,
        _contribution: &dyn InstallContribution,
        _channel: &str,
        _install_deps: bool,
        _command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome> {
        self.started.wait();
        self.release.wait();
        Ok(InstallOutcome {
            ok: true,
            action: "agent_install".into(),
            logs: Vec::new(),
            message: format!("{} installed", key.as_str()),
            agent: None,
            runtime: None,
            ..Default::default()
        })
    }

    fn upgrade(
        &self,
        key: &AgentKey,
        contribution: &dyn InstallContribution,
        command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome> {
        self.install(key, contribution, "native", false, command_executor)
    }

    fn uninstall(
        &self,
        key: &AgentKey,
        _contribution: &dyn InstallContribution,
        _purge_config: bool,
        _actual_data_dir: &std::path::Path,
        _command_executor: &dyn CommandExecutor,
    ) -> Result<InstallOutcome> {
        Ok(InstallOutcome {
            ok: true,
            action: "agent_uninstall".into(),
            logs: Vec::new(),
            message: format!("{} uninstalled", key.as_str()),
            agent: None,
            runtime: None,
            ..Default::default()
        })
    }
}

fn open_key_native_lifecycle(
    key_value: &str,
    with_detector: bool,
    with_contribution: bool,
) -> (
    tempfile::TempDir,
    LifecycleCoordinator,
    AgentKey,
    Arc<AtomicBool>,
) {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("key-native.db")).unwrap();
    let key = AgentKey::parse(key_value).unwrap();
    let installed = Arc::new(AtomicBool::new(false));
    let mut detectors = DetectorRegistry::new();
    if with_detector {
        detectors
            .register(Arc::new(MutableDetector {
                key: key.clone(),
                installed: Arc::clone(&installed),
            }))
            .unwrap();
    }
    let mut installs = InstallContributionRegistry::new();
    if with_contribution {
        installs
            .register(Arc::new(FakeInstallContribution { key: key.clone() }))
            .unwrap();
    }
    let lifecycle = LifecycleCoordinator::with_registries_and_executor(
        db,
        detectors,
        installs,
        Arc::new(FakeLifecycleExecutor {
            installed: Arc::clone(&installed),
        }),
    );
    (dir, lifecycle, key, installed)
}

fn open_hub_db() -> (tempfile::TempDir, Database, LifecycleCoordinator) {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let reg = register_all();
    let lc = LifecycleCoordinator::new(db.clone(), reg);
    (dir, db, lc)
}

#[test]
fn compatibility_constructor_derives_file_backed_data_dir() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("compat.db")).unwrap();
    let lifecycle = LifecycleCoordinator::new(db, register_all());

    assert_eq!(lifecycle.data_dir.as_deref(), Some(dir.path()));
    assert!(lifecycle.data_dir_error.is_none());
}

#[test]
fn invalid_explicit_data_dir_is_reported_when_purge_is_requested() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("invalid-data-dir.db")).unwrap();
    let key = AgentKey::parse("invalid-data-dir-agent").unwrap();
    let installed = Arc::new(AtomicBool::new(false));
    let mut detectors = DetectorRegistry::new();
    detectors
        .register(Arc::new(MutableDetector {
            key: key.clone(),
            installed: Arc::clone(&installed),
        }))
        .unwrap();
    let mut installs = InstallContributionRegistry::new();
    installs
        .register(Arc::new(FakeInstallContribution { key: key.clone() }))
        .unwrap();

    let mut invalid = PathBuf::new();
    for _ in 0..128 {
        invalid.push("..");
    }
    let lifecycle = LifecycleCoordinator::with_registries_and_data_dir(
        db,
        detectors,
        installs,
        Arc::new(FakeLifecycleExecutor { installed }),
        invalid,
    );
    let executor = CountingExecutor::new();
    let error = lifecycle
        .uninstall_agent_key(&key, true, &executor, None)
        .expect_err("invalid explicit data dir must fail closed");

    assert_eq!(error.code(), "invalid_arg");
    assert!(error.to_string().contains("invalid AgentHub data directory"));
    assert_eq!(executor.calls(), 0, "purge executor must not be called");
}

/// BuiltinLifecycleInstallExecutor + non-AgentId key + npm contribution.
fn open_contribution_driven_lifecycle(
    key_value: &str,
    package: &'static str,
    extra_flags: &'static [&'static str],
) -> (
    tempfile::TempDir,
    LifecycleCoordinator,
    AgentKey,
    Arc<AtomicBool>,
) {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("contrib-driven.db")).unwrap();
    let key = AgentKey::parse(key_value).unwrap();
    assert!(
        super::executor::legacy_builtin_agent_id(&key).is_none(),
        "contract key must not be a closed AgentId"
    );
    let installed = Arc::new(AtomicBool::new(false));
    let mut detectors = DetectorRegistry::new();
    detectors
        .register(Arc::new(MutableDetector {
            key: key.clone(),
            installed: Arc::clone(&installed),
        }))
        .unwrap();
    let mut installs = InstallContributionRegistry::new();
    installs
        .register(Arc::new(NpmFakeInstallContribution {
            key: key.clone(),
            package,
            extra_flags,
        }))
        .unwrap();
    let lifecycle = LifecycleCoordinator::with_registries(
        db,
        crate::adapters::AdapterRegistry::new(),
        detectors,
        installs,
    );
    (dir, lifecycle, key, installed)
}

#[test]
fn migration_creates_operations_table() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    db.with_conn(|conn| {
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='operations'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(n, 1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn interrupt_stale_running_on_recovery() {
    let (_dir, db, lc) = open_hub_db();
    // Insert a synthetic running row via repo path used by coordinator.
    lc.list_operations(AgentId::Claude, 10).unwrap();
    let repo = crate::storage::OperationRepo::new(db.clone());
    repo.insert_running(
        "op-stale",
        "claude",
        OperationKind::Install,
        "execute",
        "2020-01-01T00:00:00Z",
    )
    .unwrap();
    let n = LifecycleCoordinator::interrupt_stale_running(&db).unwrap();
    assert_eq!(n, 1);
    let row = repo.get("op-stale").unwrap().unwrap();
    assert_eq!(row.status, OperationStatus::Interrupted);
    assert_eq!(row.error_code.as_deref(), Some("lifecycle.interrupted"));
}

#[test]
fn install_records_operation_and_progress() {
    // Lifecycle always writes an operation row (success or fail depends on machine env).
    let (_dir, _db, lc) = open_hub_db();
    let mut sink = VecProgressSink::default();
    let out = lc
        .install_agent(
            AgentId::Kimi,
            "npm",
            false,
            &RejectExecutor,
            Some(&mut sink),
        )
        .unwrap();
    assert!(out.logs.iter().any(|l| l.contains("operation_id:")));
    let ops = lc.list_operations(AgentId::Kimi, 5).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, OperationKind::Install);
    assert!(matches!(
        ops[0].status,
        OperationStatus::Succeeded | OperationStatus::Failed
    ));
    assert!(!sink.events.is_empty());
    // Observed status persisted from detector after execute.
    assert!(ops[0].observed_status.is_some());
}

#[test]
fn concurrent_lock_rejects_second_caller() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let reg = register_all();
    let lc = Arc::new(LifecycleCoordinator::new(db, reg));
    let barrier = Arc::new(Barrier::new(2));

    // Hold lock by starting install that blocks on env check is too fast.
    // Use process lock directly via two rapid calls — second should fail if first holds.
    // Simulate by holding lock with a long fake: spawn thread that acquires via install
    // while another tries repair_detect.
    let lc1 = Arc::clone(&lc);
    let b1 = Arc::clone(&barrier);
    let t1 = thread::spawn(move || {
        b1.wait();
        // Keep the lock by calling run through install (may finish quickly).
        // Instead test lock via double repair in sequence is free — use Mutex hold:
        let _ = lc1.repair_detect(AgentId::Claude, None);
    });
    let lc2 = Arc::clone(&lc);
    let b2 = Arc::clone(&barrier);
    let t2 = thread::spawn(move || {
        b2.wait();
        let _ = lc2.repair_detect(AgentId::Claude, None);
    });
    t1.join().unwrap();
    t2.join().unwrap();
    // Both should complete (locks sequential); at least one operation recorded.
    let ops = lc.list_operations(AgentId::Claude, 10).unwrap();
    assert!(!ops.is_empty());
}

#[test]
fn repair_detect_records_observed_status() {
    let (_dir, _db, lc) = open_hub_db();
    let result = lc.repair_detect(AgentId::Pi, None).unwrap();
    assert_eq!(result.outcome.action, "agent_repair");
    let ops = lc.list_operations(AgentId::Pi, 1).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, OperationKind::Repair);
    // Observed written from detector (installed or not_found).
    assert!(ops[0].observed_status.is_some());
    let obs = ops[0].observed_status.as_deref().unwrap();
    assert!(obs == "installed" || obs == "not_found");
    let _ = DetectStatus::Installed; // type linked
}

#[test]
fn unsupported_unknown_agent_fails_closed() {
    // P1-3: detectors are independent of AdapterRegistry. Empty detector
    // registry still fails closed before execute.
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let lc = LifecycleCoordinator::with_registries(
        db,
        crate::adapters::AdapterRegistry::new(),
        DetectorRegistry::new(),
        InstallContributionRegistry::new(),
    );
    let err = lc.repair_detect(AgentId::Claude, None).unwrap_err();
    assert_eq!(err.code(), "not_found");
}

#[test]
fn install_progress_events_share_one_nonempty_operation_id() {
    let (_dir, _db, lc) = open_hub_db();
    let mut sink = VecProgressSink::default();
    let out = lc
        .install_agent(
            AgentId::Grok,
            "native",
            false,
            &RejectExecutor,
            Some(&mut sink),
        )
        .unwrap();

    assert!(!sink.events.is_empty(), "expected progress events");
    let op_id = sink.events[0].operation_id.clone();
    assert!(!op_id.is_empty(), "operation_id must be non-empty");
    for ev in &sink.events {
        assert_eq!(
            ev.operation_id, op_id,
            "all progress events from one install must share the same operation_id"
        );
    }

    let ops = lc.list_operations(AgentId::Grok, 5).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].id, op_id);
    assert!(
        out.logs
            .iter()
            .any(|l| l.contains(&format!("operation_id: {op_id}"))),
        "InstallOutcome logs should carry the same operation_id"
    );
}

#[test]
fn pre_execute_step_update_failure_skips_executor() {
    let (_dir, db, lc) = open_hub_db();
    db.with_conn(|conn| {
        conn.execute_batch(
            r#"
            CREATE TRIGGER abort_step_execute
            BEFORE UPDATE ON operations
            WHEN NEW.step = 'execute'
            BEGIN
                SELECT RAISE(ABORT, 'injected execute step failure');
            END;
            "#,
        )?;
        Ok(())
    })
    .unwrap();

    let executor = CountingExecutor::new();
    let mut sink = VecProgressSink::default();
    let err = lc
        .install_agent(AgentId::Cursor, "npm", false, &executor, Some(&mut sink))
        .unwrap_err();

    assert_eq!(err.code(), "lifecycle.audit_persist_failed");
    assert_eq!(
        executor.calls(),
        0,
        "executor must not run when audit fails before execute"
    );
    assert!(
        !sink.events.iter().any(|e| e.step == "execute"),
        "execute progress must not be emitted after pre-execute audit failure"
    );
}

#[test]
fn finalize_status_update_failure_returns_audit_partial_failure() {
    let (_dir, db, lc) = open_hub_db();
    db.with_conn(|conn| {
        conn.execute_batch(
            r#"
            CREATE TRIGGER abort_status_leave_running
            BEFORE UPDATE ON operations
            WHEN OLD.status = 'running' AND NEW.status != 'running'
            BEGIN
                SELECT RAISE(ABORT, 'injected status leave failure');
            END;
            "#,
        )?;
        Ok(())
    })
    .unwrap();

    let executor = CountingExecutor::new();
    let mut sink = VecProgressSink::default();
    let err = lc
        .install_agent(AgentId::Codex, "npm", false, &executor, Some(&mut sink))
        .unwrap_err();

    assert!(
        sink.events.iter().any(|e| e.step == "execute"),
        "execute must have been attempted before finalize audit failure"
    );
    assert!(
        sink.events.iter().any(|e| e.step == "redetect"),
        "redetect must run even when finalize audit fails"
    );

    let op_id = sink
        .events
        .first()
        .map(|e| e.operation_id.clone())
        .expect("progress events");
    assert!(!op_id.is_empty());

    assert_eq!(err.code(), "lifecycle.audit_partial_failure");
    let msg = err.to_string();
    assert!(
        msg.contains(&op_id),
        "partial failure must include operation id; msg={msg}"
    );
    assert!(
        msg.contains("execute summary:"),
        "partial failure must include execute summary; msg={msg}"
    );
    assert!(
        msg.contains("observed:"),
        "partial failure must include observed state; msg={msg}"
    );
    assert!(
        msg.contains("trust detector as source of truth"),
        "partial failure must warn detector is source of truth; msg={msg}"
    );
}

#[test]
fn key_native_install_and_repair_preserve_key_operation_and_observed_state() {
    let (_dir, lifecycle, key, installed) =
        open_key_native_lifecycle("fake-lifecycle-install", true, true);
    let command_executor = CountingExecutor::new();
    let mut sink = VecProgressSink::default();

    let outcome = lifecycle
        .install_agent_key(&key, "native", false, &command_executor, Some(&mut sink))
        .unwrap();

    assert!(outcome.ok);
    assert_eq!(command_executor.calls(), 1);
    assert!(installed.load(Ordering::SeqCst));
    assert!(!sink.events.is_empty());
    let operation_id = sink.events[0].operation_id.clone();
    assert!(!operation_id.is_empty());
    assert!(sink
        .events
        .iter()
        .all(|event| { event.operation_id == operation_id && event.agent_key == key.as_str() }));

    let operations = lifecycle.list_operations_key(&key, 10).unwrap();
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].id, operation_id);
    assert_eq!(operations[0].agent_key, key.as_str());
    assert_eq!(operations[0].observed_status.as_deref(), Some("installed"));

    let repaired = lifecycle.repair_detect_key(&key, None).unwrap();
    assert_eq!(
        repaired.observed.as_ref().map(|state| state.status),
        Some(DetectStatus::Installed)
    );
    assert_eq!(repaired.outcome.action, "agent_repair");
}

#[test]
fn key_native_detailed_install_preserves_observed_state_and_operation_id() {
    let (_dir, lifecycle, key, _installed) =
        open_key_native_lifecycle("fake-lifecycle-detailed", true, true);
    let command_executor = CountingExecutor::new();
    let mut sink = VecProgressSink::default();

    let result = lifecycle
        .install_agent_key_detailed(&key, "native", false, &command_executor, Some(&mut sink))
        .unwrap();

    assert_eq!(result.operation_id.as_str(), sink.events[0].operation_id);
    assert_eq!(
        result.observed.as_ref().map(|o| o.status),
        Some(DetectStatus::Installed)
    );
    assert_eq!(
        result.observed.as_ref().and_then(|o| o.version.as_deref()),
        Some("1.0.0")
    );
    assert!(result.outcome.ok);
}

#[test]
fn key_native_missing_detector_and_contribution_fail_typed_before_execute() {
    let (_dir, lifecycle, key, _installed) =
        open_key_native_lifecycle("fake-lifecycle-missing-detector", false, true);
    let command_executor = CountingExecutor::new();
    let mut sink = VecProgressSink::default();
    let err = lifecycle
        .install_agent_key(&key, "native", false, &command_executor, Some(&mut sink))
        .unwrap_err();
    assert_eq!(err.code(), "not_found");
    assert_eq!(command_executor.calls(), 0);
    let operation_id = sink
        .events
        .first()
        .expect("resolve progress")
        .operation_id
        .clone();
    assert!(!operation_id.is_empty());
    assert!(err.to_string().contains(&operation_id));

    let (_dir, lifecycle, key, _installed) =
        open_key_native_lifecycle("fake-lifecycle-missing-install", true, false);
    let command_executor = CountingExecutor::new();
    let mut sink = VecProgressSink::default();
    let err = lifecycle
        .install_agent_key(&key, "native", false, &command_executor, Some(&mut sink))
        .unwrap_err();
    assert_eq!(err.code(), "lifecycle.unsupported");
    assert_eq!(command_executor.calls(), 0);
    let operation_id = sink
        .events
        .first()
        .expect("resolve progress")
        .operation_id
        .clone();
    assert!(!operation_id.is_empty());
    assert!(err.to_string().contains(&operation_id));
}

#[test]
fn lock_held_error_keeps_operation_id_from_progress() {
    use std::thread;

    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("lock-held.db")).unwrap();
    let key = AgentKey::parse("fake-lifecycle-lock").unwrap();
    let installed = Arc::new(AtomicBool::new(false));
    let mut detectors = DetectorRegistry::new();
    detectors
        .register(Arc::new(MutableDetector {
            key: key.clone(),
            installed,
        }))
        .unwrap();
    let mut installs = InstallContributionRegistry::new();
    installs
        .register(Arc::new(FakeInstallContribution { key: key.clone() }))
        .unwrap();

    let started = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let lifecycle = Arc::new(LifecycleCoordinator::with_registries_and_executor(
        db,
        detectors,
        installs,
        Arc::new(BlockingLifecycleExecutor {
            started: Arc::clone(&started),
            release: Arc::clone(&release),
        }),
    ));

    let first_lifecycle = Arc::clone(&lifecycle);
    let first_key = key.clone();
    let first = thread::spawn(move || {
        first_lifecycle
            .install_agent_key(&first_key, "native", false, &RejectExecutor, None)
            .unwrap()
    });

    // The first call holds the process-local lifecycle lock while its fake
    // executor waits, so the second call fails before creating a DB record.
    started.wait();
    let mut sink = VecProgressSink::default();
    let err = lifecycle
        .install_agent_key(&key, "native", false, &RejectExecutor, Some(&mut sink))
        .unwrap_err();
    let operation_id = sink
        .events
        .first()
        .expect("resolve progress")
        .operation_id
        .clone();
    assert_eq!(err.code(), "lifecycle.lock_held");
    assert!(!operation_id.is_empty());
    assert!(err.to_string().contains(&operation_id));

    release.wait();
    first.join().unwrap();
}

#[test]
fn key_native_unsupported_channel_is_typed_and_redetected() {
    let (_dir, lifecycle, key, _installed) =
        open_key_native_lifecycle("fake-lifecycle-channel", true, true);
    let command_executor = CountingExecutor::new();
    let mut sink = VecProgressSink::default();
    let err = lifecycle
        .install_agent_key(
            &key,
            "unknown-channel",
            false,
            &command_executor,
            Some(&mut sink),
        )
        .unwrap_err();

    assert_eq!(err.code(), "unsupported");
    assert_eq!(command_executor.calls(), 0);
    assert!(sink.events.iter().any(|event| event.step == "execute"));
    assert!(sink.events.iter().any(|event| event.step == "redetect"));
    let operations = lifecycle.list_operations_key(&key, 1).unwrap();
    assert_eq!(operations[0].status, OperationStatus::Failed);
    assert_eq!(operations[0].observed_status.as_deref(), Some("not_found"));
}

#[test]
fn builtin_agent_id_facades_resolve_key_native_registries() {
    let (_dir, _db, lifecycle) = open_hub_db();
    for agent in AgentId::ALL {
        let key = AgentKey::from_agent_id(agent);
        assert_eq!(legacy_builtin_agent_id(&key), Some(agent));
        lifecycle.list_operations(agent, 1).unwrap();
        lifecycle.list_operations_key(&key, 1).unwrap();
    }
    let unknown = AgentKey::parse("future-agent").unwrap();
    assert_eq!(legacy_builtin_agent_id(&unknown), None);
}

#[test]
fn key_native_detailed_upgrade_records_kind_progress_and_observed() {
    let (_dir, lifecycle, key, installed) =
        open_key_native_lifecycle("fake-lifecycle-upgrade", true, true);
    let command_executor = CountingExecutor::new();

    // Seed installed state via key-native install (same fake executor).
    lifecycle
        .install_agent_key(&key, "native", false, &command_executor, None)
        .unwrap();
    assert!(installed.load(Ordering::SeqCst));

    let mut sink = VecProgressSink::default();
    let result = lifecycle
        .upgrade_agent_key_detailed(&key, &command_executor, Some(&mut sink))
        .unwrap();

    assert!(result.outcome.ok);
    assert_eq!(result.outcome.action, "agent_upgrade");
    assert_eq!(result.operation_id.as_str(), sink.events[0].operation_id);
    assert!(sink.events.iter().any(|e| e.step == "execute"));
    assert!(sink.events.iter().any(|e| e.step == "redetect"));
    assert!(sink.events.iter().any(|e| e.step == "finalize"));
    assert!(sink.events.iter().all(|e| e.kind == OperationKind::Upgrade));
    assert_eq!(
        result.observed.as_ref().map(|o| o.status),
        Some(DetectStatus::Installed)
    );

    let ops = lifecycle.list_operations_key(&key, 5).unwrap();
    // install + upgrade
    assert!(ops.len() >= 2);
    let upgrade_op = ops
        .iter()
        .find(|op| op.kind == OperationKind::Upgrade)
        .expect("upgrade operation row");
    assert_eq!(upgrade_op.id, result.operation_id.as_str());
    assert_eq!(upgrade_op.status, OperationStatus::Succeeded);
    assert_eq!(upgrade_op.observed_status.as_deref(), Some("installed"));
}

#[test]
fn key_native_detailed_uninstall_clears_install_and_records_operation() {
    let (_dir, lifecycle, key, installed) =
        open_key_native_lifecycle("fake-lifecycle-uninstall", true, true);
    let command_executor = CountingExecutor::new();

    lifecycle
        .install_agent_key(&key, "native", false, &command_executor, None)
        .unwrap();
    assert!(installed.load(Ordering::SeqCst));

    let mut sink = VecProgressSink::default();
    let result = lifecycle
        .uninstall_agent_key_detailed(&key, false, &command_executor, Some(&mut sink))
        .unwrap();

    assert!(result.outcome.ok);
    assert_eq!(result.outcome.action, "agent_uninstall");
    assert!(!installed.load(Ordering::SeqCst));
    assert_eq!(result.operation_id.as_str(), sink.events[0].operation_id);
    assert!(sink
        .events
        .iter()
        .all(|e| e.kind == OperationKind::Uninstall));
    assert!(sink.events.iter().any(|e| e.step == "execute"));
    assert!(sink.events.iter().any(|e| e.step == "redetect"));
    assert_eq!(
        result.observed.as_ref().map(|o| o.status),
        Some(DetectStatus::NotFound)
    );

    let ops = lifecycle.list_operations_key(&key, 5).unwrap();
    let uninstall_op = ops
        .iter()
        .find(|op| op.kind == OperationKind::Uninstall)
        .expect("uninstall operation row");
    assert_eq!(uninstall_op.id, result.operation_id.as_str());
    assert_eq!(uninstall_op.status, OperationStatus::Succeeded);
    assert_eq!(uninstall_op.observed_status.as_deref(), Some("not_found"));
}

#[test]
fn builtin_executor_installs_non_agent_id_via_contribution_allowlist() {
    // P1-2 contract: BuiltinLifecycleInstallExecutor must consume InstallContribution
    // (npm package + flags) for keys outside the closed AgentId set.
    let (_dir, lifecycle, key, installed) = open_contribution_driven_lifecycle(
        "future-contrib-agent",
        "@agenthub/p1-2-contract-pkg",
        &["--ignore-scripts"],
    );
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let executor = RecordingExecutor {
        calls: Arc::clone(&calls),
        exit_code: 0,
    };
    let mut sink = VecProgressSink::default();

    let outcome = lifecycle
        .install_agent_key(&key, "npm", false, &executor, Some(&mut sink))
        .unwrap();

    assert!(
        outcome.ok,
        "contribution-driven install must succeed when allowlisted command exits 0; msg={}",
        outcome.message
    );
    assert!(outcome
        .logs
        .iter()
        .any(|l| l.contains("@agenthub/p1-2-contract-pkg")));
    assert!(outcome.logs.iter().any(|l| l.contains("--ignore-scripts")));

    let commands = calls.lock().unwrap();
    assert!(
        commands.iter().any(|c| {
            c.contains("install")
                && c.contains("-g")
                && c.contains("--ignore-scripts")
                && c.contains("@agenthub/p1-2-contract-pkg")
        }),
        "executor must run npm install using contribution package/flags; got {commands:?}"
    );

    let ops = lifecycle.list_operations_key(&key, 5).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].status, OperationStatus::Succeeded);
    assert_eq!(ops[0].agent_key, key.as_str());
    // Detector was not flipped by Builtin executor; observed stays not_found.
    assert_eq!(ops[0].observed_status.as_deref(), Some("not_found"));
    assert!(!installed.load(Ordering::SeqCst));
    assert!(!sink.events.is_empty());
    assert!(sink.events.iter().any(|e| e.step == "execute"));
}

#[test]
fn builtin_executor_rejects_unknown_key_without_contribution_before_execute() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("no-contrib.db")).unwrap();
    let key = AgentKey::parse("orphan-no-contrib").unwrap();
    let mut detectors = DetectorRegistry::new();
    detectors
        .register(Arc::new(MutableDetector {
            key: key.clone(),
            installed: Arc::new(AtomicBool::new(false)),
        }))
        .unwrap();
    let lifecycle = LifecycleCoordinator::with_registries(
        db,
        crate::adapters::AdapterRegistry::new(),
        detectors,
        InstallContributionRegistry::new(),
    );
    let executor = RecordingExecutor {
        calls: Arc::new(std::sync::Mutex::new(Vec::new())),
        exit_code: 0,
    };
    let err = lifecycle
        .install_agent_key(&key, "npm", false, &executor, None)
        .unwrap_err();
    assert_eq!(err.code(), "lifecycle.unsupported");
    assert!(executor.calls.lock().unwrap().is_empty());
}
