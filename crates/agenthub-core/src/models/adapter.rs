//! Read-only adapter route and configuration-plan DTOs.

use serde::{Deserialize, Serialize};

use super::AgentId;

/// Persisted connection table selected by an adapter analysis request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterSourceKind {
    Account,
    Provider,
}

impl AdapterSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Provider => "provider",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "account" => Some(Self::Account),
            "provider" => Some(Self::Provider),
            _ => None,
        }
    }
}

/// Input to the read-only route analysis service. `source_id` is always a DB id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterRouteRequest {
    pub source_kind: AdapterSourceKind,
    pub source_id: String,
    pub target_agent_id: AgentId,
}

/// The only routes a compatibility rule can return.
///
/// Keeping this closed prevents UI callers from treating an unknown route as
/// safe to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterRoute {
    ConfigSync,
    NativeEndpoint,
    LocalBridge,
    Unsupported,
}

impl AdapterRoute {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConfigSync => "config_sync",
            Self::NativeEndpoint => "native_endpoint",
            Self::LocalBridge => "local_bridge",
            Self::Unsupported => "unsupported",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "config_sync" => Some(Self::ConfigSync),
            "native_endpoint" => Some(Self::NativeEndpoint),
            "local_bridge" => Some(Self::LocalBridge),
            "unsupported" => Some(Self::Unsupported),
            _ => None,
        }
    }

    pub fn is_profile_supported(self) -> bool {
        !matches!(self, Self::Unsupported)
    }
}

/// Lifecycle state for a persisted adapter profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterProfileStatus {
    Applying,
    Active,
    NeedsAttention,
}

impl AdapterProfileStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Applying => "applying",
            Self::Active => "active",
            Self::NeedsAttention => "needs_attention",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "applying" => Some(Self::Applying),
            "active" => Some(Self::Active),
            "needs_attention" => Some(Self::NeedsAttention),
            _ => None,
        }
    }
}

/// Persisted, credential-free record of an adapter configuration projection.
///
/// The profile only identifies its source and any generated provider; it never
/// stores source credentials or generated configuration content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterProfile {
    pub id: String,
    pub name: String,
    pub source_kind: AdapterSourceKind,
    pub source_id: String,
    pub target_agent_id: AgentId,
    pub route: AdapterRoute,
    pub status: AdapterProfileStatus,
    pub rule_id: String,
    pub rule_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_provider_id: Option<String>,
    /// Actual loopback port for a local bridge. This is intentionally absent
    /// until the runtime has successfully bound its listener.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_port: Option<u16>,
    /// Whether a local bridge profile should be restored by its desktop host.
    /// The value is persisted even while a profile is waiting for recovery so
    /// the host can make a deterministic startup decision.
    pub auto_start: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Optional, typed filters for persisted adapter profiles.
///
/// This is deliberately a pure DTO: repository implementations remain the
/// only place that turns it into SQL, keeping GUI/Tauri callers from building
/// their own filter semantics.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterProfileFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<AdapterSourceKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_agent_id: Option<AgentId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<AdapterRoute>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<AdapterProfileStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_start: Option<bool>,
}

/// Confidence and availability of an adapter rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterSupport {
    Stable,
    Experimental,
    Unsupported,
}

/// Structured presentation / gate class for analyze UI (not a write authorization).
///
/// UI must prefer this over parsing `reason` text. Write permission remains
/// `AdapterApplyPlan.can_apply` only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterGateKind {
    /// Applicable or ordinary preview with no special gate chrome.
    #[default]
    None,
    /// Stable/experimental rule that is intentionally preview-only.
    PreviewOnly,
    /// Closed experimental subscription-bridge candidate (e.g. Codex OAuth → Claude).
    SubscriptionCandidate,
    /// Generic missing / unsupported combination.
    Unsupported,
}

impl AdapterGateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::PreviewOnly => "preview_only",
            Self::SubscriptionCandidate => "subscription_candidate",
            Self::Unsupported => "unsupported",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "preview_only" => Some(Self::PreviewOnly),
            "subscription_candidate" => Some(Self::SubscriptionCandidate),
            "unsupported" => Some(Self::Unsupported),
            _ => None,
        }
    }
}

/// A safe, structured description of one required future action.
///
/// `secret = true` is a reference to the selected Connection only. Such an
/// action must never carry `value`, so neither analysis nor plan can leak a
/// credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterAction {
    pub kind: String,
    pub target: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub secret: bool,
}

/// Verifiable official source for a compatibility rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterEvidence {
    pub label: String,
    pub url: String,
    pub verified_at: String,
}

/// Safe route preview. This intentionally contains no credentials or raw configs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterRouteAnalysis {
    pub route: AdapterRoute,
    pub support: AdapterSupport,
    pub reason: String,
    pub actions: Vec<AdapterAction>,
    pub limitations: Vec<String>,
    pub evidence: Vec<AdapterEvidence>,
    /// Capability-matrix rule id when a cell matched; never a secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    /// Structured gate class for UI; prefer over parsing `reason`.
    #[serde(default)]
    pub gate_kind: AdapterGateKind,
}

/// Whether an eventual apply would need a local runtime service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterServiceImpact {
    None,
    RequiresLocalBridge,
}

/// A single configuration field that an eventual apply would write.
///
/// `secret = true` intentionally has no `value`: it says to reference the
/// selected Connection when apply is introduced, not to expose the secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterPlanChange {
    pub target: String,
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub secret: bool,
}

/// Safe apply preview.
///
/// `can_apply` is true only when **both** hold:
/// 1. the capability matrix cell is open (`can_apply` + all gates), and
/// 2. a concrete apply implementation exists (provider-source whitelist).
/// The matrix alone never authorizes writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterApplyPlan {
    pub analysis: AdapterRouteAnalysis,
    pub target_agent_id: AgentId,
    pub can_apply: bool,
    pub service_impact: AdapterServiceImpact,
    pub changes: Vec<AdapterPlanChange>,
}

/// Write request for a supported adapter route.  Unlike [`AdapterApplyPlan`],
/// this is executed only by the narrow apply service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterApplyRequest {
    pub source_kind: AdapterSourceKind,
    pub source_id: String,
    pub target_agent_id: AgentId,
}

/// Persisted outcome of applying an adapter route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterApplyResult {
    pub profile: AdapterProfile,
    /// The generated provider is safe to return: it contains a reference
    /// marker, never the source secret.
    pub provider: super::Provider,
}
