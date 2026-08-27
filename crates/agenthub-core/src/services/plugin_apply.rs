//! Enable / disable listed Claude and Grok plugin packs via official CLI.
//!
//! Snapshots the agent's settings/config file before the CLI runs. If the CLI
//! fails, the snapshot is restored so a half-written file is not left behind.
//! AgentHub does not edit vendor plugin cache itself.

use std::fs;
use std::path::{Path, PathBuf};

use crate::models::AgentId;
use crate::services::plugin_inventory::{CliRun, PluginCliRunner, SystemPluginCliRunner};
use crate::utils::paths::agent_home;

/// Homes + binaries + CLI runner used by enable/disable (tests inject fakes).
pub struct PluginApplyContext<'a> {
    pub claude_home: PathBuf,
    pub grok_home: PathBuf,
    pub claude_bin: Option<PathBuf>,
    pub grok_bin: Option<PathBuf>,
    pub runner: &'a dyn PluginCliRunner,
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
    let claude_home = agent_home(AgentId::Claude).unwrap_or_else(|_| PathBuf::from("/"));
    let grok_home = agent_home(AgentId::Grok).unwrap_or_else(|_| PathBuf::from("/"));
    PluginApplyContext {
        claude_home,
        grok_home,
        claude_bin: which::which("claude").ok(),
        grok_bin: which::which("grok").ok(),
        runner: &RUNNER,
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
