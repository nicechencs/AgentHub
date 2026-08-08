//! Install / upgrade / uninstall outcome shapes (CLI + GUI shared).

use serde::{Deserialize, Serialize};

use super::{DetectResult, EnvStatus};

/// Result of an install-related mutation (never claims success without redetect).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOutcome {
    pub ok: bool,
    /// `env_install` | `agent_install` | `agent_upgrade` | `agent_uninstall`
    pub action: String,
    pub logs: Vec<String>,
    pub message: String,
    /// Present after agent install/upgrade/uninstall redetect.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<DetectResult>,
    /// Present after runtime install redetect.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<EnvStatus>,
}

impl InstallOutcome {
    pub fn failure(
        action: impl Into<String>,
        logs: Vec<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            ok: false,
            action: action.into(),
            logs,
            message: message.into(),
            agent: None,
            runtime: None,
        }
    }
}
