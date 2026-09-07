use super::*;

#[test]
fn parse_agent_list_ok() {
    let ids = parse_agent_list("claude, codex,claude").unwrap();
    assert_eq!(ids, vec![AgentId::Claude, AgentId::Codex]);
}

#[test]
fn parse_agent_list_rejects_invalid() {
    assert!(parse_agent_list("claude,foo").is_err());
    assert!(parse_agent_list("").is_err());
    assert!(parse_agent_list("  ,  ").is_err());
}

#[test]
fn run_mode_parse() {
    assert_eq!(RunMode::parse("parallel"), Some(RunMode::Parallel));
    assert_eq!(RunMode::parse("SEQ"), Some(RunMode::Sequential));
    assert_eq!(RunMode::parse("nope"), None);
}

#[test]
fn run_spec_display_quotes_spaces() {
    let spec = RunSpec {
        agent: AgentId::Claude,
        program: PathBuf::from(r"C:\Program Files\claude.exe"),
        args: vec!["-p".into(), "hello world".into()],
        cwd: None,
        env: vec![],
    };
    let s = spec.display_command();
    assert!(s.contains('\"'));
    assert!(s.contains("<prompt>"));
    assert!(!s.contains("hello world"));
}

#[test]
fn run_spec_hides_codex_positional_prompt() {
    let spec = RunSpec {
        agent: AgentId::Codex,
        program: PathBuf::from("codex"),
        args: vec![
            "exec".into(),
            "--skip-git-repo-check".into(),
            "--json".into(),
            "只回一个字：好".into(),
        ],
        cwd: None,
        env: vec![],
    };
    let s = spec.display_command();
    assert!(s.contains("<prompt>"));
    assert!(!s.contains("只回一个字"));
}

#[test]
fn multi_run_report_ok_ignores_skipped() {
    let report = MultiRunReport::from_results(
        "p".into(),
        RunMode::Parallel,
        vec![
            AgentRunResult::skipped(AgentId::Claude, "missing"),
            AgentRunResult {
                agent: AgentId::Codex,
                status: RunStatus::Ok,
                exit_code: Some(0),
                duration_ms: 1,
                stdout: "ok".into(),
                stderr: String::new(),
                command: "codex".into(),
                error: None,
                truncated: false,
                native_session_id: None,
            },
        ],
        "t0".into(),
        "t1".into(),
    );
    assert!(report.ok);
    assert_eq!(report.success_count(), 1);
}

#[test]
fn multi_run_report_failed_sets_ok_false() {
    let report = MultiRunReport::from_results(
        "p".into(),
        RunMode::Sequential,
        vec![AgentRunResult {
            agent: AgentId::Grok,
            status: RunStatus::Timeout,
            exit_code: None,
            duration_ms: 100,
            stdout: String::new(),
            stderr: String::new(),
            command: "grok".into(),
            error: Some("timeout".into()),
            truncated: false,
            native_session_id: None,
        }],
        "t0".into(),
        "t1".into(),
    );
    assert!(!report.ok);
    assert_eq!(report.hard_failure_count(), 1);
}
