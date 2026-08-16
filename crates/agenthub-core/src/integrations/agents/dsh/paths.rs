use std::path::PathBuf;
use std::sync::Arc;

use crate::error::Result;
use crate::models::AgentId;
use crate::platform::paths::AgentPathContribution;

struct DshPaths;

impl AgentPathContribution for DshPaths {
    fn agent_id(&self) -> AgentId {
        AgentId::Dsh
    }

    fn home_dir(&self) -> Result<PathBuf> {
        crate::adapters::dsh::resolve_dsh_home()
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.paths.register(Arc::new(DshPaths));
}
