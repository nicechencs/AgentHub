use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::runtime::RuntimeId;
use crate::error::{AppError, Result};

/// Supported agents (lowercase id in CLI/API).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentId {
    Claude,
    Codex,
    Kimi,
    Grok,
    Pi,
    WorkBuddy,
    /// Cursor Agent CLI (half-surface: install/detect/run/skills/projects; no vscdb account pool).
    Cursor,
}

impl AgentId {
    pub const ALL: [AgentId; 7] = [
        AgentId::Claude,
        AgentId::Codex,
        AgentId::Kimi,
        AgentId::Grok,
        AgentId::Pi,
        AgentId::WorkBuddy,
        AgentId::Cursor,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Kimi => "kimi",
            Self::Grok => "grok",
            Self::Pi => "pi",
            Self::WorkBuddy => "workbuddy",
            Self::Cursor => "cursor",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "kimi" => Some(Self::Kimi),
            "grok" => Some(Self::Grok),
            "pi" => Some(Self::Pi),
            "workbuddy" => Some(Self::WorkBuddy),
            // Alias: historical docs sometimes say cursor-agent; serialize id stays `cursor`.
            "cursor" | "cursor-agent" => Some(Self::Cursor),
            _ => None,
        }
    }

    /// Parse a required agent id; invalid values become [`AppError::InvalidArg`].
    ///
    /// Shared by CLI and Tauri so error wording stays consistent.
    pub fn parse_required(s: &str) -> Result<Self> {
        Self::parse(s).ok_or_else(|| {
            AppError::InvalidArg(format!(
                "invalid agent id '{s}', expected: {}",
                Self::expected_list()
            ))
        })
    }

    /// Parse an optional agent filter (`None` / blank → `Ok(None)`).
    pub fn parse_optional(s: Option<&str>) -> Result<Option<Self>> {
        match s {
            None => Ok(None),
            Some(raw) if raw.trim().is_empty() => Ok(None),
            Some(raw) => Ok(Some(Self::parse_required(raw)?)),
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Kimi => "Kimi Code",
            Self::Grok => "Grok",
            Self::Pi => "Pi",
            Self::WorkBuddy => "WorkBuddy",
            Self::Cursor => "Cursor Agent",
        }
    }

    /// Pipe-joined ids for CLI/Tauri error messages (`claude|codex|…`).
    /// Prefer this over hardcoding agent lists at call sites.
    pub fn expected_list() -> String {
        Self::ALL
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
            .join("|")
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectStatus {
    Installed,
    NotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectResult {
    pub agent: AgentId,
    pub status: DetectStatus,
    pub version: Option<String>,
    pub binary_path: Option<PathBuf>,
    pub channel: Option<String>,
    /// Whether default install channel's runtimes are ready.
    pub env_ready: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallChannel {
    pub id: String,
    pub label: String,
    pub requires: Vec<RuntimeId>,
    /// Example: Node >= 18
    pub min_runtime_notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub agent: AgentId,
    /// Opaque live config snapshot (JSON); field shapes differ per agent.
    pub raw: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthHealth {
    Verified,
    Renewable,
    Configured,
    NeedsLogin,
    Unknown,
    Missing,
}

impl Default for AuthHealth {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthState {
    pub agent: AgentId,
    pub kind: Option<String>,
    /// Desensitized summary only.
    pub summary: String,
    pub has_credentials: bool,
    /// Live authentication health. Older payloads omit this field and decode
    /// as [`AuthHealth::Unknown`].
    #[serde(default)]
    pub health: AuthHealth,
    /// Non-secret identifier for the source that was inspected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Opaque revision of the live auth source (for change detection only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}
