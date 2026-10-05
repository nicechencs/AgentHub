//! Isolated Go route supervisor for saved test routes.
//!
//! Scratch home only. Refuses product port 43121 and real ~/.agenthub.
//! Does not start BridgeRuntimeHost or write login / connection / Agent config.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[cfg(any(unix, windows))]
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, OpenOptions};
#[cfg(any(unix, windows))]
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
#[cfg(any(unix, windows, feature = "go-route-bind-probe"))]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(any(unix, windows))]
use std::sync::Condvar;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[cfg(unix)]
use std::{os::unix::fs::MetadataExt, os::unix::fs::OpenOptionsExt};

#[cfg(any(unix, windows))]
use agenthub_core::error::AppError;
use agenthub_core::AgentHub;

const PRODUCT_DEFAULT_PORT: u16 = 43121;
const OWNER_ID: &str = "agenthub-gui";
const PROTOCOL_VERSION: &str = "route-runtime.v0-isolated";
const CONFIG_FORMAT_VERSION: &str = "route-config.v1-usage-spool";
#[cfg(debug_assertions)]
const ISOLATED_DEV_PACKAGE_VERSION: &str = "0.0.0-isolated";
const BUNDLED_PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");
const BUNDLED_SHA256: &str = env!("AGENTHUB_ADAPTERD_BUNDLED_SHA256");
const EMBEDDED_BUNDLED_VERSION: &str = env!("AGENTHUB_ADAPTERD_BUNDLED_VERSION");
const CONFIG_STREAM_CAPABILITY: &str = "config.stdin_stream.atomic";
const OAUTH_REFRESH_CAPABILITY: &str = "control.oauth_refresh.v1";
const ISOLATED_LEASE_BUDGET_MS: i64 = 24 * 60 * 60 * 1_000;
const OWNER_RENEW_INTERVAL: Duration = Duration::from_secs(30);
#[cfg(any(unix, windows))]
const OAUTH_REFRESH_POLL_WAIT_MS: u64 = 1_000;
#[cfg(any(unix, windows))]
const OAUTH_REFRESH_CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(any(unix, windows))]
const OAUTH_REFRESH_HANDLER_LIMIT: usize = 8;
#[cfg(any(unix, windows))]
const OAUTH_REFRESH_QUEUE_LIMIT: usize = 8;
#[cfg(any(unix, windows))]
const OAUTH_REFRESH_ACTION_GATE_LIMIT: usize = 4_096;
#[cfg(any(unix, windows))]
const OAUTH_REFRESH_COMPLETED_ID_LIMIT: usize = 4_096;
#[cfg(any(unix, windows))]
const OAUTH_REFRESH_COMPLETED_ID_TTL: Duration = Duration::from_secs(90);
const START_STOP_WAIT: Duration = Duration::from_secs(10);
const GRACEFUL_STOP_WAIT: Duration = Duration::from_secs(9);
const CONFIG_WRITE_WAIT: Duration = Duration::from_secs(8);
const CONTROL_START_WAIT: Duration = Duration::from_secs(8);
const CONTROL_TOKEN_BYTES: usize = 32;
const CONTROL_TOKEN_ENCODED_BYTES: usize = 43;
const LEGACY_CONTROL_TOKEN_ENV: &str = "AGENTHUB_ADAPTERD_CONTROL_TOKEN";
const TCP_CONTROL_STDOUT_PREFIX: &str = "agenthub-adapterd control listener: tcp4 ";
const MAX_CONTROL_STDOUT_LINE_BYTES: usize = 4 * 1024;
const MAX_CONTROL_STDOUT_LOG_BYTES: usize = 64 * 1024;
const MAX_CONTROL_RESPONSE_HEADER_BYTES: usize = 32 * 1024;
const MAX_CONTROL_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_RECOVERY_BUDGET: u32 = 3;
const STABLE_RUN_RESET: Duration = Duration::from_secs(30);
const ERROR_ISOLATED_UNAVAILABLE: &str = "Go route is unavailable in this build";
const ERROR_START_FAILED: &str = "Go route could not start";
const ERROR_CONTROL_UNAVAILABLE: &str = "Go route status is unavailable";
#[cfg(any(unix, windows))]
const ERROR_MODE_ACTIVE: &str = "Go route is already active in another mode";
#[cfg(any(unix, windows))]
const ERROR_PREPARED_PRODUCT_REQUIRED: &str =
    "Product Go route must start from a prepared Product plan";
#[cfg(any(unix, windows))]
const ERROR_REQUIRED_RELOAD_FAILED: &str = "go.route.required_reload_failed";
#[cfg(any(unix, windows))]
const REQUIRED_RELOAD_STOPPED_MESSAGE: &str =
    "Go route configuration could not be updated; Go route was stopped";
const MAX_RUNTIME_CONFIG_BYTES: usize = 8 * 1024 * 1024;
#[cfg(any(unix, windows))]
const SCRATCH_ROOT_PREFIX: &str = "agenthub-go-route-isolated-";
#[cfg(any(unix, windows))]
const SCRATCH_OWNER_MARKER: &str = ".agenthub-go-route-owner-v1";
#[cfg(any(unix, windows))]
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
    pub edge_statuses: Vec<GoRouteEdgeStatus>,
    pub recovering: bool,
    pub restart_count: u32,
}

/// Sanitized per-pool runtime status from Go control. It is informational for
/// future backend selection only: the process boundary never returns ingress
/// keys, request bodies, login data, or upstream messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoRouteEdgeStatus {
    pub pool_id: String,
    pub surface: String,
    pub member_count: u64,
    pub healthy_member_count: u64,
    pub in_flight_count: u64,
    pub request_success_count: u64,
    pub request_failure_count: u64,
    pub last_error_code: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GoRouteRunMode {
    Isolated,
    Product,
}

impl GoRouteRunMode {
    fn as_arg(self) -> &'static str {
        match self {
            Self::Isolated => "isolated",
            Self::Product => "product",
        }
    }
}

pub struct GoRouteIsolatedHost {
    hub: Option<Arc<AgentHub>>,
    inner: Mutex<Inner>,
    update_gate: Mutex<()>,
    #[cfg(feature = "go-route-bind-probe")]
    required_reload_ack_count: AtomicU64,
}

#[cfg(any(unix, windows))]
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
    active_mode: Option<GoRouteRunMode>,
    lifecycle_generation: u64,
    lifecycle_in_flight: bool,
    prepared_product_generation: Option<u64>,
    desired: bool,
    stopping: bool,
    recovery_budget_used: u32,
    stable_since: Option<Instant>,
    next_restart_at: Option<Instant>,
    committed_plan: Option<RuntimePlan>,
    #[cfg(any(unix, windows))]
    oauth_refresh_worker_session: Option<(String, i64)>,
}

#[derive(Clone)]
struct RuntimePlan {
    config: Vec<u8>,
    config_hash: String,
    port: u16,
    product_home: Option<ProductHomeLocation>,
    mode: GoRouteRunMode,
}

/// Opaque, single-use Product startup input.
///
/// The configuration bytes and Product home identity are captured together by
/// [`GoRouteIsolatedHost::prepare_product_plan`]. Keeping the fields private and
/// deliberately omitting `Clone`, `Debug`, and serialization prevents callers
/// from copying or exposing the in-memory login material.
#[allow(dead_code)]
pub(crate) struct PreparedProductPlan {
    host: Weak<GoRouteIsolatedHost>,
    generation: u64,
    plan: Option<RuntimePlan>,
    reservation_active: bool,
}

/// Non-secret expectations exposed only to real-process runtime probes.
#[cfg(feature = "go-route-product-probe")]
pub(crate) struct PreparedProductProbeSummary {
    pub expected_port: u16,
    pub expected_config_hash: String,
}

/// Sanitized `/health` observation. It never retains or returns the bearer.
#[cfg(feature = "go-route-product-probe")]
pub(crate) struct GoRouteHealthProbe {
    pub http_status: u16,
    pub listen_ready: Option<bool>,
    pub member_count: Option<u64>,
    pub healthy_member_count: Option<u64>,
}

#[cfg(any(unix, windows))]
impl Drop for PreparedProductPlan {
    fn drop(&mut self) {
        if !self.reservation_active {
            return;
        }
        if let Some(host) = self.host.upgrade() {
            host.release_product_reservation(self.generation);
        }
        self.reservation_active = false;
    }
}

#[cfg(any(unix, windows))]
#[derive(Clone, PartialEq, Eq)]
struct ProductHomeLocation {
    data_dir: PathBuf,
    home: PathBuf,
    identity: ProductDataDirIdentity,
}

#[cfg(unix)]
#[derive(Clone, PartialEq, Eq)]
struct ProductDataDirIdentity {
    device: u64,
    inode: u64,
}

#[cfg(windows)]
#[derive(Clone, PartialEq, Eq)]
struct ProductDataDirIdentity {
    volume_serial: u32,
    file_index: u64,
}

struct Session {
    home: PathBuf,
    staging_home: PathBuf,
    endpoint: ControlEndpoint,
    owner_term: i64,
    instance_epoch: String,
    mode: GoRouteRunMode,
    port: u16,
    product_home: Option<ProductHomeLocation>,
    next_owner_renewal: Instant,
    oauth_refresh_supported: bool,
    config_stdin: Arc<Mutex<ChildStdin>>,
    adapterd: AdapterdProcess,
    #[cfg(any(unix, windows))]
    _product_home_handles: Vec<fs::File>,
    #[cfg(windows)]
    scratch_handles: Vec<OwnedHandle>,
}

struct AdapterdProcess {
    child: Child,
    stdout_reader: Option<std::thread::JoinHandle<()>>,
    #[cfg(windows)]
    _job: WindowsJob,
}

impl std::ops::Deref for AdapterdProcess {
    type Target = Child;

    fn deref(&self) -> &Self::Target {
        &self.child
    }
}

impl std::ops::DerefMut for AdapterdProcess {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

#[derive(Clone)]
enum ControlEndpoint {
    #[cfg(unix)]
    Unix(PathBuf),
    Tcp {
        address: SocketAddrV4,
        bearer: Arc<str>,
    },
}

#[cfg(any(unix, windows))]
struct ResolvedAdapterd {
    path: PathBuf,
    package_version: &'static str,
    #[cfg(windows)]
    _verified_handles: Vec<fs::File>,
}

#[cfg(any(unix, windows))]
struct ScratchHomeGuard {
    home: PathBuf,
    armed: bool,
    #[cfg(windows)]
    handles: Vec<OwnedHandle>,
}

#[cfg(any(unix, windows))]
struct CreatedScratchHome {
    home: PathBuf,
    #[cfg(windows)]
    handles: Vec<OwnedHandle>,
}

#[cfg(any(unix, windows))]
struct PreparedProductHome {
    handles: Vec<fs::File>,
}

#[cfg(any(unix, windows))]
impl ScratchHomeGuard {
    fn disarm(&mut self) {
        self.armed = false;
    }

    #[cfg(windows)]
    fn take_handles(&mut self) -> Vec<OwnedHandle> {
        std::mem::take(&mut self.handles)
    }
}

#[cfg(any(unix, windows))]
impl Drop for ScratchHomeGuard {
    fn drop(&mut self) {
        if self.armed {
            #[cfg(windows)]
            self.handles.clear();
            cleanup_scratch_home(&self.home);
        }
    }
}

impl AdapterdProcess {
    fn join_stdout_reader(&mut self) {
        if let Some(reader) = self.stdout_reader.take() {
            let _ = reader.join();
        }
    }
}

#[cfg(windows)]
struct WindowsJob {
    handle: OwnedHandle,
}

#[cfg(windows)]
impl AdapterdProcess {
    fn terminate_job(&self) {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;

        // SAFETY: the handle is an owned Job Object handle kept alive by this
        // process wrapper for at least the duration of the call.
        unsafe {
            let _ = TerminateJobObject(self._job.handle.as_raw_handle() as _, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for AdapterdProcess {
    fn drop(&mut self) {
        self.terminate_job();
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.join_stdout_reader();
    }
}

#[cfg(any(unix, windows))]
#[derive(Clone)]
struct ControlSession {
    home: PathBuf,
    endpoint: ControlEndpoint,
    owner_term: i64,
    instance_epoch: String,
    mode: GoRouteRunMode,
    expected_port: u16,
    product_home: Option<ProductHomeLocation>,
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
        edge_statuses: Vec::new(),
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
        edge_statuses: Vec::new(),
        recovering: false,
        restart_count: 0,
    }
}

#[cfg(any(unix, windows))]
fn mode_conflict_status() -> GoRouteIsolatedStatus {
    GoRouteIsolatedStatus {
        state: "failed".into(),
        listen_ready: false,
        port: None,
        last_error: Some(ERROR_MODE_ACTIVE.into()),
        home: None,
        lifecycle: Some("mode_conflict".into()),
        in_flight_count: 0,
        member_count: 0,
        healthy_member_count: 0,
        edge_statuses: Vec::new(),
        recovering: false,
        restart_count: 0,
    }
}

#[cfg(any(unix, windows))]
fn prepared_product_required_status() -> GoRouteIsolatedStatus {
    GoRouteIsolatedStatus {
        state: "failed".into(),
        listen_ready: false,
        port: None,
        last_error: Some(ERROR_PREPARED_PRODUCT_REQUIRED.into()),
        home: None,
        lifecycle: Some("prepared_product_required".into()),
        in_flight_count: 0,
        member_count: 0,
        healthy_member_count: 0,
        edge_statuses: Vec::new(),
        recovering: false,
        restart_count: 0,
    }
}

impl GoRouteIsolatedHost {
    pub fn new(hub: Option<Arc<AgentHub>>) -> Arc<Self> {
        #[cfg(any(unix, windows))]
        cleanup_stale_scratch_roots();
        let host = Arc::new(Self {
            hub,
            inner: Mutex::new(Inner {
                status: stopped_status(),
                session: None,
                active_mode: None,
                lifecycle_generation: 0,
                lifecycle_in_flight: false,
                prepared_product_generation: None,
                desired: false,
                stopping: false,
                recovery_budget_used: 0,
                stable_since: None,
                next_restart_at: None,
                committed_plan: None,
                #[cfg(any(unix, windows))]
                oauth_refresh_worker_session: None,
            }),
            update_gate: Mutex::new(()),
            #[cfg(feature = "go-route-bind-probe")]
            required_reload_ack_count: AtomicU64::new(0),
        });
        #[cfg(any(unix, windows))]
        Self::spawn_monitor(Arc::downgrade(&host));
        #[cfg(any(unix, windows))]
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
        #[cfg(not(any(unix, windows)))]
        {
            return platform_unavailable_status();
        }
        #[cfg(any(unix, windows))]
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
                        inner.lifecycle_in_flight = true;
                        mark_runtime_unavailable(&mut inner);
                        drop(inner);
                        if let Some(session) = session.as_mut() {
                            terminate_session(session);
                        }
                        let mut inner = self.lock();
                        inner.lifecycle_in_flight = false;
                        return inner.status.clone();
                    }
                }
            }
            inner.status.clone()
        }
    }

    #[cfg(any(feature = "go-route-bind-probe", feature = "go-route-product-probe"))]
    pub(crate) fn probe_config_hash(&self) -> Option<String> {
        self.lock()
            .committed_plan
            .as_ref()
            .map(|plan| plan.config_hash.clone())
    }

    #[cfg(feature = "go-route-product-probe")]
    pub(crate) fn probe_session_process(&self) -> Result<(u32, PathBuf, PathBuf), String> {
        let inner = self.lock();
        let session = inner
            .session
            .as_ref()
            .ok_or_else(|| "Go route session is not running".to_string())?;
        Ok((
            session.adapterd.id(),
            session.home.clone(),
            session.staging_home.clone(),
        ))
    }

    #[cfg(feature = "go-route-product-probe")]
    pub(crate) fn probe_prepared_product_summary(
        prepared: &PreparedProductPlan,
    ) -> Result<PreparedProductProbeSummary, String> {
        let plan = prepared
            .plan
            .as_ref()
            .ok_or_else(|| "Prepared Product plan was already consumed".to_string())?;
        Ok(PreparedProductProbeSummary {
            expected_port: plan.port,
            expected_config_hash: plan.config_hash.clone(),
        })
    }

    /// Probe one Product data-plane health endpoint without using environment
    /// proxies, redirects, external hosts, or an unbounded response body.
    #[cfg(feature = "go-route-product-probe")]
    pub(crate) fn probe_data_plane_health(
        port: u16,
        bearer: &str,
    ) -> Result<GoRouteHealthProbe, String> {
        const HEALTH_TIMEOUT: Duration = Duration::from_secs(2);
        const MAX_HEALTH_BODY_BYTES: u64 = 32 * 1024;
        if port == 0
            || bearer.trim().is_empty()
            || bearer.bytes().any(|byte| byte <= b' ' || byte == 0x7f)
        {
            return Err("Product health probe input is invalid".into());
        }
        let agent = ureq::AgentBuilder::new()
            .timeout(HEALTH_TIMEOUT)
            .redirects(0)
            .try_proxy_from_env(false)
            .build();
        let response = match agent
            .get(&format!("http://127.0.0.1:{port}/health"))
            .set("Authorization", &format!("Bearer {bearer}"))
            .call()
        {
            Ok(response) => response,
            Err(ureq::Error::Status(_, response)) => response,
            Err(ureq::Error::Transport(_)) => {
                return Err("Product health probe transport failed".into())
            }
        };
        let http_status = response.status();
        if http_status != 200 {
            return Ok(GoRouteHealthProbe {
                http_status,
                listen_ready: None,
                member_count: None,
                healthy_member_count: None,
            });
        }
        let mut body = Vec::new();
        response
            .into_reader()
            .take(MAX_HEALTH_BODY_BYTES.saturating_add(1))
            .read_to_end(&mut body)
            .map_err(|_| "Product health probe body read failed".to_string())?;
        if body.len() as u64 > MAX_HEALTH_BODY_BYTES {
            return Err("Product health probe body exceeded its limit".into());
        }
        let body: Value = serde_json::from_slice(&body)
            .map_err(|_| "Product health probe returned invalid JSON".to_string())?;
        Ok(GoRouteHealthProbe {
            http_status,
            listen_ready: body.get("listen_ready").and_then(Value::as_bool),
            member_count: body.get("member_count").and_then(Value::as_u64),
            healthy_member_count: body.get("healthy_member_count").and_then(Value::as_u64),
        })
    }

    #[cfg(feature = "go-route-product-probe")]
    pub(crate) fn probe_kill_process(&self) -> Result<(), String> {
        let mut inner = self.lock();
        let session = inner
            .session
            .as_mut()
            .ok_or_else(|| "Go route session is not running".to_string())?;
        session
            .adapterd
            .kill()
            .map_err(|error| format!("kill Go route process: {error}"))
    }

    #[cfg(feature = "go-route-bind-probe")]
    pub(crate) fn probe_required_reload_ack_count(&self) -> u64 {
        self.required_reload_ack_count.load(Ordering::SeqCst)
    }

    #[cfg(all(unix, feature = "go-route-tcp-control-probe"))]
    pub(crate) fn probe_tcp_control_startup_secret_scan(&self) -> Result<bool, String> {
        #[cfg(not(target_os = "linux"))]
        {
            return Err("TCP control startup secret scan requires Linux /proc".into());
        }
        #[cfg(target_os = "linux")]
        {
            let inner = self.lock();
            let session = inner
                .session
                .as_ref()
                .ok_or_else(|| "Go route session is not running".to_string())?;
            let ControlEndpoint::Tcp { bearer, .. } = &session.endpoint else {
                return Err("Go route is not using TCP control".into());
            };
            let needles = [bearer.as_bytes(), LEGACY_CONTROL_TOKEN_ENV.as_bytes()];
            let proc_root = PathBuf::from(format!("/proc/{}", session.adapterd.id()));
            if file_contains_any(&proc_root.join("cmdline"), &needles)?
                || file_contains_any(&proc_root.join("environ"), &needles)?
                || regular_tree_contains_any(&session.home, &needles)?
            {
                return Err("TCP control startup secret scan failed".into());
            }
            Ok(true)
        }
    }

    #[cfg(all(
        unix,
        feature = "go-route-bind-probe",
        feature = "go-route-tcp-control-probe"
    ))]
    pub(crate) fn probe_tcp_control_semantic_rejection(&self) -> Result<u16, String> {
        let (control, active_hash, control_port) = {
            let inner = self.lock();
            let session = inner
                .session
                .as_ref()
                .ok_or_else(|| "Go route session is not running".to_string())?;
            let ControlEndpoint::Tcp { address, .. } = &session.endpoint else {
                return Err("Go route is not using TCP control".into());
            };
            let active_hash = inner
                .committed_plan
                .as_ref()
                .map(|plan| plan.config_hash.clone())
                .ok_or_else(|| "Go route has no committed configuration".to_string())?;
            (ControlSession::from(session), active_hash, address.port())
        };
        let event = OAuthRefreshEvent {
            refresh_id: "probe-unknown-refresh".into(),
            instance_epoch: control.instance_epoch.clone(),
            owner_term: control.owner_term,
            active_hash,
            edge_id: "probe-edge".into(),
            member_id: "probe-member".into(),
            source_kind: "provider".into(),
            source_id: "probe-source".into(),
            refresh_kind: "official_login".into(),
        };
        match complete_oauth_refresh(
            &control,
            &event,
            &OAuthRefreshCompletion::NotRefreshed,
            &request_id("probe-complete-oauth-refresh"),
        ) {
            Err(CompleteOAuthRefreshError::Rejected) => Ok(control_port),
            Err(CompleteOAuthRefreshError::Transport) => {
                Err("TCP control treated a semantic rejection as a transport failure".into())
            }
            Ok(()) => Err("TCP control accepted an unknown OAuth refresh completion".into()),
        }
    }

    #[cfg(feature = "go-route-bind-probe")]
    fn record_required_reload_ack(&self) {
        self.required_reload_ack_count
            .fetch_add(1, Ordering::SeqCst);
    }

    pub fn start(&self) -> GoRouteIsolatedStatus {
        self.start_mode(GoRouteRunMode::Isolated)
    }

    /// Reserves this idle host and captures one complete Product startup input.
    /// Starting the returned plan never rebuilds its configuration from the
    /// database; normal runtime reloads remain responsible for later writes.
    /// Dropping an unconsumed plan releases the reservation.
    #[allow(dead_code)]
    pub(crate) fn prepare_product_plan(self: &Arc<Self>) -> Result<PreparedProductPlan, String> {
        #[cfg(not(any(unix, windows)))]
        {
            Err(ERROR_ISOLATED_UNAVAILABLE.into())
        }
        #[cfg(any(unix, windows))]
        {
            let generation = {
                let mut inner = self.lock();
                refresh_locked(&mut inner);
                if inner.stopping
                    || inner.lifecycle_in_flight
                    || inner.prepared_product_generation.is_some()
                    || inner.session.is_some()
                    || inner.desired
                    || inner.active_mode.is_some()
                    || inner.committed_plan.is_some()
                {
                    return Err(ERROR_MODE_ACTIVE.into());
                }
                inner.lifecycle_generation = inner.lifecycle_generation.wrapping_add(1);
                let generation = inner.lifecycle_generation;
                inner.prepared_product_generation = Some(generation);
                inner.active_mode = Some(GoRouteRunMode::Product);
                generation
            };
            let mut prepared = PreparedProductPlan {
                host: Arc::downgrade(self),
                generation,
                plan: None,
                reservation_active: true,
            };
            let hub = self
                .hub
                .as_ref()
                .ok_or_else(|| ERROR_ISOLATED_UNAVAILABLE.to_string())?;
            let (port, config) = prepare_product_runtime(hub)?;
            let plan = RuntimePlan {
                config_hash: sha256_hex(&config),
                config,
                port,
                product_home: Some(resolve_product_home(hub)?),
                mode: GoRouteRunMode::Product,
            };
            {
                let inner = self.lock();
                if inner.prepared_product_generation != Some(generation)
                    || inner.active_mode != Some(GoRouteRunMode::Product)
                    || inner.lifecycle_generation != generation
                {
                    return Err("Prepared Product plan was invalidated during preparation".into());
                }
            }
            prepared.plan = Some(plan);
            Ok(prepared)
        }
    }

    /// Starts an idle host from the exact single-use Product plan captured by
    /// [`Self::prepare_product_plan`]. The plan is consumed even when the host
    /// is no longer idle, so stale login material cannot be retried implicitly.
    #[allow(dead_code)]
    pub(crate) fn start_prepared_product(
        self: &Arc<Self>,
        mut prepared: PreparedProductPlan,
    ) -> GoRouteIsolatedStatus {
        #[cfg(not(any(unix, windows)))]
        {
            let _ = prepared;
            platform_unavailable_status()
        }
        #[cfg(any(unix, windows))]
        {
            let Some(prepared_host) = prepared.host.upgrade() else {
                return unavailable_status();
            };
            if !Arc::ptr_eq(self, &prepared_host) {
                return mode_conflict_status();
            }
            drop(prepared_host);
            let Some(plan) = prepared.plan.take() else {
                return mode_conflict_status();
            };
            let generation = prepared.generation;
            {
                let mut inner = self.lock();
                refresh_locked(&mut inner);
                if inner.prepared_product_generation != Some(generation)
                    || inner.lifecycle_generation != generation
                    || inner.active_mode != Some(GoRouteRunMode::Product)
                    || inner.stopping
                    || inner.lifecycle_in_flight
                    || inner.session.is_some()
                    || inner.desired
                    || inner.committed_plan.is_some()
                {
                    return mode_conflict_status();
                }
                inner.prepared_product_generation = None;
                inner.desired = true;
                inner.recovery_budget_used = 0;
                inner.stable_since = None;
                inner.next_restart_at = None;
                inner.status.state = "starting".into();
                inner.status.last_error = None;
                inner.status.listen_ready = false;
                inner.lifecycle_in_flight = true;
            }
            prepared.reservation_active = false;
            self.finish_start(false, generation, Some(plan))
        }
    }

    /// Starts this single process host in the requested mode. A live session
    /// or desired automatic recovery owns its mode until it is fully stopped.
    pub(crate) fn start_mode(&self, mode: GoRouteRunMode) -> GoRouteIsolatedStatus {
        #[cfg(not(any(unix, windows)))]
        {
            let _ = mode;
            return platform_unavailable_status();
        }
        #[cfg(any(unix, windows))]
        {
            if mode == GoRouteRunMode::Product {
                return prepared_product_required_status();
            }
            let start_generation = {
                let mut inner = self.lock();
                refresh_locked(&mut inner);
                if inner.prepared_product_generation.is_some() {
                    return mode_conflict_status();
                }
                if inner.stopping {
                    return mode_conflict_status();
                }
                if inner.lifecycle_in_flight {
                    return if inner.active_mode == Some(mode) {
                        inner.status.clone()
                    } else {
                        mode_conflict_status()
                    };
                }
                let mode_owned = inner.session.is_some() || inner.desired;
                if mode_owned {
                    match inner.active_mode {
                        Some(active) if active == mode => {}
                        Some(_) | None => return mode_conflict_status(),
                    }
                }
                inner.active_mode = Some(mode);
                inner.desired = true;
                inner.stopping = false;
                inner.recovery_budget_used = 0;
                inner.stable_since = None;
                inner.next_restart_at = None;
                if inner.status.state == "starting" {
                    return inner.status.clone();
                }
                if inner.session.is_some() {
                    None
                } else {
                    inner.status.state = "starting".into();
                    inner.status.last_error = None;
                    inner.status.listen_ready = false;
                    inner.lifecycle_generation = inner.lifecycle_generation.wrapping_add(1);
                    inner.lifecycle_in_flight = true;
                    Some(inner.lifecycle_generation)
                }
            };
            match start_generation {
                Some(generation) => self.finish_start(false, generation, None),
                None => self.reload(),
            }
        }
    }

    /// Rebuilds the complete edge table and swaps it into the running Go
    /// process. The previous committed snapshot remains the recovery source
    /// until Status acknowledges the exact new digest.
    pub fn reload(&self) -> GoRouteIsolatedStatus {
        #[cfg(not(any(unix, windows)))]
        {
            return platform_unavailable_status();
        }
        #[cfg(any(unix, windows))]
        {
            let _update = self
                .update_gate
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(hub) = self.hub.as_ref() else {
                return unavailable_status();
            };
            let (config_stdin, control, port, mode) = {
                let mut inner = self.lock();
                refresh_locked(&mut inner);
                if inner.prepared_product_generation.is_some() {
                    return mode_conflict_status();
                }
                let Some(session) = inner.session.as_ref() else {
                    return inner.status.clone();
                };
                (
                    Arc::clone(&session.config_stdin),
                    ControlSession::from(session),
                    session.port,
                    session.mode,
                )
            };
            let runtime_config = match build_runtime_config(hub, mode, port) {
                Ok(config) => config,
                Err(_) if mode == GoRouteRunMode::Product => {
                    let _ = self.fail_required_reload();
                    return self.lock().status.clone();
                }
                Err(_) => return self.status_with_reload_error(),
            };
            if runtime_config.product_home != control.product_home {
                let _ = self.fail_required_reload();
                return self.lock().status.clone();
            }
            let product_home = runtime_config.product_home;
            let config = runtime_config.config;
            let config_hash = sha256_hex(&config);
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
                product_home,
                mode,
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
        #[cfg(not(any(unix, windows)))]
        {
            return GoRouteRequiredReloadResult::Skipped {
                reason: GoRouteRequiredReloadSkipReason::Unavailable,
            };
        }
        #[cfg(any(unix, windows))]
        {
            let _update = self
                .update_gate
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());

            let session = {
                let mut inner = self.lock();
                refresh_locked(&mut inner);
                if clear_product_reservation_locked(&mut inner) {
                    return GoRouteRequiredReloadResult::Failed {
                        code: ERROR_REQUIRED_RELOAD_FAILED.into(),
                    };
                }
                if inner.desired && !inner.stopping {
                    inner.session.as_ref().map(|session| {
                        (
                            Arc::clone(&session.config_stdin),
                            ControlSession::from(session),
                            session.port,
                            session.mode,
                        )
                    })
                } else {
                    return GoRouteRequiredReloadResult::Skipped {
                        reason: GoRouteRequiredReloadSkipReason::NotRunning,
                    };
                }
            };
            let Some((config_stdin, control, port, mode)) = session else {
                return self.fail_required_reload();
            };

            let Some(hub) = self.hub.as_ref() else {
                return self.fail_required_reload();
            };
            let runtime_config = match build_runtime_config(hub, mode, port) {
                Ok(config) => config,
                Err(_) => return self.fail_required_reload(),
            };
            if runtime_config.product_home != control.product_home {
                return self.fail_required_reload();
            }
            let product_home = runtime_config.product_home;
            let config = runtime_config.config;
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
                        #[cfg(feature = "go-route-bind-probe")]
                        self.record_required_reload_ack();
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
                        product_home,
                        mode,
                    });
                    true
                } else {
                    false
                }
            };
            if !committed {
                return self.fail_required_reload();
            }

            #[cfg(feature = "go-route-bind-probe")]
            self.record_required_reload_ack();
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
        #[cfg(not(any(unix, windows)))]
        {
            GoRouteRequiredReloadResult::Skipped {
                reason: GoRouteRequiredReloadSkipReason::Unavailable,
            }
        }
        #[cfg(any(unix, windows))]
        {
            self.fail_required_reload()
        }
    }

    #[cfg(any(unix, windows))]
    fn fail_required_reload(&self) -> GoRouteRequiredReloadResult {
        let mut inner = self.lock();
        if clear_product_reservation_locked(&mut inner) {
            return GoRouteRequiredReloadResult::Failed {
                code: ERROR_REQUIRED_RELOAD_FAILED.into(),
            };
        }
        // Another lifecycle owner can have removed its session before doing
        // blocking process cleanup. It remains responsible for releasing the
        // mode: releasing it here would permit another mode to claim the
        // listener before that cleanup ends.
        if inner.lifecycle_in_flight && inner.status.state != "starting" {
            return GoRouteRequiredReloadResult::Failed {
                code: ERROR_REQUIRED_RELOAD_FAILED.into(),
            };
        }
        // A start has no session yet, but its blocking process creation may
        // still be in flight. Cancel it in place and leave its mode owned;
        // finish_start will reap its result and make the host idle.
        let start_pending = inner.lifecycle_in_flight
            && inner.session.is_none()
            && inner.status.state == "starting";
        if !start_pending {
            inner.lifecycle_in_flight = true;
        }
        let mut session = inner.session.take();
        let restart_count = inner.status.restart_count;
        inner.desired = false;
        inner.stopping = true;
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
            edge_statuses: Vec::new(),
            recovering: false,
            restart_count,
        };
        drop(inner);
        if let Some(session) = session.as_mut() {
            terminate_session(session);
        }
        if !start_pending {
            let mut inner = self.lock();
            inner.active_mode = None;
            inner.lifecycle_in_flight = false;
            inner.stopping = false;
        }
        GoRouteRequiredReloadResult::Failed {
            code: ERROR_REQUIRED_RELOAD_FAILED.into(),
        }
    }

    #[cfg(any(unix, windows))]
    fn fail_session(&self, control: &ControlSession) -> GoRouteIsolatedStatus {
        let mut inner = self.lock();
        let same_session = inner
            .session
            .as_ref()
            .is_some_and(|session| session.instance_epoch == control.instance_epoch);
        let mut session = same_session.then(|| inner.session.take()).flatten();
        if same_session {
            inner.lifecycle_in_flight = true;
            mark_runtime_unavailable(&mut inner);
        }
        drop(inner);
        if let Some(session) = session.as_mut() {
            terminate_session(session);
        }
        if same_session {
            self.lock().lifecycle_in_flight = false;
        }
        self.lock().status.clone()
    }

    #[cfg(any(unix, windows))]
    fn status_with_reload_error(&self) -> GoRouteIsolatedStatus {
        let mut inner = self.lock();
        inner.status.last_error = Some("Go route configuration could not be updated".into());
        inner.status.clone()
    }

    #[cfg(any(unix, windows))]
    fn finish_start(
        &self,
        recovering: bool,
        generation: u64,
        prepared_plan: Option<RuntimePlan>,
    ) -> GoRouteIsolatedStatus {
        let prepared_start = prepared_plan.is_some();
        let (existing_plan, active_mode) = {
            let inner = self.lock();
            if inner.lifecycle_generation != generation {
                return inner.status.clone();
            }
            (inner.committed_plan.clone(), inner.active_mode)
        };
        let plan = prepared_plan.map(Ok).unwrap_or_else(|| {
            existing_plan.map(Ok).unwrap_or_else(|| {
                active_mode
                    .ok_or_else(|| ERROR_ISOLATED_UNAVAILABLE.to_string())
                    .and_then(|mode| {
                        self.hub
                            .as_ref()
                            .ok_or_else(|| ERROR_ISOLATED_UNAVAILABLE.to_string())
                            .and_then(|hub| build_runtime_plan(hub, mode))
                    })
            })
        });
        let result = plan.and_then(|plan| start_session(&plan).map(|session| (session, plan)));
        match result {
            Ok((mut session, plan)) => {
                let mut inner = self.lock();
                if inner.lifecycle_generation != generation
                    || inner.status.state != "starting"
                    || !inner.desired
                    || inner.stopping
                {
                    drop(inner);
                    terminate_session(&mut session);
                    let mut inner = self.lock();
                    if inner.lifecycle_generation == generation && !inner.desired {
                        let restart_count = inner.status.restart_count;
                        inner.status = stopped_status();
                        inner.status.restart_count = restart_count;
                        inner.committed_plan = None;
                        inner.active_mode = None;
                        inner.lifecycle_in_flight = false;
                        inner.stopping = false;
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
                    edge_statuses: Vec::new(),
                    recovering: false,
                    restart_count: inner.status.restart_count + u32::from(recovering),
                };
                inner.session = Some(session);
                inner.active_mode = Some(plan.mode);
                inner.lifecycle_in_flight = false;
                inner.committed_plan = Some(plan);
                inner.stable_since = Some(Instant::now());
                inner.next_restart_at = None;
                inner.status = status.clone();
                status
            }
            Err(_) => {
                let mut inner = self.lock();
                if inner.lifecycle_generation != generation {
                    return inner.status.clone();
                }
                inner.session = None;
                if !inner.desired {
                    let restart_count = inner.status.restart_count;
                    inner.status = stopped_status();
                    inner.status.restart_count = restart_count;
                    inner.committed_plan = None;
                    inner.active_mode = None;
                    inner.lifecycle_in_flight = false;
                    inner.stopping = false;
                    inner.stable_since = None;
                    inner.next_restart_at = None;
                    return inner.status.clone();
                }
                inner.recovery_budget_used = inner.recovery_budget_used.saturating_add(1);
                inner.lifecycle_in_flight = false;
                inner.stable_since = None;
                if prepared_start {
                    inner.desired = false;
                    inner.active_mode = None;
                    inner.next_restart_at = None;
                } else {
                    inner.next_restart_at =
                        Some(Instant::now() + restart_backoff(inner.recovery_budget_used));
                }
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
                    edge_statuses: Vec::new(),
                    recovering: !prepared_start
                        && inner.desired
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
        #[cfg(not(any(unix, windows)))]
        {
            return platform_unavailable_status();
        }
        #[cfg(any(unix, windows))]
        {
            let owns_stop = {
                let mut inner = self.lock();
                if clear_product_reservation_locked(&mut inner) {
                    let restart_count = inner.status.restart_count;
                    inner.status = stopped_status();
                    inner.status.restart_count = restart_count;
                    return inner.status.clone();
                }
                if inner.stopping {
                    false
                } else {
                    inner.desired = false;
                    inner.stopping = true;
                    inner.next_restart_at = None;
                    inner.stable_since = None;
                    inner.status.recovering = false;
                    true
                }
            };
            if !owns_stop {
                let wait_started = Instant::now();
                loop {
                    let mut inner = self.lock();
                    refresh_locked(&mut inner);
                    if !inner.stopping || wait_started.elapsed() >= START_STOP_WAIT {
                        return inner.status.clone();
                    }
                    drop(inner);
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
            let wait_started = Instant::now();
            loop {
                let mut inner = self.lock();
                refresh_locked(&mut inner);
                if inner.lifecycle_in_flight {
                    if wait_started.elapsed() >= START_STOP_WAIT {
                        return inner.status.clone();
                    }
                    drop(inner);
                    std::thread::sleep(Duration::from_millis(25));
                    continue;
                }
                if let Some(mut session) = inner.session.take() {
                    inner.lifecycle_in_flight = true;
                    drop(inner);
                    stop_session(&mut session);
                    let mut inner = self.lock();
                    let restart_count = inner.status.restart_count;
                    inner.status = stopped_status();
                    inner.status.restart_count = restart_count;
                    inner.committed_plan = None;
                    inner.active_mode = None;
                    inner.lifecycle_in_flight = false;
                    inner.stopping = false;
                    return inner.status.clone();
                }
                if inner.status.state != "starting" {
                    let restart_count = inner.status.restart_count;
                    inner.status = stopped_status();
                    inner.status.restart_count = restart_count;
                    inner.stopping = false;
                    inner.committed_plan = None;
                    inner.active_mode = None;
                    inner.lifecycle_in_flight = false;
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

    #[cfg(any(unix, windows))]
    #[allow(dead_code)]
    fn release_product_reservation(&self, generation: u64) {
        let mut inner = self.lock();
        if inner.prepared_product_generation == Some(generation) {
            clear_product_reservation_locked(&mut inner);
        }
    }

    #[cfg(any(unix, windows))]
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
            let restart_generation = {
                let mut inner = host.lock();
                refresh_locked(&mut inner);
                reset_stable_recovery_budget(&mut inner);
                let retry_due = inner
                    .next_restart_at
                    .map(|deadline| Instant::now() >= deadline)
                    .unwrap_or(true);
                let should = inner.desired
                    && !inner.stopping
                    && !inner.lifecycle_in_flight
                    && inner.prepared_product_generation.is_none()
                    && inner.session.is_none()
                    && inner.status.state != "starting"
                    && inner.recovery_budget_used < MAX_RECOVERY_BUDGET
                    && retry_due;
                if should {
                    inner.status.state = "starting".into();
                    inner.status.recovering = true;
                    inner.lifecycle_generation = inner.lifecycle_generation.wrapping_add(1);
                    inner.lifecycle_in_flight = true;
                    Some(inner.lifecycle_generation)
                } else if inner.recovery_budget_used >= MAX_RECOVERY_BUDGET {
                    inner.status.recovering = false;
                    None
                } else {
                    None
                }
            };
            if let Some(generation) = restart_generation {
                let _ = host.finish_start(true, generation, None);
            }
        });
    }

    #[cfg(any(unix, windows))]
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
                inner.lifecycle_in_flight = true;
                mark_runtime_unavailable(&mut inner);
                drop(inner);
                if let Some(session) = session.as_mut() {
                    terminate_session(session);
                }
                self.lock().lifecycle_in_flight = false;
            }
        }
    }

    #[cfg(any(unix, windows))]
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

#[cfg(all(unix, target_os = "linux", feature = "go-route-tcp-control-probe"))]
fn regular_tree_contains_any(root: &Path, needles: &[&[u8]]) -> Result<bool, String> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_file() {
            if file_contains_any(&path, needles)? {
                return Ok(true);
            }
            continue;
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
                pending.push(entry.map_err(|error| error.to_string())?.path());
            }
        }
    }
    Ok(false)
}

#[cfg(all(unix, target_os = "linux", feature = "go-route-tcp-control-probe"))]
fn file_contains_any(path: &Path, needles: &[&[u8]]) -> Result<bool, String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let overlap = needles
        .iter()
        .map(|needle| needle.len().saturating_sub(1))
        .max()
        .unwrap_or(0);
    let mut retained = Vec::with_capacity(overlap + 8192);
    let mut chunk = [0_u8; 8192];
    loop {
        let read = file.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            return Ok(false);
        }
        retained.extend_from_slice(&chunk[..read]);
        if needles.iter().any(|needle| {
            !needle.is_empty()
                && retained
                    .windows(needle.len())
                    .any(|window| window == *needle)
        }) {
            return Ok(true);
        }
        if retained.len() > overlap {
            retained.drain(..retained.len() - overlap);
        }
    }
}

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
enum OAuthRefreshCompletion {
    ConfigApplied(String),
    NotRefreshed,
}

#[cfg(any(unix, windows))]
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
#[cfg(any(unix, windows))]
#[derive(Clone, PartialEq, Eq, Hash)]
struct OAuthRefreshActionKey {
    instance_epoch: String,
    owner_term: i64,
    active_hash: String,
    source_kind: String,
    source_id: String,
    refresh_kind: String,
}

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
#[derive(Default)]
struct OAuthRefreshActionState {
    running: bool,
    sealed: bool,
    generation: u64,
}

#[cfg(any(unix, windows))]
#[derive(Default)]
struct OAuthRefreshActionGate {
    state: Mutex<OAuthRefreshActionState>,
    completed: Condvar,
}

#[cfg(any(unix, windows))]
#[derive(Default)]
struct OAuthRefreshActionRegistry {
    gates: HashMap<OAuthRefreshActionKey, Arc<OAuthRefreshActionGate>>,
    fifo: VecDeque<OAuthRefreshActionKey>,
}

#[cfg(any(unix, windows))]
#[derive(Default)]
struct OAuthRefreshActionCoordinator {
    registry: Arc<Mutex<OAuthRefreshActionRegistry>>,
}

#[cfg(any(unix, windows))]
enum OAuthRefreshActionRole {
    Sealed,
    Wait(u64),
    Lead(u64),
}

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
struct OAuthRefreshActionLeader {
    gate: Arc<OAuthRefreshActionGate>,
    registry: Arc<Mutex<OAuthRefreshActionRegistry>>,
    generation: u64,
    finished: bool,
}

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
impl Drop for OAuthRefreshActionLeader {
    fn drop(&mut self) {
        if !self.finished {
            self.finish(false);
        }
    }
}

#[cfg(any(unix, windows))]
#[derive(Default)]
struct OAuthRefreshQueueState {
    events: VecDeque<OAuthRefreshEvent>,
    active_refresh_ids: HashSet<String>,
    completed_refresh_ids: HashSet<String>,
    completed_refresh_fifo: VecDeque<(Instant, String)>,
    stopped: bool,
}

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
#[derive(Default)]
struct OAuthRefreshWorkQueue {
    state: Mutex<OAuthRefreshQueueState>,
    event_ready: Condvar,
    space_ready: Condvar,
}

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
enum NextOAuthRefreshError {
    Transport,
    InvalidEvent,
}

#[cfg(any(unix, windows))]
fn next_oauth_refresh(
    control: &ControlSession,
) -> Result<Option<OAuthRefreshEvent>, NextOAuthRefreshError> {
    let reply = post_control_with_timeout(
        &control.endpoint,
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

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
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
        &control.endpoint,
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

#[cfg(any(unix, windows))]
enum CompleteOAuthRefreshError {
    Transport,
    Rejected,
}

#[cfg(not(any(unix, windows)))]
fn platform_unavailable_status() -> GoRouteIsolatedStatus {
    GoRouteIsolatedStatus {
        state: "failed".into(),
        listen_ready: false,
        port: None,
        last_error: Some(ERROR_ISOLATED_UNAVAILABLE.into()),
        home: None,
        lifecycle: Some("failed".into()),
        in_flight_count: 0,
        member_count: 0,
        healthy_member_count: 0,
        edge_statuses: Vec::new(),
        recovering: false,
        restart_count: 0,
    }
}

#[cfg(any(unix, windows))]
fn clear_product_reservation_locked(inner: &mut Inner) -> bool {
    if inner.prepared_product_generation.take().is_none() {
        return false;
    }
    inner.lifecycle_generation = inner.lifecycle_generation.wrapping_add(1);
    inner.active_mode = None;
    true
}

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
fn reset_stable_recovery_budget(inner: &mut Inner) {
    let stable = inner
        .stable_since
        .is_some_and(|started| started.elapsed() >= STABLE_RUN_RESET);
    if stable {
        inner.recovery_budget_used = 0;
        inner.stable_since = None;
    }
}

#[cfg(any(unix, windows))]
fn stop_session(session: &mut Session) {
    let started = Instant::now();
    let _ = post_control_with_timeout(
        &session.endpoint,
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
                #[cfg(windows)]
                session.adapterd.terminate_job();
                session.adapterd.join_stdout_reader();
                #[cfg(windows)]
                session.scratch_handles.clear();
                cleanup_scratch_home(&session.staging_home);
                return;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => break,
        }
    }
    terminate_session(session);
}

#[cfg(any(unix, windows))]
fn terminate_session(session: &mut Session) {
    #[cfg(windows)]
    session.adapterd.terminate_job();
    let _ = session.adapterd.kill();
    let _ = session.adapterd.wait();
    session.adapterd.join_stdout_reader();
    #[cfg(windows)]
    session.scratch_handles.clear();
    cleanup_scratch_home(&session.staging_home);
}

#[cfg(any(unix, windows))]
struct RuntimeConfigSnapshot {
    config: Vec<u8>,
    product_home: Option<ProductHomeLocation>,
}

#[cfg(any(unix, windows))]
fn build_runtime_config(
    hub: &AgentHub,
    mode: GoRouteRunMode,
    expected_port: u16,
) -> Result<RuntimeConfigSnapshot, String> {
    let config = match mode {
        GoRouteRunMode::Isolated => {
            let pools = hub
                .route_pools()
                .list_gateway_listener_pools()
                .map_err(|error| error.to_string())?;
            hub.adapter_bridge()
                .build_go_route_isolated_config(&pools)
                .map_err(|error| error.to_string())?
        }
        GoRouteRunMode::Product => {
            let (port, config) = prepare_product_runtime(hub)?;
            if port != expected_port {
                return Err("saved Go route port changed while the runtime was active".into());
            }
            config
        }
    };
    let product_home = match mode {
        GoRouteRunMode::Isolated => None,
        GoRouteRunMode::Product => Some(resolve_product_home(hub)?),
    };
    Ok(RuntimeConfigSnapshot {
        config,
        product_home,
    })
}

#[cfg(any(unix, windows))]
fn build_runtime_plan(hub: &AgentHub, mode: GoRouteRunMode) -> Result<RuntimePlan, String> {
    let (port, config) = match mode {
        GoRouteRunMode::Isolated => {
            let pools = hub
                .route_pools()
                .list_gateway_listener_pools()
                .map_err(|error| error.to_string())?;
            let config = hub
                .adapter_bridge()
                .build_go_route_isolated_config(&pools)
                .map_err(|error| error.to_string())?;
            (pick_loopback_port()?, config)
        }
        GoRouteRunMode::Product => prepare_product_runtime(hub)?,
    };
    Ok(RuntimePlan {
        config_hash: sha256_hex(&config),
        config,
        port,
        product_home: match mode {
            GoRouteRunMode::Isolated => None,
            GoRouteRunMode::Product => Some(resolve_product_home(hub)?),
        },
        mode,
    })
}

#[cfg(any(unix, windows))]
fn prepare_product_runtime(hub: &AgentHub) -> Result<(u16, Vec<u8>), String> {
    // Product alone receives a durable usage spool. Isolated mode deliberately
    // continues to omit the child field so ad-hoc probes cannot write outside
    // their explicit scratch scope.
    let usage_spool_dir = agenthub_core::utils::paths::usage_gateway_dir_at(hub.data_dir());
    let prepared = hub
        .adapter_bridge()
        .prepare_go_product_config_with_usage_spool(Some(&usage_spool_dir))
        .map_err(|error| error.to_string())?;
    let summary = prepared.summary();
    if !summary.eligible() {
        return Err(format!(
            "product Go route preflight rejected: {}",
            summary.reason.as_str()
        ));
    }
    let port = summary
        .port
        .ok_or_else(|| "eligible Product preflight omitted its saved port".to_string())?;
    let config = prepared
        .into_config()
        .ok_or_else(|| "eligible Product preflight omitted its configuration".to_string())?;
    Ok((port, config))
}

#[cfg(any(unix, windows))]
fn start_session(plan: &RuntimePlan) -> Result<Session, String> {
    if sha256_hex(&plan.config) != plan.config_hash {
        return Err("Go route plan configuration changed after preparation".into());
    }
    let scratch = create_scratch_home()?;
    let staging_home = scratch.home;
    let mut scratch_guard = ScratchHomeGuard {
        home: staging_home.clone(),
        armed: true,
        #[cfg(windows)]
        handles: scratch.handles,
    };
    let mut product_home_handles = Vec::new();
    let home = match plan.mode {
        GoRouteRunMode::Isolated => {
            if plan.product_home.is_some()
                || is_forbidden_user_home(&staging_home)
                || !is_scratch_home(&staging_home)
                || plan.port == PRODUCT_DEFAULT_PORT
            {
                return Err("isolated Go route plan is invalid".into());
            }
            staging_home.clone()
        }
        GoRouteRunMode::Product => {
            let location = plan
                .product_home
                .as_ref()
                .ok_or_else(|| "product Go route home is unavailable".to_string())?;
            let home = &location.home;
            if !home.is_absolute() || plan.port == 0 {
                return Err("product Go route plan is invalid".into());
            }
            product_home_handles = prepare_product_home(location)?.handles;
            home.clone()
        }
    };

    let scratch_root = staging_home.parent().unwrap_or(staging_home.as_path());
    let bin = resolve_adapterd_bin(scratch_root)?;
    let adapterd_log = staging_home.join("logs/adapterd.stdout.log");
    let tcp_control = tcp_control_requested()?;
    let bearer = tcp_control.then(generate_control_bearer).transpose()?;
    let mut command = Command::new(&bin.path);
    command
        .arg("run")
        .arg("--home")
        .arg(&home)
        .arg("--listen-port")
        .arg(plan.port.to_string())
        .arg("--runtime-scope")
        .arg(plan.mode.as_arg())
        .arg("--runtime-config-stdin-stream")
        .stdin(Stdio::piped())
        .env("AGENTHUB_HOME", &home)
        .env_remove(LEGACY_CONTROL_TOKEN_ENV)
        .env_remove("AGENTHUB_ADAPTERD_CONTROL_SOCKET")
        .env_remove("AGENTHUB_ADAPTERD_CONTROL_LISTEN")
        .env_remove("AGENTHUB_ADAPTERD_LISTEN_PORT");
    if tcp_control {
        command
            .arg("--control-listen")
            .arg("127.0.0.1:0")
            .arg("--control-token-stdin");
    }
    let (mut adapterd, endpoint_receiver) = {
        #[cfg(windows)]
        {
            let (child, receiver) = spawn_tcp_control_logged(&mut command, &adapterd_log)
                .map_err(|err| format!("adapterd spawn failed: {err}"))?;
            (child, Some(receiver))
        }
        #[cfg(unix)]
        {
            if tcp_control {
                let (child, receiver) = spawn_tcp_control_logged(&mut command, &adapterd_log)
                    .map_err(|err| format!("adapterd spawn failed: {err}"))?;
                (child, Some(receiver))
            } else {
                (
                    spawn_logged(&mut command, &adapterd_log)
                        .map_err(|err| format!("adapterd spawn failed: {err}"))?,
                    None,
                )
            }
        }
    };
    let Some(config_stdin) = adapterd.stdin.take() else {
        let _ = adapterd.kill();
        let _ = adapterd.wait();
        return Err("adapterd stdin unavailable".into());
    };
    let config_stdin = Arc::new(Mutex::new(config_stdin));
    if let Err(error) = write_startup_input_with_timeout(
        Arc::clone(&config_stdin),
        bearer.clone(),
        plan.config.clone(),
        CONFIG_WRITE_WAIT,
    ) {
        let _ = adapterd.kill();
        let _ = adapterd.wait();
        return Err(format!("adapterd runtime config write failed: {error}"));
    }

    let endpoint = match (bearer, endpoint_receiver) {
        (Some(bearer), Some(receiver)) => match receiver.recv_timeout(CONTROL_START_WAIT) {
            Ok(Ok(address)) => ControlEndpoint::Tcp { address, bearer },
            Ok(Err(error)) => {
                let _ = adapterd.kill();
                let _ = adapterd.wait();
                return Err(error);
            }
            Err(_) => {
                let _ = adapterd.kill();
                let _ = adapterd.wait();
                return Err("timed out waiting for TCP control listener".into());
            }
        },
        #[cfg(unix)]
        (None, None) => ControlEndpoint::Unix(home.join("run/adapterd.sock")),
        _ => {
            let _ = adapterd.kill();
            let _ = adapterd.wait();
            return Err("control transport setup was inconsistent".into());
        }
    };
    let started = match handshake_start(
        &home,
        &endpoint,
        plan.mode,
        plan.port,
        &plan.config_hash,
        bin.package_version,
    ) {
        Ok(session) => {
            scratch_guard.disarm();
            Session {
                home: session.home,
                staging_home,
                endpoint: session.endpoint,
                owner_term: session.owner_term,
                instance_epoch: session.instance_epoch,
                mode: plan.mode,
                port: session.port,
                product_home: plan.product_home.clone(),
                next_owner_renewal: Instant::now() + OWNER_RENEW_INTERVAL,
                oauth_refresh_supported: session.oauth_refresh_supported,
                config_stdin,
                adapterd,
                _product_home_handles: product_home_handles,
                #[cfg(windows)]
                scratch_handles: scratch_guard.take_handles(),
            }
        }
        Err(err) => {
            let _ = adapterd.kill();
            let _ = adapterd.wait();
            return Err(err);
        }
    };
    Ok(started)
}

#[cfg(unix)]
fn tcp_control_requested() -> Result<bool, String> {
    match std::env::var("AGENTHUB_GO_ROUTE_CONTROL_TRANSPORT") {
        Err(std::env::VarError::NotPresent) => Ok(false),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err("Go route control transport is invalid".into())
        }
        Ok(value) if value == "tcp" => {
            if cfg!(feature = "go-route-tcp-control-probe") {
                Ok(true)
            } else {
                Err("TCP Go route control is unavailable in this build".into())
            }
        }
        Ok(_) => Err("Go route control transport is invalid".into()),
    }
}

#[cfg(windows)]
fn tcp_control_requested() -> Result<bool, String> {
    // Windows has no Unix-domain control transport in this supervisor. TCP is
    // mandatory and authenticated with the one-time bearer sent over stdin.
    Ok(true)
}

#[cfg(any(unix, windows))]
fn generate_control_bearer() -> Result<Arc<str>, String> {
    let mut raw = [0_u8; CONTROL_TOKEN_BYTES];
    getrandom::getrandom(&mut raw)
        .map_err(|_| "control authentication token could not be generated".to_string())?;
    let bearer = URL_SAFE_NO_PAD.encode(raw);
    if bearer.len() != CONTROL_TOKEN_ENCODED_BYTES {
        return Err("control authentication token could not be generated".into());
    }
    Ok(Arc::from(bearer))
}

#[cfg(any(unix, windows))]
fn write_control_token_prelude(writer: &mut impl Write, bearer: &str) -> Result<(), String> {
    if bearer.len() != CONTROL_TOKEN_ENCODED_BYTES {
        return Err("control authentication token is invalid".into());
    }
    writer
        .write_all(&(CONTROL_TOKEN_ENCODED_BYTES as u32).to_be_bytes())
        .and_then(|_| writer.write_all(bearer.as_bytes()))
        .map_err(|_| "control authentication token write failed".to_string())
}

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
fn write_startup_input_with_timeout(
    writer: Arc<Mutex<ChildStdin>>,
    bearer: Option<Arc<str>>,
    config: Vec<u8>,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let (sent, received) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("agenthub-go-startup-write".into())
        .spawn(move || {
            let result = {
                let mut writer = writer
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some(bearer) = bearer.as_deref() {
                    write_control_token_prelude(&mut *writer, bearer)?;
                }
                write_runtime_config_frame(&mut *writer, &config)
            };
            let _ = sent.send(result.map(|()| config));
            Ok::<(), String>(())
        })
        .map_err(|_| "startup input writer could not start".to_string())?;
    received
        .recv_timeout(timeout)
        .map_err(|_| "startup input write timed out".to_string())?
}

#[cfg(any(unix, windows))]
fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

struct HandshakeMeta {
    home: PathBuf,
    endpoint: ControlEndpoint,
    owner_term: i64,
    instance_epoch: String,
    port: u16,
    oauth_refresh_supported: bool,
}

#[cfg(any(unix, windows))]
fn handshake_start(
    home: &Path,
    endpoint: &ControlEndpoint,
    mode: GoRouteRunMode,
    fallback_port: u16,
    expected_config_hash: &str,
    expected_package_version: &str,
) -> Result<HandshakeMeta, String> {
    #[cfg(unix)]
    {
        if let ControlEndpoint::Unix(socket) = endpoint {
            wait_for_socket(socket, Duration::from_secs(8))?;
        }
    }
    let home_s = home.display().to_string();
    let hs = post_control(
        endpoint,
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
        endpoint,
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
        endpoint,
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
        endpoint,
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
    let raw_port = status_payload
        .get("port")
        .and_then(Value::as_u64)
        .ok_or_else(|| ERROR_START_FAILED.to_string())?;
    let port = validate_reported_port(mode, fallback_port, raw_port)?;
    if let Some(start_port) = start_payload.get("port") {
        let start_port = start_port
            .as_u64()
            .ok_or_else(|| ERROR_START_FAILED.to_string())?;
        if validate_reported_port(mode, fallback_port, start_port)? != port {
            return Err(ERROR_START_FAILED.into());
        }
    }
    Ok(HandshakeMeta {
        home: home.to_path_buf(),
        endpoint: endpoint.clone(),
        owner_term,
        instance_epoch,
        port,
        oauth_refresh_supported,
    })
}

#[cfg(any(unix, windows))]
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
fn resolve_product_home(hub: &AgentHub) -> Result<ProductHomeLocation, String> {
    let data_dir = hub.data_dir();
    let handle = open_unix_product_anchor(data_dir)?;
    let canonical = fs::canonicalize(data_dir).map_err(|error| error.to_string())?;
    let canonical_handle = open_unix_product_anchor(&canonical)?;
    let identity = unix_product_data_dir_identity(&handle)?;
    if unix_product_data_dir_identity(&canonical_handle)? != identity {
        return Err("product Go route data directory changed during resolution".into());
    }
    Ok(ProductHomeLocation {
        home: canonical.join("runtime").join("adapterd"),
        data_dir: canonical,
        identity,
    })
}

#[cfg(unix)]
fn unix_product_data_dir_identity(handle: &fs::File) -> Result<ProductDataDirIdentity, String> {
    let metadata = handle.metadata().map_err(|error| error.to_string())?;
    Ok(ProductDataDirIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(unix)]
fn prepare_product_home(location: &ProductHomeLocation) -> Result<PreparedProductHome, String> {
    use std::ffi::CString;

    let home = &location.home;
    if home.file_name().and_then(|name| name.to_str()) != Some("adapterd") {
        return Err("product Go route home is invalid".into());
    }
    let runtime = home
        .parent()
        .filter(|path| path.file_name().and_then(|name| name.to_str()) == Some("runtime"))
        .ok_or_else(|| "product Go route home is invalid".to_string())?;
    let data_dir = runtime
        .parent()
        .ok_or_else(|| "product Go route data directory is invalid".to_string())?;
    if data_dir != location.data_dir {
        return Err("product Go route data directory drifted".into());
    }

    let data_handle = open_unix_product_anchor(data_dir)?;
    if unix_product_data_dir_identity(&data_handle)? != location.identity {
        return Err("product Go route data directory drifted".into());
    }
    let runtime_handle = open_or_create_unix_product_directory(
        &data_handle,
        &CString::new("runtime").expect("static component"),
    )?;
    let home_handle = open_or_create_unix_product_directory(
        &runtime_handle,
        &CString::new("adapterd").expect("static component"),
    )?;
    let mut child_handles = Vec::new();
    for component in ["run", "config", "logs"] {
        child_handles.push(open_or_create_unix_product_directory(
            &home_handle,
            &CString::new(component).expect("static component"),
        )?);
    }
    let mut handles = vec![data_handle, runtime_handle, home_handle];
    handles.extend(child_handles);
    Ok(PreparedProductHome { handles })
}

#[cfg(unix)]
fn open_unix_product_anchor(path: &Path) -> Result<fs::File, String> {
    let before = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !before.file_type().is_dir() || before.uid() != current_euid() || before.mode() & 0o022 != 0
    {
        return Err("product Go route data directory is not private".into());
    }
    let handle = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|error| error.to_string())?;
    let after = handle.metadata().map_err(|error| error.to_string())?;
    if !after.file_type().is_dir()
        || after.uid() != current_euid()
        || after.mode() & 0o022 != 0
        || before.dev() != after.dev()
        || before.ino() != after.ino()
    {
        return Err("product Go route data directory changed during validation".into());
    }
    Ok(handle)
}

#[cfg(unix)]
fn open_or_create_unix_product_directory(
    parent: &fs::File,
    component: &std::ffi::CStr,
) -> Result<fs::File, String> {
    let created = match unsafe { libc::mkdirat(parent.as_raw_fd(), component.as_ptr(), 0o700) } {
        0 => true,
        _ if std::io::Error::last_os_error().kind() == std::io::ErrorKind::AlreadyExists => false,
        _ => return Err(std::io::Error::last_os_error().to_string()),
    };
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            component.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // SAFETY: openat returned a new owned descriptor on success.
    let handle = unsafe { fs::File::from_raw_fd(fd) };
    if created && unsafe { libc::fchmod(handle.as_raw_fd(), 0o700) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let metadata = handle.metadata().map_err(|error| error.to_string())?;
    if !metadata.file_type().is_dir()
        || metadata.uid() != current_euid()
        || metadata.mode() & 0o777 != 0o700
    {
        return Err("product Go route directory is not private".into());
    }
    Ok(handle)
}

#[cfg(windows)]
fn resolve_product_home(hub: &AgentHub) -> Result<ProductHomeLocation, String> {
    let data_dir = hub.data_dir();
    let handle = open_windows_path_no_reparse(data_dir, true)?;
    let canonical = fs::canonicalize(data_dir).map_err(|error| error.to_string())?;
    let canonical_handle = open_windows_path_no_reparse(&canonical, true)?;
    let identity = windows_product_data_dir_identity(&handle)?;
    if windows_product_data_dir_identity(&canonical_handle)? != identity {
        return Err("product Go route data directory changed during resolution".into());
    }
    Ok(ProductHomeLocation {
        home: canonical.join("runtime").join("adapterd"),
        data_dir: canonical,
        identity,
    })
}

#[cfg(windows)]
fn windows_product_data_dir_identity(
    handle: &impl AsRawHandle,
) -> Result<ProductDataDirIdentity, String> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(handle.as_raw_handle() as _, &mut info) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(ProductDataDirIdentity {
        volume_serial: info.dwVolumeSerialNumber,
        file_index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    })
}

#[cfg(windows)]
fn prepare_product_home(location: &ProductHomeLocation) -> Result<PreparedProductHome, String> {
    let home = &location.home;
    if home.file_name().and_then(|name| name.to_str()) != Some("adapterd") {
        return Err("product Go route home is invalid".into());
    }
    let runtime = home
        .parent()
        .filter(|path| path.file_name().and_then(|name| name.to_str()) == Some("runtime"))
        .ok_or_else(|| "product Go route home is invalid".to_string())?;
    let data_dir = runtime
        .parent()
        .ok_or_else(|| "product Go route data directory is invalid".to_string())?;
    if data_dir != location.data_dir {
        return Err("product Go route data directory drifted".into());
    }

    let data_handle = open_windows_path_no_reparse(data_dir, true)?;
    if windows_product_data_dir_identity(&data_handle)? != location.identity {
        return Err("product Go route data directory drifted".into());
    }
    let mut handles = vec![data_handle];
    for directory in [
        runtime.to_path_buf(),
        home.to_path_buf(),
        home.join("run"),
        home.join("config"),
        home.join("logs"),
    ] {
        match create_private_windows_directory(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
        let handle = open_windows_path_no_reparse(&directory, true)?;
        verify_windows_protected_dacl(&handle)?;
        handles.push(handle);
    }
    Ok(PreparedProductHome { handles })
}

#[cfg(unix)]
fn create_scratch_home() -> Result<CreatedScratchHome, String> {
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
    Ok(CreatedScratchHome { home })
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

#[cfg(windows)]
fn windows_wide(value: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn windows_has_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(windows)]
fn windows_has_single_link(handle: &impl AsRawHandle) -> bool {
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    (unsafe { GetFileInformationByHandle(handle.as_raw_handle() as _, &mut info) }) != 0
        && info.nNumberOfLinks == 1
}

#[cfg(windows)]
fn create_private_windows_directory(path: &Path) -> std::io::Result<()> {
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;

    let sddl = windows_wide(std::ffi::OsStr::new("D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)"));
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if converted == 0 || descriptor.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let wide = windows_wide(path.as_os_str());
    let created = unsafe { CreateDirectoryW(wide.as_ptr(), &attributes) };
    let error = if created == 0 {
        Some(std::io::Error::last_os_error())
    } else {
        None
    };
    unsafe {
        let _ = LocalFree(descriptor as _);
    }
    if let Some(error) = error {
        return Err(error);
    }
    Ok(())
}

#[cfg(windows)]
fn verify_windows_protected_dacl(handle: &impl AsRawHandle) -> Result<(), String> {
    use windows_sys::Win32::Foundation::{LocalFree, ERROR_SUCCESS};
    use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        AclSizeInformation, GetAclInformation, GetSecurityDescriptorControl,
        GetSecurityDescriptorDacl, ACL_SIZE_INFORMATION, DACL_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, SE_DACL_PROTECTED,
    };

    let expected_dacl = expected_windows_scratch_dacl()?;
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let result = unsafe {
        GetSecurityInfo(
            handle.as_raw_handle() as _,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if result != ERROR_SUCCESS || descriptor.is_null() {
        return Err("scratch directory security could not be verified".into());
    }
    let verified = (|| {
        let mut control = 0_u16;
        let mut revision = 0_u32;
        if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0
            || control & SE_DACL_PROTECTED == 0
        {
            return false;
        }
        let mut present = 0;
        let mut defaulted = 0;
        let mut dacl = std::ptr::null_mut();
        if unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) }
            == 0
            || present == 0
            || defaulted != 0
            || dacl.is_null()
        {
            return false;
        }
        let mut info = ACL_SIZE_INFORMATION::default();
        let acl_valid = unsafe {
            GetAclInformation(
                dacl,
                &mut info as *mut _ as _,
                std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            ) != 0
        };
        if !acl_valid || info.AceCount != 2 || info.AclBytesInUse as usize != expected_dacl.len() {
            return false;
        }
        let actual =
            unsafe { std::slice::from_raw_parts(dacl.cast::<u8>(), info.AclBytesInUse as usize) };
        actual == expected_dacl.as_slice()
    })();
    unsafe {
        let _ = LocalFree(descriptor as _);
    }
    if !verified {
        return Err("scratch directory security could not be verified".into());
    }
    Ok(())
}

#[cfg(windows)]
fn expected_windows_scratch_dacl() -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        AclSizeInformation, GetAclInformation, GetSecurityDescriptorDacl, ACL_SIZE_INFORMATION,
        PSECURITY_DESCRIPTOR,
    };

    let sddl = windows_wide(std::ffi::OsStr::new("D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)"));
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if converted == 0 || descriptor.is_null() {
        return Err("scratch directory security could not be verified".into());
    }
    let result = (|| {
        let mut present = 0;
        let mut defaulted = 0;
        let mut dacl = std::ptr::null_mut();
        if unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) }
            == 0
            || present == 0
            || defaulted != 0
            || dacl.is_null()
        {
            return Err("scratch directory security could not be verified".into());
        }
        let mut info = ACL_SIZE_INFORMATION::default();
        if unsafe {
            GetAclInformation(
                dacl,
                &mut info as *mut _ as _,
                std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            )
        } == 0
            || info.AclBytesInUse == 0
        {
            return Err("scratch directory security could not be verified".into());
        }
        Ok(unsafe {
            std::slice::from_raw_parts(dacl.cast::<u8>(), info.AclBytesInUse as usize).to_vec()
        })
    })();
    unsafe {
        let _ = LocalFree(descriptor as _);
    }
    result
}

#[cfg(windows)]
fn open_windows_path_no_reparse(path: &Path, directory: bool) -> Result<fs::File, String> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_READ_DATA, FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL,
    };

    let flags = FILE_FLAG_OPEN_REPARSE_POINT
        | if directory {
            FILE_FLAG_BACKUP_SEMANTICS
        } else {
            0
        };
    let file = OpenOptions::new()
        .access_mode(
            FILE_READ_ATTRIBUTES | READ_CONTROL | if directory { 0 } else { FILE_READ_DATA },
        )
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(flags)
        .open(path)
        .map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if windows_has_reparse_point(&metadata)
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err("scratch path is not a plain filesystem object".into());
    }
    Ok(file)
}

#[cfg(windows)]
fn windows_scratch_marker_contents(pid: u32, nanos: u128, nonce: &str) -> String {
    format!("agenthub-go-route-owner-v1\npid={pid}\ncreated_unix_nanos={nanos}\nnonce={nonce}\n")
}

#[cfg(windows)]
fn parse_windows_scratch_root_name(name: &str) -> Option<(u32, u128, String)> {
    let mut parts = name.strip_prefix(SCRATCH_ROOT_PREFIX)?.split('-');
    let pid = parts.next()?.parse::<u32>().ok()?;
    let nanos = parts.next()?.parse::<u128>().ok()?;
    let nonce = parts.next()?.to_owned();
    if pid == 0
        || nonce.len() != 32
        || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
        || parts.next().is_some()
    {
        return None;
    }
    Some((pid, nanos, nonce))
}

#[cfg(windows)]
fn cleanup_empty_windows_scratch_root(root: &Path) {
    // Used only before any child object exists. Non-recursive removal avoids
    // following an attacker-controlled replacement if validation failed.
    let _ = fs::remove_dir(root);
}

#[cfg(windows)]
fn create_scratch_home() -> Result<CreatedScratchHome, String> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;

    let temp = fs::canonicalize(std::env::temp_dir()).map_err(|error| error.to_string())?;
    if windows_has_reparse_point(&fs::symlink_metadata(&temp).map_err(|error| error.to_string())?) {
        return Err("operating-system temp directory is a reparse point".into());
    }
    for _ in 0..16_u32 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut random = [0_u8; 16];
        getrandom::getrandom(&mut random)
            .map_err(|_| "scratch directory randomness is unavailable".to_string())?;
        let nonce = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let root = temp.join(format!(
            "{SCRATCH_ROOT_PREFIX}{}-{nanos}-{nonce}",
            std::process::id()
        ));
        match create_private_windows_directory(&root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
        let root_file = match open_windows_path_no_reparse(&root, true) {
            Ok(file) => file,
            Err(error) => {
                cleanup_empty_windows_scratch_root(&root);
                return Err(error);
            }
        };
        if let Err(error) = verify_windows_protected_dacl(&root_file) {
            drop(root_file);
            cleanup_empty_windows_scratch_root(&root);
            return Err(error);
        }
        let mut handles = vec![root_file.into()];
        let marker_path = root.join(SCRATCH_OWNER_MARKER);
        let marker_result = (|| {
            let mut marker = OpenOptions::new()
                .write(true)
                .create_new(true)
                .share_mode(0)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&marker_path)?;
            marker.write_all(
                windows_scratch_marker_contents(std::process::id(), nanos, &nonce).as_bytes(),
            )?;
            marker.sync_all()?;
            if windows_has_reparse_point(&marker.metadata()?) {
                return Err(std::io::Error::other("marker is a reparse point"));
            }
            Ok::<(), std::io::Error>(())
        })();
        if let Err(error) = marker_result {
            // The verified root handle excludes root replacement while the
            // incomplete marker is removed. The root is removed only after
            // that handle is released, and only if it is empty.
            let _ = fs::remove_file(&marker_path);
            handles.clear();
            cleanup_empty_windows_scratch_root(&root);
            return Err(error.to_string());
        }
        for directory in ["home", "home/config", "home/run", "home/logs"] {
            let path = root.join(directory);
            if let Err(error) = create_private_windows_directory(&path) {
                handles.clear();
                cleanup_windows_scratch_root(&root);
                return Err(error.to_string());
            }
            let file = match open_windows_path_no_reparse(&path, true) {
                Ok(file) => file,
                Err(error) => {
                    handles.clear();
                    cleanup_windows_scratch_root(&root);
                    return Err(error);
                }
            };
            if let Err(error) = verify_windows_protected_dacl(&file) {
                handles.clear();
                cleanup_windows_scratch_root(&root);
                return Err(error);
            }
            handles.push(file.into());
        }
        let home = match fs::canonicalize(root.join("home")) {
            Ok(home) => home,
            Err(error) => {
                handles.clear();
                cleanup_windows_scratch_root(&root);
                return Err(error.to_string());
            }
        };
        if !is_scratch_home(&home) {
            handles.clear();
            cleanup_windows_scratch_root(&root);
            return Err("scratch home escaped the operating-system temp directory".into());
        }
        return Ok(CreatedScratchHome { home, handles });
    }
    Err("could not create isolated runtime directory".into())
}

#[cfg(windows)]
fn validate_windows_scratch_root(root: &Path) -> Option<(u32, SystemTime, OwnedHandle)> {
    let temp = fs::canonicalize(std::env::temp_dir()).ok()?;
    let resolved_root = fs::canonicalize(root).ok()?;
    if resolved_root.parent() != Some(temp.as_path()) {
        return None;
    }
    let (pid, nanos, nonce) =
        parse_windows_scratch_root_name(resolved_root.file_name()?.to_str()?)?;
    let file = open_windows_path_no_reparse(&resolved_root, true).ok()?;
    let handle: OwnedHandle = file.into();
    verify_windows_protected_dacl(&handle).ok()?;
    let marker_path = resolved_root.join(SCRATCH_OWNER_MARKER);
    let mut marker = open_windows_path_no_reparse(&marker_path, false).ok()?;
    if marker.metadata().ok()?.len() > 256 {
        return None;
    }
    let mut contents = String::new();
    marker.read_to_string(&mut contents).ok()?;
    if contents != windows_scratch_marker_contents(pid, nanos, &nonce) {
        return None;
    }
    Some((pid, marker.metadata().ok()?.modified().ok()?, handle))
}

#[cfg(windows)]
fn windows_tree_is_plain(root: &Path) -> bool {
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            return false;
        };
        if windows_has_reparse_point(&metadata) {
            return false;
        }
        if metadata.is_dir() {
            let Ok(entries) = fs::read_dir(path) else {
                return false;
            };
            for entry in entries {
                let Ok(entry) = entry else { return false };
                pending.push(entry.path());
            }
        }
    }
    true
}

#[cfg(windows)]
fn scratch_root_for_home(path: &Path) -> Option<PathBuf> {
    let resolved = fs::canonicalize(path).ok()?;
    if resolved.file_name().and_then(|name| name.to_str()) != Some("home") {
        return None;
    }
    let root = resolved.parent()?.to_path_buf();
    let (_, _, handle) = validate_windows_scratch_root(&root)?;
    drop(handle);
    Some(root)
}

#[cfg(windows)]
fn is_scratch_home(path: &Path) -> bool {
    scratch_root_for_home(path).is_some()
}

#[cfg(windows)]
fn is_forbidden_user_home(path: &Path) -> bool {
    let Ok(real) = agenthub_core::utils::paths::default_data_dir() else {
        return false;
    };
    let real = fs::canonicalize(&real).unwrap_or(real);
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    resolved == real || resolved.starts_with(&real)
}

#[cfg(windows)]
fn cleanup_scratch_home(home: &Path) {
    let Some(root) = scratch_root_for_home(home) else {
        return;
    };
    cleanup_windows_scratch_root(&root);
}

#[cfg(windows)]
fn cleanup_windows_scratch_root(root: &Path) {
    if !windows_tree_is_plain(root) {
        return;
    }
    let Some((_, _, handle)) = validate_windows_scratch_root(root) else {
        return;
    };
    drop(handle);
    if !windows_tree_is_plain(root) {
        return;
    }
    let Some((_, _, handle)) = validate_windows_scratch_root(root) else {
        return;
    };
    drop(handle);
    let _ = fs::remove_dir_all(root);
}

#[cfg(windows)]
fn process_is_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::STILL_ACTIVE;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        // Only ERROR_INVALID_PARAMETER proves the PID does not exist. Access
        // denial or another transient error must fail closed as "alive".
        return std::io::Error::last_os_error().raw_os_error() != Some(87);
    }
    // SAFETY: OpenProcess returned a unique owned handle.
    let handle = unsafe { OwnedHandle::from_raw_handle(handle as _) };
    let mut exit_code = 0_u32;
    if unsafe { GetExitCodeProcess(handle.as_raw_handle() as _, &mut exit_code) } == 0 {
        return true;
    }
    exit_code == STILL_ACTIVE as u32
}

#[cfg(windows)]
fn cleanup_stale_scratch_roots() {
    let Ok(temp) = fs::canonicalize(std::env::temp_dir()) else {
        return;
    };
    let Ok(entries) = fs::read_dir(&temp) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let root = entry.path();
        let Some((pid, created, handle)) = validate_windows_scratch_root(&root) else {
            continue;
        };
        let old_enough = now
            .duration_since(created)
            .is_ok_and(|age| age >= STALE_SCRATCH_MIN_AGE);
        drop(handle);
        if old_enough && !process_is_alive(pid) {
            cleanup_windows_scratch_root(&root);
        }
    }
}

#[cfg(any(unix, windows))]
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

#[cfg(windows)]
fn bundled_adapterd(scratch_root: &Path) -> Result<ResolvedAdapterd, String> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};

    if BUNDLED_SHA256.len() != 64 || EMBEDDED_BUNDLED_VERSION != BUNDLED_PACKAGE_VERSION {
        return Err("bundled Go route identity is unavailable".into());
    }
    let exe = std::env::current_exe()
        .map_err(|_| "bundled Go route location is unavailable".to_string())?;
    let directory = exe
        .parent()
        .ok_or_else(|| "bundled Go route location is unavailable".to_string())?;
    let source_path = directory.join("agenthub-adapterd.exe");
    let mut source = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&source_path)
        .map_err(|_| "bundled Go route is unavailable".to_string())?;
    let source_metadata = source
        .metadata()
        .map_err(|_| "bundled Go route is unavailable".to_string())?;
    if !source_metadata.is_file()
        || windows_has_reparse_point(&source_metadata)
        || !windows_has_single_link(&source)
    {
        return Err("bundled Go route is unavailable".into());
    }
    if sha256_file(&mut source)? != BUNDLED_SHA256 {
        return Err("bundled Go route integrity check failed".into());
    }

    let dest_dir = scratch_root.join("bin");
    let dest_dir_handle = ensure_private_windows_directory(&dest_dir)
        .map_err(|_| "bundled Go route staging failed".to_string())?;
    let dest_path = dest_dir.join("agenthub-adapterd.exe");
    let mut dest = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&dest_path)
        .map_err(|_| "bundled Go route staging failed".to_string())?;
    std::io::copy(&mut source, &mut dest)
        .and_then(|_| dest.sync_all())
        .map_err(|_| "bundled Go route staging failed".to_string())?;
    let dest_metadata = dest
        .metadata()
        .map_err(|_| "bundled Go route staging failed".to_string())?;
    if !dest_metadata.is_file()
        || windows_has_reparse_point(&dest_metadata)
        || !windows_has_single_link(&dest)
        || dest_metadata.len() != source_metadata.len()
        || sha256_file(&mut dest)? != BUNDLED_SHA256
    {
        return Err("bundled Go route staged integrity check failed".into());
    }
    drop(dest);
    drop(source);
    let mut verified = hold_verified_windows_executable(&dest_path)?;
    let verified_metadata = verified
        .metadata()
        .map_err(|_| "bundled Go route staged integrity check failed".to_string())?;
    if !windows_has_single_link(&verified)
        || verified_metadata.len() != source_metadata.len()
        || sha256_file(&mut verified)? != BUNDLED_SHA256
    {
        return Err("bundled Go route staged integrity check failed".into());
    }
    Ok(ResolvedAdapterd {
        path: dest_path,
        package_version: BUNDLED_PACKAGE_VERSION,
        _verified_handles: vec![dest_dir_handle, verified],
    })
}

#[cfg(windows)]
fn ensure_private_windows_directory(path: &Path) -> Result<fs::File, String> {
    match create_private_windows_directory(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.to_string()),
    }
    let file = open_windows_path_no_reparse(path, true)?;
    verify_windows_protected_dacl(&file)?;
    Ok(file)
}

#[cfg(windows)]
fn hold_verified_windows_executable(path: &Path) -> Result<fs::File, String> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};

    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| "bundled Go route staged integrity check failed".to_string())?;
    let metadata = file
        .metadata()
        .map_err(|_| "bundled Go route staged integrity check failed".to_string())?;
    if !metadata.is_file()
        || windows_has_reparse_point(&metadata)
        || !windows_has_single_link(&file)
    {
        return Err("bundled Go route staged integrity check failed".into());
    }
    Ok(file)
}

#[cfg(any(unix, windows))]
fn resolve_adapterd_bin(_scratch_root: &Path) -> Result<ResolvedAdapterd, String> {
    #[cfg(debug_assertions)]
    if let Ok(raw) = std::env::var("AGENTHUB_ADAPTERD_BIN") {
        let path = PathBuf::from(raw);
        if path.is_file() {
            return Ok(ResolvedAdapterd {
                #[cfg(windows)]
                _verified_handles: vec![hold_verified_windows_executable(&path)?],
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
        #[cfg(unix)]
        fs::create_dir_all(&dest_dir).map_err(|err| err.to_string())?;
        #[cfg(windows)]
        let dest_dir_handle = ensure_private_windows_directory(&dest_dir)?;
        let dest = dest_dir.join(if cfg!(windows) {
            "agenthub-adapterd.exe"
        } else {
            "agenthub-adapterd"
        });
        let mut command = Command::new("go");
        command
            .arg("build")
            .arg("-trimpath")
            .arg("-buildvcs=false")
            .arg("-o")
            .arg(&dest)
            .current_dir(&src);
        agenthub_core::utils::process::apply_no_window(&mut command);
        let status = command.status().map_err(|err| format!("go build: {err}"))?;
        if !status.success() {
            return Err("go build agenthub-adapterd failed".into());
        }
        #[cfg(windows)]
        let verified = hold_verified_windows_executable(&dest)?;
        Ok(ResolvedAdapterd {
            path: dest,
            package_version: ISOLATED_DEV_PACKAGE_VERSION,
            #[cfg(windows)]
            _verified_handles: vec![dest_dir_handle, verified],
        })
    }
}

#[cfg(all(any(unix, windows), debug_assertions))]
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
fn spawn_logged(cmd: &mut Command, log_path: &Path) -> Result<AdapterdProcess, String> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|err| err.to_string())?;
    let err_file = file.try_clone().map_err(|err| err.to_string())?;
    let child = cmd
        .stdout(Stdio::from(file))
        .stderr(Stdio::from(err_file))
        .spawn()
        .map_err(|err| err.to_string())?;
    Ok(AdapterdProcess {
        child,
        stdout_reader: None,
    })
}

#[cfg(unix)]
fn spawn_adapterd_process(cmd: &mut Command) -> Result<AdapterdProcess, String> {
    let child = cmd.spawn().map_err(|error| error.to_string())?;
    Ok(AdapterdProcess {
        child,
        stdout_reader: None,
    })
}

#[cfg(windows)]
fn spawn_adapterd_process(cmd: &mut Command) -> Result<AdapterdProcess, String> {
    use std::mem::size_of;
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{
        OpenThread, ResumeThread, CREATE_NO_WINDOW, CREATE_SUSPENDED, THREAD_SUSPEND_RESUME,
    };

    // Start suspended so no adapter code or descendant can run before the
    // process is attached to the kill-on-close Job Object.
    let raw_job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if raw_job.is_null() {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // SAFETY: CreateJobObjectW returned a unique owned handle.
    let job = WindowsJob {
        handle: unsafe { OwnedHandle::from_raw_handle(raw_job as _) },
    };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let configured = unsafe {
        SetInformationJobObject(
            job.handle.as_raw_handle() as _,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as _,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if configured == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }

    cmd.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
    let mut child = cmd.spawn().map_err(|error| error.to_string())?;
    let assigned = unsafe {
        AssignProcessToJobObject(job.handle.as_raw_handle() as _, child.as_raw_handle() as _)
    };
    if assigned == 0 {
        let error = std::io::Error::last_os_error();
        unsafe {
            let _ = TerminateJobObject(job.handle.as_raw_handle() as _, 1);
        }
        let _ = child.kill();
        let _ = child.wait();
        return Err(error.to_string());
    }

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
        let error = std::io::Error::last_os_error();
        unsafe {
            let _ = TerminateJobObject(job.handle.as_raw_handle() as _, 1);
        }
        let _ = child.kill();
        let _ = child.wait();
        return Err(error.to_string());
    }
    // SAFETY: the snapshot handle is unique and owned here.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot as _) };
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        ..THREADENTRY32::default()
    };
    let mut resumed = false;
    let mut has_entry = unsafe { Thread32First(snapshot.as_raw_handle() as _, &mut entry) } != 0;
    while has_entry {
        if entry.th32OwnerProcessID == child.id() {
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if !thread.is_null() {
                // SAFETY: OpenThread returned an owned handle.
                let thread = unsafe { OwnedHandle::from_raw_handle(thread as _) };
                if unsafe { ResumeThread(thread.as_raw_handle() as _) } != u32::MAX {
                    resumed = true;
                    break;
                }
            }
        }
        has_entry = unsafe { Thread32Next(snapshot.as_raw_handle() as _, &mut entry) } != 0;
    }
    if !resumed {
        unsafe {
            let _ = TerminateJobObject(job.handle.as_raw_handle() as _, 1);
        }
        let _ = child.kill();
        let _ = child.wait();
        return Err("adapterd suspended process could not be resumed".into());
    }
    Ok(AdapterdProcess {
        child,
        stdout_reader: None,
        _job: job,
    })
}

#[cfg(any(unix, windows))]
fn spawn_tcp_control_logged(
    cmd: &mut Command,
    log_path: &Path,
) -> Result<
    (
        AdapterdProcess,
        std::sync::mpsc::Receiver<Result<SocketAddrV4, String>>,
    ),
    String,
> {
    let stderr_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|err| err.to_string())?;
    cmd.stdout(Stdio::piped()).stderr(Stdio::from(stderr_file));
    let mut child = spawn_adapterd_process(cmd)?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("adapterd stdout unavailable".into());
    };
    let log_path = log_path.to_path_buf();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let reader = match std::thread::Builder::new()
        .name("agenthub-go-control-stdout".into())
        .spawn(move || drain_tcp_control_stdout(stdout, &log_path, sender))
    {
        Ok(reader) => reader,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("adapterd stdout reader could not start".into());
        }
    };
    child.stdout_reader = Some(reader);
    Ok((child, receiver))
}

#[cfg(any(unix, windows))]
fn drain_tcp_control_stdout(
    mut stdout: std::process::ChildStdout,
    log_path: &Path,
    sender: std::sync::mpsc::SyncSender<Result<SocketAddrV4, String>>,
) {
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .ok();
    let mut log_bytes = 0_usize;
    let mut line = Vec::with_capacity(256);
    let mut line_too_long = false;
    let mut result_sent = false;
    let mut chunk = [0_u8; 1024];

    loop {
        let read = match stdout.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => read,
            Err(_) => {
                if !result_sent {
                    let _ =
                        sender.send(Err("TCP control listener output could not be read".into()));
                    result_sent = true;
                }
                break;
            }
        };
        if let Some(file) = log.as_mut() {
            let remaining = MAX_CONTROL_STDOUT_LOG_BYTES.saturating_sub(log_bytes);
            let write_len = remaining.min(read);
            if write_len > 0 && file.write_all(&chunk[..write_len]).is_ok() {
                log_bytes += write_len;
            }
        }
        for &byte in &chunk[..read] {
            if byte == b'\n' {
                if !result_sent {
                    let parsed = if line_too_long {
                        Err("TCP control listener output line was too long".into())
                    } else {
                        parse_tcp_control_listener_line(&line)
                    };
                    match parsed {
                        Ok(None) => {}
                        Ok(Some(address)) => {
                            let _ = sender.send(Ok(address));
                            result_sent = true;
                        }
                        Err(error) => {
                            let _ = sender.send(Err(error));
                            result_sent = true;
                        }
                    }
                }
                line.clear();
                line_too_long = false;
            } else if line.len() < MAX_CONTROL_STDOUT_LINE_BYTES {
                line.push(byte);
            } else {
                line_too_long = true;
            }
        }
    }
    if !result_sent {
        let parsed = if line_too_long {
            Err("TCP control listener output line was too long".into())
        } else {
            parse_tcp_control_listener_line(&line)
        };
        let result = match parsed {
            Ok(Some(address)) => Ok(address),
            Ok(None) => Err("TCP control listener was not reported".into()),
            Err(error) => Err(error),
        };
        let _ = sender.send(result);
    }
}

#[cfg(any(unix, windows))]
fn parse_tcp_control_listener_line(line: &[u8]) -> Result<Option<SocketAddrV4>, String> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let Ok(line) = std::str::from_utf8(line) else {
        return Err("TCP control listener output was invalid".into());
    };
    if !line.starts_with("agenthub-adapterd control listener:") {
        return Ok(None);
    }
    let Some(address) = line.strip_prefix(TCP_CONTROL_STDOUT_PREFIX) else {
        return Err("TCP control listener output was invalid".into());
    };
    let parsed = address
        .parse::<SocketAddrV4>()
        .map_err(|_| "TCP control listener output was invalid".to_string())?;
    if parsed.ip() != &Ipv4Addr::LOCALHOST || parsed.port() == 0 || parsed.to_string() != address {
        return Err("TCP control listener output was invalid".into());
    }
    Ok(Some(parsed))
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

#[cfg(any(unix, windows))]
fn post_control(endpoint: &ControlEndpoint, body: &Value) -> Result<Value, String> {
    post_control_with_timeout(endpoint, body, Duration::from_secs(8))
}

#[cfg(any(unix, windows))]
fn post_control_with_timeout(
    endpoint: &ControlEndpoint,
    body: &Value,
    timeout: Duration,
) -> Result<Value, String> {
    match endpoint {
        #[cfg(unix)]
        ControlEndpoint::Unix(socket) => post_unix_control(socket, body, timeout),
        ControlEndpoint::Tcp { address, bearer } => {
            post_tcp_control(*address, bearer, body, timeout)
        }
    }
}

#[cfg(unix)]
fn post_unix_control(socket: &Path, body: &Value, timeout: Duration) -> Result<Value, String> {
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

#[cfg(any(unix, windows))]
fn post_tcp_control(
    address: SocketAddrV4,
    bearer: &str,
    body: &Value,
    timeout: Duration,
) -> Result<Value, String> {
    let raw = serde_json::to_vec(body).map_err(|err| err.to_string())?;
    let started = Instant::now();
    let mut stream = TcpStream::connect_timeout(&SocketAddr::V4(address), timeout)
        .map_err(|_| ERROR_CONTROL_UNAVAILABLE.to_string())?;
    let remaining = timeout
        .checked_sub(started.elapsed())
        .filter(|remaining| !remaining.is_zero())
        .unwrap_or(Duration::from_millis(1));
    stream
        .set_read_timeout(Some(remaining))
        .map_err(|_| ERROR_CONTROL_UNAVAILABLE.to_string())?;
    stream
        .set_write_timeout(Some(remaining))
        .map_err(|_| ERROR_CONTROL_UNAVAILABLE.to_string())?;
    let header = format!(
        "POST /control HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        address,
        bearer,
        raw.len()
    );
    stream
        .write_all(header.as_bytes())
        .and_then(|_| stream.write_all(&raw))
        .and_then(|_| stream.flush())
        .map_err(|_| ERROR_CONTROL_UNAVAILABLE.to_string())?;

    let mut response = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 4096];
    loop {
        let remaining = timeout
            .checked_sub(started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| ERROR_CONTROL_UNAVAILABLE.to_string())?;
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|_| ERROR_CONTROL_UNAVAILABLE.to_string())?;
        let read = stream
            .read(&mut chunk)
            .map_err(|_| ERROR_CONTROL_UNAVAILABLE.to_string())?;
        if read == 0 {
            break;
        }
        if response.len().saturating_add(read) > MAX_CONTROL_RESPONSE_BYTES {
            return Err(ERROR_CONTROL_UNAVAILABLE.into());
        }
        response.extend_from_slice(&chunk[..read]);
        if find_http_header_end(&response).is_none()
            && response.len() > MAX_CONTROL_RESPONSE_HEADER_BYTES
        {
            return Err(ERROR_CONTROL_UNAVAILABLE.into());
        }
    }
    let header_end =
        find_http_header_end(&response).ok_or_else(|| ERROR_CONTROL_UNAVAILABLE.to_string())?;
    if header_end > MAX_CONTROL_RESPONSE_HEADER_BYTES {
        return Err(ERROR_CONTROL_UNAVAILABLE.into());
    }
    let status_line_end = response
        .windows(2)
        .position(|window| window == b"\r\n")
        .ok_or_else(|| ERROR_CONTROL_UNAVAILABLE.to_string())?;
    let status_line = std::str::from_utf8(&response[..status_line_end])
        .map_err(|_| ERROR_CONTROL_UNAVAILABLE.to_string())?;
    let mut status_parts = status_line.split_ascii_whitespace();
    let version = status_parts.next();
    let status = status_parts.next();
    if !matches!(version, Some("HTTP/1.0" | "HTTP/1.1")) || !matches!(status, Some("200" | "400")) {
        return Err(ERROR_CONTROL_UNAVAILABLE.into());
    }
    parse_http_json(&response)
}

fn find_http_header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
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

#[cfg(any(unix, windows))]
fn session_status(session: &ControlSession) -> Result<Value, String> {
    let reply = post_control(
        &session.endpoint,
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
    let payload = require_ok(&reply)?.clone();
    let raw_port = payload
        .get("port")
        .and_then(Value::as_u64)
        .ok_or_else(|| ERROR_CONTROL_UNAVAILABLE.to_string())?;
    validate_reported_port(session.mode, session.expected_port, raw_port)
        .map_err(|_| ERROR_CONTROL_UNAVAILABLE.to_string())?;
    Ok(payload)
}

fn validate_reported_port(
    mode: GoRouteRunMode,
    expected_port: u16,
    raw_port: u64,
) -> Result<u16, String> {
    let port = u16::try_from(raw_port).map_err(|_| ERROR_CONTROL_UNAVAILABLE.to_string())?;
    match mode {
        GoRouteRunMode::Isolated if port == 0 || port == PRODUCT_DEFAULT_PORT => {
            Err(ERROR_CONTROL_UNAVAILABLE.into())
        }
        GoRouteRunMode::Product if port == 0 || port != expected_port => {
            Err(ERROR_CONTROL_UNAVAILABLE.into())
        }
        _ => Ok(port),
    }
}

#[cfg(any(unix, windows))]
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

#[cfg(any(unix, windows))]
fn renew_owner_and_status(session: &ControlSession) -> Result<Value, String> {
    let renewal = post_control(
        &session.endpoint,
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

#[cfg(any(unix, windows))]
impl From<&Session> for ControlSession {
    fn from(session: &Session) -> Self {
        Self {
            home: session.home.clone(),
            endpoint: session.endpoint.clone(),
            owner_term: session.owner_term,
            instance_epoch: session.instance_epoch.clone(),
            mode: session.mode,
            expected_port: session.port,
            product_home: session.product_home.clone(),
        }
    }
}

fn status_with_supervisor(
    payload: Value,
    previous: &GoRouteIsolatedStatus,
) -> GoRouteIsolatedStatus {
    let reported_ready = payload
        .get("listen_ready")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let port = payload
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .filter(|port| *port != 0);
    let listen_ready = reported_ready && port.is_some();
    GoRouteIsolatedStatus {
        state: if listen_ready { "ready" } else { "failed" }.into(),
        listen_ready,
        port,
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
        edge_statuses: edge_statuses_from_supervisor(&payload),
        recovering: previous.recovering,
        restart_count: previous.restart_count,
    }
}

fn edge_statuses_from_supervisor(payload: &Value) -> Vec<GoRouteEdgeStatus> {
    let Some(items) = payload.get("edge_statuses").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let pool_id = item.get("pool_id").and_then(Value::as_str)?;
            if pool_id.is_empty()
                || pool_id.len() > 256
                || !pool_id.bytes().all(|byte| byte.is_ascii_graphic())
            {
                return None;
            }
            let surface = match item.get("surface").and_then(Value::as_str)? {
                "messages" | "responses" | "chat_completions" => {
                    item.get("surface").and_then(Value::as_str)?.to_owned()
                }
                _ => return None,
            };
            Some(GoRouteEdgeStatus {
                pool_id: pool_id.to_owned(),
                surface,
                member_count: item
                    .get("member_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                healthy_member_count: item
                    .get("healthy_member_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                in_flight_count: item
                    .get("in_flight_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                request_success_count: item
                    .get("request_success_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                request_failure_count: item
                    .get("request_failure_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                last_error_code: item
                    .get("last_error_code")
                    .and_then(Value::as_str)
                    .filter(|code| {
                        matches!(
                            *code,
                            "route_busy"
                                | "request_canceled"
                                | "downstream_write_failed"
                                | "invalid_request"
                                | "request_unauthorized"
                                | "request_not_found"
                                | "request_too_large"
                                | "upstream_unavailable"
                                | "request_failed"
                        )
                    })
                    .map(str::to_owned),
            })
        })
        .collect()
}

fn restart_backoff(attempt: u32) -> Duration {
    Duration::from_secs(1_u64 << attempt.saturating_sub(1).min(3))
}
