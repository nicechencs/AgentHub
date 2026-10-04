//! Isolated Go route supervisor for saved test routes.
//!
//! Scratch home only. Refuses product port 43121 and real ~/.agenthub.
//! Does not start BridgeRuntimeHost or write login / connection / Agent config.

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
#[cfg(debug_assertions)]
use std::sync::Weak;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(all(debug_assertions, unix))]
use agenthub_core::error::AppError;
use agenthub_core::AgentHub;

const PRODUCT_DEFAULT_PORT: u16 = 43121;
const OWNER_ID: &str = "agenthub-gui";
const PROTOCOL_VERSION: &str = "route-runtime.v0-isolated";
const CONFIG_FORMAT_VERSION: &str = "route-config.v0-isolated";
const PACKAGE_VERSION: &str = "0.0.0-isolated";
const CONFIG_STREAM_CAPABILITY: &str = "config.stdin_stream.atomic";
const ISOLATED_LEASE_BUDGET_MS: i64 = 24 * 60 * 60 * 1_000;
const OWNER_RENEW_INTERVAL: Duration = Duration::from_secs(30);
const START_STOP_WAIT: Duration = Duration::from_secs(10);
const GRACEFUL_STOP_WAIT: Duration = Duration::from_secs(9);
const CONFIG_WRITE_WAIT: Duration = Duration::from_secs(8);
const MAX_RECOVERY_BUDGET: u32 = 3;
const STABLE_RUN_RESET: Duration = Duration::from_secs(30);
const ERROR_ISOLATED_UNAVAILABLE: &str = "Go route is unavailable in this build";
const ERROR_START_FAILED: &str = "Go route could not start";
const ERROR_CONTROL_UNAVAILABLE: &str = "Go route status is unavailable";
#[cfg(all(debug_assertions, unix))]
const ERROR_REQUIRED_RELOAD_FAILED: &str = "go.route.required_reload_failed";
#[cfg(all(debug_assertions, unix))]
const REQUIRED_RELOAD_STOPPED_MESSAGE: &str =
    "Go route configuration could not be updated; Go route was stopped";
const MAX_RUNTIME_CONFIG_BYTES: usize = 8 * 1024 * 1024;

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
    #[allow(dead_code)] // constructed only by release and non-Unix builds
    Unavailable,
    Unchanged,
}

pub struct GoRouteIsolatedHost {
    hub: Option<Arc<AgentHub>>,
    inner: Mutex<Inner>,
    update_gate: Mutex<()>,
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
    config_stdin: Arc<Mutex<ChildStdin>>,
    adapterd: Child,
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
            }),
            update_gate: Mutex::new(()),
        });
        #[cfg(debug_assertions)]
        Self::spawn_monitor(Arc::downgrade(&host));
        #[cfg(all(debug_assertions, unix))]
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
        #[cfg(not(debug_assertions))]
        {
            return unavailable_status();
        }
        #[cfg(all(debug_assertions, not(unix)))]
        {
            return unix_only_failed();
        }
        #[cfg(all(debug_assertions, unix))]
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
        #[cfg(not(debug_assertions))]
        {
            return unavailable_status();
        }
        #[cfg(all(debug_assertions, not(unix)))]
        {
            return unix_only_failed();
        }
        #[cfg(all(debug_assertions, unix))]
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
        #[cfg(not(debug_assertions))]
        {
            return unavailable_status();
        }
        #[cfg(all(debug_assertions, not(unix)))]
        {
            return unix_only_failed();
        }
        #[cfg(all(debug_assertions, unix))]
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
        #[cfg(not(debug_assertions))]
        {
            return GoRouteRequiredReloadResult::Skipped {
                reason: GoRouteRequiredReloadSkipReason::Unavailable,
            };
        }
        #[cfg(all(debug_assertions, not(unix)))]
        {
            return GoRouteRequiredReloadResult::Skipped {
                reason: GoRouteRequiredReloadSkipReason::Unavailable,
            };
        }
        #[cfg(all(debug_assertions, unix))]
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
        #[cfg(not(debug_assertions))]
        {
            GoRouteRequiredReloadResult::Skipped {
                reason: GoRouteRequiredReloadSkipReason::Unavailable,
            }
        }
        #[cfg(all(debug_assertions, not(unix)))]
        {
            GoRouteRequiredReloadResult::Skipped {
                reason: GoRouteRequiredReloadSkipReason::Unavailable,
            }
        }
        #[cfg(all(debug_assertions, unix))]
        {
            self.fail_required_reload()
        }
    }

    #[cfg(all(debug_assertions, unix))]
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

    #[cfg(all(debug_assertions, unix))]
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

    #[cfg(all(debug_assertions, unix))]
    fn status_with_reload_error(&self) -> GoRouteIsolatedStatus {
        let mut inner = self.lock();
        inner.status.last_error = Some("Go route configuration could not be updated".into());
        inner.status.clone()
    }

    #[cfg(all(debug_assertions, unix))]
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
        #[cfg(not(debug_assertions))]
        {
            return unavailable_status();
        }
        #[cfg(all(debug_assertions, not(unix)))]
        {
            return unix_only_failed();
        }
        #[cfg(all(debug_assertions, unix))]
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

    #[cfg(debug_assertions)]
    fn spawn_monitor(host: Weak<Self>) {
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(500));
            let Some(host) = host.upgrade() else { return };
            #[cfg(unix)]
            {
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
            }
        });
    }

    #[cfg(all(debug_assertions, unix))]
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

#[cfg(all(debug_assertions, unix))]
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

#[cfg(all(debug_assertions, unix))]
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
            Ok(Some(_)) => return,
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
        Command::new(&bin)
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
    let started = match handshake_start(&home, &socket, plan.port, &plan.config_hash) {
        Ok(session) => Session {
            home: session.home,
            socket: session.socket,
            owner_term: session.owner_term,
            instance_epoch: session.instance_epoch,
            port: session.port,
            next_owner_renewal: Instant::now() + OWNER_RENEW_INTERVAL,
            config_stdin,
            adapterd,
        },
        Err(err) => {
            let _ = adapterd.kill();
            let _ = adapterd.wait();
            return Err(err);
        }
    };
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
}

#[cfg(unix)]
fn handshake_start(
    home: &Path,
    socket: &Path,
    fallback_port: u16,
    expected_config_hash: &str,
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
                "package_version": PACKAGE_VERSION,
                "app_data_dir": home_s,
            },
        }),
    )?;
    let hs_payload = require_ok(&hs)?;
    let supports_config_stream = hs_payload
        .get("capabilities")
        .and_then(Value::as_array)
        .is_some_and(|capabilities| {
            capabilities
                .iter()
                .any(|capability| capability.as_str() == Some(CONFIG_STREAM_CAPABILITY))
        });
    if !supports_config_stream {
        return Err("Go route config stream is unavailable".into());
    }
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
    })
}

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

fn create_scratch_home() -> Result<PathBuf, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = PathBuf::from("/tmp/agenthub-go-route-isolated")
        .join(format!("{}-{nanos}", std::process::id()));
    let home = root.join("home");
    fs::create_dir_all(home.join("config")).map_err(|err| err.to_string())?;
    fs::create_dir_all(home.join("run")).map_err(|err| err.to_string())?;
    fs::create_dir_all(home.join("logs")).map_err(|err| err.to_string())?;
    let home = fs::canonicalize(&home).map_err(|err| err.to_string())?;
    if !is_scratch_home(&home) {
        return Err("scratch home escaped /tmp".into());
    }
    Ok(home)
}

fn is_scratch_home(path: &Path) -> bool {
    path.starts_with("/tmp") || path.starts_with("/var/tmp")
}

fn is_forbidden_user_home(path: &Path) -> bool {
    let Ok(user_home) = std::env::var("HOME") else {
        return false;
    };
    let real = PathBuf::from(user_home).join(".agenthub");
    let real = fs::canonicalize(&real).unwrap_or(real);
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    resolved == real || resolved.starts_with(&real)
}

fn resolve_adapterd_bin(scratch_root: &Path) -> Result<PathBuf, String> {
    if let Ok(raw) = std::env::var("AGENTHUB_ADAPTERD_BIN") {
        let path = PathBuf::from(raw);
        if path.is_file() {
            return Ok(path);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("agenthub-adapterd");
            if sibling.is_file() {
                return Ok(sibling);
            }
        }
    }
    let src = find_adapterd_src()?;
    let dest_dir = scratch_root.join("bin");
    fs::create_dir_all(&dest_dir).map_err(|err| err.to_string())?;
    let dest = dest_dir.join("agenthub-adapterd");
    let status = Command::new("go")
        .arg("build")
        .arg("-o")
        .arg(&dest)
        .current_dir(&src)
        .status()
        .map_err(|err| format!("go build: {err}"))?;
    if !status.success() {
        return Err("go build agenthub-adapterd failed".into());
    }
    Ok(dest)
}

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
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("gui-{kind}-{nanos}")
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

#[cfg(all(debug_assertions, unix))]
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

#[cfg(all(debug_assertions, unix))]
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
