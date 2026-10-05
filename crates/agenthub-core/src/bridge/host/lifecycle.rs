use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant, SystemTime};

use axum::Router;
use futures_util::future::join_all;
use reqwest::Url;
use tokio::net::TcpListener;
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::bridge::route_index::EffectiveRouteIndex;
use crate::bridge::runtime::{
    BridgeRuntimeState, BridgeRuntimeStatus, BridgeStartSpec, BridgeUpstreamStatus,
};
use crate::bridge::BridgeMemberSpec;
use crate::models::RouteSchedulePolicy;

use super::gateway::{
    BridgeHostError, CleanupCompletion, EdgeRuntime, EdgeState, Gateway, GatewayRegistry,
    SocketInstance,
};
use super::http::router;
use super::inbound::InboundRequestRecord;
use super::inbound::InboundRequestStats;
use super::{
    DRAIN_TIMEOUT, FORCE_CANCEL_GRACE, MAX_IN_FLIGHT_REQUESTS_PER_PROFILE, TASK_POLL_INTERVAL,
};

#[cfg(test)]
mod tests;

/// Owns the in-process loopback gateway. A host belongs to one desktop-process lifetime: once
/// [`Self::shutdown`] begins, its closing latch rejects all further starts.
#[derive(Clone)]
pub struct BridgeRuntimeHost {
    gateway: Gateway,
    app: Router,
    closing: Arc<AtomicBool>,
    /// A gate is held only by operations on one profile. Slow graceful draining for profile A
    /// must not make profile B unavailable.
    profile_gates: Arc<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>>,
    /// Coordinates the short start-registration critical section with shutdown. It is never held
    /// while a listener drains, so it does not reintroduce a global stop/start bottleneck.
    registration: Arc<AsyncMutex<()>>,
    /// The first shutdown starts an owned background cleanup. Every later caller joins that same
    /// cleanup, including if the caller that initiated shutdown is cancelled.
    shutdown: Arc<AsyncMutex<Option<Arc<CleanupCompletion>>>>,
    /// Logical gate for a capture -> stop -> exact restore handoff. Operations
    /// take short owned permits; no synchronous guard is held across an await
    /// or while a per-profile gate is acquired.
    transition: Arc<Mutex<GatewayTransitionState>>,
}

impl Default for BridgeRuntimeHost {
    fn default() -> Self {
        let gateway = Gateway::new();
        Self {
            app: router(gateway.clone()),
            gateway,
            closing: Arc::new(AtomicBool::new(false)),
            profile_gates: Arc::new(Mutex::new(HashMap::new())),
            registration: Arc::new(AsyncMutex::new(())),
            shutdown: Arc::new(AsyncMutex::new(None)),
            transition: Arc::new(Mutex::new(GatewayTransitionState::default())),
        }
    }
}

#[derive(Default)]
struct GatewayTransitionState {
    next_id: u64,
    mutation_count: usize,
    active: Option<ActiveGatewayTransition>,
}

struct ActiveGatewayTransition {
    id: u64,
    owner_alive: bool,
    background_tasks: usize,
    fail_closed: bool,
}

struct HostMutationPermit {
    transition: Arc<Mutex<GatewayTransitionState>>,
}

impl Drop for HostMutationPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.transition.lock() {
            state.mutation_count = state.mutation_count.saturating_sub(1);
        }
    }
}

struct SnapshotTaskPermit {
    transition: Arc<Mutex<GatewayTransitionState>>,
    id: u64,
}

impl Drop for SnapshotTaskPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.transition.lock() {
            let clear = if let Some(active) = state.active.as_mut().filter(|row| row.id == self.id)
            {
                active.background_tasks = active.background_tasks.saturating_sub(1);
                !active.owner_alive && active.background_tasks == 0 && !active.fail_closed
            } else {
                false
            };
            if clear {
                state.active = None;
            }
        }
    }
}

struct GatewaySnapshotContents {
    port: u16,
    entries: Vec<GatewaySnapshotEntry>,
    extra_bearers: Vec<(String, String)>,
    state: BridgeGatewaySnapshotState,
    #[cfg(feature = "gateway-snapshot-probe")]
    probe_faults: GatewaySnapshotProbeFaults,
}

#[derive(Clone)]
struct GatewaySnapshotEntry {
    spec: BridgeStartSpec,
    observed_upstream: BridgeUpstreamStatus,
}

#[cfg(feature = "gateway-snapshot-probe")]
#[derive(Default)]
struct GatewaySnapshotProbeFaults {
    stop_error_after_cleanup: bool,
    fail_second_restore_entry: bool,
    fail_health: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeGatewaySnapshotState {
    Captured,
    Stopped,
    Restored,
    Committed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeGatewayStopState {
    Running,
    Stopped,
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeGatewayStopReport {
    pub state: BridgeGatewayStopState,
    pub stop_error_count: usize,
}

/// Opaque, process-local handoff state for the in-process Rust gateway.
///
/// It intentionally has no `Clone` or serialization implementation. Debug
/// output contains only counts and the non-secret loopback port.
pub struct BridgeGatewaySnapshot {
    contents: Arc<Mutex<GatewaySnapshotContents>>,
    operation: Arc<AsyncMutex<()>>,
    transition: Weak<Mutex<GatewayTransitionState>>,
    transition_id: u64,
}

impl BridgeGatewaySnapshot {
    pub fn port(&self) -> u16 {
        self.contents.lock().map(|row| row.port).unwrap_or_default()
    }

    pub fn entry_count(&self) -> usize {
        self.contents
            .lock()
            .map(|row| row.entries.len())
            .unwrap_or_default()
    }

    pub fn state(&self) -> BridgeGatewaySnapshotState {
        self.contents
            .lock()
            .map(|row| row.state)
            .unwrap_or(BridgeGatewaySnapshotState::Stopped)
    }

    #[cfg(feature = "gateway-snapshot-probe")]
    pub fn probe_set_faults(
        &self,
        stop_error_after_cleanup: bool,
        fail_second_restore_entry: bool,
        fail_health: bool,
    ) {
        if let Ok(mut contents) = self.contents.lock() {
            contents.probe_faults = GatewaySnapshotProbeFaults {
                stop_error_after_cleanup,
                fail_second_restore_entry,
                fail_health,
            };
        }
    }
}

impl fmt::Debug for BridgeGatewaySnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.contents.lock() {
            Ok(contents) => formatter
                .debug_struct("BridgeGatewaySnapshot")
                .field("port", &contents.port)
                .field("entry_count", &contents.entries.len())
                .field("extra_bearer_count", &contents.extra_bearers.len())
                .field("state", &contents.state)
                .finish(),
            Err(_) => formatter
                .debug_struct("BridgeGatewaySnapshot")
                .field("state", &"unavailable")
                .finish(),
        }
    }
}

impl Drop for BridgeGatewaySnapshot {
    fn drop(&mut self) {
        let Some(transition) = self.transition.upgrade() else {
            return;
        };
        let snapshot_state = self
            .contents
            .lock()
            .map(|contents| contents.state)
            .unwrap_or(BridgeGatewaySnapshotState::Stopped);
        if let Ok(mut state) = transition.lock() {
            let clear = if let Some(active) = state
                .active
                .as_mut()
                .filter(|row| row.id == self.transition_id)
            {
                active.owner_alive = false;
                active.fail_closed = snapshot_state == BridgeGatewaySnapshotState::Stopped;
                active.background_tasks == 0 && !active.fail_closed
            } else {
                false
            };
            if clear {
                state.active = None;
            }
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeGatewayCleanupStatus {
    NotRequired,
    Complete,
    Partial,
    Failed,
}

#[derive(Debug)]
pub struct BridgeGatewayRestoreError {
    cause: BridgeHostError,
    cleanup: BridgeGatewayCleanupStatus,
}

impl BridgeGatewayRestoreError {
    pub fn cause_error(&self) -> &BridgeHostError {
        &self.cause
    }

    pub fn cleanup_status(&self) -> BridgeGatewayCleanupStatus {
        self.cleanup
    }
}

impl fmt::Display for BridgeGatewayRestoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "bridge gateway restore failed: {}; cleanup={:?}",
            self.cause, self.cleanup
        )
    }
}

impl std::error::Error for BridgeGatewayRestoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

struct ActiveSocketTask {
    port: u16,
    task: JoinHandle<Result<(), ()>>,
}

const GATEWAY_SNAPSHOT_STOP_TIMEOUT: Duration = Duration::from_secs(12);
const GATEWAY_SNAPSHOT_HEALTH_TIMEOUT: Duration = Duration::from_secs(2);

impl BridgeRuntimeHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Installs the durable gateway usage spool directory. Must be called
    /// before edges start; later calls are ignored. Unset keeps capture a
    /// no-op, so CLI runs and tests never write spool files.
    pub fn set_usage_spool_dir(&self, dir: std::path::PathBuf) {
        self.gateway.usage_spool.set(std::sync::Arc::new(
            crate::bridge::usage_capture::UsageSpool::new(dir),
        ));
    }

    /// Enable disposable sqlite persistence for Activity monitor history.
    /// Loads the live UI ring from `path` when present; later calls are ignored.
    /// Unset keeps an in-memory ring only (CLI / unit tests).
    pub fn set_route_trace_persist_path(&self, path: std::path::PathBuf) {
        self.gateway.route_traces.enable_persist(path);
    }

    fn begin_mutation(&self) -> Result<HostMutationPermit, BridgeHostError> {
        let mut state = self
            .transition
            .lock()
            .map_err(|_| BridgeHostError::StatePoisoned)?;
        if state.active.is_some() {
            return Err(BridgeHostError::GatewayTransitionActive);
        }
        state.mutation_count = state.mutation_count.saturating_add(1);
        Ok(HostMutationPermit {
            transition: Arc::clone(&self.transition),
        })
    }

    fn begin_snapshot_task(&self, id: u64) -> Result<SnapshotTaskPermit, BridgeHostError> {
        let mut state = self
            .transition
            .lock()
            .map_err(|_| BridgeHostError::StatePoisoned)?;
        let active = state
            .active
            .as_mut()
            .filter(|row| row.id == id)
            .ok_or(BridgeHostError::GatewaySnapshotHostMismatch)?;
        active.background_tasks = active.background_tasks.saturating_add(1);
        Ok(SnapshotTaskPermit {
            transition: Arc::clone(&self.transition),
            id,
        })
    }

    fn validate_snapshot(&self, snapshot: &BridgeGatewaySnapshot) -> Result<(), BridgeHostError> {
        let Some(owner) = snapshot.transition.upgrade() else {
            return Err(BridgeHostError::GatewaySnapshotHostMismatch);
        };
        if !Arc::ptr_eq(&owner, &self.transition) {
            return Err(BridgeHostError::GatewaySnapshotHostMismatch);
        }
        self.validate_snapshot_id(snapshot.transition_id)
    }

    fn validate_snapshot_id(&self, id: u64) -> Result<(), BridgeHostError> {
        let state = self
            .transition
            .lock()
            .map_err(|_| BridgeHostError::StatePoisoned)?;
        if state
            .active
            .as_ref()
            .is_none_or(|active| active.id != id || !active.owner_alive)
        {
            return Err(BridgeHostError::GatewaySnapshotHostMismatch);
        }
        Ok(())
    }

    fn finish_snapshot_transition(&self, id: u64) {
        if let Ok(mut state) = self.transition.lock() {
            if let Some(active) = state.active.as_mut().filter(|row| row.id == id) {
                active.owner_alive = false;
                active.fail_closed = false;
                if active.background_tasks == 0 {
                    state.active = None;
                }
            }
        }
    }

    fn mark_snapshot_stopped(
        &self,
        snapshot: &BridgeGatewaySnapshot,
    ) -> Result<(), BridgeHostError> {
        {
            let mut contents = snapshot
                .contents
                .lock()
                .map_err(|_| BridgeHostError::StatePoisoned)?;
            match contents.state {
                BridgeGatewaySnapshotState::Captured | BridgeGatewaySnapshotState::Stopped => {
                    contents.state = BridgeGatewaySnapshotState::Stopped;
                }
                BridgeGatewaySnapshotState::Restored | BridgeGatewaySnapshotState::Committed => {
                    return Err(BridgeHostError::GatewaySnapshotHostMismatch);
                }
            }
        }
        let mut state = self
            .transition
            .lock()
            .map_err(|_| BridgeHostError::StatePoisoned)?;
        let active = state
            .active
            .as_mut()
            .filter(|row| row.id == snapshot.transition_id && row.owner_alive)
            .ok_or(BridgeHostError::GatewaySnapshotHostMismatch)?;
        active.fail_closed = true;
        Ok(())
    }

    /// Capture the exact live Rust gateway without reading or writing storage.
    /// Dropping an untouched capture releases the transition; after stop begins,
    /// only an exact restore or explicit stopped commit may release it.
    pub fn capture_gateway_snapshot(&self) -> Result<BridgeGatewaySnapshot, BridgeHostError> {
        if self.closing.load(Ordering::SeqCst) {
            return Err(BridgeHostError::HostClosing);
        }
        let transition_id = {
            let mut state = self
                .transition
                .lock()
                .map_err(|_| BridgeHostError::StatePoisoned)?;
            if state.active.is_some() || state.mutation_count != 0 {
                return Err(BridgeHostError::GatewayTransitionActive);
            }
            state.next_id = state.next_id.wrapping_add(1).max(1);
            let id = state.next_id;
            state.active = Some(ActiveGatewayTransition {
                id,
                owner_alive: true,
                background_tasks: 0,
                fail_closed: false,
            });
            id
        };

        let captured = (|| {
            // Authentication reads use this same order. Keeping both guards
            // alive makes the bearer table and runtime table one capture point.
            let extra_bearers = self.gateway.lock_extra_bearers()?;
            let registry = self.gateway.lock()?;
            let mut live_ports = registry
                .sockets
                .iter()
                .filter(|(_, socket)| socket.task.as_ref().is_some_and(|task| !task.is_finished()))
                .map(|(port, _)| *port);
            let port = live_ports
                .next()
                .filter(|port| *port != 0)
                .ok_or(BridgeHostError::GatewaySnapshotUnavailable)?;
            if live_ports.next().is_some()
                || registry.sockets.len() != 1
                || registry.primary_port != Some(port)
                || registry.runtimes.is_empty()
                || registry.runtimes.values().any(|runtime| {
                    runtime.lifecycle != BridgeRuntimeState::Running || runtime.cited_port != port
                })
            {
                return Err(BridgeHostError::GatewaySnapshotUnavailable);
            }
            let mut entries = registry
                .runtimes
                .values()
                .map(snapshot_entry_from_runtime)
                .collect::<Vec<_>>();
            entries.sort_by(|left, right| left.spec.profile_id.cmp(&right.spec.profile_id));
            let extra_bearers = extra_bearers
                .iter()
                .map(|(token, profile_id)| (token.to_string(), profile_id.to_string()))
                .collect();
            Ok(GatewaySnapshotContents {
                port,
                entries,
                extra_bearers,
                state: BridgeGatewaySnapshotState::Captured,
                #[cfg(feature = "gateway-snapshot-probe")]
                probe_faults: GatewaySnapshotProbeFaults::default(),
            })
        })();

        match captured {
            Ok(contents) => Ok(BridgeGatewaySnapshot {
                contents: Arc::new(Mutex::new(contents)),
                operation: Arc::new(AsyncMutex::new(())),
                transition: Arc::downgrade(&self.transition),
                transition_id,
            }),
            Err(error) => {
                self.finish_snapshot_transition(transition_id);
                Err(error)
            }
        }
    }

    /// Cancellation-safe, bounded stop of every entry held by `snapshot`.
    /// Missing entries are already stopped; draining entries are awaited.
    pub async fn stop_gateway_snapshot(
        &self,
        snapshot: &BridgeGatewaySnapshot,
    ) -> Result<BridgeGatewayStopReport, BridgeHostError> {
        self.validate_snapshot(snapshot)?;
        self.mark_snapshot_stopped(snapshot)?;
        let task_permit = self.begin_snapshot_task(snapshot.transition_id)?;
        let host = self.clone();
        let contents = Arc::clone(&snapshot.contents);
        let operation = Arc::clone(&snapshot.operation);
        let transition_id = snapshot.transition_id;
        tokio::spawn(async move {
            let _task_permit = task_permit;
            let _operation = operation.lock_owned().await;
            host.validate_snapshot_id(transition_id)?;
            host.stop_snapshot_owned(contents, transition_id).await
        })
        .await
        .map_err(|_| BridgeHostError::GatewaySnapshotTaskFailed)?
    }

    /// Restore every captured entry on the exact cited port, then authenticate
    /// a local `/health` request for every edge before releasing the transition.
    pub async fn restore_gateway_snapshot(
        &self,
        snapshot: &BridgeGatewaySnapshot,
    ) -> Result<(), BridgeGatewayRestoreError> {
        self.validate_snapshot(snapshot)
            .map_err(|cause| BridgeGatewayRestoreError {
                cause,
                cleanup: BridgeGatewayCleanupStatus::Failed,
            })?;
        self.mark_snapshot_stopped(snapshot)
            .map_err(|cause| BridgeGatewayRestoreError {
                cause,
                cleanup: BridgeGatewayCleanupStatus::Failed,
            })?;
        let task_permit = self
            .begin_snapshot_task(snapshot.transition_id)
            .map_err(|cause| BridgeGatewayRestoreError {
                cause,
                cleanup: BridgeGatewayCleanupStatus::Failed,
            })?;
        let host = self.clone();
        let contents = Arc::clone(&snapshot.contents);
        let operation = Arc::clone(&snapshot.operation);
        let transition_id = snapshot.transition_id;
        tokio::spawn(async move {
            let _task_permit = task_permit;
            let _operation = operation.lock_owned().await;
            host.validate_snapshot_id(transition_id).map_err(|cause| {
                BridgeGatewayRestoreError {
                    cause,
                    cleanup: BridgeGatewayCleanupStatus::Failed,
                }
            })?;
            host.restore_snapshot_owned(contents, transition_id).await
        })
        .await
        .map_err(|_| BridgeGatewayRestoreError {
            cause: BridgeHostError::GatewaySnapshotTaskFailed,
            cleanup: BridgeGatewayCleanupStatus::Failed,
        })?
    }

    /// Explicitly accept a fully stopped Rust gateway. This is reserved for a
    /// future backend-switch saga after the replacement runtime is healthy.
    /// Dropping a stopped snapshot without this call remains fail-closed.
    pub async fn commit_stopped_gateway_snapshot(
        &self,
        snapshot: &BridgeGatewaySnapshot,
    ) -> Result<(), BridgeHostError> {
        self.validate_snapshot(snapshot)?;
        let _operation = snapshot.operation.lock().await;
        self.validate_snapshot(snapshot)?;
        if snapshot.state() != BridgeGatewaySnapshotState::Stopped
            || self.observe_snapshot_stop(&snapshot.contents)? != BridgeGatewayStopState::Stopped
        {
            return Err(BridgeHostError::GatewaySnapshotUnavailable);
        }
        snapshot
            .contents
            .lock()
            .map_err(|_| BridgeHostError::StatePoisoned)?
            .state = BridgeGatewaySnapshotState::Committed;
        self.finish_snapshot_transition(snapshot.transition_id);
        Ok(())
    }

    async fn stop_snapshot_owned(
        &self,
        contents: Arc<Mutex<GatewaySnapshotContents>>,
        transition_id: u64,
    ) -> Result<BridgeGatewayStopReport, BridgeHostError> {
        let profile_ids = {
            let snapshot = contents
                .lock()
                .map_err(|_| BridgeHostError::StatePoisoned)?;
            snapshot
                .entries
                .iter()
                .map(|entry| entry.spec.profile_id.clone())
                .collect::<Vec<_>>()
        };
        let stops = profile_ids.iter().map(|profile_id| {
            self.stop_inner(profile_id, Some((Arc::clone(&contents), transition_id)))
        });
        let mut stop_error_count = 0;
        for result in join_all(stops).await {
            match result {
                Ok(_) | Err(BridgeHostError::NotRunning | BridgeHostError::Stopping) => {}
                Err(_) => stop_error_count += 1,
            }
        }

        #[cfg(feature = "gateway-snapshot-probe")]
        if contents
            .lock()
            .map_err(|_| BridgeHostError::StatePoisoned)?
            .probe_faults
            .stop_error_after_cleanup
        {
            stop_error_count += 1;
        }

        let deadline = Instant::now() + GATEWAY_SNAPSHOT_STOP_TIMEOUT;
        loop {
            let state = self.observe_snapshot_stop(&contents)?;
            match state {
                BridgeGatewayStopState::Stopped => {
                    self.gateway.set_extra_bearers(Vec::new())?;
                    return Ok(BridgeGatewayStopReport {
                        state,
                        stop_error_count,
                    });
                }
                BridgeGatewayStopState::Running => {
                    return Ok(BridgeGatewayStopReport {
                        state,
                        stop_error_count,
                    });
                }
                BridgeGatewayStopState::Partial if Instant::now() >= deadline => {
                    return Ok(BridgeGatewayStopReport {
                        state,
                        stop_error_count,
                    });
                }
                BridgeGatewayStopState::Partial => {}
            }
            tokio::time::sleep(TASK_POLL_INTERVAL).await;
        }
    }

    fn observe_snapshot_stop(
        &self,
        contents: &Arc<Mutex<GatewaySnapshotContents>>,
    ) -> Result<BridgeGatewayStopState, BridgeHostError> {
        let registry = self.gateway.lock()?;
        let contents = contents
            .lock()
            .map_err(|_| BridgeHostError::StatePoisoned)?;
        let port = contents.port;
        if registry.runtimes.is_empty() && !registry.sockets.contains_key(&port) {
            return Ok(BridgeGatewayStopState::Stopped);
        }
        let socket_live = registry
            .sockets
            .get(&port)
            .is_some_and(|socket| socket.task.as_ref().is_some_and(|task| !task.is_finished()));
        let fully_running = socket_live
            && registry.sockets.len() == 1
            && registry.runtimes.len() == contents.entries.len()
            && contents.entries.iter().all(|entry| {
                registry
                    .runtimes
                    .get(&entry.spec.profile_id)
                    .is_some_and(|runtime| {
                        runtime.lifecycle == BridgeRuntimeState::Running
                            && runtime.cited_port == port
                    })
            });
        Ok(if fully_running {
            BridgeGatewayStopState::Running
        } else {
            BridgeGatewayStopState::Partial
        })
    }

    async fn restore_snapshot_owned(
        &self,
        contents: Arc<Mutex<GatewaySnapshotContents>>,
        transition_id: u64,
    ) -> Result<(), BridgeGatewayRestoreError> {
        let stopped = self
            .stop_snapshot_owned(Arc::clone(&contents), transition_id)
            .await
            .map_err(|cause| BridgeGatewayRestoreError {
                cause,
                cleanup: BridgeGatewayCleanupStatus::Failed,
            })?;
        match stopped.state {
            BridgeGatewayStopState::Stopped => {}
            BridgeGatewayStopState::Partial => {
                return Err(BridgeGatewayRestoreError {
                    cause: BridgeHostError::GatewaySnapshotStopTimeout,
                    cleanup: BridgeGatewayCleanupStatus::Partial,
                });
            }
            BridgeGatewayStopState::Running => {
                return Err(BridgeGatewayRestoreError {
                    cause: BridgeHostError::GatewaySnapshotStopTimeout,
                    cleanup: BridgeGatewayCleanupStatus::Failed,
                });
            }
        }
        let (port, entries, extra_bearers) = {
            let snapshot = contents.lock().map_err(|_| BridgeGatewayRestoreError {
                cause: BridgeHostError::StatePoisoned,
                cleanup: BridgeGatewayCleanupStatus::Complete,
            })?;
            (
                snapshot.port,
                snapshot.entries.clone(),
                snapshot.extra_bearers.clone(),
            )
        };
        let mut started = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            let _ = index;
            #[cfg(feature = "gateway-snapshot-probe")]
            if index == 1
                && contents
                    .lock()
                    .map_err(|_| BridgeGatewayRestoreError {
                        cause: BridgeHostError::StatePoisoned,
                        cleanup: BridgeGatewayCleanupStatus::Failed,
                    })?
                    .probe_faults
                    .fail_second_restore_entry
            {
                return Err(self
                    .restore_failure(
                        BridgeHostError::StatePoisoned,
                        &contents,
                        transition_id,
                        &started,
                    )
                    .await);
            }
            match self.start_inner(entry.spec.clone()).await {
                Ok(status)
                    if status.running
                        && status.state == BridgeRuntimeState::Running
                        && status.port == port =>
                {
                    started.push(status.profile_id.clone());
                    if let Err(cause) = self
                        .restore_snapshot_observation(&status.profile_id, entry.observed_upstream)
                    {
                        return Err(self
                            .restore_failure(cause, &contents, transition_id, &started)
                            .await);
                    }
                }
                Ok(status) => {
                    started.push(status.profile_id);
                    return Err(self
                        .restore_failure(
                            BridgeHostError::GatewaySnapshotUnavailable,
                            &contents,
                            transition_id,
                            &started,
                        )
                        .await);
                }
                Err(cause) => {
                    return Err(self
                        .restore_failure(cause, &contents, transition_id, &started)
                        .await);
                }
            }
        }
        if let Err(cause) = self.gateway.set_extra_bearers(extra_bearers) {
            return Err(self
                .restore_failure(cause, &contents, transition_id, &started)
                .await);
        }
        #[cfg(feature = "gateway-snapshot-probe")]
        let fail_health = contents
            .lock()
            .map_err(|_| BridgeGatewayRestoreError {
                cause: BridgeHostError::StatePoisoned,
                cleanup: BridgeGatewayCleanupStatus::Failed,
            })?
            .probe_faults
            .fail_health;
        #[cfg(not(feature = "gateway-snapshot-probe"))]
        let fail_health = false;
        let health = if fail_health {
            Err(BridgeHostError::GatewaySnapshotHealthFailed)
        } else {
            self.verify_restored_snapshot(port, &entries).await
        };
        if let Err(cause) = health {
            return Err(self
                .restore_failure(cause, &contents, transition_id, &started)
                .await);
        }
        contents
            .lock()
            .map_err(|_| BridgeGatewayRestoreError {
                cause: BridgeHostError::StatePoisoned,
                cleanup: BridgeGatewayCleanupStatus::Failed,
            })?
            .state = BridgeGatewaySnapshotState::Restored;
        self.finish_snapshot_transition(transition_id);
        Ok(())
    }

    async fn restore_failure(
        &self,
        cause: BridgeHostError,
        contents: &Arc<Mutex<GatewaySnapshotContents>>,
        transition_id: u64,
        started: &[String],
    ) -> BridgeGatewayRestoreError {
        if started.is_empty() {
            return BridgeGatewayRestoreError {
                cause,
                cleanup: BridgeGatewayCleanupStatus::NotRequired,
            };
        }
        let cleanup = match self
            .stop_snapshot_owned(Arc::clone(contents), transition_id)
            .await
        {
            Ok(report) if report.state == BridgeGatewayStopState::Stopped => {
                BridgeGatewayCleanupStatus::Complete
            }
            Ok(report) if report.state == BridgeGatewayStopState::Partial => {
                BridgeGatewayCleanupStatus::Partial
            }
            Ok(_) => BridgeGatewayCleanupStatus::Failed,
            Err(_) => BridgeGatewayCleanupStatus::Failed,
        };
        BridgeGatewayRestoreError { cause, cleanup }
    }

    async fn verify_restored_snapshot(
        &self,
        port: u16,
        entries: &[GatewaySnapshotEntry],
    ) -> Result<(), BridgeHostError> {
        {
            let registry = self.gateway.lock()?;
            if registry.primary_port != Some(port)
                || registry.sockets.len() != 1
                || !registry.sockets.contains_key(&port)
                || registry.runtimes.len() != entries.len()
                || entries.iter().any(|entry| {
                    registry
                        .runtimes
                        .get(&entry.spec.profile_id)
                        .is_none_or(|runtime| {
                            runtime.lifecycle != BridgeRuntimeState::Running
                                || runtime.cited_port != port
                                || runtime.state.observed_upstream() != entry.observed_upstream
                        })
                })
            {
                return Err(BridgeHostError::GatewaySnapshotUnavailable);
            }
        }
        let client = reqwest::Client::builder()
            .connect_timeout(GATEWAY_SNAPSHOT_HEALTH_TIMEOUT)
            .timeout(GATEWAY_SNAPSHOT_HEALTH_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| BridgeHostError::GatewaySnapshotHealthFailed)?;
        let url = format!("http://127.0.0.1:{port}/health");
        for entry in entries {
            let response = client
                .get(&url)
                .bearer_auth(&entry.spec.local_token)
                .send()
                .await
                .map_err(|_| BridgeHostError::GatewaySnapshotHealthFailed)?;
            if response.status() != reqwest::StatusCode::OK {
                return Err(BridgeHostError::GatewaySnapshotHealthFailed);
            }
        }
        Ok(())
    }

    fn restore_snapshot_observation(
        &self,
        profile_id: &str,
        observed_upstream: BridgeUpstreamStatus,
    ) -> Result<(), BridgeHostError> {
        let registry = self.gateway.lock()?;
        let runtime = registry
            .runtimes
            .get(profile_id)
            .ok_or(BridgeHostError::GatewaySnapshotUnavailable)?;
        runtime.state.record_upstream(observed_upstream);
        Ok(())
    }

    /// Starts an edge and ensures a loopback socket. Repeating an exact live start is
    /// idempotent; attempting to start while a matching profile drains fails rather than
    /// racing a second edge.
    pub async fn start(
        &self,
        spec: BridgeStartSpec,
    ) -> Result<BridgeRuntimeStatus, BridgeHostError> {
        let _mutation = self.begin_mutation()?;
        self.start_inner(spec).await
    }

    async fn start_inner(
        &self,
        spec: BridgeStartSpec,
    ) -> Result<BridgeRuntimeStatus, BridgeHostError> {
        let upstream_url = validate_start_spec(&spec)?;
        if self.closing.load(Ordering::SeqCst) {
            return Err(BridgeHostError::HostClosing);
        }

        {
            let registry = self.gateway.lock()?;
            if let Some(existing) = registry.runtimes.get(&spec.profile_id) {
                match existing.lifecycle {
                    BridgeRuntimeState::Stopping => return Err(BridgeHostError::Stopping),
                    BridgeRuntimeState::Running | BridgeRuntimeState::Starting
                        if registry.sockets_live() =>
                    {
                        if same_spec(&existing.spec, &spec) {
                            return Ok(existing.status(true));
                        }
                        return Err(BridgeHostError::ConflictingStart);
                    }
                    _ => {}
                }
            }
        }

        let gate = self.profile_gate(&spec.profile_id)?;
        let _profile_operation = gate.lock_owned().await;
        let _registration = self.registration.lock().await;
        if self.closing.load(Ordering::SeqCst) {
            return Err(BridgeHostError::HostClosing);
        }
        let mut registry = self.gateway.lock()?;
        if let Some(existing) = registry.runtimes.get(&spec.profile_id) {
            match existing.lifecycle {
                BridgeRuntimeState::Stopping => return Err(BridgeHostError::Stopping),
                BridgeRuntimeState::Running | BridgeRuntimeState::Starting
                    if registry.sockets_live() =>
                {
                    if same_spec(&existing.spec, &spec) {
                        return Ok(existing.status(true));
                    }
                    return Err(BridgeHostError::ConflictingStart);
                }
                _ => {
                    registry.runtimes.remove(&spec.profile_id);
                }
            }
        }
        if registry.token_owned_by_other(&spec) {
            return Err(BridgeHostError::ConflictingStart);
        }

        let cited_port = ensure_socket(&mut registry, spec.port, self.app.clone())?;
        let force_shutdown = CancellationToken::new();
        let state = EdgeState::from_spec(
            &spec,
            upstream_url,
            force_shutdown,
            self.gateway.auth_reload.clone(),
            self.gateway.usage_spool.clone(),
            self.gateway.route_traces.clone(),
        );
        let runtime = EdgeRuntime {
            spec,
            cited_port,
            started_at: SystemTime::now(),
            lifecycle: BridgeRuntimeState::Running,
            state,
            stop_completion: None,
        };
        let status = runtime.status(true);
        registry.runtimes.insert(status.profile_id.clone(), runtime);
        Ok(status)
    }

    pub fn live_route_index(
        &self,
        profile_id: &str,
    ) -> Result<Option<EffectiveRouteIndex>, BridgeHostError> {
        let registry = self.gateway.lock()?;
        Ok(registry
            .runtimes
            .get(profile_id)
            .and_then(|runtime| runtime.spec.route_index.clone()))
    }

    pub fn status(&self, profile_id: &str) -> Result<Option<BridgeRuntimeStatus>, BridgeHostError> {
        let registry = self.gateway.lock()?;
        let live = registry.sockets_live();
        Ok(registry
            .runtimes
            .get(profile_id)
            .map(|runtime| runtime.status(live)))
    }

    /// Extra named loopback bearers accepted besides each edge's primary token.
    pub fn set_extra_local_bearers(
        &self,
        rows: Vec<(String, String)>,
    ) -> Result<(), BridgeHostError> {
        let _mutation = self.begin_mutation()?;
        self.gateway.set_extra_bearers(rows)
    }

    /// The loopback bearer this listener actually accepts. Empty when not running.
    pub fn local_token(&self, profile_id: &str) -> Result<Option<String>, BridgeHostError> {
        let registry = self.gateway.lock()?;
        Ok(registry
            .runtimes
            .get(profile_id)
            .map(|runtime| runtime.state.local_token.as_ref().to_owned()))
    }

    /// Last inbound requests for this profile (newest first). Credential-free.
    pub fn recent_inbound(&self, profile_id: &str) -> Vec<InboundRequestRecord> {
        self.gateway.inbound.recent(profile_id)
    }

    /// Last route request traces for this profile (newest first). Credential-free.
    pub fn recent_route_traces(
        &self,
        profile_id: &str,
    ) -> Vec<super::route_trace::RouteRequestTrace> {
        self.gateway.route_traces.recent(profile_id)
    }

    /// Failed local-auth attempts without a profile binding (newest first).
    pub fn recent_unauthenticated_route_traces(
        &self,
    ) -> Vec<super::route_trace::RouteRequestTrace> {
        self.gateway.route_traces.recent_unauthenticated()
    }

    pub fn query_route_traces(
        &self,
        query: super::route_trace::RouteTraceQuery,
    ) -> super::route_trace::RouteTracePage {
        self.gateway.route_traces.query(query)
    }

    pub fn delete_route_traces(
        &self,
        request_ids: &[String],
    ) -> super::route_trace::RouteTraceDeleteResult {
        self.gateway.route_traces.delete_ids(request_ids)
    }

    /// Process-lifetime inbound counters for this profile (not capped by the ring).
    pub fn inbound_stats(&self, profile_id: &str) -> InboundRequestStats {
        self.gateway.inbound.stats(profile_id)
    }

    pub fn statuses(&self) -> Result<Vec<BridgeRuntimeStatus>, BridgeHostError> {
        let registry = self.gateway.lock()?;
        let live = registry.sockets_live();
        Ok(registry
            .runtimes
            .values()
            .map(|runtime| runtime.status(live))
            .collect())
    }

    /// Acquires one real request-admission slot for the disposable gateway
    /// snapshot process probe. Production builds expose no such bypass.
    #[cfg(feature = "gateway-snapshot-probe")]
    pub fn probe_hold_admission(
        &self,
        profile_id: &str,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, BridgeHostError> {
        let admission = self
            .gateway
            .lock()?
            .runtimes
            .get(profile_id)
            .ok_or(BridgeHostError::NotRunning)?
            .state
            .admission
            .clone();
        admission
            .try_acquire_owned()
            .map_err(|_| BridgeHostError::StatePoisoned)
    }

    /// Records the last observed health or request outcome. Status/health reads
    /// later report this stored value and must not issue a new upstream probe.
    pub fn record_upstream_outcome(
        &self,
        profile_id: &str,
        status: BridgeUpstreamStatus,
    ) -> Result<Option<BridgeRuntimeStatus>, BridgeHostError> {
        let _mutation = self.begin_mutation()?;
        let registry = self.gateway.lock()?;
        let Some(runtime) = registry.runtimes.get(profile_id) else {
            return Ok(None);
        };
        runtime.state.record_upstream(status);
        Ok(Some(runtime.status(registry.sockets_live())))
    }

    /// Re-admit an isolated member after reconcile / re-login. Does not restart
    /// the listener or rotate the local bearer.
    pub fn restore_member_health(
        &self,
        profile_id: &str,
        source_id: &str,
        health: crate::bridge::account::MemberHealth,
    ) -> Result<(), BridgeHostError> {
        let _mutation = self.begin_mutation()?;
        let registry = self.gateway.lock()?;
        let runtime = registry
            .runtimes
            .get(profile_id)
            .ok_or(BridgeHostError::NotRunning)?;
        runtime.state.account_picker.restore(source_id, health);
        if health.is_eligible() {
            if let Some(member) = runtime
                .state
                .account_picker
                .members()
                .iter()
                .find(|member| member.source_id == source_id)
            {
                runtime
                    .state
                    .auth_reload
                    .clear_isolated(&member.authorization_fingerprint());
            }
        }
        Ok(())
    }

    /// Hot-apply a pool schedule onto every live runtime keyed by that pool id
    /// or whose route index `route_id` is that pool. Profile-keyed enroll
    /// listeners are included. Sticky, continuation, cooldown, and health stay.
    /// The stored spec is updated so a later identical `start` stays idempotent.
    /// Does not stop or restart the listener.
    pub fn apply_pool_schedule_policy(
        &self,
        pool_id: &str,
        policy: RouteSchedulePolicy,
    ) -> Result<usize, BridgeHostError> {
        let _mutation = self.begin_mutation()?;
        let pool_id = pool_id.trim();
        if pool_id.is_empty() {
            return Ok(0);
        }
        let mut registry = self.gateway.lock()?;
        let mut updated = 0;
        for runtime in registry.runtimes.values_mut() {
            if !runtime_matches_pool(runtime, pool_id) {
                continue;
            }
            runtime.spec.schedule_policy = policy;
            runtime.state.account_picker.apply_schedule_policy(policy);
            updated += 1;
        }
        Ok(updated)
    }

    /// Hot-apply one account's quota hint onto every live listener that contains
    /// it. Ranking changes on the next pick. Sticky and continuation stay.
    /// Stored member specs are updated so a later identical `start` stays idempotent.
    pub fn apply_account_quota(
        &self,
        source_id: &str,
        remaining_pct: Option<f64>,
        reset_at: Option<SystemTime>,
        fresh_until: Option<SystemTime>,
        credit: bool,
    ) -> Result<usize, BridgeHostError> {
        let _mutation = self.begin_mutation()?;
        let source_id = source_id.trim();
        if source_id.is_empty() {
            return Ok(0);
        }
        let mut registry = self.gateway.lock()?;
        let mut updated = 0;
        for runtime in registry.runtimes.values_mut() {
            let picker_hit = runtime.state.account_picker.apply_member_quota(
                source_id,
                remaining_pct,
                reset_at,
                fresh_until,
                credit,
            );
            let mut spec_hit = false;
            for member in &mut runtime.spec.members {
                if member.source_id != source_id {
                    continue;
                }
                member.quota_remaining_pct = remaining_pct.filter(|value| value.is_finite());
                member.quota_reset_at = reset_at;
                member.quota_fresh_until = fresh_until;
                member.quota_credit = credit;
                spec_hit = true;
            }
            if picker_hit || spec_hit {
                updated += 1;
            }
        }
        Ok(updated)
    }

    fn profile_gate(&self, profile_id: &str) -> Result<Arc<AsyncMutex<()>>, BridgeHostError> {
        let mut gates = self
            .profile_gates
            .lock()
            .map_err(|_| BridgeHostError::StatePoisoned)?;
        Ok(gates
            .entry(profile_id.to_owned())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone())
    }

    pub async fn stop(&self, profile_id: &str) -> Result<BridgeRuntimeStatus, BridgeHostError> {
        let _mutation = self.begin_mutation()?;
        self.stop_inner(profile_id, None).await
    }

    async fn stop_inner(
        &self,
        profile_id: &str,
        snapshot: Option<(Arc<Mutex<GatewaySnapshotContents>>, u64)>,
    ) -> Result<BridgeRuntimeStatus, BridgeHostError> {
        let strict_snapshot = snapshot.is_some();
        let gate = self.profile_gate(profile_id)?;
        let profile_operation = gate.lock_owned().await;
        let (edge, cited_port, stopped, completion) = {
            let mut registry = self.gateway.lock()?;
            let sockets_live = registry.sockets_live();
            let runtime = registry
                .runtimes
                .get_mut(profile_id)
                .ok_or(BridgeHostError::NotRunning)?;
            if runtime.lifecycle == BridgeRuntimeState::Stopping && !strict_snapshot {
                return Err(BridgeHostError::Stopping);
            }
            if !sockets_live && !strict_snapshot {
                registry.runtimes.remove(profile_id);
                return Err(BridgeHostError::NotRunning);
            }
            runtime.lifecycle = BridgeRuntimeState::Stopping;
            runtime.state.stopping.store(true, Ordering::SeqCst);
            if !strict_snapshot {
                runtime.state.record_upstream(BridgeUpstreamStatus::Stopped);
            }
            let stopped = runtime.stopped_status();
            let completion = Arc::new(CleanupCompletion::new());
            runtime.stop_completion = Some(completion.clone());
            (
                runtime.state.clone(),
                runtime.cited_port,
                stopped,
                completion,
            )
        };

        let gateway = self.gateway.clone();
        let snapshot_task = snapshot
            .as_ref()
            .map(|(_, transition_id)| self.begin_snapshot_task(*transition_id))
            .transpose()?;
        let snapshot_contents = snapshot.map(|(contents, _)| contents);
        let profile_id = profile_id.to_owned();
        let cleanup_completion = completion.clone();
        tokio::spawn(async move {
            let _snapshot_task = snapshot_task;
            let _profile_operation = profile_operation;
            let fully_drained = drain_edge(&edge).await;
            if strict_snapshot && !fully_drained {
                cleanup_completion.finish(false);
                return;
            }
            let sockets = {
                let mut registry = match gateway.lock() {
                    Ok(registry) => registry,
                    Err(_) => {
                        cleanup_completion.finish(true);
                        return;
                    }
                };
                if let Some(contents) = snapshot_contents.as_ref() {
                    if let Some(runtime) = registry.runtimes.get(&profile_id) {
                        reconcile_snapshot_entry(contents, runtime);
                    }
                }
                registry.runtimes.remove(&profile_id);
                take_unbind_tasks(&mut registry, cited_port, &profile_id)
            };
            let failed = drain_socket_tasks(sockets).await;
            cleanup_completion.finish(failed);
        });

        completion.wait().await?;
        Ok(stopped)
    }

    /// Stops every edge and unbinds remaining sockets. All accepts are closed before any
    /// listener is awaited; after a short drain deadline remaining socket tasks are aborted.
    pub async fn shutdown(&self) -> Result<(), BridgeHostError> {
        let _mutation = self.begin_mutation()?;
        let (completion, starts_cleanup) = {
            let mut shutdown = self.shutdown.lock().await;
            if let Some(completion) = shutdown.as_ref() {
                (completion.clone(), false)
            } else {
                let _registration = self.registration.lock().await;
                self.closing.store(true, Ordering::SeqCst);
                let completion = Arc::new(CleanupCompletion::new());
                *shutdown = Some(completion.clone());
                (completion, true)
            }
        };
        if starts_cleanup {
            tokio::spawn(run_shutdown(self.gateway.clone(), completion.clone()));
        }
        completion.wait().await
    }

    /// Waits for in-flight graceful shutdown tasks. `shutdown` first requests listener draining.
    pub async fn drain(&self) -> Result<(), BridgeHostError> {
        self.shutdown().await
    }

    /// Current unified loopback port, if any socket is live.
    pub fn gateway_port(&self) -> Result<Option<u16>, BridgeHostError> {
        let registry = self.gateway.lock()?;
        Ok(registry
            .primary_port
            .filter(|port| registry.sockets.contains_key(port)))
    }

    /// Live edge ids citing the unified gateway. Empty when the relay is down.
    pub fn running_ids(&self) -> Result<Vec<String>, BridgeHostError> {
        let registry = self.gateway.lock()?;
        if !registry.sockets_live() {
            return Ok(Vec::new());
        }
        Ok(registry.runtimes.keys().cloned().collect())
    }

    /// Move the unified gateway socket to `port`.
    ///
    /// Occupancy or bind failure leaves existing sockets, citers, and client-facing
    /// ports unchanged. Edges that already share the old primary follow the new
    /// port; explicit alias ports stay bound until they have no remaining citers.
    pub async fn set_gateway_port(&self, port: u16) -> Result<u16, BridgeHostError> {
        let _mutation = self.begin_mutation()?;
        if port == 0 {
            return Err(BridgeHostError::InvalidGatewayPort);
        }
        if self.closing.load(Ordering::SeqCst) {
            return Err(BridgeHostError::HostClosing);
        }
        let _registration = self.registration.lock().await;
        if self.closing.load(Ordering::SeqCst) {
            return Err(BridgeHostError::HostClosing);
        }

        let unbind = {
            let mut registry = self.gateway.lock()?;
            prune_dead_sockets(&mut registry);
            if registry.primary_port == Some(port) && registry.sockets.contains_key(&port) {
                return Ok(port);
            }
            let old_primary = registry.primary_port;
            if !registry.sockets.contains_key(&port) {
                let listener = bind_loopback(port)?;
                let (bound, socket) = listen_on(listener, self.app.clone())?;
                debug_assert_eq!(bound, port);
                registry.sockets.insert(bound, socket);
            }
            registry.primary_port = Some(port);
            if let Some(old) = old_primary.filter(|old| *old != port) {
                for runtime in registry.runtimes.values_mut() {
                    if runtime.cited_port == old {
                        runtime.cited_port = port;
                    }
                }
                if registry.remaining_citers(old, None) == 0 {
                    take_socket_tasks(&mut registry, &[old])
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        };
        let _ = drain_socket_tasks(unbind).await;
        Ok(port)
    }
}

fn ensure_socket(
    registry: &mut GatewayRegistry,
    requested: u16,
    app: Router,
) -> Result<u16, BridgeHostError> {
    prune_dead_sockets(registry);
    if requested == 0 {
        if let Some(primary) = registry
            .primary_port
            .filter(|port| registry.sockets.contains_key(port))
        {
            return Ok(primary);
        }
    } else if registry.sockets.contains_key(&requested) {
        return Ok(requested);
    }

    let listener = bind_loopback(requested)?;
    let (port, socket) = listen_on(listener, app)?;
    registry.sockets.insert(port, socket);
    if registry.primary_port.is_none() {
        registry.primary_port = Some(port);
    }
    Ok(port)
}

fn listen_on(listener: TcpListener, app: Router) -> Result<(u16, SocketInstance), BridgeHostError> {
    let port = listener.local_addr()?.port();
    let accept_shutdown = CancellationToken::new();
    let task_shutdown = accept_shutdown.clone();
    let task = tokio::spawn(async move {
        tracing::info!(
            target: "core.adapter",
            op = "serve",
            port,
            "bridge listener started"
        );
        match axum::serve(listener, app)
            .with_graceful_shutdown(async move { task_shutdown.cancelled().await })
            .await
        {
            Ok(()) => Ok(()),
            Err(_) => {
                tracing::warn!(
                    target: "core.adapter",
                    op = "serve",
                    port,
                    code = "listener_error",
                    "bridge listener stopped unexpectedly"
                );
                Err(())
            }
        }
    });
    Ok((
        port,
        SocketInstance {
            accept_shutdown,
            task: Some(task),
        },
    ))
}

fn prune_dead_sockets(registry: &mut GatewayRegistry) {
    registry
        .sockets
        .retain(|_, socket| socket.task.as_ref().is_some_and(|task| !task.is_finished()));
    if registry
        .primary_port
        .is_some_and(|port| !registry.sockets.contains_key(&port))
    {
        registry.primary_port = registry.sockets.keys().copied().next();
    }
}

fn bind_loopback(port: u16) -> Result<TcpListener, BridgeHostError> {
    let requested = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    #[cfg(windows)]
    {
        // SO_EXCLUSIVEADDRUSE: another process cannot steal this port via SO_REUSEADDR.
        let socket = socket2::Socket::new(
            socket2::Domain::IPV4,
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )?;
        socket.set_reuse_address(false)?;
        set_exclusiveaddruse(&socket)?;
        socket.bind(&requested.into())?;
        socket.listen(1024)?;
        socket.set_nonblocking(true)?;
        let std_listener: std::net::TcpListener = socket.into();
        Ok(TcpListener::from_std(std_listener)?)
    }
    #[cfg(not(windows))]
    {
        // SO_REUSEADDR shortens the TIME_WAIT gap after tauri/dev hot-reload or
        // graceful stop so the same loopback port can rebind without connection refused.
        let socket = tokio::net::TcpSocket::new_v4()?;
        socket.set_reuseaddr(true)?;
        socket.bind(requested)?;
        Ok(socket.listen(1024)?)
    }
}

/// socket2 0.5 has no `set_exclusiveaddruse`; set SO_EXCLUSIVEADDRUSE via Winsock.
#[cfg(windows)]
fn set_exclusiveaddruse(socket: &socket2::Socket) -> std::io::Result<()> {
    use std::os::windows::io::AsRawSocket;

    const SOL_SOCKET: i32 = 0xffff;
    const SO_EXCLUSIVEADDRUSE: i32 = !0x0004;
    let enable: i32 = 1;
    let ret = unsafe {
        winsock_setsockopt(
            socket.as_raw_socket() as usize,
            SOL_SOCKET,
            SO_EXCLUSIVEADDRUSE,
            (&enable as *const i32).cast(),
            std::mem::size_of::<i32>() as i32,
        )
    };
    if ret == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(windows)]
#[link(name = "ws2_32")]
extern "system" {
    #[link_name = "setsockopt"]
    fn winsock_setsockopt(
        s: usize,
        level: i32,
        optname: i32,
        optval: *const core::ffi::c_char,
        optlen: i32,
    ) -> i32;
}

fn take_unbind_tasks(
    registry: &mut GatewayRegistry,
    cited_port: u16,
    stopped_profile: &str,
) -> Vec<ActiveSocketTask> {
    let ports: Vec<u16> = if registry.runtimes.is_empty() {
        registry.sockets.keys().copied().collect()
    } else if registry.remaining_citers(cited_port, Some(stopped_profile)) == 0 {
        vec![cited_port]
    } else {
        Vec::new()
    };
    take_socket_tasks(registry, &ports)
}

fn take_socket_tasks(registry: &mut GatewayRegistry, ports: &[u16]) -> Vec<ActiveSocketTask> {
    let mut tasks = Vec::new();
    for port in ports {
        if let Some(mut socket) = registry.sockets.remove(port) {
            socket.accept_shutdown.cancel();
            if let Some(task) = socket.task.take() {
                tasks.push(ActiveSocketTask { port: *port, task });
            }
        }
    }
    if registry
        .primary_port
        .is_some_and(|port| !registry.sockets.contains_key(&port))
    {
        registry.primary_port = registry.sockets.keys().copied().next();
    }
    tasks
}

async fn run_shutdown(gateway: Gateway, completion: Arc<CleanupCompletion>) {
    let (sockets, stopping, state_failed) = match gateway.lock() {
        Ok(mut registry) => {
            let mut stopping = Vec::new();
            for runtime in registry.runtimes.values_mut() {
                runtime.lifecycle = BridgeRuntimeState::Stopping;
                runtime.state.stopping.store(true, Ordering::SeqCst);
                runtime.state.record_upstream(BridgeUpstreamStatus::Stopped);
                if let Some(stop_completion) = &runtime.stop_completion {
                    stopping.push(stop_completion.clone());
                }
            }
            let ports: Vec<u16> = registry.sockets.keys().copied().collect();
            let sockets = take_socket_tasks(&mut registry, &ports);
            (sockets, stopping, false)
        }
        Err(_) => (Vec::new(), Vec::new(), true),
    };
    if state_failed {
        completion.finish(true);
        return;
    }

    let socket_failed = drain_socket_tasks(sockets).await;
    let mut stopping_failed = false;
    for stop_completion in &stopping {
        stopping_failed |= stop_completion.wait().await.is_err();
    }
    let failed = socket_failed
        || stopping_failed
        || gateway
            .lock()
            .map(|mut registry| {
                registry.runtimes.clear();
                registry.sockets.clear();
                registry.primary_port = None;
                false
            })
            .unwrap_or(true);
    completion.finish(failed);
}

async fn drain_edge(state: &EdgeState) -> bool {
    let deadline = Instant::now() + DRAIN_TIMEOUT;
    while state.admission.available_permits() < MAX_IN_FLIGHT_REQUESTS_PER_PROFILE
        && Instant::now() < deadline
    {
        tokio::time::sleep(TASK_POLL_INTERVAL).await;
    }
    if state.admission.available_permits() < MAX_IN_FLIGHT_REQUESTS_PER_PROFILE {
        state.force_shutdown.cancel();
        let force_deadline = Instant::now() + FORCE_CANCEL_GRACE;
        while state.admission.available_permits() < MAX_IN_FLIGHT_REQUESTS_PER_PROFILE
            && Instant::now() < force_deadline
        {
            tokio::time::sleep(TASK_POLL_INTERVAL).await;
        }
    }
    state.admission.available_permits() == MAX_IN_FLIGHT_REQUESTS_PER_PROFILE
}

async fn drain_socket_tasks(mut tasks: Vec<ActiveSocketTask>) -> bool {
    let deadline = Instant::now() + DRAIN_TIMEOUT;
    while tasks.iter().any(|task| !task.task.is_finished()) && Instant::now() < deadline {
        tokio::time::sleep(TASK_POLL_INTERVAL).await;
    }
    let mut forced = false;
    for task in &mut tasks {
        if !task.task.is_finished() {
            task.task.abort();
            forced = true;
        }
    }
    let mut failed = forced;
    for task in tasks {
        let result = task.task.await;
        log_socket_result(task.port, forced, &result);
        failed |= result.is_err() || matches!(result, Ok(Err(())));
    }
    failed
}

fn log_socket_result(
    port: u16,
    forced: bool,
    result: &Result<Result<(), ()>, tokio::task::JoinError>,
) {
    if forced {
        tracing::warn!(
            target: "core.adapter",
            port,
            op = "stop",
            code = "forced_shutdown",
            "bridge listener force-stopped after drain timeout"
        );
    } else if result.is_err() || matches!(result, Ok(Err(()))) {
        tracing::warn!(
            target: "core.adapter",
            port,
            op = "stop",
            code = "listener_error",
            "bridge listener stopped with an internal error"
        );
    } else {
        tracing::info!(
            target: "core.adapter",
            port,
            op = "stop",
            "bridge listener stopped"
        );
    }
}

fn runtime_matches_pool(runtime: &super::gateway::EdgeRuntime, pool_id: &str) -> bool {
    if runtime.spec.profile_id == pool_id {
        return true;
    }
    runtime
        .spec
        .route_index
        .as_ref()
        .is_some_and(|index| index.route_id == pool_id)
        || runtime
            .state
            .route_index
            .as_ref()
            .is_some_and(|index| index.route_id == pool_id)
}

fn snapshot_spec_from_runtime(runtime: &super::gateway::EdgeRuntime) -> BridgeStartSpec {
    let mut spec = runtime.spec.clone();
    spec.port = runtime.cited_port;
    spec.schedule_policy = runtime.state.account_picker.schedule_policy();
    spec.multi_account = runtime.state.account_picker.multi_account();
    spec.members = runtime
        .state
        .account_picker
        .members()
        .iter()
        .map(|member| BridgeMemberSpec {
            ticket_id: member.ticket_id.clone(),
            source_kind: member.source_kind.clone(),
            source_id: member.source_id.clone(),
            label: member.label.clone(),
            auth: member.auth.clone(),
            reload: member.reload.clone(),
            health: member.health(),
            priority: member.priority,
            position: member.position,
            quota_remaining_pct: member.quota_remaining_pct(),
            quota_reset_at: member.quota_reset_at(),
            quota_fresh_until: member.quota_fresh_until(),
            quota_credit: member.quota_credit(),
            kiro_http: member.kiro_http.clone(),
        })
        .collect();
    if let Some(lead) = spec.members.first() {
        spec.upstream.auth = lead.auth.clone();
    }
    spec
}

fn snapshot_entry_from_runtime(runtime: &super::gateway::EdgeRuntime) -> GatewaySnapshotEntry {
    GatewaySnapshotEntry {
        spec: snapshot_spec_from_runtime(runtime),
        observed_upstream: runtime.state.observed_upstream(),
    }
}

fn reconcile_snapshot_entry(
    contents: &Arc<Mutex<GatewaySnapshotContents>>,
    runtime: &super::gateway::EdgeRuntime,
) {
    let Ok(mut contents) = contents.lock() else {
        return;
    };
    let updated = snapshot_entry_from_runtime(runtime);
    if let Some(entry) = contents
        .entries
        .iter_mut()
        .find(|entry| entry.spec.profile_id == updated.spec.profile_id)
    {
        *entry = updated;
    }
}

fn validate_start_spec(spec: &BridgeStartSpec) -> Result<Url, BridgeHostError> {
    if spec.profile_id.trim().is_empty() {
        return Err(BridgeHostError::EmptyProfileId);
    }
    if spec.local_token.trim().is_empty() {
        return Err(BridgeHostError::EmptyLocalToken);
    }
    if spec.upstream.base_url.trim().is_empty() {
        return Err(BridgeHostError::EmptyUpstreamUrl);
    }
    if spec.upstream.auth.token().trim().is_empty() {
        return Err(BridgeHostError::EmptyUpstreamToken);
    }
    // Empty gateways use this loopback value only as a dormant placeholder. A spec that already
    // identifies a real source must never start with it, otherwise requests are forwarded to port
    // 80 on this machine and surface later as a misleading upstream 404/502.
    if spec.upstream.source_id.is_some()
        && spec.upstream.auth.token() == "pending"
        && spec.upstream.base_url.trim_end_matches('/') == "http://127.0.0.1"
    {
        return Err(BridgeHostError::InvalidUpstreamUrl);
    }
    crate::utils::loopback::validate_upstream_base_url(&spec.upstream.base_url)
        .map_err(|()| BridgeHostError::InvalidUpstreamUrl)
}

fn same_spec(left: &BridgeStartSpec, right: &BridgeStartSpec) -> bool {
    left.profile_id == right.profile_id
        && left.local_token == right.local_token
        && left.upstream.base_url == right.upstream.base_url
        && left.upstream.model == right.upstream.model
        && left.upstream.source_id == right.upstream.source_id
        && left.upstream.auth.token() == right.upstream.auth.token()
        && left.upstream.protocol == right.upstream.protocol
        && left.upstream.local_surface == right.upstream.local_surface
        && left.listed_models == right.listed_models
        && left.downstream_responses_profile == right.downstream_responses_profile
        && left.multi_account == right.multi_account
        && member_fingerprint(left) == member_fingerprint(right)
        && left.route_index == right.route_index
        && left.schedule_policy == right.schedule_policy
}

fn member_fingerprint(
    spec: &BridgeStartSpec,
) -> Vec<(String, String, String, Option<u64>, Option<u64>)> {
    spec.members
        .iter()
        .map(|member| {
            (
                member.ticket_id.clone(),
                member.source_id.clone(),
                member.auth.token(),
                // Quota-relevant fields so a refreshed snapshot forces rebuild
                // instead of reusing a listener that baked a stale percentage.
                member
                    .quota_remaining_pct
                    .filter(|value| value.is_finite())
                    .map(|value| value.to_bits()),
                member.quota_fresh_until.and_then(|until| {
                    until
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|duration| duration.as_secs())
                }),
            )
        })
        .collect()
}
