//! Desktop-process control plane for Adapter local bridges.
//!
//! `agenthub-core` owns profile persistence, source-secret resolution and the
//! generated target provider projection.  This module deliberately owns the
//! cross-boundary saga: the loopback listener must bind before the generated
//! provider is made current, and a failed apply must not leave a newly-started
//! listener running.  Neither upstream nor local bearer tokens cross the
//! Tauri command boundary.
//!
//! Process-local profile / target gates and the credential-free status DTO live
//! in [`agenthub_core::adapter_control`] so commands stay Tauri-neutral.

use std::sync::Arc;
use std::time::SystemTime;

use agenthub_core::adapter_control::AdapterBridgeStatus;
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
    // First-time apply must make the generated target Connection current.
    apply_local_bridge_locked(hub, host, coordinator, request, true).await
}

async fn apply_local_bridge_locked(
    hub: Arc<AgentHub>,
    host: Arc<BridgeRuntimeHost>,
    coordinator: Arc<AdapterBridgeSagaCoordinator>,
    request: AdapterBridgePrepareRequest,
    // When true (initial apply), always switch the target Agent to the
    // generated bridge. Manual start keeps the user's current Connection
    // unless the generated bridge provider was already current (then refresh
    // live config only).
    force_switch_current: bool,
) -> Result<AdapterApplyResult, String> {
    let target_agent_id = request.target_agent_id;
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

    // Keep the same Tauri target authority as every configuration/account/
    // provider mutation. Inside the blocking critical section, the Core guard
    // retains the cross-process switch lock from snapshot through
    // projection/finalize/rollback; do not call ordinary lock-taking provider
    // APIs while it is held.
    let target_agent = prepared.profile().target_agent_id;
    let _target_guard = coordinator.lock_target(target_agent).await;
    let result = with_hub_blocking(hub.clone(), move |hub| {
        let core_guard = hub
            .providers
            .begin_live_saga(target_agent)
            .map_err(|error| map_err_string("begin_adapter_bridge_provider_saga", error))?;
        let projection = hub
            .adapter_bridge
            .revalidate_provider_projection(&prepared, port)
            .map_err(|error| map_err_string("revalidate_adapter_bridge_provider", error))?;
        let provider_id = prepared.profile().generated_provider_id.clone();
        let snapshot =
            capture_provider_snapshot(hub, &core_guard, provider_id.as_deref(), target_agent)?;
        persist_bridge_projection_inner(
            hub,
            &core_guard,
            &prepared,
            projection,
            port,
            &snapshot,
            force_switch_current,
        )
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
    // Manual start must not steal Codex current if the user already switched away.
    let applied = apply_local_bridge_locked(hub, host.clone(), coordinator, request, false).await?;
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
    let _target_guard = coordinator.lock_target(profile.target_agent_id).await;
    if profile.route != AdapterRoute::LocalBridge {
        return with_hub_blocking(hub, move |hub| {
            hub.adapter_apply
                .remove(&profile_id)
                .map_err(|error| map_err_string("remove_adapter", error))
        })
        .await;
    }

    // Stop outside the Core live-saga critical section so listener drain does
    // not hold the cross-process provider lock. Current bindings are allowed:
    // unbind restores previous live before deleting the projection.
    let _ = stop_bridge_runtime(&host, &profile).await?;
    let ticket_id = agenthub_core::models::ticket_id(profile.source_kind, &profile.source_id);
    let agent_id = profile.target_agent_id;
    with_hub_blocking(hub, move |hub| {
        hub.ticket_bind
            .unbind(&agenthub_core::models::TicketUnbindRequest {
                ticket_id,
                agent_id,
            })
            .map_err(|error| map_err_string("unbind_ticket", error))
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
                let _ = host.record_upstream_outcome(&profile.id, BridgeUpstreamStatus::Degraded);
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
            let _ = host.record_upstream_outcome(&profile.id, BridgeUpstreamStatus::Connected);

            // Preferred-port rebind must rewrite profile.local_port and the
            // generated target endpoint; otherwise restore leaves a dead endpoint.
            if Some(runtime.status.port) != profile.local_port || material.needs_reprojection() {
                let _target_guard = coordinator.lock_target(profile.target_agent_id).await;
                if let Err(error) = with_hub_blocking(hub.clone(), {
                    let profile_id = profile.id.clone();
                    let port = runtime.status.port;
                    move |hub| realign_restored_bridge_port(hub, &profile_id, port)
                })
                .await
                {
                    let _ =
                        compensate_started_bridge(&host, &profile.id, runtime.owned_by_saga).await;
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
    force_switch_current: bool,
) -> Result<AdapterApplyResult, String> {
    let provider_id = prepared
        .profile()
        .generated_provider_id
        .as_deref()
        .ok_or_else(|| "adapter bridge profile has no generated provider".to_string())?
        .to_owned();

    // Keep the row returned by create/update.  Re-reading it below is not only
    // unnecessary, it creates a failure window after the projection has already
    // been persisted: a transient read error would bypass compensation and
    // leave the provider/profile pair out of sync.
    let (created, projected_provider) = match projection {
        AdapterBridgeProviderProjection::Create(input) => {
            let provider = hub
                .providers
                .create_with_guard(core_guard, &input)
                .map_err(|error| map_err_string("create_adapter_bridge_provider", error))?;
            (true, Some(provider))
        }
        AdapterBridgeProviderProjection::Update(input) => {
            let provider = hub
                .providers
                .update_with_guard(core_guard, &input)
                .map_err(|error| map_err_string("update_adapter_bridge_provider", error))?;
            (false, Some(provider))
        }
        AdapterBridgeProviderProjection::None => (false, None),
    };

    let generated_was_current = snapshot
        .generated
        .as_ref()
        .map(|provider| provider.is_current)
        .unwrap_or(false);
    let should_switch = should_make_bridge_current(force_switch_current, generated_was_current);

    let previous_current_id = snapshot
        .current_provider
        .as_ref()
        .map(|provider| provider.id.as_str())
        .filter(|id| *id != provider_id.as_str());
    let provider = if should_switch {
        match hub.providers.switch_with_guard(
            core_guard,
            &provider_id,
            prepared.profile().target_agent_id,
        ) {
            Ok(result) => {
                let backup_id = result.backup.as_ref().map(|backup| backup.id.as_str());
                match hub.providers.persist_first_bind_restore_meta_with_guard(
                    core_guard,
                    &result.provider,
                    previous_current_id,
                    backup_id,
                ) {
                    Ok(provider) => provider.redacted(),
                    Err(error) => {
                        let rollback = rollback_bridge_projection(
                            hub,
                            core_guard,
                            &provider_id,
                            snapshot,
                            created,
                            prepared.profile().target_agent_id,
                        );
                        return Err(composite_saga_error(
                            "persist_adapter_bridge_restore_meta",
                            map_err_string("persist_adapter_bridge_restore_meta", error),
                            rollback,
                        ));
                    }
                }
            }
            Err(error) => {
                let rollback = rollback_bridge_projection(
                    hub,
                    core_guard,
                    &provider_id,
                    snapshot,
                    created,
                    prepared.profile().target_agent_id,
                );
                return Err(composite_saga_error(
                    "switch_adapter_bridge_provider",
                    map_err_string("switch_adapter_bridge_provider", error),
                    rollback,
                ));
            }
        }
    } else if let Some(provider) = projected_provider {
        provider.redacted()
    } else {
        // `None` means no pool mutation was needed.  The provider must still
        // exist for the result, but this read happens only on the no-op path,
        // before any new projection has been written by this saga.
        hub.providers
            .repo()
            .get_by_id(&provider_id)
            .map_err(|error| map_err_string("load_adapter_bridge_provider", error))?
            .ok_or_else(|| "adapter bridge provider missing after projection".to_string())?
            .redacted()
    };
    let profile = match hub.adapter_bridge.finalize(prepared, port) {
        Ok(profile) => profile,
        Err(error) => {
            let rollback = rollback_bridge_projection(
                hub,
                core_guard,
                &provider_id,
                snapshot,
                created,
                prepared.profile().target_agent_id,
            );
            return Err(composite_saga_error(
                "finalize_adapter_bridge",
                map_err_string("finalize_adapter_bridge", error),
                rollback,
            ));
        }
    };
    Ok(AdapterApplyResult { profile, provider })
}

/// Initial apply always promotes the generated bridge; manual start only
/// refreshes live config when that bridge was already the current Connection.
fn should_make_bridge_current(force_switch_current: bool, generated_was_current: bool) -> bool {
    force_switch_current || generated_was_current
}

#[derive(Clone)]
struct BridgeProviderSnapshot {
    generated: Option<Provider>,
    current_provider: Option<Provider>,
    live_config: ProviderLiveConfigSnapshot,
}

fn capture_provider_snapshot(
    hub: &AgentHub,
    core_guard: &ProviderLiveSagaGuard<'_>,
    generated_provider_id: Option<&str>,
    target_agent: AgentId,
) -> Result<BridgeProviderSnapshot, String> {
    let generated = match generated_provider_id {
        Some(id) => hub
            .providers
            .repo()
            .get_by_id(id)
            .map_err(|error| map_err_string("snapshot_adapter_bridge_provider", error))?,
        None => None,
    };
    let current_provider = hub
        .providers
        .repo()
        .get_current(target_agent)
        .map_err(|error| map_err_string("snapshot_adapter_bridge_provider", error))?;
    let live_config = hub
        .providers
        .capture_live_config_snapshot_with_guard(core_guard, target_agent)
        .map_err(|error| map_err_string("snapshot_adapter_bridge_live_config", error))?;
    Ok(BridgeProviderSnapshot {
        generated,
        current_provider,
        live_config,
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
    target_agent: AgentId,
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
            .delete_with_guard(core_guard, provider_id, target_agent)
            .is_err()
    {
        failed = true;
    }

    if let Some(old_current) = &snapshot.current_provider {
        if hub
            .providers
            .switch_with_guard(core_guard, &old_current.id, target_agent)
            .is_err()
        {
            failed = true;
        }
    }

    // A provider switch back to an old current row may backfill a drifted
    // value rather than the byte-exact snapshot captured before this saga.
    // Always restore the snapshot last, whether or not there was an old
    // current provider, so failed finalize/switch cannot leave live config
    // changed.
    if hub
        .providers
        .restore_live_config_snapshot_with_guard(core_guard, &snapshot.live_config)
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
        || !matches!(
            profile.target_agent_id,
            AgentId::Codex | AgentId::Claude | AgentId::Grok | AgentId::Kimi | AgentId::Dsh
        )
        || !matches!(
            profile.source_kind,
            AdapterSourceKind::Provider | AdapterSourceKind::Account
        )
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
    let profile = hub
        .adapter_apply
        .list(None, None, None)
        .map_err(|error| map_err_string("load_adapter_bridge_restore_profile", error))?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("adapter profile not found: {profile_id}"))?;
    let target_agent = profile.target_agent_id;
    let core_guard = hub
        .providers
        .begin_live_saga(target_agent)
        .map_err(|error| map_err_string("begin_adapter_bridge_restore_rebind_saga", error))?;
    let (input, was_current) = hub
        .adapter_bridge
        .projection_for_restored_port(profile_id, port)
        .map_err(|error| map_err_string("projection_adapter_bridge_restore_port", error))?;
    // Snapshot before any pool or live mutation so a later-stage failure can
    // restore the previous generated row and target config in reverse order.
    let snapshot =
        capture_provider_snapshot(hub, &core_guard, Some(input.id.as_str()), target_agent)?;
    let provider_id = input.id.clone();

    if let Err(error) = hub.providers.update_with_guard(&core_guard, &input) {
        // SQL ABORT leaves the pool unchanged. Restore-port projections are
        // always demoted, so live config was not written and must not be
        // rewritten here (that would replace the injected update error).
        return Err(map_err_string("update_adapter_bridge_restore_port", error));
    }
    if was_current {
        if let Err(error) = hub
            .providers
            .switch_with_guard(&core_guard, &provider_id, target_agent)
        {
            let rollback = rollback_bridge_projection(
                hub,
                &core_guard,
                &provider_id,
                &snapshot,
                false,
                target_agent,
            );
            return Err(composite_saga_error(
                "switch_adapter_bridge_restore_port",
                map_err_string("switch_adapter_bridge_restore_port", error),
                rollback,
            ));
        }
    }
    if let Err(error) = hub.adapter_bridge.persist_restored_port(profile_id, port) {
        // persist_restored_port is last and transactional, so a failure leaves
        // profile.local_port on the old preferred port. Restore the generated
        // provider row; rewrite live config only when switch already mutated it.
        let rollback = rollback_restored_bridge_port(
            hub,
            &core_guard,
            &provider_id,
            &snapshot,
            was_current,
            target_agent,
        );
        return Err(composite_saga_error(
            "persist_adapter_bridge_restore_port",
            map_err_string("persist_adapter_bridge_restore_port", error),
            rollback,
        ));
    }
    Ok(())
}

/// Inverse of a restore-port apply after the demoted provider row was written.
///
/// Non-current restores never call `switch_with_guard`, so live config is
/// untouched and a full [`rollback_bridge_projection`] would write the real
/// Codex file unnecessarily. Current restores already switched, so they reuse
/// the apply-saga inverse including live snapshot restore.
fn rollback_restored_bridge_port(
    hub: &AgentHub,
    core_guard: &ProviderLiveSagaGuard<'_>,
    provider_id: &str,
    snapshot: &BridgeProviderSnapshot,
    was_current: bool,
    target_agent: AgentId,
) -> Result<(), &'static str> {
    if was_current {
        return rollback_bridge_projection(
            hub,
            core_guard,
            provider_id,
            snapshot,
            false,
            target_agent,
        );
    }
    if let Some(old) = &snapshot.generated {
        let input = provider_to_non_current_input(old);
        if hub.providers.update_with_guard(core_guard, &input).is_err() {
            return Err("adapter.bridge_rollback");
        }
    }
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
        hub.adapter_bridge
            .profile_id_for_request(&request)
            .map_err(|error| map_err_string("adapter_bridge_profile_id", error))
    })
    .await
}

fn map_bridge_host_error(error: BridgeHostError) -> String {
    // Host error implementations intentionally contain no bearer; still use a
    // stable GUI-facing code and do not serialize the Debug representation.
    format!("本机路由无法启动或停止 [{CODE_BRIDGE_START}]: {error}")
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
