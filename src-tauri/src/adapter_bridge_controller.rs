//! Desktop-process control plane for Adapter local bridges.
//!
//! `agenthub-core` owns profile persistence, source-secret resolution and the
//! generated Codex provider projection.  This module deliberately owns the
//! cross-boundary saga: the loopback listener must bind before the generated
//! provider is made current, and a failed apply must not leave a newly-started
//! listener running.  Neither upstream nor local bearer tokens cross the
//! Tauri command boundary.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use agenthub_core::bridge::{
    BridgeHostError, BridgeRuntimeHost, BridgeRuntimeState, BridgeRuntimeStatus,
    BridgeUpstreamStatus,
};
use agenthub_core::models::{
    AdapterApplyResult, AdapterProfile, AdapterProfileStatus, AdapterRoute, AdapterSourceKind,
    AgentId, Provider, ProviderInput,
};
use agenthub_core::services::{
    AdapterBridgePrepareRequest, AdapterBridgePrepared, AdapterBridgeProviderProjection,
    ProviderLiveConfigSnapshot, ProviderLiveSagaGuard,
};
use agenthub_core::AgentHub;
use serde::Serialize;

use crate::commands::{map_err_string, with_hub_blocking};
use crate::exit_coordinator::LifecycleShutdownBarrier;

const CODE_BRIDGE_START: &str = "adapter.bridge_start";
const CODE_BRIDGE_PROJECTION: &str = "adapter.bridge_projection";
const CODE_BRIDGE_FINALIZE: &str = "adapter.bridge_finalize";
const CODE_BRIDGE_RESTORE_SOURCE: &str = "adapter.bridge_restore_source";
const CODE_BRIDGE_RESTORE_START: &str = "adapter.bridge_restore_start";
const CODE_BRIDGE_PORT_IN_USE: &str = "adapter.port_in_use";

/// Process-local authority for bridge sagas. Every operation for one profile
/// takes the same lock, while provider-changing stages also serialize against
/// other Codex bridge projections before a live config snapshot is captured.
/// This is intentionally owned by `AppState`, not a global: tests and
/// alternate desktop hosts must not share lifecycle state.
pub(crate) struct AdapterBridgeSagaCoordinator {
    profiles: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    targets: Mutex<HashMap<AgentId, Arc<tokio::sync::Mutex<()>>>>,
}

impl AdapterBridgeSagaCoordinator {
    pub(crate) fn new() -> Self {
        Self {
            profiles: Mutex::new(HashMap::new()),
            targets: Mutex::new(HashMap::new()),
        }
    }

    async fn lock_profile(&self, profile_id: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut profiles = self
                .profiles
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Arc::clone(
                profiles
                    .entry(profile_id.to_owned())
                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
            )
        };
        lock.lock_owned().await
    }

    /// The single Tauri authority for mutations that can change one target
    /// agent's live configuration or authentication.  The lock is per-agent
    /// so a Claude operation never unnecessarily blocks Codex, while all
    /// Codex paths share exactly the same authority as a bridge saga.
    pub(crate) async fn lock_target(&self, agent: AgentId) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut targets = self
                .targets
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Arc::clone(
                targets
                    .entry(agent)
                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
            )
        };
        lock.lock_owned().await
    }
}

impl Default for AdapterBridgeSagaCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// Credential-free listener state exposed through Tauri.
///
/// The Core `BridgeRuntimeStatus` intentionally has no serde implementation,
/// so the GUI gets this small, deliberate DTO rather than a structural dump of
/// runtime internals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdapterBridgeStatusDto {
    pub profile_id: String,
    pub port: Option<u16>,
    pub running: bool,
    pub state: String,
    pub upstream_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_connection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at_unix_ms: Option<u128>,
}

impl AdapterBridgeStatusDto {
    fn stopped(profile: &AdapterProfile) -> Self {
        Self {
            profile_id: profile.id.clone(),
            port: profile.local_port,
            running: false,
            state: "stopped".into(),
            upstream_status: "unknown".into(),
            source_connection_id: Some(profile.source_id.clone()),
            started_at_unix_ms: None,
        }
    }

    fn from_runtime(status: BridgeRuntimeStatus) -> Self {
        Self {
            profile_id: status.profile_id,
            port: Some(status.port),
            running: status.running,
            state: runtime_state_name(status.state).into(),
            upstream_status: upstream_status_name(status.upstream_status).into(),
            source_connection_id: status.source_connection_id,
            started_at_unix_ms: system_time_millis(status.started_at),
        }
    }
}

/// Apply a Kimi membership -> Codex local bridge.
///
/// The direct Kimi -> Claude adapter intentionally stays in
/// `AdapterApplyService`; only the local route has a listener lifecycle.
pub(crate) async fn apply_local_bridge(
    hub: Arc<AgentHub>,
    host: Arc<BridgeRuntimeHost>,
    coordinator: Arc<AdapterBridgeSagaCoordinator>,
    lifecycle_barrier: Arc<LifecycleShutdownBarrier>,
    request: AdapterBridgePrepareRequest,
) -> Result<AdapterApplyResult, String> {
    let _lifecycle_permit = lifecycle_barrier.enter().await?;
    let profile_id = bridge_profile_id_for_request(hub.clone(), request.clone()).await?;
    let _profile_guard = coordinator.lock_profile(&profile_id).await;
    apply_local_bridge_locked(hub, host, coordinator, request).await
}

async fn apply_local_bridge_locked(
    hub: Arc<AgentHub>,
    host: Arc<BridgeRuntimeHost>,
    coordinator: Arc<AdapterBridgeSagaCoordinator>,
    request: AdapterBridgePrepareRequest,
) -> Result<AdapterApplyResult, String> {
    let prepared = with_hub_blocking(hub.clone(), move |hub| {
        hub.adapter_bridge
            .prepare(&request)
            .map_err(|error| map_err_string("prepare_adapter_bridge", error))
    })
    .await?;

    let profile_id = prepared.profile().id.clone();
    let runtime = match ensure_bridge_listener(host.as_ref(), prepared.runtime_material()).await {
        Ok(runtime) => runtime,
        Err(error) => {
            let code = if matches!(error, BridgeHostError::Bind(_)) {
                CODE_BRIDGE_PORT_IN_USE
            } else {
                CODE_BRIDGE_START
            };
            mark_retryable(hub, &profile_id, code).await;
            return Err(map_bridge_host_error(error));
        }
    };
    // Own any listener this saga started or replaced. An idempotent reuse of an
    // already-running identical spec is not compensated on later projection failure.
    let owns_listener = runtime.owned_by_saga;

    let port = runtime.status.port;
    if let Err(error) = prepared.runtime_material().verify_bound_health(port).await {
        let code = error.code().to_owned();
        let listener_compensated =
            compensate_started_bridge(&host, &profile_id, owns_listener).await;
        if listener_compensated {
            mark_retryable(hub, &profile_id, &code).await;
        } else {
            mark_needs_attention(hub, &profile_id, "adapter.bridge_stop").await;
        }
        return Err(map_err_string("verify_adapter_bridge_health", error));
    }

    // Keep the same Tauri target authority as every configuration/account/
    // provider mutation. Inside the blocking critical section, the Core guard
    // retains the cross-process switch lock from snapshot through
    // projection/finalize/rollback; do not call ordinary lock-taking provider
    // APIs while it is held.
    let _target_guard = coordinator.lock_target(AgentId::Codex).await;
    let result = with_hub_blocking(hub.clone(), move |hub| {
        let core_guard = hub
            .providers
            .begin_live_saga(AgentId::Codex)
            .map_err(|error| map_err_string("begin_adapter_bridge_provider_saga", error))?;
        let projection = hub
            .adapter_bridge
            .revalidate_provider_projection(&prepared, port)
            .map_err(|error| map_err_string("revalidate_adapter_bridge_provider", error))?;
        let provider_id = prepared.profile().generated_provider_id.clone();
        let snapshot = capture_provider_snapshot(hub, &core_guard, provider_id.as_deref())?;
        persist_bridge_projection_inner(hub, &core_guard, &prepared, projection, port, &snapshot)
    })
    .await;

    match result {
        Ok(result) => Ok(result),
        Err(error) => {
            let listener_compensated =
                compensate_started_bridge(&host, &profile_id, owns_listener).await;
            let code = if error.contains("finalize_adapter_bridge") {
                CODE_BRIDGE_FINALIZE
            } else {
                CODE_BRIDGE_PROJECTION
            };
            // A reversible failure remains retryable. NeedsAttention is
            // reserved for a failed rollback/listener compensation, where the
            // stored profile can no longer truthfully describe runtime state.
            if listener_compensated && !error.contains("adapter.bridge_rollback") {
                mark_retryable(hub, &profile_id, code).await;
            } else {
                mark_needs_attention(hub, &profile_id, "adapter.bridge_rollback").await;
            }
            Err(error)
        }
    }
}

/// Start an existing local bridge.  This is the manual counterpart to the
/// background restore flow and works even when automatic restore is disabled.
pub(crate) async fn start_local_bridge(
    hub: Arc<AgentHub>,
    host: Arc<BridgeRuntimeHost>,
    coordinator: Arc<AdapterBridgeSagaCoordinator>,
    lifecycle_barrier: Arc<LifecycleShutdownBarrier>,
    profile_id: String,
) -> Result<AdapterBridgeStatusDto, String> {
    let _lifecycle_permit = lifecycle_barrier.enter().await?;
    let _profile_guard = coordinator.lock_profile(&profile_id).await;
    let profile = load_bridge_profile(hub.clone(), profile_id).await?;
    let request = AdapterBridgePrepareRequest {
        source_kind: profile.source_kind,
        source_id: profile.source_id,
        target_agent_id: profile.target_agent_id,
        auto_start: profile.auto_start,
    };
    let applied = apply_local_bridge_locked(hub, host.clone(), coordinator, request).await?;
    let status = host
        .status(&applied.profile.id)
        .map_err(map_bridge_host_error)?
        .ok_or_else(|| "bridge listener did not report a runtime status".to_string())?;
    Ok(AdapterBridgeStatusDto::from_runtime(status))
}

/// Stop a bridge without altering its generated provider or auto-start choice.
/// A later manual start or application restart can bring the same active
/// profile back with the persisted loopback port.
pub(crate) async fn stop_local_bridge(
    hub: Arc<AgentHub>,
    host: Arc<BridgeRuntimeHost>,
    coordinator: Arc<AdapterBridgeSagaCoordinator>,
    lifecycle_barrier: Arc<LifecycleShutdownBarrier>,
    profile_id: String,
) -> Result<AdapterBridgeStatusDto, String> {
    let _lifecycle_permit = lifecycle_barrier.enter().await?;
    let _profile_guard = coordinator.lock_profile(&profile_id).await;
    let profile = load_bridge_profile(hub, profile_id).await?;
    let status = stop_bridge_runtime(&host, &profile).await?;
    Ok(AdapterBridgeStatusDto::from_runtime(status))
}

/// Return an observable bridge state without returning any bearer or upstream
/// configuration details.
pub(crate) async fn local_bridge_status(
    hub: Arc<AgentHub>,
    host: Arc<BridgeRuntimeHost>,
    profile_id: String,
) -> Result<AdapterBridgeStatusDto, String> {
    let profile = load_bridge_profile(hub, profile_id).await?;
    let status = host.status(&profile.id).map_err(map_bridge_host_error)?;
    Ok(status
        .map(AdapterBridgeStatusDto::from_runtime)
        .unwrap_or_else(|| AdapterBridgeStatusDto::stopped(&profile)))
}

/// Persist an existing bridge profile's auto-start preference.  It controls
/// desktop startup restoration only; it does not start or stop a listener.
pub(crate) async fn set_local_bridge_auto_start(
    hub: Arc<AgentHub>,
    profile_id: String,
    auto_start: bool,
) -> Result<AdapterProfile, String> {
    with_hub_blocking(hub, move |hub| {
        hub.adapter_bridge
            .set_auto_start(&profile_id, auto_start)
            .map_err(|error| map_err_string("set_adapter_bridge_auto_start", error))
    })
    .await
}

/// Remove an adapter profile, stopping a local bridge only after a strict
/// bridge-specific preflight. Local bridges cannot use the Claude-only
/// `AdapterApplyService::remove` ownership validator.
pub(crate) async fn remove_adapter_with_bridge_cleanup(
    hub: Arc<AgentHub>,
    host: Arc<BridgeRuntimeHost>,
    coordinator: Arc<AdapterBridgeSagaCoordinator>,
    lifecycle_barrier: Arc<LifecycleShutdownBarrier>,
    profile_id: String,
) -> Result<(), String> {
    let _lifecycle_permit = lifecycle_barrier.enter().await?;
    let _profile_guard = coordinator.lock_profile(&profile_id).await;
    let profile = load_adapter_profile(hub.clone(), profile_id.clone()).await?;
    // Every route takes the same target authority before selecting its Core
    // removal path. This keeps direct Claude removal from interleaving a
    // target-level Tauri configuration/account/provider mutation.
    let _target_guard = coordinator.lock_target(profile.target_agent_id).await;
    if profile.route != AdapterRoute::LocalBridge {
        return with_hub_blocking(hub, move |hub| {
            hub.adapter_apply
                .remove(&profile_id)
                .map_err(|error| map_err_string("remove_adapter", error))
        })
        .await;
    }

    // Preflight and stop outside the Core live-saga critical section so listener
    // drain does not hold the cross-process provider lock.
    let preflight_profile = with_hub_blocking(hub.clone(), {
        let profile_id = profile_id.clone();
        move |hub| {
            hub.adapter_bridge
                .preflight_remove(&profile_id)
                .map(|removal| removal.profile().clone())
                .map_err(|error| map_err_string("preflight_remove_adapter_bridge", error))
        }
    })
    .await?;
    let _ = stop_bridge_runtime(&host, &preflight_profile).await?;

    // Revalidate, delete provider, and complete profile removal under the same
    // target authority and Core live-saga guard as bridge apply.
    with_hub_blocking(hub, move |hub| {
        let core_guard = hub
            .providers
            .begin_live_saga(AgentId::Codex)
            .map_err(|error| map_err_string("begin_adapter_bridge_remove_saga", error))?;
        let removal = hub
            .adapter_bridge
            .preflight_remove(&profile_id)
            .map_err(|error| map_err_string("remove_adapter_bridge", error))?;
        let recovery = removal.recovery_input();
        if let Some(provider_id) = removal.generated_provider_id() {
            hub.providers
                .delete_with_guard(&core_guard, provider_id, AgentId::Codex)
                .map_err(|error| map_err_string("remove_adapter_bridge_provider", error))?;
        }
        if let Err(_error) = hub.adapter_bridge.complete_remove(&removal) {
            let restored = recovery
                .as_ref()
                .is_some_and(|input| hub.providers.create_with_guard(&core_guard, input).is_ok());
            let code = if restored {
                "adapter.bridge_remove_profile_restored"
            } else {
                "adapter.bridge_remove_incomplete"
            };
            return Err(format!(
                "本地适配器删除未完成；{} [{code}]",
                if restored {
                    "已恢复 generated Connection"
                } else {
                    "generated Connection 已移除，请重试删除"
                }
            ));
        }
        Ok(())
    })
    .await
}

/// Schedule automatic bridge recovery after the GUI has finished setting up.
/// It intentionally returns immediately: a failed source or occupied port is
/// isolated to that profile and never delays the first window/tray paint.
pub(crate) fn restore_adapter_bridges(
    hub: Arc<AgentHub>,
    host: Arc<BridgeRuntimeHost>,
    coordinator: Arc<AdapterBridgeSagaCoordinator>,
    lifecycle_barrier: Arc<LifecycleShutdownBarrier>,
) {
    tauri::async_runtime::spawn(async move {
        let profiles = match with_hub_blocking(hub.clone(), |hub| {
            hub.adapter_bridge
                .list_auto_start_profiles()
                .map_err(|error| map_err_string("list_adapter_bridge_restore", error))
        })
        .await
        {
            Ok(profiles) => profiles,
            Err(_) => {
                tracing::warn!(target: "gui", op = "adapter_bridge_restore", code = "adapter.bridge_restore_list", "adapter bridge restore list failed");
                return;
            }
        };

        for profile in restorable_profiles(profiles) {
            let _lifecycle_permit = match lifecycle_barrier.enter().await {
                Ok(permit) => permit,
                Err(_) => return,
            };
            let _profile_guard = coordinator.lock_profile(&profile.id).await;
            let profile_id = profile.id.clone();
            let material = match with_hub_blocking(hub.clone(), move |hub| {
                hub.adapter_bridge
                    .resolve_restore_material(&profile_id)
                    .map_err(|error| map_err_string("resolve_adapter_bridge_restore", error))
            })
            .await
            {
                Ok(material) => material,
                Err(_) => {
                    mark_retryable(hub.clone(), &profile.id, CODE_BRIDGE_RESTORE_SOURCE).await;
                    tracing::warn!(target: "gui", op = "adapter_bridge_restore", profile_id = %profile.id, code = CODE_BRIDGE_RESTORE_SOURCE, "adapter bridge source could not be restored");
                    continue;
                }
            };

            let runtime = match ensure_bridge_listener(host.as_ref(), material.runtime_material())
                .await
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let code = if matches!(error, BridgeHostError::Bind(_)) {
                        CODE_BRIDGE_PORT_IN_USE
                    } else {
                        CODE_BRIDGE_RESTORE_START
                    };
                    mark_retryable(hub.clone(), &profile.id, code).await;
                    tracing::warn!(target: "gui", op = "adapter_bridge_restore", profile_id = %profile.id, code, "adapter bridge listener could not be restored");
                    continue;
                }
            };

            if let Err(error) = material
                .runtime_material()
                .verify_bound_health(runtime.status.port)
                .await
            {
                let code = error.code().to_owned();
                let listener_compensated =
                    compensate_started_bridge(&host, &profile.id, runtime.owned_by_saga).await;
                if listener_compensated {
                    mark_retryable(hub.clone(), &profile.id, &code).await;
                } else {
                    mark_needs_attention(hub.clone(), &profile.id, "adapter.bridge_stop").await;
                }
                tracing::warn!(target: "gui", op = "adapter_bridge_restore", profile_id = %profile.id, code = %code, "adapter bridge health check failed after restore");
                continue;
            }

            // Preferred-port rebind must rewrite profile.local_port and the
            // generated Codex base_url; otherwise restore leaves a dead endpoint.
            if Some(runtime.status.port) != profile.local_port {
                let _target_guard = coordinator.lock_target(AgentId::Codex).await;
                if let Err(error) = with_hub_blocking(hub.clone(), {
                    let profile_id = profile.id.clone();
                    let port = runtime.status.port;
                    move |hub| {
                        realign_restored_bridge_port(hub, &profile_id, port)
                    }
                })
                .await
                {
                    let _ = compensate_started_bridge(&host, &profile.id, runtime.owned_by_saga).await;
                    mark_retryable(hub.clone(), &profile.id, CODE_BRIDGE_PROJECTION).await;
                    tracing::warn!(target: "gui", op = "adapter_bridge_restore", profile_id = %profile.id, error = %error, "bridge rebound to a new port but provider projection could not be realigned");
                    continue;
                }
            } else if let Err(error) = with_hub_blocking(hub.clone(), {
                let profile_id = profile.id.clone();
                move |hub| {
                    hub.adapter_bridge
                        .clear_retryable_error(&profile_id)
                        .map(|_| ())
                        .map_err(|error| map_err_string("clear_adapter_bridge_retryable", error))
                }
            })
            .await
            {
                tracing::warn!(target: "gui", op = "adapter_bridge_restore", profile_id = %profile.id, error = %error, "healthy bridge restored but retryable marker could not be cleared");
            }
        }
    });
}

fn persist_bridge_projection_inner(
    hub: &AgentHub,
    core_guard: &ProviderLiveSagaGuard<'_>,
    prepared: &AdapterBridgePrepared,
    projection: AdapterBridgeProviderProjection,
    port: u16,
    snapshot: &BridgeProviderSnapshot,
) -> Result<AdapterApplyResult, String> {
    let provider_id = prepared
        .profile()
        .generated_provider_id
        .as_deref()
        .ok_or_else(|| "adapter bridge profile has no generated provider".to_string())?
        .to_owned();

    let created = match projection {
        AdapterBridgeProviderProjection::Create(input) => {
            hub.providers
                .create_with_guard(core_guard, &input)
                .map_err(|error| map_err_string("create_adapter_bridge_provider", error))?;
            true
        }
        AdapterBridgeProviderProjection::Update(input) => {
            hub.providers
                .update_with_guard(core_guard, &input)
                .map_err(|error| map_err_string("update_adapter_bridge_provider", error))?;
            false
        }
        AdapterBridgeProviderProjection::None => false,
    };

    let switched = match hub
        .providers
        .switch_with_guard(core_guard, &provider_id, AgentId::Codex)
    {
        Ok(result) => result,
        Err(error) => {
            let rollback =
                rollback_bridge_projection(hub, core_guard, &provider_id, snapshot, created);
            return Err(composite_saga_error(
                "switch_adapter_bridge_provider",
                map_err_string("switch_adapter_bridge_provider", error),
                rollback,
            ));
        }
    };
    let profile = match hub.adapter_bridge.finalize(prepared, port) {
        Ok(profile) => profile,
        Err(error) => {
            let rollback =
                rollback_bridge_projection(hub, core_guard, &provider_id, snapshot, created);
            return Err(composite_saga_error(
                "finalize_adapter_bridge",
                map_err_string("finalize_adapter_bridge", error),
                rollback,
            ));
        }
    };
    Ok(AdapterApplyResult {
        profile,
        provider: switched.provider.redacted(),
    })
}

#[derive(Clone)]
struct BridgeProviderSnapshot {
    generated: Option<Provider>,
    current_codex: Option<Provider>,
    live_codex: ProviderLiveConfigSnapshot,
}

fn capture_provider_snapshot(
    hub: &AgentHub,
    core_guard: &ProviderLiveSagaGuard<'_>,
    generated_provider_id: Option<&str>,
) -> Result<BridgeProviderSnapshot, String> {
    let generated = match generated_provider_id {
        Some(id) => hub
            .providers
            .repo()
            .get_by_id(id)
            .map_err(|error| map_err_string("snapshot_adapter_bridge_provider", error))?,
        None => None,
    };
    let current_codex = hub
        .providers
        .repo()
        .get_current(AgentId::Codex)
        .map_err(|error| map_err_string("snapshot_adapter_bridge_provider", error))?;
    let live_codex = hub
        .providers
        .capture_live_config_snapshot_with_guard(core_guard, AgentId::Codex)
        .map_err(|error| map_err_string("snapshot_adapter_bridge_live_config", error))?;
    Ok(BridgeProviderSnapshot {
        generated,
        current_codex,
        live_codex,
    })
}

/// Restore the persisted provider pool through `ProviderService`; it is the
/// sole live-config owner.  We intentionally try every inverse action so a
/// retry starts from the best possible state, then return a stable code only.
fn rollback_bridge_projection(
    hub: &AgentHub,
    core_guard: &ProviderLiveSagaGuard<'_>,
    provider_id: &str,
    snapshot: &BridgeProviderSnapshot,
    created: bool,
) -> Result<(), &'static str> {
    let mut failed = false;

    if let Some(old) = &snapshot.generated {
        let input = provider_to_non_current_input(old);
        if hub.providers.update_with_guard(core_guard, &input).is_err() {
            failed = true;
        }
    } else if created
        && hub
            .providers
            .delete_with_guard(core_guard, provider_id, AgentId::Codex)
            .is_err()
    {
        failed = true;
    }

    if let Some(old_current) = &snapshot.current_codex {
        if hub
            .providers
            .switch_with_guard(core_guard, &old_current.id, AgentId::Codex)
            .is_err()
        {
            failed = true;
        }
    }

    // A provider switch back to an old current row may backfill a drifted
    // value rather than the byte-exact snapshot captured before this saga.
    // Always restore the snapshot last, whether or not there was an old
    // current provider, so failed finalize/switch cannot leave live Codex
    // config changed.
    if hub
        .providers
        .restore_live_config_snapshot_with_guard(core_guard, &snapshot.live_codex)
        .is_err()
    {
        failed = true;
    }

    if failed {
        Err("adapter.bridge_rollback")
    } else {
        Ok(())
    }
}

fn provider_to_non_current_input(provider: &Provider) -> ProviderInput {
    ProviderInput {
        id: provider.id.clone(),
        agent_id: provider.agent_id,
        name: provider.name.clone(),
        settings_config: provider.settings_config.clone(),
        meta: provider.meta.clone(),
        // Restore the saved pool row before asking ProviderService to select
        // the pre-saga current provider (if there was one).
        is_current: false,
    }
}

fn composite_saga_error(
    operation: &str,
    original: String,
    rollback: Result<(), &'static str>,
) -> String {
    match rollback {
        Ok(()) => original,
        Err(code) => format!("{operation} failed and compensation was incomplete [{code}]"),
    }
}

async fn load_adapter_profile(
    hub: Arc<AgentHub>,
    profile_id: String,
) -> Result<AdapterProfile, String> {
    with_hub_blocking(hub, move |hub| {
        hub.adapter_apply
            .list(None, None, None)
            .map_err(|error| map_err_string("list_adapter_profiles", error))?
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| format!("adapter profile not found: {profile_id}"))
    })
    .await
}

async fn load_bridge_profile(
    hub: Arc<AgentHub>,
    profile_id: String,
) -> Result<AdapterProfile, String> {
    let profile = load_adapter_profile(hub, profile_id).await?;
    if profile.route != AdapterRoute::LocalBridge
        || profile.target_agent_id != AgentId::Codex
        || profile.source_kind != AdapterSourceKind::Provider
    {
        return Err("adapter profile is not a supported local bridge".into());
    }
    Ok(profile)
}

async fn mark_needs_attention(hub: Arc<AgentHub>, profile_id: &str, code: &str) {
    let profile_id = profile_id.to_owned();
    let code = code.to_owned();
    let operation_profile_id = profile_id.clone();
    let operation_code = code.clone();
    if with_hub_blocking(hub, move |hub| {
        hub.adapter_bridge
            .mark_needs_attention(&operation_profile_id, &operation_code)
            .map(|_| ())
            .map_err(|error| map_err_string("mark_adapter_bridge_needs_attention", error))
    })
    .await
    .is_err()
    {
        tracing::warn!(target: "gui", op = "adapter_bridge_attention", profile_id = %profile_id, code = %code, "adapter bridge profile failure could not be persisted");
    }
}

async fn mark_retryable(hub: Arc<AgentHub>, profile_id: &str, code: &str) {
    let profile_id = profile_id.to_owned();
    let code = code.to_owned();
    let operation_profile_id = profile_id.clone();
    let operation_code = code.clone();
    if with_hub_blocking(hub, move |hub| {
        hub.adapter_bridge
            .mark_retryable(&operation_profile_id, &operation_code)
            .map(|_| ())
            .map_err(|error| map_err_string("mark_adapter_bridge_retryable", error))
    })
    .await
    .is_err()
    {
        tracing::warn!(target: "gui", op = "adapter_bridge_retryable", profile_id = %profile_id, code = %code, "adapter bridge transient failure could not be persisted");
    }
}

async fn compensate_started_bridge(
    host: &BridgeRuntimeHost,
    profile_id: &str,
    should_stop: bool,
) -> bool {
    if !should_stop {
        return true;
    }
    if let Err(error) = host.stop(profile_id).await {
        if !matches!(error, BridgeHostError::NotRunning) {
            tracing::warn!(target: "gui", op = "adapter_bridge_compensate", profile_id, code = "adapter.bridge_stop", "adapter bridge compensation stop failed");
            return false;
        }
    }
    true
}

pub(crate) struct EnsuredBridgeListener {
    pub(crate) status: BridgeRuntimeStatus,
    /// True when this saga started or replaced the listener and must stop it
    /// on later failure. False when an already-running identical spec was reused.
    pub(crate) owned_by_saga: bool,
}

/// Start (or refresh) a loopback listener for one profile.
///
/// - Identical running specs are reused without ownership.
/// - `ConflictingStart` (token/port drift) stops the old listener then starts
///   with the new material so credential rotation can take effect.
/// - `Bind` on the preferred port retries once with port `0`.
pub(crate) async fn ensure_bridge_listener(
    host: &BridgeRuntimeHost,
    material: &agenthub_core::services::AdapterBridgeRuntimeMaterial,
) -> Result<EnsuredBridgeListener, BridgeHostError> {
    let profile_id = material.profile_id().to_owned();
    let had_running = host
        .status(&profile_id)?
        .is_some_and(|status| status.running);
    let preferred = material.start_spec(None);
    match host.start(preferred).await {
        Ok(status) => {
            // host.start is idempotent for an identical live instance; ownership
            // only attaches when we did not already have a running listener.
            Ok(EnsuredBridgeListener {
                owned_by_saga: !had_running,
                status,
            })
        }
        Err(BridgeHostError::ConflictingStart) => {
            match host.stop(&profile_id).await {
                Ok(_) | Err(BridgeHostError::NotRunning) => {}
                Err(error) => return Err(error),
            }
            let status = start_with_bind_fallback(host, material).await?;
            Ok(EnsuredBridgeListener {
                status,
                owned_by_saga: true,
            })
        }
        Err(BridgeHostError::Bind(_)) => {
            let status = host.start(material.start_spec(Some(0))).await?;
            Ok(EnsuredBridgeListener {
                status,
                owned_by_saga: true,
            })
        }
        Err(error) => Err(error),
    }
}

async fn start_with_bind_fallback(
    host: &BridgeRuntimeHost,
    material: &agenthub_core::services::AdapterBridgeRuntimeMaterial,
) -> Result<BridgeRuntimeStatus, BridgeHostError> {
    match host.start(material.start_spec(None)).await {
        Ok(status) => Ok(status),
        Err(BridgeHostError::Bind(_)) => host.start(material.start_spec(Some(0))).await,
        Err(error) => Err(error),
    }
}

fn realign_restored_bridge_port(hub: &AgentHub, profile_id: &str, port: u16) -> Result<(), String> {
    let core_guard = hub
        .providers
        .begin_live_saga(AgentId::Codex)
        .map_err(|error| map_err_string("begin_adapter_bridge_restore_rebind_saga", error))?;
    let (input, was_current) = hub
        .adapter_bridge
        .projection_for_restored_port(profile_id, port)
        .map_err(|error| map_err_string("projection_adapter_bridge_restore_port", error))?;
    hub.providers
        .update_with_guard(&core_guard, &input)
        .map_err(|error| map_err_string("update_adapter_bridge_restore_port", error))?;
    if was_current {
        hub.providers
            .switch_with_guard(&core_guard, &input.id, AgentId::Codex)
            .map_err(|error| map_err_string("switch_adapter_bridge_restore_port", error))?;
    }
    hub.adapter_bridge
        .persist_restored_port(profile_id, port)
        .map_err(|error| map_err_string("persist_adapter_bridge_restore_port", error))?;
    Ok(())
}

async fn stop_bridge_runtime(
    host: &BridgeRuntimeHost,
    profile: &AdapterProfile,
) -> Result<BridgeRuntimeStatus, String> {
    match host.stop(&profile.id).await {
        Ok(status) => Ok(status),
        Err(BridgeHostError::NotRunning) => Ok(stopped_runtime_status(profile)),
        Err(BridgeHostError::Stopping) => Ok(host
            .status(&profile.id)
            .map_err(map_bridge_host_error)?
            .unwrap_or_else(|| stopped_runtime_status(profile))),
        Err(error) => Err(map_bridge_host_error(error)),
    }
}

fn stopped_runtime_status(profile: &AdapterProfile) -> BridgeRuntimeStatus {
    BridgeRuntimeStatus {
        profile_id: profile.id.clone(),
        port: profile.local_port.unwrap_or_default(),
        running: false,
        started_at: SystemTime::now(),
        source_connection_id: Some(profile.source_id.clone()),
        state: BridgeRuntimeState::Stopped,
        upstream_status: BridgeUpstreamStatus::Unknown,
    }
}

fn restorable_profiles(profiles: Vec<AdapterProfile>) -> Vec<AdapterProfile> {
    profiles
        .into_iter()
        .filter(|profile| {
            profile.route == AdapterRoute::LocalBridge
                && profile.status == AdapterProfileStatus::Active
                && profile.auto_start
        })
        .collect()
}

async fn bridge_profile_id_for_request(
    hub: Arc<AgentHub>,
    request: AdapterBridgePrepareRequest,
) -> Result<String, String> {
    with_hub_blocking(hub, move |hub| {
        hub.adapter_bridge
            .profile_id_for_request(&request)
            .map_err(|error| map_err_string("adapter_bridge_profile_id", error))
    })
    .await
}

fn map_bridge_host_error(error: BridgeHostError) -> String {
    // Host error implementations intentionally contain no bearer; still use a
    // stable GUI-facing code and do not serialize the Debug representation.
    format!("本地适配服务无法启动或停止 [{CODE_BRIDGE_START}]: {error}")
}

fn runtime_state_name(state: BridgeRuntimeState) -> &'static str {
    match state {
        BridgeRuntimeState::Starting => "starting",
        BridgeRuntimeState::Running => "running",
        BridgeRuntimeState::Stopping => "stopping",
        BridgeRuntimeState::Stopped => "stopped",
        BridgeRuntimeState::Error => "error",
        BridgeRuntimeState::Degraded => "degraded",
    }
}

fn upstream_status_name(status: BridgeUpstreamStatus) -> &'static str {
    match status {
        BridgeUpstreamStatus::Unknown => "unknown",
    }
}

fn system_time_millis(value: SystemTime) -> Option<u128> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|value| value.as_millis())
}

#[cfg(test)]
mod tests;
