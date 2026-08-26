//! Agent-native project containers and session records.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::AgentId;

/// Workspace / storage container grouping native sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProject {
    /// Stable id: `{agent}:proj:{storage_key}` (forward-slash separators).
    pub id: String,
    pub agent_id: AgentId,
    /// Display title (prefer actual_path tail, else storage dir name).
    pub title: String,
    /// Absolute native storage root (e.g. `~/.claude/projects/-Users-…`).
    pub storage_path: String,
    /// Workspace path that exists on disk (decoded + verified). Display may
    /// still restore an unverified address in the UI; only this field is openable.
    pub actual_path: Option<String>,
    /// Relative path under the agent home.
    pub relative_path: String,
    pub session_count: u32,
    /// Rough message / line count when cheap to aggregate.
    pub message_count: Option<u32>,
    pub size_bytes: u64,
    /// ISO-8601 mtime (max of child sessions when applicable).
    pub updated_at: String,
    /// Optional preview from the newest session.
    pub preview: Option<String>,
    /// User alias from AgentHub-side metadata (not native logs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    /// Hidden via AgentHub-side metadata.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

/// One session file (or Cursor workspace folder listed as a leaf in older flows).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    /// Stable id: `{agent}:{relative_path}` (forward-slash separators).
    pub id: String,
    /// Parent container id (`{agent}:proj:…`).
    pub project_id: String,
    pub agent_id: AgentId,
    /// Display title (preview / file stem / cwd tail).
    pub title: String,
    /// Best-effort workspace path for this session.
    pub cwd: Option<String>,
    /// Absolute path to the session file.
    pub path: String,
    /// Relative path under the agent home.
    pub relative_path: String,
    pub size_bytes: u64,
    /// ISO-8601 mtime when available.
    pub updated_at: String,
    /// Optional first-user-message / summary snippet (truncated).
    pub preview: Option<String>,
    /// Rough message / line count when cheap to compute.
    pub message_count: Option<u32>,
    /// Native CLI session id (for resume / copy), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Conversation text for project preview / continue-chat context.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProjectExcerpt {
    pub id: String,
    pub agent_id: AgentId,
    pub title: String,
    pub cwd: Option<String>,
    pub updated_at: String,
    /// Extracted user/assistant turns, kept in full.
    pub excerpt: String,
}

/// Per-project user preferences (stored under AgentHub data_dir, not agent home).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUserMeta {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

impl ProjectUserMeta {
    pub fn is_empty(&self) -> bool {
        !self.hidden
            && self
                .alias
                .as_ref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
    }
}

/// On-disk document: `{data_dir}/project_metadata.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMetadataFile {
    pub version: u32,
    #[serde(default)]
    pub show_hidden_projects: bool,
    /// Keyed by `AgentProject.id`.
    #[serde(default)]
    pub projects: BTreeMap<String, ProjectUserMeta>,
}

impl Default for ProjectMetadataFile {
    fn default() -> Self {
        Self {
            version: 1,
            show_hidden_projects: false,
            projects: BTreeMap::new(),
        }
    }
}
