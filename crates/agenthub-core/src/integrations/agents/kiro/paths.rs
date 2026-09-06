use std::path::PathBuf;
use std::sync::Arc;

use crate::error::Result;
use crate::models::AgentId;
use crate::platform::paths::AgentPathContribution;
use crate::utils::paths::home_dir;

struct KiroPaths;

impl AgentPathContribution for KiroPaths {
    fn agent_id(&self) -> AgentId {
        AgentId::Kiro
    }

    fn home_dir(&self) -> Result<PathBuf> {
        Ok(home_dir()?.join(".kiro"))
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.paths.register(Arc::new(KiroPaths));
}
