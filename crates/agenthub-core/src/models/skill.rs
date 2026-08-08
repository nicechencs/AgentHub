//! Skill source + per-agent projection models.
//!
//! Wire format is camelCase / lowercase enums for CLI JSON and GUI.
//!
//! Projection is orthogonal to install: a skill is **installed** when it exists
//! under a skill root; a **projection** maps a source skill into one agent root
//! via link (symlink / junction) or copy.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::AgentId;

/// How a projected skill directory is linked (if at all).
///
/// - [`None`]: ordinary directory / missing / unsupported
/// - [`Symlink`]: POSIX symlink or Windows directory/file symlink
/// - [`Junction`]: Windows directory junction (no admin required)
/// - [`Hardlink`]: hard link (rare for skill trees; reserved for completeness)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SkillLinkKind {
    #[default]
    None,
    Symlink,
    Junction,
    Hardlink,
}

impl SkillLinkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Symlink => "symlink",
            Self::Junction => "junction",
            Self::Hardlink => "hardlink",
        }
    }

    pub fn is_link(self) -> bool {
        !matches!(self, Self::None)
    }
}

impl std::fmt::Display for SkillLinkKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per-agent skill projection status relative to the shared source tree.
///
/// - [`Unsupported`]: adapter reports no skills support, or no skills directory.
/// - [`Linked`]: target is a link whose resolved path is the source skill.
/// - [`Copied`]: target is a real directory whose regular-file tree matches source.
/// - [`Absent`]: target skill directory is missing (not projected yet).
/// - [`Foreign`]: target is a link pointing elsewhere, or a real directory whose
///   content differs from source (mapped, but not to this source / not identical).
/// - [`Conflict`]: target exists but cannot be classified safely (file-not-dir,
///   nested unsafe entries, unreadable, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillSyncState {
    Unsupported,
    Linked,
    Copied,
    Absent,
    Foreign,
    Conflict,
}

impl SkillSyncState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Linked => "linked",
            Self::Copied => "copied",
            Self::Absent => "absent",
            Self::Foreign => "foreign",
            Self::Conflict => "conflict",
        }
    }

    /// Whether the agent currently has a usable projection of the source skill.
    pub fn is_mapped(self) -> bool {
        matches!(self, Self::Linked | Self::Copied)
    }
}

impl std::fmt::Display for SkillSyncState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Explicit mappability reason for UI (orthogonal to [`SkillSyncState`]).
///
/// - [`Available`]: can project / already mapped (linked|copied|absent).
/// - [`PrivateSource`]: skill lives only under an agent-private root.
/// - [`AgentUnsupported`]: adapter does not support a skills directory (e.g. Kimi).
/// - [`AgentNotInstalled`]: reserved for UI that knows agent detect status.
/// - [`TargetUnavailable`]: agent supports skills but skills root is missing/unusable.
/// - [`Conflict`]: target exists with different content — still projectable with force.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkillMapStatus {
    #[default]
    Available,
    PrivateSource,
    AgentUnsupported,
    AgentNotInstalled,
    TargetUnavailable,
    Conflict,
}

impl SkillMapStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::PrivateSource => "private_source",
            Self::AgentUnsupported => "agent_unsupported",
            Self::AgentNotInstalled => "agent_not_installed",
            Self::TargetUnavailable => "target_unavailable",
            Self::Conflict => "conflict",
        }
    }

    /// True when the cell may accept a project action (possibly with force).
    pub fn is_actionable(self) -> bool {
        matches!(self, Self::Available | Self::Conflict)
    }
}

impl std::fmt::Display for SkillMapStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One agent's projection row for a skill (stable order follows [`AgentId::ALL`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillProjection {
    pub agent: AgentId,
    pub state: SkillSyncState,
    /// How the target is linked, if at all.
    pub link_kind: SkillLinkKind,
    /// Absolute target skill directory when the adapter supports skills and
    /// reports a skills root; `None` for unsupported agents.
    pub target_dir: Option<PathBuf>,
    /// Absolute path after resolving a link (junction/symlink); `None` when the
    /// target is missing, is a real directory, or resolution failed.
    pub resolved_target: Option<PathBuf>,
    /// Refined mappability reason for UI (backend-derived; not guessed by frontend).
    #[serde(default)]
    pub map_status: SkillMapStatus,
}

/// A skill discovered under the shared source root (`~/.agents/skills/<id>/`).
///
/// `id` is the immediate child directory name (stable key for CLI/GUI).
/// `name` / `description` come from optional `SKILL.md` frontmatter, with safe
/// fallbacks when metadata is missing or malformed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    /// Stable skill id — directory basename under the source root.
    pub id: String,
    pub name: String,
    pub description: String,
    /// Absolute path to the source skill directory.
    pub source_dir: PathBuf,
    /// Per-agent projection matrix in [`AgentId::ALL`] order.
    pub projections: Vec<SkillProjection>,
}

impl Skill {
    /// Projection state for a specific agent (if present in the matrix).
    pub fn state_for(&self, agent: AgentId) -> Option<SkillSyncState> {
        self.projections
            .iter()
            .find(|p| p.agent == agent)
            .map(|p| p.state)
    }

    /// Full projection row for a specific agent.
    pub fn projection_for(&self, agent: AgentId) -> Option<&SkillProjection> {
        self.projections.iter().find(|p| p.agent == agent)
    }
}

/// How a skill was installed into a skill root (`.skill-lock.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSourceRecord {
    /// Origin kind: `local` | `git` | `zip` | `market` | `unknown`.
    pub kind: String,
    /// Market id, git URL, or original local/zip path.
    pub locator: String,
    pub version: Option<String>,
    /// ISO-8601 install time.
    pub installed_at: String,
    /// Last successful update time, if any.
    pub updated_at: Option<String>,
}

/// Projection mode requested by the user (link vs copy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillProjectMode {
    Link,
    Copy,
}

impl SkillProjectMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Copy => "copy",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "link" => Some(Self::Link),
            "copy" => Some(Self::Copy),
            _ => None,
        }
    }
}

/// Result of projecting a skill onto one agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillProjectResult {
    pub skill_id: String,
    pub agent: AgentId,
    /// Requested mode (`link` / `copy`).
    pub requested_mode: SkillProjectMode,
    /// Actual materialization used after fallbacks.
    pub applied_link_kind: SkillLinkKind,
    /// True when the operation fell back (e.g. junction → symlink → copy).
    pub fell_back: bool,
    pub target_dir: PathBuf,
}

/// Where an installed skill lives (source root vs agent-private root).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source_dir: PathBuf,
    /// Short label for the skill root, e.g. `~/.agents/skills`.
    pub root_label: String,
    /// Absolute skill root path.
    pub root_dir: PathBuf,
    /// `shared` for true source; otherwise the owning agent id.
    pub origin: String,
    /// Whether this skill can be projected to other agents (only shared source).
    pub projectable: bool,
    /// Skill-level mappability: `available` for shared, `private_source` for private.
    #[serde(default)]
    pub map_status: SkillMapStatus,
    pub source: Option<SkillSourceRecord>,
    pub projections: Vec<SkillProjection>,
}

/// Read-only preview of a skill's `SKILL.md` body for GUI rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMarkdownPreview {
    pub skill_id: String,
    pub name: String,
    /// Absolute path to the `SKILL.md` file that was read.
    pub path: PathBuf,
    /// Markdown body (may be truncated when over the size cap).
    pub content: String,
    /// True when `content` was cut to the preview character limit.
    pub truncated: bool,
}

/// Market listing (local catalog / remote markets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillListing {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub provider_id: String,
    pub installed: bool,
    /// Public web detail page (skills.sh / skillhub.cn). Open in browser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail_url: Option<String>,
}

/// Payload a market provider returns for local install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillLocalPayload {
    /// Absolute path to a skill directory containing `SKILL.md`.
    pub path: PathBuf,
    pub version: Option<String>,
    pub source_locator: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_sync_state_serde_lowercase() {
        for state in [
            SkillSyncState::Unsupported,
            SkillSyncState::Linked,
            SkillSyncState::Copied,
            SkillSyncState::Absent,
            SkillSyncState::Foreign,
            SkillSyncState::Conflict,
        ] {
            let json = serde_json::to_string(&state).unwrap();
            assert_eq!(json, format!("\"{}\"", state.as_str()));
            let back: SkillSyncState = serde_json::from_str(&json).unwrap();
            assert_eq!(back, state);
        }
    }

    #[test]
    fn skill_link_kind_serde_lowercase() {
        for kind in [
            SkillLinkKind::None,
            SkillLinkKind::Symlink,
            SkillLinkKind::Junction,
            SkillLinkKind::Hardlink,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{}\"", kind.as_str()));
            let back: SkillLinkKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, kind);
        }
    }

    #[test]
    fn skill_map_status_serde_snake_case() {
        for status in [
            SkillMapStatus::Available,
            SkillMapStatus::PrivateSource,
            SkillMapStatus::AgentUnsupported,
            SkillMapStatus::AgentNotInstalled,
            SkillMapStatus::TargetUnavailable,
            SkillMapStatus::Conflict,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json, format!("\"{}\"", status.as_str()));
            let back: SkillMapStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, status);
        }
        assert!(SkillMapStatus::Available.is_actionable());
        assert!(SkillMapStatus::Conflict.is_actionable());
        assert!(!SkillMapStatus::PrivateSource.is_actionable());
        assert!(!SkillMapStatus::AgentUnsupported.is_actionable());
    }

    #[test]
    fn skill_serde_camel_case() {
        let skill = Skill {
            id: "dbs-diagnosis".into(),
            name: "Diagnosis".into(),
            description: "Business model diagnosis".into(),
            source_dir: PathBuf::from(r"D:\tmp\skills\dbs-diagnosis"),
            projections: vec![
                SkillProjection {
                    agent: AgentId::Claude,
                    state: SkillSyncState::Linked,
                    link_kind: SkillLinkKind::Junction,
                    target_dir: Some(PathBuf::from(r"D:\tmp\.claude\skills\dbs-diagnosis")),
                    resolved_target: Some(PathBuf::from(r"D:\tmp\skills\dbs-diagnosis")),
                    map_status: SkillMapStatus::Available,
                },
                SkillProjection {
                    agent: AgentId::Codex,
                    state: SkillSyncState::Absent,
                    link_kind: SkillLinkKind::None,
                    target_dir: Some(PathBuf::from(r"D:\tmp\.codex\skills\dbs-diagnosis")),
                    resolved_target: None,
                    map_status: SkillMapStatus::Available,
                },
                SkillProjection {
                    agent: AgentId::Kimi,
                    state: SkillSyncState::Unsupported,
                    link_kind: SkillLinkKind::None,
                    target_dir: None,
                    resolved_target: None,
                    map_status: SkillMapStatus::AgentUnsupported,
                },
                SkillProjection {
                    agent: AgentId::Grok,
                    state: SkillSyncState::Foreign,
                    link_kind: SkillLinkKind::None,
                    target_dir: Some(PathBuf::from(r"D:\tmp\.grok\skills\dbs-diagnosis")),
                    resolved_target: None,
                    map_status: SkillMapStatus::Conflict,
                },
            ],
        };
        let v = serde_json::to_value(&skill).unwrap();
        assert_eq!(v["id"], "dbs-diagnosis");
        assert_eq!(v["name"], "Diagnosis");
        assert_eq!(v["description"], "Business model diagnosis");
        assert_eq!(v["sourceDir"], r"D:\tmp\skills\dbs-diagnosis");
        assert_eq!(v["projections"][0]["agent"], "claude");
        assert_eq!(v["projections"][0]["state"], "linked");
        assert_eq!(v["projections"][0]["linkKind"], "junction");
        assert_eq!(v["projections"][0]["mapStatus"], "available");
        assert_eq!(
            v["projections"][0]["resolvedTarget"],
            r"D:\tmp\skills\dbs-diagnosis"
        );
        assert_eq!(v["projections"][1]["state"], "absent");
        assert_eq!(v["projections"][2]["state"], "unsupported");
        assert_eq!(v["projections"][2]["mapStatus"], "agent_unsupported");
        assert_eq!(v["projections"][2]["targetDir"], serde_json::Value::Null);
        assert_eq!(v["projections"][3]["state"], "foreign");
        assert_eq!(v["projections"][3]["mapStatus"], "conflict");
        assert_eq!(
            skill.state_for(AgentId::Grok),
            Some(SkillSyncState::Foreign)
        );
        assert!(SkillSyncState::Linked.is_mapped());
        assert!(SkillSyncState::Copied.is_mapped());
        assert!(!SkillSyncState::Foreign.is_mapped());
    }
}
