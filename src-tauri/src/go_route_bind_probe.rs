//! Feature-gated executable probe for the desktop plan/bind/Go/unbind path.
//!
//! This module is intentionally not part of normal builds. The shell wrapper
//! supplies a disposable environment and a controlled loopback upstream.

use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use agenthub_core::adapter_control::AdapterControl;
use agenthub_core::models::{
    AdapterRoute, AdapterSourceKind, AgentId, BackupKind, Provider, ProviderInput,
    RouteDownstreamSurface, TicketBindingRoute,
};
use agenthub_core::AgentHub;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::commands::adapter::{bind_ticket_inner, plan_ticket_inner, unbind_ticket_inner};
use crate::commands::provider::{
    delete_provider_state_inner, import_provider_live_state_inner, switch_provider_state_inner,
};
use crate::state::AppState;

const SOURCE_ID: &str = "probe-openai-codex";
const SOURCE_KEY: &str = "sk-agenthub-bind-go-probe-do-not-use-000000";
const MODEL: &str = "gpt-4o";
const UPSTREAM_MARKER: &str = "agenthub-bind-go-upstream-marker";
const PRODUCT_DEFAULT_PORT: u16 = 43121;

type ProbeResult<T> = Result<T, String>;

pub fn main_entry() -> ProbeResult<()> {
    let mut args = std::env::args_os().skip(1);
    let root = args.next().map(PathBuf::from).ok_or_else(|| {
        "usage: go_route_bind_e2e_probe <scratch> <upstream-base-url>".to_string()
    })?;
    let upstream = args
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| "controlled upstream URL is required".to_string())?;
    if args.next().is_some() {
        return Err("unexpected extra arguments".into());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("create probe runtime: {error}"))?;
    let evidence = runtime.block_on(run(root, upstream))?;
    println!(
        "{}",
        serde_json::to_string(&evidence).map_err(|error| format!("encode evidence: {error}"))?
    );
    Ok(())
}

async fn run(root: PathBuf, upstream: String) -> ProbeResult<Value> {
    let paths = validate_isolation(&root)?;
    let upstream = validate_loopback_url(&upstream)?;
    let codex_config = paths.codex.join("config.toml");
    let codex_auth = paths.codex.join("auth.json");
    let seeded_config =
        b"model = \"probe-original-model\"\nmodel_reasoning_effort = \"medium\"\n".to_vec();
    let seeded_auth = b"{\n  \"OPENAI_API_KEY\": \"probe-original-local-key\"\n}\n".to_vec();
    fs::write(&codex_config, &seeded_config)
        .map_err(|error| format!("seed Codex config: {error}"))?;
    fs::write(&codex_auth, &seeded_auth).map_err(|error| format!("seed Codex auth: {error}"))?;

    let state = AppState::new();
    let hub = state.hub_arc()?;
    let original_provider = import_provider_live_state_inner(
        &state,
        AgentId::Codex,
        Some("Probe original Codex provider".into()),
    )
    .await?;
    ensure(
        !original_provider.is_current,
        "live import unexpectedly selected the Codex provider",
    )?;
    let original_selected =
        switch_provider_state_inner(&state, AgentId::Codex, original_provider.id.clone()).await?;
    ensure(
        original_selected.provider.id == original_provider.id
            && original_selected.provider.is_current,
        "imported Codex provider did not become current",
    )?;
    let config_before =
        fs::read(&codex_config).map_err(|error| format!("read selected Codex config: {error}"))?;
    let auth_before =
        fs::read(&codex_auth).map_err(|error| format!("read selected Codex auth: {error}"))?;
    hub.providers()
        .create(&ProviderInput {
            id: SOURCE_ID.into(),
            agent_id: AgentId::Codex,
            name: "Probe OpenAI provider".into(),
            settings_config: json!({
                "apiKey": SOURCE_KEY,
                "base_url": "https://api.openai.com/v1",
                "model": MODEL,
                "listedModels": [MODEL],
                "endpoints": [{"target": "codex", "url": upstream}],
            }),
            meta: json!({"preset": "openai"}),
            is_current: false,
        })
        .map_err(|error| format!("create source provider: {error}"))?;

    let ticket_id = format!("provider:{SOURCE_ID}");
    let plan = plan_ticket_inner(&state, ticket_id.clone(), AgentId::Codex).await?;
    ensure(plan.can_apply, "plan is not writable")?;
    ensure(
        plan.analysis.route == AdapterRoute::LocalBridge,
        "plan did not select local_bridge",
    )?;

    let binding = bind_ticket_inner(&state, ticket_id.clone(), AgentId::Codex)
        .await
        .map_err(gui_error)?;
    ensure(
        !binding.active,
        "first local_bridge bind unexpectedly became current",
    )?;
    ensure(
        binding.route == TicketBindingRoute::Bridge,
        "binding did not use bridge route",
    )?;
    let profile_id = binding
        .profile_id
        .clone()
        .ok_or_else(|| "binding did not return a profile".to_string())?;
    let profile = hub
        .route_pools()
        .get_adapter_profile(&profile_id)
        .map_err(|error| format!("read bound profile: {error}"))?
        .ok_or_else(|| "bound profile is missing".to_string())?;
    let generated_provider_id = profile
        .generated_provider_id
        .clone()
        .ok_or_else(|| "bound profile has no generated provider".to_string())?;
    let rust_port = profile
        .local_port
        .ok_or_else(|| "bound profile has no listener port".to_string())?;
    ensure(
        rust_port != PRODUCT_DEFAULT_PORT,
        "Rust listener used product port",
    )?;
    let switched =
        switch_provider_state_inner(&state, AgentId::Codex, generated_provider_id.clone()).await?;
    ensure(
        switched.provider.id == generated_provider_id && switched.provider.is_current,
        "generated provider switch did not become current",
    )?;
    ensure(
        fs::read(&codex_config).map_err(|error| format!("read bound Codex config: {error}"))?
            != config_before,
        "generated provider switch did not change Codex config",
    )?;
    let stored_generated = hub
        .providers()
        .get_by_id(&generated_provider_id)
        .map_err(|error| format!("read generated provider restore metadata: {error}"))?
        .ok_or_else(|| "generated provider missing after switch".to_string())?;
    ensure(
        stored_generated
            .meta
            .get("previousCurrentId")
            .and_then(Value::as_str)
            == Some(original_provider.id.as_str()),
        "generated provider did not atomically preserve the previous current provider",
    )?;
    let previous_backup_id = stored_generated
        .meta
        .get("previousBackupId")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| "generated provider did not preserve a previous backup".to_string())?;
    let previous_backup = hub
        .backups()
        .get_by_id(previous_backup_id)
        .map_err(|error| format!("read generated provider restore backup: {error}"))?;
    ensure(
        previous_backup.agent_id == Some(AgentId::Codex)
            && previous_backup.kind == BackupKind::AutoSwitch
            && !previous_backup.files.is_empty(),
        "generated provider restore backup was not a completed Codex switch snapshot",
    )?;
    probe_bridge_rollback_preserves_legacy_snapshot(
        &hub,
        &stored_generated,
        &codex_config,
        &codex_auth,
    )?;

    let request_token = binding_token(&state, &profile_id).await?;
    let listener_pools = hub
        .route_pools()
        .list_gateway_listener_pools()
        .map_err(|error| format!("list persisted listener pools: {error}"))?;
    ensure(
        listener_pools.len() == 1,
        "persisted listener pool set was not unique",
    )?;
    let persisted_pool = &listener_pools[0];
    ensure(
        persisted_pool.target_agent_id == AgentId::Codex
            && persisted_pool.downstream_surface == RouteDownstreamSurface::Responses
            && persisted_pool.unified_gateway_enrolled,
        "persisted listener pool did not match enrolled Codex Responses route",
    )?;
    ensure(
        persisted_pool.hub_token.as_bytes() == request_token.as_bytes(),
        "Rust bridge token did not match persisted pool ingress key",
    )?;
    let persisted_members = hub
        .route_pools()
        .list_members(&persisted_pool.id)
        .map_err(|error| format!("list persisted pool members: {error}"))?;
    ensure(
        persisted_members.len() == 1
            && persisted_members[0].source_kind == AdapterSourceKind::Provider
            && persisted_members[0].source_id == SOURCE_ID
            && persisted_members[0].enabled,
        "persisted listener pool did not contain the expected source member",
    )?;

    let go_host = state.go_route_isolated();
    let start_host = Arc::clone(&go_host);
    let started = tokio::task::spawn_blocking(move || start_host.start())
        .await
        .map_err(|error| format!("join Go start: {error}"))?;
    ensure(
        started.state == "ready" && started.listen_ready,
        "Go route did not become ready",
    )?;
    let go_port = started
        .port
        .ok_or_else(|| "Go route returned no port".to_string())?;
    ensure(
        go_port != PRODUCT_DEFAULT_PORT,
        "Go route used product port",
    )?;
    ensure(
        go_port != rust_port,
        "Go and Rust listeners reused one port",
    )?;
    let go_hash_before = go_host
        .probe_config_hash()
        .ok_or_else(|| "Go route has no committed config hash".to_string())?;
    let status_deadline = Instant::now() + Duration::from_secs(5);
    let go_before_unbind = loop {
        let status = go_host.status();
        if status.state == "ready"
            && status.listen_ready
            && status.port == Some(go_port)
            && status.member_count >= 1
            && status.healthy_member_count >= 1
        {
            break status;
        }
        if Instant::now() >= status_deadline {
            return Err("Go route status did not publish a healthy member".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let response = post_go(go_port, request_token)?;
    ensure(response.0 == 200, "Go request did not return HTTP 200")?;
    let response_json: Value = serde_json::from_str(&response.1)
        .map_err(|_| "Go response was not valid JSON".to_string())?;
    ensure(
        response_json.get("object").and_then(Value::as_str) == Some("response")
            && response_json.get("status").and_then(Value::as_str) == Some("completed"),
        "Go response did not use the non-streaming Responses shape",
    )?;
    ensure(
        response_output_texts(&response_json).any(|text| text == UPSTREAM_MARKER),
        "Go Responses output did not contain the upstream marker",
    )?;

    let reload_ack_count_before_delete = go_host.probe_required_reload_ack_count();
    delete_provider_state_inner(&state, AgentId::Codex, original_provider.id.clone()).await?;
    ensure(
        hub.providers()
            .get_by_id(&original_provider.id)
            .map_err(|error| format!("verify original provider deletion: {error}"))?
            .is_none(),
        "original provider still existed before unbind",
    )?;
    let reload_ack_count_before = go_host.probe_required_reload_ack_count();
    ensure(
        reload_ack_count_before == reload_ack_count_before_delete + 1,
        "original provider deletion did not complete exactly one required Go reload acknowledgement",
    )?;

    unbind_ticket_inner(&state, ticket_id, AgentId::Codex)
        .await
        .map_err(gui_error)?;
    let config_after =
        fs::read(&codex_config).map_err(|error| format!("read restored Codex config: {error}"))?;
    let auth_after =
        fs::read(&codex_auth).map_err(|error| format!("read restored Codex auth: {error}"))?;
    ensure(
        config_after == config_before,
        "Codex config bytes were not restored",
    )?;
    ensure(
        auth_after == auth_before,
        "Codex auth bytes were not restored",
    )?;
    let restored_current = hub
        .providers()
        .get_current(AgentId::Codex)
        .map_err(|error| format!("read current provider after backup restore: {error}"))?;
    ensure(
        restored_current.is_none(),
        "unbind resurrected a deleted provider or retained the generated current provider",
    )?;
    ensure(
        hub.providers()
            .get_by_id(&original_provider.id)
            .map_err(|error| format!("query deleted provider after unbind: {error}"))?
            .is_none(),
        "unbind restored the deleted provider row from backup",
    )?;
    ensure(
        hub.route_pools()
            .get_adapter_profile(&profile_id)
            .map_err(|error| format!("read removed profile: {error}"))?
            .is_none(),
        "generated profile still exists after unbind",
    )?;
    let generated_exists = hub
        .providers()
        .list(Some(AgentId::Codex))
        .map_err(|error| format!("list Codex providers: {error}"))?
        .iter()
        .any(|provider| provider.id == generated_provider_id);
    ensure(
        !generated_exists,
        "generated provider still exists after unbind",
    )?;
    ensure(
        port_released(rust_port),
        "Rust listener port was not released",
    )?;

    let go_after_unbind = go_host.status();
    let reload_ack_count_after = go_host.probe_required_reload_ack_count();
    ensure(
        reload_ack_count_after == reload_ack_count_before + 1,
        "unbind did not complete exactly one required Go reload acknowledgement",
    )?;
    ensure(
        go_after_unbind.state == "ready" && go_after_unbind.listen_ready,
        "Go route was not consistent after unbind reload",
    )?;
    let go_hash_after = go_host
        .probe_config_hash()
        .ok_or_else(|| "Go route lost its committed config hash".to_string())?;
    ensure(
        go_after_unbind.port == Some(go_port),
        "Go reload changed the listener port",
    )?;
    let unbind_reload_outcome = if go_hash_before == go_hash_after {
        "acknowledged_unchanged"
    } else {
        "acknowledged_reloaded"
    };

    let stop_host = Arc::clone(&go_host);
    let stopped = tokio::task::spawn_blocking(move || stop_host.stop())
        .await
        .map_err(|error| format!("join Go stop: {error}"))?;
    ensure(stopped.state == "stopped", "Go route did not stop")?;
    ensure(port_released(go_port), "Go listener port was not released")?;

    Ok(json!({
        "schema": "go-route-bind-e2e-probe.v1",
        "status": "ok",
        "source_fingerprint": required_env("AGENTHUB_PROBE_SOURCE_FINGERPRINT")?,
        "build_fingerprint": current_exe_sha256()?,
        "plan_route": "local_bridge",
        "rule_id": plan.analysis.rule_id,
        "first_bind_active": false,
        "generated_provider_switched_current": true,
        "restore_pointers_committed_with_switch": true,
        "restore_backup_is_completed_auto_switch": true,
        "bridge_rollback_preserved_legacy_snapshot": true,
        "persisted_pool_enrolled": true,
        "persisted_pool_member_matches_source": true,
        "persisted_pool_ingress_key_matches_request": true,
        "rust_listener_port": rust_port,
        "go_port": go_port,
        "go_state_before_unbind": go_before_unbind.state,
        "go_hash_before_unbind": go_hash_before,
        "go_member_count_before_unbind": go_before_unbind.member_count,
        "go_healthy_member_count_before_unbind": go_before_unbind.healthy_member_count,
        "http_status": response.0,
        "upstream_marker_seen": true,
        "responses_conversion_seen": true,
        "codex_bytes_restored": true,
        "original_provider_deleted_before_unbind": true,
        "deleted_provider_not_resurrected": true,
        "original_provider_absent_after_unbind": true,
        "generated_profile_removed": true,
        "generated_provider_removed": true,
        "rust_listener_stopped": true,
        "go_state_after_unbind": go_after_unbind.state,
        "go_unbind_reload_outcome": unbind_reload_outcome,
        "go_required_reload_ack_count_before_delete": reload_ack_count_before_delete,
        "go_required_reload_ack_count_after_delete": reload_ack_count_before,
        "go_required_reload_ack_count_before_unbind": reload_ack_count_before,
        "go_required_reload_ack_count_after_unbind": reload_ack_count_after,
        "go_hash_after_unbind": go_hash_after,
        "go_member_count_after_unbind": go_after_unbind.member_count,
        "go_port_stable_after_reload": true,
        "go_stopped": true,
        "go_port_released": true,
    }))
}

fn probe_bridge_rollback_preserves_legacy_snapshot(
    hub: &AgentHub,
    generated: &Provider,
    codex_config: &Path,
    codex_auth: &Path,
) -> ProbeResult<()> {
    let providers = hub.providers();
    let guard = providers
        .begin_live_saga(AgentId::Codex)
        .map_err(|error| format!("begin bridge rollback probe: {error}"))?;

    // Reproduce a legacy adapter-owned current row without first-bind
    // pointers. A failed refresh must restore this exact metadata instead of
    // initializing it from the failed projection snapshot.
    let mut legacy = generated.clone();
    let legacy_meta = legacy
        .meta
        .as_object_mut()
        .ok_or_else(|| "generated provider metadata was not an object".to_string())?;
    legacy_meta.remove("previousCurrentId");
    legacy_meta.remove("previousBackupId");
    legacy.is_current = true;
    let legacy = providers
        .update_with_guard(&guard, &provider_input(&legacy))
        .map_err(|error| format!("prepare legacy bridge rollback snapshot: {error}"))?;
    let config_before = fs::read(codex_config)
        .map_err(|error| format!("read bridge rollback Codex config: {error}"))?;
    let auth_before = fs::read(codex_auth)
        .map_err(|error| format!("read bridge rollback Codex auth: {error}"))?;
    let snapshot = hub
        .adapter_bridge()
        .capture_provider_snapshot(providers, &guard, Some(&generated.id), AgentId::Codex)
        .map_err(|error| format!("capture bridge rollback snapshot: {error}"))?;

    let mut failed_projection = legacy.clone();
    failed_projection.is_current = false;
    failed_projection
        .meta
        .as_object_mut()
        .ok_or_else(|| "legacy provider metadata was not an object".to_string())?
        .insert("probeFailedProjection".into(), json!(true));
    providers
        .update_with_guard(&guard, &provider_input(&failed_projection))
        .map_err(|error| format!("stage failed bridge projection: {error}"))?;
    hub.adapter_bridge()
        .rollback_bridge_projection(
            providers,
            &guard,
            &generated.id,
            &snapshot,
            false,
            true,
            AgentId::Codex,
        )
        .map_err(|code| format!("bridge rollback compensation failed: {code}"))?;

    let restored = providers
        .get_by_id(&generated.id)
        .map_err(|error| format!("read compensated bridge provider: {error}"))?
        .ok_or_else(|| "bridge rollback removed the generated provider".to_string())?;
    ensure(
        restored.is_current && restored.meta == legacy.meta,
        "bridge rollback did not restore legacy generated metadata/current exactly",
    )?;
    ensure(
        fs::read(codex_config)
            .map_err(|error| format!("read compensated Codex config: {error}"))?
            == config_before,
        "bridge rollback did not restore Codex config bytes exactly",
    )?;
    ensure(
        fs::read(codex_auth).map_err(|error| format!("read compensated Codex auth: {error}"))?
            == auth_before,
        "bridge rollback did not restore Codex auth bytes exactly",
    )?;

    // Return the main probe to the successfully switched first-bind state so
    // the later delete + unbind leg still proves backup fallback.
    let restored_first_bind = providers
        .update_with_guard(&guard, &provider_input(generated))
        .map_err(|error| format!("restore first-bind probe state: {error}"))?;
    ensure(
        restored_first_bind.is_current && restored_first_bind.meta == generated.meta,
        "bridge rollback probe could not restore first-bind state",
    )
}

fn provider_input(provider: &Provider) -> ProviderInput {
    ProviderInput {
        id: provider.id.clone(),
        agent_id: provider.agent_id,
        name: provider.name.clone(),
        settings_config: provider.settings_config.clone(),
        meta: provider.meta.clone(),
        is_current: provider.is_current,
    }
}

struct ProbePaths {
    codex: PathBuf,
}

fn validate_isolation(requested: &Path) -> ProbeResult<ProbePaths> {
    let requested_meta = fs::symlink_metadata(requested)
        .map_err(|error| format!("inspect pre-created scratch: {error}"))?;
    ensure(
        requested_meta.is_dir() && !requested_meta.file_type().is_symlink(),
        "scratch must be a pre-created real directory",
    )?;
    validate_private_scratch(&requested_meta)?;
    let root =
        fs::canonicalize(requested).map_err(|error| format!("canonicalize scratch: {error}"))?;
    ensure(root.is_absolute(), "scratch must be absolute")?;
    ensure(
        is_safe_temp_tree(&root),
        "scratch is outside the probe temp tree",
    )?;
    let real_home = canonical_env("AGENTHUB_PROBE_REAL_HOME")?;
    ensure(
        !root.starts_with(&real_home),
        "scratch overlaps the real user home",
    )?;
    let expected = [
        ("HOME", "home"),
        ("AGENTHUB_HOME", "agenthub"),
        ("CODEX_HOME", "codex"),
        ("XDG_CONFIG_HOME", "xdg-config"),
        ("XDG_DATA_HOME", "xdg-data"),
        ("XDG_CACHE_HOME", "xdg-cache"),
        ("XDG_STATE_HOME", "xdg-state"),
    ];
    let mut codex = None;
    for (name, child) in expected {
        let path = root.join(child);
        fs::create_dir_all(&path).map_err(|error| format!("create {name}: {error}"))?;
        let path =
            fs::canonicalize(path).map_err(|error| format!("canonicalize {name}: {error}"))?;
        ensure(
            canonical_env(name)? == path,
            &format!("{name} escaped scratch"),
        )?;
        ensure(
            !path.starts_with(&real_home),
            &format!("{name} overlaps real home"),
        )?;
        if name == "CODEX_HOME" {
            codex = Some(path);
        }
    }
    Ok(ProbePaths {
        codex: codex.ok_or_else(|| "CODEX_HOME is missing".to_string())?,
    })
}

fn is_safe_temp_tree(path: &Path) -> bool {
    let marked = path
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| {
            name.starts_with("agenthub-bind-go-e2e.") && name.len() > "agenthub-bind-go-e2e.".len()
        });
    marked
        && fs::canonicalize("/tmp")
            .ok()
            .is_some_and(|root| path.parent() == Some(root.as_path()))
}

#[cfg(unix)]
fn validate_private_scratch(metadata: &fs::Metadata) -> ProbeResult<()> {
    use std::os::unix::fs::MetadataExt;

    ensure(
        metadata.uid() == unsafe { libc::geteuid() },
        "scratch is not owned by the current user",
    )?;
    ensure(
        metadata.mode() & 0o077 == 0,
        "scratch permissions are not private",
    )
}

#[cfg(not(unix))]
fn validate_private_scratch(_metadata: &fs::Metadata) -> ProbeResult<()> {
    Ok(())
}

fn validate_loopback_url(raw: &str) -> ProbeResult<String> {
    let trimmed = raw.trim_end_matches('/');
    let port = trimmed
        .strip_prefix("http://127.0.0.1:")
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port > 0)
        .ok_or_else(|| "upstream must be http://127.0.0.1:<port>".to_string())?;
    ensure(port != PRODUCT_DEFAULT_PORT, "upstream used product port")?;
    Ok(trimmed.to_owned())
}

async fn binding_token(state: &AppState, profile_id: &str) -> ProbeResult<String> {
    state
        .adapter_control()?
        .bridge_status(profile_id.to_owned())
        .await?
        .local_token
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| "bound listener did not expose its saved ingress key".to_string())
}

fn post_go(port: u16, token: String) -> ProbeResult<(u16, String)> {
    let url = format!("http://127.0.0.1:{port}/v1/responses");
    let response = ureq::post(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(15))
        .send_json(json!({"model": MODEL, "input": "probe"}))
        .map_err(|error| format!("send Go request: {error}"))?;
    let status = response.status();
    let mut body = String::new();
    response
        .into_reader()
        .take(1 << 20)
        .read_to_string(&mut body)
        .map_err(|error| format!("read Go response: {error}"))?;
    Ok((status, body))
}

fn response_output_texts(response: &Value) -> impl Iterator<Item = &str> {
    response
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("message"))
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("output_text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
}

fn port_released(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    for _ in 0..40 {
        if TcpStream::connect_timeout(&address, Duration::from_millis(50)).is_err() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

fn required_env(name: &str) -> ProbeResult<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

fn canonical_env(name: &str) -> ProbeResult<PathBuf> {
    let raw = std::env::var_os(name).ok_or_else(|| format!("{name} is required"))?;
    fs::canonicalize(raw).map_err(|error| format!("canonicalize {name}: {error}"))
}

fn current_exe_sha256() -> ProbeResult<String> {
    let path = std::env::current_exe().map_err(|error| format!("read executable path: {error}"))?;
    let bytes = fs::read(path).map_err(|error| format!("read executable: {error}"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn ensure(condition: bool, message: &str) -> ProbeResult<()> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

fn gui_error(error: crate::commands::GuiError) -> String {
    error.message
}
