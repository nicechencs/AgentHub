use std::sync::Arc;

use crate::models::{
    ticket_id, AdapterRoute, AdapterSourceKind, AgentId, Provider, TicketPlanRequest,
};
use crate::storage::{AdapterProfileRepo, ProviderRepo};
use serde_json::json;

use super::{
    resolve_bind_action, resolve_unbind_action, surface_unbind_and_restart, AdapterSagaCoordinator,
    BindAction,
};

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

#[tokio::test]
async fn lock_profile_recycles_unused_entries() {
    let coordinator = AdapterSagaCoordinator::new();
    for i in 0..100 {
        drop(coordinator.lock_profile(&format!("profile-{i}")).await);
    }
    let _held = coordinator.lock_profile("recycle-trigger").await;
    assert_eq!(coordinator.profile_lock_count(), 1);
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

#[test]
fn resolve_bind_action_rejects_unknown_custom_relay_without_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let hub = crate::AgentHub::open(Some(dir.path())).unwrap();
    ProviderRepo::new(hub.db.clone())
        .create(&Provider {
            id: "relay-source".into(),
            agent_id: AgentId::Claude,
            name: "Custom relay".into(),
            settings_config: json!({
                "apiKey": "relay-secret",
                "baseUrl": "https://relay.example/v1"
            }),
            meta: json!({"preset": "openai-compatible"}),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    ProviderRepo::new(hub.db.clone())
        .create(&Provider {
            id: "relay-source-pi".into(),
            agent_id: AgentId::Claude,
            name: "Custom relay reshape".into(),
            settings_config: json!({
                "apiKey": "relay-secret",
                "baseUrl": "https://relay.example/v1"
            }),
            meta: json!({"preset": "openai-compatible"}),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();

    let ticket = ticket_id(AdapterSourceKind::Provider, "relay-source");
    let plan = hub
        .tickets
        .plan(&TicketPlanRequest {
            ticket_id: ticket.clone(),
            target_agent_id: AgentId::Codex,
        })
        .unwrap();
    assert!(plan.can_apply);
    assert_eq!(plan.analysis.route, AdapterRoute::LocalBridge);

    let providers = ProviderRepo::new(hub.db.clone());
    let profiles = AdapterProfileRepo::new(hub.db.clone());
    let before_provider = providers.get_by_id("relay-source").unwrap().unwrap();
    let before_provider_count = providers.list(None).unwrap().len();
    let before_profile_count = profiles.list(None, None, None).unwrap().len();

    let action = resolve_bind_action(&hub, &ticket, AgentId::Codex).unwrap();
    assert!(matches!(action, crate::adapter_control::BindAction::LocalBridge(_)));
    assert_eq!(providers.list(None).unwrap().len(), before_provider_count);
    assert_eq!(
        profiles.list(None, None, None).unwrap().len(),
        before_profile_count
    );
    assert_eq!(
        providers.get_by_id("relay-source").unwrap().unwrap(),
        before_provider
    );

    let before_provider_count = providers.list(None).unwrap().len();
    let before_profile_count = profiles.list(None, None, None).unwrap().len();
    let reshape = resolve_bind_action(
        &hub,
        &ticket_id(AdapterSourceKind::Provider, "relay-source-pi"),
        AgentId::Pi,
    )
    .unwrap();
    assert!(matches!(
        reshape,
        crate::adapter_control::BindAction::Reshape(_)
            | crate::adapter_control::BindAction::LocalBridge(_)
    ));
    assert_eq!(providers.list(None).unwrap().len(), before_provider_count);
    assert_eq!(
        profiles.list(None, None, None).unwrap().len(),
        before_profile_count
    );
}

#[test]
fn resolve_bind_action_keeps_official_openai_and_xai_routes_available() {
    let dir = tempfile::tempdir().unwrap();
    let hub = crate::AgentHub::open(Some(dir.path())).unwrap();
    let providers = ProviderRepo::new(hub.db.clone());
    providers
        .create(&Provider {
            id: "openai-source".into(),
            agent_id: AgentId::Codex,
            name: "OpenAI API".into(),
            settings_config: json!({"api_key": "openai-secret"}),
            meta: json!({"preset": "openai"}),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    providers
        .create(&Provider {
            id: "xai-source".into(),
            agent_id: AgentId::Grok,
            name: "xAI API".into(),
            settings_config: json!({"api_key": "xai-secret"}),
            meta: json!({"preset": "xai"}),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();

    let openai_action = resolve_bind_action(
        &hub,
        &ticket_id(AdapterSourceKind::Provider, "openai-source"),
        AgentId::Codex,
    )
    .unwrap();
    assert!(matches!(openai_action, BindAction::LocalBridge(_)));

    let xai_action = resolve_bind_action(
        &hub,
        &ticket_id(AdapterSourceKind::Provider, "xai-source"),
        AgentId::Pi,
    )
    .unwrap();
    assert!(matches!(xai_action, BindAction::Reshape(_)));
}

#[test]
fn unbind_restart_failure_does_not_look_like_success() {
    let unbind_only = surface_unbind_and_restart("unbind_ticket failed".into(), Ok(()));
    assert!(unbind_only.contains("unbind_ticket"), "{unbind_only}");
    assert!(!unbind_only.is_empty());

    let both = surface_unbind_and_restart(
        "unbind_ticket failed".into(),
        Err("adapter.bridge_start: port in use".into()),
    );
    assert!(both.contains("unbind_ticket"), "{both}");
    assert!(both.contains("adapter.bridge_start"), "{both}");
}
