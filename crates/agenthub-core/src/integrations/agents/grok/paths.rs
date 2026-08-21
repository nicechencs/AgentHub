use std::path::PathBuf;
use std::sync::Arc;

use crate::error::Result;
use crate::models::AgentId;
use crate::platform::paths::AgentPathContribution;
use crate::utils::paths::{first_env_path, home_dir};

struct GrokPaths;

impl AgentPathContribution for GrokPaths {
    fn agent_id(&self) -> AgentId {
        AgentId::Grok
    }

    fn home_dir(&self) -> Result<PathBuf> {
        if let Some(dir) = first_env_path("GROK_HOME") {
            return Ok(dir);
        }
        Ok(home_dir()?.join(".grok"))
    }

    fn default_home_dir(&self) -> Result<PathBuf> {
        Ok(home_dir()?.join(".grok"))
    }

    fn home_dir_is_default(&self) -> bool {
        first_env_path("GROK_HOME").is_none()
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.paths.register(Arc::new(GrokPaths));
}
