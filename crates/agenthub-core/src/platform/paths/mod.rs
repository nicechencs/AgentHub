//! Agent path contributions — home / config roots without utils-level match arms.
//!
//! [`crate::utils::paths::agent_home`] is a thin façade over this registry.
//!
//! Per-agent roots live in [`crate::integrations`].

mod contribution;
mod registry;
mod sources;

pub use contribution::AgentPathContribution;
pub use registry::{builtin_path_registry, AgentPathRegistry};

use crate::error::Result;
use crate::models::AgentId;
use std::path::PathBuf;

/// Resolve agent data/config home (env overrides included).
pub fn resolve_agent_home(agent: AgentId) -> Result<PathBuf> {
    builtin_path_registry()
        .get(agent)
        .ok_or_else(|| {
            crate::error::AppError::NotFound(format!(
                "no path contribution for agent {}",
                agent.as_str()
            ))
        })?
        .home_dir()
}

/// Resolve directory to open for manual verification (may differ from home).
pub fn resolve_agent_config_dir(agent: AgentId) -> Result<PathBuf> {
    builtin_path_registry()
        .get(agent)
        .ok_or_else(|| {
            crate::error::AppError::NotFound(format!(
                "no path contribution for agent {}",
                agent.as_str()
            ))
        })?
        .config_dir()
}

#[cfg(test)]
mod tests;
