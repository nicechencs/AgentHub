//! Manage Claude, Grok, and Pi plugin packs through their official CLIs.
//!
//! Snapshots the agent's settings/config file before the CLI runs. If the CLI
//! fails, the snapshot is restored so a half-written file is not left behind.
//! Vendor package/cache changes already made by the CLI are not rolled back.
//! AgentHub does not edit vendor plugin cache itself. Install without confirm
//! (Grok `--trust` / Claude `-y`) does not call the official command.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use crate::models::AgentId;
use crate::models::{RunSpec, RunStatus};
use crate::services::plugin_inventory::{
    parse_cli_available_plugin_list, preview_local_plugin, CliRun, PluginCliRunner, PluginEntry,
    SystemPluginCliRunner,
};
use crate::utils::paths::{agent_config_dir, agent_home, home_dir};
use crate::utils::process::{ProcessRunner, SystemProcessRunner};

const AVAILABLE_TIMEOUT: Duration = Duration::from_secs(20);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(120);
const UNINSTALL_TIMEOUT: Duration = Duration::from_secs(30);
const UPDATE_TIMEOUT: Duration = Duration::from_secs(180);
const UPDATE_OUTPUT_LIMIT: usize = 64 * 1024;
static CLAUDE_WRITE_LOCK: Mutex<()> = Mutex::new(());
static GROK_WRITE_LOCK: Mutex<()> = Mutex::new(());
static PI_WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Homes + binaries + CLI runner used by Claude/Grok operations (tests inject fakes).
pub struct PluginApplyContext<'a> {
    pub user_home: PathBuf,
    pub claude_home: PathBuf,
    pub grok_home: PathBuf,
    pub claude_bin: Option<PathBuf>,
    pub grok_bin: Option<PathBuf>,
    pub runner: &'a dyn PluginCliRunner,
}

/// Confirm-before-CLI install. `confirmed` maps to Grok `--trust` / Claude `-y`.
#[derive(Debug, Clone, Copy)]
pub struct PluginInstallOptions {
    pub confirmed: bool,
}

/// Uninstall options. Default `keep_data` is true (do not delete `plugins/data`).
#[derive(Debug, Clone, Copy)]
pub struct PluginUninstallOptions {
    pub keep_data: bool,
}

/// Confirm-before-CLI update. Updating a pack may execute vendor/package code.
#[derive(Debug, Clone, Copy)]
pub struct PluginUpdateOptions {
    pub confirmed: bool,
}

impl Default for PluginUninstallOptions {
    fn default() -> Self {
        Self { keep_data: true }
    }
}

struct FileSnapshot {
    path: PathBuf,
    contents: Option<Vec<u8>>,
}

/// Enable a listed Claude or Grok pack (`claude plugin enable` / `grok plugin enable`).
pub fn enable_plugin(agent: AgentId, name: &str, marketplace: Option<&str>) -> Result<(), String> {
    enable_plugin_with(&system_ctx(), agent, name, marketplace)
}

/// Disable a listed Claude or Grok pack (`claude plugin disable` / `grok plugin disable`).
pub fn disable_plugin(agent: AgentId, name: &str, marketplace: Option<&str>) -> Result<(), String> {
    disable_plugin_with(&system_ctx(), agent, name, marketplace)
}

pub fn enable_plugin_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
) -> Result<(), String> {
    set_plugin_enabled_with(ctx, agent, name, marketplace, true)
}

pub fn disable_plugin_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
) -> Result<(), String> {
    set_plugin_enabled_with(ctx, agent, name, marketplace, false)
}

/// Refresh the configured Claude or Grok marketplace catalogs.
pub fn refresh_plugin_marketplace(agent: AgentId) -> Result<(), String> {
    refresh_plugin_marketplace_with(&system_ctx(), agent)
}

pub fn refresh_plugin_marketplace_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
) -> Result<(), String> {
    let (bin, live) = update_agent_paths(ctx, agent)?;
    run_with_snapshot(
        agent,
        ctx.runner,
        bin,
        &["plugin", "marketplace", "update"],
        UPDATE_TIMEOUT,
        &live,
        "marketplace refresh",
    )
}

/// Update one installed Claude or Grok pack through its official CLI.
pub fn update_plugin(
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
    scope: Option<&str>,
    options: PluginUpdateOptions,
) -> Result<(), String> {
    update_plugin_with(&system_ctx(), agent, name, marketplace, scope, options)
}

pub fn update_plugin_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
    scope: Option<&str>,
    options: PluginUpdateOptions,
) -> Result<(), String> {
    if !options.confirmed {
        return Err("update needs confirmation".into());
    }
    if scope != Some("user") {
        return Err("only user-scope plugin packs can be updated here".into());
    }
    let spec = vendor_spec(agent, name, marketplace)?;
    let (bin, live) = update_agent_paths(ctx, agent)?;
    let args = update_args(agent, &spec)?;
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_with_snapshot(
        agent,
        ctx.runner,
        bin,
        &arg_refs,
        UPDATE_TIMEOUT,
        &live,
        "update",
    )
}

/// Update eligible Pi extensions. Pi skips exact npm semver pins itself.
pub fn update_pi_plugins(options: PluginUpdateOptions) -> Result<(), String> {
    if !options.confirmed {
        return Err("update needs confirmation".into());
    }
    let user_home = home_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let pi_config =
        agent_config_dir(AgentId::Pi).unwrap_or_else(|_| user_home.join(".pi").join("agent"));
    let bin = which::which("pi")
        .map_err(|_| "official Pi command not found; cannot update plugins".to_string())?;
    run_pi_update_with_snapshot(&bin, &pi_config.join("settings.json"))
}

fn system_ctx() -> PluginApplyContext<'static> {
    static RUNNER: SystemPluginCliRunner = SystemPluginCliRunner;
    let user_home = home_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let claude_home = agent_home(AgentId::Claude).unwrap_or_else(|_| user_home.join(".claude"));
    let grok_home = agent_home(AgentId::Grok).unwrap_or_else(|_| user_home.join(".grok"));
    PluginApplyContext {
        user_home,
        claude_home,
        grok_home,
        claude_bin: which::which("claude").ok(),
        grok_bin: which::which("grok").ok(),
        runner: &RUNNER,
    }
}

/// Marketplace rows from `plugin list --json --available` (not installed inventory).
pub fn list_available_plugins(agent: AgentId) -> Result<Vec<PluginEntry>, String> {
    list_available_plugins_with(&system_ctx(), agent)
}

pub fn list_available_plugins_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
) -> Result<Vec<PluginEntry>, String> {
    let bin = agent_bin(ctx, agent)?;
    let run = ctx.runner.run_plugin_with_timeout(
        bin,
        &["plugin", "list", "--json", "--available"],
        AVAILABLE_TIMEOUT,
    );
    if !run.success() {
        return Err(cli_error_detail(&run, "list"));
    }
    parse_cli_available_plugin_list(agent, &run.stdout, &ctx.user_home)
}

/// Preview components for a marketplace name, Grok git/local source, or typed spec.
pub fn preview_plugin_install(agent: AgentId, source: &str) -> Result<PluginEntry, String> {
    preview_plugin_install_with(&system_ctx(), agent, source)
}

pub fn preview_plugin_install_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
    source: &str,
) -> Result<PluginEntry, String> {
    let kind = classify_install_source(agent, source)?;
    match kind {
        InstallSource::Local(path) => preview_local_plugin(agent, &path, &ctx.user_home),
        InstallSource::Git(raw) => Ok(stub_preview(agent, &raw, None)),
        InstallSource::Marketplace { name, marketplace } => {
            let rows = list_available_plugins_with(ctx, agent)?;
            if let Some(row) = rows.into_iter().find(|row| {
                row.name == name
                    && marketplace_matches(row.marketplace.as_deref(), marketplace.as_deref())
            }) {
                return Ok(row);
            }
            Ok(stub_preview(agent, &name, marketplace.as_deref()))
        }
    }
}

/// Install after UI confirm. Without `confirmed`, the official command is not called.
pub fn install_plugin(
    agent: AgentId,
    source: &str,
    options: PluginInstallOptions,
) -> Result<(), String> {
    install_plugin_with(&system_ctx(), agent, source, options)
}

pub fn install_plugin_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
    source: &str,
    options: PluginInstallOptions,
) -> Result<(), String> {
    let kind = classify_install_source(agent, source)?;
    if !options.confirmed {
        return Err("installation needs confirmation".into());
    }
    let spec = kind.cli_source();
    let (bin, live) = agent_paths(ctx, agent)?;
    let _guard = plugin_write_guard(agent)?;
    let snapshot = snapshot_file(&live)?;
    let args = install_args(agent, &spec);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let run = ctx
        .runner
        .run_plugin_with_timeout(bin, &arg_refs, INSTALL_TIMEOUT);
    if run.success() {
        return Ok(());
    }
    let restore_err = restore_file(&snapshot).err();
    let detail = cli_error_detail(&run, "install");
    Err(cli_failure_with_restore(detail, restore_err))
}

/// Uninstall a listed Claude or Grok pack. Default keeps `plugins/data`.
pub fn uninstall_plugin(
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
    options: PluginUninstallOptions,
) -> Result<(), String> {
    uninstall_plugin_with(&system_ctx(), agent, name, marketplace, options)
}

pub fn uninstall_plugin_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
    options: PluginUninstallOptions,
) -> Result<(), String> {
    let spec = vendor_spec(agent, name, marketplace)?;
    let (bin, live) = agent_paths(ctx, agent)?;
    let _guard = plugin_write_guard(agent)?;
    let snapshot = snapshot_file(&live)?;
    let args = uninstall_args(agent, &spec, options.keep_data);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let run = ctx
        .runner
        .run_plugin_with_timeout(bin, &arg_refs, UNINSTALL_TIMEOUT);
    if run.success() {
        return Ok(());
    }
    let restore_err = restore_file(&snapshot).err();
    let detail = cli_error_detail(&run, "uninstall");
    Err(cli_failure_with_restore(detail, restore_err))
}

enum InstallSource {
    Marketplace {
        name: String,
        marketplace: Option<String>,
    },
    Git(String),
    Local(PathBuf),
}

impl InstallSource {
    fn cli_source(&self) -> String {
        match self {
            Self::Marketplace { name, marketplace } => {
                match (agent_spec_needs_marketplace(name), marketplace) {
                    (true, Some(market)) => format!("{name}@{market}"),
                    _ => name.clone(),
                }
            }
            Self::Git(raw) => raw.clone(),
            Self::Local(path) => path.to_string_lossy().into_owned(),
        }
    }
}

fn agent_spec_needs_marketplace(name: &str) -> bool {
    !name.contains('@')
}

fn classify_install_source(agent: AgentId, raw: &str) -> Result<InstallSource, String> {
    let source = raw.trim();
    if source.is_empty() {
        return Err("plugin source is required".into());
    }
    if source == "mcpServers"
        || source.contains('\0')
        || source.contains(['\n', '\r', ';', '|', '$', '`'])
    {
        return Err("invalid plugin source".into());
    }
    match agent {
        AgentId::Claude => {
            if looks_like_local_path(source) || looks_like_git_source(source) {
                return Err(
                    "Claude install accepts name@marketplace, not a git URL or local path".into(),
                );
            }
            if source.contains(['/', '\\']) {
                return Err("invalid plugin source".into());
            }
            let (name, marketplace) = split_install_name(source);
            if !valid_plugin_token(&name)
                || marketplace
                    .as_deref()
                    .is_some_and(|m| !valid_plugin_token(m))
            {
                return Err("invalid plugin source".into());
            }
            Ok(InstallSource::Marketplace { name, marketplace })
        }
        AgentId::Grok => {
            if looks_like_local_path(source) {
                let path = expand_local_path(source);
                if path_has_parent_dir(&path) {
                    return Err("invalid plugin source".into());
                }
                return Ok(InstallSource::Local(path));
            }
            if looks_like_git_source(source) {
                return Ok(InstallSource::Git(source.to_string()));
            }
            if source.contains(['/', '\\']) {
                return Err("invalid plugin source".into());
            }
            if !valid_plugin_token(source) {
                return Err("invalid plugin source".into());
            }
            Ok(InstallSource::Marketplace {
                name: source.to_string(),
                marketplace: None,
            })
        }
        _ => Err("install is only available for listed Claude and Grok plugin packs".into()),
    }
}

fn split_install_name(raw: &str) -> (String, Option<String>) {
    if let Some((name, market)) = raw.split_once('@') {
        if !name.is_empty() && !market.is_empty() {
            return (name.to_string(), Some(market.to_string()));
        }
    }
    (raw.to_string(), None)
}

fn valid_plugin_token(raw: &str) -> bool {
    let mut chars = raw.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn looks_like_local_path(source: &str) -> bool {
    source.starts_with('/')
        || source.starts_with('.')
        || source.starts_with('~')
        || source.contains('\\')
}

fn looks_like_git_source(source: &str) -> bool {
    source.starts_with("https://")
        || source.starts_with("http://")
        || source.starts_with("git@")
        || source.starts_with("ssh://")
        || {
            let parts: Vec<&str> = source.split('/').collect();
            parts.len() == 2
                && valid_plugin_token(parts[0])
                && valid_plugin_token(parts[1].split(['@', '#']).next().unwrap_or(""))
        }
}

fn expand_local_path(source: &str) -> PathBuf {
    if let Some(rest) = source.strip_prefix("~/") {
        if let Ok(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(source)
}

fn path_has_parent_dir(path: &Path) -> bool {
    path.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

fn marketplace_matches(row: Option<&str>, wanted: Option<&str>) -> bool {
    match wanted {
        None | Some("") => true,
        Some(market) => row == Some(market),
    }
}

fn stub_preview(agent: AgentId, name: &str, marketplace: Option<&str>) -> PluginEntry {
    PluginEntry {
        id: match marketplace {
            Some(m) => format!("{}:{}@{}", agent.as_str(), name, m),
            None => format!("{}:{}", agent.as_str(), name),
        },
        agent,
        name: name.to_string(),
        marketplace: marketplace.map(str::to_string),
        version: None,
        requested_version: None,
        scope: Some("user".into()),
        enabled: None,
        trusted: None,
        path: None,
        description: None,
        source: "available".into(),
        components: Vec::new(),
    }
}

fn install_args(agent: AgentId, spec: &str) -> Vec<String> {
    match agent {
        AgentId::Claude => vec![
            "plugin".into(),
            "install".into(),
            spec.into(),
            "-y".into(),
            "-s".into(),
            "user".into(),
        ],
        AgentId::Grok => vec![
            "plugin".into(),
            "install".into(),
            spec.into(),
            "--trust".into(),
        ],
        _ => vec!["plugin".into(), "install".into(), spec.into()],
    }
}

fn uninstall_args(agent: AgentId, spec: &str, keep_data: bool) -> Vec<String> {
    let mut args = vec!["plugin".into(), "uninstall".into(), spec.into()];
    match agent {
        AgentId::Grok => {
            args.push("--confirm".into());
            if keep_data {
                args.push("--keep-data".into());
            }
        }
        AgentId::Claude => {
            args.extend(["-s".into(), "user".into(), "-y".into()]);
            if keep_data {
                args.push("--keep-data".into());
            }
        }
        _ => {}
    }
    args
}

fn update_args(agent: AgentId, spec: &str) -> Result<Vec<String>, String> {
    match agent {
        AgentId::Claude => Ok(vec![
            "plugin".into(),
            "update".into(),
            spec.into(),
            "-y".into(),
            "-s".into(),
            "user".into(),
        ]),
        AgentId::Grok => Ok(vec!["plugin".into(), "update".into(), spec.into()]),
        _ => Err("individual update is only available for Claude and Grok plugin packs".into()),
    }
}

fn agent_bin<'a>(ctx: &'a PluginApplyContext<'_>, agent: AgentId) -> Result<&'a Path, String> {
    match agent {
        AgentId::Claude => ctx.claude_bin.as_deref().ok_or_else(|| {
            "official Claude command not found; cannot list or install plugins".to_string()
        }),
        AgentId::Grok => ctx.grok_bin.as_deref().ok_or_else(|| {
            "official Grok command not found; cannot list or install plugins".to_string()
        }),
        _ => Err("install is only available for listed Claude and Grok plugin packs".into()),
    }
}

fn set_plugin_enabled_with(
    ctx: &PluginApplyContext<'_>,
    agent: AgentId,
    name: &str,
    marketplace: Option<&str>,
    enabled: bool,
) -> Result<(), String> {
    let spec = vendor_spec(agent, name, marketplace)?;
    let (bin, live) = agent_paths(ctx, agent)?;
    let _guard = plugin_write_guard(agent)?;
    let snapshot = snapshot_file(&live)?;
    let action = if enabled { "enable" } else { "disable" };
    let run = ctx
        .runner
        .run_plugin(bin, &["plugin", action, spec.as_str()]);
    if run.success() {
        return Ok(());
    }
    let restore_err = restore_file(&snapshot).err();
    let detail = cli_error_detail(&run, action);
    Err(cli_failure_with_restore(detail, restore_err))
}

fn agent_paths<'a>(
    ctx: &'a PluginApplyContext<'_>,
    agent: AgentId,
) -> Result<(&'a Path, PathBuf), String> {
    match agent {
        AgentId::Claude => {
            let bin = ctx.claude_bin.as_deref().ok_or_else(|| {
                "official Claude command not found; cannot enable or disable plugins".to_string()
            })?;
            Ok((bin, ctx.claude_home.join("settings.json")))
        }
        AgentId::Grok => {
            let bin = ctx.grok_bin.as_deref().ok_or_else(|| {
                "official Grok command not found; cannot enable or disable plugins".to_string()
            })?;
            Ok((bin, ctx.grok_home.join("config.toml")))
        }
        _ => Err("enable/disable is only available for listed Claude and Grok plugin packs".into()),
    }
}

fn update_agent_paths<'a>(
    ctx: &'a PluginApplyContext<'_>,
    agent: AgentId,
) -> Result<(&'a Path, PathBuf), String> {
    match agent {
        AgentId::Claude => ctx
            .claude_bin
            .as_deref()
            .map(|bin| (bin, ctx.claude_home.join("settings.json")))
            .ok_or_else(|| "official Claude command not found; cannot update plugins".into()),
        AgentId::Grok => ctx
            .grok_bin
            .as_deref()
            .map(|bin| (bin, ctx.grok_home.join("config.toml")))
            .ok_or_else(|| "official Grok command not found; cannot update plugins".into()),
        _ => Err("marketplace refresh and individual update are only available for Claude and Grok plugin packs".into()),
    }
}

fn vendor_spec(agent: AgentId, name: &str, marketplace: Option<&str>) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("plugin name is required".into());
    }
    if name == "mcpServers" || name.starts_with('-') || name.chars().any(char::is_control) {
        return Err("invalid plugin name".into());
    }
    match agent {
        AgentId::Claude => {
            let mut parts = name.split('@');
            let plugin = parts.next().unwrap_or_default();
            let embedded_market = parts.next();
            if parts.next().is_some()
                || !valid_plugin_token(plugin)
                || embedded_market.is_some_and(|market| !valid_plugin_token(market))
            {
                return Err("invalid plugin name".into());
            }
            let market =
                embedded_market.or_else(|| marketplace.map(str::trim).filter(|s| !s.is_empty()));
            if market.is_some_and(|value| !valid_plugin_token(value)) {
                return Err("invalid plugin marketplace".into());
            }
            Ok(match market {
                Some(value) => format!("{plugin}@{value}"),
                None => plugin.to_string(),
            })
        }
        AgentId::Grok if valid_plugin_token(name) => Ok(name.to_string()),
        AgentId::Grok => Err("invalid plugin name".into()),
        _ => Err("enable/disable is only available for listed Claude and Grok plugin packs".into()),
    }
}

fn snapshot_file(path: &Path) -> Result<FileSnapshot, String> {
    if !path.exists() {
        return Ok(FileSnapshot {
            path: path.to_path_buf(),
            contents: None,
        });
    }
    let contents = fs::read(path).map_err(|e| format!("backup {}: {e}", path.display()))?;
    Ok(FileSnapshot {
        path: path.to_path_buf(),
        contents: Some(contents),
    })
}

fn restore_file(snapshot: &FileSnapshot) -> Result<(), String> {
    match &snapshot.contents {
        Some(bytes) => {
            if let Some(parent) = snapshot.path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("restore {}: {e}", snapshot.path.display()))?;
            }
            fs::write(&snapshot.path, bytes)
                .map_err(|e| format!("restore {}: {e}", snapshot.path.display()))
        }
        None => {
            if snapshot.path.exists() {
                fs::remove_file(&snapshot.path)
                    .map_err(|e| format!("restore {}: {e}", snapshot.path.display()))?;
            }
            Ok(())
        }
    }
}

fn run_with_snapshot(
    agent: AgentId,
    runner: &dyn PluginCliRunner,
    bin: &Path,
    args: &[&str],
    timeout: Duration,
    live: &Path,
    action: &str,
) -> Result<(), String> {
    let _guard = plugin_write_guard(agent)?;
    let snapshot = snapshot_file(live)?;
    let run = runner.run_plugin_with_timeout(bin, args, timeout);
    if run.success() {
        return Ok(());
    }
    let restore_err = restore_file(&snapshot).err();
    let detail = cli_error_detail(&run, action);
    Err(cli_failure_with_restore(detail, restore_err))
}

fn run_pi_update_with_snapshot(bin: &Path, live: &Path) -> Result<(), String> {
    let neutral_cwd = tempfile::Builder::new()
        .prefix("agenthub-pi-update-")
        .tempdir()
        .map_err(|_| "cannot create an isolated Pi update directory".to_string())?;
    let _guard = plugin_write_guard(AgentId::Pi)?;
    let snapshot = snapshot_file(live)?;
    let result = SystemProcessRunner.run(
        &RunSpec {
            agent: AgentId::Pi,
            program: bin.to_path_buf(),
            args: vec![
                "update".into(),
                "--extensions".into(),
                "--no-approve".into(),
            ],
            cwd: Some(neutral_cwd.path().to_path_buf()),
            env: Vec::new(),
        },
        UPDATE_TIMEOUT,
        UPDATE_OUTPUT_LIMIT,
    );
    if result.status == RunStatus::Ok && result.exit_code == Some(0) {
        return Ok(());
    }
    let restore_err = restore_file(&snapshot).err();
    let detail = match result.status {
        RunStatus::Timeout => "plugin update timed out".to_string(),
        _ => "plugin update failed".to_string(),
    };
    Err(cli_failure_with_restore(detail, restore_err))
}

fn plugin_write_guard(agent: AgentId) -> Result<MutexGuard<'static, ()>, String> {
    let lock = match agent {
        AgentId::Claude => &CLAUDE_WRITE_LOCK,
        AgentId::Grok => &GROK_WRITE_LOCK,
        AgentId::Pi => &PI_WRITE_LOCK,
        _ => return Err("plugin writes are not available for this agent".into()),
    };
    lock.lock()
        .map_err(|_| "plugin write lock is unavailable".to_string())
}

fn cli_error_detail(run: &CliRun, action: &str) -> String {
    if run.unavailable() {
        return "official plugin command is unavailable".into();
    }
    if run.timed_out {
        return format!("plugin {action} timed out");
    }
    format!("plugin {action} failed")
}

fn cli_failure_with_restore(detail: String, restore_error: Option<String>) -> String {
    if restore_error.is_some() {
        format!("{detail}; plugin configuration restore failed")
    } else {
        detail
    }
}

#[cfg(test)]
#[path = "plugin_apply/tests.rs"]
mod tests;
