//! Isolated real-file probe for the core ticket bind saga.
//!
//! This is an executable probe, not a Rust test. The caller must provide a
//! scratch tree and explicit environment paths; every live write is confined
//! below that tree.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use agenthub_core::error::{AppError, Result as CoreResult};
use agenthub_core::models::{
    AdapterProfileStatus, AdapterRoute, AgentId, ProviderInput, TicketPlanRequest,
    TicketUnbindRequest,
};
use agenthub_core::AgentHub;
use serde_json::{json, Value};

const SOURCE_ID: &str = "probe-kimi-membership";
const PREVIOUS_ID: &str = "probe-previous-claude";
const SOURCE_KEY: &str = "sk-agenthub-probe-kimi-source-do-not-use-000000";
const OAUTH_ACCESS: &str = "oauth-agenthub-probe-access-do-not-use-000000";
const OAUTH_REFRESH: &str = "oauth-agenthub-probe-refresh-do-not-use-000000";

type ProbeResult<T> = Result<T, String>;

#[derive(Clone, Copy)]
enum Scenario {
    SuccessUnbind,
    FinalizeFailure,
}

struct ProbePaths {
    data: PathBuf,
    skills: PathBuf,
    claude: PathBuf,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("ticket bind saga probe failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> ProbeResult<()> {
    let mut args = std::env::args_os().skip(1);
    let scenario = match args.next().as_deref() {
        Some(value) if value == OsStr::new("success-unbind") => Scenario::SuccessUnbind,
        Some(value) if value == OsStr::new("finalize-failure") => Scenario::FinalizeFailure,
        _ => {
            return Err(
                "usage: ticket_bind_saga_probe <success-unbind|finalize-failure> <scratch>".into(),
            )
        }
    };
    let scratch = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "scratch path is required".to_string())?;
    if args.next().is_some() {
        return Err("unexpected extra arguments".into());
    }

    let paths = validate_scratch(&scratch)?;
    let (settings_before, credentials_before) = seed_live_files(&paths.claude)?;
    let hub = core(
        AgentHub::open_with_skills_root(Some(&paths.data), Some(&paths.skills)),
        "open isolated AgentHub",
    )?;
    create_source(&hub)?;
    let request = plan_request();
    let plan = core(hub.tickets().plan(&request), "plan ticket")?;
    ensure(plan.can_apply, "ticket plan was not writable")?;
    ensure(
        plan.analysis.route == AdapterRoute::NativeEndpoint,
        "ticket plan did not select native_endpoint",
    )?;

    let evidence = match scenario {
        Scenario::SuccessUnbind => run_success_unbind(
            &hub,
            &paths,
            &request,
            &settings_before,
            &credentials_before,
        )?,
        Scenario::FinalizeFailure => run_finalize_failure(
            &hub,
            &paths,
            &request,
            &settings_before,
            &credentials_before,
        )?,
    };
    println!(
        "{}",
        serde_json::to_string(&evidence).map_err(|_| "serialize evidence failed".to_string())?
    );
    Ok(())
}

fn validate_scratch(requested: &Path) -> ProbeResult<ProbePaths> {
    fs::create_dir_all(requested).map_err(|_| "create scratch failed".to_string())?;
    let scratch =
        fs::canonicalize(requested).map_err(|_| "canonicalize scratch failed".to_string())?;
    ensure(scratch.is_absolute(), "scratch must be absolute")?;
    ensure(
        is_safe_temp_tree(&scratch),
        "scratch is outside the probe temp tree",
    )?;

    let real_home = required_canonical_env("AGENTHUB_PROBE_REAL_HOME")?;
    ensure(
        !scratch.starts_with(&real_home),
        "scratch must not be inside the real user home",
    )?;

    let data = create_and_canonicalize(&scratch.join("data"), "AgentHub data")?;
    let skills = create_and_canonicalize(&scratch.join("skills"), "skills")?;
    let claude = create_and_canonicalize(&scratch.join("claude"), "Claude config")?;
    let home = create_and_canonicalize(&scratch.join("home"), "HOME")?;
    let xdg_config = create_and_canonicalize(&scratch.join("xdg-config"), "XDG config")?;
    let xdg_data = create_and_canonicalize(&scratch.join("xdg-data"), "XDG data")?;

    require_env_path("AGENTHUB_HOME", &data)?;
    require_env_path("CLAUDE_CONFIG_DIR", &claude)?;
    require_env_path("HOME", &home)?;
    require_env_path("XDG_CONFIG_HOME", &xdg_config)?;
    require_env_path("XDG_DATA_HOME", &xdg_data)?;
    for path in [&data, &skills, &claude, &home, &xdg_config, &xdg_data] {
        ensure(path.starts_with(&scratch), "isolated path escaped scratch")?;
        ensure(
            !path.starts_with(&real_home),
            "isolated path overlaps the real user home",
        )?;
    }

    Ok(ProbePaths {
        data,
        skills,
        claude,
    })
}

fn is_safe_temp_tree(path: &Path) -> bool {
    let marked = path
        .components()
        .any(|part| part.as_os_str() == OsStr::new("agenthub-ticket-bind-saga"));
    if !marked {
        return false;
    }
    [Path::new("/tmp"), Path::new("/var/tmp")]
        .into_iter()
        .filter_map(|root| fs::canonicalize(root).ok())
        .any(|root| path != root && path.starts_with(root))
}

fn required_canonical_env(name: &str) -> ProbeResult<PathBuf> {
    let value = std::env::var_os(name).ok_or_else(|| format!("{name} is required"))?;
    fs::canonicalize(value).map_err(|_| format!("canonicalize {name} failed"))
}

fn require_env_path(name: &str, expected: &Path) -> ProbeResult<()> {
    let actual = required_canonical_env(name)?;
    ensure(
        actual == expected,
        &format!("{name} must point inside scratch"),
    )
}

fn create_and_canonicalize(path: &Path, label: &str) -> ProbeResult<PathBuf> {
    fs::create_dir_all(path).map_err(|_| format!("create {label} directory failed"))?;
    fs::canonicalize(path).map_err(|_| format!("canonicalize {label} directory failed"))
}

fn seed_live_files(claude: &Path) -> ProbeResult<(Vec<u8>, Vec<u8>)> {
    let settings = json!({
        "env": {"AGENTHUB_PROBE_ORIGINAL": "preserve-me"},
        "model": "probe-original-model",
        "permissions": {"allow": ["Read"]}
    });
    let credentials = json!({
        "claudeAiOauth": {
            "accessToken": OAUTH_ACCESS,
            "refreshToken": OAUTH_REFRESH,
            "expiresAt": 4102444800000_u64
        }
    });
    let settings = pretty_json_bytes(&settings)?;
    let credentials = pretty_json_bytes(&credentials)?;
    fs::write(claude.join("settings.json"), &settings)
        .map_err(|_| "write isolated settings failed".to_string())?;
    fs::write(claude.join(".credentials.json"), &credentials)
        .map_err(|_| "write isolated credentials failed".to_string())?;
    Ok((settings, credentials))
}

fn pretty_json_bytes(value: &Value) -> ProbeResult<Vec<u8>> {
    let mut bytes =
        serde_json::to_vec_pretty(value).map_err(|_| "encode fixture JSON failed".to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn create_source(hub: &AgentHub) -> ProbeResult<()> {
    core(
        hub.providers().create(&ProviderInput {
            id: SOURCE_ID.into(),
            agent_id: AgentId::Kimi,
            name: "Probe Kimi membership".into(),
            settings_config: json!({"apiKey": SOURCE_KEY}),
            meta: json!({"preset": "kimi-code-membership"}),
            is_current: false,
        }),
        "create synthetic source",
    )?;
    Ok(())
}

fn create_previous_current(hub: &AgentHub, settings: &Value) -> ProbeResult<()> {
    core(
        hub.providers().create(&ProviderInput {
            id: PREVIOUS_ID.into(),
            agent_id: AgentId::Claude,
            name: "Probe previous Claude".into(),
            settings_config: settings.clone(),
            meta: json!({"source": "probe"}),
            is_current: true,
        }),
        "create previous current provider",
    )?;
    Ok(())
}

fn plan_request() -> TicketPlanRequest {
    TicketPlanRequest {
        ticket_id: format!("provider:{SOURCE_ID}"),
        target_agent_id: AgentId::Claude,
    }
}

fn run_success_unbind(
    hub: &AgentHub,
    paths: &ProbePaths,
    request: &TicketPlanRequest,
    settings_before: &[u8],
    credentials_before: &[u8],
) -> ProbeResult<Value> {
    ensure(
        core(
            hub.providers().get_current(AgentId::Claude),
            "read initial current provider",
        )?
        .is_none(),
        "success scenario must begin without a current provider",
    )?;

    let binding = core(hub.ticket_bind().bind(request), "bind ticket")?;
    let settings_after = read_file(&paths.claude.join("settings.json"), "bound settings")?;
    ensure(
        settings_after != settings_before,
        "bind did not change the live settings",
    )?;
    ensure(
        contains_bytes(&settings_after, SOURCE_KEY.as_bytes()),
        "bind did not materialize the synthetic source",
    )?;
    ensure(
        !paths.claude.join(".credentials.json").exists(),
        "bind did not replace the isolated credentials file",
    )?;
    ensure(binding.active, "bound ticket was not active")?;
    ensure(
        core(
            hub.providers().get_current(AgentId::Claude),
            "read bound current provider",
        )?
        .is_some(),
        "bind did not select a current provider",
    )?;

    core(
        hub.ticket_bind().unbind(&TicketUnbindRequest {
            ticket_id: request.ticket_id.clone(),
            agent_id: AgentId::Claude,
        }),
        "unbind ticket",
    )?;
    assert_live_restored(paths, settings_before, credentials_before)?;
    ensure(
        core(
            hub.providers().get_current(AgentId::Claude),
            "read restored current provider",
        )?
        .is_none(),
        "unbind did not restore the empty current state",
    )?;
    ensure(
        core(
            hub.providers().list(Some(AgentId::Claude)),
            "list providers after unbind",
        )?
        .is_empty(),
        "unbind left a Claude provider projection",
    )?;
    ensure(
        core(
            hub.adapter_apply().list(None, None, Some(AgentId::Claude)),
            "list profiles after unbind",
        )?
        .is_empty(),
        "unbind left an adapter profile",
    )?;

    Ok(json!({
        "schema": "ticket-bind-saga-probe.v1",
        "scenario": "success-unbind",
        "status": "ok",
        "plan_route": "native_endpoint",
        "live_write_observed": true,
        "settings_restored_exact": true,
        "credentials_restored_exact": true,
        "current_restored": "none",
        "claude_provider_count": 0,
        "coverage": "core TicketReadService/TicketBindService plus real Claude writer; no Tauri host"
    }))
}

fn run_finalize_failure(
    hub: &AgentHub,
    paths: &ProbePaths,
    request: &TicketPlanRequest,
    settings_before: &[u8],
    credentials_before: &[u8],
) -> ProbeResult<Value> {
    let original_settings: Value = serde_json::from_slice(settings_before)
        .map_err(|_| "decode original settings failed".to_string())?;
    create_previous_current(hub, &original_settings)?;
    install_finalize_failure_trigger(hub)?;

    let error = match hub.ticket_bind().bind(request) {
        Err(error) => error,
        Ok(_) => return Err("finalize trigger unexpectedly allowed bind".into()),
    };
    ensure(
        error.code() == "adapter.profile_finalize",
        "bind failed at an unexpected stage",
    )?;
    assert_live_restored(paths, settings_before, credentials_before)?;

    let current = core(
        hub.providers().get_current(AgentId::Claude),
        "read compensated current provider",
    )?
    .ok_or_else(|| "compensation lost the previous current provider".to_string())?;
    ensure(
        current.id == PREVIOUS_ID && current.is_current,
        "compensation did not retain the previous current provider",
    )?;
    let providers = core(
        hub.providers().list(Some(AgentId::Claude)),
        "list compensated providers",
    )?;
    ensure(
        providers.len() == 1 && providers[0].id == PREVIOUS_ID && providers[0].is_current,
        "compensation did not remove the generated provider projection",
    )?;
    let profiles = core(
        hub.adapter_apply().list(None, None, Some(AgentId::Claude)),
        "list compensated profiles",
    )?;
    ensure(
        profiles.len() == 1
            && profiles[0].status == AdapterProfileStatus::NeedsAttention
            && profiles[0].last_error_code.as_deref() == Some("adapter.profile_finalize"),
        "failed bind did not leave the expected profile state",
    )?;

    Ok(json!({
        "schema": "ticket-bind-saga-probe.v1",
        "scenario": "finalize-failure",
        "status": "ok",
        "failure_code": "adapter.profile_finalize",
        "settings_restored_exact": true,
        "credentials_restored_exact": true,
        "previous_current_retained": true,
        "generated_current_count": 0,
        "generated_provider_absent": true,
        "coverage": "core TicketReadService/TicketBindService plus real Claude writer; no Tauri host"
    }))
}

fn install_finalize_failure_trigger(hub: &AgentHub) -> ProbeResult<()> {
    core(
        hub.db().with_conn(|connection| {
            connection.execute_batch(
                r#"
                CREATE TRIGGER fail_ticket_probe_profile_finalize
                BEFORE UPDATE OF status ON adapter_profiles
                WHEN NEW.status = 'active'
                BEGIN
                    SELECT RAISE(ABORT, 'injected ticket saga probe finalization failure');
                END;
                "#,
            )?;
            Ok(())
        }),
        "install finalize failure trigger",
    )
}

fn assert_live_restored(
    paths: &ProbePaths,
    settings_before: &[u8],
    credentials_before: &[u8],
) -> ProbeResult<()> {
    ensure(
        read_file(&paths.claude.join("settings.json"), "restored settings")? == settings_before,
        "settings were not restored byte-for-byte",
    )?;
    ensure(
        read_file(
            &paths.claude.join(".credentials.json"),
            "restored credentials",
        )? == credentials_before,
        "credentials were not restored byte-for-byte",
    )
}

fn read_file(path: &Path, label: &str) -> ProbeResult<Vec<u8>> {
    fs::read(path).map_err(|_| format!("read {label} failed"))
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn core<T>(result: CoreResult<T>, operation: &str) -> ProbeResult<T> {
    result.map_err(|error| safe_core_error(operation, &error))
}

fn safe_core_error(operation: &str, error: &AppError) -> String {
    format!("{operation} failed [{}]", error.code())
}

fn ensure(condition: bool, message: &str) -> ProbeResult<()> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}
