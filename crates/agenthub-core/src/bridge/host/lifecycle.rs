use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use axum::Router;
use reqwest::Url;
use tokio::net::TcpListener;
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::bridge::route_index::EffectiveRouteIndex;
use crate::bridge::runtime::{
    BridgeRuntimeState, BridgeRuntimeStatus, BridgeStartSpec, BridgeUpstreamStatus,
};

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
        }
    }
}

struct ActiveSocketTask {
    port: u16,
    task: JoinHandle<Result<(), ()>>,
}

impl BridgeRuntimeHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Installs the durable gateway usage spool directory. Must be called
    /// before edges start; later calls are ignored. Unset keeps capture a
    /// no-op, so CLI runs and tests never write spool files.
    pub fn set_usage_spool_dir(&self, dir: std::path::PathBuf) {
        self.gateway
            .usage_spool
            .set(std::sync::Arc::new(crate::bridge::usage_capture::UsageSpool::new(dir)));
    }

    /// Enable durable route-trace ring persistence (Activity monitor history).
    /// Loads last N traces from `path` when present; later calls are ignored.
    /// Unset keeps an in-memory ring only (CLI / unit tests).
    pub fn set_route_trace_persist_path(&self, path: std::path::PathBuf) {
        self.gateway.route_traces.enable_persist(path);
    }

    /// Starts an edge and ensures a loopback socket. Repeating an exact live start is
    /// idempotent; attempting to start while a matching profile drains fails rather than
    /// racing a second edge.
    pub async fn start(
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
    pub fn recent_unauthenticated_route_traces(&self) -> Vec<super::route_trace::RouteRequestTrace> {
        self.gateway.route_traces.recent_unauthenticated()
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

    /// Records the last observed health or request outcome. Status/health reads
    /// later report this stored value and must not issue a new upstream probe.
    pub fn record_upstream_outcome(
        &self,
        profile_id: &str,
        status: BridgeUpstreamStatus,
    ) -> Result<Option<BridgeRuntimeStatus>, BridgeHostError> {
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
        let gate = self.profile_gate(profile_id)?;
        let profile_operation = gate.lock_owned().await;
        let (edge, cited_port, stopped, completion) = {
            let mut registry = self.gateway.lock()?;
            let sockets_live = registry.sockets_live();
            let runtime = registry
                .runtimes
                .get_mut(profile_id)
                .ok_or(BridgeHostError::NotRunning)?;
            if runtime.lifecycle == BridgeRuntimeState::Stopping {
                return Err(BridgeHostError::Stopping);
            }
            if !sockets_live {
                registry.runtimes.remove(profile_id);
                return Err(BridgeHostError::NotRunning);
            }
            runtime.lifecycle = BridgeRuntimeState::Stopping;
            runtime.state.stopping.store(true, Ordering::SeqCst);
            runtime.state.record_upstream(BridgeUpstreamStatus::Stopped);
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
        let profile_id = profile_id.to_owned();
        let cleanup_completion = completion.clone();
        tokio::spawn(async move {
            let _profile_operation = profile_operation;
            drain_edge(&edge).await;
            let sockets = {
                let mut registry = match gateway.lock() {
                    Ok(registry) => registry,
                    Err(_) => {
                        cleanup_completion.finish(true);
                        return;
                    }
                };
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
    // SO_REUSEADDR shortens the TIME_WAIT gap after tauri/dev hot-reload or
    // graceful stop so the same loopback port can rebind without connection refused.
    let requested = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let socket = tokio::net::TcpSocket::new_v4()?;
    socket.set_reuseaddr(true)?;
    socket.bind(requested)?;
    Ok(socket.listen(1024)?)
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

async fn drain_edge(state: &EdgeState) {
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
    let mut upstream =
        Url::parse(&spec.upstream.base_url).map_err(|_| BridgeHostError::InvalidUpstreamUrl)?;
    if upstream.host_str().is_none()
        || !upstream.username().is_empty()
        || upstream.password().is_some()
        || upstream.fragment().is_some()
    {
        return Err(BridgeHostError::InvalidUpstreamUrl);
    }
    match upstream.scheme() {
        "https" => {}
        "http" if crate::utils::loopback::is_loopback_host(upstream.host_str()) => {}
        _ => return Err(BridgeHostError::InvalidUpstreamUrl),
    }
    // `Url::join` treats a path without a trailing slash as a file. Normalize it so a configured
    // provider base path such as `/coding/v1` is retained when appending `chat/completions`.
    if !upstream.path().ends_with('/') {
        let path = format!("{}/", upstream.path());
        upstream.set_path(&path);
    }
    Ok(upstream)
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

fn member_fingerprint(spec: &BridgeStartSpec) -> Vec<(String, String, String)> {
    spec.members
        .iter()
        .map(|member| {
            (
                member.ticket_id.clone(),
                member.source_id.clone(),
                member.auth.token(),
            )
        })
        .collect()
}
