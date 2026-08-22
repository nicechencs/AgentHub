use std::path::PathBuf;
use std::sync::Arc;

use crate::error::Result;
use crate::models::AgentId;
use crate::platform::paths::AgentPathContribution;
use crate::utils::paths::{first_env_path, home_dir};

struct DshPaths;

impl AgentPathContribution for DshPaths {
    fn agent_id(&self) -> AgentId {
        AgentId::Dsh
    }

    fn home_dir(&self) -> Result<PathBuf> {
        crate::adapters::dsh::resolve_dsh_home()
    }

    fn default_home_dir(&self) -> Result<PathBuf> {
        Ok(home_dir()?.join(".dsh"))
    }

    fn home_dir_is_default(&self) -> bool {
        first_env_path("DSH_HOME").is_none()
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.paths.register(Arc::new(DshPaths));
}
