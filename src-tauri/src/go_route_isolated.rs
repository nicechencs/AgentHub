//! Isolated Go route supervisor for saved test routes.
//!
//! Scratch home only. Refuses product port 43121 and real ~/.agenthub.
//! Does not start BridgeRuntimeHost or write login / connection / Agent config.

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, OpenOptions};
#[cfg(unix)]
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(unix)]
use std::sync::Condvar;
#[cfg(unix)]
use std::sync::Weak;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[cfg(unix)]
use std::{os::unix::fs::MetadataExt, os::unix::fs::OpenOptionsExt};

#[cfg(unix)]
use agenthub_core::error::AppError;
use agenthub_core::AgentHub;

const PRODUCT_DEFAULT_PORT: u16 = 43121;
const OWNER_ID: &str = "agenthub-gui";
const PROTOCOL_VERSION: &str = "route-runtime.v0-isolated";
const CONFIG_FORMAT_VERSION: &str = "route-config.v0-isolated";
#[cfg(debug_assertions)]
const ISOLATED_DEV_PACKAGE_VERSION: &str = "0.0.0-isolated";
const BUNDLED_PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");
const BUNDLED_SHA256: &str = env!("AGENTHUB_ADAPTERD_BUNDLED_SHA256");
const EMBEDDED_BUNDLED_VERSION: &str = env!("AGENTHUB_ADAPTERD_BUNDLED_VERSION");
const CONFIG_STREAM_CAPABILITY: &str = "config.stdin_stream.atomic";
const OAUTH_REFRESH_CAPABILITY: &str = "control.oauth_refresh.v1";
const ISOLATED_LEASE_BUDGET_MS: i64 = 24 * 60 * 60 * 1_000;
const OWNER_RENEW_INTERVAL: Duration = Duration::from_secs(30);
#[cfg(unix)]
const OAUTH_REFRESH_POLL_WAIT_MS: u64 = 1_000;
#[cfg(unix)]
const OAUTH_REFRESH_CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(unix)]
const OAUTH_REFRESH_HANDLER_LIMIT: usize = 8;
#[cfg(unix)]
const OAUTH_REFRESH_QUEUE_LIMIT: usize = 8;
#[cfg(unix)]
const OAUTH_REFRESH_ACTION_GATE_LIMIT: usize = 4_096;
#[cfg(unix)]
const OAUTH_REFRESH_COMPLETED_ID_LIMIT: usize = 4_096;
#[cfg(unix)]
const OAUTH_REFRESH_COMPLETED_ID_TTL: Duration = Duration::from_secs(90);
const START_STOP_WAIT: Duration = Duration::from_secs(10);
const GRACEFUL_STOP_WAIT: Duration = Duration::from_secs(9);
const CONFIG_WRITE_WAIT: Duration = Duration::from_secs(8);
const MAX_RECOVERY_BUDGET: u32 = 3;
const STABLE_RUN_RESET: Duration = Duration::from_secs(30);
const ERROR_ISOLATED_UNAVAILABLE: &str = "Go route is unavailable in this build";
const ERROR_START_FAILED: &str = "Go route could not start";
const ERROR_CONTROL_UNAVAILABLE: &str = "Go route status is unavailable";
#[cfg(unix)]
const ERROR_REQUIRED_RELOAD_FAILED: &str = "go.route.required_reload_failed";
#[cfg(unix)]
const REQUIRED_RELOAD_STOPPED_MESSAGE: &str =
    "Go route configuration could not be updated; Go route was stopped";
const MAX_RUNTIME_CONFIG_BYTES: usize = 8 * 1024 * 1024;
#[cfg(unix)]
const SCRATCH_ROOT_PREFIX: &str = "agenthub-go-route-isolated-";
#[cfg(unix)]
const SCRATCH_OWNER_MARKER: &str = ".agenthub-go-route-owner-v1";
#[cfg(unix)]
const STALE_SCRATCH_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoRouteIsolatedStatus {
    pub state: String,
    pub listen_ready: bool,
    pub port: Option<u16>,
    pub last_error: Option<String>,
    pub home: Option<String>,
    pub lifecycle: Option<String>,
    pub in_flight_count: u64,
    pub member_count: u64,
    pub healthy_member_count: u64,
    pub recovering: bool,
    pub restart_count: u32,
}

/// Result of synchronizing a committed product write into the optional Go
/// route. This is deliberately a value rather than an error: callers need to
/// distinguish an unavailable/disabled experiment from an enabled route that
/// was stopped because it could not accept the new configuration.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum GoRouteRequiredReloadResult {
    Reloaded {
        config_hash: String,
        instance_epoch: String,
        port: u16,
    },
    Skipped {
        reason: GoRouteRequiredReloadSkipReason,
    },
    Failed {
        code: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoRouteRequiredReloadSkipReason {
    NotRunning,
    #[allow(dead_code)] // constructed only by non-Unix builds
    Unavailable,
    Unchanged,
}

pub struct GoRouteIsolatedHost {
    hub: Option<Arc<AgentHub>>,
    inner: Mutex<Inner>,
    update_gate: Mutex<()>,
}

#[cfg(unix)]
impl Drop for GoRouteIsolatedHost {
    fn drop(&mut self) {
        let inner = self
            .inner
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(mut session) = inner.session.take() {
            terminate_session(&mut session);
        }
    }
}

struct Inner {
    status: GoRouteIsolatedStatus,
    session: Option<Session>,
    desired: bool,
    stopping: bool,
    recovery_budget_used: u32,
    stable_since: Option<Instant>,
    next_restart_at: Option<Instant>,
    committed_plan: Option<RuntimePlan>,
    #[cfg(unix)]
    oauth_refresh_worker_session: Option<(String, i64)>,
}

#[derive(Clone)]
struct RuntimePlan {
    config: Vec<u8>,
    config_hash: String,
    port: u16,
}

struct Session {
    home: PathBuf,
    socket: PathBuf,
    owner_term: i64,
    instance_epoch: String,
    port: u16,
    next_owner_renewal: Instant,
    oauth_refresh_supported: bool,
    config_stdin: Arc<Mutex<ChildStdin>>,
    adapterd: Child,
}

#[cfg(unix)]
struct ResolvedAdapterd {
    path: PathBuf,
    package_version: &'static str,
}

#[cfg(unix)]
struct ScratchHomeGuard {
    home: PathBuf,
    armed: bool,
}

#[cfg(unix)]
impl ScratchHomeGuard {
    fn disarm(mut self) {
        self.armed = false;
    }
}

#[cfg(unix)]
impl Drop for ScratchHomeGuard {
    fn drop(&mut self) {
        if self.armed {
            cleanup_scratch_home(&self.home);
        }
    }
}

#[cfg(unix)]
#[derive(Clone)]
struct ControlSession {
    home: PathBuf,
    socket: PathBuf,
    owner_term: i64,
    instance_epoch: String,
}

fn stopped_status() -> GoRouteIsolatedStatus {
    GoRouteIsolatedStatus {
        state: "stopped".into(),
        listen_ready: false,
        port: None,
        last_error: None,
        home: None,
        lifecycle: Some("stopped".into()),
        in_flight_count: 0,
        member_count: 0,
        healthy_member_count: 0,
        recovering: false,
        restart_count: 0,
    }
}

fn unavailable_status() -> GoRouteIsolatedStatus {
    GoRouteIsolatedStatus {
        state: "failed".into(),
        listen_ready: false,
        port: None,
        last_error: Some(ERROR_ISOLATED_UNAVAILABLE.into()),
        home: None,
        lifecycle: Some("unavailable".into()),
        in_flight_count: 0,
        member_count: 0,
        healthy_member_count: 0,
        recovering: false,
        restart_count: 0,
    }
}

impl GoRouteIsolatedHost {
    pub fn new(hub: Option<Arc<AgentHub>>) -> Arc<Self> {
        #[cfg(unix)]
        cleanup_stale_scratch_roots();
        let host = Arc::new(Self {
            hub,
            inner: Mutex::new(Inner {
                status: stopped_status(),
                session: None,
                desired: false,
                stopping: false,
                recovery_budget_used: 0,
                stable_since: None,
                next_restart_at: None,
                committed_plan: None,
                #[cfg(unix)]
                oauth_refresh_worker_session: None,
            }),
            update_gate: Mutex::new(()),
        });
        #[cfg(unix)]
        Self::spawn_monitor(Arc::downgrade(&host));
        #[cfg(unix)]
        if let Some(hub) = host.hub.as_ref() {
            let weak_host = Arc::downgrade(&host);
            hub.accounts().set_oauth_access_publish(Arc::new(move || {
                let Some(host) = weak_host.upgrade() else {
                    return Ok(());
                };
                match host.reload_required_after_write() {
                    GoRouteRequiredReloadResult::Reloaded { .. }
                    | GoRouteRequiredReloadResult::Skipped { .. } => Ok(()),
                    GoRouteRequiredReloadResult::Failed { .. } => Err(AppError::message(
                        ERROR_REQUIRED_RELOAD_FAILED,
                        REQUIRED_RELOAD_STOPPED_MESSAGE,
                    )),
                }
            }));
        }
        host
    }

    pub fn status(&self) -> GoRouteIsolatedStatus {
        #[cfg(not(unix))]
        {
            return unix_only_failed();
        }
        #[cfg(unix)]
        {
            let control = {
                let mut inner = self.lock();
                refresh_locked(&mut inner);
                inner.session.as_ref().map(ControlSession::from)
            };
            let Some(control) = control else {
                return self.lock().status.clone();
            };
            let result = session_status(&control);
            let mut inner = self.lock();
            refresh_locked(&mut inner);
            let same_session = inner
                .session
                .as_ref()
                .map(|session| session.instance_epoch.as_str())
                == Some(control.instance_epoch.as_str());
            if same_session {
                match result {
                    Ok(status) if owner_lease_valid(&status) => {
                        inner.status = status_with_supervisor(status, &inner.status);
                    }
                    Ok(_) | Err(_) => {
                        let mut session = inner.session.take();
                        mark_runtime_unavailable(&mut inner);
                        drop(inner);
                        if let Some(session) = session.as_mut() {
                            terminate_session(session);
                        }
                        return self.lock().status.clone();
                    }
                }
            }
            inner.status.clone()
        }
    }

    pub fn start(&self) -> GoRouteIsolatedStatus {
        #[cfg(not(unix))]
        {
            return unix_only_failed();
        }
        #[cfg(unix)]
        {
            let reload = {
                let mut inner = self.lock();
                refresh_locked(&mut inner);
                inner.desired = true;
                inner.stopping = false;
                inner.recovery_budget_used = 0;
                inner.stable_since = None;
                inner.next_restart_at = None;
                if inner.status.state == "starting" {
                    return inner.status.clone();
                }
                if inner.session.is_some() {
                    true
                } else {
                    inner.status.state = "starting".into();
                    inner.status.last_error = None;
                    inner.status.listen_ready = false;
                    false
                }
            };
            if reload {
                return self.reload();
            }
            self.finish_start(false)
        }
    }

    /// Rebuilds the complete edge table and swaps it into the running Go
    /// process. The previous committed snapshot remains the recovery source
    /// until Status acknowledges the exact new digest.
    pub fn reload(&self) -> GoRouteIsolatedStatus {
        #[cfg(not(unix))]
        {
            return unix_only_failed();
        }
        #[cfg(unix)]
        {
            let _update = self
                .update_gate
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(hub) = self.hub.as_ref() else {
                return unavailable_status();
            };
            let config = match build_runtime_config(hub) {
                Ok(config) => config,
                Err(_) => return self.status_with_reload_error(),
            };
            let config_hash = sha256_hex(&config);

            let (config_stdin, control, port) = {
                let mut inner = self.lock();
                refresh_locked(&mut inner);
                let Some(session) = inner.session.as_ref() else {
                    return inner.status.clone();
                };
                (
                    Arc::clone(&session.config_stdin),
                    ControlSession::from(session),
                    session.port,
                )
            };
            let config =
                match write_runtime_config_with_timeout(config_stdin, config, CONFIG_WRITE_WAIT) {
                    Ok(config) => config,
                    Err(_) => return self.fail_session(&control),
                };

            let deadline = Instant::now() + Duration::from_secs(8);
            let acknowledged = loop {
                match session_status(&control) {
                    Ok(status)
                        if status.get("active_hash").and_then(Value::as_str)
                            == Some(config_hash.as_str())
                            && status.get("instance_epoch").and_then(Value::as_str)
                                == Some(control.instance_epoch.as_str())
                            && status.get("port").and_then(Value::as_u64)
                                == Some(u64::from(port))
                            && status.get("listen_ready").and_then(Value::as_bool)
                                == Some(true) =>
                    {
                        break true;
                    }
                    Ok(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(25));
                    }
                    Ok(_) | Err(_) => break false,
                }
            };
            if !acknowledged {
                return self.fail_session(&control);
            }

            let mut inner = self.lock();
            let same_session = inner
                .session
                .as_ref()
                .is_some_and(|session| session.instance_epoch == control.instance_epoch);
            if !same_session {
                return inner.status.clone();
            }
            inner.committed_plan = Some(RuntimePlan {
                config,
                config_hash,
                port,
            });
            inner.status.clone()
        }
    }

    /// Synchronizes a successful product write into an enabled Go route.
    ///
    /// This method is synchronous so async commands can run it with
    /// `spawn_blocking`. A skipped result never changes product state. Once an
    /// enabled session enters the reload, every failure is fail-closed: the
    /// process is terminated, automatic recovery is disabled, and the last
    /// committed runtime snapshot is discarded so stale configuration cannot
    /// be restored.
    pub fn reload_required_after_write(&self) -> GoRouteRequiredReloadResult {
        #[cfg(not(unix))]
        {
            return GoRouteRequiredReloadResult::Skipped {
                reason: GoRouteRequiredReloadSkipReason::Unavailable,
            };
        }
        #[cfg(unix)]
        {
            let _update = self
                .update_gate
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());

            let session = {
                let mut inner = self.lock();
                refresh_locked(&mut inner);
                if inner.desired && !inner.stopping {
                    inner.session.as_ref().map(|session| {
                        (
                            Arc::clone(&session.config_stdin),
                            ControlSession::from(session),
                            session.port,
                        )
                    })
                } else {
                    return GoRouteRequiredReloadResult::Skipped {
                        reason: GoRouteRequiredReloadSkipReason::NotRunning,
                    };
                }
            };
            let Some((config_stdin, control, port)) = session else {
                return self.fail_required_reload();
            };

            let Some(hub) = self.hub.as_ref() else {
                return self.fail_required_reload();
            };
            let config = match build_runtime_config(hub) {
                Ok(config) => config,
                Err(_) => return self.fail_required_reload(),
            };
            let config_hash = sha256_hex(&config);

            let (same_session, unchanged) = {
                let mut inner = self.lock();
                refresh_locked(&mut inner);
                let same_session = inner.desired
                    && !inner.stopping
                    && inner.session.as_ref().is_some_and(|session| {
                        session.instance_epoch == control.instance_epoch && session.port == port
                    });
                let unchanged = same_session
                    && inner
                        .committed_plan
                        .as_ref()
                        .is_some_and(|plan| plan.config_hash == config_hash && plan.port == port);
                (same_session, unchanged)
            };
            if !same_session {
                return self.fail_required_reload();
            }
            if unchanged {
                return match session_status(&control) {
                    Ok(status)
                        if required_reload_ack_matches(
                            &status,
                            &config_hash,
                            &control.instance_epoch,
                            port,
                        ) =>
                    {
                        GoRouteRequiredReloadResult::Skipped {
                            reason: GoRouteRequiredReloadSkipReason::Unchanged,
                        }
                    }
                    Ok(_) | Err(_) => self.fail_required_reload(),
                };
            }

            let config =
                match write_runtime_config_with_timeout(config_stdin, config, CONFIG_WRITE_WAIT) {
                    Ok(config) => config,
                    Err(_) => return self.fail_required_reload(),
                };

            let deadline = Instant::now() + Duration::from_secs(8);
            let acknowledged = loop {
                match session_status(&control) {
                    Ok(status)
                        if required_reload_ack_matches(
                            &status,
                            &config_hash,
                            &control.instance_epoch,
                            port,
                        ) =>
                    {
                        break true;
                    }
                    Ok(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(25));
                    }
                    Ok(_) | Err(_) => break false,
                }
            };
            if !acknowledged {
                return self.fail_required_reload();
            }

            let committed = {
                let mut inner = self.lock();
                refresh_locked(&mut inner);
                let same_session = inner
                    .session
                    .as_ref()
                    .is_some_and(|session| session.instance_epoch == control.instance_epoch);
                if same_session && inner.desired && !inner.stopping {
                    inner.committed_plan = Some(RuntimePlan {
                        config,
                        config_hash: config_hash.clone(),
                        port,
                    });
                    true
                } else {
                    false
                }
            };
            if !committed {
                return self.fail_required_reload();
            }

            GoRouteRequiredReloadResult::Reloaded {
                config_hash,
                instance_epoch: control.instance_epoch,
                port,
            }
        }
    }

    /// Fail closed when the blocking task that owns a required reload cannot
    /// return a result (for example, because its worker panicked).
    pub fn fail_required_reload_task(&self) -> GoRouteRequiredReloadResult {
        #[cfg(not(unix))]
        {
            GoRouteRequiredReloadResult::Skipped {
                reason: GoRouteRequiredReloadSkipReason::Unavailable,
            }
        }
        #[cfg(unix)]
        {
            self.fail_required_reload()
        }
    }

    #[cfg(unix)]
    fn fail_required_reload(&self) -> GoRouteRequiredReloadResult {
        let mut inner = self.lock();
        let mut session = inner.session.take();
        let restart_count = inner.status.restart_count;
        inner.desired = false;
        inner.stopping = false;
        inner.recovery_budget_used = 0;
        inner.stable_since = None;
        inner.next_restart_at = None;
        inner.committed_plan = None;
        inner.status = GoRouteIsolatedStatus {
            state: "failed".into(),
            listen_ready: false,
            port: None,
            last_error: Some(REQUIRED_RELOAD_STOPPED_MESSAGE.into()),
            home: inner.status.home.clone(),
            lifecycle: Some("stopped".into()),
            in_flight_count: 0,
            member_count: 0,
            healthy_member_count: 0,
            recovering: false,
            restart_count,
        };
        drop(inner);
        if let Some(session) = session.as_mut() {
            terminate_session(session);
        }
        GoRouteRequiredReloadResult::Failed {
            code: ERROR_REQUIRED_RELOAD_FAILED.into(),
        }
    }

    #[cfg(unix)]
    fn fail_session(&self, control: &ControlSession) -> GoRouteIsolatedStatus {
        let mut inner = self.lock();
        let same_session = inner
            .session
            .as_ref()
            .is_some_and(|session| session.instance_epoch == control.instance_epoch);
        let mut session = same_session.then(|| inner.session.take()).flatten();
        if same_session {
            mark_runtime_unavailable(&mut inner);
        }
        drop(inner);
        if let Some(session) = session.as_mut() {
            terminate_session(session);
        }
        self.lock().status.clone()
    }

    #[cfg(unix)]
    fn status_with_reload_error(&self) -> GoRouteIsolatedStatus {
        let mut inner = self.lock();
        inner.status.last_error = Some("Go route configuration could not be updated".into());
        inner.status.clone()
    }

    #[cfg(unix)]
    fn finish_start(&self, recovering: bool) -> GoRouteIsolatedStatus {
        let existing_plan = self.lock().committed_plan.clone();
        let plan = existing_plan.map(Ok).unwrap_or_else(|| {
            self.hub
                .as_ref()
                .ok_or_else(|| ERROR_ISOLATED_UNAVAILABLE.to_string())
                .and_then(|hub| build_runtime_plan(hub))
        });
        let result = plan.and_then(|plan| start_session(&plan).map(|session| (session, plan)));
        match result {
            Ok((mut session, plan)) => {
                let mut inner = self.lock();
                if inner.status.state != "starting" || !inner.desired || inner.stopping {
                    drop(inner);
                    terminate_session(&mut session);
                    let mut inner = self.lock();
                    if !inner.desired {
                        let restart_count = inner.status.restart_count;
                        inner.status = stopped_status();
                        inner.status.restart_count = restart_count;
                    }
                    return inner.status.clone();
                }
                let status = GoRouteIsolatedStatus {
                    state: "ready".into(),
                    listen_ready: true,
                    port: Some(session.port),
                    last_error: None,
                    home: Some(session.home.display().to_string()),
                    lifecycle: Some("serving".into()),
                    in_flight_count: 0,
                    member_count: 0,
                    healthy_member_count: 0,
                    recovering: false,
                    restart_count: inner.status.restart_count + u32::from(recovering),
                };
                inner.session = Some(session);
                inner.committed_plan = Some(plan);
                inner.stable_since = Some(Instant::now());
                inner.next_restart_at = None;
                inner.status = status.clone();
                status
            }
            Err(_) => {
                let mut inner = self.lock();
                inner.session = None;
                inner.recovery_budget_used = inner.recovery_budget_used.saturating_add(1);
                inner.stable_since = None;
                inner.next_restart_at =
                    Some(Instant::now() + restart_backoff(inner.recovery_budget_used));
                let status = GoRouteIsolatedStatus {
                    state: "failed".into(),
                    listen_ready: false,
                    port: None,
                    last_error: Some(ERROR_START_FAILED.into()),
                    home: inner.status.home.clone(),
                    lifecycle: Some("failed".into()),
                    in_flight_count: 0,
                    member_count: 0,
                    healthy_member_count: 0,
                    recovering: inner.desired
                        && !inner.stopping
                        && inner.recovery_budget_used < MAX_RECOVERY_BUDGET,
                    restart_count: inner.status.restart_count,
                };
                inner.status = status.clone();
                status
            }
        }
    }

    pub fn stop(&self) -> GoRouteIsolatedStatus {
        #[cfg(not(unix))]
        {
            return unix_only_failed();
        }
        #[cfg(unix)]
        {
            {
                let mut inner = self.lock();
                inner.desired = false;
                inner.stopping = true;
                inner.next_restart_at = None;
                inner.stable_since = None;
                inner.status.recovering = false;
            }
            let wait_started = Instant::now();
            loop {
                let mut inner = self.lock();
                refresh_locked(&mut inner);
                if let Some(mut session) = inner.session.take() {
                    drop(inner);
                    stop_session(&mut session);
                    let mut inner = self.lock();
                    let restart_count = inner.status.restart_count;
                    inner.status = stopped_status();
                    inner.status.restart_count = restart_count;
                    inner.committed_plan = None;
                    inner.stopping = false;
                    return inner.status.clone();
                }
                if inner.status.state != "starting" {
                    let restart_count = inner.status.restart_count;
                    inner.status = stopped_status();
                    inner.status.restart_count = restart_count;
                    inner.stopping = false;
                    inner.committed_plan = None;
                    return inner.status.clone();
                }
                if wait_started.elapsed() >= START_STOP_WAIT {
                    inner.status.state = "failed".into();
                    inner.status.last_error =
                        Some("Timed out waiting for Go route startup to stop".into());
                    return inner.status.clone();
                }
                drop(inner);
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(unix)]
    fn spawn_monitor(host: Weak<Self>) {
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(500));
            let Some(host) = host.upgrade() else { return };
            let refresh_worker = {
                let mut inner = host.lock();
                refresh_locked(&mut inner);
                let control = inner.session.as_ref().and_then(|session| {
                    (session.oauth_refresh_supported
                        && !inner.oauth_refresh_worker_session.as_ref().is_some_and(
                            |(epoch, term)| {
                                epoch == &session.instance_epoch && *term == session.owner_term
                            },
                        ))
                    .then(|| ControlSession::from(session))
                });
                if let Some(control) = control.as_ref() {
                    inner.oauth_refresh_worker_session =
                        Some((control.instance_epoch.clone(), control.owner_term));
                }
                control
            };
            if let Some(control) = refresh_worker {
                Self::spawn_oauth_refresh_worker(Arc::downgrade(&host), control);
            }
            let renewal = {
                let mut inner = host.lock();
                refresh_locked(&mut inner);
                reset_stable_recovery_budget(&mut inner);
                let now = Instant::now();
                inner.session.as_mut().and_then(|session| {
                    if now >= session.next_owner_renewal {
                        session.next_owner_renewal = now + OWNER_RENEW_INTERVAL;
                        Some(ControlSession::from(&*session))
                    } else {
                        None
                    }
                })
            };
            if let Some(control) = renewal {
                host.finish_owner_renewal(control);
                continue;
            }
            let should_restart = {
                let mut inner = host.lock();
                refresh_locked(&mut inner);
                reset_stable_recovery_budget(&mut inner);
                let retry_due = inner
                    .next_restart_at
                    .map(|deadline| Instant::now() >= deadline)
                    .unwrap_or(true);
                let should = inner.desired
                    && !inner.stopping
                    && inner.session.is_none()
                    && inner.status.state != "starting"
                    && inner.recovery_budget_used < MAX_RECOVERY_BUDGET
                    && retry_due;
                if should {
                    inner.status.state = "starting".into();
                    inner.status.recovering = true;
                } else if inner.recovery_budget_used >= MAX_RECOVERY_BUDGET {
                    inner.status.recovering = false;
                }
                should
            };
            if should_restart {
                let _ = host.finish_start(true);
            }
        });
    }

    #[cfg(unix)]
    fn finish_owner_renewal(&self, control: ControlSession) {
        let result = renew_owner_and_status(&control);
        let mut inner = self.lock();
        refresh_locked(&mut inner);
        let same_session = inner
            .session
            .as_ref()
            .map(|session| session.instance_epoch.as_str())
            == Some(control.instance_epoch.as_str());
        if !same_session {
            return;
        }
        match result {
            Ok(status) if owner_lease_valid(&status) => {
                if let Some(session) = inner.session.as_mut() {
                    session.next_owner_renewal = Instant::now() + OWNER_RENEW_INTERVAL;
                }
                inner.status = status_with_supervisor(status, &inner.status);
            }
            Ok(_) | Err(_) => {
                let mut session = inner.session.take();
                mark_runtime_unavailable(&mut inner);
                drop(inner);
                if let Some(session) = session.as_mut() {
                    terminate_session(session);
                }
            }
        }
    }

    #[cfg(unix)]
    fn spawn_oauth_refresh_worker(host: Weak<Self>, control: ControlSession) {
        let worker_session = (control.instance_epoch.clone(), control.owner_term);
        let fallback_host = host.clone();
        let cleanup_host = host.clone();
        let cleanup_session = worker_session.clone();
        let spawn = std::thread::Builder::new()
            .name("agenthub-go-oauth-refresh".into())
            .spawn(move || {
                oauth_refresh_worker(host, control);
                let Some(host) = cleanup_host.upgrade() else {
                    return;
                };
                let mut inner = host.lock();
                if inner.oauth_refresh_worker_session.as_ref() == Some(&cleanup_session) {
                    inner.oauth_refresh_worker_session = None;
                }
            });
        if spawn.is_err() {
            let Some(host) = fallback_host.upgrade() else {
                return;
            };
            let mut inner = host.lock();
            if inner.oauth_refresh_worker_session.as_ref() == Some(&worker_session) {
                inner.oauth_refresh_worker_session = None;
            }
            tracing::warn!(
                target: "core.adapter",
                code = "go.route.oauth_refresh.worker_start_failed",
                count = 1_u64,
                "Go route OAuth refresh worker could not start"
            );
        }
    }
}

#[cfg(unix)]
struct OAuthRefreshEvent {
    refresh_id: String,
    instance_epoch: String,
    owner_term: i64,
    active_hash: String,
    edge_id: String,
    member_id: String,
    source_kind: String,
    source_id: String,
    refresh_kind: String,
}

#[cfg(unix)]
enum OAuthRefreshCompletion {
    ConfigApplied(String),
    NotRefreshed,
}

#[cfg(unix)]
#[derive(Default)]
struct OAuthRefreshWorkerCounters {
    completed: AtomicU64,
    rejected: AtomicU64,
}

/// One refresh action per saved login and Go configuration generation.
///
/// Go coalesces identical member attempts, but the same saved login may appear
/// through multiple edges or member ids. Successful gates retain the completed
/// action for late events from the same generation. Failed waves wake their
/// current waiters without sealing the key, so a later event may retry. The
/// registry evicts its oldest idle gates under bounded-memory pressure.
#[cfg(unix)]
#[derive(Clone, PartialEq, Eq, Hash)]
struct OAuthRefreshActionKey {
    instance_epoch: String,
    owner_term: i64,
    active_hash: String,
    source_kind: String,
    source_id: String,
    refresh_kind: String,
}

#[cfg(unix)]
impl From<&OAuthRefreshEvent> for OAuthRefreshActionKey {
    fn from(event: &OAuthRefreshEvent) -> Self {
        Self {
            instance_epoch: event.instance_epoch.clone(),
            owner_term: event.owner_term,
            active_hash: event.active_hash.clone(),
            source_kind: event.source_kind.clone(),
            source_id: event.source_id.clone(),
            refresh_kind: event.refresh_kind.clone(),
        }
    }
}

#[cfg(unix)]
#[derive(Default)]
struct OAuthRefreshActionState {
    running: bool,
    sealed: bool,
    generation: u64,
}

#[cfg(unix)]
#[derive(Default)]
struct OAuthRefreshActionGate {
    state: Mutex<OAuthRefreshActionState>,
    completed: Condvar,
}

#[cfg(unix)]
#[derive(Default)]
struct OAuthRefreshActionRegistry {
    gates: HashMap<OAuthRefreshActionKey, Arc<OAuthRefreshActionGate>>,
    fifo: VecDeque<OAuthRefreshActionKey>,
}

#[cfg(unix)]
#[derive(Default)]
struct OAuthRefreshActionCoordinator {
    registry: Arc<Mutex<OAuthRefreshActionRegistry>>,
}

#[cfg(unix)]
enum OAuthRefreshActionRole {
    Sealed,
    Wait(u64),
    Lead(u64),
}

#[cfg(unix)]
impl OAuthRefreshActionCoordinator {
    fn run_once(
        &self,
        key: OAuthRefreshActionKey,
        host: &Weak<GoRouteIsolatedHost>,
        control: &ControlSession,
        action: impl FnOnce(),
    ) -> bool {
        let active_hash = key.active_hash.clone();
        let (gate, role) = {
            let mut registry = self
                .registry
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let gate = if let Some(gate) = registry.gates.get(&key) {
                Arc::clone(gate)
            } else {
                let gate = Arc::new(OAuthRefreshActionGate::default());
                registry.fifo.push_back(key.clone());
                registry.gates.insert(key, Arc::clone(&gate));
                gate
            };
            let role = {
                let mut state = gate
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if state.sealed {
                    OAuthRefreshActionRole::Sealed
                } else if state.running {
                    OAuthRefreshActionRole::Wait(state.generation)
                } else {
                    state.running = true;
                    state.generation = state.generation.wrapping_add(1);
                    OAuthRefreshActionRole::Lead(state.generation)
                }
            };
            (gate, role)
        };

        prune_oauth_refresh_action_gates(&self.registry);
        match role {
            OAuthRefreshActionRole::Sealed => true,
            OAuthRefreshActionRole::Lead(generation) => {
                let mut leader = OAuthRefreshActionLeader {
                    gate,
                    registry: Arc::clone(&self.registry),
                    generation,
                    finished: false,
                };
                action();
                let sealed = host
                    .upgrade()
                    .and_then(|host| current_committed_hash(&host, control))
                    .is_some_and(|committed_hash| committed_hash != active_hash);
                leader.finish(sealed);
                true
            }
            OAuthRefreshActionRole::Wait(generation) => loop {
                let state = gate
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if !(state.running && state.generation == generation) {
                    return true;
                }
                let (state_after_wait, _) = gate
                    .completed
                    .wait_timeout(state, Duration::from_millis(100))
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                drop(state_after_wait);
                let Some(host) = host.upgrade() else {
                    return false;
                };
                if current_committed_hash(&host, control).is_none() {
                    return false;
                }
            },
        }
    }
}

#[cfg(unix)]
fn prune_oauth_refresh_action_gates(registry: &Arc<Mutex<OAuthRefreshActionRegistry>>) {
    let mut registry = registry
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    while registry.gates.len() > OAUTH_REFRESH_ACTION_GATE_LIMIT {
        let removable = registry.fifo.iter().position(|key| {
            registry.gates.get(key).is_some_and(|gate| {
                !gate
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .running
            })
        });
        let Some(index) = removable else {
            break;
        };
        let Some(key) = registry.fifo.remove(index) else {
            break;
        };
        registry.gates.remove(&key);
    }
}

#[cfg(unix)]
struct OAuthRefreshActionLeader {
    gate: Arc<OAuthRefreshActionGate>,
    registry: Arc<Mutex<OAuthRefreshActionRegistry>>,
    generation: u64,
    finished: bool,
}

#[cfg(unix)]
impl OAuthRefreshActionLeader {
    fn finish(&mut self, sealed: bool) {
        {
            let mut state = self
                .gate
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.running && state.generation == self.generation {
                state.running = false;
                state.sealed = sealed;
            }
            self.finished = true;
        }
        self.gate.completed.notify_all();
        prune_oauth_refresh_action_gates(&self.registry);
    }
}

#[cfg(unix)]
impl Drop for OAuthRefreshActionLeader {
    fn drop(&mut self) {
        if !self.finished {
            self.finish(false);
        }
    }
}

#[cfg(unix)]
#[derive(Default)]
struct OAuthRefreshQueueState {
    events: VecDeque<OAuthRefreshEvent>,
    active_refresh_ids: HashSet<String>,
    completed_refresh_ids: HashSet<String>,
    completed_refresh_fifo: VecDeque<(Instant, String)>,
    stopped: bool,
}

#[cfg(unix)]
impl OAuthRefreshQueueState {
    fn prune_completed_refresh_ids(&mut self, now: Instant) {
        loop {
            let expired = self
                .completed_refresh_fifo
                .front()
                .is_some_and(|(completed_at, _)| {
                    now.saturating_duration_since(*completed_at) >= OAUTH_REFRESH_COMPLETED_ID_TTL
                });
            if !expired && self.completed_refresh_ids.len() <= OAUTH_REFRESH_COMPLETED_ID_LIMIT {
                break;
            }
            let Some((_, refresh_id)) = self.completed_refresh_fifo.pop_front() else {
                break;
            };
            self.completed_refresh_ids.remove(&refresh_id);
        }
    }
}

#[cfg(unix)]
#[derive(Default)]
struct OAuthRefreshWorkQueue {
    state: Mutex<OAuthRefreshQueueState>,
    event_ready: Condvar,
    space_ready: Condvar,
}

#[cfg(unix)]
impl OAuthRefreshWorkQueue {
    fn push_for_session(
        &self,
        host: &Weak<GoRouteIsolatedHost>,
        control: &ControlSession,
        event: OAuthRefreshEvent,
    ) -> bool {
        let mut event = Some(event);
        loop {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.stopped {
                return false;
            }
            state.prune_completed_refresh_ids(Instant::now());
            let refresh_id = &event.as_ref().expect("event retained").refresh_id;
            if state.active_refresh_ids.contains(refresh_id)
                || state.completed_refresh_ids.contains(refresh_id)
            {
                return true;
            }
            if state.events.len() < OAUTH_REFRESH_QUEUE_LIMIT {
                let Some(event) = event.take() else {
                    return false;
                };
                state.active_refresh_ids.insert(event.refresh_id.clone());
                state.events.push_back(event);
                self.event_ready.notify_one();
                return true;
            }
            let (state_after_wait, _) = self
                .space_ready
                .wait_timeout(state, Duration::from_millis(100))
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            drop(state_after_wait);
            let Some(host) = host.upgrade() else {
                return false;
            };
            if current_committed_hash(&host, control).is_none() {
                return false;
            }
        }
    }

    fn finish(&self, refresh_id: &str) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();
        if state.active_refresh_ids.remove(refresh_id)
            && state.completed_refresh_ids.insert(refresh_id.to_owned())
        {
            state
                .completed_refresh_fifo
                .push_back((now, refresh_id.to_owned()));
        }
        state.prune_completed_refresh_ids(now);
        self.space_ready.notify_one();
    }

    fn pop_for_session(
        &self,
        host: &Weak<GoRouteIsolatedHost>,
        control: &ControlSession,
    ) -> Option<OAuthRefreshEvent> {
        loop {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(event) = state.events.pop_front() {
                self.space_ready.notify_one();
                return Some(event);
            }
            if state.stopped {
                return None;
            }
            let (state_after_wait, _) = self
                .event_ready
                .wait_timeout(state, Duration::from_millis(100))
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            drop(state_after_wait);
            let Some(host) = host.upgrade() else {
                return None;
            };
            if current_committed_hash(&host, control).is_none() {
                return None;
            }
        }
    }

    fn stop(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.stopped = true;
        self.event_ready.notify_all();
        self.space_ready.notify_all();
    }
}

#[cfg(unix)]
fn oauth_refresh_worker(host: Weak<GoRouteIsolatedHost>, control: ControlSession) {
    let queue = Arc::new(OAuthRefreshWorkQueue::default());
    let counters = Arc::new(OAuthRefreshWorkerCounters::default());
    let actions = Arc::new(OAuthRefreshActionCoordinator::default());
    let mut handlers = Vec::with_capacity(OAUTH_REFRESH_HANDLER_LIMIT);
    for index in 0..OAUTH_REFRESH_HANDLER_LIMIT {
        let handler_host = host.clone();
        let handler_control = control.clone();
        let handler_queue = Arc::clone(&queue);
        let handler_counters = Arc::clone(&counters);
        let handler_actions = Arc::clone(&actions);
        let spawn = std::thread::Builder::new()
            .name(format!("agenthub-go-oauth-refresh-{index}"))
            .spawn(move || {
                while let Some(event) =
                    handler_queue.pop_for_session(&handler_host, &handler_control)
                {
                    let refresh_id = event.refresh_id.clone();
                    process_oauth_refresh_event(
                        &handler_host,
                        &handler_control,
                        event,
                        &handler_counters,
                        &handler_actions,
                    );
                    handler_queue.finish(&refresh_id);
                }
            });
        match spawn {
            Ok(handler) => handlers.push(handler),
            Err(_) => {
                let count = counters.rejected.fetch_add(1, Ordering::Relaxed) + 1;
                tracing::warn!(
                    target: "core.adapter",
                    code = "go.route.oauth_refresh.handler_start_failed",
                    count,
                    "Go route OAuth refresh handler could not start"
                );
            }
        }
    }
    if handlers.is_empty() {
        return;
    }

    loop {
        let Some(host_ref) = host.upgrade() else {
            break;
        };
        if current_committed_hash(&host_ref, &control).is_none() {
            break;
        }
        drop(host_ref);

        let event = match next_oauth_refresh(&control) {
            Ok(Some(event)) => event,
            Ok(None) => continue,
            Err(NextOAuthRefreshError::Transport) => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(NextOAuthRefreshError::InvalidEvent) => {
                let count = counters.rejected.fetch_add(1, Ordering::Relaxed) + 1;
                tracing::warn!(
                    target: "core.adapter",
                    code = "go.route.oauth_refresh.invalid_event",
                    count,
                    "Go route OAuth refresh event was rejected"
                );
                break;
            }
        };
        if !queue.push_for_session(&host, &control, event) {
            break;
        }
    }

    queue.stop();
    for handler in handlers {
        let _ = handler.join();
    }
}

#[cfg(unix)]
fn process_oauth_refresh_event(
    host: &Weak<GoRouteIsolatedHost>,
    control: &ControlSession,
    event: OAuthRefreshEvent,
    counters: &OAuthRefreshWorkerCounters,
    actions: &OAuthRefreshActionCoordinator,
) {
    let Some(host_ref) = host.upgrade() else {
        return;
    };
    let event_identity_is_current = event.instance_epoch == control.instance_epoch
        && event.owner_term == control.owner_term
        && event.source_kind == "account";
    if !event_identity_is_current {
        let count = counters.rejected.fetch_add(1, Ordering::Relaxed) + 1;
        tracing::warn!(
            target: "core.adapter",
            code = "go.route.oauth_refresh.identity_rejected",
            count,
            "Go route OAuth refresh event identity was rejected"
        );
        return;
    }
    let event_is_current =
        current_committed_hash(&host_ref, control).as_deref() == Some(event.active_hash.as_str());

    if event_is_current {
        if let Some(hub) = host_ref.hub.as_ref().map(Arc::clone) {
            let account_id = hub.adapter_bridge().resolve_go_route_oauth_refresh(
                &event.edge_id,
                &event.member_id,
                &event.source_id,
                &event.refresh_kind,
            );
            match account_id {
                Ok(account_id) => {
                    let key = OAuthRefreshActionKey::from(&event);
                    let action_completed = actions.run_once(key, host, control, || {
                        if hub
                            .accounts()
                            .reload_oauth_upstream_access(&account_id)
                            .is_err()
                        {
                            let count = counters.rejected.fetch_add(1, Ordering::Relaxed) + 1;
                            tracing::warn!(
                                target: "core.adapter",
                                code = "go.route.oauth_refresh.reload_failed",
                                count,
                                "Go route OAuth refresh did not complete"
                            );
                        }
                    });
                    if !action_completed {
                        return;
                    }
                }
                Err(_) => {
                    let count = counters.rejected.fetch_add(1, Ordering::Relaxed) + 1;
                    tracing::warn!(
                        target: "core.adapter",
                        code = "go.route.oauth_refresh.resolve_rejected",
                        count,
                        "Go route OAuth refresh event was rejected"
                    );
                }
            }
        }
    } else {
        let count = counters.rejected.fetch_add(1, Ordering::Relaxed) + 1;
        tracing::warn!(
            target: "core.adapter",
            code = "go.route.oauth_refresh.stale_event",
            count,
            "Stale Go route OAuth refresh event was rejected"
        );
    }
    drop(host_ref);

    loop {
        let Some(host_ref) = host.upgrade() else {
            return;
        };
        let Some(current_hash) = current_committed_hash(&host_ref, control) else {
            return;
        };
        let completion = if current_hash != event.active_hash {
            OAuthRefreshCompletion::ConfigApplied(current_hash)
        } else {
            OAuthRefreshCompletion::NotRefreshed
        };
        drop(host_ref);

        let completion_request_id = request_id("complete-oauth-refresh");
        loop {
            let Some(host_ref) = host.upgrade() else {
                return;
            };
            if current_committed_hash(&host_ref, control).is_none() {
                return;
            }
            drop(host_ref);

            match complete_oauth_refresh(control, &event, &completion, &completion_request_id) {
                Ok(()) => {
                    let count = counters.completed.fetch_add(1, Ordering::Relaxed) + 1;
                    let code = match completion {
                        OAuthRefreshCompletion::ConfigApplied(_) => {
                            "go.route.oauth_refresh.config_applied"
                        }
                        OAuthRefreshCompletion::NotRefreshed => {
                            "go.route.oauth_refresh.not_refreshed"
                        }
                    };
                    tracing::info!(
                        target: "core.adapter",
                        code,
                        count,
                        "Go route OAuth refresh event completed"
                    );
                    return;
                }
                Err(CompleteOAuthRefreshError::Transport) => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(CompleteOAuthRefreshError::Rejected) => {
                    let count = counters.rejected.fetch_add(1, Ordering::Relaxed) + 1;
                    tracing::warn!(
                        target: "core.adapter",
                        code = "go.route.oauth_refresh.completion_rejected",
                        count,
                        "Go route OAuth refresh completion was rejected"
                    );
                    let Some(host_ref) = host.upgrade() else {
                        return;
                    };
                    let Some(latest_hash) = current_committed_hash(&host_ref, control) else {
                        return;
                    };
                    let completion_was_superseded = match &completion {
                        OAuthRefreshCompletion::ConfigApplied(attempted_hash) => {
                            &latest_hash != attempted_hash
                        }
                        OAuthRefreshCompletion::NotRefreshed => latest_hash != event.active_hash,
                    };
                    if !completion_was_superseded {
                        return;
                    }
                    break;
                }
            }
        }
    }
}

#[cfg(unix)]
fn current_committed_hash(host: &GoRouteIsolatedHost, control: &ControlSession) -> Option<String> {
    let inner = host.lock();
    let session = inner.session.as_ref()?;
    if !inner.desired
        || inner.stopping
        || session.instance_epoch != control.instance_epoch
        || session.owner_term != control.owner_term
    {
        return None;
    }
    inner
        .committed_plan
        .as_ref()
        .map(|plan| plan.config_hash.clone())
}

#[cfg(unix)]
enum NextOAuthRefreshError {
    Transport,
    InvalidEvent,
}

#[cfg(unix)]
fn next_oauth_refresh(
    control: &ControlSession,
) -> Result<Option<OAuthRefreshEvent>, NextOAuthRefreshError> {
    let reply = post_control_with_timeout(
        &control.socket,
        &json!({
            "type": "NextOAuthRefresh",
            "request_id": request_id("next-oauth-refresh"),
            "instance_epoch": control.instance_epoch,
            "owner_id": OWNER_ID,
            "owner_term": control.owner_term,
            "app_data_dir": control.home.display().to_string(),
            "payload": { "wait_ms": OAUTH_REFRESH_POLL_WAIT_MS },
        }),
        OAUTH_REFRESH_CONTROL_TIMEOUT,
    )
    .map_err(|_| NextOAuthRefreshError::Transport)?;
    let payload = require_ok(&reply).map_err(|_| NextOAuthRefreshError::Transport)?;
    let Some(event) = payload.get("event") else {
        return Err(NextOAuthRefreshError::InvalidEvent);
    };
    if event.is_null() {
        return Ok(None);
    }
    parse_oauth_refresh_event(event)
        .map(Some)
        .ok_or(NextOAuthRefreshError::InvalidEvent)
}

#[cfg(unix)]
fn parse_oauth_refresh_event(value: &Value) -> Option<OAuthRefreshEvent> {
    fn required_string(value: &Value, key: &str) -> Option<String> {
        let value = value.get(key)?.as_str()?;
        (!value.is_empty() && value.len() <= 512 && value.trim() == value).then(|| value.to_owned())
    }
    fn required_hash(value: &Value, key: &str) -> Option<String> {
        let value = required_string(value, key)?;
        (value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(value)
    }

    Some(OAuthRefreshEvent {
        refresh_id: required_string(value, "refresh_id")?,
        instance_epoch: required_string(value, "instance_epoch")?,
        owner_term: value.get("owner_term")?.as_i64()?,
        active_hash: required_hash(value, "active_hash")?,
        edge_id: required_string(value, "edge_id")?,
        member_id: required_string(value, "member_id")?,
        source_kind: required_string(value, "source_kind")?,
        source_id: required_string(value, "source_id")?,
        refresh_kind: required_string(value, "refresh_kind")?,
    })
}

#[cfg(unix)]
fn complete_oauth_refresh(
    control: &ControlSession,
    event: &OAuthRefreshEvent,
    completion: &OAuthRefreshCompletion,
    completion_request_id: &str,
) -> Result<(), CompleteOAuthRefreshError> {
    let (outcome, applied_active_hash) = match completion {
        OAuthRefreshCompletion::ConfigApplied(hash) => ("config_applied", hash.as_str()),
        OAuthRefreshCompletion::NotRefreshed => ("not_refreshed", ""),
    };
    let reply = post_control_with_timeout(
        &control.socket,
        &json!({
            "type": "CompleteOAuthRefresh",
            "request_id": completion_request_id,
            "instance_epoch": control.instance_epoch,
            "owner_id": OWNER_ID,
            "owner_term": control.owner_term,
            "app_data_dir": control.home.display().to_string(),
            "payload": {
                "refresh_id": event.refresh_id,
                "active_hash": event.active_hash,
                "edge_id": event.edge_id,
                "member_id": event.member_id,
                "source_kind": event.source_kind,
                "source_id": event.source_id,
                "refresh_kind": event.refresh_kind,
                "applied_active_hash": applied_active_hash,
                "outcome": outcome,
            },
        }),
        OAUTH_REFRESH_CONTROL_TIMEOUT,
    )
    .map_err(|_| CompleteOAuthRefreshError::Transport)?;
    let payload = require_ok(&reply).map_err(|_| CompleteOAuthRefreshError::Rejected)?;
    if payload.get("completed").and_then(Value::as_bool) != Some(true)
        || payload
            .get("retry_eligible")
            .and_then(Value::as_bool)
            .is_none()
    {
        return Err(CompleteOAuthRefreshError::Rejected);
    }
    Ok(())
}

#[cfg(unix)]
enum CompleteOAuthRefreshError {
    Transport,
    Rejected,
}

#[cfg(not(unix))]
fn unix_only_failed() -> GoRouteIsolatedStatus {
    GoRouteIsolatedStatus {
        state: "failed".into(),
        listen_ready: false,
        port: None,
        last_error: Some("unix control only".into()),
        home: None,
        lifecycle: Some("failed".into()),
        in_flight_count: 0,
        member_count: 0,
        healthy_member_count: 0,
        recovering: false,
        restart_count: 0,
    }
}

#[cfg(unix)]
fn refresh_locked(inner: &mut Inner) {
    let Some(session) = inner.session.as_mut() else {
        return;
    };
    let adapterd_dead = match session.adapterd.try_wait() {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(_) => true,
    };
    if adapterd_dead {
        let mut session = inner.session.take().expect("session checked above");
        terminate_session(&mut session);
        inner.status.state = "failed".into();
        inner.status.listen_ready = false;
        inner.status.lifecycle = Some("failed".into());
        inner.status.last_error = Some("Go route process exited".into());
        inner.recovery_budget_used = inner.recovery_budget_used.saturating_add(1);
        inner.stable_since = None;
        inner.status.recovering =
            inner.desired && !inner.stopping && inner.recovery_budget_used < MAX_RECOVERY_BUDGET;
        inner.next_restart_at = Some(Instant::now() + restart_backoff(inner.recovery_budget_used));
        return;
    }
}

#[cfg(unix)]
fn mark_runtime_unavailable(inner: &mut Inner) {
    inner.recovery_budget_used = inner.recovery_budget_used.saturating_add(1);
    inner.stable_since = None;
    inner.status.state = "failed".into();
    inner.status.listen_ready = false;
    inner.status.lifecycle = Some("unavailable".into());
    inner.status.last_error = Some(ERROR_CONTROL_UNAVAILABLE.into());
    inner.status.recovering =
        inner.desired && !inner.stopping && inner.recovery_budget_used < MAX_RECOVERY_BUDGET;
    inner.next_restart_at = Some(Instant::now() + restart_backoff(inner.recovery_budget_used));
}

#[cfg(unix)]
fn reset_stable_recovery_budget(inner: &mut Inner) {
    let stable = inner
        .stable_since
        .is_some_and(|started| started.elapsed() >= STABLE_RUN_RESET);
    if stable {
        inner.recovery_budget_used = 0;
        inner.stable_since = None;
    }
}

#[cfg(unix)]
fn stop_session(session: &mut Session) {
    let started = Instant::now();
    let _ = post_control_with_timeout(
        &session.socket,
        &json!({
            "type": "Stop",
            "request_id": request_id("stop"),
            "instance_epoch": session.instance_epoch,
            "owner_id": OWNER_ID,
            "owner_term": session.owner_term,
            "app_data_dir": session.home.display().to_string(),
            "payload": {},
        }),
        Duration::from_secs(3),
    );
    while started.elapsed() < GRACEFUL_STOP_WAIT {
        match session.adapterd.try_wait() {
            Ok(Some(_)) => {
                cleanup_scratch_home(&session.home);
                return;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => break,
        }
    }
    terminate_session(session);
}

#[cfg(unix)]
fn terminate_session(session: &mut Session) {
    let _ = session.adapterd.kill();
    let _ = session.adapterd.wait();
    cleanup_scratch_home(&session.home);
}

#[cfg(unix)]
fn build_runtime_config(hub: &AgentHub) -> Result<Vec<u8>, String> {
    let pools = hub
        .route_pools()
        .list_gateway_listener_pools()
        .map_err(|error| error.to_string())?;
    hub.adapter_bridge()
        .build_go_route_isolated_config(&pools)
        .map_err(|error| error.to_string())
}

#[cfg(unix)]
fn build_runtime_plan(hub: &AgentHub) -> Result<RuntimePlan, String> {
    let config = build_runtime_config(hub)?;
    Ok(RuntimePlan {
        config_hash: sha256_hex(&config),
        config,
        port: pick_loopback_port()?,
    })
}

#[cfg(unix)]
fn start_session(plan: &RuntimePlan) -> Result<Session, String> {
    let home = create_scratch_home()?;
    let scratch_guard = ScratchHomeGuard {
        home: home.clone(),
        armed: true,
    };
    if is_forbidden_user_home(&home) {
        return Err("refusing real user AGENTHUB_HOME".into());
    }
    if !is_scratch_home(&home) {
        return Err("AGENTHUB_HOME must be an absolute scratch directory under /tmp".into());
    }

    if plan.port == PRODUCT_DEFAULT_PORT {
        return Err(format!(
            "refusing product default listen port {PRODUCT_DEFAULT_PORT}"
        ));
    }

    let scratch_root = home.parent().unwrap_or(home.as_path());
    let bin = resolve_adapterd_bin(scratch_root)?;
    let adapterd_log = home.join("logs/adapterd.stdout.log");
    let mut adapterd = spawn_logged(
        Command::new(&bin.path)
            .arg("run")
            .arg("--home")
            .arg(&home)
            .arg("--listen-port")
            .arg(plan.port.to_string())
            .arg("--runtime-config-stdin-stream")
            .stdin(Stdio::piped())
            .env("AGENTHUB_HOME", &home),
        &adapterd_log,
    )
    .map_err(|err| format!("adapterd spawn failed: {err}"))?;
    let Some(config_stdin) = adapterd.stdin.take() else {
        let _ = adapterd.kill();
        let _ = adapterd.wait();
        return Err("adapterd stdin unavailable".into());
    };
    let config_stdin = Arc::new(Mutex::new(config_stdin));
    if let Err(error) = write_runtime_config_with_timeout(
        Arc::clone(&config_stdin),
        plan.config.clone(),
        CONFIG_WRITE_WAIT,
    ) {
        let _ = adapterd.kill();
        let _ = adapterd.wait();
        return Err(format!("adapterd runtime config write failed: {error}"));
    }

    let socket = home.join("run/adapterd.sock");
    let started = match handshake_start(
        &home,
        &socket,
        plan.port,
        &plan.config_hash,
        bin.package_version,
    ) {
        Ok(session) => Session {
            home: session.home,
            socket: session.socket,
            owner_term: session.owner_term,
            instance_epoch: session.instance_epoch,
            port: session.port,
            next_owner_renewal: Instant::now() + OWNER_RENEW_INTERVAL,
            oauth_refresh_supported: session.oauth_refresh_supported,
            config_stdin,
            adapterd,
        },
        Err(err) => {
            let _ = adapterd.kill();
            let _ = adapterd.wait();
            return Err(err);
        }
    };
    scratch_guard.disarm();
    Ok(started)
}

#[cfg(unix)]
fn write_runtime_config_frame(writer: &mut impl Write, config: &[u8]) -> Result<(), String> {
    if config.is_empty() || config.len() > MAX_RUNTIME_CONFIG_BYTES {
        return Err("runtime config frame is invalid".into());
    }
    let length =
        u32::try_from(config.len()).map_err(|_| "runtime config frame is too large".to_string())?;
    writer
        .write_all(&length.to_be_bytes())
        .and_then(|_| writer.write_all(config))
        .and_then(|_| writer.flush())
        .map_err(|_| "runtime config frame write failed".to_string())
}

#[cfg(unix)]
fn write_runtime_config_with_timeout(
    writer: Arc<Mutex<ChildStdin>>,
    config: Vec<u8>,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let (sent, received) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("agenthub-go-config-write".into())
        .spawn(move || {
            let result = {
                let mut writer = writer
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                write_runtime_config_frame(&mut *writer, &config)
            };
            let _ = sent.send(result.map(|()| config));
        })
        .map_err(|_| "runtime config writer could not start".to_string())?;
    received
        .recv_timeout(timeout)
        .map_err(|_| "runtime config frame write timed out".to_string())?
}

#[cfg(unix)]
fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

struct HandshakeMeta {
    home: PathBuf,
    socket: PathBuf,
    owner_term: i64,
    instance_epoch: String,
    port: u16,
    oauth_refresh_supported: bool,
}

#[cfg(unix)]
fn handshake_start(
    home: &Path,
    socket: &Path,
    fallback_port: u16,
    expected_config_hash: &str,
    expected_package_version: &str,
) -> Result<HandshakeMeta, String> {
    wait_for_socket(socket, Duration::from_secs(8))?;
    let home_s = home.display().to_string();
    let hs = post_control(
        socket,
        &json!({
            "type": "Handshake",
            "request_id": request_id("hs"),
            "app_data_dir": home_s,
            "payload": {
                "protocol_version": PROTOCOL_VERSION,
                "config_format_version": CONFIG_FORMAT_VERSION,
                "package_version": expected_package_version,
                "app_data_dir": home_s,
            },
        }),
    )?;
    let hs_payload = require_ok(&hs)?;
    if hs_payload.get("package_version").and_then(Value::as_str) != Some(expected_package_version) {
        return Err("Go route package version mismatch".into());
    }
    let capabilities = hs_payload
        .get("capabilities")
        .and_then(Value::as_array)
        .ok_or_else(|| "handshake missing capabilities".to_string())?;
    let supports_config_stream = capabilities
        .iter()
        .any(|capability| capability.as_str() == Some(CONFIG_STREAM_CAPABILITY));
    if !supports_config_stream {
        return Err("Go route config stream is unavailable".into());
    }
    let oauth_refresh_supported = capabilities
        .iter()
        .any(|capability| capability.as_str() == Some(OAUTH_REFRESH_CAPABILITY));
    let instance_epoch = hs_payload
        .get("instance_epoch")
        .and_then(Value::as_str)
        .ok_or_else(|| "handshake missing instance_epoch".to_string())?
        .to_string();
    let acq = post_control(
        socket,
        &json!({
            "type": "AcquireOrRenewOwner",
            "request_id": request_id("acq"),
            "instance_epoch": instance_epoch,
            "owner_id": OWNER_ID,
            "app_data_dir": home_s,
            "payload": { "mode": "acquire", "lease_budget_ms": ISOLATED_LEASE_BUDGET_MS },
        }),
    )?;
    let acq_payload = require_ok(&acq)?;
    let owner_term = acq_payload
        .get("owner_term")
        .and_then(Value::as_i64)
        .ok_or_else(|| "acquire missing owner_term".to_string())?;
    let start = post_control(
        socket,
        &json!({
            "type": "Start",
            "request_id": request_id("start"),
            "instance_epoch": instance_epoch,
            "owner_id": OWNER_ID,
            "owner_term": owner_term,
            "app_data_dir": home_s,
            "payload": {},
        }),
    )?;
    let start_payload = require_ok(&start)?;
    let st = post_control(
        socket,
        &json!({
            "type": "Status",
            "request_id": request_id("st"),
            "instance_epoch": instance_epoch,
            "owner_id": OWNER_ID,
            "owner_term": owner_term,
            "app_data_dir": home_s,
            "payload": {},
        }),
    )?;
    let status_payload = require_ok(&st)?;
    if status_payload.get("listen_ready").and_then(Value::as_bool) != Some(true)
        || !owner_lease_valid(status_payload)
        || status_payload.get("active_hash").and_then(Value::as_str) != Some(expected_config_hash)
    {
        return Err(ERROR_START_FAILED.into());
    }
    let port = status_payload
        .get("port")
        .and_then(Value::as_u64)
        .map(|n| n as u16)
        .or_else(|| {
            start_payload
                .get("port")
                .and_then(Value::as_u64)
                .map(|n| n as u16)
        })
        .unwrap_or(fallback_port);
    if port == PRODUCT_DEFAULT_PORT {
        return Err(format!(
            "refusing product default listen port {PRODUCT_DEFAULT_PORT}"
        ));
    }
    Ok(HandshakeMeta {
        home: home.to_path_buf(),
        socket: socket.to_path_buf(),
        owner_term,
        instance_epoch,
        port,
        oauth_refresh_supported,
    })
}

#[cfg(unix)]
fn pick_loopback_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|err| err.to_string())?;
    let port = listener.local_addr().map_err(|err| err.to_string())?.port();
    drop(listener);
    if port == PRODUCT_DEFAULT_PORT {
        return Err(format!(
            "refusing product default listen port {PRODUCT_DEFAULT_PORT}"
        ));
    }
    Ok(port)
}

#[cfg(unix)]
fn create_scratch_home() -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;

    // Keep the Unix-domain socket comfortably below the macOS path limit.
    // The per-session directory is still created exclusively with mode 0700.
    let temp = fs::canonicalize("/tmp").map_err(|err| err.to_string())?;
    let mut created = None;
    for attempt in 0..16_u32 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let candidate = temp.join(format!(
            "{SCRATCH_ROOT_PREFIX}{}-{nanos}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => {
                if let Err(error) =
                    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o700))
                {
                    let _ = fs::remove_dir(&candidate);
                    return Err(error.to_string());
                }
                if let Err(error) = write_scratch_owner_marker(&candidate, nanos, attempt) {
                    let _ = fs::remove_dir_all(&candidate);
                    return Err(error);
                }
                created = Some(candidate);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    let root = created.ok_or_else(|| "could not create isolated runtime directory".to_string())?;
    let home = root.join("home");
    if let Err(error) = (|| {
        fs::create_dir(&home)?;
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
        fs::create_dir(home.join("config"))?;
        fs::set_permissions(home.join("config"), fs::Permissions::from_mode(0o700))?;
        fs::create_dir(home.join("run"))?;
        fs::set_permissions(home.join("run"), fs::Permissions::from_mode(0o700))?;
        fs::create_dir(home.join("logs"))?;
        fs::set_permissions(home.join("logs"), fs::Permissions::from_mode(0o700))?;
        Ok::<(), std::io::Error>(())
    })() {
        let _ = fs::remove_dir_all(&root);
        return Err(error.to_string());
    }
    let home = match fs::canonicalize(&home) {
        Ok(home) => home,
        Err(error) => {
            let _ = fs::remove_dir_all(&root);
            return Err(error.to_string());
        }
    };
    if !is_scratch_home(&home) {
        cleanup_scratch_home(&home);
        return Err("scratch home escaped the operating-system temp directory".into());
    }
    Ok(home)
}

#[cfg(unix)]
fn current_euid() -> u32 {
    // SAFETY: geteuid has no preconditions and does not dereference pointers.
    unsafe { libc::geteuid() }
}

#[cfg(unix)]
fn scratch_marker_contents(pid: u32, nanos: u128, attempt: u32) -> String {
    format!(
        "agenthub-go-route-owner-v1\npid={pid}\ncreated_unix_nanos={nanos}\nattempt={attempt}\n"
    )
}

#[cfg(unix)]
fn write_scratch_owner_marker(root: &Path, nanos: u128, attempt: u32) -> Result<(), String> {
    let marker = root.join(SCRATCH_OWNER_MARKER);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&marker)
        .map_err(|error| error.to_string())?;
    file.write_all(scratch_marker_contents(std::process::id(), nanos, attempt).as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| error.to_string())
}

#[cfg(unix)]
fn parse_scratch_root_name(name: &str) -> Option<(u32, u128, u32)> {
    let mut parts = name.strip_prefix(SCRATCH_ROOT_PREFIX)?.split('-');
    let pid = parts.next()?.parse::<u32>().ok()?;
    let nanos = parts.next()?.parse::<u128>().ok()?;
    let attempt = parts.next()?.parse::<u32>().ok()?;
    if pid == 0 || pid > i32::MAX as u32 || parts.next().is_some() {
        return None;
    }
    Some((pid, nanos, attempt))
}

#[cfg(unix)]
fn validate_scratch_root(root: &Path) -> Option<(u32, SystemTime)> {
    let temp = fs::canonicalize("/tmp").ok()?;
    if root.parent() != Some(temp.as_path()) {
        return None;
    }
    let name = root.file_name()?.to_str()?;
    let (pid, nanos, attempt) = parse_scratch_root_name(name)?;
    let metadata = fs::symlink_metadata(root).ok()?;
    if !metadata.file_type().is_dir()
        || metadata.uid() != current_euid()
        || metadata.mode() & 0o777 != 0o700
    {
        return None;
    }
    let marker_path = root.join(SCRATCH_OWNER_MARKER);
    let mut marker = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(marker_path)
        .ok()?;
    let marker_metadata = marker.metadata().ok()?;
    if !marker_metadata.file_type().is_file()
        || marker_metadata.uid() != current_euid()
        || marker_metadata.mode() & 0o777 != 0o600
        || marker_metadata.nlink() != 1
        || marker_metadata.len() > 256
    {
        return None;
    }
    let mut contents = String::new();
    marker.read_to_string(&mut contents).ok()?;
    if contents != scratch_marker_contents(pid, nanos, attempt) {
        return None;
    }
    Some((pid, marker_metadata.modified().ok()?))
}

#[cfg(unix)]
fn scratch_root_for_home(path: &Path) -> Option<PathBuf> {
    let resolved = fs::canonicalize(path).ok()?;
    if resolved.file_name().and_then(|name| name.to_str()) != Some("home") {
        return None;
    }
    let root = resolved.parent()?.to_path_buf();
    validate_scratch_root(&root)?;
    Some(root)
}

#[cfg(unix)]
fn is_scratch_home(path: &Path) -> bool {
    scratch_root_for_home(path).is_some()
}

#[cfg(unix)]
fn is_forbidden_user_home(path: &Path) -> bool {
    let Ok(user_home) = std::env::var("HOME") else {
        return false;
    };
    let real = PathBuf::from(user_home).join(".agenthub");
    let real = fs::canonicalize(&real).unwrap_or(real);
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    resolved == real || resolved.starts_with(&real)
}

#[cfg(unix)]
fn cleanup_scratch_home(home: &Path) {
    let Some(root) = scratch_root_for_home(home) else {
        return;
    };
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    // SAFETY: kill(pid, 0) performs permission/existence checking only.
    let result = unsafe { libc::kill(pid as i32, 0) };
    if result == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(unix)]
fn cleanup_stale_scratch_roots() {
    let Ok(temp) = fs::canonicalize("/tmp") else {
        return;
    };
    let Ok(entries) = fs::read_dir(&temp) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let root = entry.path();
        let Some((pid, created)) = validate_scratch_root(&root) else {
            continue;
        };
        let old_enough = now
            .duration_since(created)
            .is_ok_and(|age| age >= STALE_SCRATCH_MIN_AGE);
        if !old_enough || process_is_alive(pid) {
            continue;
        }
        // Revalidate immediately before removal; remove_dir_all on Unix does
        // not follow a root symlink, and only our exact marker/name contract
        // is eligible.
        if validate_scratch_root(&root).is_some() {
            let _ = fs::remove_dir_all(root);
        }
    }
}

#[cfg(unix)]
fn sha256_file(file: &mut fs::File) -> Result<String, String> {
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "bundled Go route could not be verified".to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| "bundled Go route could not be verified".to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "bundled Go route could not be verified".to_string())?;
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(unix)]
fn bundled_adapterd(scratch_root: &Path) -> Result<ResolvedAdapterd, String> {
    use std::os::unix::fs::PermissionsExt;

    if BUNDLED_SHA256.len() != 64 || EMBEDDED_BUNDLED_VERSION != BUNDLED_PACKAGE_VERSION {
        return Err("bundled Go route identity is unavailable".into());
    }
    let exe = std::env::current_exe()
        .map_err(|_| "bundled Go route location is unavailable".to_string())?;
    let directory = exe
        .parent()
        .ok_or_else(|| "bundled Go route location is unavailable".to_string())?;
    let source_path = directory.join("agenthub-adapterd");
    let mut source = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&source_path)
        .map_err(|_| "bundled Go route is unavailable".to_string())?;
    let source_metadata = source
        .metadata()
        .map_err(|_| "bundled Go route is unavailable".to_string())?;
    let source_uid = source_metadata.uid();
    if !source_metadata.file_type().is_file()
        || (source_uid != current_euid() && source_uid != 0)
        || source_metadata.mode() & 0o022 != 0
    {
        return Err("bundled Go route is unavailable".into());
    }
    if sha256_file(&mut source)? != BUNDLED_SHA256 {
        return Err("bundled Go route integrity check failed".into());
    }

    let dest_dir = scratch_root.join("bin");
    fs::create_dir(&dest_dir)
        .and_then(|()| fs::set_permissions(&dest_dir, fs::Permissions::from_mode(0o700)))
        .map_err(|_| "bundled Go route staging failed".to_string())?;
    let dest_path = dest_dir.join("agenthub-adapterd");
    let mut dest = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o500)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&dest_path)
        .map_err(|_| "bundled Go route staging failed".to_string())?;
    std::io::copy(&mut source, &mut dest)
        .and_then(|_| dest.sync_all())
        .and_then(|()| dest.set_permissions(fs::Permissions::from_mode(0o500)))
        .map_err(|_| "bundled Go route staging failed".to_string())?;
    let dest_metadata = dest
        .metadata()
        .map_err(|_| "bundled Go route staging failed".to_string())?;
    if !dest_metadata.file_type().is_file()
        || dest_metadata.uid() != current_euid()
        || dest_metadata.mode() & 0o777 != 0o500
        || dest_metadata.nlink() != 1
        || sha256_file(&mut dest)? != BUNDLED_SHA256
    {
        return Err("bundled Go route staged integrity check failed".into());
    }
    drop(dest);
    drop(source);
    Ok(ResolvedAdapterd {
        path: dest_path,
        package_version: BUNDLED_PACKAGE_VERSION,
    })
}

#[cfg(unix)]
fn resolve_adapterd_bin(_scratch_root: &Path) -> Result<ResolvedAdapterd, String> {
    #[cfg(debug_assertions)]
    if let Ok(raw) = std::env::var("AGENTHUB_ADAPTERD_BIN") {
        let path = PathBuf::from(raw);
        if path.is_file() {
            return Ok(ResolvedAdapterd {
                path,
                package_version: ISOLATED_DEV_PACKAGE_VERSION,
            });
        }
    }

    if BUNDLED_SHA256.len() == 64 {
        #[cfg(not(debug_assertions))]
        return bundled_adapterd(_scratch_root);
        #[cfg(debug_assertions)]
        if let Ok(bundled) = bundled_adapterd(_scratch_root) {
            return Ok(bundled);
        }
    }

    #[cfg(not(debug_assertions))]
    return Err("bundled Go route identity is unavailable".into());

    #[cfg(debug_assertions)]
    {
        let src = find_adapterd_src()?;
        let dest_dir = _scratch_root.join("bin");
        fs::create_dir_all(&dest_dir).map_err(|err| err.to_string())?;
        let dest = dest_dir.join("agenthub-adapterd");
        let status = Command::new("go")
            .arg("build")
            .arg("-trimpath")
            .arg("-buildvcs=false")
            .arg("-o")
            .arg(&dest)
            .current_dir(&src)
            .status()
            .map_err(|err| format!("go build: {err}"))?;
        if !status.success() {
            return Err("go build agenthub-adapterd failed".into());
        }
        Ok(ResolvedAdapterd {
            path: dest,
            package_version: ISOLATED_DEV_PACKAGE_VERSION,
        })
    }
}

#[cfg(all(unix, debug_assertions))]
fn find_adapterd_src() -> Result<PathBuf, String> {
    let mut candidates =
        vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../go/agenthub-adapterd")];
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("go/agenthub-adapterd"));
        candidates.push(cwd.join("../go/agenthub-adapterd"));
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(Path::to_path_buf);
        for _ in 0..8 {
            let Some(current) = dir else { break };
            candidates.push(current.join("go/agenthub-adapterd"));
            dir = current.parent().map(Path::to_path_buf);
        }
    }
    for candidate in candidates {
        if candidate.join("go.mod").is_file() {
            return fs::canonicalize(candidate).map_err(|err| err.to_string());
        }
    }
    Err("go/agenthub-adapterd source not found".into())
}

#[cfg(unix)]
fn spawn_logged(cmd: &mut Command, log_path: &Path) -> Result<Child, String> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|err| err.to_string())?;
    let err_file = file.try_clone().map_err(|err| err.to_string())?;
    cmd.stdout(Stdio::from(file))
        .stderr(Stdio::from(err_file))
        .spawn()
        .map_err(|err| err.to_string())
}

#[cfg(unix)]
fn wait_for_socket(path: &Path, timeout: Duration) -> Result<(), String> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if path.exists() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err("timed out waiting for control socket".into())
}

#[cfg(unix)]
fn post_control(socket: &Path, body: &Value) -> Result<Value, String> {
    post_control_with_timeout(socket, body, Duration::from_secs(8))
}

#[cfg(unix)]
fn post_control_with_timeout(
    socket: &Path,
    body: &Value,
    timeout: Duration,
) -> Result<Value, String> {
    use std::os::unix::net::UnixStream;
    let raw = serde_json::to_vec(body).map_err(|err| err.to_string())?;
    let started = Instant::now();
    let mut stream = loop {
        match UnixStream::connect(socket) {
            Ok(stream) => break stream,
            Err(err) => {
                if started.elapsed() >= timeout {
                    return Err(err.to_string());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    };
    let remaining = timeout
        .checked_sub(started.elapsed())
        .filter(|remaining| !remaining.is_zero())
        .unwrap_or(Duration::from_millis(1));
    stream
        .set_read_timeout(Some(remaining))
        .map_err(|err| err.to_string())?;
    stream
        .set_write_timeout(Some(remaining))
        .map_err(|err| err.to_string())?;
    let header = format!(
        "POST /control HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        raw.len()
    );
    stream
        .write_all(header.as_bytes())
        .map_err(|err| err.to_string())?;
    stream.write_all(&raw).map_err(|err| err.to_string())?;
    stream.flush().map_err(|err| err.to_string())?;
    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .map_err(|err| err.to_string())?;
    parse_http_json(&buf)
}

fn parse_http_json(buf: &[u8]) -> Result<Value, String> {
    let text = String::from_utf8_lossy(buf);
    let idx = text
        .find("\r\n\r\n")
        .ok_or_else(|| "control reply missing body".to_string())?;
    let body = text[idx + 4..].trim();
    if body.is_empty() {
        return Err("control reply body is empty".into());
    }
    serde_json::from_str(body).map_err(|err| err.to_string())
}

fn require_ok(reply: &Value) -> Result<&Value, String> {
    if reply.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(reply.get("payload").unwrap_or(&Value::Null))
    } else {
        Err(ERROR_CONTROL_UNAVAILABLE.into())
    }
}

fn request_id(kind: &str) -> String {
    static REQUEST_ID_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = REQUEST_ID_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("gui-{kind}-{}-{nanos}-{sequence}", std::process::id())
}

#[cfg(unix)]
fn session_status(session: &ControlSession) -> Result<Value, String> {
    let reply = post_control(
        &session.socket,
        &json!({
            "type": "Status",
            "request_id": request_id("status"),
            "instance_epoch": session.instance_epoch,
            "owner_id": OWNER_ID,
            "owner_term": session.owner_term,
            "app_data_dir": session.home.display().to_string(),
            "payload": {},
        }),
    )?;
    Ok(require_ok(&reply)?.clone())
}

#[cfg(unix)]
fn required_reload_ack_matches(
    status: &Value,
    expected_hash: &str,
    expected_epoch: &str,
    expected_port: u16,
) -> bool {
    status.get("active_hash").and_then(Value::as_str) == Some(expected_hash)
        && status.get("instance_epoch").and_then(Value::as_str) == Some(expected_epoch)
        && status.get("port").and_then(Value::as_u64) == Some(u64::from(expected_port))
        && status.get("listen_ready").and_then(Value::as_bool) == Some(true)
}

#[cfg(unix)]
fn renew_owner_and_status(session: &ControlSession) -> Result<Value, String> {
    let renewal = post_control(
        &session.socket,
        &json!({
            "type": "AcquireOrRenewOwner",
            "request_id": request_id("renew"),
            "instance_epoch": session.instance_epoch,
            "owner_id": OWNER_ID,
            "owner_term": session.owner_term,
            "app_data_dir": session.home.display().to_string(),
            "payload": {
                "mode": "renew",
                "lease_budget_ms": ISOLATED_LEASE_BUDGET_MS,
            },
        }),
    )?;
    let payload = require_ok(&renewal)?;
    if payload.get("owner_term").and_then(Value::as_i64) != Some(session.owner_term) {
        return Err(ERROR_CONTROL_UNAVAILABLE.into());
    }
    session_status(session)
}

fn owner_lease_valid(payload: &Value) -> bool {
    payload.get("owner_lease_valid").and_then(Value::as_bool) == Some(true)
}

#[cfg(unix)]
impl From<&Session> for ControlSession {
    fn from(session: &Session) -> Self {
        Self {
            home: session.home.clone(),
            socket: session.socket.clone(),
            owner_term: session.owner_term,
            instance_epoch: session.instance_epoch.clone(),
        }
    }
}

fn status_with_supervisor(
    payload: Value,
    previous: &GoRouteIsolatedStatus,
) -> GoRouteIsolatedStatus {
    let listen_ready = payload
        .get("listen_ready")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    GoRouteIsolatedStatus {
        state: if listen_ready { "ready" } else { "failed" }.into(),
        listen_ready,
        port: payload
            .get("port")
            .and_then(Value::as_u64)
            .map(|value| value as u16),
        last_error: (payload
            .get("last_error")
            .is_some_and(|value| !value.is_null())
            || !listen_ready)
            .then(|| ERROR_CONTROL_UNAVAILABLE.into()),
        home: previous.home.clone(),
        lifecycle: payload
            .get("lifecycle")
            .and_then(Value::as_str)
            .map(str::to_owned),
        in_flight_count: payload
            .get("in_flight_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        member_count: payload
            .get("member_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        healthy_member_count: payload
            .get("healthy_member_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        recovering: previous.recovering,
        restart_count: previous.restart_count,
    }
}

fn restart_backoff(attempt: u32) -> Duration {
    Duration::from_secs(1_u64 << attempt.saturating_sub(1).min(3))
}
