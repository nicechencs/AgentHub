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
    /// DeepSeek Harness (`dsh`) — npm coding agent, not the DeepSeek API ticket.
    Dsh,
}

impl AgentId {
    pub const ALL: [AgentId; 8] = [
        AgentId::Claude,
        AgentId::Codex,
        AgentId::Kimi,
        AgentId::Grok,
        AgentId::Pi,
        AgentId::WorkBuddy,
        AgentId::Cursor,
        AgentId::Dsh,
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
            Self::Dsh => "dsh",
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
            "dsh" | "deepseek-harness" => Some(Self::Dsh),
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
            Self::Dsh => "DeepSeek Harness",
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

/// Lifecycle for one on-disk copy: source, update path, uninstall path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallLifecycle {
    /// npm | native | ide | desktop | leftover-agenthub
    pub source: &'static str,
    /// in_app | ide | desktop | official | none
    pub update_via: &'static str,
    /// in_app | ide | desktop | official | leftover | none
    pub uninstall_via: &'static str,
}

/// Same object for every agent copy so UI does not mix npm / IDE / Store.
pub fn install_lifecycle(agent: AgentId, kind: &str) -> InstallLifecycle {
    match kind {
        "npm" => InstallLifecycle {
            source: "npm",
            update_via: "in_app",
            uninstall_via: "in_app",
        },
        "native" if agent == AgentId::WorkBuddy => InstallLifecycle {
            source: "native",
            update_via: "official",
            uninstall_via: "in_app",
        },
        "native" => InstallLifecycle {
            source: "native",
            update_via: "in_app",
            uninstall_via: "in_app",
        },
        "ide" => InstallLifecycle {
            source: "ide",
            update_via: "ide",
            uninstall_via: "ide",
        },
        "desktop" => InstallLifecycle {
            source: "desktop",
            update_via: "desktop",
            uninstall_via: "desktop",
        },
        "leftover-agenthub" => InstallLifecycle {
            source: "leftover-agenthub",
            update_via: "none",
            uninstall_via: "leftover",
        },
        _ => InstallLifecycle {
            source: "native",
            update_via: "none",
            uninstall_via: "none",
        },
    }
}

/// An additional on-disk copy of an agent CLI (not the spawn target).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DetectedBinaryCopy {
    pub path: PathBuf,
    /// `npm` | `native` | `ide` | `desktop` | `leftover-agenthub`
    pub kind: String,
    pub version: Option<String>,
    pub channel: Option<String>,
    /// Install source (same codes as `kind` for known copies).
    #[serde(default)]
    pub source: String,
    /// Where this copy is updated.
    #[serde(default)]
    pub update_via: String,
    /// Where this copy is uninstalled.
    #[serde(default)]
    pub uninstall_via: String,
}

impl Default for DetectedBinaryCopy {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            kind: String::new(),
            version: None,
            channel: None,
            source: String::new(),
            update_via: "none".into(),
            uninstall_via: "none".into(),
        }
    }
}

impl DetectedBinaryCopy {
    pub fn from_kind(
        agent: AgentId,
        path: PathBuf,
        kind: &str,
        version: Option<String>,
        channel: Option<String>,
    ) -> Self {
        let life = install_lifecycle(agent, kind);
        Self {
            path,
            kind: kind.to_string(),
            version,
            channel,
            source: life.source.to_string(),
            update_via: life.update_via.to_string(),
            uninstall_via: life.uninstall_via.to_string(),
        }
    }
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
    /// Other copies besides `binary_path`. Empty for agents that only track one binary.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_copies: Vec<DetectedBinaryCopy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Other live credential families on disk besides [`Self::kind`].
    /// Typical values: `oauth`, `api_key`. Empty when only one family exists.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also_present: Vec<String>,
    /// SHA-256 of the live API key (trimmed). Never the raw secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_hash: Option<String>,
}

impl AuthState {
    /// Mark additional live credential families without changing the winning `kind`.
    pub fn with_also_present(mut self, kinds: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.also_present = kinds.into_iter().map(Into::into).collect();
        self
    }

    /// Attach a live API-key fingerprint without changing `kind`.
    pub fn with_secret_hash(mut self, hash: Option<String>) -> Self {
        self.secret_hash = hash.filter(|value| !value.is_empty());
        self
    }
}
