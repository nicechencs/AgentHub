//! Process-local route runtime facade.
//!
//! Phase one keeps Rust as the only active product runtime. The existing Go
//! host remains the isolated development/probe runtime; it is owned here so
//! process observation and shutdown have one authority, but it is not exposed
//! as the product Go backend.

use std::sync::Arc;

#[cfg(feature = "route-runtime-product-handoff-probe")]
use agenthub_core::adapter_control::AdapterSagaCoordinator;
use agenthub_core::bridge::host::{
    BridgeGatewayRestoreError, BridgeGatewaySnapshot, BridgeGatewayStopReport, BridgeHostError,
    RouteTraceDeleteResult, RouteTracePage, RouteTraceQuery,
};
#[cfg(feature = "route-runtime-product-handoff-probe")]
use agenthub_core::bridge::host::{BridgeGatewaySnapshotState, BridgeGatewayStopState};
use agenthub_core::bridge::BridgeRuntimeHost;
use agenthub_core::logging::{self, targets};
use agenthub_core::models::RouteSchedulePolicy;
use agenthub_core::services::account_quota::MemberQuotaHint;
use agenthub_core::AgentHub;

#[cfg(feature = "route-runtime-product-handoff-probe")]
use crate::exit_coordinator::LifecycleShutdownBarrier;
use crate::go_route_isolated::GoRouteIsolatedHost;

#[cfg(feature = "route-runtime-product-handoff-probe")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProductHandoffTrialReport {
    pub port: u16,
    pub rust_entry_count: usize,
    pub prepared_hash_matched: bool,
    pub rust_stopped_before_go: bool,
    pub rust_mutator_blocked: bool,
    pub product_health_ready: bool,
    pub go_stopped_before_restore: bool,
    pub rust_exact_restored: bool,
}

#[cfg(feature = "route-runtime-product-handoff-probe")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProductHandoffTrialError {
    pub stage: &'static str,
    pub go_stopped: bool,
    pub rust_restored: bool,
}

#[cfg(feature = "route-runtime-product-handoff-probe")]
impl std::fmt::Display for ProductHandoffTrialError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Product handoff trial failed at {}", self.stage)
    }
}

/// Product runtime implementations known by the desktop shell.
///
/// This is deliberately not persisted in phase one. `GoProduct` describes a
/// capability only; no product Go host is constructed or selected yet.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeBackend {
    Rust,
    GoProduct,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeScope {
    Gateway,
    Profile(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeAvailability {
    Available,
    Unavailable,
}

/// Observed process state. It never advances durable adapter state and must
/// not be used as a replacement for the saved desired-running intent.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeSnapshot {
    pub backend: RuntimeBackend,
    pub scope: RuntimeScope,
    pub availability: RuntimeAvailability,
    pub running: Option<bool>,
    pub active_route_count: Option<usize>,
    pub port: Option<u16>,
}

/// Owns every process-local route runtime without selecting a new default.
pub(crate) struct RouteRuntimeManager {
    rust: Arc<BridgeRuntimeHost>,
    isolated_go: Arc<GoRouteIsolatedHost>,
}

impl RouteRuntimeManager {
    pub(crate) fn new(go_route_hub: Option<Arc<AgentHub>>) -> Self {
        let rust = BridgeRuntimeHost::new();
        // Install the durable gateway usage spool once, before any edge can
        // start. An unresolved dir keeps capture disabled (never fails startup).
        match agenthub_core::utils::paths::usage_gateway_dir() {
            Ok(dir) => rust.set_usage_spool_dir(dir),
            Err(error) => logging::log_app_error(targets::GUI, "usage_gateway_dir", &error),
        }
        // Restore Activity/monitor traces from the disposable sqlite file.
        match agenthub_core::utils::paths::route_traces_persist_path() {
            Ok(path) => rust.set_route_trace_persist_path(path),
            Err(error) => logging::log_app_error(targets::GUI, "route_traces_persist_path", &error),
        }
        let isolated_go = GoRouteIsolatedHost::new(go_route_hub);
        Self {
            rust: Arc::new(rust),
            isolated_go,
        }
    }

    /// Probe-only constructor that cannot write route traces or usage into the
    /// user's normal data directory.
    #[cfg(feature = "route-runtime-product-handoff-probe")]
    pub(crate) fn new_product_handoff_probe(hub: Arc<AgentHub>) -> Self {
        Self {
            rust: Arc::new(BridgeRuntimeHost::new()),
            isolated_go: GoRouteIsolatedHost::new(Some(hub)),
        }
    }

    /// Spawn an owned, cancellation-safe Rust -> Product Go -> Rust trial.
    /// Dropping the returned handle detaches the task; the task keeps both
    /// lifecycle guards and the stopped Rust snapshot until compensation ends.
    #[cfg(feature = "route-runtime-product-handoff-probe")]
    pub(crate) fn spawn_product_handoff_trial(
        self: &Arc<Self>,
        lifecycle_barrier: Arc<LifecycleShutdownBarrier>,
        coordinator: Arc<AdapterSagaCoordinator>,
        health_bearer: String,
    ) -> tauri::async_runtime::JoinHandle<Result<ProductHandoffTrialReport, ProductHandoffTrialError>>
    {
        let runtime = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let _lifecycle_permit =
                lifecycle_barrier
                    .enter()
                    .await
                    .map_err(|_| ProductHandoffTrialError {
                        stage: "lifecycle_barrier",
                        go_stopped: true,
                        rust_restored: true,
                    })?;
            let _gateway_guard = coordinator.lock_local_gateway().await;
            runtime.run_product_handoff_trial(health_bearer).await
        })
    }

    #[cfg(feature = "route-runtime-product-handoff-probe")]
    async fn run_product_handoff_trial(
        &self,
        health_bearer: String,
    ) -> Result<ProductHandoffTrialReport, ProductHandoffTrialError> {
        if health_bearer.trim().is_empty()
            || health_bearer
                .bytes()
                .any(|byte| byte <= b' ' || byte == 0x7f)
        {
            return Err(ProductHandoffTrialError {
                stage: "health_input",
                go_stopped: true,
                rust_restored: true,
            });
        }

        let prepared =
            self.isolated_go
                .prepare_product_plan()
                .map_err(|_| ProductHandoffTrialError {
                    stage: "prepare_product",
                    go_stopped: true,
                    rust_restored: true,
                })?;
        let prepared_summary = GoRouteIsolatedHost::probe_prepared_product_summary(&prepared)
            .map_err(|_| ProductHandoffTrialError {
                stage: "prepared_summary",
                go_stopped: true,
                rust_restored: true,
            })?;
        let snapshot = self
            .capture_active_gateway()
            .map_err(|_| ProductHandoffTrialError {
                stage: "capture_rust",
                go_stopped: true,
                rust_restored: true,
            })?;
        let port = snapshot.port();
        let entry_count = snapshot.entry_count();
        if port == 0 || prepared_summary.expected_port != port {
            return Err(ProductHandoffTrialError {
                stage: "port_mismatch",
                go_stopped: true,
                rust_restored: true,
            });
        }

        let stop_report = match self.stop_active_gateway_snapshot(&snapshot).await {
            Ok(report) => report,
            Err(_) => {
                return Err(self
                    .compensate_product_handoff(&snapshot, port, "stop_rust")
                    .await)
            }
        };
        if stop_report.state != BridgeGatewayStopState::Stopped
            || stop_report.stop_error_count != 0
            || self.rust.gateway_port().ok().flatten().is_some()
            || self
                .rust
                .statuses()
                .map(|statuses| !statuses.is_empty())
                .unwrap_or(true)
        {
            return Err(self
                .compensate_product_handoff(&snapshot, port, "strict_rust_stop")
                .await);
        }
        let rust_mutator_blocked = matches!(
            self.rust.set_gateway_port(port).await,
            Err(BridgeHostError::GatewayTransitionActive)
        );
        if !rust_mutator_blocked {
            return Err(self
                .compensate_product_handoff(&snapshot, port, "rust_mutator_gate")
                .await);
        }

        let go = Arc::clone(&self.isolated_go);
        let started =
            match tauri::async_runtime::spawn_blocking(move || go.start_prepared_product(prepared))
                .await
            {
                Ok(status) => status,
                Err(_) => {
                    return Err(self
                        .compensate_product_handoff(&snapshot, port, "start_go_task")
                        .await)
                }
            };
        if started.state != "ready" || !started.listen_ready || started.port != Some(port) {
            return Err(self
                .compensate_product_handoff(&snapshot, port, "start_go")
                .await);
        }
        let prepared_hash_matched = self.isolated_go.probe_config_hash().as_deref()
            == Some(prepared_summary.expected_config_hash.as_str());
        if !prepared_hash_matched {
            return Err(self
                .compensate_product_handoff(&snapshot, port, "prepared_hash")
                .await);
        }
        let health = match tauri::async_runtime::spawn_blocking(move || {
            GoRouteIsolatedHost::probe_data_plane_health(port, &health_bearer)
        })
        .await
        {
            Ok(Ok(health)) => health,
            Ok(Err(_)) | Err(_) => {
                return Err(self
                    .compensate_product_handoff(&snapshot, port, "product_health_task")
                    .await)
            }
        };
        let product_health_ready = health.http_status == 200
            && health.listen_ready == Some(true)
            && health.member_count.is_some_and(|count| count >= 1)
            && health.healthy_member_count.is_some_and(|count| count >= 1);
        if !product_health_ready {
            return Err(self
                .compensate_product_handoff(&snapshot, port, "product_health")
                .await);
        }

        self.stop_go_before_restore(port, "stop_go").await?;
        self.restore_and_verify_rust(&snapshot, port, entry_count, "restore_rust")
            .await?;
        Ok(ProductHandoffTrialReport {
            port,
            rust_entry_count: entry_count,
            prepared_hash_matched,
            rust_stopped_before_go: true,
            rust_mutator_blocked,
            product_health_ready,
            go_stopped_before_restore: true,
            rust_exact_restored: true,
        })
    }

    #[cfg(feature = "route-runtime-product-handoff-probe")]
    async fn compensate_product_handoff(
        &self,
        snapshot: &BridgeGatewaySnapshot,
        port: u16,
        stage: &'static str,
    ) -> ProductHandoffTrialError {
        if self.stop_go_before_restore(port, stage).await.is_err() {
            return ProductHandoffTrialError {
                stage,
                go_stopped: false,
                rust_restored: false,
            };
        }
        let entry_count = snapshot.entry_count();
        match self
            .restore_and_verify_rust(snapshot, port, entry_count, stage)
            .await
        {
            Ok(()) => ProductHandoffTrialError {
                stage,
                go_stopped: true,
                rust_restored: true,
            },
            Err(_) => ProductHandoffTrialError {
                stage,
                go_stopped: true,
                rust_restored: false,
            },
        }
    }

    #[cfg(feature = "route-runtime-product-handoff-probe")]
    async fn stop_go_before_restore(
        &self,
        port: u16,
        stage: &'static str,
    ) -> Result<(), ProductHandoffTrialError> {
        let go = Arc::clone(&self.isolated_go);
        let stopped = tauri::async_runtime::spawn_blocking(move || go.stop())
            .await
            .map_err(|_| ProductHandoffTrialError {
                stage,
                go_stopped: false,
                rust_restored: false,
            })?;
        if stopped.state != "stopped" || stopped.listen_ready || stopped.port.is_some() {
            return Err(ProductHandoffTrialError {
                stage,
                go_stopped: false,
                rust_restored: false,
            });
        }
        let listener = std::net::TcpListener::bind(("127.0.0.1", port)).map_err(|_| {
            ProductHandoffTrialError {
                stage,
                go_stopped: false,
                rust_restored: false,
            }
        })?;
        drop(listener);
        Ok(())
    }

    #[cfg(feature = "route-runtime-product-handoff-probe")]
    async fn restore_and_verify_rust(
        &self,
        snapshot: &BridgeGatewaySnapshot,
        port: u16,
        entry_count: usize,
        stage: &'static str,
    ) -> Result<(), ProductHandoffTrialError> {
        self.restore_active_gateway_snapshot(snapshot)
            .await
            .map_err(|_| ProductHandoffTrialError {
                stage,
                go_stopped: true,
                rust_restored: false,
            })?;
        let statuses = self.rust.statuses().map_err(|_| ProductHandoffTrialError {
            stage,
            go_stopped: true,
            rust_restored: false,
        })?;
        if snapshot.state() != BridgeGatewaySnapshotState::Restored
            || self.rust.gateway_port().ok().flatten() != Some(port)
            || statuses.len() != entry_count
            || statuses
                .iter()
                .any(|status| !status.running || status.port != port)
        {
            return Err(ProductHandoffTrialError {
                stage,
                go_stopped: true,
                rust_restored: false,
            });
        }
        Ok(())
    }

    pub(crate) fn active_backend(&self) -> RuntimeBackend {
        RuntimeBackend::Rust
    }

    pub(crate) fn availability(&self, backend: RuntimeBackend) -> RuntimeAvailability {
        match backend {
            RuntimeBackend::Rust => RuntimeAvailability::Available,
            RuntimeBackend::GoProduct => RuntimeAvailability::Unavailable,
        }
    }

    /// Observe the fixed active backend.
    pub(crate) fn observe(&self, scope: RuntimeScope) -> Result<RuntimeSnapshot, String> {
        self.observe_backend(self.active_backend(), scope)
    }

    pub(crate) fn profile_observation(
        &self,
        profile_id: impl Into<String>,
    ) -> Result<RuntimeSnapshot, String> {
        self.observe(RuntimeScope::Profile(profile_id.into()))
    }

    pub(crate) fn gateway_observation(&self) -> Result<RuntimeSnapshot, String> {
        self.observe(RuntimeScope::Gateway)
    }

    /// Observe one product backend without changing selection or starting it.
    pub(crate) fn observe_backend(
        &self,
        backend: RuntimeBackend,
        scope: RuntimeScope,
    ) -> Result<RuntimeSnapshot, String> {
        let availability = self.availability(backend);
        if availability == RuntimeAvailability::Unavailable {
            return Ok(RuntimeSnapshot {
                backend,
                scope,
                availability,
                running: None,
                active_route_count: None,
                port: None,
            });
        }

        match &scope {
            RuntimeScope::Gateway => {
                let statuses = self.rust.statuses().map_err(|error| error.to_string())?;
                let port = self
                    .rust
                    .gateway_port()
                    .map_err(|error| error.to_string())?;
                let running = port.is_some() && statuses.iter().any(|status| status.running);
                Ok(RuntimeSnapshot {
                    backend,
                    scope,
                    availability,
                    running: Some(running),
                    active_route_count: Some(statuses.len()),
                    port,
                })
            }
            RuntimeScope::Profile(profile_id) => {
                let status = self
                    .rust
                    .status(profile_id)
                    .map_err(|error| error.to_string())?;
                Ok(RuntimeSnapshot {
                    backend,
                    scope,
                    availability,
                    running: Some(status.as_ref().is_some_and(|status| status.running)),
                    active_route_count: Some(usize::from(status.is_some())),
                    port: status.map(|status| status.port),
                })
            }
        }
    }

    /// Exit-impact count for the active product runtime. Observation failure
    /// stays unknown so the caller can fail closed and show confirmation.
    pub(crate) fn exit_impact_count(&self) -> Option<usize> {
        self.gateway_observation()
            .ok()
            .and_then(|snapshot| snapshot.active_route_count)
    }

    /// Capture the fixed active Rust gateway for an in-memory backend handoff.
    /// This does not select, persist, start, or construct a Product Go host.
    #[allow(dead_code)]
    pub(crate) fn capture_active_gateway(&self) -> Result<BridgeGatewaySnapshot, BridgeHostError> {
        self.rust.capture_gateway_snapshot()
    }

    #[allow(dead_code)]
    pub(crate) async fn stop_active_gateway_snapshot(
        &self,
        snapshot: &BridgeGatewaySnapshot,
    ) -> Result<BridgeGatewayStopReport, BridgeHostError> {
        self.rust.stop_gateway_snapshot(snapshot).await
    }

    #[allow(dead_code)]
    pub(crate) async fn restore_active_gateway_snapshot(
        &self,
        snapshot: &BridgeGatewaySnapshot,
    ) -> Result<(), BridgeGatewayRestoreError> {
        self.rust.restore_gateway_snapshot(snapshot).await
    }

    #[allow(dead_code)]
    pub(crate) async fn commit_active_gateway_stopped(
        &self,
        snapshot: &BridgeGatewaySnapshot,
    ) -> Result<(), BridgeHostError> {
        self.rust.commit_stopped_gateway_snapshot(snapshot).await
    }

    /// Hot-apply one pool schedule without changing the selected runtime.
    pub(crate) fn apply_pool_schedule_policy(
        &self,
        pool_id: &str,
        policy: RouteSchedulePolicy,
    ) -> Result<usize, String> {
        self.rust
            .apply_pool_schedule_policy(pool_id, policy)
            .map_err(|error| error.to_string())
    }

    /// Hot-apply one account quota snapshot without restarting listeners.
    pub(crate) fn apply_account_quota(
        &self,
        source_id: &str,
        hint: MemberQuotaHint,
    ) -> Result<usize, String> {
        self.rust
            .apply_account_quota(
                source_id,
                hint.remaining_pct,
                hint.reset_at,
                hint.fresh_until,
                hint.credit,
            )
            .map_err(|error| error.to_string())
    }

    pub(crate) fn query_route_traces(&self, query: RouteTraceQuery) -> RouteTracePage {
        self.rust.query_route_traces(query)
    }

    pub(crate) fn delete_route_traces(&self, request_ids: &[String]) -> RouteTraceDeleteResult {
        self.rust.delete_route_traces(request_ids)
    }

    /// Stop all process-owned route runtimes. The isolated Go host is not a
    /// product candidate, but a manually started probe must still not survive
    /// desktop shutdown.
    pub(crate) async fn shutdown(&self) -> Result<(), String> {
        let isolated_go = Arc::clone(&self.isolated_go);
        let go_shutdown = tauri::async_runtime::spawn_blocking(move || isolated_go.stop());
        let rust_shutdown = self.rust.shutdown();
        let (go_result, rust_result) = tokio::join!(go_shutdown, rust_shutdown);

        let mut errors = Vec::new();
        if let Err(error) = go_result {
            errors.push(format!("isolated Go route shutdown task failed: {error}"));
        }
        if let Err(error) = rust_result {
            errors.push(format!("Rust route shutdown failed: {error}"));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    /// Transitional bridge-controller seam. Bind/unbind stays on the Rust host
    /// until the controller accepts backend-neutral desired snapshots.
    pub(crate) fn rust_host_for_bridge_saga(&self) -> Arc<BridgeRuntimeHost> {
        Arc::clone(&self.rust)
    }

    /// Transitional isolated development/probe seam. This is not the product
    /// Go backend represented by `RuntimeBackend::GoProduct`.
    pub(crate) fn isolated_go_host(&self) -> Arc<GoRouteIsolatedHost> {
        Arc::clone(&self.isolated_go)
    }
}
