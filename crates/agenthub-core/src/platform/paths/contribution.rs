//! Per-agent path roots.

use std::path::PathBuf;

use crate::error::Result;
use crate::models::AgentId;
use crate::platform::AgentKey;

/// Agent-specific home / config directory resolution.
pub trait AgentPathContribution: Send + Sync {
    fn agent_id(&self) -> AgentId;

    fn agent_key(&self) -> AgentKey {
        AgentKey::from_agent_id(self.agent_id())
    }

    /// Primary data root (e.g. `~/.claude`, `~/.codex`).
    fn home_dir(&self) -> Result<PathBuf>;

    /// Directory for file-manager open / live config (defaults to [`Self::home_dir`]).
    fn config_dir(&self) -> Result<PathBuf> {
        self.home_dir()
    }
}
