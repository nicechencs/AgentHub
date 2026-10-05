//! Process-local route runtime facade.
//!
//! Phase one keeps Rust as the only active product runtime. The existing Go
//! host remains the isolated development/probe runtime; it is owned here so
//! process observation and shutdown have one authority, but it is not exposed
//! as the product Go backend.

use std::sync::Arc;

use agenthub_core::bridge::host::{RouteTraceDeleteResult, RouteTracePage, RouteTraceQuery};
use agenthub_core::bridge::BridgeRuntimeHost;
use agenthub_core::logging::{self, targets};
use agenthub_core::models::RouteSchedulePolicy;
use agenthub_core::services::account_quota::MemberQuotaHint;
use agenthub_core::AgentHub;

use crate::go_route_isolated::GoRouteIsolatedHost;

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
