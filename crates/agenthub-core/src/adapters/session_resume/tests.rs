use super::{plan_native_resume, NativeResumePlan};
use crate::models::AgentId;

fn expect_argv(agent: AgentId, session_id: &str, argv: &[&str]) {
    assert_eq!(
        plan_native_resume(agent, session_id),
        Some(NativeResumePlan {
            agent,
            argv: argv.iter().map(|s| (*s).to_string()).collect(),
        })
    );
}

#[test]
fn plans_herdr_compatible_resume_argv() {
    expect_argv(AgentId::Claude, "claude-session", &["claude", "--resume", "claude-session"]);
    expect_argv(AgentId::Codex, "codex-session", &["codex", "resume", "codex-session"]);
    expect_argv(AgentId::Kimi, "kimi-session", &["kimi", "--session", "kimi-session"]);
    expect_argv(AgentId::Grok, "grok-session", &["grok", "--resume", "grok-session"]);
    expect_argv(AgentId::Pi, "pi-session", &["pi", "--session", "pi-session"]);
}

#[test]
fn cursor_uses_platform_cli_name() {
    let plan = plan_native_resume(AgentId::Cursor, "cursor-session").unwrap();
    let expected = if cfg!(windows) {
        "cursor-agent.cmd"
    } else {
        "cursor-agent"
    };
    assert_eq!(
        plan.argv,
        vec![expected, "--resume", "cursor-session"]
    );
}

#[test]
fn rejects_empty_control_and_oversized_ids() {
    assert!(plan_native_resume(AgentId::Claude, "  ").is_none());
    assert!(plan_native_resume(AgentId::Claude, "bad\nid").is_none());
    assert!(plan_native_resume(AgentId::Claude, &"x".repeat(513)).is_none());
}

#[test]
fn trims_whitespace_around_valid_ids() {
    expect_argv(AgentId::Claude, "  abc  ", &["claude", "--resume", "abc"]);
}

#[test]
fn unknown_resume_agents_return_none() {
    assert!(plan_native_resume(AgentId::WorkBuddy, "wb").is_none());
    assert!(plan_native_resume(AgentId::Dsh, "dsh-session").is_none());
}

#[test]
fn print_resume_is_only_claude_and_codex() {
    assert!(super::supports_print_resume(AgentId::Claude));
    assert!(super::supports_print_resume(AgentId::Codex));
    assert!(!super::supports_print_resume(AgentId::Kimi));
    assert!(!super::supports_print_resume(AgentId::Grok));
}
