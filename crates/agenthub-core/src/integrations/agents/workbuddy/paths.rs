use std::path::PathBuf;
use std::sync::Arc;

use crate::error::Result;
use crate::models::AgentId;
use crate::platform::paths::AgentPathContribution;
use crate::utils::paths::{first_env_path, home_dir};

struct WorkBuddyPaths;

impl AgentPathContribution for WorkBuddyPaths {
    fn agent_id(&self) -> AgentId {
        AgentId::WorkBuddy
    }

    fn home_dir(&self) -> Result<PathBuf> {
        Ok(home_dir()?.join(".workbuddy"))
    }

    fn config_dir(&self) -> Result<PathBuf> {
        for key in ["WORKBUDDY_CONFIG_DIR", "CODEBUDDY_CONFIG_DIR"] {
            if let Some(dir) = first_env_path(key) {
                return Ok(dir);
            }
        }
        self.home_dir()
    }

    fn config_dir_is_default(&self) -> bool {
        first_env_path("WORKBUDDY_CONFIG_DIR").is_none()
            && first_env_path("CODEBUDDY_CONFIG_DIR").is_none()
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.paths.register(Arc::new(WorkBuddyPaths));
}
