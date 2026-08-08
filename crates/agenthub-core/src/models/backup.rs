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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_kind_serde_and_parse_roundtrip() {
        for kind in [
            BackupKind::Manual,
            BackupKind::AutoSwitch,
            BackupKind::PreUninstall,
            BackupKind::PreRestore,
            BackupKind::PreSkillUninstall,
        ] {
            let s = kind.as_str();
            assert_eq!(BackupKind::parse(s), Some(kind));
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{s}\""));
            let back: BackupKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, kind);
        }
        assert_eq!(BackupKind::parse("unknown"), None);
        assert_eq!(BackupKind::parse(""), None);
    }

    #[test]
    fn backup_record_serde_camel_case() {
        let rec = BackupRecord {
            id: "bk-1".into(),
            agent_id: Some(AgentId::Claude),
            kind: BackupKind::AutoSwitch,
            path: r"D:\tmp\backups\live\claude\bk-1".into(),
            files: vec!["settings.json".into(), "auth.json".into()],
            size: 42,
            note: Some("before switch".into()),
            created_at: "2026-07-01T12:00:00Z".into(),
        };
        let v = serde_json::to_value(&rec).unwrap();
        assert_eq!(v["id"], "bk-1");
        assert_eq!(v["agentId"], "claude");
        assert_eq!(v["kind"], "auto-switch");
        assert_eq!(v["path"], r"D:\tmp\backups\live\claude\bk-1");
        assert_eq!(v["files"][0], "settings.json");
        assert_eq!(v["size"], 42);
        assert_eq!(v["note"], "before switch");
        assert_eq!(v["createdAt"], "2026-07-01T12:00:00Z");
    }
}
