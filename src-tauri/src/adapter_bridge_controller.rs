//! Desktop-process control plane for Adapter local bridges.
//!
//! `agenthub-core` owns profile persistence, source-secret resolution and the
//! generated target provider projection.  This module owns the cross-boundary
//! saga: the loopback listener must bind before the generated provider is
//! persisted, and a failed apply must not leave a newly-started listener
//! running. Generated loopback is persisted non-current; live config is
//! refreshed only if that row is already current. Neither upstream nor local
//! bearer tokens cross the Tauri command boundary.
//!
//! Process-local profile / target gates and the credential-free status DTO live
//! in [`agenthub_core::adapter_control`] so commands stay Tauri-neutral.

use std::sync::Arc;
use std::time::SystemTime;

use agenthub_core::adapter_control::{surface_unbind_and_restart, AdapterBridgeStatus};
use agenthub_core::bridge::{
    BridgeHostError, BridgeMemberSpec, BridgeRuntimeHost, BridgeRuntimeState, BridgeRuntimeStatus,
    BridgeUpstreamStatus, MemberHealth, UpstreamAuthReload,
};
use agenthub_core::models::{
    local_bridge_multi_account, ticket_id, AdapterApplyResult, AdapterProfile,
    AdapterProfileStatus, AdapterRoute, AdapterSourceKind, AgentId,
};
use agenthub_core::services::{
    oauth_bridge_reload_callback, AdapterBridgePrepareRequest, AdapterBridgePrepared,
    AdapterBridgeProviderProjection, AdapterBridgeRuntimeMaterial, BridgeProviderSnapshot,
    ProviderLiveSagaGuard,
};

#[cfg(test)]
#[allow(unused_imports)]
use agenthub_core::services::should_make_bridge_current;
use agenthub_core::AgentHub;

use crate::commands::{map_err_string, with_hub_blocking};
use crate::exit_coordinator::LifecycleShutdownBarrier;

const CODE_BRIDGE_START: &str = "adapter.bridge_start";
const CODE_BRIDGE_PROJECTION: &str = "adapter.bridge_projection";
const CODE_BRIDGE_FINALIZE: &str = "adapter.bridge_finalize";
const CODE_BRIDGE_RESTORE_SOURCE: &str = "adapter.bridge_restore_source";
const CODE_BRIDGE_RESTORE_START: &str = "adapter.bridge_restore_start";
const CODE_BRIDGE_PORT_IN_USE: &str = "adapter.port_in_use";

/// Process-local saga gates (Tauri-neutral; defined in core).
pub(crate) use agenthub_core::adapter_control::AdapterBridgeSagaCoordinator;

/// Wire / test alias for the core credential-free bridge status DTO.
pub(crate) type AdapterBridgeStatusDto = AdapterBridgeStatus;

/// Apply a supported local bridge through the desktop host saga.
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
    let target_agent_id = request.target_agent_id;
    let prepared = with_hub_blocking(hub.clone(), move |hub| {
        hub.adapter_bridge()
            .prepare(&request)
            .map_err(|error| map_err_string("prepare_adapter_bridge", error))
    })
    .await?;

    let profile_id = prepared.profile().id.clone();
    let mut runtime_material = attach_live_prior_index(
        hub.clone(),
        host.as_ref(),
        prepared.profile().clone(),
        prepared.runtime_material().clone(),
    )
    .await?;
    let reload = oauth_reload_for_material(
        hub.as_ref(),
        &runtime_material,
        prepared.profile().source_kind,
        &prepared.profile().source_id,
    );
    let members = resolve_start_members(
        hub.as_ref(),
        prepared.profile(),
        &runtime_material,
        reload.clone(),
    );
    let multi_account = local_bridge_multi_account(&prepared.profile().rule_id);
    let runtime = match ensure_bridge_listener(
        host.as_ref(),
        &runtime_material,
        reload.clone(),
        members,
        multi_account,
    )
    .await
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let code = if matches!(error, BridgeHostError::Bind(_)) {
                CODE_BRIDGE_PORT_IN_USE
            } else {
                CODE_BRIDGE_START
            };
            mark_retryable(hub, &profile_id, code).await;
            tracing::error!(
                target: "core.adapter",
                op = "start",
                profile_id = %profile_id,
                code,
                "本机转发启动失败"
            );
            return Err(map_bridge_host_error(error));
        }
    };
    // Own any listener this saga started or replaced. An idempotent reuse of an
    // already-running identical spec is not compensated on later projection failure.
    let mut owns_listener = runtime.owned_by_saga;
    let mut port = runtime.status.port;
    if let Err(error) = runtime_material.verify_bound_health(port).await {
        let _ = host.record_upstream_outcome(&profile_id, BridgeUpstreamStatus::Degraded);
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
    let _ = host.record_upstream_outcome(&profile_id, BridgeUpstreamStatus::Connected);

    match enroll_v2_and_refresh_index(
        hub.clone(),
        host.as_ref(),
        prepared.profile().clone(),
        runtime_material,
        port,
        reload,
        multi_account,
    )
    .await
    {
        Ok(refresh) => {
            refresh.fold_into(&mut owns_listener, &mut port);
        }
        Err(error) => {
            let listener_compensated =
                compensate_started_bridge(&host, &profile_id, owns_listener).await;
            if listener_compensated {
                mark_retryable(hub, &profile_id, CODE_BRIDGE_START).await;
            } else {
                mark_needs_attention(hub, &profile_id, "adapter.bridge_stop").await;
            }
            return Err(error);
        }
    }

    // Keep the same Tauri target authority as every configuration/account/
    // provider mutation. Inside the blocking critical section, the Core guard
    // retains the cross-process switch lock from snapshot through
    // projection/finalize/rollback; do not call ordinary lock-taking provider
    // APIs while it is held.
    let target_agent = prepared.profile().target_agent_id;
    let _target_guard = coordinator.lock_target(target_agent).await;
    let result = with_hub_blocking(hub.clone(), move |hub| {
        let core_guard = hub
            .providers()
            .begin_live_saga(target_agent)
            .map_err(|error| map_err_string("begin_adapter_bridge_provider_saga", error))?;
        let projection = hub
            .adapter_bridge()
            .revalidate_provider_projection(&prepared, port)
            .map_err(|error| map_err_string("revalidate_adapter_bridge_provider", error))?;
        let provider_id = prepared.profile().generated_provider_id.clone();
        let snapshot =
            capture_provider_snapshot(hub, &core_guard, provider_id.as_deref(), target_agent)?;
        persist_bridge_projection_inner(hub, &core_guard, &prepared, projection, port, &snapshot)
    })
    .await;

    match result {
        Ok(result) => {
            tracing::info!(
                target: "core.adapter",
                op = "apply_bridge",
                profile_id = %result.profile.id,
                agent = result.profile.target_agent_id.as_str(),
                route = "local_bridge",
                port = port,
                "local bridge applied"
            );
            Ok(result)
        }
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
            Err(map_bridge_apply_error(&error, target_agent_id))
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
    tracing::info!(
        target: "core.adapter",
        op = "start",
        profile_id = %applied.profile.id,
        agent = applied.profile.target_agent_id.as_str(),
        route = "local_bridge",
        port = status.port,
        "local bridge started"
    );
    Ok(status_dto(
        &host,
        &applied.profile.id,
        AdapterBridgeStatusDto::from_runtime(status),
    ))
}

/// Stop the listener, restore previous live, and delete the projection while
/// holding profile + target locks. If restore/delete fails, restart the
/// listener so the Agent is not left pointing at a dead port.
pub(crate) async fn unbind_local_bridge(
    hub: Arc<AgentHub>,
    host: Arc<BridgeRuntimeHost>,
    coordinator: Arc<AdapterBridgeSagaCoordinator>,
    lifecycle_barrier: Arc<LifecycleShutdownBarrier>,
    profile_id: String,
    request: agenthub_core::models::TicketUnbindRequest,
) -> Result<(), String> {
    let _lifecycle_permit = lifecycle_barrier.enter().await?;
    let _profile_guard = coordinator.lock_profile(&profile_id).await;
    let profile = load_bridge_profile(hub.clone(), profile_id.clone()).await?;
    let target_guard = coordinator.lock_target(profile.target_agent_id).await;
    stop_bridge_runtime(&host, &profile).await?;
    let unbind_hub = hub.clone();
    let result = with_hub_blocking(unbind_hub, move |hub| {
        hub.ticket_bind()
            .unbind(&request)
            .map_err(|error| map_err_string("unbind_ticket", error))
    })
    .await;
    if let Err(error) = result {
        // apply_local_bridge_locked takes the same target mutex; drop first or
        // this task deadlocks on the non-reentrant tokio Mutex.
        drop(target_guard);
        let restart = AdapterBridgePrepareRequest {
            source_kind: profile.source_kind,
            source_id: profile.source_id.clone(),
            target_agent_id: profile.target_agent_id,
            auto_start: profile.auto_start,
        };
        let restart = apply_local_bridge_locked(hub, host, coordinator, restart)
            .await
            .map(|_| ());
        tracing::error!(
            target: "core.adapter",
            op = "unbind",
            profile_id = %profile_id,
            agent = profile.target_agent_id.as_str(),
            route = "local_bridge",
            "local bridge unbind failed; listener restart attempted"
        );
        return Err(surface_unbind_and_restart(error, restart));
    }
    tracing::info!(
        target: "core.adapter",
        op = "unbind",
        profile_id = %profile_id,
        agent = profile.target_agent_id.as_str(),
        route = "local_bridge",
        "local bridge unbound"
    );
    Ok(())
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
    tracing::info!(
        target: "core.adapter",
        op = "stop",
        profile_id = %profile.id,
        agent = profile.target_agent_id.as_str(),
        route = "local_bridge",
        "local bridge stopped"
    );
    Ok(status_dto(
        &host,
        &profile.id,
        AdapterBridgeStatusDto::from_runtime(status),
    ))
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
    Ok(status_dto(
        &host,
        &profile.id,
        status
            .map(AdapterBridgeStatusDto::from_runtime)
            .unwrap_or_else(|| AdapterBridgeStatusDto::stopped(&profile)),
    ))
}

fn status_dto(
    host: &BridgeRuntimeHost,
    profile_id: &str,
    dto: AdapterBridgeStatusDto,
) -> AdapterBridgeStatusDto {
    let token = host.local_token(profile_id).ok().flatten();
    dto.with_recent_inbound(host.recent_inbound(profile_id))
        .with_local_token(token)
}

/// Persist an existing bridge profile's auto-start preference.  It controls
/// desktop startup restoration only; it does not start or stop a listener.
pub(crate) async fn set_local_bridge_auto_start(
    hub: Arc<AgentHub>,
    profile_id: String,
    auto_start: bool,
) -> Result<AdapterProfile, String> {
    with_hub_blocking(hub, move |hub| {
        hub.adapter_bridge()
            .set_auto_start(&profile_id, auto_start)
            .map_err(|error| map_err_string("set_adapter_bridge_auto_start", error))
    })
    .await
}

/// Remove an adapter profile. Local bridges stop first, then `unbind`
/// restores previous live (including current) and deletes the projection.
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
    let target_guard = coordinator.lock_target(profile.target_agent_id).await;
    if profile.route != AdapterRoute::LocalBridge {
        return with_hub_blocking(hub, move |hub| {
            hub.adapter_apply()
                .remove(&profile_id)
                .map_err(|error| map_err_string("remove_adapter", error))
        })
        .await;
    }

    // Stop outside the Core live-saga critical section so listener drain does
    // not hold the cross-process provider lock. Current bindings are allowed:
    // unbind restores previous live before deleting the projection.
    stop_bridge_runtime(&host, &profile).await?;
    let ticket_id = agenthub_core::models::ticket_id(profile.source_kind, &profile.source_id);
    let agent_id = profile.target_agent_id;
    let result = with_hub_blocking(hub.clone(), move |hub| {
        hub.ticket_bind()
            .unbind(&agenthub_core::models::TicketUnbindRequest {
                ticket_id,
                agent_id,
            })
            .map_err(|error| map_err_string("unbind_ticket", error))
    })
    .await;
    if let Err(error) = result {
        // apply_local_bridge_locked takes the same target mutex; drop first or
        // this task deadlocks on the non-reentrant tokio Mutex.
        drop(target_guard);
        let restart = AdapterBridgePrepareRequest {
            source_kind: profile.source_kind,
            source_id: profile.source_id.clone(),
            target_agent_id: profile.target_agent_id,
            auto_start: profile.auto_start,
        };
        let restart = apply_local_bridge_locked(hub, host, coordinator, restart)
            .await
            .map(|_| ());
        return Err(surface_unbind_and_restart(error, restart));
    }
    Ok(())
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
            hub.adapter_bridge()
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
                hub.adapter_bridge()
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

            let mut runtime_material = match attach_live_prior_index(
                hub.clone(),
                host.as_ref(),
                material.profile().clone(),
                material.runtime_material().clone(),
            )
            .await
            {
                Ok(runtime_material) => runtime_material,
                Err(error) => {
                    mark_retryable(hub.clone(), &profile.id, CODE_BRIDGE_RESTORE_START).await;
                    tracing::warn!(target: "gui", op = "adapter_bridge_restore", profile_id = %profile.id, error = %error, "adapter bridge restore could not attach live index");
                    continue;
                }
            };
            let reload = oauth_reload_for_material(
                hub.as_ref(),
                &runtime_material,
                material.profile().source_kind,
                &material.profile().source_id,
            );
            let members = resolve_start_members(
                hub.as_ref(),
                material.profile(),
                &runtime_material,
                reload.clone(),
            );
            let multi_account = local_bridge_multi_account(&material.profile().rule_id);
            let runtime = match ensure_bridge_listener(
                host.as_ref(),
                &runtime_material,
                reload.clone(),
                members,
                multi_account,
            )
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

            let mut owns_listener = runtime.owned_by_saga;
            let mut port = runtime.status.port;
            if let Err(error) = runtime_material.verify_bound_health(port).await {
                let _ = host.record_upstream_outcome(&profile.id, BridgeUpstreamStatus::Degraded);
                let code = error.code().to_owned();
                let listener_compensated =
                    compensate_started_bridge(&host, &profile.id, owns_listener).await;
                if listener_compensated {
                    mark_retryable(hub.clone(), &profile.id, &code).await;
                } else {
                    mark_needs_attention(hub.clone(), &profile.id, "adapter.bridge_stop").await;
                }
                tracing::warn!(target: "gui", op = "adapter_bridge_restore", profile_id = %profile.id, code = %code, "adapter bridge health check failed after restore");
                continue;
            }
            let _ = host.record_upstream_outcome(&profile.id, BridgeUpstreamStatus::Connected);

            match enroll_v2_and_refresh_index(
                hub.clone(),
                host.as_ref(),
                material.profile().clone(),
                runtime_material,
                port,
                reload,
                multi_account,
            )
            .await
            {
                Ok(refresh) => {
                    refresh.fold_into(&mut owns_listener, &mut port);
                }
                Err(error) => {
                    let _ = compensate_started_bridge(&host, &profile.id, owns_listener).await;
                    mark_retryable(hub.clone(), &profile.id, CODE_BRIDGE_RESTORE_START).await;
                    tracing::warn!(target: "gui", op = "adapter_bridge_restore", profile_id = %profile.id, error = %error, "adapter bridge v2 enroll after restore failed");
                    continue;
                }
            }

            // Preferred-port rebind must rewrite profile.local_port and the
            // generated target endpoint; otherwise restore leaves a dead endpoint.
            if Some(port) != profile.local_port || material.needs_reprojection() {
                let _target_guard = coordinator.lock_target(profile.target_agent_id).await;
                if let Err(error) = with_hub_blocking(hub.clone(), {
                    let profile_id = profile.id.clone();
                    move |hub| realign_restored_bridge_port(hub, &profile_id, port)
                })
                .await
                {
                    let _ = compensate_started_bridge(&host, &profile.id, owns_listener).await;
                    mark_retryable(hub.clone(), &profile.id, CODE_BRIDGE_PROJECTION).await;
                    tracing::warn!(target: "gui", op = "adapter_bridge_restore", profile_id = %profile.id, error = %error, "bridge rebound to a new port but provider projection could not be realigned");
                    continue;
                }
            } else if let Err(error) = with_hub_blocking(hub.clone(), {
                let profile_id = profile.id.clone();
                move |hub| {
                    hub.adapter_bridge()
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
    hub.adapter_bridge().persist_bridge_projection_inner(
        hub.providers(),
        core_guard,
        prepared,
        projection,
        port,
        snapshot,
    )
}

fn capture_provider_snapshot(
    hub: &AgentHub,
    core_guard: &ProviderLiveSagaGuard<'_>,
    generated_provider_id: Option<&str>,
    target_agent: AgentId,
) -> Result<BridgeProviderSnapshot, String> {
    hub.adapter_bridge().capture_provider_snapshot(
        hub.providers(),
        core_guard,
        generated_provider_id,
        target_agent,
    )
}

/// Inverse of persist: restore the generated pool row. Reverse live switch
/// only when this saga actually refreshed current (`switched_live`).
#[allow(dead_code)]
fn rollback_bridge_projection(
    hub: &AgentHub,
    core_guard: &ProviderLiveSagaGuard<'_>,
    provider_id: &str,
    snapshot: &BridgeProviderSnapshot,
    created: bool,
    switched_live: bool,
    target_agent: AgentId,
) -> Result<(), &'static str> {
    hub.adapter_bridge().rollback_bridge_projection(
        hub.providers(),
        core_guard,
        provider_id,
        snapshot,
        created,
        switched_live,
        target_agent,
    )
}

async fn load_adapter_profile(
    hub: Arc<AgentHub>,
    profile_id: String,
) -> Result<AdapterProfile, String> {
    with_hub_blocking(hub, move |hub| {
        hub.adapter_apply()
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
        || !matches!(
            profile.target_agent_id,
            AgentId::Codex | AgentId::Claude | AgentId::Grok | AgentId::Kimi | AgentId::Dsh
        )
        || !matches!(
            profile.source_kind,
            AdapterSourceKind::Provider | AdapterSourceKind::Account
        )
    {
        return Err("这条本机路由已失效，无法启动。请删除后重建。".into());
    }
    Ok(profile)
}

async fn mark_needs_attention(hub: Arc<AgentHub>, profile_id: &str, code: &str) {
    let profile_id = profile_id.to_owned();
    let code = code.to_owned();
    let operation_profile_id = profile_id.clone();
    let operation_code = code.clone();
    if with_hub_blocking(hub, move |hub| {
        hub.adapter_bridge()
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
        hub.adapter_bridge()
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

pub(crate) struct EnrolledIndexRefresh {
    /// Indexed start spec after enroll. Tests chain a second enroll with this;
    /// production `fold_into` only takes listener ownership and port.
    #[allow(dead_code)]
    pub(crate) material: AdapterBridgeRuntimeMaterial,
    pub(crate) listener: Option<EnsuredBridgeListener>,
}

impl EnrolledIndexRefresh {
    fn material_only(material: AdapterBridgeRuntimeMaterial) -> Self {
        Self {
            material,
            listener: None,
        }
    }

    fn fold_into(self, owns_listener: &mut bool, port: &mut u16) {
        if let Some(listener) = self.listener {
            *owns_listener |= listener.owned_by_saga;
            *port = listener.status.port;
        }
    }
}

/// Start (or refresh) a loopback listener for one profile.
///
/// - Identical running specs are reused without ownership.
/// - `ConflictingStart` (token/port drift) stops the old listener then starts
///   with the new material so credential rotation can take effect.
/// - `Bind` on the preferred port retries once with port `0`.
fn oauth_reload_for_material(
    hub: &AgentHub,
    material: &AdapterBridgeRuntimeMaterial,
    source_kind: AdapterSourceKind,
    source_id: &str,
) -> Option<UpstreamAuthReload> {
    oauth_bridge_reload_callback(
        hub.accounts().clone(),
        hub.adapter_secret_resolver(),
        source_kind,
        source_id.to_owned(),
        material.protocol(),
    )
}

fn resolve_start_members(
    hub: &AgentHub,
    profile: &AdapterProfile,
    material: &AdapterBridgeRuntimeMaterial,
    lead_reload: Option<UpstreamAuthReload>,
) -> Vec<BridgeMemberSpec> {
    if let Some(members) = resolve_v2_pool_members(hub, profile, material, lead_reload.clone()) {
        return members;
    }
    resolve_pool_members(hub, profile, material, lead_reload)
}

/// Enrolled v2 pool members. Does not open `multi_account`; the start spec
/// keeps every member only because a route index is attached.
fn resolve_v2_pool_members(
    hub: &AgentHub,
    profile: &AdapterProfile,
    material: &AdapterBridgeRuntimeMaterial,
    lead_reload: Option<UpstreamAuthReload>,
) -> Option<Vec<BridgeMemberSpec>> {
    if material.route_index().is_none() {
        return None;
    }
    let members = hub.route_pools().list_members(&profile.id).ok()?;
    if members.is_empty() {
        return None;
    }
    let protocol = material.protocol();
    let mut resolved = Vec::with_capacity(members.len());
    for member in members.into_iter().filter(|member| member.enabled) {
        let is_lead =
            member.source_kind == profile.source_kind && member.source_id == profile.source_id;
        if is_lead {
            resolved.push(BridgeMemberSpec {
                ticket_id: ticket_id(member.source_kind, &member.source_id),
                source_kind: member.source_kind.as_str().to_owned(),
                source_id: member.source_id.clone(),
                label: member.source_id.clone(),
                auth: material.start_spec(None).upstream.auth,
                reload: lead_reload.clone(),
                health: MemberHealth::Renewable,
                priority: member.priority,
                position: member.position,
            });
            continue;
        }
        match hub.adapter_bridge().resolve_member_auth(
            &profile.rule_id,
            member.source_kind,
            &member.source_id,
        ) {
            Ok(auth) if auth.has_token() => {
                resolved.push(BridgeMemberSpec {
                    ticket_id: ticket_id(member.source_kind, &member.source_id),
                    source_kind: member.source_kind.as_str().to_owned(),
                    source_id: member.source_id.clone(),
                    label: member.source_id.clone(),
                    auth,
                    reload: oauth_bridge_reload_callback(
                        hub.accounts().clone(),
                        hub.adapter_secret_resolver(),
                        member.source_kind,
                        member.source_id.clone(),
                        protocol,
                    ),
                    health: MemberHealth::Renewable,
                    priority: member.priority,
                    position: member.position,
                });
            }
            _ => {
                tracing::info!(
                    target: "gui",
                    op = "adapter_bridge_member_isolated",
                    profile_id = %profile.id,
                    account_id = %member.source_id,
                    "isolating v2 pool member whose secret could not be resolved"
                );
                resolved.push(BridgeMemberSpec {
                    ticket_id: ticket_id(member.source_kind, &member.source_id),
                    source_kind: member.source_kind.as_str().to_owned(),
                    source_id: member.source_id.clone(),
                    label: member.source_id.clone(),
                    auth: agenthub_core::bridge::ResolvedAuth::bearer(""),
                    reload: oauth_bridge_reload_callback(
                        hub.accounts().clone(),
                        hub.adapter_secret_resolver(),
                        member.source_kind,
                        member.source_id.clone(),
                        protocol,
                    ),
                    health: MemberHealth::NeedsLogin,
                    priority: member.priority,
                    position: member.position,
                });
            }
        }
    }
    if !resolved
        .iter()
        .any(|member| member.health.is_eligible() && member.auth.has_token())
    {
        let source_id = material.source_connection_id().to_owned();
        resolved.insert(
            0,
            BridgeMemberSpec {
                ticket_id: ticket_id(profile.source_kind, &source_id),
                source_kind: profile.source_kind.as_str().to_owned(),
                source_id: source_id.clone(),
                label: source_id,
                auth: material.start_spec(None).upstream.auth,
                reload: lead_reload,
                health: MemberHealth::Renewable,
                priority: 0,
                position: 0,
            },
        );
    }
    Some(resolved)
}

/// Resolve C1 surface-group siblings. A closed multi_account gate returns an
/// empty list so the host synthesizes the lead (byte-equivalent start spec).
/// A sibling secret failure isolates that member instead of failing start.
fn resolve_pool_members(
    hub: &AgentHub,
    profile: &AdapterProfile,
    material: &AdapterBridgeRuntimeMaterial,
    lead_reload: Option<UpstreamAuthReload>,
) -> Vec<BridgeMemberSpec> {
    if !local_bridge_multi_account(&profile.rule_id) {
        return Vec::new();
    }
    let lead_ticket = ticket_id(profile.source_kind, &profile.source_id);
    let Ok(wallet) = hub.tickets().list_wallet() else {
        return Vec::new();
    };
    let Some(lead_ticket_row) = wallet
        .tickets
        .iter()
        .find(|ticket| ticket.id == lead_ticket)
    else {
        return Vec::new();
    };
    let Some(group) = wallet.surface_groups.iter().find(|group| {
        group.surface == lead_ticket_row.surface
            && group.credential_class == lead_ticket_row.credential_class
    }) else {
        return Vec::new();
    };

    let protocol = material.protocol();
    let mut members = Vec::with_capacity(group.members.len());
    for member in &group.members {
        if member.ticket_id == lead_ticket {
            members.push(BridgeMemberSpec {
                ticket_id: member.ticket_id.clone(),
                source_kind: member.source_kind.as_str().to_owned(),
                source_id: member.source_id.clone(),
                label: member.label.clone(),
                auth: material.start_spec(None).upstream.auth,
                reload: lead_reload.clone(),
                health: MemberHealth::Renewable,
                priority: 0,
                position: members.len() as i64,
            });
            continue;
        }
        match hub.adapter_bridge().resolve_member_auth(
            &profile.rule_id,
            member.source_kind,
            &member.source_id,
        ) {
            Ok(auth) if auth.has_token() => {
                members.push(BridgeMemberSpec {
                    ticket_id: member.ticket_id.clone(),
                    source_kind: member.source_kind.as_str().to_owned(),
                    source_id: member.source_id.clone(),
                    label: member.label.clone(),
                    auth,
                    reload: oauth_bridge_reload_callback(
                        hub.accounts().clone(),
                        hub.adapter_secret_resolver(),
                        member.source_kind,
                        member.source_id.clone(),
                        protocol,
                    ),
                    health: MemberHealth::Renewable,
                    priority: 0,
                    position: members.len() as i64,
                });
            }
            _ => {
                tracing::info!(
                    target: "gui",
                    op = "adapter_bridge_member_isolated",
                    profile_id = %profile.id,
                    account_id = %member.source_id,
                    "isolating pool member whose secret could not be resolved"
                );
                members.push(BridgeMemberSpec {
                    ticket_id: member.ticket_id.clone(),
                    source_kind: member.source_kind.as_str().to_owned(),
                    source_id: member.source_id.clone(),
                    label: member.label.clone(),
                    auth: agenthub_core::bridge::ResolvedAuth::bearer(""),
                    reload: oauth_bridge_reload_callback(
                        hub.accounts().clone(),
                        hub.adapter_secret_resolver(),
                        member.source_kind,
                        member.source_id.clone(),
                        protocol,
                    ),
                    health: MemberHealth::NeedsLogin,
                    priority: 0,
                    position: members.len() as i64,
                });
            }
        }
    }
    members
}

async fn attach_live_prior_index(
    hub: Arc<AgentHub>,
    host: &BridgeRuntimeHost,
    profile: AdapterProfile,
    material: AdapterBridgeRuntimeMaterial,
) -> Result<AdapterBridgeRuntimeMaterial, String> {
    let seeded = match host.live_route_index(material.profile_id()) {
        Ok(Some(live)) => material.with_prior_route_index(live),
        Ok(None) => return Ok(material),
        Err(error) => return Err(map_bridge_host_error(error)),
    };
    with_hub_blocking(hub, move |hub| {
        hub.adapter_bridge()
            .attach_route_index(seeded, &profile)
            .map_err(|error| map_err_string("attach_adapter_bridge_route_index", error))
    })
    .await
}

fn seed_prior_from_host(
    host: &BridgeRuntimeHost,
    material: AdapterBridgeRuntimeMaterial,
) -> Result<AdapterBridgeRuntimeMaterial, String> {
    match host.live_route_index(material.profile_id()) {
        Ok(Some(live)) => Ok(material.with_prior_route_index(live)),
        Ok(None) => Ok(material),
        Err(error) => Err(map_bridge_host_error(error)),
    }
}

/// After a healthy bind, enroll the local-bridge pool and refresh the live
/// start spec with the v2 index. Bind / health failures must not call this.
/// Refresh uses the enrolled port and never falls back to port 0.
/// A replacement listener is returned so later compensation can stop it.
pub(crate) async fn enroll_v2_and_refresh_index(
    hub: Arc<AgentHub>,
    host: &BridgeRuntimeHost,
    profile: AdapterProfile,
    material: AdapterBridgeRuntimeMaterial,
    port: u16,
    reload: Option<UpstreamAuthReload>,
    multi_account: bool,
) -> Result<EnrolledIndexRefresh, String> {
    let profile_for_enroll = profile.clone();
    let did_enroll = with_hub_blocking(hub.clone(), move |hub| {
        hub.adapter_bridge()
            .enroll_v2_after_bind(&profile_for_enroll, port)
            .map_err(|error| map_err_string("enroll_adapter_bridge_v2", error))
    })
    .await?;
    if !did_enroll {
        return Ok(EnrolledIndexRefresh::material_only(material));
    }
    let seeded = seed_prior_from_host(host, material)?;
    let profile_for_attach = profile.clone();
    let refreshed = with_hub_blocking(hub.clone(), move |hub| {
        hub.adapter_bridge()
            .attach_route_index(seeded, &profile_for_attach)
            .map_err(|error| map_err_string("attach_adapter_bridge_route_index", error))
    })
    .await?;
    if refreshed.route_index().is_none() {
        return Ok(EnrolledIndexRefresh::material_only(refreshed));
    }
    let members = resolve_start_members(hub.as_ref(), &profile, &refreshed, reload.clone());
    let listener = ensure_bridge_listener(host, &refreshed, reload, members, multi_account)
        .await
        .map_err(map_bridge_host_error)?;
    Ok(EnrolledIndexRefresh {
        material: refreshed,
        listener: Some(listener),
    })
}

/// Start (or refresh) a loopback listener for one profile.
///
/// Identical running specs are reused. `ConflictingStart` stops then restarts.
/// `Bind` on the preferred port retries with port `0` unless
/// [`AdapterBridgeRuntimeMaterial::freeze_gateway_port`] is set.
pub(crate) async fn ensure_bridge_listener(
    host: &BridgeRuntimeHost,
    material: &AdapterBridgeRuntimeMaterial,
    reload: Option<UpstreamAuthReload>,
    members: Vec<BridgeMemberSpec>,
    multi_account: bool,
) -> Result<EnsuredBridgeListener, BridgeHostError> {
    let profile_id = material.profile_id().to_owned();
    let had_running = host
        .status(&profile_id)?
        .is_some_and(|status| status.running);
    let preferred = material
        .start_spec(None)
        .with_reload_upstream_auth(reload.clone())
        .with_members(members.clone())
        .with_multi_account(multi_account);
    match host.start(preferred).await {
        Ok(status) => Ok(EnsuredBridgeListener {
            owned_by_saga: !had_running,
            status,
        }),
        Err(BridgeHostError::ConflictingStart) => {
            match host.stop(&profile_id).await {
                Ok(_) | Err(BridgeHostError::NotRunning) => {}
                Err(error) => return Err(error),
            }
            let status =
                start_with_bind_fallback(host, material, reload, members, multi_account).await?;
            Ok(EnsuredBridgeListener {
                status,
                owned_by_saga: true,
            })
        }
        Err(error @ BridgeHostError::Bind(_)) => {
            if material.freeze_gateway_port() {
                return Err(error);
            }
            let status = host
                .start(
                    material
                        .start_spec(Some(0))
                        .with_reload_upstream_auth(reload)
                        .with_members(members)
                        .with_multi_account(multi_account),
                )
                .await?;
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
    material: &AdapterBridgeRuntimeMaterial,
    reload: Option<UpstreamAuthReload>,
    members: Vec<BridgeMemberSpec>,
    multi_account: bool,
) -> Result<BridgeRuntimeStatus, BridgeHostError> {
    match host
        .start(
            material
                .start_spec(None)
                .with_reload_upstream_auth(reload.clone())
                .with_members(members.clone())
                .with_multi_account(multi_account),
        )
        .await
    {
        Ok(status) => Ok(status),
        Err(error @ BridgeHostError::Bind(_)) => {
            if material.freeze_gateway_port() {
                return Err(error);
            }
            host.start(
                material
                    .start_spec(Some(0))
                    .with_reload_upstream_auth(reload)
                    .with_members(members)
                    .with_multi_account(multi_account),
            )
            .await
        }
        Err(error) => Err(error),
    }
}

fn realign_restored_bridge_port(hub: &AgentHub, profile_id: &str, port: u16) -> Result<(), String> {
    hub.adapter_bridge()
        .realign_restored_bridge_port(hub.providers(), profile_id, port)
}

/// Inverse of a restore-port apply after the demoted provider row was written.
///
/// Non-current restores never call `switch_with_guard`, so live config is
/// untouched and a full [`rollback_bridge_projection`] would write the real
/// Codex file unnecessarily. Current restores already switched, so they reuse
/// the apply-saga inverse including live snapshot restore.
#[allow(dead_code)]
fn rollback_restored_bridge_port(
    hub: &AgentHub,
    core_guard: &ProviderLiveSagaGuard<'_>,
    provider_id: &str,
    snapshot: &BridgeProviderSnapshot,
    was_current: bool,
    target_agent: AgentId,
) -> Result<(), &'static str> {
    hub.adapter_bridge().rollback_restored_bridge_port(
        hub.providers(),
        core_guard,
        provider_id,
        snapshot,
        was_current,
        target_agent,
    )
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
        upstream_status: BridgeUpstreamStatus::Stopped,
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
        hub.adapter_bridge()
            .profile_id_for_request(&request)
            .map_err(|error| map_err_string("adapter_bridge_profile_id", error))
    })
    .await
}

fn map_bridge_host_error(error: BridgeHostError) -> String {
    // Host error implementations intentionally contain no bearer; still use a
    // stable GUI-facing code and do not serialize the Debug representation.
    match &error {
        BridgeHostError::Bind(io) if io.kind() == std::io::ErrorKind::AddrInUse => {
            format!(
                "本机端口已被占用。将自动换一个空闲端口并写回，请点重试。 [{CODE_BRIDGE_PORT_IN_USE}]"
            )
        }
        BridgeHostError::Bind(_) => {
            format!("本机转发无法监听端口，请点重试。 [{CODE_BRIDGE_PORT_IN_USE}]")
        }
        _ => format!("本机转发无法启动或停止，请点重试。 [{CODE_BRIDGE_START}]"),
    }
}

/// Replace the raw English resolver failure with a Chinese sentence the
/// Connections confirm dialog can show. Other apply errors pass through.
fn map_bridge_apply_error(error: &str, target_agent: AgentId) -> String {
    if !error.contains("invalid adapter secret reference") {
        return error.to_owned();
    }
    let message = if target_agent == AgentId::Claude {
        "这份 Grok 登录没法解析成 Claude 路由要用的密钥"
    } else {
        "这份登录没法解析成目标路由要用的密钥"
    };
    format!("{message} [invalid_arg]")
}

#[cfg(test)]
mod tests;
