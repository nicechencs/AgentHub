//! ProjectSource contributions — agent-specific discovery only.
//!
//! Scan helpers live in the parent module; this file only wires the port.
//! TODO(P13): relocate under integrations/agents/<key>/.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use crate::error::Result;
use crate::models::{AgentId, AgentProject, AgentSession};
use crate::platform::projects::{ProjectScanContext, ProjectSource, ProjectSourceRegistry};
use crate::platform::AgentKey;

use super::finish_sessions;
use super::scan::{
    aggregate_projects, grok_session_dir_for_delete, kimi_session_dir_for_delete,
    list_claude_workbuddy_sessions, list_codex_sessions, list_cursor_projects, list_dsh_sessions,
    list_grok_sessions, list_kimi_sessions, list_pi_sessions,
};

pub fn builtin_project_registry() -> &'static ProjectSourceRegistry {
    static REG: OnceLock<ProjectSourceRegistry> = OnceLock::new();
    REG.get_or_init(build_registry)
}

fn build_registry() -> ProjectSourceRegistry {
    let mut reg = ProjectSourceRegistry::new();
    // Keep the established AgentId::ALL product order explicit.
    reg.register(Arc::new(ClaudeProjectSource))
        .expect("unique built-in project source");
    reg.register(Arc::new(CodexProjectSource))
        .expect("unique built-in project source");
    reg.register(Arc::new(KimiProjectSource))
        .expect("unique built-in project source");
    reg.register(Arc::new(GrokProjectSource))
        .expect("unique built-in project source");
    reg.register(Arc::new(PiProjectSource))
        .expect("unique built-in project source");
    reg.register(Arc::new(WorkBuddyProjectSource))
        .expect("unique built-in project source");
    reg.register(Arc::new(CursorProjectSource))
        .expect("unique built-in project source");
    reg.register(Arc::new(DshProjectSource))
        .expect("unique built-in project source");
    reg
}

fn builtin_key(key: &'static str) -> AgentKey {
    AgentKey::parse(key).expect("built-in project source key is valid")
}

fn empty_if_missing(home: &Path) -> bool {
    !home.exists()
}

// --- Claude / WorkBuddy (shared layout) ------------------------------------

struct ClaudeProjectSource;
struct WorkBuddyProjectSource;

macro_rules! claude_like_source {
    ($ty:ident, $key:literal, $agent:expr) => {
        impl ProjectSource for $ty {
            fn agent_key(&self) -> AgentKey {
                builtin_key($key)
            }

            fn list_projects(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>> {
                if empty_if_missing(ctx.home) {
                    return Ok(vec![]);
                }
                let sessions = self.list_sessions(ctx)?;
                Ok(aggregate_projects($agent, ctx.home, &sessions))
            }

            fn list_sessions(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentSession>> {
                if empty_if_missing(ctx.home) {
                    return Ok(vec![]);
                }
                Ok(finish_sessions(list_claude_workbuddy_sessions(
                    ctx.home, $agent, None,
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
                    $agent,
                    Some(key),
                )?))
            }
        }
    };
}

claude_like_source!(ClaudeProjectSource, "claude", AgentId::Claude);
claude_like_source!(WorkBuddyProjectSource, "workbuddy", AgentId::WorkBuddy);

// --- Codex -----------------------------------------------------------------

struct CodexProjectSource;

impl ProjectSource for CodexProjectSource {
    fn agent_key(&self) -> AgentKey {
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

// --- Kimi ------------------------------------------------------------------

struct KimiProjectSource;

impl ProjectSource for KimiProjectSource {
    fn agent_key(&self) -> AgentKey {
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

// --- Grok ------------------------------------------------------------------

struct GrokProjectSource;

impl ProjectSource for GrokProjectSource {
    fn agent_key(&self) -> AgentKey {
        builtin_key("grok")
    }

    fn list_projects(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        let sessions = self.list_sessions(ctx)?;
        Ok(aggregate_projects(AgentId::Grok, ctx.home, &sessions))
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

// --- Pi --------------------------------------------------------------------

struct PiProjectSource;

impl ProjectSource for PiProjectSource {
    fn agent_key(&self) -> AgentKey {
        builtin_key("pi")
    }

    fn list_projects(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        let sessions = self.list_sessions(ctx)?;
        Ok(aggregate_projects(AgentId::Pi, ctx.home, &sessions))
    }

    fn list_sessions(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentSession>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        Ok(finish_sessions(list_pi_sessions(ctx.home, None)?))
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
        Ok(finish_sessions(list_pi_sessions(ctx.home, Some(key))?))
    }
}

// --- Cursor (workspace folders only; no session transcripts) ---------------

struct CursorProjectSource;

impl ProjectSource for CursorProjectSource {
    fn agent_key(&self) -> AgentKey {
        builtin_key("cursor")
    }

    fn list_projects(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        list_cursor_projects(ctx.home)
    }

    fn list_sessions(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentSession>> {
        let _ = ctx;
        Ok(vec![])
    }

    fn list_sessions_in_project(
        &self,
        ctx: &ProjectScanContext<'_>,
        _project_id: &str,
        _key: &str,
    ) -> Result<Vec<AgentSession>> {
        let _ = ctx;
        Ok(vec![])
    }
}

// --- DSH (JSONL under known persistence roots) -----------------------------

struct DshProjectSource;

impl ProjectSource for DshProjectSource {
    fn agent_key(&self) -> AgentKey {
        builtin_key("dsh")
    }

    fn list_projects(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        let sessions = self.list_sessions(ctx)?;
        Ok(aggregate_projects(AgentId::Dsh, ctx.home, &sessions))
    }

    fn list_sessions(&self, ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentSession>> {
        if empty_if_missing(ctx.home) {
            return Ok(vec![]);
        }
        Ok(finish_sessions(list_dsh_sessions(
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
        Ok(finish_sessions(list_dsh_sessions(
            ctx.home,
            Some(project_id),
            ctx.data_dir,
        )?))
    }
}
