use super::*;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use agenthub_core::adapters::{AdapterRegistry, AgentAdapter};
use agenthub_core::bridge::{
    index_from_member_listings, BridgeHostError, BridgeStartSpec, BridgeUpstreamConfig,
    BridgeUpstreamStatus, MemberListing, ResolvedAuth,
};
use agenthub_core::error::{AppError, Result as CoreResult};
use agenthub_core::models::{
    Account, AccountKind, AgentConfig, AuthState, Capability, CapabilityState, DetectResult,
    DetectStatus, InstallChannel, LiveAccount, Provider, RunOptions, RunSpec,
};
use agenthub_core::services::{
    AccountService, AdapterBridgePrepareRequest, AdapterBridgePrepared,
    AdapterBridgeProviderProjection, AdapterBridgeRuntimeMaterial, ProviderService,
};
use agenthub_core::storage::{AccountRepo, AdapterProfileRepo, ProviderRepo};
use agenthub_core::AgentHub;
use serde_json::json;

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
        mode: agenthub_core::models::AdapterProfileMode::Api,
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
            protocol: agenthub_core::bridge::BridgeUpstreamProtocol::OpenAiChatCompletions,
            local_surface: agenthub_core::bridge::BridgeLocalSurface::Responses,
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
fn status_dto_maps_observed_upstream_and_missing_instance_to_stopped() {
    tauri::async_runtime::block_on(async {
        let host = BridgeRuntimeHost::new();
        let runtime = host.start(start_spec("profile-upstream")).await.unwrap();
        let unknown = AdapterBridgeStatusDto::from_runtime(runtime.clone());
        assert_eq!(unknown.upstream_status, "unknown");
        assert_eq!(unknown.state, "running");

        let connected = host
            .record_upstream_outcome("profile-upstream", BridgeUpstreamStatus::Connected)
            .unwrap()
            .unwrap();
        assert_eq!(
            AdapterBridgeStatusDto::from_runtime(connected).upstream_status,
            "connected"
        );

        let degraded = host
            .record_upstream_outcome("profile-upstream", BridgeUpstreamStatus::Degraded)
            .unwrap()
            .unwrap();
        assert_eq!(
            AdapterBridgeStatusDto::from_runtime(degraded).upstream_status,
            "degraded"
        );

        let stopped =
            AdapterBridgeStatusDto::from_runtime(host.stop("profile-upstream").await.unwrap());
        assert_eq!(stopped.state, "stopped");
        assert_eq!(stopped.upstream_status, "stopped");

        let missing = AdapterBridgeStatusDto::stopped(&profile(
            "profile-upstream",
            AdapterRoute::LocalBridge,
            AdapterProfileStatus::Active,
            true,
        ));
        assert_eq!(missing.state, "stopped");
        assert_eq!(missing.upstream_status, "stopped");
        host.shutdown().await.unwrap();
    });
}

#[test]
fn invalid_secret_reference_is_shown_in_chinese_for_claude_target() {
    assert_eq!(
        map_bridge_apply_error(
            "invalid argument: invalid adapter secret reference [invalid_arg]",
            AgentId::Claude,
        ),
        "这份 Grok 登录没法解析成 Claude 路由要用的密钥 [invalid_arg]"
    );
    assert_eq!(
        map_bridge_apply_error(
            "invalid argument: invalid adapter secret reference [invalid_arg]",
            AgentId::Codex,
        ),
        "这份登录没法解析成目标路由要用的密钥 [invalid_arg]"
    );
    assert_eq!(
        map_bridge_apply_error(
            "本机路由无法启动或停止 [adapter.bridge_start]",
            AgentId::Claude
        ),
        "本机路由无法启动或停止 [adapter.bridge_start]"
    );
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
        let first_status = ensure_bridge_listener(&host, &first, None, Vec::new(), false)
            .await
            .unwrap();
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

        let replaced = ensure_bridge_listener(&host, &rotated, None, Vec::new(), false)
            .await
            .unwrap();
        assert!(replaced.status.running);
        assert!(replaced.owned_by_saga);
        // Reuse of the same rotated material is not owned by a later saga.
        let reused = ensure_bridge_listener(&host, &rotated, None, Vec::new(), false)
            .await
            .unwrap();
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

        let ensured = ensure_bridge_listener(&host, &material, None, Vec::new(), false)
            .await
            .unwrap();
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
fn frozen_v2_port_does_not_rebind_on_occupancy() {
    tauri::async_runtime::block_on(async {
        let blocker = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let busy_port = blocker.local_addr().unwrap().port();
        let host = BridgeRuntimeHost::new();
        let index = index_from_member_listings(
            "profile-frozen",
            1,
            "responses",
            &[MemberListing {
                member_id: "test-source".into(),
                listed_models: vec!["m1".into()],
                upstream_provider: "kimi".into(),
                upstream_dialect: "kimi".into(),
                upstream_endpoint: "https://api.kimi.com/coding/v1".into(),
                transport_key: "openai:generic".into(),
                snapshot_ok: true,
            }],
            None,
        );
        let material = AdapterBridgeRuntimeMaterial::for_test(
            "profile-frozen",
            Some(busy_port),
            "local-bearer-frozen-value-xxxxxxxxx",
            "upstream-bearer-frozen-value-xxxxxxx",
        )
        .with_route_index_for_test(index);

        let error = match ensure_bridge_listener(&host, &material, None, Vec::new(), false).await {
            Err(error) => error,
            Ok(_) => panic!("occupancy must fail bind on the frozen port"),
        };
        assert!(matches!(error, BridgeHostError::Bind(_)));
        assert!(
            host.status("profile-frozen").unwrap().is_none(),
            "occupancy must not start a rewritten listener"
        );

        host.shutdown().await.unwrap();
        drop(blocker);
    });
}

/// Dogfood #1 (partial): upstream key rotation must replace the running listener
/// while the loopback local bearer stays stable for Codex.
#[test]
fn ensure_listener_replaces_upstream_auth_while_keeping_local_bearer() {
    tauri::async_runtime::block_on(async {
        let host = BridgeRuntimeHost::new();
        const LOCAL: &str = "local-bearer-stable-across-upstream-rot";
        let first = AdapterBridgeRuntimeMaterial::for_test(
            "profile-upstream-rotate",
            Some(0),
            LOCAL,
            "upstream-bearer-original-value-xxxxx",
        );
        let started = ensure_bridge_listener(&host, &first, None, Vec::new(), false)
            .await
            .unwrap();
        assert!(started.status.running);
        let first_port = started.status.port;

        let rotated = AdapterBridgeRuntimeMaterial::for_test(
            "profile-upstream-rotate",
            Some(first_port),
            LOCAL,
            "upstream-bearer-rotated-value-xxxxxx",
        );
        // Upstream-only drift must not be treated as an identical live start.
        assert_eq!(rotated.start_spec(None).local_token, LOCAL);
        assert!(matches!(
            host.start(rotated.start_spec(None)).await.unwrap_err(),
            BridgeHostError::ConflictingStart
        ));

        let replaced = ensure_bridge_listener(&host, &rotated, None, Vec::new(), false)
            .await
            .unwrap();
        assert!(replaced.status.running);
        assert!(replaced.owned_by_saga);
        assert!(
            host.status("profile-upstream-rotate")
                .unwrap()
                .is_some_and(|status| status.running),
            "listener must be running after upstream rotation"
        );

        // Identical rotated material is reused; local bearer remains the stable loopback token.
        let reused = ensure_bridge_listener(&host, &rotated, None, Vec::new(), false)
            .await
            .unwrap();
        assert!(reused.status.running);
        assert!(!reused.owned_by_saga);
        assert_eq!(rotated.start_spec(None).local_token, LOCAL);

        host.shutdown().await.unwrap();
    });
}

/// Dogfood #2 + #7 (partial): preferred port busy → rebind listener → realign
/// profile/provider projection so Codex base_url tracks the new port.
#[test]
fn busy_preferred_port_rebind_then_realign_updates_projection() {
    tauri::async_runtime::block_on(async {
        let blocker = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let preferred = blocker.local_addr().unwrap().port();
        let dir = tempfile::tempdir().unwrap();
        let hub = AgentHub::open(Some(dir.path())).unwrap();
        let profile = seed_active_bridge(&hub, "kimi-rebind-chain", preferred);
        let provider_id = profile.generated_provider_id.clone().unwrap();
        let local_bearer = hub
            .providers
            .repo()
            .get_by_id(&provider_id)
            .unwrap()
            .unwrap()
            .settings_config["auth"]["OPENAI_API_KEY"]
            .as_str()
            .unwrap()
            .to_owned();

        let material = AdapterBridgeRuntimeMaterial::for_test(
            profile.id.clone(),
            Some(preferred),
            local_bearer,
            "upstream-membership-secret",
        );
        let host = BridgeRuntimeHost::new();
        let ensured = ensure_bridge_listener(&host, &material, None, Vec::new(), false)
            .await
            .unwrap();
        assert!(ensured.status.running);
        assert_ne!(
            ensured.status.port, preferred,
            "must leave the occupied preferred port"
        );

        realign_restored_bridge_port(&hub, &profile.id, ensured.status.port).unwrap();

        let persisted = AdapterProfileRepo::new(hub.db.clone())
            .get(&profile.id)
            .unwrap()
            .unwrap();
        assert_eq!(persisted.local_port, Some(ensured.status.port));
        assert!(provider_content_contains(
            &hub,
            &provider_id,
            &format!("127.0.0.1:{}", ensured.status.port)
        ));
        assert!(!provider_content_contains(
            &hub,
            &provider_id,
            &format!("127.0.0.1:{preferred}")
        ));

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
fn apply_does_not_occupy_target_current_unless_generated_already_current() {
    assert!(!should_make_bridge_current(false));
    assert!(should_make_bridge_current(true));
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
            mode: agenthub_core::models::AdapterProfileMode::Api,
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

fn kimi_source(id: &str, api_key: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Kimi,
        name: "Kimi membership".into(),
        settings_config: json!({"apiKey": api_key}),
        meta: json!({"preset": "kimi-code-membership"}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn restore_prepare_request(source_id: &str) -> AdapterBridgePrepareRequest {
    AdapterBridgePrepareRequest {
        source_kind: AdapterSourceKind::Provider,
        source_id: source_id.into(),
        target_agent_id: AgentId::Codex,
        auto_start: true,
    }
}

fn create_projection(hub: &AgentHub, prepared: &AdapterBridgePrepared, port: u16) -> Provider {
    let input = match prepared.provider_projection(port).unwrap() {
        AdapterBridgeProviderProjection::Create(input) => input,
        other => panic!("expected create projection, got {other:?}"),
    };
    // Keep the generated row non-current so realign never writes ~/.codex.
    assert!(!input.is_current);
    hub.providers.create(&input).unwrap()
}

fn seed_active_bridge(hub: &AgentHub, source_id: &str, old_port: u16) -> AdapterProfile {
    ProviderRepo::new(hub.db.clone())
        .create(&kimi_source(source_id, "upstream-membership-secret"))
        .unwrap();
    let prepared = hub
        .adapter_bridge
        .prepare(&restore_prepare_request(source_id))
        .unwrap();
    let generated = create_projection(hub, &prepared, old_port);
    assert!(!generated.is_current);
    hub.adapter_bridge.finalize(&prepared, old_port).unwrap()
}

fn provider_content_contains(hub: &AgentHub, provider_id: &str, needle: &str) -> bool {
    let stored = hub
        .providers
        .repo()
        .get_by_id(provider_id)
        .unwrap()
        .unwrap();
    stored.settings_config["content"]
        .as_str()
        .unwrap_or_default()
        .contains(needle)
}

fn install_sql_trigger(hub: &AgentHub, sql: &str) {
    hub.db
        .with_conn(|conn| {
            conn.execute_batch(sql)?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn realign_restored_bridge_port_updates_provider_and_profile() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let profile = seed_active_bridge(&hub, "kimi-restore-happy", 43121);
    let provider_id = profile.generated_provider_id.clone().unwrap();

    realign_restored_bridge_port(&hub, &profile.id, 43155).unwrap();

    let persisted = AdapterProfileRepo::new(hub.db.clone())
        .get(&profile.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.local_port, Some(43155));
    assert_eq!(persisted.last_error_code, None);
    assert!(provider_content_contains(
        &hub,
        &provider_id,
        "127.0.0.1:43155"
    ));
    assert!(!provider_content_contains(
        &hub,
        &provider_id,
        "127.0.0.1:43121"
    ));
}

#[test]
fn realign_restored_bridge_port_rolls_back_when_provider_update_fails() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let profile = seed_active_bridge(&hub, "kimi-restore-update-fail", 43121);
    let provider_id = profile.generated_provider_id.clone().unwrap();
    install_sql_trigger(
        &hub,
        r#"
        CREATE TRIGGER fail_restore_rebind_update
        BEFORE UPDATE ON providers
        WHEN NEW.settings_config LIKE '%127.0.0.1:43155%'
        BEGIN
            SELECT RAISE(ABORT, 'injected restore update failure');
        END;
        "#,
    );

    let error = realign_restored_bridge_port(&hub, &profile.id, 43155).unwrap_err();
    assert!(error.contains("injected restore update failure"), "{error}");
    assert!(
        !error.contains("adapter.bridge_rollback"),
        "update-stage failure must not need compensation: {error}"
    );

    let persisted = AdapterProfileRepo::new(hub.db.clone())
        .get(&profile.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.local_port, Some(43121));
    assert!(provider_content_contains(
        &hub,
        &provider_id,
        "127.0.0.1:43121"
    ));
    assert!(!provider_content_contains(
        &hub,
        &provider_id,
        "127.0.0.1:43155"
    ));
}

#[test]
fn realign_restored_bridge_port_rolls_back_provider_when_persist_fails() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let profile = seed_active_bridge(&hub, "kimi-restore-persist-fail", 43121);
    let provider_id = profile.generated_provider_id.clone().unwrap();
    install_sql_trigger(
        &hub,
        r#"
        CREATE TRIGGER fail_restore_rebind_persist
        BEFORE UPDATE OF local_port ON adapter_profiles
        WHEN NEW.local_port = 43155
        BEGIN
            SELECT RAISE(ABORT, 'injected restore persist failure');
        END;
        "#,
    );

    let error = realign_restored_bridge_port(&hub, &profile.id, 43155).unwrap_err();
    assert!(
        error.contains("injected restore persist failure"),
        "{error}"
    );
    assert!(
        !error.contains("adapter.bridge_rollback"),
        "successful compensation must not report adapter.bridge_rollback: {error}"
    );

    let persisted = AdapterProfileRepo::new(hub.db.clone())
        .get(&profile.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.local_port, Some(43121));
    assert!(provider_content_contains(
        &hub,
        &provider_id,
        "127.0.0.1:43121"
    ));
    assert!(!provider_content_contains(
        &hub,
        &provider_id,
        "127.0.0.1:43155"
    ));
}

struct IsolatedCodexAdapter {
    config: Mutex<AgentConfig>,
    config_path: PathBuf,
    write_attempts: AtomicUsize,
    fail_on_write: AtomicUsize,
}

impl IsolatedCodexAdapter {
    fn new(config: AgentConfig, config_path: PathBuf) -> Self {
        Self {
            config: Mutex::new(config),
            config_path,
            write_attempts: AtomicUsize::new(0),
            fail_on_write: AtomicUsize::new(0),
        }
    }

    fn fail_on_write(&self, attempt: usize) {
        self.fail_on_write.store(attempt, Ordering::SeqCst);
    }

    fn config(&self) -> AgentConfig {
        self.config.lock().unwrap().clone()
    }
}

impl AgentAdapter for IsolatedCodexAdapter {
    fn id(&self) -> AgentId {
        AgentId::Codex
    }

    fn detect(&self) -> DetectResult {
        DetectResult {
            agent: AgentId::Codex,
            status: DetectStatus::NotFound,
            version: None,
            binary_path: None,
            channel: None,
            env_ready: true,
            notes: vec![],
            extra_copies: Vec::new(),
        }
    }

    fn install_channels(&self) -> Vec<InstallChannel> {
        vec![]
    }

    fn read_config(&self) -> CoreResult<AgentConfig> {
        Ok(self.config())
    }

    fn write_config(&self, config: &AgentConfig) -> CoreResult<()> {
        let attempt = self.write_attempts.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_on_write.load(Ordering::SeqCst) == attempt {
            return Err(AppError::message(
                "test.write",
                format!("injected live write failure {attempt}"),
            ));
        }
        let bytes = serde_json::to_vec(config)?;
        std::fs::create_dir_all(self.config_path.parent().unwrap()).ok();
        std::fs::write(&self.config_path, bytes)?;
        *self.config.lock().unwrap() = config.clone();
        Ok(())
    }

    fn read_auth(&self) -> CoreResult<AuthState> {
        Err(AppError::Unsupported("isolated".into()))
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        match cap {
            Capability::ConfigWrite | Capability::LiveBackup => CapabilityState::full(),
            _ => CapabilityState::unsupported("isolated"),
        }
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        None
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        vec![self.config_path.clone()]
    }

    fn build_run_spec(
        &self,
        _binary: &Path,
        _prompt: &str,
        _opts: &RunOptions,
    ) -> CoreResult<RunSpec> {
        Err(AppError::Unsupported("isolated".into()))
    }
}

fn isolated_restore_hub(
    live: AgentConfig,
) -> (tempfile::TempDir, AgentHub, Arc<IsolatedCodexAdapter>) {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = AgentHub::open(Some(dir.path())).unwrap();
    let adapter = Arc::new(IsolatedCodexAdapter::new(
        live,
        dir.path().join("isolated-codex.json"),
    ));
    let mut registry = AdapterRegistry::new();
    registry.register(adapter.clone());
    hub.providers = ProviderService::with_live(
        hub.db.clone(),
        registry,
        dir.path().join("isolated-backups"),
    );
    (dir, hub, adapter)
}

fn mark_generated_current(hub: &AgentHub, provider_id: &str) {
    let mut stored = hub
        .providers
        .repo()
        .get_by_id(provider_id)
        .unwrap()
        .unwrap();
    stored.is_current = true;
    hub.providers.repo().update(&stored).unwrap();
}

#[test]
fn realign_restored_bridge_port_rolls_back_when_switch_fails() {
    let old_live = AgentConfig {
        agent: AgentId::Codex,
        raw: json!({
            "format": "toml",
            "content": "model = \"old-live\"\n"
        }),
    };
    let (_dir, hub, adapter) = isolated_restore_hub(old_live.clone());
    let profile = seed_active_bridge(&hub, "kimi-restore-switch-fail", 43121);
    let provider_id = profile.generated_provider_id.clone().unwrap();
    mark_generated_current(&hub, &provider_id);
    // Fail the first live write, which switch_locked_inner performs after the
    // pool update has already rewritten the generated provider to the new port.
    adapter.fail_on_write(1);

    let error = realign_restored_bridge_port(&hub, &profile.id, 43155).unwrap_err();
    assert!(error.contains("injected live write failure"), "{error}");
    assert!(
        !error.contains("adapter.bridge_rollback"),
        "successful compensation must not report adapter.bridge_rollback: {error}"
    );

    let persisted = AdapterProfileRepo::new(hub.db.clone())
        .get(&profile.id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.local_port, Some(43121));
    assert!(provider_content_contains(
        &hub,
        &provider_id,
        "127.0.0.1:43121"
    ));
    assert!(!provider_content_contains(
        &hub,
        &provider_id,
        "127.0.0.1:43155"
    ));
    assert_eq!(adapter.config(), old_live);
}

struct IsolatedLiveAdapter {
    agent: AgentId,
    config: Mutex<AgentConfig>,
    config_path: PathBuf,
    auth_path: PathBuf,
    config_writes: AtomicUsize,
    auth_writes: AtomicUsize,
}

impl IsolatedLiveAdapter {
    fn new(agent: AgentId, config: AgentConfig, config_path: PathBuf, auth_path: PathBuf) -> Self {
        Self {
            agent,
            config: Mutex::new(config),
            config_path,
            auth_path,
            config_writes: AtomicUsize::new(0),
            auth_writes: AtomicUsize::new(0),
        }
    }

    fn config_writes(&self) -> usize {
        self.config_writes.load(Ordering::SeqCst)
    }

    fn auth_writes(&self) -> usize {
        self.auth_writes.load(Ordering::SeqCst)
    }
}

impl AgentAdapter for IsolatedLiveAdapter {
    fn id(&self) -> AgentId {
        self.agent
    }

    fn detect(&self) -> DetectResult {
        DetectResult {
            agent: self.agent,
            status: DetectStatus::NotFound,
            version: None,
            binary_path: None,
            channel: None,
            env_ready: true,
            notes: vec![],
            extra_copies: Vec::new(),
        }
    }

    fn install_channels(&self) -> Vec<InstallChannel> {
        vec![]
    }

    fn read_config(&self) -> CoreResult<AgentConfig> {
        Ok(self.config.lock().unwrap().clone())
    }

    fn write_config(&self, config: &AgentConfig) -> CoreResult<()> {
        self.config_writes.fetch_add(1, Ordering::SeqCst);
        let bytes = serde_json::to_vec(config)?;
        std::fs::create_dir_all(self.config_path.parent().unwrap()).ok();
        std::fs::write(&self.config_path, bytes)?;
        *self.config.lock().unwrap() = config.clone();
        Ok(())
    }

    fn read_auth(&self) -> CoreResult<AuthState> {
        Err(AppError::Unsupported("isolated".into()))
    }

    fn read_account(&self) -> CoreResult<LiveAccount> {
        Err(AppError::NotFound("no live auth".into()))
    }

    fn apply_account(&self, account: &LiveAccount) -> CoreResult<()> {
        if account.agent != self.agent {
            return Err(AppError::InvalidArg("account agent mismatch".into()));
        }
        self.auth_writes.fetch_add(1, Ordering::SeqCst);
        std::fs::create_dir_all(self.auth_path.parent().unwrap()).ok();
        std::fs::write(&self.auth_path, serde_json::to_vec(&account.credentials)?)?;
        Ok(())
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        match cap {
            Capability::ConfigWrite | Capability::LiveBackup | Capability::AccountSwitch => {
                CapabilityState::full()
            }
            _ => CapabilityState::unsupported("isolated"),
        }
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        None
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        vec![self.config_path.clone(), self.auth_path.clone()]
    }

    fn build_run_spec(
        &self,
        _binary: &Path,
        _prompt: &str,
        _opts: &RunOptions,
    ) -> CoreResult<RunSpec> {
        Err(AppError::Unsupported("isolated".into()))
    }
}

fn grok_oauth_account(id: &str) -> Account {
    Account {
        id: id.into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::Oauth,
        label: "Grok subscription".into(),
        credentials: json!({
            "format": "oauth",
            "access_token": "grok-upstream-secret",
            "refresh_token": "grok-refresh-secret"
        }),
        extra: json!({}),
        status: "active".into(),
        is_current: true,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

fn isolated_route_hub(
    source: AgentId,
    target: AgentId,
) -> (
    tempfile::TempDir,
    AgentHub,
    Arc<IsolatedLiveAdapter>,
    Arc<IsolatedLiveAdapter>,
) {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = AgentHub::open(Some(dir.path())).unwrap();
    let source_adapter = Arc::new(IsolatedLiveAdapter::new(
        source,
        AgentConfig {
            agent: source,
            raw: json!({"source": "live"}),
        },
        dir.path().join(format!("{}.config.json", source.as_str())),
        dir.path().join(format!("{}.auth.json", source.as_str())),
    ));
    let target_adapter = Arc::new(IsolatedLiveAdapter::new(
        target,
        AgentConfig {
            agent: target,
            raw: json!({"target": "live"}),
        },
        dir.path().join(format!("{}.config.json", target.as_str())),
        dir.path().join(format!("{}.auth.json", target.as_str())),
    ));
    std::fs::write(&source_adapter.auth_path, b"original-source-auth\n").unwrap();
    std::fs::write(&target_adapter.auth_path, b"original-target-auth\n").unwrap();
    std::fs::write(&source_adapter.config_path, b"original-source-config\n").unwrap();
    std::fs::write(&target_adapter.config_path, b"original-target-config\n").unwrap();
    let mut registry = AdapterRegistry::new();
    registry.register(source_adapter.clone());
    registry.register(target_adapter.clone());
    let backups = dir.path().join("isolated-backups");
    hub.providers = ProviderService::with_live(hub.db.clone(), registry.clone(), backups.clone());
    hub.accounts = AccountService::with_live(hub.db.clone(), registry, backups);
    (dir, hub, source_adapter, target_adapter)
}

fn seed_current_target_provider(hub: &AgentHub, target: AgentId) {
    ProviderRepo::new(hub.db.clone())
        .create(&Provider {
            id: "target-current".into(),
            agent_id: target,
            name: "existing live".into(),
            settings_config: json!({}),
            meta: json!({}),
            is_current: true,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();
}

#[test]
fn oauth_local_bridge_bind_reuses_login_row_and_does_not_occupy_live() {
    let cases = [
        (
            "grok-claude",
            AgentId::Grok,
            AgentId::Claude,
            AdapterBridgePrepareRequest {
                source_kind: AdapterSourceKind::Account,
                source_id: "grok-subscription".into(),
                target_agent_id: AgentId::Claude,
                auto_start: true,
            },
        ),
        (
            "grok-codex",
            AgentId::Grok,
            AgentId::Codex,
            AdapterBridgePrepareRequest {
                source_kind: AdapterSourceKind::Account,
                source_id: "grok-subscription".into(),
                target_agent_id: AgentId::Codex,
                auto_start: true,
            },
        ),
    ];
    for (label, source_agent, target_agent, request) in cases {
        let (_dir, hub, source_adapter, target_adapter) =
            isolated_route_hub(source_agent, target_agent);
        AccountRepo::new(hub.db.clone())
            .create(&grok_oauth_account("grok-subscription"))
            .unwrap();
        seed_current_target_provider(&hub, target_agent);
        let source_auth_before = std::fs::read(&source_adapter.auth_path).unwrap();
        let source_config_before = std::fs::read(&source_adapter.config_path).unwrap();
        let target_auth_before = std::fs::read(&target_adapter.auth_path).unwrap();
        let target_config_before = std::fs::read(&target_adapter.config_path).unwrap();

        let prepared = hub.adapter_bridge.prepare(&request).unwrap();
        let core_guard = hub.providers.begin_live_saga(target_agent).unwrap();
        let projection = hub
            .adapter_bridge
            .revalidate_provider_projection(&prepared, 43121)
            .unwrap();
        let snapshot = capture_provider_snapshot(
            &hub,
            &core_guard,
            prepared.profile().generated_provider_id.as_deref(),
            target_agent,
        )
        .unwrap();
        let result = persist_bridge_projection_inner(
            &hub,
            &core_guard,
            &prepared,
            projection,
            43121,
            &snapshot,
        )
        .unwrap();

        let accounts = AccountRepo::new(hub.db.clone()).list(None).unwrap();
        assert_eq!(accounts.len(), 1, "{label}");
        assert_eq!(accounts[0].id, "grok-subscription", "{label}");
        assert_eq!(accounts[0].agent_id, AgentId::Grok, "{label}");
        assert_eq!(prepared.profile().source_id, "grok-subscription", "{label}");
        assert!(!result.provider.is_current, "{label}");
        let current = hub
            .providers
            .repo()
            .get_current(target_agent)
            .unwrap()
            .expect("target still has a current provider");
        assert_eq!(current.id, "target-current", "{label}");
        assert_eq!(source_adapter.config_writes(), 0, "{label}");
        assert_eq!(source_adapter.auth_writes(), 0, "{label}");
        assert_eq!(target_adapter.config_writes(), 0, "{label}");
        assert_eq!(target_adapter.auth_writes(), 0, "{label}");
        assert_eq!(
            std::fs::read(&source_adapter.auth_path).unwrap(),
            source_auth_before,
            "{label}"
        );
        assert_eq!(
            std::fs::read(&source_adapter.config_path).unwrap(),
            source_config_before,
            "{label}"
        );
        assert_eq!(
            std::fs::read(&target_adapter.auth_path).unwrap(),
            target_auth_before,
            "{label}"
        );
        assert_eq!(
            std::fs::read(&target_adapter.config_path).unwrap(),
            target_config_before,
            "{label}"
        );
    }
}

#[test]
fn rollback_skips_live_switch_when_bind_did_not_occupy() {
    let (_dir, hub, source_adapter, target_adapter) =
        isolated_route_hub(AgentId::Grok, AgentId::Claude);
    AccountRepo::new(hub.db.clone())
        .create(&grok_oauth_account("grok-subscription"))
        .unwrap();
    seed_current_target_provider(&hub, AgentId::Claude);
    let target_config_before = std::fs::read(&target_adapter.config_path).unwrap();
    let request = AdapterBridgePrepareRequest {
        source_kind: AdapterSourceKind::Account,
        source_id: "grok-subscription".into(),
        target_agent_id: AgentId::Claude,
        auto_start: true,
    };
    let prepared = hub.adapter_bridge.prepare(&request).unwrap();
    install_sql_trigger(
        &hub,
        r#"
        CREATE TRIGGER fail_bridge_finalize
        BEFORE UPDATE OF local_port ON adapter_profiles
        WHEN NEW.local_port = 43121
        BEGIN
            SELECT RAISE(ABORT, 'injected finalize failure');
        END;
        "#,
    );
    let core_guard = hub.providers.begin_live_saga(AgentId::Claude).unwrap();
    let projection = hub
        .adapter_bridge
        .revalidate_provider_projection(&prepared, 43121)
        .unwrap();
    let snapshot = capture_provider_snapshot(
        &hub,
        &core_guard,
        prepared.profile().generated_provider_id.as_deref(),
        AgentId::Claude,
    )
    .unwrap();
    let error =
        persist_bridge_projection_inner(&hub, &core_guard, &prepared, projection, 43121, &snapshot)
            .unwrap_err();
    assert!(error.contains("injected finalize failure"), "{error}");
    assert!(
        !error.contains("adapter.bridge_rollback"),
        "non-occupying rollback must not rewrite live: {error}"
    );
    let current = hub
        .providers
        .repo()
        .get_current(AgentId::Claude)
        .unwrap()
        .unwrap();
    assert_eq!(current.id, "target-current");
    assert_eq!(source_adapter.config_writes(), 0);
    assert_eq!(target_adapter.config_writes(), 0);
    assert_eq!(
        std::fs::read(&target_adapter.config_path).unwrap(),
        target_config_before
    );
}

#[test]
fn apply_local_bridge_from_grok_oauth_does_not_occupy_claude_current() {
    tauri::async_runtime::block_on(async {
        let (_dir, hub, source_adapter, target_adapter) =
            isolated_route_hub(AgentId::Grok, AgentId::Claude);
        AccountRepo::new(hub.db.clone())
            .create(&grok_oauth_account("grok-subscription"))
            .unwrap();
        seed_current_target_provider(&hub, AgentId::Claude);
        let source_auth_before = std::fs::read(&source_adapter.auth_path).unwrap();
        let hub = Arc::new(hub);
        let host = Arc::new(BridgeRuntimeHost::new());
        let coordinator = Arc::new(AdapterBridgeSagaCoordinator::new());
        let exit = crate::exit_coordinator::ExitCoordinator::new();
        let result = apply_local_bridge(
            Arc::clone(&hub),
            Arc::clone(&host),
            coordinator,
            exit.lifecycle_barrier(),
            AdapterBridgePrepareRequest {
                source_kind: AdapterSourceKind::Account,
                source_id: "grok-subscription".into(),
                target_agent_id: AgentId::Claude,
                auto_start: true,
            },
        )
        .await
        .unwrap();

        let accounts = AccountRepo::new(hub.db.clone()).list(None).unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, "grok-subscription");
        assert!(!result.provider.is_current);
        let current = hub
            .providers
            .repo()
            .get_current(AgentId::Claude)
            .unwrap()
            .unwrap();
        assert_eq!(current.id, "target-current");
        assert_eq!(result.profile.status, AdapterProfileStatus::Active);
        assert!(result.profile.local_port.is_some());
        assert_eq!(source_adapter.auth_writes(), 0);
        assert_eq!(source_adapter.config_writes(), 0);
        assert_eq!(target_adapter.config_writes(), 0);
        assert_eq!(
            std::fs::read(&source_adapter.auth_path).unwrap(),
            source_auth_before
        );
        host.shutdown().await.unwrap();
    });
}
