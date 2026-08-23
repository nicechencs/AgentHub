use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::Result;
use crate::integrations::shared::projects::{builtin_key, empty_if_missing, finish_sessions};
use crate::models::{AgentProject, AgentSession};
use crate::platform::projects::{ProjectScanContext, ProjectSource};
use crate::services::project_service::{
    grok_session_dir_for_delete, list_grok_projects, list_grok_sessions,
};

struct GrokProjectSource;

impl ProjectSource for GrokProjectSource {
    fn agent_key(&self) -> crate::platform::AgentKey {
        builtin_key("grok")
    }

    fn list_projects(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        list_grok_projects(ctx.home)
    }

    fn list_sessions(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentSession>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        Ok(finish_sessions(list_grok_sessions(ctx.home, None)?))
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
        Ok(finish_sessions(list_grok_sessions(ctx.home, Some(key))?))
    }

    fn delete_root_for_session_file(&self, abs: &Path) -> Option<PathBuf> {
        grok_session_dir_for_delete(abs)
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.projects
        .register(Arc::new(GrokProjectSource))
        .expect("unique built-in project source");
}
