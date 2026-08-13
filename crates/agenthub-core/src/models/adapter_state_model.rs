//! Adapter state model: durable profile vs runtime observed vs host availability.
//!
//! **Not wired into Tauri/GUI DTOs yet.** Pure data + derivation helpers only;
//! do not assume list/status APIs already emit these views.
//!
//! These layers must not be mixed in APIs:
//! - **durable**: SQLite profile lifecycle / restore intent
//! - **observed**: in-process or sidecar listener snapshot
//! - **host derived**: client cannot reach a matching runtime host
//!
//! Aligns with `docs/adapter-sidecar-design.md` §8. This module does not talk to
//! SQLite or start listeners.

use super::{AdapterProfile, AdapterProfileStatus, AdapterRoute};
use serde::{Deserialize, Serialize};

/// Durable profile lifecycle stored in SQLite (or derived for display).
///
/// Extends the persisted [`AdapterProfileStatus`] with draft/removing/removed
/// phases used by sidecar sagas. Existing rows only use applying/active/needs_attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterDurableProfileState {
    Draft,
    Applying,
    Active,
    NeedsAttention,
    Removing,
    Removed,
}

impl AdapterDurableProfileState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Applying => "applying",
            Self::Active => "active",
            Self::NeedsAttention => "needs_attention",
            Self::Removing => "removing",
            Self::Removed => "removed",
        }
    }

    pub fn from_profile_status(status: AdapterProfileStatus) -> Self {
        match status {
            AdapterProfileStatus::Applying => Self::Applying,
            AdapterProfileStatus::Active => Self::Active,
            AdapterProfileStatus::NeedsAttention => Self::NeedsAttention,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "draft" => Some(Self::Draft),
            "applying" => Some(Self::Applying),
            "active" => Some(Self::Active),
            "needs_attention" => Some(Self::NeedsAttention),
            "removing" => Some(Self::Removing),
            "removed" => Some(Self::Removed),
            _ => None,
        }
    }
}

/// Restore intent persisted on the profile. Not an OS login-item flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterRestoreIntent {
    /// Do not auto-restore the listener after host start.
    Manual,
    /// Host/sidecar should attempt restore after start (`auto_start = true`).
    AutoStart,
}

impl AdapterRestoreIntent {
    pub fn from_auto_start(auto_start: bool) -> Self {
        if auto_start {
            Self::AutoStart
        } else {
            Self::Manual
        }
    }

    pub fn as_auto_start(self) -> bool {
        matches!(self, Self::AutoStart)
    }
}

/// Runtime state observed by the current host / sidecar instance.
///
/// Never persist this as the sole truth of “is the bridge running”.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterObservedRuntimeState {
    Unknown,
    Stopped,
    Starting,
    Running,
    Degraded,
    Error,
    /// Drain in progress before stop (sidecar target; maps from stopping).
    Draining,
    Stopping,
}

impl AdapterObservedRuntimeState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Degraded => "degraded",
            Self::Error => "error",
            Self::Draining => "draining",
            Self::Stopping => "stopping",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "unknown" => Some(Self::Unknown),
            "stopped" => Some(Self::Stopped),
            "starting" => Some(Self::Starting),
            "running" => Some(Self::Running),
            "degraded" => Some(Self::Degraded),
            "error" => Some(Self::Error),
            "draining" => Some(Self::Draining),
            "stopping" => Some(Self::Stopping),
            _ => None,
        }
    }

    /// Map from the current in-process bridge host state name.
    pub fn from_bridge_runtime_name(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "starting" => Self::Starting,
            "running" => Self::Running,
            "stopping" => Self::Stopping,
            "stopped" => Self::Stopped,
            "error" => Self::Error,
            "degraded" => Self::Degraded,
            "draining" => Self::Draining,
            _ => Self::Unknown,
        }
    }
}

/// Whether the client can reach a healthy runtime host for this data-dir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterHostAvailability {
    Available,
    /// Control plane unreachable; never invent a running observed state.
    HostUnavailable,
}

impl AdapterHostAvailability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::HostUnavailable => "host_unavailable",
        }
    }
}

/// Unified, credential-free view for UI / CLI status rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterProfileStateView {
    pub profile_id: String,
    pub route: AdapterRoute,
    pub durable: AdapterDurableProfileState,
    pub restore_intent: AdapterRestoreIntent,
    pub host: AdapterHostAvailability,
    /// Present only when the host is available and reported a snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed: Option<AdapterObservedRuntimeState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error_code: Option<String>,
    /// Client-facing aggregate label; may be `host_unavailable`.
    pub display_state: AdapterDisplayState,
}

/// Aggregate display state. Includes host-derived `host_unavailable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterDisplayState {
    Draft,
    Applying,
    Active,
    NeedsAttention,
    Removing,
    Removed,
    Starting,
    Running,
    Degraded,
    Error,
    Draining,
    Stopping,
    Stopped,
    HostUnavailable,
    Unknown,
}

impl AdapterDisplayState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Applying => "applying",
            Self::Active => "active",
            Self::NeedsAttention => "needs_attention",
            Self::Removing => "removing",
            Self::Removed => "removed",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Degraded => "degraded",
            Self::Error => "error",
            Self::Draining => "draining",
            Self::Stopping => "stopping",
            Self::Stopped => "stopped",
            Self::HostUnavailable => "host_unavailable",
            Self::Unknown => "unknown",
        }
    }
}

/// Build a state view from durable profile + optional observed snapshot + host reachability.
pub fn derive_adapter_profile_state(
    profile: &AdapterProfile,
    host: AdapterHostAvailability,
    observed: Option<AdapterObservedRuntimeState>,
) -> AdapterProfileStateView {
    let durable = AdapterDurableProfileState::from_profile_status(profile.status);
    let restore_intent = AdapterRestoreIntent::from_auto_start(profile.auto_start);
    let display_state = derive_display_state(profile.route, durable, host, observed);

    AdapterProfileStateView {
        profile_id: profile.id.clone(),
        route: profile.route,
        durable,
        restore_intent,
        host,
        observed: match host {
            AdapterHostAvailability::Available => observed,
            // Never claim a live observed state when the host is gone.
            AdapterHostAvailability::HostUnavailable => None,
        },
        local_port: profile.local_port,
        last_error_code: profile.last_error_code.clone(),
        display_state,
    }
}

fn derive_display_state(
    route: AdapterRoute,
    durable: AdapterDurableProfileState,
    host: AdapterHostAvailability,
    observed: Option<AdapterObservedRuntimeState>,
) -> AdapterDisplayState {
    if matches!(durable, AdapterDurableProfileState::Removed) {
        return AdapterDisplayState::Removed;
    }
    if matches!(durable, AdapterDurableProfileState::Removing) {
        return AdapterDisplayState::Removing;
    }
    if matches!(durable, AdapterDurableProfileState::Draft) {
        return AdapterDisplayState::Draft;
    }
    if matches!(durable, AdapterDurableProfileState::Applying) {
        return AdapterDisplayState::Applying;
    }
    if matches!(durable, AdapterDurableProfileState::NeedsAttention) {
        return AdapterDisplayState::NeedsAttention;
    }

    // Active durable profile.
    if route == AdapterRoute::LocalBridge
        && matches!(host, AdapterHostAvailability::HostUnavailable)
    {
        return AdapterDisplayState::HostUnavailable;
    }

    match observed {
        Some(AdapterObservedRuntimeState::Starting) => AdapterDisplayState::Starting,
        Some(AdapterObservedRuntimeState::Running) => AdapterDisplayState::Running,
        Some(AdapterObservedRuntimeState::Degraded) => AdapterDisplayState::Degraded,
        Some(AdapterObservedRuntimeState::Error) => AdapterDisplayState::Error,
        Some(AdapterObservedRuntimeState::Draining) => AdapterDisplayState::Draining,
        Some(AdapterObservedRuntimeState::Stopping) => AdapterDisplayState::Stopping,
        Some(AdapterObservedRuntimeState::Stopped) => AdapterDisplayState::Stopped,
        Some(AdapterObservedRuntimeState::Unknown) | None => {
            if route == AdapterRoute::LocalBridge {
                AdapterDisplayState::Stopped
            } else {
                AdapterDisplayState::Active
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AdapterSourceKind, AgentId};

    fn sample_bridge_profile(status: AdapterProfileStatus, auto_start: bool) -> AdapterProfile {
        AdapterProfile {
            id: "profile-1".into(),
            name: "Kimi → Codex".into(),
            source_kind: AdapterSourceKind::Provider,
            source_id: "src".into(),
            target_agent_id: AgentId::Codex,
            route: AdapterRoute::LocalBridge,
            status,
            rule_id: "kimi-membership-to-codex-v1".into(),
            rule_version: "1".into(),
            generated_provider_id: Some("prov".into()),
            local_port: Some(43121),
            auto_start,
            last_error_code: None,
            created_at: "now".into(),
            updated_at: "now".into(),
        }
    }

    #[test]
    fn host_unavailable_overrides_stale_running_for_local_bridge() {
        let profile = sample_bridge_profile(AdapterProfileStatus::Active, true);
        let view = derive_adapter_profile_state(
            &profile,
            AdapterHostAvailability::HostUnavailable,
            // Even if a client cached "running", host unavailability wins.
            Some(AdapterObservedRuntimeState::Running),
        );
        assert_eq!(view.display_state, AdapterDisplayState::HostUnavailable);
        assert_eq!(view.host, AdapterHostAvailability::HostUnavailable);
        assert!(view.observed.is_none());
        assert_eq!(view.restore_intent, AdapterRestoreIntent::AutoStart);
        assert_eq!(view.durable, AdapterDurableProfileState::Active);
    }

    #[test]
    fn observed_running_when_host_available() {
        let profile = sample_bridge_profile(AdapterProfileStatus::Active, false);
        let view = derive_adapter_profile_state(
            &profile,
            AdapterHostAvailability::Available,
            Some(AdapterObservedRuntimeState::Running),
        );
        assert_eq!(view.display_state, AdapterDisplayState::Running);
        assert_eq!(view.observed, Some(AdapterObservedRuntimeState::Running));
        assert_eq!(view.restore_intent, AdapterRestoreIntent::Manual);
    }

    #[test]
    fn needs_attention_beats_observed_running() {
        let profile = sample_bridge_profile(AdapterProfileStatus::NeedsAttention, true);
        let view = derive_adapter_profile_state(
            &profile,
            AdapterHostAvailability::Available,
            Some(AdapterObservedRuntimeState::Running),
        );
        assert_eq!(view.display_state, AdapterDisplayState::NeedsAttention);
    }

    #[test]
    fn non_bridge_active_profile_is_active_without_observed() {
        let mut profile = sample_bridge_profile(AdapterProfileStatus::Active, false);
        profile.route = AdapterRoute::NativeEndpoint;
        profile.target_agent_id = AgentId::Claude;
        profile.local_port = None;
        let view =
            derive_adapter_profile_state(&profile, AdapterHostAvailability::HostUnavailable, None);
        // Direct routes do not depend on the bridge host.
        assert_eq!(view.display_state, AdapterDisplayState::Active);
    }

    #[test]
    fn profile_status_maps_into_durable_states() {
        assert_eq!(
            AdapterDurableProfileState::from_profile_status(AdapterProfileStatus::Applying),
            AdapterDurableProfileState::Applying
        );
        assert_eq!(
            AdapterDurableProfileState::from_profile_status(AdapterProfileStatus::Active),
            AdapterDurableProfileState::Active
        );
        assert_eq!(
            AdapterDurableProfileState::from_profile_status(AdapterProfileStatus::NeedsAttention),
            AdapterDurableProfileState::NeedsAttention
        );
    }
}
