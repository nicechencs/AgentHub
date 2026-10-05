//! Disposable real-process probe for the dormant Product Go route mode.
//!
//! This is feature-gated out of normal builds. The shell wrapper supplies an
//! isolated AgentHub data directory, a packaged sidecar, and a loopback-only
//! synthetic upstream.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use agenthub_core::models::{AdapterSourceKind, AgentId, ProviderInput, RouteDownstreamSurface};
use agenthub_core::AgentHub;
use base64::Engine as _;
use serde_json::{json, Value};

use crate::go_route_isolated::{GoRouteIsolatedHost, GoRouteRequiredReloadResult, GoRouteRunMode};

const PRODUCT_PORT: u16 = 43121;
const SOURCE_ID: &str = "product-go-route-probe-source";
const SOURCE_KEY: &str = "sk-product-go-route-probe-do-not-use-000000";
const WRONG_BEARER: &str = "ahb-product-probe-wrong-bearer";
const USAGE_REQUEST_MARKER: &str = "product-usage-request-body";
const USAGE_RESPONSE_MARKER: &str = "product-usage-response-ok";
const MAX_PROBE_RESPONSE_BYTES: u64 = 32 * 1024;

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
    let option = match args.next() {
        None => ProbeOption::Full {
            hold_evidence: None,
        },
        Some(value) if value == "--hold-after-ready" => ProbeOption::Full {
            hold_evidence: Some(
                args.next()
                    .map(PathBuf::from)
                    .ok_or_else(|| "--hold-after-ready requires an evidence path".to_string())?,
            ),
        },
        Some(value) if value == "--windows-reparse-preflight" => {
            ProbeOption::WindowsReparsePreflight
        }
        Some(_) => return Err("unexpected probe option".into()),
    };
    if args.next().is_some() {
        return Err("unexpected extra arguments".into());
    }
    let evidence = match option {
        ProbeOption::Full { hold_evidence } => run(root, upstream, hold_evidence)?,
        ProbeOption::WindowsReparsePreflight => run_windows_reparse_preflight(root, upstream)?,
    };
    println!(
        "{}",
        serde_json::to_string(&evidence).map_err(|error| format!("encode evidence: {error}"))?
    );
    Ok(())
}

enum ProbeOption {
    Full { hold_evidence: Option<PathBuf> },
    WindowsReparsePreflight,
}

fn run(root: PathBuf, upstream: String, hold_evidence: Option<PathBuf>) -> ProbeResult<Value> {
    let root = validate_root(&root)?;
    ensure_loopback_url(&upstream)?;
    let hold_evidence = hold_evidence
        .as_deref()
        .map(|path| validate_hold_evidence_path(&root, path))
        .transpose()?;
    let data_dir = root.join("data");
    let skills_dir = root.join("skills");
    create_private_dir(&data_dir)?;
    create_private_dir(&skills_dir)?;
    set_existing_data_dir_probe_mode(&data_dir)?;
    let data_mode_before = directory_mode(&data_dir)?;
    let (hub, pool) = initialize_product_probe_hub(&data_dir, &skills_dir, &upstream)?;
    ensure(
        pool.gateway_port == Some(PRODUCT_PORT),
        "saved product port was not retained",
    )?;

    let host = GoRouteIsolatedHost::new(Some(Arc::clone(&hub)));
    let unprepared_product = host.start_mode(GoRouteRunMode::Product);
    ensure(
        unprepared_product.state == "failed"
            && unprepared_product.lifecycle.as_deref() == Some("prepared_product_required"),
        "direct Product start bypassed the prepared Product plan",
    )?;
    let dropped_prepared = host.prepare_product_plan()?;
    drop(dropped_prepared);
    let stopped_prepared = host.prepare_product_plan()?;
    let reservation_stopped = host.stop();
    ensure(
        reservation_stopped.state == "stopped" && !reservation_stopped.listen_ready,
        "stop did not cancel the prepared Product reservation",
    )?;
    let stopped_prepared_start = host.start_prepared_product(stopped_prepared);
    ensure(
        stopped_prepared_start.state == "failed"
            && stopped_prepared_start.lifecycle.as_deref() == Some("mode_conflict"),
        "stopped Product reservation remained startable",
    )?;
    let invalidated_prepared = host.prepare_product_plan()?;
    let reserved_start = host.start();
    ensure(
        reserved_start.state == "failed"
            && reserved_start.lifecycle.as_deref() == Some("mode_conflict"),
        "direct start stole a prepared Product reservation",
    )?;
    let reserved_reload = host.reload();
    ensure(
        reserved_reload.state == "failed"
            && reserved_reload.lifecycle.as_deref() == Some("mode_conflict"),
        "direct reload stole a prepared Product reservation",
    )?;
    ensure(
        matches!(
            host.reload_required_after_write(),
            GoRouteRequiredReloadResult::Failed { .. }
        ),
        "required reload did not invalidate the prepared Product reservation",
    )?;
    let invalidated_start = host.start_prepared_product(invalidated_prepared);
    ensure(
        invalidated_start.state == "failed"
            && invalidated_start.lifecycle.as_deref() == Some("mode_conflict"),
        "invalidated Product reservation remained startable",
    )?;
    let prepared = host.prepare_product_plan()?;
    let prepared_summary = GoRouteIsolatedHost::probe_prepared_product_summary(&prepared)?;
    ensure(
        prepared_summary.expected_port == PRODUCT_PORT
            && prepared_summary.expected_config_hash.len() == 64,
        "Product preflight did not retain safe startup expectations",
    )?;
    hub.route_pools()
        .create_local_token(&pool.id, "Product probe additional key")
        .map_err(|error| format!("change synthetic route configuration: {error}"))?;
    let started = host.start_prepared_product(prepared);
    ensure_ready(&started, PRODUCT_PORT, "initial start")?;
    let database_before = database_and_wal_snapshot(&data_dir)?;
    let (first_pid, product_home, first_staging) = host.probe_session_process()?;
    let expected_home = fs::canonicalize(&data_dir)
        .map_err(|error| format!("canonicalize data directory: {error}"))?
        .join("runtime")
        .join("adapterd");
    ensure(
        product_home == expected_home,
        "Product home was not canonical",
    )?;
    verify_product_tree(&data_dir, &product_home, data_mode_before)?;

    let initial_status = host.status();
    ensure_ready(&initial_status, PRODUCT_PORT, "initial status")?;
    let health = GoRouteIsolatedHost::probe_data_plane_health(PRODUCT_PORT, &pool.hub_token)?;
    ensure(
        health.http_status == 200
            && health.listen_ready == Some(true)
            && health.member_count == Some(1)
            && health.healthy_member_count == Some(1),
        "authenticated Product data-plane health was not ready",
    )?;
    let wrong_health = GoRouteIsolatedHost::probe_data_plane_health(PRODUCT_PORT, WRONG_BEARER)?;
    ensure(
        wrong_health.http_status == 401
            && wrong_health.listen_ready.is_none()
            && wrong_health.member_count.is_none()
            && wrong_health.healthy_member_count.is_none(),
        "wrong Product bearer did not receive a sanitized 401",
    )?;
    let mode_conflict = host.start();
    ensure(
        mode_conflict.state == "failed"
            && mode_conflict.lifecycle.as_deref() == Some("mode_conflict"),
        "isolated start changed an active Product host mode",
    )?;
    ensure_ready(&host.status(), PRODUCT_PORT, "status after mode conflict")?;
    let hash_before = host
        .probe_config_hash()
        .ok_or_else(|| "initial committed config hash is missing".to_string())?;
    ensure(
        hash_before == prepared_summary.expected_config_hash,
        "prepared Product start rebuilt configuration from saved state",
    )?;
    ensure(
        product_usage_request_succeeds(&pool.hub_token, "probe-model"),
        "Product route did not complete the synthetic usage request",
    )?;
    verify_product_usage_spool(&data_dir, &pool.id)?;
    let usage_scan_values = [
        SOURCE_KEY.as_bytes(),
        pool.hub_token.as_bytes(),
        WRONG_BEARER.as_bytes(),
        USAGE_REQUEST_MARKER.as_bytes(),
        USAGE_RESPONSE_MARKER.as_bytes(),
        upstream.as_bytes(),
    ];
    ensure(
        !tree_contains_any(&data_dir.join("usage-gateway"), &usage_scan_values)?
            && !tree_contains_any(&product_home.join("logs"), &usage_scan_values)?
            && optional_file_is_free_of(&root.join("run.log"), &usage_scan_values)?,
        "Product usage spool or probe logs exposed synthetic request data",
    )?;
    ensure(
        database_and_wal_snapshot(&data_dir)? == database_before,
        "Product usage request changed the database or WAL",
    )?;
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

    if let Some(path) = hold_evidence {
        hold_after_ready(&path, first_pid, &product_home)?;
    }

    host.probe_kill_process()?;
    let recovering = host.status();
    ensure(
        recovering.state == "failed" && recovering.recovering,
        "killed Product route did not enter desired recovery",
    )?;
    let recovery_mode_conflict = host.start();
    ensure(
        recovery_mode_conflict.state == "failed"
            && recovery_mode_conflict.lifecycle.as_deref() == Some("mode_conflict"),
        "isolated start changed mode while Product recovery was desired",
    )?;
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

    let stop_barrier = Arc::new(Barrier::new(3));
    let first_stop_host = Arc::clone(&host);
    let first_stop_barrier = Arc::clone(&stop_barrier);
    let first_stop = std::thread::spawn(move || {
        first_stop_barrier.wait();
        first_stop_host.stop()
    });
    let second_stop_host = Arc::clone(&host);
    let second_stop_barrier = Arc::clone(&stop_barrier);
    let second_stop = std::thread::spawn(move || {
        second_stop_barrier.wait();
        second_stop_host.stop()
    });
    stop_barrier.wait();
    let stopped = first_stop
        .join()
        .map_err(|_| "first concurrent stop panicked".to_string())?;
    let also_stopped = second_stop
        .join()
        .map_err(|_| "second concurrent stop panicked".to_string())?;
    ensure(
        stopped.state == "stopped"
            && !stopped.listen_ready
            && also_stopped.state == "stopped"
            && !also_stopped.listen_ready,
        "concurrent Product stop did not converge",
    )?;
    ensure(
        !second_staging.exists(),
        "stop retained the active staging directory",
    )?;
    verify_product_tree(&data_dir, &product_home, data_mode_before)?;
    let released = wait_for_port_release(PRODUCT_PORT, Duration::from_secs(5));
    ensure(released, "Product port was not released after stop")?;
    let isolated = host.start();
    ensure(
        isolated.state == "ready"
            && isolated.listen_ready
            && isolated.port.is_some_and(|port| port != PRODUCT_PORT),
        "completed Product stop did not release the host for isolated mode",
    )?;
    let isolated_port = isolated.port;
    let isolated_stopped = host.stop();
    ensure(
        isolated_stopped.state == "stopped" && !isolated_stopped.listen_ready,
        "isolated route did not stop after mode release",
    )?;
    Ok(json!({
        "schema": "go-route-product-e2e-probe.v1",
        "status": "ok",
        "port": PRODUCT_PORT,
        "saved_port_preserved": true,
        "prepared_drop_released": true,
        "unprepared_product_rejected": true,
        "prepared_stop_cancelled": true,
        "prepared_write_invalidated": true,
        "prepared_reservation_exclusive": true,
        "prepared_plan_preserved": true,
        "health_ready": true,
        "wrong_bearer_rejected": true,
        "active_mode_change_rejected": true,
        "recovery_mode_change_rejected": true,
        "concurrent_stop_converged": true,
        "stopped_mode_released": true,
        "isolated_port": isolated_port,
        "reload_committed": true,
        "restart_count": recovered.restart_count,
        "same_port_recovered": true,
        "product_home_preserved": true,
        "staging_cleaned": true,
        "port_released": released,
        "startup_control_secret_scan": startup_control_secret_scan,
        "runtime_secret_scan": true,
        "usage_jsonl_recorded": true,
        "usage_spool_and_logs_secret_scan": true,
        "usage_request_database_and_wal_unchanged": true,
        "data_dir_mode_unchanged": true,
    }))
}

fn initialize_product_probe_hub(
    data_dir: &Path,
    skills_dir: &Path,
    upstream: &str,
) -> ProbeResult<(Arc<AgentHub>, agenthub_core::models::RoutePool)> {
    let hub = Arc::new(
        AgentHub::open_with_skills_root(Some(data_dir), Some(skills_dir))
            .map_err(|error| format!("open disposable AgentHub: {error}"))?,
    );
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
    Ok((hub, pool))
}

#[cfg(windows)]
fn run_windows_reparse_preflight(root: PathBuf, upstream: String) -> ProbeResult<Value> {
    let root = validate_root(&root)?;
    ensure_loopback_url(&upstream)?;
    let data_dir = root.join("data");
    let runtime_dir = data_dir.join("runtime");
    let skills_dir = root.join("skills");
    ensure(
        data_dir.is_dir() && !windows_path_is_reparse_point(&data_dir)?,
        "Windows reparse preflight requires a plain pre-created data directory",
    )?;
    ensure(
        runtime_dir.is_dir() && windows_path_is_reparse_point(&runtime_dir)?,
        "Windows reparse preflight requires a runtime junction",
    )?;
    create_private_dir(&skills_dir)?;
    let (hub, _) = initialize_product_probe_hub(&data_dir, &skills_dir, &upstream)?;
    let host = GoRouteIsolatedHost::new(Some(hub));
    let prepared = host.prepare_product_plan()?;
    let started = host.start_prepared_product(prepared);
    ensure(
        started.state == "failed" && !started.listen_ready,
        "runtime junction did not reject Product startup",
    )?;
    ensure(
        host.probe_session_process().is_err(),
        "runtime junction allowed a Product Go process to start",
    )?;
    let port_available = wait_for_port_release(PRODUCT_PORT, Duration::from_secs(1));
    ensure(
        port_available,
        "runtime junction rejection left the saved Product port bound",
    )?;
    let stopped = host.stop();
    ensure(
        stopped.state == "stopped" && !stopped.listen_ready,
        "runtime junction rejection did not converge to stopped",
    )?;
    Ok(json!({
        "schema": "go-route-product-reparse-preflight.v1",
        "status": "ok",
        "port": PRODUCT_PORT,
        "runtime_junction_rejected": true,
        "go_process_not_started": true,
        "port_available": port_available,
    }))
}

#[cfg(not(windows))]
fn run_windows_reparse_preflight(_root: PathBuf, _upstream: String) -> ProbeResult<Value> {
    Err("--windows-reparse-preflight requires Windows".into())
}

#[cfg(windows)]
fn windows_path_is_reparse_point(path: &Path) -> ProbeResult<bool> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    Ok(fs::symlink_metadata(path)
        .map_err(|error| format!("inspect {}: {error}", path.display()))?
        .file_attributes()
        & FILE_ATTRIBUTE_REPARSE_POINT
        != 0)
}

fn hold_after_ready(path: &Path, go_pid: u32, product_home: &Path) -> ProbeResult<()> {
    let evidence = json!({
        "schema": "go-route-product-ready-hold.v1",
        "status": "ready",
        "rust_pid": std::process::id(),
        "go_pid": go_pid,
        "port": PRODUCT_PORT,
        "product_home": product_home,
    });
    let (mut file, temporary_path) = create_hold_evidence_temp(path)?;
    let written = (|| -> ProbeResult<()> {
        serde_json::to_writer(&mut file, &evidence)
            .map_err(|error| format!("encode hold evidence: {error}"))?;
        file.write_all(b"\n")
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("write hold evidence {}: {error}", path.display()))?;
        drop(file);
        fs::rename(&temporary_path, path).map_err(|error| {
            format!(
                "publish hold evidence {} from {}: {error}",
                path.display(),
                temporary_path.display()
            )
        })
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    written?;
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn create_hold_evidence_temp(path: &Path) -> ProbeResult<(fs::File, PathBuf)> {
    let parent = path
        .parent()
        .ok_or_else(|| "hold evidence path has no parent directory".to_string())?;
    for _ in 0..8 {
        let mut nonce = [0_u8; 16];
        getrandom::getrandom(&mut nonce)
            .map_err(|_| "generate hold evidence temporary name".to_string())?;
        let temporary_path = parent.join(format!(
            ".agenthub-product-ready-{}-{}.tmp",
            std::process::id(),
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(nonce),
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => return Ok((file, temporary_path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "create hold evidence temporary file {}: {error}",
                    temporary_path.display()
                ))
            }
        }
    }
    Err("create unique hold evidence temporary file".into())
}

fn product_usage_request_succeeds(bearer: &str, model: &str) -> bool {
    let Some((status, body)) = post_product_bytes(
        "/v1/responses",
        bearer,
        json!({
            "model": model,
            "input": USAGE_REQUEST_MARKER,
            "stream": false,
        }),
    ) else {
        return false;
    };
    if status != 200 {
        return false;
    }
    let Ok(response) = serde_json::from_slice::<Value>(&body) else {
        return false;
    };
    response.get("object").and_then(Value::as_str) == Some("response")
        && response.get("status").and_then(Value::as_str) == Some("completed")
        && response.get("model").and_then(Value::as_str) == Some(model)
        && response_output_has_text(&response, USAGE_RESPONSE_MARKER)
}

fn post_product_bytes(path: &str, bearer: &str, body: Value) -> Option<(u16, Vec<u8>)> {
    let request_body = serde_json::to_string(&body).ok()?;
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(2))
        .redirects(0)
        .try_proxy_from_env(false)
        .build();
    let response = match agent
        .post(&format!("http://127.0.0.1:{PRODUCT_PORT}{path}"))
        .set("Authorization", &format!("Bearer {bearer}"))
        .set("Content-Type", "application/json")
        .send_string(&request_body)
    {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(ureq::Error::Transport(_)) => return None,
    };
    let status = response.status();
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_PROBE_RESPONSE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= MAX_PROBE_RESPONSE_BYTES).then_some((status, bytes))
}

fn response_output_has_text(response: &Value, expected: &str) -> bool {
    response
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("message"))
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .any(|part| {
            part.get("type").and_then(Value::as_str) == Some("output_text")
                && part.get("text").and_then(Value::as_str) == Some(expected)
        })
}

fn verify_product_usage_spool(data_dir: &Path, expected_profile_id: &str) -> ProbeResult<()> {
    const ALLOWED_FIELDS: &[&str] = &[
        "request_id",
        "ts",
        "profile_id",
        "surface",
        "upstream_channel",
        "ticket_id",
        "account_source_kind",
        "account_source_id",
        "model",
        "upstream_model",
        "input_tokens",
        "output_tokens",
        "status",
        "status_code",
        "error_class",
        "latency_ms",
        "ttft_ms",
        "attempts",
    ];

    let spool_dir = data_dir.join("usage-gateway");
    let mut files = fs::read_dir(&spool_dir)
        .map_err(|error| format!("read Product usage spool: {error}"))?
        .map(|entry| entry.map_err(|error| format!("read Product usage spool entry: {error}")))
        .collect::<Result<Vec<_>, _>>()?;
    ensure(
        files.len() == 1 && files[0].file_type().is_ok_and(|kind| kind.is_file()),
        "Product usage spool did not contain exactly one JSONL file",
    )?;
    let file = files.pop().expect("exactly one file checked above");
    let name = file
        .file_name()
        .into_string()
        .map_err(|_| "Product usage spool filename was not UTF-8".to_string())?;
    let day = usage_spool_filename_day(&name)?;
    let bytes =
        fs::read(file.path()).map_err(|error| format!("read Product usage spool row: {error}"))?;
    ensure(
        bytes.ends_with(b"\n"),
        "Product usage spool row did not end with a newline",
    )?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| "Product usage spool row was not UTF-8 JSONL".to_string())?;
    let row = text
        .strip_suffix('\n')
        .filter(|row| !row.is_empty() && !row.contains('\n'))
        .ok_or_else(|| "Product usage spool did not contain exactly one JSONL row".to_string())?;
    let event = serde_json::from_str::<Value>(row)
        .map_err(|_| "Product usage spool row was not valid JSON".to_string())?;
    let fields = event
        .as_object()
        .ok_or_else(|| "Product usage spool row was not a JSON object".to_string())?;
    ensure(
        fields
            .keys()
            .all(|key| ALLOWED_FIELDS.contains(&key.as_str())),
        "Product usage spool row contained an unsupported field",
    )?;
    ensure(
        fields
            .get("request_id")
            .and_then(Value::as_str)
            .is_some_and(is_uuid_v4),
        "Product usage spool row omitted a v4 request id",
    )?;
    let timestamp = fields
        .get("ts")
        .and_then(Value::as_str)
        .ok_or_else(|| "Product usage spool row omitted its timestamp".to_string())?;
    ensure(
        usage_timestamp_day(timestamp).as_deref() == Some(day.as_str()),
        "Product usage spool filename did not match its timestamp",
    )?;
    ensure(
        fields.get("profile_id").and_then(Value::as_str) == Some(expected_profile_id)
            && fields.get("surface").and_then(Value::as_str) == Some("responses")
            && fields.get("upstream_channel").and_then(Value::as_str) == Some("openai_chat")
            && fields.get("account_source_kind").and_then(Value::as_str) == Some("provider")
            && fields.get("account_source_id").and_then(Value::as_str) == Some(SOURCE_ID)
            && fields.get("model").and_then(Value::as_str) == Some("probe-model")
            && fields.get("upstream_model").and_then(Value::as_str) == Some("probe-model")
            && fields.get("input_tokens").and_then(Value::as_u64) == Some(0)
            && fields.get("output_tokens").and_then(Value::as_u64) == Some(0)
            && fields.get("status").and_then(Value::as_str) == Some("ok")
            && fields.get("status_code").and_then(Value::as_u64) == Some(200)
            && fields.get("latency_ms").and_then(Value::as_u64).is_some()
            && fields.get("attempts").and_then(Value::as_u64) == Some(1)
            && !fields.contains_key("error_class")
            && !fields.contains_key("ttft_ms"),
        "Product usage spool row did not contain the expected public success fields",
    )
}

fn usage_spool_filename_day(name: &str) -> ProbeResult<String> {
    let day = name
        .strip_prefix("gateway-")
        .and_then(|name| name.strip_suffix(".jsonl"))
        .filter(|day| day.len() == 8 && day.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or_else(|| "Product usage spool filename was invalid".to_string())?;
    Ok(day.to_owned())
}

fn usage_timestamp_day(timestamp: &str) -> Option<String> {
    let bytes = timestamp.as_bytes();
    (bytes.len() >= 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && [0, 1, 2, 3, 5, 6, 8, 9]
            .into_iter()
            .all(|index| bytes[index].is_ascii_digit()))
    .then(|| {
        format!(
            "{}{}{}",
            &timestamp[0..4],
            &timestamp[5..7],
            &timestamp[8..10]
        )
    })
}

fn is_uuid_v4(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23]
            .into_iter()
            .all(|index| bytes[index] == b'-')
        && bytes[14] == b'4'
        && matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit())
}

fn database_and_wal_snapshot(data_dir: &Path) -> ProbeResult<Vec<(String, Option<Vec<u8>>)>> {
    ["agenthub.db", "agenthub.db-wal"]
        .into_iter()
        .map(|name| {
            let path = data_dir.join(name);
            let contents = if path.exists() {
                Some(fs::read(&path).map_err(|error| {
                    format!("read Product database snapshot {}: {error}", path.display())
                })?)
            } else {
                None
            };
            Ok((name.to_owned(), contents))
        })
        .collect()
}

fn optional_file_is_free_of(path: &Path, needles: &[&[u8]]) -> ProbeResult<bool> {
    if !path.exists() {
        return Ok(true);
    }
    tree_contains_any(path, needles).map(|contains| !contains)
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

fn validate_hold_evidence_path(root: &Path, path: &Path) -> ProbeResult<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| "hold evidence path has no parent directory".to_string())?;
    let parent = fs::canonicalize(parent)
        .map_err(|error| format!("canonicalize hold evidence parent: {error}"))?;
    ensure(
        parent == root,
        "hold evidence must be a new file directly inside the scratch directory",
    )?;
    let name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "hold evidence path has no file name".to_string())?;
    let path = parent.join(name);
    ensure(!path.exists(), "hold evidence path already exists")?;
    Ok(path)
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
