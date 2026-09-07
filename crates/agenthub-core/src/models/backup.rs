//! Backup models aligned with the `backups` table and frontend `BackupMeta`.
//!
//! Live snapshot, restore, and delete are orchestrated by `BackupService`.
//! `BackupKind::PreRestore` marks the automatic re-snapshot of current live
//! files taken immediately before a restore overwrites them.

use serde::{Deserialize, Serialize};

use super::AgentId;

/// Why a live (or future self-data) backup was taken.
///
/// Wire format uses kebab-case strings matching the product docs and UI:
/// `manual` | `auto-switch` | `pre-uninstall` | `pre-restore` | `pre-skill-uninstall`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackupKind {
    /// Backups page / Dashboard "backup now".
    Manual,
    /// Provider / account switch — snapshot live files before write.
    AutoSwitch,
    /// Before uninstall (especially when deleting config).
    PreUninstall,
    /// Before restore — re-snapshot current live so restore is reversible.
    PreRestore,
    /// Before removing a skill from the shared source root.
    PreSkillUninstall,
}

impl BackupKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::AutoSwitch => "auto-switch",
            Self::PreUninstall => "pre-uninstall",
            Self::PreRestore => "pre-restore",
            Self::PreSkillUninstall => "pre-skill-uninstall",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "manual" => Some(Self::Manual),
            "auto-switch" => Some(Self::AutoSwitch),
            "pre-uninstall" => Some(Self::PreUninstall),
            "pre-restore" => Some(Self::PreRestore),
            "pre-skill-uninstall" => Some(Self::PreSkillUninstall),
            _ => None,
        }
    }
}

impl std::fmt::Display for BackupKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Indexed live backup record (`backups` table row).
///
/// `path` is the snapshot directory (absolute). `files` lists destination
/// basenames inside that directory (never absolute source paths).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRecord {
    pub id: String,
    /// Live agent backups always set this; schema allows NULL for future db-only rows.
    pub agent_id: Option<AgentId>,
    pub kind: BackupKind,
    pub path: String,
    pub files: Vec<String>,
    /// Total size of copied files in bytes.
    pub size: u64,
    pub note: Option<String>,
    pub created_at: String,
}

/// One non-secret fact extracted from a backup file (UI translates `key`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFact {
    pub key: String,
    pub value: String,
}

/// Preview of one file inside a snapshot directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFileView {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Absolute snapshot path; GUI uses it to reveal the file.
    pub path: String,
    pub size: u64,
    /// UTF-8 text as stored. `None` when the file is not text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<BackupFact>,
}

/// Full backup inspect payload for the details pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInspect {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
    pub kind: BackupKind,
    pub created_at: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<BackupFact>,
    pub files: Vec<BackupFileView>,
}

/// List row with a short identity so cards can tell backups apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupListItem {
    #[serde(flatten)]
    pub record: BackupRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
}

#[cfg(test)]
mod tests;
