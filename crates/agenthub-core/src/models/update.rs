//! Agent CLI update-check DTOs (GUI + CLI shared).

use serde::{Deserialize, Serialize};

use super::AgentId;

/// Result of comparing installed version vs a remote latest source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentUpdateState {
    /// Installed and remote latest is strictly newer.
    UpdateAvailable,
    /// Installed and versions match (or local appears newer / equal).
    UpToDate,
    /// Could not determine (network failure, no registry package, unparseable versions).
    Unknown,
    /// Agent has no automated update probe (e.g. Setup-only WorkBuddy).
    Unsupported,
    /// Not installed — UI should hide upgrade affordances.
    NotInstalled,
}

/// Per-agent update probe result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUpdateInfo {
    pub agent_id: AgentId,
    pub state: AgentUpdateState,
    pub current_version: Option<String>,
    pub latest_version: Option<String>,
    /// Probe source: `npm` | `npm:next` | `none` | install-channel label when not probed.
    pub source: Option<String>,
    /// ISO-8601 UTC when a remote latest was successfully read (cache or network).
    pub checked_at: Option<String>,
    /// Human-readable note (errors, channel limitations).
    pub note: Option<String>,
    /// Official Setup / download page when auto-update is unsupported (open in browser).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_url: Option<String>,
}

impl AgentUpdateInfo {
    pub fn not_installed(agent_id: AgentId) -> Self {
        Self {
            agent_id,
            state: AgentUpdateState::NotInstalled,
            current_version: None,
            latest_version: None,
            source: None,
            checked_at: None,
            note: None,
            setup_url: None,
        }
    }

    pub fn unsupported(
        agent_id: AgentId,
        current: Option<String>,
        note: impl Into<String>,
        setup_url: Option<String>,
    ) -> Self {
        Self {
            agent_id,
            state: AgentUpdateState::Unsupported,
            current_version: current,
            latest_version: None,
            source: Some("none".into()),
            checked_at: None,
            note: Some(note.into()),
            setup_url,
        }
    }

    pub fn unknown(
        agent_id: AgentId,
        current: Option<String>,
        source: Option<String>,
        note: impl Into<String>,
    ) -> Self {
        Self {
            agent_id,
            state: AgentUpdateState::Unknown,
            current_version: current,
            latest_version: None,
            source,
            checked_at: None,
            note: Some(note.into()),
            setup_url: None,
        }
    }
}
