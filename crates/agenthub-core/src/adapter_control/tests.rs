use std::sync::Arc;

use crate::models::AgentId;

use super::{resolve_bind_action, resolve_unbind_action, AdapterSagaCoordinator};

#[tokio::test]
async fn lock_target_serializes_same_agent_and_allows_other_agents() {
    let coordinator = Arc::new(AdapterSagaCoordinator::new());
    let first = coordinator.lock_target(AgentId::Codex).await;
    let waiter = Arc::clone(&coordinator);
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    let join = tokio::spawn(async move {
        let _guard = waiter.lock_target(AgentId::Codex).await;
        let _ = tx.send(());
    });
    tokio::task::yield_now().await;
    assert!(rx.try_recv().is_err(), "same-agent lock must wait");
    drop(first);
    rx.await.expect("waiter should proceed after unlock");
    join.await.expect("waiter task");

    let _claude = coordinator.lock_target(AgentId::Claude).await;
    let _codex = coordinator.lock_target(AgentId::Codex).await;
}

#[tokio::test]
async fn lock_profile_is_independent_per_profile_id() {
    let coordinator = AdapterSagaCoordinator::new();
    let _a = coordinator.lock_profile("profile-a").await;
    let _b = coordinator.lock_profile("profile-b").await;
}

#[test]
fn adapter_control_status_stopped_has_no_secrets() {
    use crate::adapter_control::AdapterBridgeStatus;
    use crate::models::{
        AdapterProfile, AdapterProfileMode, AdapterProfileStatus, AdapterRoute, AdapterSourceKind,
    };

    let profile = AdapterProfile {
        id: "p1".into(),
        name: "bridge".into(),
        source_kind: AdapterSourceKind::Provider,
        source_id: "src".into(),
        target_agent_id: AgentId::Codex,
        route: AdapterRoute::LocalBridge,
        mode: AdapterProfileMode::Api,
        status: AdapterProfileStatus::Active,
        rule_id: "rule".into(),
        rule_version: "1".into(),
        generated_provider_id: Some("gen".into()),
        local_port: Some(43121),
        auto_start: false,
        last_error_code: None,
        created_at: "now".into(),
        updated_at: "now".into(),
    };
    let json = serde_json::to_string(&AdapterBridgeStatus::stopped(&profile)).unwrap();
    assert!(json.contains("stopped"));
    assert!(!json.contains("bearer"));
    assert!(!json.contains("token"));
}

#[test]
fn resolve_unbind_action_without_profile_skips_bridge_stop() {
    let dir = tempfile::tempdir().unwrap();
    let hub = crate::AgentHub::open(Some(dir.path())).unwrap();
    let action = resolve_unbind_action(&hub, "provider:missing", AgentId::Claude).unwrap();
    assert!(action.stop_bridge_profile_id.is_none());
    assert!(action.lock_target.is_none());
    assert_eq!(action.request.agent_id, AgentId::Claude);
}

// Keep resolve_bind_action covered via TicketBindService integration tests;
// this module only asserts the unbind planner stays available without a profile.
