//! Disposable real-process probe for the dormant Product Go route mode.
//!
//! This is feature-gated out of normal builds. The shell wrapper supplies an
//! isolated AgentHub data directory, a packaged sidecar, and a loopback-only
//! synthetic upstream.

use std::fs;
use std::io::Read;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use agenthub_core::models::{AdapterSourceKind, AgentId, ProviderInput, RouteDownstreamSurface};
use agenthub_core::AgentHub;
use serde_json::{json, Value};

use crate::go_route_isolated::{GoRouteIsolatedHost, GoRouteRunMode};

const PRODUCT_PORT: u16 = 43121;
const SOURCE_ID: &str = "product-go-route-probe-source";
const SOURCE_KEY: &str = "sk-product-go-route-probe-do-not-use-000000";

type ProbeResult<T> = Result<T, String>;

pub fn main_entry() -> ProbeResult<()> {
    let mut args = std::env::args_os().skip(1);
    let root = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: go_route_product_e2e_probe <scratch> <upstream-url>".to_string())?;
    let upstream = args
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| "loopback upstream URL is required".to_string())?;
    if args.next().is_some() {
        return Err("unexpected extra arguments".into());
    }
    let evidence = run(root, upstream)?;
    println!(
        "{}",
        serde_json::to_string(&evidence).map_err(|error| format!("encode evidence: {error}"))?
    );
    Ok(())
}

fn run(root: PathBuf, upstream: String) -> ProbeResult<Value> {
    let root = validate_root(&root)?;
    ensure_loopback_url(&upstream)?;
    let data_dir = root.join("data");
    let skills_dir = root.join("skills");
    create_private_dir(&data_dir)?;
    create_private_dir(&skills_dir)?;

    let hub = Arc::new(
        AgentHub::open_with_skills_root(Some(&data_dir), Some(&skills_dir))
            .map_err(|error| format!("open disposable AgentHub: {error}"))?,
    );
    set_existing_data_dir_probe_mode(&data_dir)?;
    let data_mode_before = directory_mode(&data_dir)?;
    hub.providers()
        .create(&ProviderInput {
            id: SOURCE_ID.into(),
            agent_id: AgentId::WorkBuddy,
            name: "Product Go route probe source".into(),
            settings_config: json!({
                "api_key": SOURCE_KEY,
                "base_url": upstream,
                "model": "probe-model",
            }),
            meta: json!({"preset": "deepseek-api"}),
            is_current: false,
        })
        .map_err(|error| format!("create synthetic provider: {error}"))?;
    let pool = hub
        .route_pools()
        .ensure_default_pool(AgentId::Codex, RouteDownstreamSurface::Responses)
        .map_err(|error| format!("create route pool: {error}"))?;
    hub.route_pools()
        .add_member(&pool.id, AdapterSourceKind::Provider, SOURCE_ID)
        .map_err(|error| format!("attach synthetic provider: {error}"))?;
    let pool = hub
        .route_pools()
        .enroll_unified_gateway_as_default(&pool.id, PRODUCT_PORT)
        .map_err(|error| format!("save product port: {error}"))?;
    ensure(
        pool.gateway_port == Some(PRODUCT_PORT),
        "saved product port was not retained",
    )?;

    let host = GoRouteIsolatedHost::new_with_mode(Some(Arc::clone(&hub)), GoRouteRunMode::Product);
    let (preflight_port, preflight_home, config_bytes) = host.probe_product_preflight()?;
    ensure(
        preflight_port == PRODUCT_PORT && config_bytes > 0,
        "Product preflight did not retain the saved port and configuration",
    )?;
    let started = host.start();
    ensure_ready(&started, PRODUCT_PORT, "initial start")?;
    let (first_pid, product_home, first_staging) = host.probe_session_process()?;
    let expected_home = fs::canonicalize(&data_dir)
        .map_err(|error| format!("canonicalize data directory: {error}"))?
        .join("runtime")
        .join("adapterd");
    ensure(
        product_home == expected_home,
        "Product home was not canonical",
    )?;
    ensure(
        preflight_home == product_home,
        "Product preflight home drifted",
    )?;
    verify_product_tree(&data_dir, &product_home, data_mode_before)?;

    let initial_status = host.status();
    ensure_ready(&initial_status, PRODUCT_PORT, "initial status")?;
    let hash_before = host
        .probe_config_hash()
        .ok_or_else(|| "initial committed config hash is missing".to_string())?;
    hub.route_pools()
        .create_local_token(&pool.id, "Product probe additional key")
        .map_err(|error| format!("change synthetic route configuration: {error}"))?;
    let reloaded = host.reload();
    ensure_ready(&reloaded, PRODUCT_PORT, "reload")?;
    let hash_after = host
        .probe_config_hash()
        .ok_or_else(|| "reloaded config hash is missing".to_string())?;
    ensure(
        hash_after != hash_before,
        "reload did not commit a new config",
    )?;

    let startup_control_secret_scan = {
        #[cfg(all(unix, feature = "go-route-tcp-control-probe"))]
        {
            host.probe_tcp_control_startup_secret_scan()?
        }
        #[cfg(not(all(unix, feature = "go-route-tcp-control-probe")))]
        {
            false
        }
    };
    ensure(
        !tree_contains_any(
            &product_home,
            &[SOURCE_KEY.as_bytes(), pool.hub_token.as_bytes()],
        )? && !tree_contains_any(
            &first_staging,
            &[SOURCE_KEY.as_bytes(), pool.hub_token.as_bytes()],
        )?,
        "runtime files exposed synthetic secrets",
    )?;

    host.probe_kill_process()?;
    let recovery_deadline = Instant::now() + Duration::from_secs(20);
    let recovered = loop {
        let status = host.status();
        if status.state == "ready"
            && status.listen_ready
            && status.port == Some(PRODUCT_PORT)
            && status.restart_count >= 1
        {
            break status;
        }
        if Instant::now() >= recovery_deadline {
            return Err(format!("Product route did not recover: {}", status.state));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let (second_pid, recovered_home, second_staging) = host.probe_session_process()?;
    ensure(first_pid != second_pid, "recovery reused the dead process")?;
    ensure(
        recovered_home == product_home,
        "recovery changed Product home",
    )?;
    ensure(
        !first_staging.exists(),
        "recovery retained the previous staging directory",
    )?;
    ensure(
        product_home.exists(),
        "recovery removed persistent Product home",
    )?;

    let stopped = host.stop();
    ensure(
        stopped.state == "stopped" && !stopped.listen_ready,
        "Product route did not stop",
    )?;
    ensure(
        !second_staging.exists(),
        "stop retained the active staging directory",
    )?;
    verify_product_tree(&data_dir, &product_home, data_mode_before)?;
    let released = wait_for_port_release(PRODUCT_PORT, Duration::from_secs(5));
    ensure(released, "Product port was not released after stop")?;

    Ok(json!({
        "schema": "go-route-product-e2e-probe.v1",
        "status": "ok",
        "port": PRODUCT_PORT,
        "saved_port_preserved": true,
        "reload_committed": true,
        "restart_count": recovered.restart_count,
        "same_port_recovered": true,
        "product_home_preserved": true,
        "staging_cleaned": true,
        "port_released": released,
        "startup_control_secret_scan": startup_control_secret_scan,
        "runtime_secret_scan": true,
        "data_dir_mode_unchanged": true,
    }))
}

fn ensure_ready(
    status: &crate::go_route_isolated::GoRouteIsolatedStatus,
    port: u16,
    step: &str,
) -> ProbeResult<()> {
    if status.state == "ready" && status.listen_ready && status.port == Some(port) {
        Ok(())
    } else {
        Err(format!(
            "{step} did not report the saved Product port (state={}, ready={}, port={:?}, error={:?})",
            status.state, status.listen_ready, status.port, status.last_error
        ))
    }
}

fn verify_product_tree(data_dir: &Path, home: &Path, original_mode: u32) -> ProbeResult<()> {
    ensure(
        directory_mode(data_dir)? == original_mode,
        "Product route changed the existing data directory mode",
    )?;
    for path in [
        data_dir.join("runtime"),
        home.to_path_buf(),
        home.join("run"),
        home.join("config"),
        home.join("logs"),
    ] {
        verify_private_directory(&path)?;
    }
    Ok(())
}

#[cfg(unix)]
fn verify_private_directory(path: &Path) -> ProbeResult<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("inspect {}: {error}", path.display()))?;
    ensure(
        metadata.file_type().is_dir()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o777 == 0o700,
        &format!("{} is not an owned 0700 directory", path.display()),
    )
}

#[cfg(not(unix))]
fn verify_private_directory(path: &Path) -> ProbeResult<()> {
    ensure(
        path.is_dir(),
        &format!("{} is not a directory", path.display()),
    )
}

#[cfg(unix)]
fn directory_mode(path: &Path) -> ProbeResult<u32> {
    use std::os::unix::fs::MetadataExt;
    Ok(fs::symlink_metadata(path)
        .map_err(|error| format!("inspect {}: {error}", path.display()))?
        .mode()
        & 0o777)
}

#[cfg(not(unix))]
fn directory_mode(path: &Path) -> ProbeResult<u32> {
    ensure(
        path.is_dir(),
        &format!("{} is not a directory", path.display()),
    )?;
    Ok(0)
}

fn tree_contains_any(root: &Path, needles: &[&[u8]]) -> ProbeResult<bool> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| format!("inspect {}: {error}", root.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "refusing symlink during secret scan: {}",
            root.display()
        ));
    }
    if metadata.is_dir() {
        for entry in
            fs::read_dir(root).map_err(|error| format!("read {}: {error}", root.display()))?
        {
            let path = entry.map_err(|error| error.to_string())?.path();
            if tree_contains_any(&path, needles)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
        return Ok(false);
    }
    let mut bytes = Vec::new();
    fs::File::open(root)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|error| format!("scan {}: {error}", root.display()))?;
    Ok(needles.iter().any(|needle| {
        !needle.is_empty() && bytes.windows(needle.len()).any(|part| part == *needle)
    }))
}

fn wait_for_port_release(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn ensure_loopback_url(raw: &str) -> ProbeResult<()> {
    let authority = raw
        .strip_prefix("http://")
        .or_else(|| raw.strip_prefix("https://"))
        .and_then(|rest| rest.split('/').next())
        .filter(|value| !value.is_empty() && !value.contains('@'))
        .ok_or_else(|| "upstream URL must be HTTP(S) without user info".to_string())?;
    let host = authority
        .split(':')
        .next()
        .ok_or_else(|| "upstream URL has no host".to_string())?;
    ensure(
        host.parse::<std::net::Ipv4Addr>()
            .is_ok_and(|address| address.is_loopback()),
        "upstream URL must be loopback HTTP(S)",
    )
}

fn validate_root(root: &Path) -> ProbeResult<PathBuf> {
    let root = fs::canonicalize(root).map_err(|error| format!("canonicalize scratch: {error}"))?;
    let temp = fs::canonicalize(std::env::temp_dir())
        .map_err(|error| format!("canonicalize temp root: {error}"))?;
    ensure(
        root.starts_with(&temp) && root != temp,
        "scratch must be a disposable child of the operating-system temp directory",
    )?;
    Ok(root)
}

fn create_private_dir(path: &Path) -> ProbeResult<()> {
    fs::create_dir(path).map_err(|error| format!("create {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("secure {}: {error}", path.display()))?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_existing_data_dir_probe_mode(path: &Path) -> ProbeResult<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o710))
        .map_err(|error| format!("set existing data directory probe mode: {error}"))
}

#[cfg(not(unix))]
fn set_existing_data_dir_probe_mode(_path: &Path) -> ProbeResult<()> {
    Ok(())
}

fn ensure(condition: bool, message: &str) -> ProbeResult<()> {
    if condition {
        Ok(())
    } else {
        Err(message.to_string())
    }
}
