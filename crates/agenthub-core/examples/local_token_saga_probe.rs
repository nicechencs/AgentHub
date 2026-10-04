//! Isolated real-store probe for RoutePool local entry-key transactions.
//!
//! This executable is intentionally not a Rust test. The caller supplies a
//! disposable tree, and the emitted evidence contains no entry-key material.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use agenthub_core::error::{AppError, Result as CoreResult};
use agenthub_core::models::{AgentId, ProviderInput, RouteDownstreamSurface};
use agenthub_core::services::adapter_projection::projected_local_bearer;
use agenthub_core::AgentHub;
use serde_json::json;

const GENERATED_PROVIDER_ID: &str = "local-token-probe-generated";
const FOREIGN_PROVIDER_ID: &str = "local-token-probe-foreign-generated";
const STALE_KEY: &str = "ahb_probe_stale_local_key_do_not_use";
const NEXT_KEY: &str = "ahb_probe_next_local_key_do_not_use";
const FOREIGN_ALIAS_KEY: &str = "ahb_probe_foreign_alias_do_not_use";

type ProbeResult<T> = Result<T, String>;

fn main() {
    if let Err(error) = run() {
        eprintln!("local token saga probe failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> ProbeResult<()> {
    let scratch = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| "usage: local_token_saga_probe <scratch>".to_string())?;
    if std::env::args_os().nth(2).is_some() {
        return Err("unexpected extra arguments".into());
    }
    let (data, skills) = validate_scratch(&scratch)?;
    let hub = core(
        AgentHub::open_with_skills_root(Some(&data), Some(&skills)),
        "open isolated AgentHub",
    )?;

    let pool = core(
        hub.route_pools()
            .ensure_default_pool(AgentId::Codex, RouteDownstreamSurface::Responses),
        "create default route pool",
    )?;
    core(
        hub.providers().create(&ProviderInput {
            id: GENERATED_PROVIDER_ID.into(),
            agent_id: AgentId::Codex,
            name: "Local token probe generated provider".into(),
            settings_config: json!({
                "format": "toml",
                "auth": {"OPENAI_API_KEY": STALE_KEY},
            }),
            meta: json!({
                "generatedBy": "adapter",
                "adapterProfileId": pool.id,
            }),
            is_current: false,
        }),
        "create historical generated provider",
    )?;

    let accepted_before = core(
        hub.route_pools().list_accepted_local_bearers(),
        "list accepted bearers before convergence",
    )?;
    ensure(
        accepted_before
            .iter()
            .any(|(token, pool_id)| token == STALE_KEY && pool_id == &pool.id),
        "historical projected key was not accepted before convergence",
    )?;

    let primary = core(
        hub.route_pools().set_local_token(&pool.id, NEXT_KEY),
        "rotate primary key",
    )?;
    ensure(
        primary.primary,
        "primary rotation returned a non-primary row",
    )?;
    let generated = core(
        hub.providers().get_by_id(GENERATED_PROVIDER_ID),
        "read converged generated provider",
    )?
    .ok_or_else(|| "generated provider disappeared".to_string())?;
    ensure(
        projected_local_bearer(&generated.settings_config).as_deref() == Some(NEXT_KEY),
        "generated provider did not converge to the new primary key",
    )?;
    let accepted_after = core(
        hub.route_pools().list_accepted_local_bearers(),
        "list accepted bearers after convergence",
    )?;
    ensure(
        accepted_after
            .iter()
            .all(|(token, _pool_id)| token != STALE_KEY),
        "historical projected key remained accepted",
    )?;
    ensure(
        accepted_after
            .iter()
            .any(|(token, pool_id)| token == NEXT_KEY && pool_id == &pool.id),
        "new primary key was not accepted",
    )?;

    let extra = core(
        hub.route_pools()
            .create_local_token(&pool.id, "Probe extra key"),
        "create extra key",
    )?;
    ensure(!extra.primary, "extra-key creation returned a primary row")?;
    let foreign_pool = core(
        hub.route_pools()
            .ensure_default_pool(AgentId::Claude, RouteDownstreamSurface::Messages),
        "create foreign route pool",
    )?;
    core(
        hub.providers().create(&ProviderInput {
            id: FOREIGN_PROVIDER_ID.into(),
            agent_id: AgentId::Claude,
            name: "Foreign local token probe provider".into(),
            settings_config: json!({
                "format": "json",
                "auth": {"ANTHROPIC_AUTH_TOKEN": FOREIGN_ALIAS_KEY},
            }),
            meta: json!({
                "generatedBy": "adapter",
                "adapterProfileId": foreign_pool.id,
            }),
            is_current: false,
        }),
        "create foreign historical generated provider",
    )?;
    let accepted_with_extra = core(
        hub.route_pools().list_accepted_local_bearers(),
        "list accepted bearers after extra creation",
    )?;
    ensure(
        accepted_with_extra
            .iter()
            .any(|(token, pool_id)| token == &extra.token && pool_id == &pool.id),
        "created extra key was not accepted",
    )?;
    ensure(
        accepted_with_extra
            .iter()
            .all(|(token, _pool_id)| token != STALE_KEY),
        "historical projected key returned after extra creation",
    )?;
    let tokens_before_conflicts = core(
        hub.route_pools().list_local_tokens(),
        "snapshot keys before duplicate writes",
    )?;
    let pool_before_conflicts = core(hub.route_pools().get(&pool.id), "snapshot route pool")?
        .ok_or_else(|| "route pool disappeared".to_string())?;
    let provider_before_conflicts = core(
        hub.providers().get_by_id(GENERATED_PROVIDER_ID),
        "snapshot generated provider",
    )?
    .ok_or_else(|| "generated provider disappeared".to_string())?;

    let primary_duplicate = expected_error(
        hub.route_pools().set_local_token(&pool.id, &extra.token),
        "extra key reused as primary unexpectedly succeeded",
    )?;
    ensure(
        primary_duplicate.code() == "invalid_arg",
        "primary duplicate returned an unexpected error code",
    )?;
    assert_unchanged(
        &hub,
        &pool.id,
        &tokens_before_conflicts,
        &pool_before_conflicts,
        &provider_before_conflicts,
    )?;

    let foreign_alias_duplicate = expected_error(
        hub.route_pools()
            .set_local_token(&pool.id, FOREIGN_ALIAS_KEY),
        "foreign projected alias reused as primary unexpectedly succeeded",
    )?;
    ensure(
        foreign_alias_duplicate.code() == "invalid_arg",
        "foreign projected alias returned an unexpected error code",
    )?;
    assert_unchanged(
        &hub,
        &pool.id,
        &tokens_before_conflicts,
        &pool_before_conflicts,
        &provider_before_conflicts,
    )?;

    let extra_duplicate = expected_error(
        hub.route_pools().set_local_token(&extra.id, NEXT_KEY),
        "primary key reused as extra unexpectedly succeeded",
    )?;
    ensure(
        extra_duplicate.code() == "invalid_arg",
        "extra duplicate returned an unexpected error code",
    )?;
    assert_unchanged(
        &hub,
        &pool.id,
        &tokens_before_conflicts,
        &pool_before_conflicts,
        &provider_before_conflicts,
    )?;

    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema": "local-token-saga-probe.v1",
            "status": "ok",
            "provider_converged": true,
            "historical_alias_removed": true,
            "extra_created": true,
            "listed_key_count": tokens_before_conflicts.len(),
            "accepted_bearer_count": accepted_with_extra.len(),
            "primary_duplicate_error_code": primary_duplicate.code(),
            "primary_duplicate_rolled_back": true,
            "extra_duplicate_error_code": extra_duplicate.code(),
            "extra_duplicate_rolled_back": true,
            "foreign_alias_duplicate_error_code": foreign_alias_duplicate.code(),
            "foreign_alias_duplicate_rolled_back": true,
        }))
        .map_err(|_| "serialize evidence failed".to_string())?
    );
    Ok(())
}

fn assert_unchanged(
    hub: &AgentHub,
    pool_id: &str,
    expected_tokens: &[agenthub_core::models::LocalTokenRecord],
    expected_pool: &agenthub_core::models::RoutePool,
    expected_provider: &agenthub_core::models::Provider,
) -> ProbeResult<()> {
    let tokens = core(
        hub.route_pools().list_local_tokens(),
        "read keys after rejected duplicate",
    )?;
    let pool = core(
        hub.route_pools().get(pool_id),
        "read route pool after rejected duplicate",
    )?
    .ok_or_else(|| "route pool disappeared after rejected duplicate".to_string())?;
    let provider = core(
        hub.providers().get_by_id(GENERATED_PROVIDER_ID),
        "read provider after rejected duplicate",
    )?
    .ok_or_else(|| "provider disappeared after rejected duplicate".to_string())?;
    ensure(
        tokens == expected_tokens,
        "rejected duplicate changed key rows",
    )?;
    ensure(
        &pool == expected_pool,
        "rejected duplicate changed the route pool",
    )?;
    ensure(
        &provider == expected_provider,
        "rejected duplicate changed the generated provider",
    )
}

fn validate_scratch(requested: &Path) -> ProbeResult<(PathBuf, PathBuf)> {
    fs::create_dir_all(requested).map_err(|_| "create scratch failed".to_string())?;
    let scratch = fs::canonicalize(requested).map_err(|_| "canonicalize scratch failed")?;
    ensure(
        is_safe_temp_tree(&scratch),
        "scratch is outside the probe temp tree",
    )?;
    let real_home = canonical_env("AGENTHUB_PROBE_REAL_HOME")?;
    ensure(
        !scratch.starts_with(&real_home),
        "scratch overlaps the real user home",
    )?;
    let data = create_dir(&scratch.join("data"))?;
    let skills = create_dir(&scratch.join("skills"))?;
    require_env_path("AGENTHUB_HOME", &data)?;
    for name in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "CODEX_HOME"] {
        let path = canonical_env(name)?;
        ensure(
            path.starts_with(&scratch),
            &format!("{name} escaped scratch"),
        )?;
        ensure(
            !path.starts_with(&real_home),
            &format!("{name} overlaps the real user home"),
        )?;
    }
    Ok((data, skills))
}

fn is_safe_temp_tree(path: &Path) -> bool {
    let marked = path
        .components()
        .any(|part| part.as_os_str() == OsStr::new("agenthub-local-token-saga"));
    marked
        && [Path::new("/tmp"), Path::new("/var/tmp")]
            .into_iter()
            .filter_map(|root| fs::canonicalize(root).ok())
            .any(|root| path != root && path.starts_with(root))
}

fn canonical_env(name: &str) -> ProbeResult<PathBuf> {
    let value = std::env::var_os(name).ok_or_else(|| format!("{name} is required"))?;
    fs::canonicalize(value).map_err(|_| format!("canonicalize {name} failed"))
}

fn create_dir(path: &Path) -> ProbeResult<PathBuf> {
    fs::create_dir_all(path).map_err(|_| "create isolated directory failed".to_string())?;
    fs::canonicalize(path).map_err(|_| "canonicalize isolated directory failed".to_string())
}

fn require_env_path(name: &str, expected: &Path) -> ProbeResult<()> {
    ensure(
        canonical_env(name)? == expected,
        &format!("{name} must point inside scratch"),
    )
}

fn expected_error<T>(result: CoreResult<T>, success_message: &str) -> ProbeResult<AppError> {
    match result {
        Err(error) => Ok(error),
        Ok(_) => Err(success_message.to_string()),
    }
}

fn core<T>(result: CoreResult<T>, label: &str) -> ProbeResult<T> {
    result.map_err(|error| format!("{label} failed [{}]", error.code()))
}

fn ensure(condition: bool, message: &str) -> ProbeResult<()> {
    if condition {
        Ok(())
    } else {
        Err(message.to_string())
    }
}
