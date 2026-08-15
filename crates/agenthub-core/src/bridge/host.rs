use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use async_stream::stream;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use reqwest::{Client, Url};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::{watch, Mutex as AsyncMutex, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::bridge::protocol::anthropic_messages::{
    anthropic_message_to_ir, encode_anthropic_message, encode_anthropic_sse,
    parse_messages_request, to_anthropic_messages_request, AnthropicStreamToIr,
};
use crate::bridge::protocol::responses::{
    encode_responses_from_ir, parse_responses_request, responses_output_to_ir,
    to_kimi_chat_request, to_responses_request, IrToResponsesSse, ResponsesStreamToIr,
};
use crate::bridge::runtime::{
    BridgeRuntimeState, BridgeRuntimeStatus, BridgeStartSpec, BridgeUpstreamProtocol,
    BridgeUpstreamStatus,
};
use crate::bridge::types::{BridgeEvent, IrEvent, ProtocolError};

const ANTHROPIC_API_VERSION: &str = "2023-06-01";

const BODY_LIMIT_BYTES: usize = 1_048_576;
/// Streamed Completions/Responses traffic can exceed the request-body safety
/// ceiling; keep a hard cap while allowing realistic agent sessions.
const STREAM_LIMIT_BYTES: usize = 32 * 1_048_576;
const MAX_IN_FLIGHT_REQUESTS_PER_PROFILE: usize = 4;
const UPSTREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const UPSTREAM_RESPONSE_HEADER_TIMEOUT: Duration = Duration::from_secs(30);
const UPSTREAM_NON_STREAM_TIMEOUT: Duration = Duration::from_secs(120);
const UPSTREAM_BODY_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(not(test))]
const UPSTREAM_STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
// Keep the production policy above while making the idle-path regression test practical.
#[cfg(test)]
const UPSTREAM_STREAM_IDLE_TIMEOUT: Duration = Duration::from_millis(100);
const REQUEST_BODY_TIMEOUT: Duration = Duration::from_secs(30);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
const FORCE_CANCEL_GRACE: Duration = Duration::from_millis(200);
const TASK_POLL_INTERVAL: Duration = Duration::from_millis(10);

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

#[derive(Clone)]
struct AppState {
    profile_id: Arc<str>,
    local_token: Arc<str>,
    upstream: crate::bridge::runtime::BridgeUpstreamConfig,
    upstream_url: Url,
    client: Client,
    force_shutdown: CancellationToken,
    admission: Arc<Semaphore>,
    observed_upstream: Arc<Mutex<BridgeUpstreamStatus>>,
}

/// A tiny cancellation-safe completion primitive. The cleanup task, rather than an RPC caller,
/// owns listener JoinHandles; callers can therefore be dropped without abandoning a stop.
pub(super) struct CleanupCompletion {
    /// `watch` retains the terminal result. A waiter that subscribes after `finish`, or while
    /// `finish` races registration, observes the latest value instead of depending on an edge-
    /// triggered notification that can be lost.
    result: watch::Sender<Option<bool>>,
}

impl CleanupCompletion {
    pub(super) fn new() -> Self {
        let (result, _receiver) = watch::channel(None);
        Self { result }
    }

    pub(super) fn finish(&self, failed: bool) {
        self.result.send_replace(Some(failed));
    }

    pub(super) async fn wait(&self) -> Result<(), BridgeHostError> {
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
        let state = AppState {
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
        "http" if is_loopback_host(upstream.host_str()) => {}
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

fn is_loopback_host(host: Option<&str>) -> bool {
    matches!(host, Some("localhost"))
        || host
            .and_then(|host| host.parse::<IpAddr>().ok())
            .is_some_and(|address| address.is_loopback())
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

fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/responses", post(responses))
        .route("/v1/messages", post(messages))
        .layer(axum::extract::DefaultBodyLimit::max(BODY_LIMIT_BYTES))
        .with_state(state)
}

impl AppState {
    fn observed_upstream(&self) -> BridgeUpstreamStatus {
        self.observed_upstream
            .lock()
            .map(|status| *status)
            .unwrap_or(BridgeUpstreamStatus::Unavailable)
    }

    fn record_upstream(&self, status: BridgeUpstreamStatus) {
        if let Ok(mut observed) = self.observed_upstream.lock() {
            *observed = status;
        }
    }

    fn record_upstream_success(&self) {
        self.record_upstream(BridgeUpstreamStatus::Connected);
    }

    fn record_upstream_failure(&self) {
        self.record_upstream(BridgeUpstreamStatus::Degraded);
    }
}

async fn health(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !has_valid_local_auth(&headers, &state.local_token) {
        tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, op = "health", code = "unauthorized", status = 401_u16, "bridge health request rejected");
        return error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "Invalid local bearer token.",
            None,
        );
    }
    // Local health is a listener liveness check. It reports the last stored
    // upstream outcome and never issues a new billable provider probe.
    let upstream_status = state.observed_upstream();
    tracing::debug!(target: "core.adapter", profile_id = %state.profile_id, op = "health", upstream_status = upstream_status.as_str(), "bridge health check");
    Json(json!({
        "ok": true,
        "service": "agenthub-bridge",
        "listener_status": "running",
        "upstream_status": upstream_status.as_str()
    }))
    .into_response()
}

async fn responses(State(state): State<AppState>, request: Request) -> Response {
    if state.upstream.protocol == BridgeUpstreamProtocol::CodexResponsesOauth {
        return StatusCode::NOT_FOUND.into_response();
    }
    handle_responses(state, request).await
}

async fn messages(State(state): State<AppState>, request: Request) -> Response {
    if state.upstream.protocol != BridgeUpstreamProtocol::CodexResponsesOauth {
        return StatusCode::NOT_FOUND.into_response();
    }
    handle_messages(state, request).await
}

async fn handle_responses(state: AppState, request: Request) -> Response {
    let request_id = uuid::Uuid::new_v4().to_string();
    let started = Instant::now();
    // Do this before extracting JSON. Axum's Json extractor would otherwise read a potentially
    // slow or oversized body for an unauthenticated peer.
    if !has_valid_local_auth(request.headers(), &state.local_token) {
        tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "responses", code = "unauthorized", status = 401_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge request rejected");
        return error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "Invalid local bearer token.",
            None,
        );
    }
    if state.force_shutdown.is_cancelled() {
        return stopping_response();
    }
    let permit = match state.admission.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "responses", code = "overloaded", status = 429_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge profile is at request capacity");
            return error_response(
                StatusCode::TOO_MANY_REQUESTS,
                "bridge_overloaded",
                "The local bridge is temporarily busy.",
                None,
            );
        }
    };
    let body = match read_request_json(request).await {
        Ok(body) => body,
        Err(response) => return response,
    };

    let request = match parse_responses_request(&body) {
        Ok(request) => request,
        Err(error) => {
            log_protocol_error(&state, &request_id, started, &error);
            return protocol_error_response(error);
        }
    };
    let stream_requested = request.stream;
    let protocol = state.upstream.protocol;
    let mut upstream_body = match protocol {
        BridgeUpstreamProtocol::KimiChatCompletions => to_kimi_chat_request(&request),
        BridgeUpstreamProtocol::AnthropicMessages => to_anthropic_messages_request(&request),
        BridgeUpstreamProtocol::CodexResponsesOauth => {
            unreachable!("messages handler owns Codex Responses OAuth")
        }
    };
    if let Some(model) = &state.upstream.model {
        upstream_body["model"] = Value::String(model.clone());
    }
    let path = match protocol {
        BridgeUpstreamProtocol::KimiChatCompletions => "chat/completions",
        BridgeUpstreamProtocol::AnthropicMessages => "messages",
        BridgeUpstreamProtocol::CodexResponsesOauth => {
            unreachable!("messages handler owns Codex Responses OAuth")
        }
    };
    let url = match state.upstream_url.join(path) {
        Ok(url) => url,
        Err(_) => {
            state.record_upstream_failure();
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider is unavailable.",
                None,
            );
        }
    };
    let mut builder = state.client.post(url).json(&upstream_body);
    builder = match protocol {
        BridgeUpstreamProtocol::KimiChatCompletions => {
            builder.bearer_auth(state.upstream.auth.token())
        }
        BridgeUpstreamProtocol::AnthropicMessages => builder
            .header("x-api-key", state.upstream.auth.token())
            .header("anthropic-version", ANTHROPIC_API_VERSION),
        BridgeUpstreamProtocol::CodexResponsesOauth => {
            unreachable!("messages handler owns Codex Responses OAuth")
        }
    };
    let upstream_request = builder.send();
    let upstream = tokio::select! {
        _ = state.force_shutdown.cancelled() => return stopping_response(),
        result = tokio::time::timeout(UPSTREAM_RESPONSE_HEADER_TIMEOUT, upstream_request) => match result {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "header_timeout", status = 504_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge upstream response headers timed out");
                state.record_upstream_failure();
                return error_response(
                    StatusCode::GATEWAY_TIMEOUT,
                    "upstream_timeout",
                    "The upstream model provider timed out.",
                    None,
                );
            }
        },
    };
    let response = match upstream {
        Ok(response) => response,
        Err(_) => {
            tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "unavailable", status = 502_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge upstream unavailable");
            state.record_upstream_failure();
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_unavailable",
                "The upstream model provider is unavailable.",
                None,
            );
        }
    };
    if !response.status().is_success() {
        let status = response.status();
        let retry_after = response.headers().get(header::RETRY_AFTER).cloned();
        let local_status = if status == StatusCode::TOO_MANY_REQUESTS {
            StatusCode::TOO_MANY_REQUESTS
        } else {
            StatusCode::BAD_GATEWAY
        };
        tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "upstream_status", status = status.as_u16(), elapsed_ms = started.elapsed().as_millis() as u64, "bridge upstream returned an error");
        state.record_upstream_failure();
        return error_response(
            local_status,
            "upstream_error",
            "The upstream model provider returned an error.",
            retry_after,
        );
    }
    if stream_requested {
        stream_response(state, response, request_id, started, permit)
    } else {
        let force_shutdown = state.force_shutdown.clone();
        tokio::select! {
            _ = force_shutdown.cancelled() => stopping_response(),
            result = tokio::time::timeout(
                UPSTREAM_NON_STREAM_TIMEOUT,
                non_stream_response(state.clone(), response, request_id, started, permit),
            ) => match result {
                Ok(response) => response,
                Err(_) => {
                    state.record_upstream_failure();
                    error_response(
                        StatusCode::GATEWAY_TIMEOUT,
                        "upstream_timeout",
                        "The upstream model provider timed out.",
                        None,
                    )
                }
            },
        }
    }
}

async fn handle_messages(state: AppState, request: Request) -> Response {
    let request_id = uuid::Uuid::new_v4().to_string();
    let started = Instant::now();
    if !has_valid_local_auth(request.headers(), &state.local_token) {
        tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "messages", code = "unauthorized", status = 401_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge request rejected");
        return error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "Invalid local bearer token.",
            None,
        );
    }
    if state.force_shutdown.is_cancelled() {
        return stopping_response();
    }
    let permit = match state.admission.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "messages", code = "overloaded", status = 429_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge profile is at request capacity");
            return error_response(
                StatusCode::TOO_MANY_REQUESTS,
                "bridge_overloaded",
                "The local bridge is temporarily busy.",
                None,
            );
        }
    };
    let body = match read_request_json(request).await {
        Ok(body) => body,
        Err(response) => return response,
    };
    let request = match parse_messages_request(&body) {
        Ok(request) => request,
        Err(error) => {
            log_protocol_error(&state, &request_id, started, &error);
            return protocol_error_response(error);
        }
    };
    let stream_requested = request.stream;
    let mut upstream_body = to_responses_request(&request);
    if let Some(model) = &state.upstream.model {
        upstream_body["model"] = Value::String(model.clone());
    }
    let url = match state.upstream_url.join("responses") {
        Ok(url) => url,
        Err(_) => {
            state.record_upstream_failure();
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider is unavailable.",
                None,
            );
        }
    };
    let upstream = tokio::select! {
        _ = state.force_shutdown.cancelled() => return stopping_response(),
        result = tokio::time::timeout(
            UPSTREAM_RESPONSE_HEADER_TIMEOUT,
            state.client
                .post(url)
                .bearer_auth(state.upstream.auth.token())
                .json(&upstream_body)
                .send(),
        ) => match result {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "header_timeout", status = 504_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge upstream response headers timed out");
                state.record_upstream_failure();
                return error_response(
                    StatusCode::GATEWAY_TIMEOUT,
                    "upstream_timeout",
                    "The upstream model provider timed out.",
                    None,
                );
            }
        },
    };
    let response = match upstream {
        Ok(response) => response,
        Err(_) => {
            tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "unavailable", status = 502_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge upstream unavailable");
            state.record_upstream_failure();
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_unavailable",
                "The upstream model provider is unavailable.",
                None,
            );
        }
    };
    if !response.status().is_success() {
        let status = response.status();
        let retry_after = response.headers().get(header::RETRY_AFTER).cloned();
        let local_status = if status == StatusCode::TOO_MANY_REQUESTS {
            StatusCode::TOO_MANY_REQUESTS
        } else {
            StatusCode::BAD_GATEWAY
        };
        tracing::warn!(target: "core.adapter", profile_id = %state.profile_id, request_id = %request_id, op = "upstream", code = "upstream_status", status = status.as_u16(), elapsed_ms = started.elapsed().as_millis() as u64, "bridge upstream returned an error");
        state.record_upstream_failure();
        return error_response(
            local_status,
            "upstream_error",
            "The upstream model provider returned an error.",
            retry_after,
        );
    }
    if stream_requested {
        messages_stream_response(state, response, request_id, started, permit)
    } else {
        let force_shutdown = state.force_shutdown.clone();
        tokio::select! {
            _ = force_shutdown.cancelled() => stopping_response(),
            result = tokio::time::timeout(
                UPSTREAM_NON_STREAM_TIMEOUT,
                messages_non_stream_response(state.clone(), response, request_id, started, permit),
            ) => match result {
                Ok(response) => response,
                Err(_) => {
                    state.record_upstream_failure();
                    error_response(
                        StatusCode::GATEWAY_TIMEOUT,
                        "upstream_timeout",
                        "The upstream model provider timed out.",
                        None,
                    )
                }
            },
        }
    }
}

async fn read_request_json(request: Request) -> Result<Value, Response> {
    let body = match tokio::time::timeout(
        REQUEST_BODY_TIMEOUT,
        axum::body::to_bytes(request.into_body(), BODY_LIMIT_BYTES),
    )
    .await
    {
        Ok(Ok(body)) => body,
        Ok(Err(_)) => {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "The request body is invalid or too large.",
                None,
            ))
        }
        Err(_) => {
            return Err(error_response(
                StatusCode::REQUEST_TIMEOUT,
                "request_timeout",
                "The request body timed out.",
                None,
            ))
        }
    };
    serde_json::from_slice::<Value>(&body).map_err(|_| {
        error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "The request body must be valid JSON.",
            None,
        )
    })
}

async fn messages_non_stream_response(
    state: AppState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    _permit: OwnedSemaphorePermit,
) -> Response {
    let upstream_body = match read_bounded_upstream_json(response, &state.force_shutdown).await {
        Ok(value) => value,
        Err(UpstreamBodyError::Stopping) => return stopping_response(),
        Err(UpstreamBodyError::InvalidOrTooLarge) => {
            state.record_upstream_failure();
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider returned an invalid response.",
                None,
            );
        }
    };
    let translated =
        responses_output_to_ir(&upstream_body).and_then(|ir| encode_anthropic_message(&ir));
    match translated {
        Ok(value) => {
            state.record_upstream_success();
            tracing::info!(target: "core.adapter.protocol", profile_id = %state.profile_id, request_id = %request_id, op = "messages", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge response completed");
            Json(value).into_response()
        }
        Err(error) => {
            state.record_upstream_failure();
            log_protocol_error(&state, &request_id, started, &error);
            protocol_error_response(error)
        }
    }
}

async fn non_stream_response(
    state: AppState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    _permit: OwnedSemaphorePermit,
) -> Response {
    let upstream_body = match read_bounded_upstream_json(response, &state.force_shutdown).await {
        Ok(value) => value,
        Err(UpstreamBodyError::Stopping) => return stopping_response(),
        Err(UpstreamBodyError::InvalidOrTooLarge) => {
            state.record_upstream_failure();
            return error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                "The upstream model provider returned an invalid response.",
                None,
            );
        }
    };
    let translated = match state.upstream.protocol {
        BridgeUpstreamProtocol::KimiChatCompletions => {
            crate::bridge::protocol::chat::translate_chat_response(
                &upstream_body,
                Some(&request_id),
            )
        }
        BridgeUpstreamProtocol::AnthropicMessages => anthropic_message_to_ir(&upstream_body)
            .and_then(|ir| encode_responses_from_ir(&ir, Some(&request_id))),
        BridgeUpstreamProtocol::CodexResponsesOauth => {
            unreachable!("messages handler owns Codex Responses OAuth")
        }
    };
    match translated {
        Ok(value) => {
            state.record_upstream_success();
            tracing::info!(target: "core.adapter.protocol", profile_id = %state.profile_id, request_id = %request_id, op = "responses", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge response completed");
            Json(value).into_response()
        }
        Err(error) => {
            state.record_upstream_failure();
            log_protocol_error(&state, &request_id, started, &error);
            protocol_error_response(error)
        }
    }
}

enum UpstreamBodyError {
    Stopping,
    InvalidOrTooLarge,
}

async fn read_bounded_upstream_json(
    response: reqwest::Response,
    force_shutdown: &CancellationToken,
) -> Result<Value, UpstreamBodyError> {
    if response
        .content_length()
        .is_some_and(|length| length > BODY_LIMIT_BYTES as u64)
    {
        return Err(UpstreamBodyError::InvalidOrTooLarge);
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = tokio::select! {
        _ = force_shutdown.cancelled() => return Err(UpstreamBodyError::Stopping),
        next = tokio::time::timeout(UPSTREAM_BODY_IDLE_TIMEOUT, stream.next()) => match next {
            Ok(next) => next,
            Err(_) => return Err(UpstreamBodyError::InvalidOrTooLarge),
        },
    } {
        let chunk = chunk.map_err(|_| UpstreamBodyError::InvalidOrTooLarge)?;
        if body.len().saturating_add(chunk.len()) > BODY_LIMIT_BYTES {
            return Err(UpstreamBodyError::InvalidOrTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| UpstreamBodyError::InvalidOrTooLarge)
}

enum StreamCodec {
    Kimi(crate::bridge::protocol::chat::ResponsesSseTranslator),
    Anthropic {
        ir: AnthropicStreamToIr,
        out: IrToResponsesSse,
    },
}

impl StreamCodec {
    fn new(protocol: BridgeUpstreamProtocol, request_id: String, model: String) -> Self {
        match protocol {
            BridgeUpstreamProtocol::KimiChatCompletions => Self::Kimi(
                crate::bridge::protocol::chat::ResponsesSseTranslator::new(request_id, model),
            ),
            BridgeUpstreamProtocol::AnthropicMessages => Self::Anthropic {
                ir: AnthropicStreamToIr::new(),
                out: IrToResponsesSse::new(request_id, model),
            },
            BridgeUpstreamProtocol::CodexResponsesOauth => {
                unreachable!("messages handler owns Codex Responses OAuth")
            }
        }
    }

    fn push(&mut self, value: &Value) -> Result<Vec<BridgeEvent>, ProtocolError> {
        match self {
            Self::Kimi(translator) => translator.push_chunk(value),
            Self::Anthropic { ir, out } => {
                let events = ir.push_event(value)?;
                let mut frames = Vec::new();
                for event in events {
                    frames.extend(out.push_event(&event)?);
                }
                Ok(frames)
            }
        }
    }

    fn finish(&mut self) -> Result<Vec<BridgeEvent>, ProtocolError> {
        match self {
            Self::Kimi(translator) => Ok(translator.finish()),
            Self::Anthropic { ir, out } => {
                let events = ir.finish();
                let mut frames = Vec::new();
                for event in events {
                    frames.extend(out.push_event(&event)?);
                }
                frames.extend(out.finish());
                Ok(frames)
            }
        }
    }

    fn completed(&self) -> bool {
        match self {
            Self::Kimi(_) => false,
            Self::Anthropic { ir, .. } => ir.completed(),
        }
    }

    fn treats_done_marker(&self) -> bool {
        matches!(self, Self::Kimi(_))
    }
}

fn stream_response(
    state: AppState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
) -> Response {
    let model = state.upstream.model.clone().unwrap_or_default();
    let profile_id = state.profile_id.clone();
    let force_shutdown = state.force_shutdown.clone();
    let observed = state.clone();
    let protocol = state.upstream.protocol;
    let bytes = response.bytes_stream();
    let output = stream! {
        let mut translator = StreamCodec::new(protocol, request_id.clone(), model);
        // `VecDeque` lets us consume complete SSE frames from the front without repeatedly
        // moving the unread tail. The cap counts all upstream bytes, not merely the current
        // partial frame, and the output cap protects a pathological translator expansion.
        let mut buffer = std::collections::VecDeque::new();
        let mut upstream_bytes = 0usize;
        let mut output_bytes = 0usize;
        let _permit = permit;
        let mut saw_done = false;
        futures_util::pin_mut!(bytes);
        'upstream: loop {
            let next = tokio::select! {
                _ = force_shutdown.cancelled() => {
                    yield Ok::<_, Infallible>(stream_error_frame());
                    return;
                }
                next = tokio::time::timeout(UPSTREAM_STREAM_IDLE_TIMEOUT, bytes.next()) => match next {
                    Ok(next) => next,
                    Err(_) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                },
            };
            let Some(chunk) = next else { break; };
            let Ok(chunk) = chunk else {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            };
            if upstream_bytes.saturating_add(chunk.len()) > STREAM_LIMIT_BYTES {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            }
            upstream_bytes += chunk.len();
            buffer.extend(chunk.iter().copied());
            while let Some((frame_end, delimiter_len)) = sse_frame_end_deque(&buffer) {
                let frame = buffer.drain(..frame_end).collect::<Vec<_>>();
                for _ in 0..delimiter_len {
                    let _ = buffer.pop_front();
                }
                let payload = match sse_data_payload(&frame) {
                    Ok(payload) => payload,
                    Err(()) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                };
                let Some(payload) = payload else { continue; };
                if payload.is_empty() { continue; }
                if payload == "[DONE]" {
                    if translator.treats_done_marker() {
                        saw_done = true;
                        break 'upstream;
                    }
                    continue;
                }
                let Ok(value) = serde_json::from_str::<Value>(&payload) else {
                    observed.record_upstream_failure();
                    yield Ok::<_, Infallible>(stream_error_frame());
                    return;
                };
                match translator.push(&value) {
                    Ok(events) => for event in events {
                        let frame = crate::bridge::protocol::chat::sse_frame(&event);
                        if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                            observed.record_upstream_failure();
                            yield Ok::<_, Infallible>(stream_error_frame());
                            return;
                        }
                        output_bytes += frame.len();
                        yield Ok::<_, Infallible>(axum::body::Bytes::from(frame));
                    },
                    Err(_) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                }
                if translator.completed() {
                    saw_done = true;
                    break 'upstream;
                }
            }
        }
        // A clean EOF without the provider's terminal marker is not a completed response. This
        // distinction matters to response clients, which otherwise persist a truncated answer.
        if !saw_done || !buffer.is_empty() {
            observed.record_upstream_failure();
            yield Ok::<_, Infallible>(stream_error_frame());
            return;
        }
        match translator.finish() {
            Ok(events) => {
                for event in events {
                    let frame = crate::bridge::protocol::chat::sse_frame(&event);
                    if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                    output_bytes += frame.len();
                    yield Ok::<_, Infallible>(axum::body::Bytes::from(frame));
                }
            }
            Err(_) => {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            }
        }
        observed.record_upstream_success();
        tracing::info!(target: "core.adapter.protocol", profile_id = %profile_id, request_id = %request_id, op = "stream", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge stream completed");
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    (StatusCode::OK, headers, Body::from_stream(output)).into_response()
}

fn messages_stream_response(
    state: AppState,
    response: reqwest::Response,
    request_id: String,
    started: Instant,
    permit: OwnedSemaphorePermit,
) -> Response {
    let profile_id = state.profile_id.clone();
    let force_shutdown = state.force_shutdown.clone();
    let observed = state.clone();
    let bytes = response.bytes_stream();
    let output = stream! {
        let mut translator = ResponsesStreamToIr::new();
        let mut ir_events: Vec<IrEvent> = Vec::new();
        let mut emitted_frames = 0usize;
        let mut buffer = std::collections::VecDeque::new();
        let mut upstream_bytes = 0usize;
        let mut output_bytes = 0usize;
        let _permit = permit;
        let mut saw_done = false;
        futures_util::pin_mut!(bytes);
        'upstream: loop {
            let next = tokio::select! {
                _ = force_shutdown.cancelled() => {
                    yield Ok::<_, Infallible>(stream_error_frame());
                    return;
                }
                next = tokio::time::timeout(UPSTREAM_STREAM_IDLE_TIMEOUT, bytes.next()) => match next {
                    Ok(next) => next,
                    Err(_) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                },
            };
            let Some(chunk) = next else { break; };
            let Ok(chunk) = chunk else {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            };
            if upstream_bytes.saturating_add(chunk.len()) > STREAM_LIMIT_BYTES {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            }
            upstream_bytes += chunk.len();
            buffer.extend(chunk.iter().copied());
            while let Some((frame_end, delimiter_len)) = sse_frame_end_deque(&buffer) {
                let frame = buffer.drain(..frame_end).collect::<Vec<_>>();
                for _ in 0..delimiter_len {
                    let _ = buffer.pop_front();
                }
                let payload = match sse_data_payload(&frame) {
                    Ok(payload) => payload,
                    Err(()) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                };
                let Some(payload) = payload else { continue; };
                if payload.is_empty() || payload == "[DONE]" { continue; }
                let Ok(value) = serde_json::from_str::<Value>(&payload) else {
                    observed.record_upstream_failure();
                    yield Ok::<_, Infallible>(stream_error_frame());
                    return;
                };
                let events = match translator.push_event(&value) {
                    Ok(events) => events,
                    Err(_) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                };
                let completed = events
                    .iter()
                    .any(|event| matches!(event, IrEvent::MessageEnd { .. }));
                ir_events.extend(events);
                let frames = match encode_anthropic_sse(&ir_events) {
                    Ok(frames) => frames,
                    Err(_) => {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                };
                for frame in frames.iter().skip(emitted_frames) {
                    if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                        observed.record_upstream_failure();
                        yield Ok::<_, Infallible>(stream_error_frame());
                        return;
                    }
                    output_bytes += frame.len();
                    yield Ok::<_, Infallible>(axum::body::Bytes::from(frame.clone()));
                }
                emitted_frames = frames.len();
                if completed {
                    saw_done = true;
                    break 'upstream;
                }
            }
        }
        if !saw_done || !buffer.is_empty() {
            observed.record_upstream_failure();
            yield Ok::<_, Infallible>(stream_error_frame());
            return;
        }
        ir_events.extend(translator.finish());
        let frames = match encode_anthropic_sse(&ir_events) {
            Ok(frames) => frames,
            Err(_) => {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            }
        };
        for frame in frames.iter().skip(emitted_frames) {
            if output_bytes.saturating_add(frame.len()) > STREAM_LIMIT_BYTES {
                observed.record_upstream_failure();
                yield Ok::<_, Infallible>(stream_error_frame());
                return;
            }
            output_bytes += frame.len();
            yield Ok::<_, Infallible>(axum::body::Bytes::from(frame.clone()));
        }
        observed.record_upstream_success();
        tracing::info!(target: "core.adapter.protocol", profile_id = %profile_id, request_id = %request_id, op = "messages_stream", status = 200_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge stream completed");
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    (StatusCode::OK, headers, Body::from_stream(output)).into_response()
}

#[cfg(test)]
pub(super) fn sse_frame_end(buffer: &[u8]) -> Option<(usize, usize)> {
    let deque: std::collections::VecDeque<u8> = buffer.iter().copied().collect();
    sse_frame_end_deque(&deque)
}

fn sse_frame_end_deque(buffer: &std::collections::VecDeque<u8>) -> Option<(usize, usize)> {
    let mut crlf = None;
    let mut lf = None;
    for index in 0..buffer.len() {
        if crlf.is_none()
            && index + 4 <= buffer.len()
            && buffer
                .iter()
                .skip(index)
                .take(4)
                .copied()
                .eq(b"\r\n\r\n".iter().copied())
        {
            crlf = Some((index, 4));
        }
        if lf.is_none()
            && index + 2 <= buffer.len()
            && buffer
                .iter()
                .skip(index)
                .take(2)
                .copied()
                .eq(b"\n\n".iter().copied())
        {
            lf = Some((index, 2));
        }
        if crlf.is_some() && lf.is_some() {
            break;
        }
    }
    match (crlf, lf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(frame), None) | (None, Some(frame)) => Some(frame),
        (None, None) => None,
    }
}

fn sse_data_payload(frame: &[u8]) -> Result<Option<String>, ()> {
    let frame = std::str::from_utf8(frame).map_err(|_| ())?;
    let payload = frame
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|line| line.strip_prefix(' ').unwrap_or(line))
        .collect::<Vec<_>>();
    if payload.is_empty() {
        Ok(None)
    } else {
        Ok(Some(payload.join("\n")))
    }
}

fn stream_error_frame() -> axum::body::Bytes {
    axum::body::Bytes::from_static(b"event: error\ndata: {\"type\":\"error\",\"error\":{\"code\":\"upstream_error\",\"message\":\"The upstream model provider returned an invalid stream.\"}}\n\n")
}

fn stopping_response() -> Response {
    error_response(
        StatusCode::SERVICE_UNAVAILABLE,
        "bridge_stopping",
        "The local bridge is stopping.",
        None,
    )
}

fn has_valid_local_auth(headers: &HeaderMap, expected: &str) -> bool {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    let api_key = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok());
    bearer
        .or(api_key)
        .is_some_and(|token| constant_time_eq(token.as_bytes(), expected.as_bytes()))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut mismatch = left.len() ^ right.len();
    let width = left.len().max(right.len());
    for index in 0..width {
        mismatch |= usize::from(*left.get(index).unwrap_or(&0) ^ *right.get(index).unwrap_or(&0));
    }
    mismatch == 0
}

fn protocol_error_response(error: ProtocolError) -> Response {
    error_response(StatusCode::BAD_REQUEST, error.code, &error.message, None)
}

fn log_protocol_error(state: &AppState, request_id: &str, started: Instant, error: &ProtocolError) {
    tracing::warn!(target: "core.adapter.protocol", profile_id = %state.profile_id, request_id, op = "protocol", code = error.code, status = 400_u16, elapsed_ms = started.elapsed().as_millis() as u64, "bridge protocol rejected request");
}

fn error_response(
    status: StatusCode,
    code: &str,
    message: &str,
    retry_after: Option<HeaderValue>,
) -> Response {
    let mut response = (
        status,
        Json(json!({ "error": { "code": code, "message": message, "type": "invalid_request_error" } })),
    )
        .into_response();
    if let Some(retry_after) = retry_after {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, retry_after);
    }
    response
}
