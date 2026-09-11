//! SessionTitleSource extension port — read-only lookup of the title an Agent
//! wrote for one of its own sessions.

use std::path::Path;

use crate::error::Result;
use crate::platform::AgentKey;

/// Agent integration contribution for the Agent's own session title.
///
/// A source only reports what the Agent itself stored. When the Agent keeps no
/// title (or the session id is unknown to it) it returns `None`; callers fall
/// back to what AgentHub derived from the conversation.
pub trait SessionTitleSource: Send + Sync {
    fn agent_key(&self) -> AgentKey;

    /// Title the Agent wrote for `session_id` under `home` (the Agent home root).
    fn title_for(&self, home: &Path, session_id: &str) -> Result<Option<String>>;
}
