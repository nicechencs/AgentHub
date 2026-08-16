use std::path::PathBuf;
use std::sync::Arc;

use crate::error::Result;
use crate::models::AgentId;
use crate::platform::paths::AgentPathContribution;
use crate::utils::paths::{first_env_path, home_dir};

struct ClaudePaths;

impl AgentPathContribution for ClaudePaths {
    fn agent_id(&self) -> AgentId {
        AgentId::Claude
    }

    fn home_dir(&self) -> Result<PathBuf> {
        if let Some(dir) = first_env_path("CLAUDE_CONFIG_DIR") {
            return Ok(dir);
        }
        Ok(home_dir()?.join(".claude"))
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.paths.register(Arc::new(ClaudePaths));
}
