//! StreamParser extension port — agent-specific line decoding only.

use std::sync::Arc;

use crate::models::{AgentId, ProcessStep};
use crate::platform::AgentKey;

/// Typed unsupported / missing parser (not a panic path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamParseError {
    pub agent_key: String,
    pub code: &'static str,
    pub message: String,
}

impl StreamParseError {
    pub fn unsupported_key(agent_key: &AgentKey) -> Self {
        Self {
            agent_key: agent_key.as_str().to_string(),
            code: "unsupported",
            message: format!(
                "structured stream parsing is not supported for {}",
                agent_key.as_str()
            ),
        }
    }

    /// Compatibility façade for callers that still use the closed built-in id.
    pub fn unsupported(agent: AgentId) -> Self {
        Self::unsupported_key(&AgentKey::from_agent_id(agent))
    }
}

impl std::fmt::Display for StreamParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for StreamParseError {}

/// Agent contribution: parse one complete NDJSON / JSON line into process steps.
///
/// Stateless parsers are fine; any session state should live inside the
/// implementation (not global statics). Does not I/O, DB, or UI.
pub trait StreamParser: Send + Sync {
    fn agent_key(&self) -> AgentKey;

    /// Parse one complete line (no trailing newline required).
    ///
    /// - `None` — not JSON / not this agent's event shape (caller may raw-fallback)
    /// - `Some(empty)` — recognized no-op event
    /// - `Some(steps)` — decoded steps (Text / Thinking / Tool / Status / Error / Raw)
    fn parse_line(&self, line: &str) -> Option<Vec<ProcessStep>>;

    /// Optional end-of-stream flush for partial parser state.
    fn flush(&self) -> Option<Vec<ProcessStep>> {
        None
    }

    /// Fresh parser for one [`crate::utils::stream_parse::StreamSession`].
    ///
    /// `None` (default) reuses the registry Arc — correct for stateless parsers.
    /// Stateful parsers must return a new instance so concurrent sessions and
    /// tests do not share turn flags.
    fn for_session(&self) -> Option<Arc<dyn StreamParser>> {
        None
    }
}
