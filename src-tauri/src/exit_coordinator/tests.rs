use agenthub_core::bridge::BridgeRuntimeHost;

use super::{
    exit_impact_action, CoordinatedShutdownAction, ExitBegin, ExitCoordinator, ExitImpactAction,
    ExitImpactChoice, ExitPreparation,
};

#[test]
fn empty_host_is_reported_to_future_exit_ui() {
    let coordinator = ExitCoordinator::new();
    let host = BridgeRuntimeHost::new();

    assert_eq!(coordinator.prepare_exit(&host).active_bridge_count, Some(0));
}

#[test]
fn restart_is_a_distinct_coordinated_shutdown_action() {
    assert_ne!(
        CoordinatedShutdownAction::Restart,
        CoordinatedShutdownAction::Exit
    );
}

#[test]
fn only_one_caller_can_claim_shutdown() {
    let coordinator = ExitCoordinator::new();
    let host = BridgeRuntimeHost::new();

    assert!(matches!(
        coordinator.begin_shutdown(&host),
        ExitBegin::Started(preparation) if preparation.active_bridge_count == Some(0)
    ));
    assert_eq!(
        coordinator.begin_shutdown(&host),
        ExitBegin::AlreadyInProgress
    );
    assert!(coordinator.shutdown_in_progress());
    assert!(!coordinator.exit_ready());
}

#[test]
fn shutdown_barrier_rejects_new_lifecycle_work_and_waits_for_inflight_saga() {
    tauri::async_runtime::block_on(async {
        let coordinator = ExitCoordinator::new();
        let host = BridgeRuntimeHost::new();
        let barrier = coordinator.lifecycle_barrier();
        let inflight = barrier.enter().await.unwrap();

        assert!(matches!(
            coordinator.begin_shutdown(&host),
            ExitBegin::Started(_)
        ));
        assert!(barrier.is_closed());
        assert!(barrier.enter().await.is_err());

        let waiting_barrier = coordinator.lifecycle_barrier();
        let mut exclusive_waiter = tauri::async_runtime::spawn(async move {
            let _exclusive = waiting_barrier.wait_for_sagas().await;
            true
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut exclusive_waiter)
                .await
                .is_err()
        );
        drop(inflight);
        assert!(exclusive_waiter.await.unwrap());
    });
}

#[test]
fn empty_host_shutdown_completes() {
    let coordinator = ExitCoordinator::new();
    let host = BridgeRuntimeHost::new();

    tauri::async_runtime::block_on(async {
        coordinator
            .shutdown_empty_host_for_test(&host)
            .await
            .unwrap();
    });
}

#[test]
fn impact_confirmation_is_required_for_active_or_unknown_bridge_state() {
    assert!(!ExitCoordinator::requires_impact_confirmation(
        ExitPreparation {
            active_bridge_count: Some(0),
        }
    ));
    assert!(ExitCoordinator::requires_impact_confirmation(
        ExitPreparation {
            active_bridge_count: Some(1),
        }
    ));
    assert!(ExitCoordinator::requires_impact_confirmation(
        ExitPreparation {
            active_bridge_count: None,
        }
    ));
}

#[test]
fn impact_choice_mapping_preserves_all_three_outcomes() {
    assert_eq!(
        exit_impact_action(ExitImpactChoice::HideToTray),
        ExitImpactAction::HideToTray
    );
    assert_eq!(
        exit_impact_action(ExitImpactChoice::StopBridgesAndExit),
        ExitImpactAction::RequestCoordinatedExit
    );
    assert_eq!(
        exit_impact_action(ExitImpactChoice::Cancel),
        ExitImpactAction::None
    );
}
