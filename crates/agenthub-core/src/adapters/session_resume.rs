//! Official CLI argv for resuming a native agent session.
//!
//! Interactive flags match herdr's `agent_resume.rs` (`claude --resume`,
//! `codex resume`, …). Chat subsequent turns use print-mode resume
//! (`claude -p --resume`, `codex exec resume`) via [`supports_print_resume`].

use crate::models::AgentId;

const MAX_SESSION_ID_LEN: usize = 512;

/// Planned native resume invocation (`program` is `argv[0]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeResumePlan {
    pub agent: AgentId,
    pub argv: Vec<String>,
}

/// Build the official **interactive TUI** resume argv for a listed native session id.
///
/// Returns `None` when the id is unusable or the agent has no known resume CLI.
pub fn plan_native_resume(agent: AgentId, session_id: &str) -> Option<NativeResumePlan> {
    let session_id = valid_session_id(session_id)?;
    let argv = resume_argv(agent, session_id)?;
    Some(NativeResumePlan { agent, argv })
}

/// Whether Chat can continue this agent with print-mode resume (not TUI).
pub fn supports_print_resume(agent: AgentId) -> bool {
    matches!(agent, AgentId::Claude | AgentId::Codex)
}

fn resume_argv(agent: AgentId, session_id: &str) -> Option<Vec<String>> {
    let argv = match agent {
        AgentId::Claude => vec!["claude".into(), "--resume".into(), session_id.into()],
        AgentId::Codex => vec!["codex".into(), "resume".into(), session_id.into()],
        AgentId::Kimi => vec!["kimi".into(), "--session".into(), session_id.into()],
        AgentId::Grok => vec!["grok".into(), "--resume".into(), session_id.into()],
        AgentId::Pi => vec!["pi".into(), "--session".into(), session_id.into()],
        AgentId::Cursor => vec![
            cursor_program().into(),
            "--resume".into(),
            session_id.into(),
        ],
        AgentId::WorkBuddy | AgentId::Dsh => return None,
    };
    Some(argv)
}

fn cursor_program() -> &'static str {
    if cfg!(windows) {
        "cursor-agent.cmd"
    } else {
        "cursor-agent"
    }
}

pub(crate) fn valid_session_id(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_SESSION_ID_LEN || value.chars().any(char::is_control) {
        return None;
    }
    Some(value)
}

#[cfg(test)]
mod tests;
