use std::path::PathBuf;
use std::sync::Arc;

use crate::error::Result;
use crate::models::AgentId;
use crate::platform::paths::AgentPathContribution;
use crate::utils::paths::home_dir;

struct CodexPaths;

impl AgentPathContribution for CodexPaths {
    fn agent_id(&self) -> AgentId {
        AgentId::Codex
    }

    fn home_dir(&self) -> Result<PathBuf> {
        Ok(home_dir()?.join(".codex"))
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.paths.register(Arc::new(CodexPaths));
}
