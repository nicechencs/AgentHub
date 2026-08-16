use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::Result;
use crate::integrations::shared::projects::{builtin_key, empty_if_missing, finish_sessions};
use crate::models::{AgentId, AgentProject, AgentSession};
use crate::platform::projects::{ProjectScanContext, ProjectSource};
use crate::services::project_service::{
    aggregate_projects, kimi_session_dir_for_delete, list_kimi_sessions,
};

struct KimiProjectSource;

impl ProjectSource for KimiProjectSource {
    fn agent_key(&self) -> crate::platform::AgentKey {
        builtin_key("kimi")
    }

    fn list_projects(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        let sessions = self.list_sessions(ctx)?;
        Ok(aggregate_projects(AgentId::Kimi, ctx.home, &sessions))
    }

    fn list_sessions(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentSession>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        Ok(finish_sessions(list_kimi_sessions(ctx.home, None)?))
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
        Ok(finish_sessions(list_kimi_sessions(ctx.home, Some(key))?))
    }

    fn delete_root_for_session_file(&self, abs: &Path) -> Option<PathBuf> {
        kimi_session_dir_for_delete(abs)
    }
}

pub fn register(ctx: &mut crate::integrations::IntegrationContext<'_>) {
    ctx.projects
        .register(Arc::new(KimiProjectSource))
        .expect("unique built-in project source");
}
