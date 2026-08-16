use std::path::PathBuf;
use std::sync::Arc;

use crate::error::Result;
use crate::models::AgentId;
use crate::platform::paths::AgentPathContribution;
use crate::utils::paths::{first_env_path, home_dir};

struct PiPaths;

impl AgentPathContribution for PiPaths {
    fn agent_id(&self) -> AgentId {
        AgentId::Pi
    }

    fn home_dir(&self) -> Result<PathBuf> {
        Ok(home_dir()?.join(".pi"))
    }

    fn config_dir(&self) -> Result<PathBuf> {
        if let Some(dir) = first_env_path("PI_CODING_AGENT_DIR") {
            return Ok(dir);
        }
        Ok(self.home_dir()?.join("agent"))
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.paths.register(Arc::new(PiPaths));
}
