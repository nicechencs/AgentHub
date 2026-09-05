use std::sync::Arc;

use crate::error::Result;
use crate::integrations::shared::projects::{builtin_key, empty_if_missing, finish_sessions};
use crate::models::{AgentId, AgentProject, AgentSession};
use crate::platform::projects::{ProjectScanContext, ProjectSource};
use crate::services::project_service::{
    list_claude_workbuddy_projects, list_claude_workbuddy_sessions,
};

struct ClaudeProjectSource;

impl ProjectSource for ClaudeProjectSource {
    fn agent_key(&self) -> crate::platform::AgentKey {
        builtin_key("claude")
    }

    fn list_projects(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        list_claude_workbuddy_projects(ctx.home, AgentId::Claude)
    }

    fn list_sessions(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentSession>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        Ok(finish_sessions(list_claude_workbuddy_sessions(
            ctx.home,
            AgentId::Claude,
            None,
            ctx.data_dir,
        )?))
    }

    fn list_sessions_in_project(
        &self,
        ctx: &ProjectScanContext<'_>,
        _project_id: &str,
        key: &str,
    ) -> Result<Vec<AgentSession>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        Ok(finish_sessions(list_claude_workbuddy_sessions(
            ctx.home,
            AgentId::Claude,
            Some(key),
            ctx.data_dir,
        )?))
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.projects
        .register(Arc::new(ClaudeProjectSource))
        .expect("unique built-in project source");
}
