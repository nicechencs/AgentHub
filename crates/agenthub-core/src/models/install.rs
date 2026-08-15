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
    /// Stable machine code for CLI exit mapping (`env.not_ready`, `unsupported`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Structured details (no secrets). Used by CLI `--output json` errors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl Default for InstallOutcome {
    fn default() -> Self {
        Self {
            ok: false,
            action: String::new(),
            logs: Vec::new(),
            message: String::new(),
            agent: None,
            runtime: None,
            code: None,
            details: None,
        }
    }
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
            ..Self::default()
        }
    }

    pub fn with_code(
        mut self,
        code: impl Into<String>,
        details: Option<serde_json::Value>,
    ) -> Self {
        self.code = Some(code.into());
        self.details = details;
        self
    }
}
