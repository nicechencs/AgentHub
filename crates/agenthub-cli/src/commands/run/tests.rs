use super::*;
use agenthub_core::models::RunMode;
use agenthub_core::AgentHub;

#[test]
fn truncate_preserves_short_text_and_ellipsis() {
    assert_eq!(truncate("hello", 10), "hello");
    assert_eq!(truncate("hello world", 6).chars().count(), 6);
    assert!(truncate("hello world", 6).ends_with('…'));
}

#[test]
fn resolve_agents_rejects_invalid_id() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let err = resolve_agents(
        &RunArgs {
            prompt: "hi".into(),
            agents: Some("not-an-agent".into()),
            all: false,
            global_agent: None,
            mode: "parallel".into(),
            timeout_secs: 1,
            cwd: None,
            dry_run: true,
            allow_dangerous: false,
        },
        &hub,
    )
    .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn resolve_agents_all_uses_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let ids = resolve_agents(
        &RunArgs {
            prompt: "hi".into(),
            agents: None,
            all: true,
            global_agent: None,
            mode: "parallel".into(),
            timeout_secs: 1,
            cwd: None,
            dry_run: true,
            allow_dangerous: false,
        },
        &hub,
    )
    .unwrap();
    assert_eq!(ids.len(), AgentId::ALL.len());
}

#[test]
fn run_mode_parse_is_stable() {
    assert_eq!(RunMode::parse("parallel"), Some(RunMode::Parallel));
    assert_eq!(RunMode::parse("sequential"), Some(RunMode::Sequential));
    assert_eq!(RunMode::parse("nope"), None);
}
