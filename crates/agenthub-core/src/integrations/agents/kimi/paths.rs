use std::path::PathBuf;
use std::sync::Arc;

use crate::error::Result;
use crate::models::AgentId;
use crate::platform::paths::AgentPathContribution;
use crate::utils::paths::home_dir;

struct KimiPaths;

impl AgentPathContribution for KimiPaths {
    fn agent_id(&self) -> AgentId {
        AgentId::Kimi
    }

    fn home_dir(&self) -> Result<PathBuf> {
        let home = home_dir()?;
        let neu = home.join(".kimi-code");
        if neu.exists() {
            Ok(neu)
        } else if home.join(".kimi").exists() {
            Ok(home.join(".kimi"))
        } else {
            Ok(neu)
        }
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.paths.register(Arc::new(KimiPaths));
}
