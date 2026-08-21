use super::*;
use crate::adapters::register_all;
use crate::models::RunStatus;
use crate::utils::process::RecordingProcessRunner;
use std::time::Duration;

fn opts() -> RunOptions {
    RunOptions {
        mode: RunMode::Parallel,
        timeout: Duration::from_secs(5),
        cwd: None,
        dry_run: false,
        skip_missing: true,
        allow_dangerous: false,
        max_output_bytes: 1024,
        process_mode: crate::models::ProcessMode::Text,
        native_session_id: None,
    }
}

#[test]
fn empty_prompt_rejected() {
    let svc = RunService::new(register_all());
    let err = svc.run(&[AgentId::Claude], "  ", &opts()).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn truncated_structured_result_keeps_captured_stdout() {
    let mut session = crate::utils::stream_parse::StreamSession::new(
        AgentId::Claude,
        crate::models::ProcessMode::Auto,
    );
    let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"partial"}]}}"#;
    let _ = session.feed(crate::models::OutputStream::Stdout, &format!("{line}\n"));

    let mut result = AgentRunResult {
        agent: AgentId::Claude,
        status: RunStatus::Ok,
        exit_code: Some(0),
        duration_ms: 0,
        stdout: "raw captured NDJSON".into(),
        stderr: String::new(),
        command: "claude".into(),
        error: None,
        truncated: true,
        native_session_id: None,
    };
    apply_structured_stdout(&mut result, &session);

    assert_eq!(result.stdout, "raw captured NDJSON");
    assert!(result.truncated);
}

#[test]
fn dry_run_does_not_call_runner() {
    let recorder = RecordingProcessRunner::new();
    let calls = Arc::clone(&recorder.calls);
    let svc = RunService::with_runner(register_all(), Arc::new(recorder));
    let mut o = opts();
    o.dry_run = true;
    // Use all agents; installed ones dry-run, missing skip — either way runner untouched.
    let report = svc.run(&AgentId::ALL, "hello dry", &o).expect("dry run");
    assert!(report.ok);
    assert!(calls.lock().unwrap().is_empty());
    assert!(report
        .results
        .iter()
        .all(|r| { matches!(r.status, RunStatus::DryRun | RunStatus::Skipped) }));
    // At least one dry_run if any agent is installed on this machine.
    // Structure: every result has agent id in ALL.
    assert_eq!(report.results.len(), AgentId::ALL.len());
}

#[test]
fn parallel_preserves_input_order() {
    let recorder = RecordingProcessRunner::new();
    let svc = RunService::with_runner(register_all(), Arc::new(recorder));
    let mut o = opts();
    o.dry_run = true;
    let order = [
        AgentId::Grok,
        AgentId::Claude,
        AgentId::Kimi,
        AgentId::Codex,
    ];
    let report = svc.run(&order, "order", &o).unwrap();
    let agents: Vec<_> = report.results.iter().map(|r| r.agent).collect();
    assert_eq!(agents, order.to_vec());
}

#[test]
fn build_run_spec_argv_snapshots() {
    let reg = register_all();
    let o = opts();
    let bin = std::path::Path::new("fake-bin");

    let claude = reg
        .get(AgentId::Claude)
        .unwrap()
        .build_run_spec(bin, "p", &o)
        .unwrap();
    assert_eq!(claude.args, vec!["-p", "p", "--output-format", "text"]);

    let codex = reg
        .get(AgentId::Codex)
        .unwrap()
        .build_run_spec(bin, "p", &o)
        .unwrap();
    assert_eq!(codex.args, vec!["exec", "--skip-git-repo-check", "p"]);

    let mut structured = o.clone();
    structured.process_mode = crate::models::ProcessMode::Auto;
    let claude_s = reg
        .get(AgentId::Claude)
        .unwrap()
        .build_run_spec(bin, "p", &structured)
        .unwrap();
    assert_eq!(
        claude_s.args,
        vec!["-p", "p", "--output-format", "stream-json", "--verbose"]
    );
    let codex_s = reg
        .get(AgentId::Codex)
        .unwrap()
        .build_run_spec(bin, "p", &structured)
        .unwrap();
    assert_eq!(
        codex_s.args,
        vec!["exec", "--skip-git-repo-check", "--json", "p"]
    );

    let kimi = reg
        .get(AgentId::Kimi)
        .unwrap()
        .build_run_spec(bin, "p", &o)
        .unwrap();
    assert_eq!(kimi.args, vec!["-p", "p", "--output-format", "text"]);

    let grok = reg
        .get(AgentId::Grok)
        .unwrap()
        .build_run_spec(bin, "p", &o)
        .unwrap();
    assert_eq!(grok.args, vec!["--no-auto-update", "-p", "p"]);

    match reg.get(AgentId::Pi).unwrap().build_run_spec(bin, "p", &o) {
        Ok(pi) => {
            assert_eq!(&pi.args[..5], ["-p", "p", "--mode", "text", "--no-session"]);
            for (k, v) in &pi.env {
                assert_eq!(k, "PATH");
                assert!(!v.is_empty());
            }
        }
        Err(err) => assert_eq!(err.code(), "env.not_ready"),
    }

    let kimi_s = reg
        .get(AgentId::Kimi)
        .unwrap()
        .build_run_spec(bin, "p", &structured)
        .unwrap();
    assert_eq!(
        kimi_s.args,
        vec!["-p", "p", "--output-format", "stream-json"]
    );
    match reg
        .get(AgentId::Pi)
        .unwrap()
        .build_run_spec(bin, "p", &structured)
    {
        Ok(pi_s) => {
            assert_eq!(
                &pi_s.args[..5],
                ["-p", "p", "--mode", "json", "--no-session"]
            );
        }
        Err(err) => assert_eq!(err.code(), "env.not_ready"),
    }
    let grok_s = reg
        .get(AgentId::Grok)
        .unwrap()
        .build_run_spec(bin, "p", &structured)
        .unwrap();
    assert_eq!(
        grok_s.args,
        vec![
            "--no-auto-update",
            "-p",
            "p",
            "--output-format",
            "streaming-json"
        ]
    );

    let mut dang = o.clone();
    dang.allow_dangerous = true;
    let claude_d = reg
        .get(AgentId::Claude)
        .unwrap()
        .build_run_spec(bin, "p", &dang)
        .unwrap();
    assert!(claude_d
        .args
        .iter()
        .any(|a| a == "--dangerously-skip-permissions"));

    let codex_d = reg
        .get(AgentId::Codex)
        .unwrap()
        .build_run_spec(bin, "p", &dang)
        .unwrap();
    assert_eq!(
        codex_d.args,
        vec![
            "exec",
            "--skip-git-repo-check",
            "--dangerously-bypass-approvals-and-sandbox",
            "p",
        ]
    );

    // kimi -p cannot take --auto/--yolo (CLI rejects the combination).
    let kimi_d = reg
        .get(AgentId::Kimi)
        .unwrap()
        .build_run_spec(bin, "p", &dang)
        .unwrap();
    assert!(!kimi_d.args.iter().any(|a| a == "--auto" || a == "--yolo"));

    let grok_d = reg
        .get(AgentId::Grok)
        .unwrap()
        .build_run_spec(bin, "p", &dang)
        .unwrap();
    assert_eq!(
        grok_d.args,
        vec!["--always-approve", "--no-auto-update", "-p", "p"]
    );
}

#[test]
fn mock_runner_timeout_marks_report_not_ok() {
    let recorder = RecordingProcessRunner::with_status(RunStatus::Timeout);
    let svc = RunService::with_runner(register_all(), Arc::new(recorder));
    let report = svc.run(&AgentId::ALL, "t", &opts()).unwrap();
    let ran = report
        .results
        .iter()
        .any(|r| r.status != RunStatus::Skipped);
    if ran {
        assert!(!report.ok);
        assert!(report.hard_failure_count() > 0);
    }
}

#[test]
fn sequential_dry_run_preserves_order() {
    let svc = RunService::with_runner(register_all(), Arc::new(RecordingProcessRunner::new()));
    let mut o = opts();
    o.mode = RunMode::Sequential;
    o.dry_run = true;
    let order = [AgentId::Codex, AgentId::Grok, AgentId::Claude];
    let report = svc.run(&order, "seq", &o).unwrap();
    let agents: Vec<_> = report.results.iter().map(|r| r.agent).collect();
    assert_eq!(agents, order.to_vec());
}

#[test]
fn parallel_mock_invokes_runner_for_installed() {
    let recorder = RecordingProcessRunner::new();
    let calls = Arc::clone(&recorder.calls);
    let svc = RunService::with_runner(register_all(), Arc::new(recorder));
    let report = svc.run(&AgentId::ALL, "parallel-mock", &opts()).unwrap();
    let installed = report
        .results
        .iter()
        .filter(|r| r.status != RunStatus::Skipped)
        .count();
    let n_calls = calls.lock().unwrap().len();
    assert_eq!(n_calls, installed);
    // Order of results matches AgentId::ALL even if runners finish out of order.
    let agents: Vec<_> = report.results.iter().map(|r| r.agent).collect();
    assert_eq!(agents, AgentId::ALL.to_vec());
}

#[test]
fn run_each_rejects_empty_jobs_and_empty_prompt() {
    let svc = RunService::with_runner(register_all(), Arc::new(RecordingProcessRunner::new()));
    let cancel = CancelToken::new();
    let err = svc.run_each(&[], &opts(), &cancel, &|_| {}).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");

    let jobs = vec![(AgentId::Claude, "  ".into())];
    let err = svc.run_each(&jobs, &opts(), &cancel, &|_| {}).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn run_each_emits_started_chunk_finished_events() {
    use std::sync::Mutex;
    let recorder = RecordingProcessRunner::new();
    let svc = RunService::with_runner(register_all(), Arc::new(recorder));
    let cancel = CancelToken::new();
    let events = Mutex::new(Vec::new());
    let jobs = vec![
        (AgentId::Claude, "prompt-a".into()),
        (AgentId::Codex, "prompt-b".into()),
    ];
    let report = svc
        .run_each(&jobs, &opts(), &cancel, &|ev| {
            events.lock().unwrap().push(ev);
        })
        .unwrap();
    assert_eq!(report.results.len(), 2);

    let events = events.into_inner().unwrap();
    let started = events
        .iter()
        .filter(|e| matches!(e, RunEvent::Started { .. }))
        .count();
    let finished = events
        .iter()
        .filter(|e| matches!(e, RunEvent::Finished { .. }))
        .count();
    // Installed agents emit Started+Finished; missing may early-finish without chunks.
    assert!(started >= 1, "expected Started events, got {events:?}");
    assert_eq!(started, finished);

    let has_chunk = events.iter().any(|e| matches!(e, RunEvent::Chunk { .. }));
    let ran = report
        .results
        .iter()
        .any(|r| r.status != RunStatus::Skipped);
    if ran {
        assert!(has_chunk, "expected Chunk for installed agent");
    }
}

#[test]
fn run_each_respects_pre_cancelled_token() {
    let mut recorder = RecordingProcessRunner::new();
    recorder.delay = Duration::from_millis(200);
    let svc = RunService::with_runner(register_all(), Arc::new(recorder));
    let cancel = CancelToken::new();
    cancel.cancel();
    let jobs = vec![(AgentId::Claude, "x".into())];
    let report = svc.run_each(&jobs, &opts(), &cancel, &|_| {}).unwrap();
    for r in &report.results {
        if r.status != RunStatus::Skipped {
            assert_eq!(r.status, RunStatus::Cancelled);
        }
    }
}
