use super::*;

use agenthub_core::bridge::{
    BridgeHostError, BridgeStartSpec, BridgeUpstreamConfig, ResolvedAuth,
};
use agenthub_core::services::AdapterBridgeRuntimeMaterial;
use agenthub_core::storage::AdapterProfileRepo;
use agenthub_core::AgentHub;

fn profile(
    id: &str,
    route: AdapterRoute,
    status: AdapterProfileStatus,
    auto_start: bool,
) -> AdapterProfile {
    AdapterProfile {
        id: id.into(),
        name: "Kimi bridge".into(),
        source_kind: AdapterSourceKind::Provider,
        source_id: "kimi-connection".into(),
        target_agent_id: AgentId::Codex,
        route,
        status,
        rule_id: "kimi-membership-to-codex-bridge-v1".into(),
        rule_version: "1".into(),
        generated_provider_id: Some("generated-provider".into()),
        local_port: Some(43121),
        auto_start,
        last_error_code: None,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn start_spec(profile_id: &str) -> BridgeStartSpec {
    BridgeStartSpec::new(
        profile_id,
        0,
        "local-bearer-that-must-never-serialize",
        BridgeUpstreamConfig {
            base_url: "https://api.kimi.com/coding/v1".into(),
            model: Some("kimi-k2.5".into()),
            source_connection_id: Some("kimi-connection".into()),
            auth: ResolvedAuth::bearer("upstream-bearer-that-must-never-serialize"),
        },
    )
}

#[test]
fn status_dto_never_serializes_local_or_upstream_bearers() {
    tauri::async_runtime::block_on(async {
        let host = BridgeRuntimeHost::new();
        let runtime = host.start(start_spec("profile-status")).await.unwrap();
        let json = serde_json::to_string(&AdapterBridgeStatusDto::from_runtime(runtime)).unwrap();

        assert!(!json.contains("local-bearer-that-must-never-serialize"));
        assert!(!json.contains("upstream-bearer-that-must-never-serialize"));
        assert!(!json.contains("base_url"));
        host.shutdown().await.unwrap();
    });
}

#[test]
fn started_listener_is_compensated_after_apply_stage_failure() {
    tauri::async_runtime::block_on(async {
        let host = BridgeRuntimeHost::new();
        host.start(start_spec("profile-compensate")).await.unwrap();

        compensate_started_bridge(&host, "profile-compensate", true).await;

        assert!(host.status("profile-compensate").unwrap().is_none());
    });
}

#[test]
fn ensure_listener_replaces_conflicting_running_spec() {
    tauri::async_runtime::block_on(async {
        let host = BridgeRuntimeHost::new();
        let first = AdapterBridgeRuntimeMaterial::for_test(
            "profile-rotate",
            Some(0),
            "local-bearer-original-value-xxxxxxx",
            "upstream-bearer-original-value-xxxxx",
        );
        let first_status = ensure_bridge_listener(&host, &first).await.unwrap();
        assert!(first_status.status.running);
        assert!(first_status.owned_by_saga);

        let rotated = AdapterBridgeRuntimeMaterial::for_test(
            "profile-rotate",
            Some(0),
            "local-bearer-rotated-value-xxxxxxxx",
            "upstream-bearer-rotated-value-xxxxxx",
        );
        // Direct host start must reject drift.
        assert!(matches!(
            host.start(rotated.start_spec(None)).await.unwrap_err(),
            BridgeHostError::ConflictingStart
        ));

        let replaced = ensure_bridge_listener(&host, &rotated).await.unwrap();
        assert!(replaced.status.running);
        assert!(replaced.owned_by_saga);
        // Reuse of the same rotated material is not owned by a later saga.
        let reused = ensure_bridge_listener(&host, &rotated).await.unwrap();
        assert!(reused.status.running);
        assert!(!reused.owned_by_saga);

        host.shutdown().await.unwrap();
    });
}

#[test]
fn ensure_listener_rebinds_when_preferred_port_is_busy() {
    tauri::async_runtime::block_on(async {
        let blocker = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let busy_port = blocker.local_addr().unwrap().port();
        let host = BridgeRuntimeHost::new();
        let material = AdapterBridgeRuntimeMaterial::for_test(
            "profile-rebind",
            Some(busy_port),
            "local-bearer-rebind-value-xxxxxxxxx",
            "upstream-bearer-rebind-value-xxxxxxx",
        );

        let ensured = ensure_bridge_listener(&host, &material).await.unwrap();
        assert!(ensured.status.running);
        assert!(ensured.owned_by_saga);
        assert_ne!(
            ensured.status.port, busy_port,
            "listener must rebind away from the occupied preferred port"
        );

        host.shutdown().await.unwrap();
        drop(blocker);
    });
}

#[test]
fn stop_is_idempotent_for_an_already_stopped_bridge() {
    tauri::async_runtime::block_on(async {
        let host = BridgeRuntimeHost::new();
        let profile = profile(
            "profile-stop",
            AdapterRoute::LocalBridge,
            AdapterProfileStatus::Active,
            true,
        );
        host.start(start_spec(&profile.id)).await.unwrap();

        let first = stop_bridge_runtime(&host, &profile).await.unwrap();
        let second = stop_bridge_runtime(&host, &profile).await.unwrap();

        assert_eq!(first.state, BridgeRuntimeState::Stopped);
        assert_eq!(second.state, BridgeRuntimeState::Stopped);
        assert!(!second.running);
    });
}

#[test]
fn apply_always_switches_current_but_manual_start_preserves_user_choice() {
    // Initial apply must promote the generated bridge Connection.
    assert!(should_make_bridge_current(true, false));
    assert!(should_make_bridge_current(true, true));
    // Manual start only refreshes live config when the bridge is already current.
    assert!(should_make_bridge_current(false, true));
    assert!(!should_make_bridge_current(false, false));
}

#[test]
fn restore_filter_only_keeps_active_auto_start_local_bridges() {
    let profiles = vec![
        profile(
            "eligible",
            AdapterRoute::LocalBridge,
            AdapterProfileStatus::Active,
            true,
        ),
        profile(
            "manual",
            AdapterRoute::LocalBridge,
            AdapterProfileStatus::Active,
            false,
        ),
        profile(
            "attention",
            AdapterRoute::LocalBridge,
            AdapterProfileStatus::NeedsAttention,
            true,
        ),
        profile(
            "direct",
            AdapterRoute::NativeEndpoint,
            AdapterProfileStatus::Active,
            true,
        ),
    ];

    assert_eq!(
        restorable_profiles(profiles)
            .into_iter()
            .map(|profile| profile.id)
            .collect::<Vec<_>>(),
        vec!["eligible"]
    );
}

#[test]
fn saga_coordinator_serializes_same_profile_but_not_different_profiles() {
    tauri::async_runtime::block_on(async {
        let coordinator = Arc::new(AdapterBridgeSagaCoordinator::new());
        let first = coordinator.lock_profile("one").await;
        let waiter = Arc::clone(&coordinator);
        let mut pending = tauri::async_runtime::spawn(async move {
            let _guard = waiter.lock_profile("one").await;
            true
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut pending)
                .await
                .is_err()
        );
        drop(first);
        assert!(pending.await.unwrap());
        let other = coordinator.lock_profile("two").await;
        drop(other);
    });
}

#[test]
fn saga_coordinator_serializes_same_target_without_blocking_other_agents() {
    tauri::async_runtime::block_on(async {
        let coordinator = Arc::new(AdapterBridgeSagaCoordinator::new());
        let first = coordinator.lock_target(AgentId::Codex).await;
        let waiter = Arc::clone(&coordinator);
        let mut pending = tauri::async_runtime::spawn(async move {
            let _guard = waiter.lock_target(AgentId::Codex).await;
            true
        });

        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut pending)
                .await
                .is_err()
        );
        let claude = coordinator.lock_target(AgentId::Claude).await;
        drop(claude);
        drop(first);
        assert!(pending.await.unwrap());
    });
}

#[test]
fn direct_remove_waits_for_the_same_target_coordinator() {
    tauri::async_runtime::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let hub = Arc::new(AgentHub::open(Some(dir.path())).unwrap());
        let direct_profile = AdapterProfile {
            id: "direct-remove-profile".into(),
            name: "Kimi → Claude".into(),
            source_kind: AdapterSourceKind::Provider,
            source_id: "kimi-connection".into(),
            target_agent_id: AgentId::Claude,
            route: AdapterRoute::NativeEndpoint,
            status: AdapterProfileStatus::Active,
            rule_id: "kimi-membership-to-claude-native-v1".into(),
            rule_version: "1".into(),
            generated_provider_id: None,
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: "now".into(),
            updated_at: "now".into(),
        };
        AdapterProfileRepo::new(hub.db.clone())
            .create(&direct_profile)
            .unwrap();

        let coordinator = Arc::new(AdapterBridgeSagaCoordinator::new());
        let target = coordinator.lock_target(AgentId::Claude).await;
        let exit = crate::exit_coordinator::ExitCoordinator::new();
        let waiter_hub = Arc::clone(&hub);
        let waiter_coordinator = Arc::clone(&coordinator);
        let barrier = exit.lifecycle_barrier();
        let mut pending = tauri::async_runtime::spawn(async move {
            remove_adapter_with_bridge_cleanup(
                waiter_hub,
                Arc::new(BridgeRuntimeHost::new()),
                waiter_coordinator,
                barrier,
                "direct-remove-profile".into(),
            )
            .await
        });

        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut pending)
                .await
                .is_err(),
            "direct removal must wait behind the target coordinator"
        );
        drop(target);
        pending.await.unwrap().unwrap();
        assert!(AdapterProfileRepo::new(hub.db.clone())
            .get("direct-remove-profile")
            .unwrap()
            .is_none());
    });
}
