//! Disposable real-store probe for the read-only Product Go preflight.
//!
//! Every case hashes the SQLite database before and after preflight, counts
//! this process' sockets and adapterd processes, and emits only credential-free
//! summaries. The feature gate keeps the probe out of production builds.

use std::fs;
use std::path::{Path, PathBuf};

use agenthub_core::models::{AdapterSourceKind, AgentId, ProviderInput, RouteDownstreamSurface};
use agenthub_core::services::GoProductPreflightReason;
use agenthub_core::AgentHub;
use rusqlite::params;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const PORT: u16 = 43121;
const SOURCE_ID: &str = "product-preflight-source";
const SOURCE_KEY: &str = "sk-product-preflight-probe-secret-do-not-use";

type ProbeResult<T> = Result<T, String>;

#[derive(Clone, Copy)]
enum Case {
    NoPool,
    MissingPort,
    ZeroPort,
    MultiplePorts,
    UncoveredProfile,
    ProfilePortMismatch,
    ConfigIncompatible,
    Eligible,
}

impl Case {
    const ALL: [Self; 8] = [
        Self::NoPool,
        Self::MissingPort,
        Self::ZeroPort,
        Self::MultiplePorts,
        Self::UncoveredProfile,
        Self::ProfilePortMismatch,
        Self::ConfigIncompatible,
        Self::Eligible,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::NoPool => "no_pool",
            Self::MissingPort => "missing_saved_port",
            Self::ZeroPort => "zero_saved_port",
            Self::MultiplePorts => "multiple_saved_ports",
            Self::UncoveredProfile => "uncovered_legacy_profile",
            Self::ProfilePortMismatch => "profile_port_mismatch",
            Self::ConfigIncompatible => "config_incompatible",
            Self::Eligible => "eligible",
        }
    }

    fn reason(self) -> GoProductPreflightReason {
        match self {
            Self::NoPool => GoProductPreflightReason::NoPool,
            Self::MissingPort => GoProductPreflightReason::MissingSavedPort,
            Self::ZeroPort => GoProductPreflightReason::ZeroSavedPort,
            Self::MultiplePorts => GoProductPreflightReason::MultipleSavedPorts,
            Self::UncoveredProfile => GoProductPreflightReason::UncoveredLegacyProfile,
            Self::ProfilePortMismatch => GoProductPreflightReason::ProfilePortMismatch,
            Self::ConfigIncompatible => GoProductPreflightReason::ConfigIncompatible,
            Self::Eligible => GoProductPreflightReason::Eligible,
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Product preflight probe failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> ProbeResult<()> {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| "usage: go_product_preflight_probe <scratch>".to_string())?;
    if std::env::args_os().nth(2).is_some() {
        return Err("unexpected extra arguments".into());
    }
    let root = validate_scratch(&root)?;
    let adapterd_before = adapterd_process_count()?;
    let mut evidence = Vec::new();
    for case in Case::ALL {
        evidence.push(run_case(&root, case)?);
    }
    ensure(
        adapterd_process_count()? == adapterd_before,
        "preflight changed the adapterd process count",
    )?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema": "go-product-preflight-probe.v1",
            "status": "ok",
            "cases": evidence,
            "db_unchanged": true,
            "socket_count_unchanged": true,
            "adapterd_process_count_unchanged": true,
            "summary_secret_scan": true,
        }))
        .map_err(|_| "serialize probe evidence failed".to_string())?
    );
    Ok(())
}

fn run_case(root: &Path, case: Case) -> ProbeResult<Value> {
    let case_root = root.join(case.name());
    let data = case_root.join("data");
    let skills = case_root.join("skills");
    fs::create_dir_all(&data).map_err(|_| "create case data directory failed".to_string())?;
    fs::create_dir_all(&skills).map_err(|_| "create case skills directory failed".to_string())?;
    let hub = AgentHub::open_with_skills_root(Some(&data), Some(&skills))
        .map_err(|_| "open isolated AgentHub failed".to_string())?;
    setup_case(&hub, case)?;

    let db_before = critical_db_hash(&data)?;
    let sockets_before = socket_count()?;
    let adapterd_before = adapterd_process_count()?;
    let prepared = hub
        .adapter_bridge()
        .prepare_go_product_config()
        .map_err(|error| format!("preflight {} failed: {error}", case.name()))?;
    let summary_json = serde_json::to_string(prepared.summary())
        .map_err(|_| "serialize preflight summary failed".to_string())?;
    ensure(
        prepared.summary().reason == case.reason(),
        &format!(
            "{} returned {}, expected {}",
            case.name(),
            prepared.summary().reason.as_str(),
            case.reason().as_str()
        ),
    )?;
    ensure(
        !summary_json.contains(SOURCE_KEY),
        "preflight summary exposed a source secret",
    )?;
    let carries_config = prepared.into_config().is_some();
    ensure(
        carries_config == matches!(case, Case::Eligible),
        &format!("{} returned an unexpected config payload", case.name()),
    )?;
    ensure(
        critical_db_hash(&data)? == db_before,
        &format!("{} changed critical database bytes", case.name()),
    )?;
    ensure(
        socket_count()? == sockets_before,
        &format!("{} opened a socket", case.name()),
    )?;
    ensure(
        adapterd_process_count()? == adapterd_before,
        &format!("{} started an adapterd process", case.name()),
    )?;

    Ok(json!({
        "case": case.name(),
        "reason": case.reason().as_str(),
        "db_unchanged": true,
        "socket_count_unchanged": true,
        "adapterd_process_count_unchanged": true,
        "config_present": carries_config,
    }))
}

fn setup_case(hub: &AgentHub, case: Case) -> ProbeResult<()> {
    if matches!(case, Case::NoPool) {
        return Ok(());
    }
    hub.providers()
        .create(&ProviderInput {
            id: SOURCE_ID.into(),
            agent_id: AgentId::WorkBuddy,
            name: "Product preflight probe source".into(),
            settings_config: json!({
                "api_key": SOURCE_KEY,
                "base_url": "http://127.0.0.1:9/v1",
                "model": "probe-model",
            }),
            meta: json!({"preset": "deepseek-api"}),
            is_current: false,
        })
        .map_err(|_| "create probe provider failed".to_string())?;
    let pool = hub
        .route_pools()
        .ensure_default_pool(AgentId::Codex, RouteDownstreamSurface::Responses)
        .map_err(|_| "create probe pool failed".to_string())?;
    hub.route_pools()
        .add_member(&pool.id, AdapterSourceKind::Provider, SOURCE_ID)
        .map_err(|_| "add probe pool member failed".to_string())?;

    match case {
        Case::NoPool | Case::MissingPort => {}
        Case::ZeroPort => set_pool_port_raw(hub, &pool.id, 0)?,
        Case::MultiplePorts => {
            save_pool_port(hub, &pool.id, PORT)?;
            let second = hub
                .route_pools()
                .ensure_default_pool(AgentId::Claude, RouteDownstreamSurface::Messages)
                .map_err(|_| "create second probe pool failed".to_string())?;
            set_pool_port_raw(hub, &second.id, PORT + 1)?;
        }
        Case::UncoveredProfile => {
            save_pool_port(hub, &pool.id, PORT)?;
            insert_profile(hub, "uncovered-source", PORT)?;
        }
        Case::ProfilePortMismatch => {
            save_pool_port(hub, &pool.id, PORT)?;
            insert_profile(hub, SOURCE_ID, PORT + 1)?;
        }
        Case::ConfigIncompatible => {
            save_pool_port(hub, &pool.id, PORT)?;
            hub.db()
                .with_conn(|conn| {
                    conn.execute(
                        "UPDATE route_members SET enabled = 0 WHERE route_pool_id = ?1",
                        [&pool.id],
                    )?;
                    Ok(())
                })
                .map_err(|_| "disable probe member failed".to_string())?;
        }
        Case::Eligible => {
            save_pool_port(hub, &pool.id, PORT)?;
            insert_profile(hub, SOURCE_ID, PORT)?;
        }
    }
    Ok(())
}

fn save_pool_port(hub: &AgentHub, pool_id: &str, port: u16) -> ProbeResult<()> {
    hub.route_pools()
        .enroll_unified_gateway_as_default(pool_id, port)
        .map(|_| ())
        .map_err(|_| "save probe pool port failed".to_string())
}

fn set_pool_port_raw(hub: &AgentHub, pool_id: &str, port: u16) -> ProbeResult<()> {
    hub.db()
        .with_conn(|conn| {
            conn.execute(
                "UPDATE route_pools SET gateway_port = ?1 WHERE id = ?2",
                params![i64::from(port), pool_id],
            )?;
            Ok(())
        })
        .map_err(|_| "set raw probe pool port failed".to_string())
}

fn insert_profile(hub: &AgentHub, source_id: &str, local_port: u16) -> ProbeResult<()> {
    hub.db()
        .with_conn(|conn| {
            conn.execute(
                r#"INSERT INTO adapter_profiles (
                    id, name, source_kind, source_id, target_agent_id, route, mode,
                    status, rule_id, rule_version, generated_provider_id, local_port,
                    auto_start, last_error_code, created_at, updated_at
                ) VALUES (?1, 'probe profile', 'provider', ?2, 'codex', 'local_bridge', 'api',
                    'active', 'probe-rule', '1', NULL, ?3, 1, NULL,
                    '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"#,
                params![
                    format!("profile-{source_id}"),
                    source_id,
                    i64::from(local_port)
                ],
            )?;
            Ok(())
        })
        .map_err(|_| "insert probe profile failed".to_string())
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

fn socket_count() -> ProbeResult<usize> {
    let entries = fs::read_dir("/proc/self/fd").map_err(|_| "read process fd table failed")?;
    let mut count = 0;
    for entry in entries {
        let entry = entry.map_err(|_| "read process fd entry failed")?;
        if fs::read_link(entry.path())
            .ok()
            .is_some_and(|target| target.to_string_lossy().starts_with("socket:["))
        {
            count += 1;
        }
    }
    Ok(count)
}

fn adapterd_process_count() -> ProbeResult<usize> {
    let entries = fs::read_dir("/proc").map_err(|_| "read proc failed")?;
    let mut count = 0;
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        if fs::read_to_string(entry.path().join("comm"))
            .ok()
            .is_some_and(|name| name.trim() == "agenthub-adapterd")
        {
            count += 1;
        }
    }
    Ok(count)
}

fn validate_scratch(requested: &Path) -> ProbeResult<PathBuf> {
    fs::create_dir_all(requested).map_err(|_| "create scratch failed".to_string())?;
    let scratch = fs::canonicalize(requested).map_err(|_| "canonicalize scratch failed")?;
    let marked = scratch.file_name().is_some_and(|name| {
        name.to_string_lossy()
            .starts_with("agenthub-go-product-preflight.")
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
