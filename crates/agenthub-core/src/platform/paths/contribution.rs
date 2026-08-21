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

    /// Fixed default root, independent of any process environment override.
    fn default_home_dir(&self) -> Result<PathBuf> {
        self.home_dir()
    }

    /// Whether `home_dir` is the contribution's fixed default root.
    ///
    /// Contributions that honor an agent-owned environment override must
    /// return `false` whenever that override is present.  Purge policy uses
    /// this explicit signal instead of attempting to maintain a blacklist of
    /// environment variable names or unsafe directories.
    fn home_dir_is_default(&self) -> bool {
        true
    }

    /// Directory for file-manager open / live config (defaults to [`Self::home_dir`]).
    fn config_dir(&self) -> Result<PathBuf> {
        self.home_dir()
    }
}
