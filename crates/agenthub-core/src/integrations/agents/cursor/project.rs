use std::sync::Arc;

use crate::error::Result;
use crate::integrations::shared::projects::{builtin_key, empty_if_missing, finish_sessions};
use crate::models::{AgentProject, AgentSession};
use crate::platform::projects::{ProjectScanContext, ProjectSource};
use crate::services::project_service::{list_cursor_projects, list_cursor_sessions};

struct CursorProjectSource;

impl ProjectSource for CursorProjectSource {
    fn agent_key(&self) -> crate::platform::AgentKey {
        builtin_key("cursor")
    }

    fn list_projects(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        let rows = list_cursor_projects(ctx.home, ctx.data_dir)?;
        // Desktop IDE windows (numeric ids / canvases) are not Cursor Agent CLI.
        Ok(rows
            .into_iter()
            .filter(|p| {
                ctx.home
                    .join(&p.relative_path)
                    .join("agent-transcripts")
                    .is_dir()
            })
            .collect())
    }

    fn list_sessions(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentSession>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        Ok(finish_sessions(list_cursor_sessions(ctx.home, None)?))
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
        Ok(finish_sessions(list_cursor_sessions(ctx.home, Some(key))?))
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.projects
        .register(Arc::new(CursorProjectSource))
        .expect("unique built-in project source");
}
