use std::sync::Arc;

use crate::error::Result;
use crate::integrations::shared::projects::{builtin_key, empty_if_missing, finish_sessions};
use crate::models::{AgentId, AgentProject, AgentSession};
use crate::platform::projects::{ProjectScanContext, ProjectSource};
use crate::services::project_service::{aggregate_projects, list_codex_sessions};

struct CodexProjectSource;

impl ProjectSource for CodexProjectSource {
    fn agent_key(&self) -> crate::platform::AgentKey {
        builtin_key("codex")
    }

    fn list_projects(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        let sessions = self.list_sessions(ctx)?;
        Ok(aggregate_projects(AgentId::Codex, ctx.home, &sessions))
    }

    fn list_sessions(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentSession>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        Ok(finish_sessions(list_codex_sessions(
            ctx.home,
            None,
            ctx.data_dir,
        )?))
    }

    fn list_sessions_in_project(
        &self,
        ctx: &ProjectScanContext<'_>,
        project_id: &str,
        _key: &str,
    ) -> Result<Vec<AgentSession>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        Ok(finish_sessions(list_codex_sessions(
            ctx.home,
            Some(project_id),
            ctx.data_dir,
        )?))
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.projects
        .register(Arc::new(CodexProjectSource))
        .expect("unique built-in project source");
}
