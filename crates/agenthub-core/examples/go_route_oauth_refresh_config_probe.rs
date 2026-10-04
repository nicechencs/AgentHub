//! Real-store probe for credential-safe Go OAuth refresh metadata.
//!
//! Evidence contains booleans and a configuration hash only. Account ids,
//! provider ids, entry keys, and upstream login information stay out of output.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use agenthub_core::models::{
    AccountInput, AccountKind, AdapterSourceKind, AgentId, ProviderInput, RouteDownstreamSurface,
};
use agenthub_core::AgentHub;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const PROVIDER_ID: &str = "go-route-refresh-provider-id-do-not-log";
const PROVIDER_KEY: &str = "sk_go_route_refresh_provider_do_not_log";
const OAUTH_ACCESS: &str = "oauth_go_route_refresh_access_do_not_log";
const OAUTH_REFRESH: &str = "oauth_go_route_refresh_refresh_do_not_log";

type ProbeResult<T> = Result<T, String>;

fn main() {
    if let Err(error) = run() {
        eprintln!("Go route OAuth refresh config probe failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> ProbeResult<()> {
    let scratch = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| "usage: go_route_oauth_refresh_config_probe <scratch>".to_string())?;
    if std::env::args_os().nth(2).is_some() {
        return Err("unexpected extra arguments".into());
    }
    let (data, skills) = validate_scratch(&scratch)?;
    let hub = AgentHub::open_with_skills_root(Some(&data), Some(&skills))
        .map_err(|_| "open isolated AgentHub failed".to_string())?;

    hub.providers()
        .create(&ProviderInput {
            id: PROVIDER_ID.into(),
            agent_id: AgentId::WorkBuddy,
            name: "Go route refresh probe provider".into(),
            settings_config: json!({
                "api_key": PROVIDER_KEY,
                "base_url": "http://127.0.0.1:18080/v1",
                "model": "probe-model",
            }),
            meta: json!({"preset": "deepseek-api"}),
            is_current: false,
        })
        .map_err(|_| "create isolated provider failed".to_string())?;
    let pool = hub
        .route_pools()
        .ensure_default_pool(AgentId::Codex, RouteDownstreamSurface::Responses)
        .map_err(|_| "create isolated route pool failed".to_string())?;
    hub.route_pools()
        .add_member(&pool.id, AdapterSourceKind::Provider, PROVIDER_ID)
        .map_err(|_| "attach isolated provider failed".to_string())?;

    let oauth = hub
        .accounts()
        .create(AccountInput {
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "Go route refresh probe login".into(),
            credentials: json!({
                "type": "oauth",
                "provider": "xai",
                "access_token": OAUTH_ACCESS,
                "refresh_token": OAUTH_REFRESH,
            }),
            extra: json!({"source": "oauth_pkce"}),
            is_current: false,
        })
        .map_err(|_| "create isolated OAuth login failed".to_string())?;
    let disabled = hub
        .route_pools()
        .add_member(&pool.id, AdapterSourceKind::Account, &oauth.id)
        .map_err(|_| "attach isolated OAuth login failed".to_string())?;
    hub.route_pools()
        .set_member_enabled(&disabled.id, false)
        .map_err(|_| "disable isolated OAuth login failed".to_string())?;

    let pools = hub
        .route_pools()
        .list_gateway_listener_pools()
        .map_err(|_| "list listener pools failed".to_string())?;
    let config = hub
        .adapter_bridge()
        .build_go_route_isolated_config(&pools)
        .map_err(|_| "build Go route config failed".to_string())?;
    let value: Value = serde_json::from_slice(&config)
        .map_err(|_| "parse generated Go route config failed".to_string())?;
    let member = value
        .get("edges")
        .and_then(Value::as_array)
        .and_then(|edges| edges.iter().find(|edge| edge["id"] == pool.id))
        .and_then(|edge| edge.get("members"))
        .and_then(Value::as_array)
        .and_then(|members| members.first())
        .ok_or_else(|| "generated config omitted provider member".to_string())?;
    ensure(
        member.get("source_kind").and_then(Value::as_str) == Some("provider")
            && member.get("source_id").and_then(Value::as_str) == Some(PROVIDER_ID)
            && member.get("refresh_kind").and_then(Value::as_str) == Some("none"),
        "generated member identity or refresh metadata is incorrect",
    )?;
    let member_id = member
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "generated member id is missing".to_string())?;

    ensure_rejected(hub.adapter_bridge().resolve_go_route_oauth_refresh(
        &pool.id,
        member_id,
        PROVIDER_ID,
        "none",
    ))?;
    ensure_rejected(hub.adapter_bridge().resolve_go_route_oauth_refresh(
        &pool.id,
        member_id,
        PROVIDER_ID,
        "codex_oauth",
    ))?;
    ensure_rejected(hub.adapter_bridge().resolve_go_route_oauth_refresh(
        &pool.id,
        &format!("account:{}", oauth.id),
        &oauth.id,
        "grok_oauth",
    ))?;
    ensure_rejected(hub.adapter_bridge().resolve_go_route_oauth_refresh(
        "stale-edge",
        "account:stale-member",
        "stale-source",
        "grok_oauth",
    ))?;

    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema": "go-route-oauth-refresh-config-probe.v1",
            "status": "ok",
            "config_hash": format!("{:x}", Sha256::digest(&config)),
            "identity_fields_present": true,
            "non_oauth_refresh_none": true,
            "provider_rejected": true,
            "wrong_refresh_kind_rejected": true,
            "disabled_member_rejected": true,
            "stale_request_rejected": true,
        }))
        .map_err(|_| "serialize evidence failed".to_string())?
    );
    Ok(())
}

fn ensure_rejected(result: agenthub_core::error::Result<String>) -> ProbeResult<()> {
    match result {
        Err(error) if error.code() == "adapter.go_route_oauth_refresh_rejected" => Ok(()),
        Ok(_) | Err(_) => Err("unsafe refresh request was not uniformly rejected".into()),
    }
}

fn validate_scratch(requested: &Path) -> ProbeResult<(PathBuf, PathBuf)> {
    fs::create_dir_all(requested).map_err(|_| "create scratch failed".to_string())?;
    let scratch = fs::canonicalize(requested).map_err(|_| "canonicalize scratch failed")?;
    let marked = scratch
        .components()
        .any(|part| part.as_os_str() == OsStr::new("agenthub-go-route-oauth-refresh-config"));
    let under_temp = [Path::new("/tmp"), Path::new("/var/tmp")]
        .into_iter()
        .filter_map(|root| fs::canonicalize(root).ok())
        .any(|root| scratch != root && scratch.starts_with(root));
    ensure(
        marked && under_temp,
        "scratch is outside the probe temp tree",
    )?;
    let data = scratch.join("data");
    let skills = scratch.join("skills");
    fs::create_dir_all(&data).map_err(|_| "create data directory failed".to_string())?;
    fs::create_dir_all(&skills).map_err(|_| "create skills directory failed".to_string())?;
    Ok((data, skills))
}

fn ensure(condition: bool, message: &str) -> ProbeResult<()> {
    condition.then_some(()).ok_or_else(|| message.to_string())
}
