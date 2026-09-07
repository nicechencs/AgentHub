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
        mode: crate::models::AdapterProfileMode::Api,
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
