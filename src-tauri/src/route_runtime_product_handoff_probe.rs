//! Disposable real-listener Rust -> Product Go -> Rust handoff trial.

use std::fs;
use std::io::Read;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use agenthub_core::adapter_control::AdapterSagaCoordinator;
use agenthub_core::models::{AdapterSourceKind, AgentId, ProviderInput, RouteDownstreamSurface};
use agenthub_core::AgentHub;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::exit_coordinator::LifecycleShutdownBarrier;
use crate::route_runtime::{
    ProductHandoffTrialProbeFailure, ProductHandoffTrialRuntimeProbe, RouteRuntimeManager,
};

const PRODUCT_PORT: u16 = 43121;
const RESPONSES_SOURCE_ID: &str = "product-handoff-probe-responses-source";
const MESSAGES_SOURCE_ID: &str = "product-handoff-probe-messages-source";
const RESPONSES_SOURCE_KEY: &str = "sk-product-handoff-responses-do-not-use-000000";
const MESSAGES_SOURCE_KEY: &str = "sk-product-handoff-messages-do-not-use-000000";
const WRONG_BEARER: &str = "ahb-product-handoff-wrong-bearer";
const RESPONSES_REQUEST_MARKER: &str = "handoff-responses-request";
const RESPONSES_FAILURE_REQUEST_MARKER: &str = "handoff-responses-upstream-failure";
const MESSAGES_REQUEST_MARKER: &str = "handoff-messages-request";
const RESPONSES_RESPONSE_MARKER: &str = "handoff-responses-ok";
const MESSAGES_RESPONSE_MARKER: &str = "handoff-messages-ok";

type ProbeResult<T> = Result<T, String>;

pub fn main_entry() -> ProbeResult<()> {
    let mut args = std::env::args_os().skip(1);
    let root = args.next().map(PathBuf::from).ok_or_else(|| {
        "usage: route_runtime_product_handoff_probe <scratch> <upstream-url> <run-log>".to_string()
    })?;
    let upstream = args
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| "loopback upstream URL is required".to_string())?;
    let run_log = args.next().map(PathBuf::from).ok_or_else(|| {
        "usage: route_runtime_product_handoff_probe <scratch> <upstream-url> <run-log>".to_string()
    })?;
    if args.next().is_some() {
        return Err("unexpected extra arguments".into());
    }
    let evidence = tauri::async_runtime::block_on(run(root, upstream, run_log))?;
    println!(
        "{}",
        serde_json::to_string(&evidence).map_err(|_| "encode probe evidence failed")?
    );
    Ok(())
}

async fn run(root: PathBuf, upstream: String, run_log: PathBuf) -> ProbeResult<Value> {
    let root = validate_root(&root)?;
    let run_log = validate_run_log(&root, &run_log)?;
    ensure_loopback_url(&upstream)?;
    let data_dir = root.join("data");
    let skills_dir = root.join("skills");
    create_private_dir(&data_dir)?;
    create_private_dir(&skills_dir)?;
    let hub = Arc::new(
        AgentHub::open_with_skills_root(Some(&data_dir), Some(&skills_dir))
            .map_err(|_| "open disposable AgentHub failed".to_string())?,
    );
    set_existing_data_dir_probe_mode(&data_dir)?;
    hub.providers()
        .create(&ProviderInput {
            id: RESPONSES_SOURCE_ID.into(),
            agent_id: AgentId::WorkBuddy,
            name: "Product handoff probe Responses source".into(),
            settings_config: json!({
                "api_key": RESPONSES_SOURCE_KEY,
                "base_url": upstream,
                "model": "probe-model",
            }),
            meta: json!({"preset": "deepseek-api"}),
            is_current: false,
        })
        .map_err(|_| "create synthetic Responses provider failed".to_string())?;
    hub.providers()
        .create(&ProviderInput {
            id: MESSAGES_SOURCE_ID.into(),
            agent_id: AgentId::Claude,
            name: "Product handoff probe Messages source".into(),
            settings_config: json!({
                "apiKey": MESSAGES_SOURCE_KEY,
                "env": {
                    "ANTHROPIC_BASE_URL": upstream,
                    "ANTHROPIC_AUTH_TOKEN": MESSAGES_SOURCE_KEY,
                },
                "model": "probe-model",
            }),
            meta: json!({"preset": "anthropic"}),
            is_current: false,
        })
        .map_err(|_| "create synthetic Messages provider failed".to_string())?;

    let responses_pool = configure_pool(
        hub.as_ref(),
        AgentId::Codex,
        RouteDownstreamSurface::Responses,
        RESPONSES_SOURCE_ID,
    )?;
    let messages_pool = configure_pool(
        hub.as_ref(),
        AgentId::Claude,
        RouteDownstreamSurface::Messages,
        MESSAGES_SOURCE_ID,
    )?;
    ensure(
        !responses_pool.hub_token.trim().is_empty()
            && !messages_pool.hub_token.trim().is_empty()
            && responses_pool.hub_token != messages_pool.hub_token,
        "dual-route probe did not receive distinct entry Keys",
    )?;
    let secret_values = vec![
        RESPONSES_SOURCE_KEY.to_owned(),
        MESSAGES_SOURCE_KEY.to_owned(),
        WRONG_BEARER.to_owned(),
        responses_pool.hub_token.clone(),
        messages_pool.hub_token.clone(),
        RESPONSES_REQUEST_MARKER.to_owned(),
        RESPONSES_FAILURE_REQUEST_MARKER.to_owned(),
        MESSAGES_REQUEST_MARKER.to_owned(),
        RESPONSES_RESPONSE_MARKER.to_owned(),
        MESSAGES_RESPONSE_MARKER.to_owned(),
    ];
    let flags = hub.route_pools().pair_adapter_flags();
    // Build every Rust start spec before the Product plan is prepared.
    let specs = [&responses_pool, &messages_pool]
        .into_iter()
        .map(|pool| hub.adapter_bridge().pool_listener_spec(pool, flags))
        .collect::<Vec<_>>();
    let responses_model = specs
        .first()
        .and_then(|spec| spec.listed_models.first())
        .cloned()
        .ok_or_else(|| "Responses route did not expose a probe model".to_string())?;
    let messages_model = specs
        .get(1)
        .and_then(|spec| spec.listed_models.first())
        .cloned()
        .ok_or_else(|| "Messages route did not expose a probe model".to_string())?;

    let runtime = Arc::new(RouteRuntimeManager::new_product_handoff_probe(Arc::clone(
        &hub,
    )));
    let rust = runtime.rust_host_for_bridge_saga();
    for spec in specs {
        let status = rust
            .start(spec)
            .await
            .map_err(|_| "start real Rust gateway entry failed".to_string())?;
        ensure(
            status.running && status.port == PRODUCT_PORT,
            "Rust gateway did not use the saved Product port",
        )?;
    }
    ensure_rust_restored(&runtime, 2)?;
    let safe_preflight = hub
        .adapter_bridge()
        .prepare_go_product_config()
        .map_err(|_| "Product preflight read failed".to_string())?;
    ensure(
        safe_preflight.summary().eligible(),
        &format!(
            "Product preflight rejected the probe topology: {}",
            safe_preflight.summary().reason.as_str()
        ),
    )?;
    drop(safe_preflight);
    let db_before = critical_db_hash(&data_dir)?;

    let lifecycle = Arc::new(LifecycleShutdownBarrier::new_product_handoff_probe());
    let coordinator = Arc::new(AdapterSagaCoordinator::new());

    let report = runtime
        .spawn_product_handoff_trial(
            Arc::clone(&lifecycle),
            Arc::clone(&coordinator),
            vec![
                responses_pool.hub_token.clone(),
                messages_pool.hub_token.clone(),
            ],
            Some(product_runtime_protocol_and_secret_probe(
                secret_values.clone(),
                responses_pool.hub_token.clone(),
                messages_pool.hub_token.clone(),
                responses_model.clone(),
                messages_model.clone(),
            )),
        )
        .await
        .map_err(|_| "successful trial task panicked".to_string())?
        .map_err(|error| error.to_string())?;
    ensure(
        report.port == PRODUCT_PORT
            && report.rust_entry_count == 2
            && report.prepared_hash_matched
            && report.rust_stopped_before_go
            && report.rust_mutator_blocked
            && report.product_health_ready
            && report.go_stopped_before_restore
            && report.rust_exact_restored,
        "successful Product handoff trial omitted required evidence",
    )?;
    ensure_rust_restored(&runtime, 2)?;

    let product_request_failure_observed = Arc::new(AtomicBool::new(false));
    let product_request_failure = runtime
        .spawn_product_handoff_trial(
            Arc::clone(&lifecycle),
            Arc::clone(&coordinator),
            vec![
                responses_pool.hub_token.clone(),
                messages_pool.hub_token.clone(),
            ],
            Some(product_runtime_expected_request_failure_probe(
                secret_values.clone(),
                responses_pool.hub_token.clone(),
                responses_model.clone(),
                Arc::clone(&product_request_failure_observed),
            )),
        )
        .await
        .map_err(|_| "failed-product-request trial task panicked".to_string())?
        .expect_err("controlled upstream failure must fail the Product runtime probe");
    let product_request_failure_compensated = product_request_failure_observed
        .load(Ordering::SeqCst)
        && product_request_failure.stage == "product_request"
        && product_request_failure.go_stopped
        && product_request_failure.rust_restored;
    ensure(
        product_request_failure_compensated,
        "failed Product request did not compensate in the safe order",
    )?;
    ensure_rust_restored(&runtime, 2)?;
    ensure(
        critical_db_hash(&data_dir)? == db_before,
        "failed Product request changed the database or selection state",
    )?;

    let failed = runtime
        .spawn_product_handoff_trial(
            Arc::clone(&lifecycle),
            Arc::clone(&coordinator),
            vec![WRONG_BEARER.into()],
            Some(product_runtime_secret_probe(secret_values.clone())),
        )
        .await
        .map_err(|_| "failed-health trial task panicked".to_string())?
        .expect_err("wrong bearer must fail the Product health stage");
    ensure(
        failed.stage == "product_health" && failed.go_stopped && failed.rust_restored,
        "failed Product health did not compensate in the safe order",
    )?;
    ensure_rust_restored(&runtime, 2)?;

    // Drop a normal caller's handle, then require the detached task to report
    // its full result through an in-process-only completion channel. This
    // avoids timing assumptions about when an executor polls a queued task.
    let (detached_completion, detached_result) = tokio::sync::oneshot::channel();
    let detached = runtime.spawn_product_handoff_trial_with_completion(
        Arc::clone(&lifecycle),
        Arc::clone(&coordinator),
        vec![
            responses_pool.hub_token.clone(),
            messages_pool.hub_token.clone(),
        ],
        Some(product_runtime_secret_probe(secret_values.clone())),
        Some(detached_completion),
    );
    drop(detached);
    let detached_report = tokio::time::timeout(Duration::from_secs(30), detached_result)
        .await
        .map_err(|_| "detached handoff trial did not complete".to_string())?
        .map_err(|_| "detached handoff trial dropped its completion evidence".to_string())?
        .map_err(|error| error.to_string())?;
    ensure(
        detached_report.rust_exact_restored && detached_report.go_stopped_before_restore,
        "detached handoff trial omitted restoration evidence",
    )?;
    ensure_rust_restored(&runtime, 2)?;
    ensure(
        critical_db_hash(&data_dir)? == db_before,
        "handoff trial changed the database or selection state",
    )?;

    runtime
        .shutdown()
        .await
        .map_err(|_| "final runtime shutdown failed".to_string())?;
    let released = TcpListener::bind(("127.0.0.1", PRODUCT_PORT)).is_ok();
    ensure(
        released,
        "final probe shutdown did not release the saved port",
    )?;
    let secret_free_evidence = !file_contains_any(
        &run_log,
        &secret_values
            .iter()
            .map(String::as_bytes)
            .collect::<Vec<_>>(),
    )?;
    ensure(
        secret_free_evidence,
        "probe output exposed a synthetic secret",
    )?;

    Ok(json!({
        "schema": "route-runtime-product-handoff-probe.v1",
        "status": "ok",
        "port": PRODUCT_PORT,
        "rust_entry_count": report.rust_entry_count,
        "detached_caller_drop_compensated": true,
        "health_failure_compensated": true,
        "product_request_failure_compensated": product_request_failure_compensated,
        "prepared_hash_matched": report.prepared_hash_matched,
        "rust_stopped_before_go": report.rust_stopped_before_go,
        "rust_mutator_blocked": report.rust_mutator_blocked,
        "product_health_ready": report.product_health_ready,
        "synthetic_protocol_requests_succeeded": true,
        "go_stopped_before_restore": report.go_stopped_before_restore,
        "rust_exact_restored": report.rust_exact_restored,
        "database_and_selection_unchanged": true,
        "final_port_released": released,
        "secret_free_evidence": secret_free_evidence,
    }))
}

fn configure_pool(
    hub: &AgentHub,
    agent: AgentId,
    surface: RouteDownstreamSurface,
    source_id: &str,
) -> ProbeResult<agenthub_core::models::RoutePool> {
    let pool = hub
        .route_pools()
        .ensure_default_pool(agent, surface)
        .map_err(|_| "create route pool failed".to_string())?;
    hub.route_pools()
        .add_member(&pool.id, AdapterSourceKind::Provider, source_id)
        .map_err(|_| "attach synthetic provider failed".to_string())?;
    let pool = hub
        .route_pools()
        .enroll_unified_gateway_as_default(&pool.id, PRODUCT_PORT)
        .map_err(|_| "save Product port failed".to_string())?;
    ensure(
        pool.gateway_port == Some(PRODUCT_PORT),
        "saved Product port was not retained",
    )?;
    Ok(pool)
}

fn ensure_rust_restored(runtime: &RouteRuntimeManager, expected: usize) -> ProbeResult<()> {
    let rust = runtime.rust_host_for_bridge_saga();
    let statuses = rust
        .statuses()
        .map_err(|_| "read Rust gateway status failed".to_string())?;
    ensure(
        rust.gateway_port().ok().flatten() == Some(PRODUCT_PORT)
            && statuses.len() == expected
            && statuses
                .iter()
                .all(|status| status.running && status.port == PRODUCT_PORT),
        "Rust gateway was not exactly restored",
    )
}

fn critical_db_hash(data: &Path) -> ProbeResult<String> {
    let mut hasher = Sha256::new();
    for name in ["agenthub.db", "agenthub.db-wal"] {
        let path = data.join(name);
        hasher.update(name.as_bytes());
        if path.exists() {
            let bytes = fs::read(path).map_err(|_| "read critical database file failed")?;
            hasher.update(bytes.len().to_le_bytes());
            hasher.update(bytes);
        } else {
            hasher.update(0_usize.to_le_bytes());
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn product_runtime_secret_probe(secret_values: Vec<String>) -> ProductHandoffTrialRuntimeProbe {
    Box::new(move |host| {
        runtime_secrets_are_absent(host, &secret_values)
            .then_some(())
            .ok_or(ProductHandoffTrialProbeFailure::RuntimeSecretScan)
    })
}

fn product_runtime_protocol_and_secret_probe(
    secret_values: Vec<String>,
    responses_bearer: String,
    messages_bearer: String,
    responses_model: String,
    messages_model: String,
) -> ProductHandoffTrialRuntimeProbe {
    Box::new(move |host| {
        if !product_data_plane_is_ready(
            &responses_bearer,
            &messages_bearer,
            &responses_model,
            &messages_model,
        ) {
            return Err(ProductHandoffTrialProbeFailure::ProductRequest);
        }
        runtime_secrets_are_absent(host, &secret_values)
            .then_some(())
            .ok_or(ProductHandoffTrialProbeFailure::RuntimeSecretScan)
    })
}

fn product_runtime_expected_request_failure_probe(
    secret_values: Vec<String>,
    responses_bearer: String,
    responses_model: String,
    observed: Arc<AtomicBool>,
) -> ProductHandoffTrialRuntimeProbe {
    Box::new(move |host| {
        let request_failed =
            product_data_plane_failure_is_observed(&responses_bearer, &responses_model);
        if !runtime_secrets_are_absent(host, &secret_values) {
            return Err(ProductHandoffTrialProbeFailure::RuntimeSecretScan);
        }
        if !request_failed {
            return Err(ProductHandoffTrialProbeFailure::ProductRequest);
        }
        observed.store(true, Ordering::SeqCst);
        Err(ProductHandoffTrialProbeFailure::ProductRequest)
    })
}

fn runtime_secrets_are_absent(
    host: &crate::go_route_isolated::GoRouteIsolatedHost,
    secret_values: &[String],
) -> bool {
    #[cfg(all(target_os = "linux", feature = "go-route-tcp-control-probe"))]
    {
        let Ok((pid, product_home, staging_home)) = host.probe_session_process() else {
            return false;
        };
        let needles = secret_values
            .iter()
            .map(String::as_bytes)
            .collect::<Vec<_>>();
        host.probe_tcp_control_startup_secret_scan()
            .unwrap_or(false)
            && matches!(
                file_contains_any(&PathBuf::from(format!("/proc/{pid}/cmdline")), &needles),
                Ok(false)
            )
            && matches!(
                file_contains_any(&PathBuf::from(format!("/proc/{pid}/environ")), &needles),
                Ok(false)
            )
            && matches!(
                regular_tree_contains_any(&product_home, &needles),
                Ok(false)
            )
            && matches!(
                regular_tree_contains_any(&staging_home, &needles),
                Ok(false)
            )
    }
    #[cfg(not(all(target_os = "linux", feature = "go-route-tcp-control-probe")))]
    {
        let _ = (host, secret_values);
        false
    }
}

fn product_data_plane_is_ready(
    responses_bearer: &str,
    messages_bearer: &str,
    responses_model: &str,
    messages_model: &str,
) -> bool {
    let responses = post_product_json(
        "/v1/responses",
        responses_bearer,
        json!({
            "model": responses_model,
            "input": RESPONSES_REQUEST_MARKER,
            "stream": false,
        }),
    );
    let messages = post_product_json(
        "/v1/messages",
        messages_bearer,
        json!({
            "model": messages_model,
            "max_tokens": 16,
            "messages": [{"role": "user", "content": MESSAGES_REQUEST_MARKER}],
            "stream": false,
        }),
    );
    responses.is_some_and(|response| {
        response.get("object").and_then(Value::as_str) == Some("response")
            && response.get("status").and_then(Value::as_str) == Some("completed")
            && response.get("model").and_then(Value::as_str) == Some(responses_model)
            && response_output_has_text(&response, RESPONSES_RESPONSE_MARKER)
    }) && messages.is_some_and(|message| {
        message.get("type").and_then(Value::as_str) == Some("message")
            && message.get("role").and_then(Value::as_str) == Some("assistant")
            && message.get("model").and_then(Value::as_str) == Some(messages_model)
            && message_content_has_text(&message, MESSAGES_RESPONSE_MARKER)
    })
}

fn product_data_plane_failure_is_observed(responses_bearer: &str, responses_model: &str) -> bool {
    matches!(
        post_product_status(
            "/v1/responses",
            responses_bearer,
            json!({
                "model": responses_model,
                "input": RESPONSES_FAILURE_REQUEST_MARKER,
                "stream": false,
            }),
        ),
        Some(status) if (500..600).contains(&status)
    )
}

fn post_product_json(path: &str, bearer: &str, body: Value) -> Option<Value> {
    let (status, bytes) = post_product_bytes(path, bearer, body)?;
    (status == 200).then(|| serde_json::from_slice(&bytes).ok())?
}

fn post_product_status(path: &str, bearer: &str, body: Value) -> Option<u16> {
    post_product_bytes(path, bearer, body).map(|(status, _)| status)
}

fn post_product_bytes(path: &str, bearer: &str, body: Value) -> Option<(u16, Vec<u8>)> {
    const MAX_BODY_BYTES: u64 = 32 * 1024;
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
        .take(MAX_BODY_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_BODY_BYTES {
        return None;
    }
    Some((status, bytes))
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

fn message_content_has_text(message: &Value, expected: &str) -> bool {
    message
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|part| {
            part.get("type").and_then(Value::as_str) == Some("text")
                && part.get("text").and_then(Value::as_str) == Some(expected)
        })
}

const MAX_SECRET_SCAN_FILE_BYTES: u64 = 4 * 1024 * 1024;

fn regular_tree_contains_any(root: &Path, needles: &[&[u8]]) -> ProbeResult<bool> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|_| "inspect Product runtime file during secret scan failed".to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("refusing symlink during secret scan".to_string());
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(root)
            .map_err(|_| "read Product runtime directory during secret scan failed".to_string())?
        {
            let entry =
                entry.map_err(|_| "read Product runtime entry during secret scan failed")?;
            if regular_tree_contains_any(&entry.path(), needles)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    file_contains_any(root, needles)
}

fn file_contains_any(path: &Path, needles: &[&[u8]]) -> ProbeResult<bool> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| "inspect Product runtime file during secret scan failed".to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("refusing symlink during secret scan".to_string());
    }
    if !metadata.is_file() {
        return Ok(false);
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| {
            file.take(MAX_SECRET_SCAN_FILE_BYTES.saturating_add(1))
                .read_to_end(&mut bytes)
        })
        .map_err(|_| "read Product runtime file during secret scan failed".to_string())?;
    if bytes.len() as u64 > MAX_SECRET_SCAN_FILE_BYTES {
        return Err("Product runtime file exceeded secret scan limit".to_string());
    }
    Ok(needles.iter().any(|needle| {
        !needle.is_empty() && bytes.windows(needle.len()).any(|part| part == *needle)
    }))
}

fn ensure_loopback_url(raw: &str) -> ProbeResult<()> {
    let authority = raw
        .strip_prefix("http://")
        .or_else(|| raw.strip_prefix("https://"))
        .and_then(|rest| rest.split('/').next())
        .filter(|value| !value.is_empty() && !value.contains('@'))
        .ok_or_else(|| "upstream URL must be loopback HTTP(S)".to_string())?;
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
    let root = fs::canonicalize(root).map_err(|_| "canonicalize scratch failed")?;
    let temp = fs::canonicalize(std::env::temp_dir())
        .map_err(|_| "canonicalize operating-system temp failed")?;
    ensure(
        root.starts_with(&temp) && root != temp,
        "scratch must be a disposable child of the operating-system temp directory",
    )?;
    Ok(root)
}

fn validate_run_log(root: &Path, run_log: &Path) -> ProbeResult<PathBuf> {
    let expected = root.join("run.log");
    ensure(
        run_log == expected,
        "probe output log must be the scratch run.log file",
    )?;
    let metadata = fs::symlink_metadata(&expected)
        .map_err(|_| "inspect probe output log failed".to_string())?;
    ensure(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "probe output log is not a regular scratch file",
    )?;
    Ok(expected)
}

fn create_private_dir(path: &Path) -> ProbeResult<()> {
    fs::create_dir(path).map_err(|_| "create private probe directory failed")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| "secure private probe directory failed")?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_existing_data_dir_probe_mode(path: &Path) -> ProbeResult<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o710))
        .map_err(|_| "set disposable data directory mode failed".to_string())
}

#[cfg(not(unix))]
fn set_existing_data_dir_probe_mode(_path: &Path) -> ProbeResult<()> {
    Ok(())
}

fn ensure(condition: bool, message: &str) -> ProbeResult<()> {
    condition.then_some(()).ok_or_else(|| message.to_string())
}
