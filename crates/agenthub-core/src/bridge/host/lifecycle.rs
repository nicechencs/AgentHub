use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use reqwest::{Client, Url};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::{watch, Mutex as AsyncMutex, Semaphore};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::bridge::runtime::{
    BridgeRuntimeState, BridgeRuntimeStatus, BridgeStartSpec, BridgeUpstreamStatus,
};

use super::http::{router, ListenerState};
use super::{
    DRAIN_TIMEOUT, FORCE_CANCEL_GRACE, MAX_IN_FLIGHT_REQUESTS_PER_PROFILE, TASK_POLL_INTERVAL,
    UPSTREAM_CONNECT_TIMEOUT,
};

#[derive(Debug, Error)]
pub enum BridgeHostError {
    #[error("bridge profile id must not be empty")]
    EmptyProfileId,
    #[error("bridge local bearer token must not be empty")]
    EmptyLocalToken,
    #[error("bridge upstream base URL must not be empty")]
    EmptyUpstreamUrl,
    #[error("bridge upstream base URL is invalid or not permitted")]
    InvalidUpstreamUrl,
    #[error("bridge upstream bearer token must not be empty")]
    EmptyUpstreamToken,
    #[error("bridge host is shutting down and cannot start listeners")]
    HostClosing,
    #[error("bridge instance already runs with a different configuration")]
    ConflictingStart,
    #[error("bridge instance is stopping")]
    Stopping,
    #[error("bridge instance is not running")]
    NotRunning,
    #[error("bridge host state is unavailable")]
    StatePoisoned,
    #[error("failed to bind loopback bridge listener: {0}")]
    Bind(#[from] std::io::Error),
}

/// Owns loopback bridge listeners. A host belongs to one desktop-process lifetime: once
/// [`Self::shutdown`] begins, its closing latch rejects all further starts.
#[derive(Clone)]
pub struct BridgeRuntimeHost {
    instances: Arc<Mutex<HashMap<String, RuntimeInstance>>>,
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
        Self {
            instances: Arc::new(Mutex::new(HashMap::new())),
            closing: Arc::new(AtomicBool::new(false)),
            profile_gates: Arc::new(Mutex::new(HashMap::new())),
            registration: Arc::new(AsyncMutex::new(())),
            shutdown: Arc::new(AsyncMutex::new(None)),
        }
    }
}

struct RuntimeInstance {
    spec: BridgeStartSpec,
    port: u16,
    started_at: SystemTime,
    lifecycle: BridgeRuntimeState,
    upstream_status: Arc<Mutex<BridgeUpstreamStatus>>,
    accept_shutdown: CancellationToken,
    force_shutdown: CancellationToken,
    task: Option<JoinHandle<Result<(), ()>>>,
    stop_completion: Option<Arc<CleanupCompletion>>,
}

struct ActiveTask {
    profile_id: String,
    force_shutdown: CancellationToken,
    task: JoinHandle<Result<(), ()>>,
}

/// A tiny cancellation-safe completion primitive. The cleanup task, rather than an RPC caller,
/// owns listener JoinHandles; callers can therefore be dropped without abandoning a stop.
pub struct CleanupCompletion {
    /// `watch` retains the terminal result. A waiter that subscribes after `finish`, or while
    /// `finish` races registration, observes the latest value instead of depending on an edge-
    /// triggered notification that can be lost.
    result: watch::Sender<Option<bool>>,
}

impl CleanupCompletion {
    pub fn new() -> Self {
        let (result, _receiver) = watch::channel(None);
        Self { result }
    }

    pub fn finish(&self, failed: bool) {
        self.result.send_replace(Some(failed));
    }

    pub async fn wait(&self) -> Result<(), BridgeHostError> {
        let mut result = self.result.subscribe();
        loop {
            if let Some(failed) = *result.borrow_and_update() {
                return if failed {
                    Err(BridgeHostError::StatePoisoned)
                } else {
                    Ok(())
                };
            }
            result
                .changed()
                .await
                .map_err(|_| BridgeHostError::StatePoisoned)?;
        }
    }
}

impl BridgeRuntimeHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a loopback listener. Repeating an exact live start is idempotent; attempting to
    /// start while a matching profile drains fails rather than racing a second listener.
    pub async fn start(
        &self,
        spec: BridgeStartSpec,
    ) -> Result<BridgeRuntimeStatus, BridgeHostError> {
        let upstream_url = validate_start_spec(&spec)?;
        if self.closing.load(Ordering::SeqCst) {
            return Err(BridgeHostError::HostClosing);
        }

        // Fast rejection means a start issued while `stop` is draining does not queue and restart
        // the same profile behind it.
        {
            let instances = self
                .instances
                .lock()
                .map_err(|_| BridgeHostError::StatePoisoned)?;
            if let Some(existing) = instances.get(&spec.profile_id) {
                match existing.lifecycle {
                    BridgeRuntimeState::Stopping => return Err(BridgeHostError::Stopping),
                    BridgeRuntimeState::Running | BridgeRuntimeState::Starting
                        if !existing.task_finished() =>
                    {
                        if same_spec(&existing.spec, &spec) {
                            return Ok(existing.status());
                        }
                        return Err(BridgeHostError::ConflictingStart);
                    }
                    _ => {}
                }
            }
        }

        let gate = self.profile_gate(&spec.profile_id)?;
        let _profile_operation = gate.lock_owned().await;
        // Shutdown and start registration are mutually exclusive only for the short interval in
        // which a listener is created and entered in the registry. Draining remains per-profile.
        let _registration = self.registration.lock().await;
        if self.closing.load(Ordering::SeqCst) {
            return Err(BridgeHostError::HostClosing);
        }
        let mut instances = self
            .instances
            .lock()
            .map_err(|_| BridgeHostError::StatePoisoned)?;
        if let Some(existing) = instances.get(&spec.profile_id) {
            match existing.lifecycle {
                BridgeRuntimeState::Stopping => return Err(BridgeHostError::Stopping),
                BridgeRuntimeState::Running | BridgeRuntimeState::Starting
                    if !existing.task_finished() =>
                {
                    if same_spec(&existing.spec, &spec) {
                        return Ok(existing.status());
                    }
                    return Err(BridgeHostError::ConflictingStart);
                }
                // A listener task cannot leave a valid, reusable running state after it exits.
                // Dropping a completed JoinHandle reaps it; the socket has already been released.
                _ => {
                    instances.remove(&spec.profile_id);
                }
            }
        }

        let requested = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), spec.port);
        let socket = std::net::TcpListener::bind(requested)?;
        socket.set_nonblocking(true)?;
        let listener = TcpListener::from_std(socket)?;
        let port = listener.local_addr()?.port();
        let accept_shutdown = CancellationToken::new();
        let force_shutdown = CancellationToken::new();
        let observed_upstream = Arc::new(Mutex::new(BridgeUpstreamStatus::Unknown));
        let state = ListenerState {
            profile_id: Arc::from(spec.profile_id.clone()),
            local_token: Arc::from(spec.local_token.clone()),
            upstream: spec.upstream.clone(),
            upstream_url,
            client: Client::builder()
                // Streaming requests deliberately have no reqwest-wide total timeout: a healthy
                // long-running SSE response is bounded by per-chunk idle time instead.
                .connect_timeout(UPSTREAM_CONNECT_TIMEOUT)
                .build()
                .expect("reqwest client builder uses static valid settings"),
            force_shutdown: force_shutdown.clone(),
            admission: Arc::new(Semaphore::new(MAX_IN_FLIGHT_REQUESTS_PER_PROFILE)),
            observed_upstream: Arc::clone(&observed_upstream),
        };
        let app = router(state);
        let task_shutdown = accept_shutdown.clone();
        let profile_id = spec.profile_id.clone();
        let task = tokio::spawn(async move {
            tracing::info!(
                target: "core.adapter",
                profile_id = %profile_id,
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
                        profile_id = %profile_id,
                        op = "serve",
                        code = "listener_error",
                        "bridge listener stopped unexpectedly"
                    );
                    Err(())
                }
            }
        });
        let instance = RuntimeInstance {
            spec,
            port,
            started_at: SystemTime::now(),
            lifecycle: BridgeRuntimeState::Running,
            upstream_status: observed_upstream,
            accept_shutdown,
            force_shutdown,
            task: Some(task),
            stop_completion: None,
        };
        let status = instance.status();
        instances.insert(status.profile_id.clone(), instance);
        Ok(status)
    }

    pub fn status(&self, profile_id: &str) -> Result<Option<BridgeRuntimeStatus>, BridgeHostError> {
        let instances = self
            .instances
            .lock()
            .map_err(|_| BridgeHostError::StatePoisoned)?;
        Ok(instances.get(profile_id).map(RuntimeInstance::status))
    }

    pub fn statuses(&self) -> Result<Vec<BridgeRuntimeStatus>, BridgeHostError> {
        let instances = self
            .instances
            .lock()
            .map_err(|_| BridgeHostError::StatePoisoned)?;
        Ok(instances.values().map(RuntimeInstance::status).collect())
    }

    /// Records the last observed health or request outcome. Status/health reads
    /// later report this stored value and must not issue a new upstream probe.
    pub fn record_upstream_outcome(
        &self,
        profile_id: &str,
        status: BridgeUpstreamStatus,
    ) -> Result<Option<BridgeRuntimeStatus>, BridgeHostError> {
        let instances = self
            .instances
            .lock()
            .map_err(|_| BridgeHostError::StatePoisoned)?;
        let Some(instance) = instances.get(profile_id) else {
            return Ok(None);
        };
        instance.record_upstream(status);
        Ok(Some(instance.status()))
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
        let (task, stopped, completion) = {
            let mut instances = self
                .instances
                .lock()
                .map_err(|_| BridgeHostError::StatePoisoned)?;
            let instance = instances
                .get_mut(profile_id)
                .ok_or(BridgeHostError::NotRunning)?;
            if instance.lifecycle == BridgeRuntimeState::Stopping {
                return Err(BridgeHostError::Stopping);
            }
            if instance.task_finished() {
                instances.remove(profile_id);
                return Err(BridgeHostError::NotRunning);
            }
            instance.lifecycle = BridgeRuntimeState::Stopping;
            instance.record_upstream(BridgeUpstreamStatus::Stopped);
            instance.accept_shutdown.cancel();
            let stopped = instance.stopped_status();
            let completion = Arc::new(CleanupCompletion::new());
            instance.stop_completion = Some(completion.clone());
            (
                ActiveTask {
                    profile_id: profile_id.to_owned(),
                    force_shutdown: instance.force_shutdown.clone(),
                    task: instance.task.take().ok_or(BridgeHostError::NotRunning)?,
                },
                stopped,
                completion,
            )
        };

        // Keep both the listener JoinHandle and its profile gate in this detached task. This makes
        // stop cancellation-safe and prevents a queued start from racing the final removal.
        let instances = Arc::clone(&self.instances);
        let profile_id = profile_id.to_owned();
        let cleanup_completion = completion.clone();
        tokio::spawn(async move {
            let _profile_operation = profile_operation;
            let mut task = task;
            let forced = drain_task(&mut task).await;
            let result = task.task.await;
            let failed = instances
                .lock()
                .map(|mut instances| instances.remove(&profile_id).is_none())
                .unwrap_or(true);
            log_task_result(&task.profile_id, "stop", forced, result);
            cleanup_completion.finish(failed);
        });

        completion.wait().await?;
        Ok(stopped)
    }

    /// Stops every listener. All accepts are closed before any listener is awaited; after a short
    /// drain deadline, in-flight handlers receive cancellation and remaining listener tasks are
    /// aborted while their JoinHandles are still owned by this method.
    pub async fn shutdown(&self) -> Result<(), BridgeHostError> {
        let (completion, starts_cleanup) = {
            let mut shutdown = self.shutdown.lock().await;
            if let Some(completion) = shutdown.as_ref() {
                (completion.clone(), false)
            } else {
                // A start that already has its profile gate cannot cross this registration gate;
                // all starts after the latch is set recheck and reject before listener creation.
                let _registration = self.registration.lock().await;
                self.closing.store(true, Ordering::SeqCst);
                let completion = Arc::new(CleanupCompletion::new());
                *shutdown = Some(completion.clone());
                (completion, true)
            }
        };
        if starts_cleanup {
            tokio::spawn(run_shutdown(
                Arc::clone(&self.instances),
                completion.clone(),
            ));
        }
        completion.wait().await
    }

    /// Waits for in-flight graceful shutdown tasks. `shutdown` first requests listener draining.
    pub async fn drain(&self) -> Result<(), BridgeHostError> {
        self.shutdown().await
    }
}

async fn run_shutdown(
    instances: Arc<Mutex<HashMap<String, RuntimeInstance>>>,
    completion: Arc<CleanupCompletion>,
) {
    // The detached owner takes all still-live JoinHandles so neither an aborted caller nor a
    // concurrent shutdown can report success while listener cleanup is unfinished.
    let (mut tasks, stopping, state_failed) = match instances.lock() {
        Ok(mut instances) => {
            let mut tasks = Vec::with_capacity(instances.len());
            let mut stopping = Vec::new();
            for (profile_id, instance) in instances.iter_mut() {
                instance.lifecycle = BridgeRuntimeState::Stopping;
                instance.record_upstream(BridgeUpstreamStatus::Stopped);
                instance.accept_shutdown.cancel();
                if let Some(task) = instance.task.take() {
                    tasks.push(ActiveTask {
                        profile_id: profile_id.clone(),
                        force_shutdown: instance.force_shutdown.clone(),
                        task,
                    });
                } else if let Some(stop_completion) = &instance.stop_completion {
                    stopping.push(stop_completion.clone());
                }
            }
            (tasks, stopping, false)
        }
        Err(_) => (Vec::new(), Vec::new(), true),
    };
    if state_failed {
        completion.finish(true);
        return;
    }

    let forced = drain_all_tasks(&mut tasks).await;
    for (task, did_force) in tasks.into_iter().zip(forced) {
        let result = task.task.await;
        log_task_result(&task.profile_id, "shutdown", did_force, result);
    }

    // Stops already in progress own their JoinHandles, so join their detached cleanup before
    // clearing the registry rather than reporting shutdown success prematurely.
    let mut stopping_failed = false;
    for stop_completion in &stopping {
        stopping_failed |= stop_completion.wait().await.is_err();
    }
    let failed = stopping_failed
        || instances
            .lock()
            .map(|mut instances| {
                instances.clear();
                false
            })
            .unwrap_or(true);
    completion.finish(failed);
}

impl RuntimeInstance {
    fn task_finished(&self) -> bool {
        self.task.as_ref().is_none_or(JoinHandle::is_finished)
    }

    fn record_upstream(&self, status: BridgeUpstreamStatus) {
        if let Ok(mut observed) = self.upstream_status.lock() {
            *observed = status;
        }
    }

    fn observed_upstream(&self) -> BridgeUpstreamStatus {
        self.upstream_status
            .lock()
            .map(|status| *status)
            .unwrap_or(BridgeUpstreamStatus::Unavailable)
    }

    fn public_upstream_status(&self, state: BridgeRuntimeState) -> BridgeUpstreamStatus {
        match state {
            BridgeRuntimeState::Stopped | BridgeRuntimeState::Stopping => {
                BridgeUpstreamStatus::Stopped
            }
            BridgeRuntimeState::Error => BridgeUpstreamStatus::Unavailable,
            _ => self.observed_upstream(),
        }
    }

    fn status(&self) -> BridgeRuntimeStatus {
        let state = match self.lifecycle {
            BridgeRuntimeState::Stopping => BridgeRuntimeState::Stopping,
            BridgeRuntimeState::Running | BridgeRuntimeState::Starting if self.task_finished() => {
                BridgeRuntimeState::Error
            }
            state => state,
        };
        BridgeRuntimeStatus {
            profile_id: self.spec.profile_id.clone(),
            port: self.port,
            running: matches!(
                state,
                BridgeRuntimeState::Running | BridgeRuntimeState::Degraded
            ),
            started_at: self.started_at,
            source_connection_id: self.spec.upstream.source_connection_id.clone(),
            state,
            upstream_status: self.public_upstream_status(state),
        }
    }

    fn stopped_status(&self) -> BridgeRuntimeStatus {
        BridgeRuntimeStatus {
            profile_id: self.spec.profile_id.clone(),
            port: self.port,
            running: false,
            started_at: self.started_at,
            source_connection_id: self.spec.upstream.source_connection_id.clone(),
            state: BridgeRuntimeState::Stopped,
            upstream_status: BridgeUpstreamStatus::Stopped,
        }
    }
}

async fn drain_task(task: &mut ActiveTask) -> bool {
    let deadline = Instant::now() + DRAIN_TIMEOUT;
    while !task.task.is_finished() && Instant::now() < deadline {
        tokio::time::sleep(TASK_POLL_INTERVAL).await;
    }
    if task.task.is_finished() {
        return false;
    }
    task.force_shutdown.cancel();
    let force_deadline = Instant::now() + FORCE_CANCEL_GRACE;
    while !task.task.is_finished() && Instant::now() < force_deadline {
        tokio::time::sleep(TASK_POLL_INTERVAL).await;
    }
    if !task.task.is_finished() {
        task.task.abort();
    }
    true
}

async fn drain_all_tasks(tasks: &mut [ActiveTask]) -> Vec<bool> {
    let deadline = Instant::now() + DRAIN_TIMEOUT;
    while tasks.iter().any(|task| !task.task.is_finished()) && Instant::now() < deadline {
        tokio::time::sleep(TASK_POLL_INTERVAL).await;
    }
    let must_force = tasks.iter().any(|task| !task.task.is_finished());
    if !must_force {
        return vec![false; tasks.len()];
    }
    for task in &*tasks {
        if !task.task.is_finished() {
            task.force_shutdown.cancel();
        }
    }
    let force_deadline = Instant::now() + FORCE_CANCEL_GRACE;
    while tasks.iter().any(|task| !task.task.is_finished()) && Instant::now() < force_deadline {
        tokio::time::sleep(TASK_POLL_INTERVAL).await;
    }
    tasks
        .iter_mut()
        .map(|task| {
            let forced = !task.task.is_finished();
            if forced {
                task.task.abort();
            }
            forced
        })
        .collect()
}

fn log_task_result(
    profile_id: &str,
    operation: &str,
    forced: bool,
    result: Result<Result<(), ()>, tokio::task::JoinError>,
) {
    if forced {
        tracing::warn!(
            target: "core.adapter",
            profile_id,
            op = operation,
            code = "forced_shutdown",
            "bridge listener force-stopped after drain timeout"
        );
    } else if result.is_err() || matches!(result, Ok(Err(()))) {
        tracing::warn!(
            target: "core.adapter",
            profile_id,
            op = operation,
            code = "listener_error",
            "bridge listener stopped with an internal error"
        );
    } else {
        tracing::info!(
            target: "core.adapter",
            profile_id,
            op = operation,
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
        && left.port == right.port
        && left.local_token == right.local_token
        && left.upstream.base_url == right.upstream.base_url
        && left.upstream.model == right.upstream.model
        && left.upstream.source_connection_id == right.upstream.source_connection_id
        && left.upstream.auth.token() == right.upstream.auth.token()
        && left.upstream.protocol == right.upstream.protocol
}
