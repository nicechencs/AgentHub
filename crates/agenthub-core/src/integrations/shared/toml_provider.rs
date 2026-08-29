//! Provider-managed TOML key lookup — one list per TOML agent.

use crate::error::{AppError, Result};
use crate::integrations::agents::{codex, grok, kimi};
use crate::models::AgentId;

/// Top-level keys `write_toml_config` may replace on provider switch.
pub(crate) fn managed_toml_provider_keys(agent: AgentId) -> Result<&'static [&'static str]> {
    match agent {
        AgentId::Codex => Ok(codex::managed::PROVIDER_TOML_KEYS),
        AgentId::Kimi => Ok(kimi::managed::PROVIDER_TOML_KEYS),
        AgentId::Grok => Ok(grok::managed::PROVIDER_TOML_KEYS),
        AgentId::Claude
        | AgentId::Pi
        | AgentId::WorkBuddy
        | AgentId::Cursor
        | AgentId::Dsh
        | AgentId::Zcode => {
            Err(AppError::InvalidArg(format!(
                "{} provider config is JSON, not TOML",
                agent.display_name()
            )))
        }
    }
}
