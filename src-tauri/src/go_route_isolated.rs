//! Isolated Go Messages supervisor for slice 1 app control.
//!
//! Scratch home only. Refuses product port 43121 and real ~/.agenthub.
//! Does not start BridgeRuntimeHost or write login / connection / Agent config.

use serde::Serialize;
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PRODUCT_DEFAULT_PORT: u16 = 43121;
const OWNER_ID: &str = "agenthub-gui";
const PROTOCOL_VERSION: &str = "route-runtime.v0-isolated";
const CONFIG_FORMAT_VERSION: &str = "route-config.v0-isolated";
const PACKAGE_VERSION: &str = "0.0.0-isolated";
const SYNTHETIC_KEY: &str = "ahb_gui_isolated_synthetic_not_a_real_login";
const FIXTURE_MODEL: &str = "claude-probe-fixture";
const ISOLATED_LEASE_BUDGET_MS: i64 = 24 * 60 * 60 * 1_000;
const START_STOP_WAIT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoRouteIsolatedStatus {
    pub state: String,
    pub listen_ready: bool,
    pub port: Option<u16>,
    pub last_error: Option<String>,
    pub home: Option<String>,
}

pub struct GoRouteIsolatedHost {
    inner: Mutex<Inner>,
}

struct Inner {
    status: GoRouteIsolatedStatus,
    session: Option<Session>,
}

struct Session {
    home: PathBuf,
    socket: PathBuf,
    owner_term: i64,
    instance_epoch: String,
    port: u16,
    adapterd: Child,
    mock: Child,
}

fn stopped_status() -> GoRouteIsolatedStatus {
    GoRouteIsolatedStatus {
        state: "stopped".into(),
        listen_ready: false,
        port: None,
        last_error: None,
        home: None,
    }
}

impl GoRouteIsolatedHost {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                status: stopped_status(),
                session: None,
            }),
        })
    }

    pub fn status(&self) -> GoRouteIsolatedStatus {
        #[cfg(not(unix))]
        {
            return unix_only_failed();
        }
        #[cfg(unix)]
        {
            let mut inner = self.lock();
            refresh_locked(&mut inner);
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
            {
                let mut inner = self.lock();
                refresh_locked(&mut inner);
                if inner.status.state == "starting" || inner.status.state == "ready" {
                    return inner.status.clone();
                }
                inner.status.state = "starting".into();
                inner.status.last_error = None;
                inner.status.listen_ready = false;
            }
            match start_session() {
                Ok(mut session) => {
                    let mut inner = self.lock();
                    if inner.status.state != "starting" {
                        let status = inner.status.clone();
                        drop(inner);
                        terminate_session(&mut session);
                        return status;
                    }
                    let status = GoRouteIsolatedStatus {
                        state: "ready".into(),
                        listen_ready: true,
                        port: Some(session.port),
                        last_error: None,
                        home: Some(session.home.display().to_string()),
                    };
                    inner.session = Some(session);
                    inner.status = status.clone();
                    status
                }
                Err(err) => {
                    let mut inner = self.lock();
                    inner.session = None;
                    let status = GoRouteIsolatedStatus {
                        state: "failed".into(),
                        listen_ready: false,
                        port: None,
                        last_error: Some(redact_secret(&err)),
                        home: inner.status.home.clone(),
                    };
                    inner.status = status.clone();
                    status
                }
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
            let wait_started = Instant::now();
            loop {
                let mut inner = self.lock();
                refresh_locked(&mut inner);
                if let Some(mut session) = inner.session.take() {
                    drop(inner);
                    stop_session(&mut session);
                    let mut inner = self.lock();
                    inner.status = stopped_status();
                    return inner.status.clone();
                }
                if inner.status.state != "starting" {
                    inner.status = stopped_status();
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
}

#[cfg(not(unix))]
fn unix_only_failed() -> GoRouteIsolatedStatus {
    GoRouteIsolatedStatus {
        state: "failed".into(),
        listen_ready: false,
        port: None,
        last_error: Some("unix control only".into()),
        home: None,
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
    let mock_dead = match session.mock.try_wait() {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(_) => true,
    };
    if adapterd_dead || mock_dead {
        let mut session = inner.session.take().expect("session checked above");
        terminate_session(&mut session);
        inner.status.state = "failed".into();
        inner.status.listen_ready = false;
        inner.status.last_error = Some("Go route test process exited".into());
    }
}

#[cfg(unix)]
fn stop_session(session: &mut Session) {
    let _ = post_control(
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
    );
    terminate_session(session);
}

#[cfg(unix)]
fn terminate_session(session: &mut Session) {
    let _ = session.adapterd.kill();
    let _ = session.adapterd.wait();
    let _ = session.mock.kill();
    let _ = session.mock.wait();
}

#[cfg(unix)]
fn start_session() -> Result<Session, String> {
    let home = create_scratch_home()?;
    if is_forbidden_user_home(&home) {
        return Err("refusing real user AGENTHUB_HOME".into());
    }
    if !is_scratch_home(&home) {
        return Err("AGENTHUB_HOME must be an absolute scratch directory under /tmp".into());
    }

    let messages_port = pick_loopback_port()?;
    let upstream_port = pick_loopback_port()?;
    write_probe_fixture(&home, upstream_port)?;

    let scratch_root = home.parent().unwrap_or(home.as_path());
    let bin = resolve_adapterd_bin(scratch_root)?;
    let mock_log = home.join("logs/mock-upstream.log");
    let adapterd_log = home.join("logs/adapterd.stdout.log");
    let mut mock = spawn_logged(
        Command::new(&bin)
            .arg("mock-upstream")
            .arg("--listen")
            .arg(format!("127.0.0.1:{upstream_port}")),
        &mock_log,
    )
    .map_err(|err| format!("mock-upstream spawn failed: {err}"))?;
    let mut adapterd = match spawn_logged(
        Command::new(&bin)
            .arg("run")
            .arg("--home")
            .arg(&home)
            .arg("--listen-port")
            .arg(messages_port.to_string())
            .env("AGENTHUB_HOME", &home),
        &adapterd_log,
    ) {
        Ok(child) => child,
        Err(err) => {
            let _ = mock.kill();
            let _ = mock.wait();
            return Err(format!("adapterd spawn failed: {err}"));
        }
    };

    let socket = home.join("run/adapterd.sock");
    let started = match handshake_start(&home, &socket, messages_port) {
        Ok(session) => Session {
            home: session.home,
            socket: session.socket,
            owner_term: session.owner_term,
            instance_epoch: session.instance_epoch,
            port: session.port,
            adapterd,
            mock,
        },
        Err(err) => {
            let _ = adapterd.kill();
            let _ = adapterd.wait();
            let _ = mock.kill();
            let _ = mock.wait();
            return Err(err);
        }
    };
    Ok(started)
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
    if status_payload.get("listen_ready").and_then(Value::as_bool) != Some(true) {
        return Err("Start did not become listen_ready".into());
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

fn write_probe_fixture(home: &Path, upstream_port: u16) -> Result<(), String> {
    let path = home.join("config/probe.json");
    let body = json!({
        "ingress_key": SYNTHETIC_KEY,
        "upstream_base_url": format!("http://127.0.0.1:{upstream_port}"),
        "fixture_model": FIXTURE_MODEL,
    });
    fs::write(
        &path,
        serde_json::to_vec_pretty(&body).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .map_err(|err| err.to_string())?;
    }
    Ok(())
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
    use std::os::unix::net::UnixStream;
    let raw = serde_json::to_vec(body).map_err(|err| err.to_string())?;
    let started = Instant::now();
    let mut stream = loop {
        match UnixStream::connect(socket) {
            Ok(stream) => break stream,
            Err(err) => {
                if started.elapsed() > Duration::from_secs(5) {
                    return Err(err.to_string());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .map_err(|err| err.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(8)))
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
        let message = reply
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("control request failed");
        Err(redact_secret(message))
    }
}

fn request_id(kind: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("gui-{kind}-{nanos}")
}

fn redact_secret(raw: &str) -> String {
    raw.replace(SYNTHETIC_KEY, "[redacted]")
}
