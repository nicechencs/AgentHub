//! Path contributions per agent.

use std::path::PathBuf;
use std::sync::Arc;

use super::contribution::AgentPathContribution;
use super::registry::AgentPathRegistry;
use crate::error::Result;
use crate::models::AgentId;
use crate::utils::paths::{first_env_path, home_dir};

struct ClaudePaths;
struct CodexPaths;
struct KimiPaths;
struct GrokPaths;
struct PiPaths;
struct WorkBuddyPaths;
struct CursorPaths;

impl AgentPathContribution for ClaudePaths {
    fn agent_id(&self) -> AgentId {
        AgentId::Claude
    }
    fn home_dir(&self) -> Result<PathBuf> {
        if let Some(dir) = first_env_path("CLAUDE_CONFIG_DIR") {
            return Ok(dir);
        }
        Ok(home_dir()?.join(".claude"))
    }
}

impl AgentPathContribution for CodexPaths {
    fn agent_id(&self) -> AgentId {
        AgentId::Codex
    }
    fn home_dir(&self) -> Result<PathBuf> {
        Ok(home_dir()?.join(".codex"))
    }
}

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

impl AgentPathContribution for GrokPaths {
    fn agent_id(&self) -> AgentId {
        AgentId::Grok
    }
    fn home_dir(&self) -> Result<PathBuf> {
        Ok(home_dir()?.join(".grok"))
    }
}

impl AgentPathContribution for PiPaths {
    fn agent_id(&self) -> AgentId {
        AgentId::Pi
    }
    fn home_dir(&self) -> Result<PathBuf> {
        Ok(home_dir()?.join(".pi"))
    }
    fn config_dir(&self) -> Result<PathBuf> {
        if let Some(dir) = first_env_path("PI_CODING_AGENT_DIR") {
            return Ok(dir);
        }
        Ok(self.home_dir()?.join("agent"))
    }
}

impl AgentPathContribution for WorkBuddyPaths {
    fn agent_id(&self) -> AgentId {
        AgentId::WorkBuddy
    }
    fn home_dir(&self) -> Result<PathBuf> {
        Ok(home_dir()?.join(".workbuddy"))
    }
    fn config_dir(&self) -> Result<PathBuf> {
        for key in ["WORKBUDDY_CONFIG_DIR", "CODEBUDDY_CONFIG_DIR"] {
            if let Some(dir) = first_env_path(key) {
                return Ok(dir);
            }
        }
        self.home_dir()
    }
}

impl AgentPathContribution for CursorPaths {
    fn agent_id(&self) -> AgentId {
        AgentId::Cursor
    }
    fn home_dir(&self) -> Result<PathBuf> {
        Ok(home_dir()?.join(".cursor"))
    }
}

pub fn build_registry() -> AgentPathRegistry {
    let mut reg = AgentPathRegistry::new();
    reg.register(Arc::new(ClaudePaths));
    reg.register(Arc::new(CodexPaths));
    reg.register(Arc::new(KimiPaths));
    reg.register(Arc::new(GrokPaths));
    reg.register(Arc::new(PiPaths));
    reg.register(Arc::new(WorkBuddyPaths));
    reg.register(Arc::new(CursorPaths));
    reg
}
