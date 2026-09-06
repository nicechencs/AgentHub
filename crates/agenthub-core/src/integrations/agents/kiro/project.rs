use std::sync::Arc;

use crate::error::Result;
use crate::integrations::shared::projects::builtin_key;
use crate::models::{AgentProject, AgentSession};
use crate::platform::projects::{ProjectScanContext, ProjectSource};

/// Wave 1: register an empty source so catalog/projects stay complete.
/// Do not invent `.kiro/` session layout until paths are verified.
struct KiroProjectSource;

impl ProjectSource for KiroProjectSource {
    fn agent_key(&self) -> crate::platform::AgentKey {
        builtin_key("kiro")
    }

    fn list_projects(&self, _ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>> {
        Ok(vec![])
    }

    fn list_sessions(&self, _ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentSession>> {
        Ok(vec![])
    }

    fn list_sessions_in_project(
        &self,
        _ctx: &ProjectScanContext<'_>,
        _project_id: &str,
        _key: &str,
    ) -> Result<Vec<AgentSession>> {
        Ok(vec![])
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.projects
        .register(Arc::new(KiroProjectSource))
        .expect("unique built-in project source");
}
