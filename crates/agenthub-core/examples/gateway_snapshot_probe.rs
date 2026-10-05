//! Disposable real-listener probe for Rust gateway capture/stop/exact restore.

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use agenthub_core::bridge::host::{
    BridgeGatewayCleanupStatus, BridgeGatewaySnapshotState, BridgeGatewayStopState, BridgeHostError,
};
use agenthub_core::bridge::{
    BridgeLocalSurface, BridgeRuntimeHost, BridgeStartSpec, BridgeUpstreamConfig,
    BridgeUpstreamProtocol, BridgeUpstreamStatus, MemberHealth, ResolvedAuth,
};
use agenthub_core::models::RouteSchedulePolicy;
use agenthub_core::AgentHub;
use serde_json::json;
use sha2::{Digest, Sha256};

const LOCAL_KEY_A: &str = "ahb-gateway-snapshot-local-a-secret";
const LOCAL_KEY_B: &str = "ahb-gateway-snapshot-local-b-secret";
const EXTRA_KEY: &str = "ahb-gateway-snapshot-extra-secret";
const UPSTREAM_KEY: &str = "sk-gateway-snapshot-upstream-secret";
const WRONG_KEY: &str = "ahb-gateway-snapshot-wrong-secret";

type ProbeResult<T> = Result<T, String>;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("gateway snapshot probe failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> ProbeResult<()> {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| "usage: gateway_snapshot_probe <scratch>".to_string())?;
    if std::env::args_os().nth(2).is_some() {
        return Err("unexpected extra arguments".into());
    }
    let root = validate_scratch(&root)?;
    let data = root.join("data");
    let skills = root.join("skills");
    fs::create_dir_all(&data).map_err(|_| "create data directory failed".to_string())?;
    fs::create_dir_all(&skills).map_err(|_| "create skills directory failed".to_string())?;
    let _hub = AgentHub::open_with_skills_root(Some(&data), Some(&skills))
        .map_err(|_| "open scratch AgentHub failed".to_string())?;
    let db_before = critical_db_hash(&data)?;

    let host = BridgeRuntimeHost::new();
    let first = host
        .start(spec("snapshot-a", 0, LOCAL_KEY_A))
        .await
        .map_err(|error| format!("start first entry: {error}"))?;
    let port = first.port;
    host.start(spec("snapshot-b", port, LOCAL_KEY_B))
        .await
        .map_err(|error| format!("start second entry: {error}"))?;
    host.set_extra_local_bearers(vec![(EXTRA_KEY.into(), "snapshot-a".into())])
        .map_err(|error| format!("set extra bearer: {error}"))?;
    host.record_upstream_outcome("snapshot-a", BridgeUpstreamStatus::Connected)
        .map_err(display)?;
    host.record_upstream_outcome("snapshot-b", BridgeUpstreamStatus::Degraded)
        .map_err(display)?;

    let snapshot = host
        .capture_gateway_snapshot()
        .map_err(|error| format!("capture gateway: {error}"))?;
    ensure(snapshot.port() == port, "snapshot lost the cited port")?;
    ensure(snapshot.entry_count() == 2, "snapshot lost a live entry")?;
    let debug = format!("{snapshot:?}");
    ensure(
        !contains_secret(&debug),
        "snapshot Debug exposed a synthetic secret",
    )?;
    ensure(
        matches!(
            host.start(spec("blocked", port, "ahb-blocked")).await,
            Err(BridgeHostError::GatewayTransitionActive)
        ),
        "transition did not block a concurrent start",
    )?;
    verify_mutator_gate(&host, port).await?;

    let stopped = host
        .stop_gateway_snapshot(&snapshot)
        .await
        .map_err(|error| format!("stop snapshot: {error}"))?;
    ensure(
        stopped.state == BridgeGatewayStopState::Stopped,
        "snapshot stop was not observed stopped",
    )?;
    ensure(
        snapshot.state() == BridgeGatewaySnapshotState::Stopped,
        "snapshot state did not advance to stopped",
    )?;
    ensure(
        host.statuses().map_err(display)?.is_empty(),
        "stop left an entry",
    )?;
    ensure(
        host.gateway_port().map_err(display)?.is_none(),
        "stop left the gateway socket",
    )?;

    let occupied = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("occupy captured port: {error}"))?;
    let restore_error = host
        .restore_gateway_snapshot(&snapshot)
        .await
        .expect_err("occupied exact port must reject restore");
    ensure(
        matches!(restore_error.cause_error(), BridgeHostError::Bind(_)),
        "occupied restore did not return Bind",
    )?;
    ensure(
        restore_error.cleanup_status() == BridgeGatewayCleanupStatus::NotRequired,
        "first-entry bind failure reported unexpected cleanup",
    )?;
    ensure(
        host.statuses().map_err(display)?.is_empty()
            && host.gateway_port().map_err(display)?.is_none(),
        "failed restore left a partial runtime",
    )?;
    ensure(
        !contains_secret(&format!("{restore_error:?}")),
        "restore error exposed a synthetic secret",
    )?;
    drop(occupied);

    host.restore_gateway_snapshot(&snapshot)
        .await
        .map_err(|error| format!("retry exact restore: {error}"))?;
    let statuses = host.statuses().map_err(display)?;
    ensure(
        statuses.len() == 2
            && statuses
                .iter()
                .all(|status| status.running && status.port == port),
        "successful restore did not recreate every entry on the exact port",
    )?;
    ensure(
        statuses.iter().any(|status| {
            status.profile_id == "snapshot-a"
                && status.upstream_status == BridgeUpstreamStatus::Connected
        }) && statuses.iter().any(|status| {
            status.profile_id == "snapshot-b"
                && status.upstream_status == BridgeUpstreamStatus::Degraded
        }),
        "restore did not preserve distinct upstream observations",
    )?;
    ensure(
        health_status(port, LOCAL_KEY_A).await? == 200,
        "key A health failed",
    )?;
    ensure(
        health_status(port, LOCAL_KEY_B).await? == 200,
        "key B health failed",
    )?;
    ensure(
        health_status(port, EXTRA_KEY).await? == 200,
        "extra key health failed",
    )?;
    ensure(
        health_status(port, WRONG_KEY).await? == 401,
        "wrong key was accepted",
    )?;
    ensure(
        health_upstream_status(port, LOCAL_KEY_A).await? == "connected",
        "key A health JSON lost connected state",
    )?;
    ensure(
        health_upstream_status(port, LOCAL_KEY_B).await? == "degraded",
        "key B health JSON lost degraded state",
    )?;
    ensure(
        health_upstream_status(port, EXTRA_KEY).await? == "connected",
        "extra key health JSON did not follow its edge",
    )?;

    host.stop("snapshot-a")
        .await
        .map_err(|error| format!("stop restored A: {error}"))?;
    host.stop("snapshot-b")
        .await
        .map_err(|error| format!("stop restored B: {error}"))?;
    let rebound = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("final port was not released: {error}"))?;
    drop(rebound);
    verify_captured_drop_releases().await?;
    verify_stopped_drop_fail_closed().await?;
    verify_explicit_commit_releases().await?;
    verify_observed_stop_error_restores().await?;
    verify_second_entry_failure_cleanup().await?;
    verify_health_failure_cleanup().await?;
    verify_strict_admission_drain().await?;
    ensure(
        critical_db_hash(&data)? == db_before,
        "gateway snapshot changed agenthub.db or its WAL",
    )?;

    println!(
        "{}",
        json!({
            "schema": "gateway-snapshot-probe.v1",
            "status": "ok",
            "entry_count": 2,
            "same_port_restore": true,
            "occupied_port_bind_error": true,
            "no_partial_entry": true,
            "retry_same_snapshot": true,
            "authorized_health": true,
            "unauthorized_health": true,
            "db_wal_unchanged": true,
            "port_released": true,
            "secret_scan": true,
            "captured_drop_releases": true,
            "stopped_drop_fail_closed": true,
            "explicit_commit_releases": true,
            "mutator_gate": true,
            "observed_stop_error_restores": true,
            "second_entry_cleanup": true,
            "health_failure_cleanup": true,
            "observed_upstream_preserved": true,
            "strict_admission_drain": true,
        })
    );
    Ok(())
}

async fn verify_mutator_gate(host: &BridgeRuntimeHost, port: u16) -> ProbeResult<()> {
    ensure_transition(
        host.set_extra_local_bearers(Vec::new()),
        "set extra bearers",
    )?;
    ensure_transition(
        host.apply_pool_schedule_policy("snapshot-a", RouteSchedulePolicy::RoundRobin),
        "apply schedule",
    )?;
    ensure_transition(
        host.apply_account_quota("snapshot-a-source", Some(50.0), None, None, false),
        "apply quota",
    )?;
    ensure_transition(
        host.restore_member_health("snapshot-a", "snapshot-a-source", MemberHealth::Renewable),
        "restore member health",
    )?;
    ensure_transition(
        host.record_upstream_outcome("snapshot-a", BridgeUpstreamStatus::Connected),
        "record upstream outcome",
    )?;
    ensure_transition(host.set_gateway_port(port + 1).await, "set gateway port")?;
    ensure_transition(host.stop("snapshot-a").await, "stop entry")?;
    ensure_transition(host.shutdown().await, "shutdown host")?;
    Ok(())
}

async fn verify_captured_drop_releases() -> ProbeResult<()> {
    let host = BridgeRuntimeHost::new();
    let started = host
        .start(spec("captured-drop", 0, LOCAL_KEY_A))
        .await
        .map_err(display)?;
    let snapshot = host.capture_gateway_snapshot().map_err(display)?;
    drop(snapshot);
    host.set_gateway_port(started.port)
        .await
        .map_err(|error| format!("captured drop did not release transition: {error}"))?;
    host.stop("captured-drop").await.map_err(display)?;
    Ok(())
}

async fn verify_stopped_drop_fail_closed() -> ProbeResult<()> {
    let host = BridgeRuntimeHost::new();
    host.start(spec("stopped-drop", 0, LOCAL_KEY_A))
        .await
        .map_err(display)?;
    let snapshot = host.capture_gateway_snapshot().map_err(display)?;
    let report = host
        .stop_gateway_snapshot(&snapshot)
        .await
        .map_err(display)?;
    ensure(
        report.state == BridgeGatewayStopState::Stopped,
        "stopped-drop setup did not stop",
    )?;
    drop(snapshot);
    ensure_transition(
        host.start(spec("must-stay-blocked", 0, LOCAL_KEY_B)).await,
        "stopped snapshot drop",
    )
}

async fn verify_explicit_commit_releases() -> ProbeResult<()> {
    let host = BridgeRuntimeHost::new();
    host.start(spec("commit", 0, LOCAL_KEY_A))
        .await
        .map_err(display)?;
    let snapshot = host.capture_gateway_snapshot().map_err(display)?;
    let report = host
        .stop_gateway_snapshot(&snapshot)
        .await
        .map_err(display)?;
    ensure(
        report.state == BridgeGatewayStopState::Stopped,
        "commit setup did not stop",
    )?;
    host.commit_stopped_gateway_snapshot(&snapshot)
        .await
        .map_err(display)?;
    ensure(
        snapshot.state() == BridgeGatewaySnapshotState::Committed,
        "explicit commit did not advance state",
    )?;
    drop(snapshot);
    host.set_extra_local_bearers(Vec::new())
        .map_err(|error| format!("explicit commit did not release transition: {error}"))?;
    Ok(())
}

async fn verify_observed_stop_error_restores() -> ProbeResult<()> {
    let (host, snapshot, _) = pair_snapshot("stop-error").await?;
    snapshot.probe_set_faults(true, false, false);
    let report = host
        .stop_gateway_snapshot(&snapshot)
        .await
        .map_err(display)?;
    ensure(
        report.state == BridgeGatewayStopState::Stopped && report.stop_error_count > 0,
        "injected post-cleanup stop error was not observed as stopped",
    )?;
    host.restore_gateway_snapshot(&snapshot)
        .await
        .map_err(|error| format!("observed stopped error blocked restore: {error}"))?;
    stop_pair(&host, "stop-error").await
}

async fn verify_second_entry_failure_cleanup() -> ProbeResult<()> {
    let (host, snapshot, _) = pair_snapshot("second-failure").await?;
    snapshot.probe_set_faults(false, true, false);
    let error = host
        .restore_gateway_snapshot(&snapshot)
        .await
        .expect_err("second entry fault must fail restore");
    ensure(
        error.cleanup_status() == BridgeGatewayCleanupStatus::Complete,
        "second entry failure did not report complete cleanup",
    )?;
    ensure_host_empty(&host, "second entry failure")?;
    snapshot.probe_set_faults(false, false, false);
    host.restore_gateway_snapshot(&snapshot)
        .await
        .map_err(|error| format!("second entry cleanup snapshot was not retryable: {error}"))?;
    stop_pair(&host, "second-failure").await
}

async fn verify_health_failure_cleanup() -> ProbeResult<()> {
    let (host, snapshot, _) = pair_snapshot("health-failure").await?;
    snapshot.probe_set_faults(false, false, true);
    let error = host
        .restore_gateway_snapshot(&snapshot)
        .await
        .expect_err("health fault must fail restore");
    ensure(
        matches!(
            error.cause_error(),
            BridgeHostError::GatewaySnapshotHealthFailed
        ) && error.cleanup_status() == BridgeGatewayCleanupStatus::Complete,
        "health failure did not report complete cleanup",
    )?;
    ensure_host_empty(&host, "health failure")?;
    snapshot.probe_set_faults(false, false, false);
    host.restore_gateway_snapshot(&snapshot)
        .await
        .map_err(|error| format!("health cleanup snapshot was not retryable: {error}"))?;
    stop_pair(&host, "health-failure").await
}

async fn verify_strict_admission_drain() -> ProbeResult<()> {
    let host = BridgeRuntimeHost::new();
    let started = host
        .start(spec("strict-drain", 0, LOCAL_KEY_A))
        .await
        .map_err(display)?;
    host.record_upstream_outcome("strict-drain", BridgeUpstreamStatus::Connected)
        .map_err(display)?;
    let snapshot = host.capture_gateway_snapshot().map_err(display)?;
    let held_admission = host.probe_hold_admission("strict-drain").map_err(display)?;

    let report = host
        .stop_gateway_snapshot(&snapshot)
        .await
        .map_err(display)?;
    ensure(
        report.state == BridgeGatewayStopState::Partial,
        "held admission was incorrectly reported stopped",
    )?;
    let status = host
        .status("strict-drain")
        .map_err(display)?
        .ok_or_else(|| "strict drain removed the runtime before admission returned".to_string())?;
    ensure(
        status.state == agenthub_core::bridge::BridgeRuntimeState::Stopping
            && host.gateway_port().map_err(display)? == Some(started.port),
        "strict drain did not retain its stopping runtime and socket",
    )?;
    ensure(
        host.commit_stopped_gateway_snapshot(&snapshot)
            .await
            .is_err(),
        "partial snapshot stop was allowed to commit",
    )?;
    let restore_error = host
        .restore_gateway_snapshot(&snapshot)
        .await
        .expect_err("partial snapshot stop was allowed to restore");
    ensure(
        restore_error.cleanup_status() == BridgeGatewayCleanupStatus::Partial,
        "partial restore rejection lost its cleanup state",
    )?;
    ensure_transition(
        host.set_extra_local_bearers(Vec::new()),
        "partial snapshot mutator gate",
    )?;

    drop(held_admission);
    let report = host
        .stop_gateway_snapshot(&snapshot)
        .await
        .map_err(display)?;
    ensure(
        report.state == BridgeGatewayStopState::Stopped,
        "released admission did not permit a strict stop retry",
    )?;
    host.restore_gateway_snapshot(&snapshot)
        .await
        .map_err(|error| format!("strict drain snapshot did not restore: {error}"))?;
    let status = host
        .status("strict-drain")
        .map_err(display)?
        .ok_or_else(|| "strict drain restore lost its runtime".to_string())?;
    ensure(
        status.upstream_status == BridgeUpstreamStatus::Connected,
        "strict drain restore lost the final upstream observation",
    )?;
    ensure(
        health_upstream_status(started.port, LOCAL_KEY_A).await? == "connected",
        "strict drain restore lost health or local authentication",
    )?;
    host.stop("strict-drain").await.map_err(display)?;
    Ok(())
}

async fn pair_snapshot(
    prefix: &str,
) -> ProbeResult<(
    BridgeRuntimeHost,
    agenthub_core::bridge::host::BridgeGatewaySnapshot,
    u16,
)> {
    let host = BridgeRuntimeHost::new();
    let first_id = format!("{prefix}-a");
    let second_id = format!("{prefix}-b");
    let first = host
        .start(spec(&first_id, 0, LOCAL_KEY_A))
        .await
        .map_err(display)?;
    host.start(spec(&second_id, first.port, LOCAL_KEY_B))
        .await
        .map_err(display)?;
    let snapshot = host.capture_gateway_snapshot().map_err(display)?;
    Ok((host, snapshot, first.port))
}

async fn stop_pair(host: &BridgeRuntimeHost, prefix: &str) -> ProbeResult<()> {
    host.stop(&format!("{prefix}-a")).await.map_err(display)?;
    host.stop(&format!("{prefix}-b")).await.map_err(display)?;
    Ok(())
}

fn ensure_host_empty(host: &BridgeRuntimeHost, context: &str) -> ProbeResult<()> {
    ensure(
        host.statuses().map_err(display)?.is_empty()
            && host.gateway_port().map_err(display)?.is_none(),
        &format!("{context} left a partial runtime"),
    )
}

fn ensure_transition<T>(result: Result<T, BridgeHostError>, context: &str) -> ProbeResult<()> {
    ensure(
        matches!(result, Err(BridgeHostError::GatewayTransitionActive)),
        &format!("{context} bypassed the transition gate"),
    )
}

fn spec(profile_id: &str, port: u16, local_key: &str) -> BridgeStartSpec {
    BridgeStartSpec::new(
        profile_id,
        port,
        local_key,
        BridgeUpstreamConfig {
            base_url: "http://127.0.0.1:9/v1".into(),
            model: Some("snapshot-probe-model".into()),
            source_id: Some(format!("{profile_id}-source")),
            auth: ResolvedAuth::bearer(UPSTREAM_KEY),
            protocol: BridgeUpstreamProtocol::OpenAiChatCompletions,
            local_surface: BridgeLocalSurface::Responses,
        },
    )
}

async fn health_status(port: u16, key: &str) -> ProbeResult<u16> {
    let response = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|_| "build local health client failed".to_string())?
        .get(format!("http://127.0.0.1:{port}/health"))
        .bearer_auth(key)
        .send()
        .await
        .map_err(|_| "local health request failed".to_string())?;
    Ok(response.status().as_u16())
}

async fn health_upstream_status(port: u16, key: &str) -> ProbeResult<String> {
    let response = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|_| "build local health client failed".to_string())?
        .get(format!("http://127.0.0.1:{port}/health"))
        .bearer_auth(key)
        .send()
        .await
        .map_err(|_| "local health request failed".to_string())?;
    ensure(response.status().as_u16() == 200, "health JSON was not 200")?;
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "health JSON parse failed".to_string())?;
    body.get("upstream_status")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "health JSON omitted upstream_status".to_string())
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

fn contains_secret(text: &str) -> bool {
    [LOCAL_KEY_A, LOCAL_KEY_B, EXTRA_KEY, UPSTREAM_KEY, WRONG_KEY]
        .iter()
        .any(|secret| text.contains(secret))
}

fn validate_scratch(requested: &Path) -> ProbeResult<PathBuf> {
    fs::create_dir_all(requested).map_err(|_| "create scratch failed".to_string())?;
    let scratch = fs::canonicalize(requested).map_err(|_| "canonicalize scratch failed")?;
    let marked = scratch.file_name().is_some_and(|name| {
        name.to_string_lossy()
            .starts_with("agenthub-gateway-snapshot.")
    });
    let under_temp = [Path::new("/tmp"), Path::new("/var/tmp")]
        .into_iter()
        .filter_map(|root| fs::canonicalize(root).ok())
        .any(|root| scratch != root && scratch.starts_with(root));
    ensure(
        marked && under_temp,
        "scratch is outside the probe temp tree",
    )?;
    Ok(scratch)
}

fn ensure(condition: bool, message: &str) -> ProbeResult<()> {
    condition.then_some(()).ok_or_else(|| message.to_string())
}

fn display(error: BridgeHostError) -> String {
    error.to_string()
}
