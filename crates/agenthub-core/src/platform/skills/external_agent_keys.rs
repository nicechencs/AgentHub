//! Fail-closed mapping between Hub [`AgentId`] and **external** ecosystem names.
//!
//! Hub skill projection for built-in agents still uses [`AgentId`] / [`AgentKey`]
//! directly (see integrations `register_skills_*`). This module exists for bridges
//! to community CLIs (e.g. a `skills --agent` namespace) where spellings often
//! disagree with product ids — uncertain entries must be [`None`], never guessed.
//!
//! Shape rules mirror the Orca/skills-cli lesson: values that look like flags
//! (leading `-`) must not be forwarded, or a CLI may silently drop targets.

use crate::models::AgentId;

/// Community `skills` CLI–style `--agent` key for a Hub [`AgentId`], when known.
///
/// Returns [`None`] when Hub does not claim a stable external spelling (caller
/// must drop the agent from the external target list rather than invent one).
///
/// This string is **not** a Hub [`crate::platform::AgentKey`] for projection
/// (`claude` stays `claude` inside Hub; external may say `claude-code`).
pub fn skills_cli_agent_key(agent: AgentId) -> Option<&'static str> {
    match agent {
        AgentId::Codex => Some("codex"),
        AgentId::Grok => Some("grok"),
        AgentId::Pi => Some("pi"),
        AgentId::Cursor => Some("cursor"),
        AgentId::Claude => Some("claude-code"),
        AgentId::Kiro => Some("kiro-cli"),
        AgentId::Kimi => Some("kimi-code-cli"),
        // No trusted external spelling yet.
        AgentId::WorkBuddy | AgentId::Zcode | AgentId::Dsh => None,
    }
}

/// Resolve an external skills-cli key back to a Hub [`AgentId`] when certain.
///
/// Unknown or flag-shaped values → [`None`] (fail-closed). Does not accept
/// Hub-only ids that lack an external mapping entry.
pub fn agent_id_for_skills_cli_key(value: &str) -> Option<AgentId> {
    if !is_usable_external_agent_key(value) {
        return None;
    }
    for agent in AgentId::ALL {
        if skills_cli_agent_key(agent) == Some(value) {
            return Some(agent);
        }
    }
    None
}

/// Whether `value` is shaped like a usable external agent key (or `*` wildcard).
///
/// Rejects empty strings and leading `-` (flag-shaped traps such as `-y`).
pub fn is_usable_external_agent_key(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    if value == "*" {
        return true;
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}
