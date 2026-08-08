use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeId {
    #[serde(rename = "nodejs")]
    NodeJs,
    Npm,
    PowerShell,
    Git,
}

impl RuntimeId {
    pub const ALL: [RuntimeId; 4] = [
        RuntimeId::NodeJs,
        RuntimeId::Npm,
        RuntimeId::PowerShell,
        RuntimeId::Git,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::NodeJs => "nodejs",
            Self::Npm => "npm",
            Self::PowerShell => "powershell",
            Self::Git => "git",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "nodejs" | "node" => Some(Self::NodeJs),
            "npm" => Some(Self::Npm),
            "powershell" | "pwsh" => Some(Self::PowerShell),
            "git" => Some(Self::Git),
            _ => None,
        }
    }
}

impl std::fmt::Display for RuntimeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvStatusKind {
    Ok,
    Missing,
    Outdated,
    BrokenPath,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvStatus {
    pub id: RuntimeId,
    pub status: EnvStatusKind,
    pub version: Option<String>,
    pub path: Option<PathBuf>,
    pub min_required: Option<String>,
    pub remediation: Option<Remediation>,
    /// Human-readable detail lines (e.g. PowerShell 5.1 vs 7 side-by-side).
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Remediation {
    pub kind: String,
    pub command: Option<String>,
    pub url: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvNotReady {
    pub agent: Option<String>,
    pub channel: Option<String>,
    pub missing: Vec<RuntimeId>,
    pub remediations: Vec<Remediation>,
    pub hint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_id_parse_aliases() {
        assert_eq!(RuntimeId::parse("nodejs"), Some(RuntimeId::NodeJs));
        assert_eq!(RuntimeId::parse("node"), Some(RuntimeId::NodeJs));
        assert_eq!(RuntimeId::parse("Node"), Some(RuntimeId::NodeJs));
        assert_eq!(RuntimeId::parse("npm"), Some(RuntimeId::Npm));
        assert_eq!(RuntimeId::parse("  NPM  "), Some(RuntimeId::Npm));
        assert_eq!(RuntimeId::parse("powershell"), Some(RuntimeId::PowerShell));
        assert_eq!(RuntimeId::parse("pwsh"), Some(RuntimeId::PowerShell));
        assert_eq!(RuntimeId::parse("PowerShell"), Some(RuntimeId::PowerShell));
        assert_eq!(RuntimeId::parse("git"), Some(RuntimeId::Git));
        assert_eq!(RuntimeId::parse("  Git  "), Some(RuntimeId::Git));
    }

    #[test]
    fn runtime_id_parse_rejects_invalid() {
        assert_eq!(RuntimeId::parse(""), None);
        assert_eq!(RuntimeId::parse("python"), None);
        assert_eq!(RuntimeId::parse("bash"), None);
        assert_eq!(RuntimeId::parse("node.js"), None);
        assert_eq!(RuntimeId::parse("github"), None);
    }

    #[test]
    fn runtime_id_as_str_roundtrip() {
        for id in RuntimeId::ALL {
            let s = id.as_str();
            assert_eq!(RuntimeId::parse(s), Some(id));
        }
    }
}
