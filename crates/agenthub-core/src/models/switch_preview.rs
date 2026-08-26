//! Read-only switch confirmation preview (no snapshot, lock, or live write).

use std::path::PathBuf;

use super::AgentId;

/// Whether the preview is for an account or provider switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchConfirmKind {
    Account,
    Provider,
}

impl SwitchConfirmKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Provider => "provider",
        }
    }
}

/// Facts the CLI (and later GUI) need to confirm a switch. Not a live write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchConfirmPreview {
    pub agent: AgentId,
    pub target: String,
    pub kind: SwitchConfirmKind,
    pub current_label: Option<String>,
    pub backup_dir: PathBuf,
}

impl SwitchConfirmPreview {
    /// Exact CLI confirm text. Wording is user-visible and snapshot-tested.
    pub fn cli_prompt(&self) -> String {
        let kind = self.kind.as_str();
        let backfill = match &self.current_label {
            Some(label) => format!("backfill: current live will be saved as 「{label}」"),
            None => format!("backfill: no current {kind}; live will be written directly"),
        };
        format!(
            "Switch {} to {kind} {}?\n  {backfill}\n  backup: {}\n  process: running agent processes are not stopped",
            self.agent.as_str(),
            self.target,
            self.backup_dir.display(),
        )
    }
}
