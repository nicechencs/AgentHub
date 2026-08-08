//! ProjectSource extension port — read-only project/session discovery.

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::models::{AgentProject, AgentSession};
use crate::platform::AgentKey;

/// Scan inputs for one agent home root.
#[derive(Debug, Clone)]
pub struct ProjectScanContext<'a> {
    pub home: &'a Path,
    /// AgentHub data dir (optional session mtime index for Codex / Kimi / Pi).
    pub data_dir: Option<&'a Path>,
}

/// Agent integration contribution for project/session discovery.
///
/// Platform service owns merge, sort, metadata, capability gates, and deletes.
pub trait ProjectSource: Send + Sync {
    fn agent_key(&self) -> AgentKey;

    /// List project containers under `ctx.home`.
    fn list_projects(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>>;

    /// List all sessions under `ctx.home` (may be empty, e.g. Cursor).
    fn list_sessions(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentSession>>;

    /// List sessions belonging to one project (`project_id` + storage `key`).
    fn list_sessions_in_project(
        &self,
        ctx: &ProjectScanContext<'_>,
        project_id: &str,
        key: &str,
    ) -> Result<Vec<AgentSession>>;

    /// When deleting a session path, optionally expand to a directory root
    /// (Grok/Kimi session dirs). Default: delete the path itself.
    fn delete_root_for_session_file(&self, _abs: &Path) -> Option<PathBuf> {
        None
    }
}
