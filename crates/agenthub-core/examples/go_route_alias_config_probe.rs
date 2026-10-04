//! Isolated real-store probe for Go route ingress-key configuration.
//!
//! The emitted evidence deliberately contains counts and hashes only. Entry
//! keys and upstream login information must never reach probe output.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use agenthub_core::models::{AdapterSourceKind, AgentId, ProviderInput, RouteDownstreamSurface};
use agenthub_core::AgentHub;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const SOURCE_ID: &str = "go-route-alias-source";
const PROJECTED_ID_A: &str = "go-route-alias-projected-a";
const PROJECTED_ID_B: &str = "go-route-alias-projected-b";
const UPSTREAM_KEY: &str = "sk_probe_go_route_upstream_do_not_use";
const PROJECTED_ALIAS: &str = "ahb_probe_go_route_projected_do_not_use";

type ProbeResult<T> = Result<T, String>;

fn main() {
    if let Err(error) = run() {
        eprintln!("Go route alias config probe failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> ProbeResult<()> {
    let scratch = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| "usage: go_route_alias_config_probe <scratch>".to_string())?;
    if std::env::args_os().nth(2).is_some() {
        return Err("unexpected extra arguments".into());
    }
    let (data, skills) = validate_scratch(&scratch)?;
    let hub = AgentHub::open_with_skills_root(Some(&data), Some(&skills))
        .map_err(|_| "open isolated AgentHub failed".to_string())?;

    hub.providers()
        .create(&ProviderInput {
            id: SOURCE_ID.into(),
            agent_id: AgentId::WorkBuddy,
            name: "Go route alias probe source".into(),
            settings_config: json!({
                "api_key": UPSTREAM_KEY,
                "base_url": "http://127.0.0.1:18080/v1",
                "model": "probe-model",
            }),
            meta: json!({"preset": "deepseek-api"}),
            is_current: false,
        })
        .map_err(|_| "create isolated upstream failed".to_string())?;
    let pool = hub
        .route_pools()
        .ensure_default_pool(AgentId::Codex, RouteDownstreamSurface::Responses)
        .map_err(|_| "create isolated route pool failed".to_string())?;
    hub.route_pools()
        .add_member(&pool.id, AdapterSourceKind::Provider, SOURCE_ID)
        .map_err(|_| "attach isolated upstream failed".to_string())?;

    for provider_id in [PROJECTED_ID_A, PROJECTED_ID_B] {
        hub.providers()
            .create(&ProviderInput {
                id: provider_id.into(),
                agent_id: AgentId::Codex,
                name: "Go route alias probe projection".into(),
                settings_config: json!({
                    "format": "toml",
                    "auth": {"OPENAI_API_KEY": PROJECTED_ALIAS},
                }),
                meta: json!({
                    "generatedBy": "adapter",
                    "adapterProfileId": pool.id,
                }),
                is_current: false,
            })
            .map_err(|_| "create historical projection failed".to_string())?;
    }

    let pools = hub
        .route_pools()
        .list_gateway_listener_pools()
        .map_err(|_| "list listener pools failed".to_string())?;
    let before = hub
        .adapter_bridge()
        .build_go_route_isolated_config(&pools)
        .map_err(|_| "build initial Go route config failed".to_string())?;

    let extra = hub
        .route_pools()
        .create_local_token(&pool.id, "Probe extra")
        .map_err(|_| "create extra entry key failed".to_string())?;
    let after = hub
        .adapter_bridge()
        .build_go_route_isolated_config(&pools)
        .map_err(|_| "build updated Go route config failed".to_string())?;
    let repeated = hub
        .adapter_bridge()
        .build_go_route_isolated_config(&pools)
        .map_err(|_| "repeat Go route config build failed".to_string())?;

    ensure(
        after == repeated,
        "config serialization was not deterministic",
    )?;
    ensure(
        hash(&before) != hash(&after),
        "extra key did not change config hash",
    )?;
    let value: Value = serde_json::from_slice(&after)
        .map_err(|_| "parse generated Go route config failed".to_string())?;
    let edge = value
        .get("edges")
        .and_then(Value::as_array)
        .and_then(|edges| edges.iter().find(|edge| edge["id"] == pool.id))
        .ok_or_else(|| "generated config omitted route edge".to_string())?;
    ensure(
        edge.get("ingress_key").and_then(Value::as_str) == Some(pool.hub_token.as_str()),
        "primary ingress key was omitted",
    )?;
    let ingress_keys = edge
        .get("ingress_keys")
        .and_then(Value::as_array)
        .ok_or_else(|| "generated config omitted ingress_keys".to_string())?;
    let keys = ingress_keys
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    ensure(
        keys.len() == 2,
        "generated config did not deduplicate aliases",
    )?;
    ensure(
        !keys.contains(&pool.hub_token.as_str()),
        "primary key was repeated as an alias",
    )?;
    ensure(
        keys.contains(&extra.token.as_str()),
        "extra entry key was omitted",
    )?;
    ensure(
        keys.contains(&PROJECTED_ALIAS),
        "historical projected key was omitted",
    )?;
    ensure(
        keys.iter().copied().collect::<HashSet<_>>().len() == keys.len(),
        "generated config contains duplicate entry keys",
    )?;

    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema": "go-route-alias-config-probe.v1",
            "status": "ok",
            "edge_count": value["edges"].as_array().map_or(0, Vec::len),
            "ingress_key_count": keys.len() + 1,
            "primary_separate": true,
            "aliases_deduplicated": true,
            "config_hash_changes_with_keys": true,
            "deterministic_serialization": true,
        }))
        .map_err(|_| "serialize evidence failed".to_string())?
    );
    Ok(())
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_scratch(requested: &Path) -> ProbeResult<(PathBuf, PathBuf)> {
    fs::create_dir_all(requested).map_err(|_| "create scratch failed".to_string())?;
    let scratch = fs::canonicalize(requested).map_err(|_| "canonicalize scratch failed")?;
    let marked = scratch
        .components()
        .any(|part| part.as_os_str() == OsStr::new("agenthub-go-route-alias-config"));
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
