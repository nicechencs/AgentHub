//! Agent capability matrix — single source of truth for "can this agent do X?".
//!
//! See `docs/capability-matrix.md`. Adapters declare via `AgentAdapter::capability`;
//! callers gate with `AdapterRegistry::require`. Exhaustive `match` (no `_ =>`)
//! so new capability variants fail compilation until every adapter answers.

use serde::{Deserialize, Serialize};

/// Capability key. Adding a variant forces every adapter to recompile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    // —— existing call sites ——
    ConfigWrite,
    AccountSwitch,
    ApiKeyAccount,
    Skills,
    LiveBackup,
    StructuredStream,
    DangerousMode,
    ProjectHistory,
    ProjectDelete,
    ProviderPresets,
    // —— reserved (no call sites yet; see docs §6) ——
    Usage,
    Mcp,
    ModelSelect,
    SessionResume,
}

impl Capability {
    pub const ALL: [Capability; 14] = [
        Self::ConfigWrite,
        Self::AccountSwitch,
        Self::ApiKeyAccount,
        Self::Skills,
        Self::LiveBackup,
        Self::StructuredStream,
        Self::DangerousMode,
        Self::ProjectHistory,
        Self::ProjectDelete,
        Self::ProviderPresets,
        Self::Usage,
        Self::Mcp,
        Self::ModelSelect,
        Self::SessionResume,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConfigWrite => "configWrite",
            Self::AccountSwitch => "accountSwitch",
            Self::ApiKeyAccount => "apiKeyAccount",
            Self::Skills => "skills",
            Self::LiveBackup => "liveBackup",
            Self::StructuredStream => "structuredStream",
            Self::DangerousMode => "dangerousMode",
            Self::ProjectHistory => "projectHistory",
            Self::ProjectDelete => "projectDelete",
            Self::ProviderPresets => "providerPresets",
            Self::Usage => "usage",
            Self::Mcp => "mcp",
            Self::ModelSelect => "modelSelect",
            Self::SessionResume => "sessionResume",
        }
    }

    /// Test-fake helper. The match is exhaustive so a new variant fails here
    /// instead of being swallowed by `_ => unsupported` in each fake adapter.
    pub fn fake_state(self, supported: &[Self]) -> CapabilityState {
        match self {
            Self::ConfigWrite
            | Self::AccountSwitch
            | Self::ApiKeyAccount
            | Self::Skills
            | Self::LiveBackup
            | Self::StructuredStream
            | Self::DangerousMode
            | Self::ProjectHistory
            | Self::ProjectDelete
            | Self::ProviderPresets
            | Self::Usage
            | Self::Mcp
            | Self::ModelSelect
            | Self::SessionResume => {
                if supported.contains(&self) {
                    CapabilityState::full()
                } else {
                    CapabilityState::unsupported("fake")
                }
            }
        }
    }

    /// Short Chinese label for error messages / CLI.
    pub fn label(self) -> &'static str {
        match self {
            Self::ConfigWrite => "配置写入",
            Self::AccountSwitch => "账号切换",
            Self::ApiKeyAccount => "API Key 账号",
            Self::Skills => "技能",
            Self::LiveBackup => "Live 备份",
            Self::StructuredStream => "结构化流式输出",
            Self::DangerousMode => "危险模式",
            Self::ProjectHistory => "项目历史",
            Self::ProjectDelete => "项目删除",
            Self::ProviderPresets => "供应商预设",
            Self::Usage => "用量统计",
            Self::Mcp => "MCP",
            Self::ModelSelect => "模型选择",
            Self::SessionResume => "会话续接",
        }
    }
}

/// Four-level support, not a boolean — see docs §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityLevel {
    /// Fully wired.
    Full,
    /// Usable with degradation; callers may proceed but must surface `reason`.
    Partial,
    /// Target CLI cannot do this (or no stable contract). Permanent boundary.
    Unsupported,
    /// CLI may support it; AgentHub has not wired it yet. Roadmap cell.
    Planned,
}

/// Declared capability state for one (agent, capability) cell.
///
/// Serialize-only: `reason` / `min_version` are `&'static str` compile-time
/// facts. API/doctor responses convert via [`CapabilityStateDto`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CapabilityState {
    pub level: CapabilityLevel,
    /// Why degraded / unsupported / planned. Compile-time fact for UI/CLI copy.
    pub reason: Option<&'static str>,
    /// Reserved for CLI version gates. Currently always `None`.
    pub min_version: Option<&'static str>,
}

/// Owned capability state for JSON wire format (doctor / GUI).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityStateDto {
    pub level: CapabilityLevel,
    pub reason: Option<String>,
    pub min_version: Option<String>,
}

impl From<CapabilityState> for CapabilityStateDto {
    fn from(value: CapabilityState) -> Self {
        Self {
            level: value.level,
            reason: value.reason.map(str::to_string),
            min_version: value.min_version.map(str::to_string),
        }
    }
}

impl CapabilityState {
    pub const fn full() -> Self {
        Self {
            level: CapabilityLevel::Full,
            reason: None,
            min_version: None,
        }
    }

    pub const fn partial(reason: &'static str) -> Self {
        Self {
            level: CapabilityLevel::Partial,
            reason: Some(reason),
            min_version: None,
        }
    }

    pub const fn unsupported(reason: &'static str) -> Self {
        Self {
            level: CapabilityLevel::Unsupported,
            reason: Some(reason),
            min_version: None,
        }
    }

    pub const fn planned(reason: &'static str) -> Self {
        Self {
            level: CapabilityLevel::Planned,
            reason: Some(reason),
            min_version: None,
        }
    }

    /// `Unsupported` or `Planned` — callers should refuse.
    pub fn is_blocked(self) -> bool {
        matches!(
            self.level,
            CapabilityLevel::Unsupported | CapabilityLevel::Planned
        )
    }

    /// `Full` or `Partial` — callers may proceed.
    pub fn is_usable(self) -> bool {
        matches!(self.level, CapabilityLevel::Full | CapabilityLevel::Partial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_covers_every_variant() {
        assert_eq!(Capability::ALL.len(), 14);
        // Exhaustive: adding a variant without updating ALL fails this match.
        for cap in Capability::ALL {
            let _ = match cap {
                Capability::ConfigWrite
                | Capability::AccountSwitch
                | Capability::ApiKeyAccount
                | Capability::Skills
                | Capability::LiveBackup
                | Capability::StructuredStream
                | Capability::DangerousMode
                | Capability::ProjectHistory
                | Capability::ProjectDelete
                | Capability::ProviderPresets
                | Capability::Usage
                | Capability::Mcp
                | Capability::ModelSelect
                | Capability::SessionResume => cap.as_str(),
            };
        }
    }

    #[test]
    fn blocked_and_usable_partition() {
        assert!(CapabilityState::full().is_usable());
        assert!(!CapabilityState::full().is_blocked());
        assert!(CapabilityState::partial("x").is_usable());
        assert!(!CapabilityState::partial("x").is_blocked());
        assert!(CapabilityState::unsupported("x").is_blocked());
        assert!(!CapabilityState::unsupported("x").is_usable());
        assert!(CapabilityState::planned("x").is_blocked());
        assert!(!CapabilityState::planned("x").is_usable());
    }

    #[test]
    fn serde_camel_case() {
        let json = serde_json::to_string(&Capability::AccountSwitch).unwrap();
        assert_eq!(json, "\"accountSwitch\"");
        let level = serde_json::to_string(&CapabilityLevel::Unsupported).unwrap();
        assert_eq!(level, "\"unsupported\"");
    }

    #[test]
    fn dto_roundtrip_owns_reason_strings() {
        let state = CapabilityState::partial("降级说明");
        let dto = CapabilityStateDto::from(state);
        assert_eq!(dto.level, CapabilityLevel::Partial);
        assert_eq!(dto.reason.as_deref(), Some("降级说明"));
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["level"], "partial");
        assert_eq!(json["reason"], "降级说明");
        let back: CapabilityStateDto = serde_json::from_value(json).unwrap();
        assert_eq!(back, dto);
    }

    #[test]
    fn label_covers_all_capabilities() {
        for cap in Capability::ALL {
            assert!(!cap.label().is_empty(), "{cap:?}");
            assert!(!cap.as_str().is_empty(), "{cap:?}");
        }
    }
}
