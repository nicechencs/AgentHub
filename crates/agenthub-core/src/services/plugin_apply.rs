//! Enable / disable / install / uninstall Claude and Grok plugin packs via official CLI.
//!
//! Snapshots the agent's settings/config file before the CLI runs. If the CLI
//! fails, the snapshot is restored so a half-written file is not left behind.
//! AgentHub does not edit vendor plugin cache itself. Install without confirm
//! (Grok `--trust` / Claude `-y`) does not call the official command.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::models::AgentId;
use crate::services::plugin_inventory::{
    parse_cli_available_plugin_list, preview_local_plugin, CliRun, PluginCliRunner, PluginEntry,
    SystemPluginCliRunner,
};
use crate::utils::paths::{agent_home, home_dir};

const AVAILABLE_TIMEOUT: Duration = Duration::from_secs(20);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(120);
const UNINSTALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Homes + binaries + CLI runner used by enable/disable/install (tests inject fakes).
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
                row.name == name && marketplace_matches(row.marketplace.as_deref(), marketplace.as_deref())
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
    match restore_err {
        Some(restore) => Err(format!("{detail}; restore failed: {restore}")),
        None => Err(detail),
    }
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
    match restore_err {
        Some(restore) => Err(format!("{detail}; restore failed: {restore}")),
        None => Err(detail),
    }
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
            Self::Marketplace { name, marketplace } => match (agent_spec_needs_marketplace(name), marketplace) {
                (true, Some(market)) => format!("{name}@{market}"),
                _ => name.clone(),
            },
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
                return Err("Claude install accepts name@marketplace, not a git URL or local path".into());
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
    match restore_err {
        Some(restore) => Err(format!("{detail}; restore failed: {restore}")),
        None => Err(detail),
    }
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

fn vendor_spec(agent: AgentId, name: &str, marketplace: Option<&str>) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("plugin name is required".into());
    }
    if name == "mcpServers"
        || name.contains(['/', '\\', '\0'])
        || name.split(['/', '\\']).any(|part| part == "..")
    {
        return Err("invalid plugin name".into());
    }
    match agent {
        AgentId::Claude => {
            if name.contains('@') {
                Ok(name.to_string())
            } else if let Some(market) = marketplace.map(str::trim).filter(|s| !s.is_empty()) {
                if market.contains(['/', '\\', '\0']) {
                    return Err("invalid plugin marketplace".into());
                }
                Ok(format!("{name}@{market}"))
            } else {
                Ok(name.to_string())
            }
        }
        AgentId::Grok => Ok(name.to_string()),
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

fn cli_error_detail(run: &CliRun, action: &str) -> String {
    if run.unavailable() {
        return run
            .spawn_error
            .clone()
            .unwrap_or_else(|| "official command not found".into());
    }
    if run.timed_out {
        return format!("plugin {action} timed out");
    }
    let detail = run
        .stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .or_else(|| run.stdout.lines().map(str::trim).find(|l| !l.is_empty()))
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("exit {}", run.exit_code.unwrap_or(-1)));
    format!("plugin {action} failed: {detail}")
}

#[cfg(test)]
#[path = "plugin_apply/tests.rs"]
mod tests;
