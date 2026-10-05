//! Process-local route runtime facade.
//!
//! Phase one keeps Rust as the only active product runtime. The existing Go
//! host remains the isolated development/probe runtime; it is owned here so
//! process observation and shutdown have one authority, but it is not exposed
//! as the product Go backend.

use std::sync::Arc;

#[cfg(feature = "route-runtime-product-handoff-probe")]
use agenthub_core::adapter_control::AdapterSagaCoordinator;
use agenthub_core::adapter_control::{AdapterBridgeStatus, LocalGatewayStatus};
use agenthub_core::bridge::host::{
    BridgeGatewayRestoreError, BridgeGatewaySnapshot, BridgeGatewayStopReport, BridgeHostError,
    RouteTraceDeleteResult, RouteTracePage, RouteTraceQuery,
};
#[cfg(feature = "route-runtime-product-handoff-probe")]
use agenthub_core::bridge::host::{BridgeGatewaySnapshotState, BridgeGatewayStopState};
use agenthub_core::bridge::BridgeRuntimeHost;
use agenthub_core::bridge::{BridgeRuntimeStatus, BridgeStartSpec};
use agenthub_core::logging::{self, targets};
use agenthub_core::models::{AgentId, RouteDownstreamSurface, RouteSchedulePolicy};
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

/// Safe failure categories for the in-process-only Product runtime probe.
#[cfg(feature = "route-runtime-product-handoff-probe")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProductHandoffTrialProbeFailure {
    RuntimeSecretScan,
    ProductRequest,
}

/// In-process-only probe hook invoked while Product Go is running. The hook
/// returns only a safe failure category, so configuration or Key material
/// cannot escape through the trial result.
#[cfg(feature = "route-runtime-product-handoff-probe")]
pub(crate) type ProductHandoffTrialRuntimeProbe =
    Box<dyn FnOnce(&GoRouteIsolatedHost) -> Result<(), ProductHandoffTrialProbeFailure> + Send>;

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

/// Complete, process-local desired state for the shared loopback gateway.
///
/// This deliberately has no `Debug`, serialization, or DTO implementation:
/// its entry specs and accepted bearer rows contain login material.  The
/// controller builds it from the durable store while it holds the lifecycle
/// and local-gateway saga locks; the runtime manager consumes it without
/// retaining it after a reconciliation attempt.
pub(crate) struct GatewayDesiredSnapshot {
    desired_running: bool,
    accepted_bearers: Vec<(String, String)>,
    entries: Vec<GatewayDesiredEntry>,
}

impl GatewayDesiredSnapshot {
    pub(crate) fn new(
        desired_running: bool,
        accepted_bearers: Vec<(String, String)>,
        entries: Vec<GatewayDesiredEntry>,
    ) -> Self {
        Self {
            desired_running,
            accepted_bearers,
            entries,
        }
    }

    pub(crate) fn desired_running(&self) -> bool {
        self.desired_running
    }

    fn entry(&self, pool_id: &str) -> Option<&GatewayDesiredEntry> {
        self.entries.iter().find(|entry| entry.pool_id == pool_id)
    }

    fn entries(&self) -> &[GatewayDesiredEntry] {
        &self.entries
    }
}

/// One shared-gateway edge inside [`GatewayDesiredSnapshot`].  The explicit
/// public metadata is safe for a controller to use in stable error logging;
/// the start spec remains private to this module's reconciliation code.
pub(crate) struct GatewayDesiredEntry {
    pool_id: String,
    target_agent: AgentId,
    downstream_surface: RouteDownstreamSurface,
    required_for_manual_start: bool,
    persists_gateway_port: bool,
    spec: BridgeStartSpec,
}

impl GatewayDesiredEntry {
    pub(crate) fn pool(
        pool_id: String,
        target_agent: AgentId,
        downstream_surface: RouteDownstreamSurface,
        required_for_manual_start: bool,
        spec: BridgeStartSpec,
    ) -> Self {
        Self {
            pool_id,
            target_agent,
            downstream_surface,
            required_for_manual_start,
            persists_gateway_port: true,
            spec,
        }
    }

    pub(crate) fn placeholder(spec: BridgeStartSpec) -> Self {
        Self {
            pool_id: "local-gateway".to_owned(),
            target_agent: AgentId::Codex,
            downstream_surface: RouteDownstreamSurface::Responses,
            required_for_manual_start: true,
            persists_gateway_port: false,
            spec,
        }
    }
}

/// How a complete desired snapshot should be applied.  This describes only
/// process-local work; it never selects or persists a Product Go backend.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum GatewayStartMode {
    RestoreBestEffort,
    ManualRequiredDefaults,
}

/// The selected post-write action.  The caller always supplies a complete
/// latest snapshot, even when one pool is the only listener that must move.
pub(crate) enum GatewaySavedWriteIntent {
    PublishOnly,
    StartSelected(Vec<String>),
    RestartSelected(Vec<String>),
}

/// Credential-free result needed to persist a first bound gateway port.
pub(crate) struct GatewayStartedEntry {
    pub(crate) pool_id: String,
    pub(crate) port: u16,
    persists_gateway_port: bool,
    was_running: bool,
}

impl GatewayStartedEntry {
    pub(crate) fn persists_gateway_port(&self) -> bool {
        self.persists_gateway_port
    }

    /// Whether this reconciliation created an edge that did not exist before
    /// its start attempt. Callers use this only to compensate a later durable
    /// enrollment failure; a reused live edge must not be stopped.
    pub(crate) fn started_new(&self) -> bool {
        !self.was_running
    }
}

/// Stable per-edge reconciliation failure.  It intentionally owns neither a
/// snapshot nor a rendered host error, so logs cannot accidentally include a
/// bearer, upstream address, or request body.
pub(crate) struct GatewayReconcileFailure {
    pub(crate) pool_id: String,
    pub(crate) target_agent: AgentId,
    pub(crate) downstream_surface: RouteDownstreamSurface,
    pub(crate) stage: &'static str,
    cause: BridgeHostError,
}

impl GatewayReconcileFailure {
    pub(crate) fn into_cause(self) -> BridgeHostError {
        self.cause
    }
}

/// Results from applying a full desired state.  Errors for optional restore
/// entries are retained as credential-free metadata; manual required entries
/// return a [`GatewayReconcileFailure`] immediately.
pub(crate) struct GatewayReconcileReport {
    pub(crate) started: Vec<GatewayStartedEntry>,
    pub(crate) failures: Vec<GatewayReconcileFailure>,
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

    /// Test-only manager with a bare in-memory Rust host. It intentionally
    /// does not resolve application data paths or enable usage/trace
    /// persistence, so test fixtures cannot touch a user's data directory.
    #[cfg(test)]
    pub(crate) fn new_test(rust: BridgeRuntimeHost) -> Self {
        Self {
            rust: Arc::new(rust),
            isolated_go: GoRouteIsolatedHost::new(None),
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
        health_bearers: Vec<String>,
        runtime_probe: Option<ProductHandoffTrialRuntimeProbe>,
    ) -> tauri::async_runtime::JoinHandle<Result<ProductHandoffTrialReport, ProductHandoffTrialError>>
    {
        self.spawn_product_handoff_trial_with_completion(
            lifecycle_barrier,
            coordinator,
            health_bearers,
            runtime_probe,
            None,
        )
    }

    /// The completion sender is probe-only evidence that a detached caller's
    /// task ran through its full compensation path. It cannot cross a product
    /// boundary and carries no configuration or bearer material.
    #[cfg(feature = "route-runtime-product-handoff-probe")]
    pub(crate) fn spawn_product_handoff_trial_with_completion(
        self: &Arc<Self>,
        lifecycle_barrier: Arc<LifecycleShutdownBarrier>,
        coordinator: Arc<AdapterSagaCoordinator>,
        health_bearers: Vec<String>,
        runtime_probe: Option<ProductHandoffTrialRuntimeProbe>,
        completion: Option<
            tokio::sync::oneshot::Sender<
                Result<ProductHandoffTrialReport, ProductHandoffTrialError>,
            >,
        >,
    ) -> tauri::async_runtime::JoinHandle<Result<ProductHandoffTrialReport, ProductHandoffTrialError>>
    {
        let runtime = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let result = async {
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
                runtime
                    .run_product_handoff_trial(health_bearers, runtime_probe)
                    .await
            }
            .await;
            if let Some(completion) = completion {
                let _ = completion.send(result);
            }
            result
        })
    }

    #[cfg(feature = "route-runtime-product-handoff-probe")]
    async fn run_product_handoff_trial(
        &self,
        health_bearers: Vec<String>,
        runtime_probe: Option<ProductHandoffTrialRuntimeProbe>,
    ) -> Result<ProductHandoffTrialReport, ProductHandoffTrialError> {
        if health_bearers.is_empty()
            || health_bearers.iter().any(|bearer| {
                bearer.trim().is_empty() || bearer.bytes().any(|byte| byte <= b' ' || byte == 0x7f)
            })
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
        if let Some(probe) = runtime_probe {
            let stage = match probe(&self.isolated_go) {
                Ok(()) => None,
                Err(ProductHandoffTrialProbeFailure::RuntimeSecretScan) => {
                    Some("runtime_secret_scan")
                }
                Err(ProductHandoffTrialProbeFailure::ProductRequest) => Some("product_request"),
            };
            if let Some(stage) = stage {
                return Err(self
                    .compensate_product_handoff(&snapshot, port, stage)
                    .await);
            }
        }
        let prepared_hash_matched = self.isolated_go.probe_config_hash().as_deref()
            == Some(prepared_summary.expected_config_hash.as_str());
        if !prepared_hash_matched {
            return Err(self
                .compensate_product_handoff(&snapshot, port, "prepared_hash")
                .await);
        }
        let mut product_health_ready = true;
        for health_bearer in health_bearers {
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
            product_health_ready &= health.http_status == 200
                && health.listen_ready == Some(true)
                && health.member_count.is_some_and(|count| count >= 1)
                && health.healthy_member_count.is_some_and(|count| count >= 1);
        }
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

    /// Publish the saved local entry Key set to the fixed active runtime.
    ///
    /// The controller reads the durable rows while holding its gateway saga
    /// lock; this method is the only production seam that writes the
    /// process-local accepted-bearer table.  Product Go remains unavailable,
    /// so the active backend is deliberately still the Rust host.
    #[cfg(test)]
    pub(crate) fn sync_active_gateway_accepted_bearers(
        &self,
        rows: Vec<(String, String)>,
    ) -> Result<(), BridgeHostError> {
        debug_assert_eq!(self.active_backend(), RuntimeBackend::Rust);
        self.rust.set_extra_local_bearers(rows)
    }

    /// Start the fixed Rust gateway from one complete saved-state snapshot.
    ///
    /// `RestoreBestEffort` leaves already-live legacy profile listeners alone
    /// and records individual failures.  A manual start still fails closed for
    /// required default entries, preserving the board-switch contract.
    pub(crate) async fn start_gateway(
        &self,
        snapshot: &GatewayDesiredSnapshot,
        mode: GatewayStartMode,
    ) -> Result<GatewayReconcileReport, GatewayReconcileFailure> {
        self.publish_gateway_bearers(snapshot)
            .map_err(|cause| GatewayReconcileFailure {
                pool_id: "local-gateway".to_owned(),
                target_agent: AgentId::Codex,
                downstream_surface: RouteDownstreamSurface::Responses,
                stage: "publish_bearers",
                cause,
            })?;

        let mut report = GatewayReconcileReport {
            started: Vec::new(),
            failures: Vec::new(),
        };
        for entry in snapshot.entries() {
            if mode == GatewayStartMode::RestoreBestEffort
                && self
                    .rust
                    .status(&entry.pool_id)
                    .map_err(|cause| Self::gateway_failure(entry, "status_existing", cause))?
                    .is_some()
            {
                continue;
            }
            match self.start_gateway_entry(entry).await {
                Ok(started) => report.started.push(started),
                Err(failure) if mode == GatewayStartMode::RestoreBestEffort => {
                    report.failures.push(failure);
                }
                Err(failure) if entry.required_for_manual_start => return Err(failure),
                Err(failure) => report.failures.push(failure),
            }
        }
        Ok(report)
    }

    /// Apply a full snapshot after a durable route/key/model write.  Only the
    /// selected pool listeners move, but every accepted bearer is republished
    /// from the just-read durable state before any listener is resumed.
    pub(crate) async fn reconcile_gateway_after_saved_write(
        &self,
        snapshot: &GatewayDesiredSnapshot,
        intent: GatewaySavedWriteIntent,
    ) -> Result<GatewayReconcileReport, GatewayReconcileFailure> {
        self.publish_gateway_bearers(snapshot)
            .map_err(|cause| GatewayReconcileFailure {
                pool_id: "local-gateway".to_owned(),
                target_agent: AgentId::Codex,
                downstream_surface: RouteDownstreamSurface::Responses,
                stage: "publish_bearers",
                cause,
            })?;

        let (pool_ids, restart) = match intent {
            GatewaySavedWriteIntent::PublishOnly => {
                return Ok(GatewayReconcileReport {
                    started: Vec::new(),
                    failures: Vec::new(),
                });
            }
            GatewaySavedWriteIntent::StartSelected(pool_ids) => (pool_ids, false),
            GatewaySavedWriteIntent::RestartSelected(pool_ids) => (pool_ids, true),
        };
        let mut report = GatewayReconcileReport {
            started: Vec::new(),
            failures: Vec::new(),
        };
        for pool_id in pool_ids {
            let Some(entry) = snapshot.entry(&pool_id) else {
                // A delete can remove the last visible entry.  Never leave
                // its old listener accepting a now-removed bearer.
                self.stop_gateway_entry(&pool_id).await.map_err(|cause| {
                    GatewayReconcileFailure {
                        pool_id,
                        target_agent: AgentId::Codex,
                        downstream_surface: RouteDownstreamSurface::Responses,
                        stage: "stop_removed",
                        cause,
                    }
                })?;
                continue;
            };
            if restart {
                self.stop_gateway_entry(&entry.pool_id)
                    .await
                    .map_err(|cause| Self::gateway_failure(entry, "stop_before_restart", cause))?;
            }
            report.started.push(self.start_gateway_entry(entry).await?);
        }
        Ok(report)
    }

    /// Recover selected listeners from a newly built durable snapshot.  This
    /// is intentionally an alias with a separate name: callers must not reuse
    /// an in-memory spec from the failed write that created the pending work.
    pub(crate) async fn recover_gateway_after_saved_write(
        &self,
        snapshot: &GatewayDesiredSnapshot,
        pool_ids: Vec<String>,
    ) -> Result<GatewayReconcileReport, GatewayReconcileFailure> {
        self.reconcile_gateway_after_saved_write(
            snapshot,
            GatewaySavedWriteIntent::StartSelected(pool_ids),
        )
        .await
    }

    /// Stop all shared-gateway listeners and wait until the loopback port is
    /// actually unbound.  The controller persists desired=false only after
    /// this returns successfully.
    pub(crate) async fn stop_gateway(&self) -> Result<(), BridgeHostError> {
        for pool_id in self.rust.running_ids()? {
            self.stop_gateway_entry(&pool_id).await?;
        }
        if self.rust.gateway_port()?.is_some() || !self.rust.running_ids()?.is_empty() {
            return Err(BridgeHostError::Stopping);
        }
        Ok(())
    }

    /// Stop selected shared-gateway edges before a durable write replaces
    /// their listener material.  `stop` waits for listener cleanup, so a
    /// caller cannot persist a replacement then accidentally leave the old
    /// edge accepting its stale configuration.
    pub(crate) async fn stop_gateway_pools(
        &self,
        pool_ids: &[String],
    ) -> Result<(), BridgeHostError> {
        for pool_id in pool_ids {
            self.stop_gateway_entry(pool_id).await?;
        }
        Ok(())
    }

    /// Read whether one shared-gateway edge is currently live.  This is an
    /// observation only; callers use it to preserve the existing rule that a
    /// model refresh never starts a listener the user has kept stopped.
    pub(crate) fn gateway_pool_is_running(&self, pool_id: &str) -> Result<bool, BridgeHostError> {
        Ok(self.rust.status(pool_id)?.is_some())
    }

    /// Credential-free board status for the fixed active gateway.
    pub(crate) fn gateway_status(
        &self,
        restarting: bool,
    ) -> Result<LocalGatewayStatus, BridgeHostError> {
        let ids = self.rust.running_ids()?;
        let port = self.rust.gateway_port()?;
        let mut statuses = Vec::new();
        for id in ids {
            if let Some(runtime) = self.rust.status(&id)? {
                statuses.push(Self::gateway_status_dto(&self.rust, &id, runtime));
            }
        }
        Ok(LocalGatewayStatus {
            running: port.is_some() && !statuses.is_empty(),
            port,
            statuses,
            recent_unauthenticated_traces: self.rust.recent_unauthenticated_route_traces(),
            restarting,
        })
    }

    fn publish_gateway_bearers(
        &self,
        snapshot: &GatewayDesiredSnapshot,
    ) -> Result<(), BridgeHostError> {
        debug_assert_eq!(self.active_backend(), RuntimeBackend::Rust);
        self.rust
            .set_extra_local_bearers(snapshot.accepted_bearers.clone())
    }

    async fn start_gateway_entry(
        &self,
        entry: &GatewayDesiredEntry,
    ) -> Result<GatewayStartedEntry, GatewayReconcileFailure> {
        let was_running = self
            .rust
            .status(&entry.pool_id)
            .map_err(|cause| Self::gateway_failure(entry, "status_before_start", cause))?
            .is_some();
        let status = self
            .rust
            .start(entry.spec.clone())
            .await
            .map_err(|cause| Self::gateway_failure(entry, "host_start", cause))?;
        Ok(GatewayStartedEntry {
            pool_id: entry.pool_id.clone(),
            port: status.port,
            persists_gateway_port: entry.persists_gateway_port,
            was_running,
        })
    }

    async fn stop_gateway_entry(&self, pool_id: &str) -> Result<(), BridgeHostError> {
        match self.rust.stop(pool_id).await {
            Ok(_) | Err(BridgeHostError::NotRunning) => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn gateway_failure(
        entry: &GatewayDesiredEntry,
        stage: &'static str,
        cause: BridgeHostError,
    ) -> GatewayReconcileFailure {
        GatewayReconcileFailure {
            pool_id: entry.pool_id.clone(),
            target_agent: entry.target_agent,
            downstream_surface: entry.downstream_surface,
            stage,
            cause,
        }
    }

    fn gateway_status_dto(
        host: &BridgeRuntimeHost,
        pool_id: &str,
        runtime: BridgeRuntimeStatus,
    ) -> AdapterBridgeStatus {
        let token = host.local_token(pool_id).ok().flatten();
        AdapterBridgeStatus::from_runtime(runtime)
            .with_recent_inbound(host.recent_inbound(pool_id))
            .with_recent_route_traces(host.recent_route_traces(pool_id))
            .with_inbound_stats(host.inbound_stats(pool_id))
            .with_local_token(token)
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
