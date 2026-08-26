//! Shared `-a/--agent` parsing for command modules.

use agenthub_core::error::{AppError, Result};
use agenthub_core::models::AgentId;

/// Parse optional global `-a/--agent` into [`AgentId`].
///
/// Invalid values become [`AppError::InvalidArg`] (CLI exit code 2).
pub fn parse_agent_filter(agent_filter: Option<&str>) -> Result<Option<AgentId>> {
    AgentId::parse_optional(agent_filter)
}

/// Require `--agent`, using `{kind} {operation} requires --agent <list>`.
pub fn require_agent(agent_filter: Option<&str>, kind: &str, operation: &str) -> Result<AgentId> {
    parse_agent_filter(agent_filter)?.ok_or_else(|| {
        AppError::InvalidArg(format!(
            "{kind} {operation} requires --agent <{}>",
            AgentId::expected_list()
        ))
    })
}
