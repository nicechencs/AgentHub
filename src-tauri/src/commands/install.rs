//! Install / upgrade / uninstall Tauri commands — thin wrappers over core.

use std::sync::Arc;

use agenthub_core::catalog::{list_install_catalog, AgentInstallCatalogEntry};
use agenthub_core::logging::targets;
use agenthub_core::models::{AgentId, AgentUpdateInfo, InstallOutcome, RuntimeId};
use agenthub_core::AgentKey;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::{map_err_string, parse_agent, with_hub_blocking};
use crate::state::AppState;

/// Event name for live install/upgrade log lines (frontend InlineTerminal).
pub const INSTALL_PROGRESS_EVENT: &str = "install-progress";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallProgressPayload {
    /// Agent id when install/upgrade/uninstall targets an agent; null for runtime-only.
    agent_id: Option<String>,
    /// `install` | `upgrade` | `uninstall` | `runtime`
    action: String,
    line: String,
}

fn install_progress_hook(
    app: AppHandle,
    agent_id: Option<String>,
    action: &'static str,
) -> agenthub_core::services::InstallLogHook {
    Arc::new(move |line: &str| {
        let payload = InstallProgressPayload {
            agent_id: agent_id.clone(),
            action: action.to_string(),
            line: line.to_string(),
        };
        if let Err(e) = app.emit(INSTALL_PROGRESS_EVENT, &payload) {
            tracing::debug!(
                target: targets::GUI,
                op = "install_progress",
                error = %e,
                "emit install-progress failed"
            );
        }
    })
}

/// Invoke: `list_install_catalog`
///
/// Read-only product install channels (npm packages / native URLs) from core catalog.
#[tauri::command]
pub fn list_install_catalog_cmd() -> Result<Vec<AgentInstallCatalogEntry>, String> {
    Ok(list_install_catalog())
}

fn parse_runtime(runtime: &str) -> Result<RuntimeId, String> {
    RuntimeId::parse(runtime).ok_or_else(|| {
        let msg = format!("invalid runtime '{runtime}', expected: nodejs|npm|powershell|git");
        tracing::warn!(target: targets::GUI, op = "parse_runtime", "{msg}");
        msg
    })
}

fn parse_lifecycle_agent_key(value: &str) -> Result<AgentKey, String> {
    let trimmed = value.trim();
    match AgentKey::parse(trimmed) {
        Ok(key) => Ok(key),
        Err(original) => {
            let normalized = trimmed.to_ascii_lowercase();
            let is_legacy_builtin = AgentId::ALL
                .iter()
                .any(|agent| agent.as_str() == normalized);
            if is_legacy_builtin {
                AgentKey::parse(normalized).map_err(|error| error.to_string())
            } else {
                Err(original.to_string())
            }
        }
    }
}

fn legacy_builtin_agent_id(key: &AgentKey) -> Option<AgentId> {
    AgentId::ALL
        .iter()
        .copied()
        .find(|agent| agent.as_str() == key.as_str())
}

/// Invoke: `install_runtime`
#[tauri::command]
pub async fn install_runtime(
    app: AppHandle,
    state: State<'_, AppState>,
    runtime_id: String,
    channel: Option<String>,
) -> Result<InstallOutcome, String> {
    let hub = state.hub_arc()?;
    let id = parse_runtime(&runtime_id)?;
    let channel = channel.unwrap_or_else(|| "winget".into());
    let hook = install_progress_hook(app, None, "runtime");
    with_hub_blocking(hub, move |hub| {
        hub.with_install_log_hook(hook, || {
            hub.install_runtime(id, &channel)
                .map_err(|e| map_err_string("install_runtime", e))
        })
    })
    .await
}

/// Invoke: `install_agent`
#[tauri::command]
pub async fn install_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    agent_id: String,
    channel: String,
    install_deps: Option<bool>,
) -> Result<InstallOutcome, String> {
    let hub = state.hub_arc()?;
    let key = parse_lifecycle_agent_key(&agent_id)?;
    let install_deps = install_deps.unwrap_or(false);
    let hook = install_progress_hook(app, Some(key.as_str().into()), "install");
    with_hub_blocking(hub, move |hub| {
        hub.with_install_log_hook(hook, || {
            hub.install_agent_key(&key, &channel, install_deps)
                .map_err(|e| map_err_string("install_agent", e))
        })
    })
    .await
}

/// Invoke: `upgrade_agent`
#[tauri::command]
pub async fn upgrade_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<InstallOutcome, String> {
    let hub = state.hub_arc()?;
    let key = parse_lifecycle_agent_key(&agent_id)?;
    let hook = install_progress_hook(app, Some(key.as_str().into()), "upgrade");
    with_hub_blocking(hub, move |hub| {
        hub.with_install_log_hook(hook, || {
            hub.upgrade_agent_key(&key)
                .map_err(|e| map_err_string("upgrade_agent", e))
        })
    })
    .await
}

/// Invoke: `check_agent_updates`
///
/// Probe npm registry (cached) for installed agents. Empty `agent_ids` → all agents.
/// `force` bypasses the 12h disk cache.
#[tauri::command]
pub async fn check_agent_updates(
    state: State<'_, AppState>,
    agent_ids: Option<Vec<String>>,
    force: Option<bool>,
) -> Result<Vec<AgentUpdateInfo>, String> {
    let hub = state.hub_arc()?;
    let force = force.unwrap_or(false);
    let parsed: Option<Vec<AgentId>> = match agent_ids {
        None => None,
        Some(ids) if ids.is_empty() => None,
        Some(ids) => {
            let mut out = Vec::with_capacity(ids.len());
            for raw in ids {
                out.push(parse_agent(&raw)?);
            }
            Some(out)
        }
    };
    with_hub_blocking(hub, move |hub| {
        let slice = parsed.as_deref();
        hub.check_agent_updates(slice, force)
            .map_err(|e| map_err_string("check_agent_updates", e))
    })
    .await
}

/// Invoke: `uninstall_agent`
#[tauri::command]
pub async fn uninstall_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    agent_id: String,
    purge_config: Option<bool>,
) -> Result<InstallOutcome, String> {
    let hub = state.hub_arc()?;
    let key = parse_lifecycle_agent_key(&agent_id)?;
    let purge = purge_config.unwrap_or(false);
    let hook = install_progress_hook(app, Some(key.as_str().into()), "uninstall");
    with_hub_blocking(hub, move |hub| {
        hub.with_install_log_hook(hook, || {
            if purge {
                // Best-effort pre-uninstall backup
                if let Some(agent) = legacy_builtin_agent_id(&key) {
                    let _ = hub.backups.snapshot(
                        agent,
                        agenthub_core::models::BackupKind::PreUninstall,
                        Some("pre-uninstall"),
                    );
                }
            }
            hub.uninstall_agent_key(&key, purge)
                .map_err(|e| map_err_string("uninstall_agent", e))
        })
    })
    .await
}

/// Invoke: `open_agent_config_dir` — open agent live config dir in OS file manager.
///
/// Uses [`agent_config_dir`] (not bare home): e.g. Pi → `~/.pi/agent`,
/// WorkBuddy env overrides, Claude `CLAUDE_CONFIG_DIR`.
#[tauri::command]
pub async fn open_agent_config_dir(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<String, String> {
    let hub = state.hub_arc()?;
    let id = parse_agent(&agent_id)?;
    with_hub_blocking(hub, move |_hub| {
        let path = agenthub_core::utils::paths::agent_config_dir(id)
            .map_err(|e| map_err_string("open_agent_config_dir", e))?;
        if !path.exists() {
            std::fs::create_dir_all(&path).map_err(|e| {
                let msg = e.to_string();
                tracing::warn!(
                    target: targets::GUI,
                    op = "open_agent_config_dir",
                    error = %msg,
                    "create agent config dir failed"
                );
                msg
            })?;
        }
        open_in_file_manager(&path)?;
        Ok(path.display().to_string())
    })
    .await
}

/// Invoke: `open_path_in_file_manager` — open any absolute directory path.
///
/// Accepts path-format drift from project discovery (`D:/work`, `cwd/D:/work`).
/// If `path` points at a file, opens its parent directory.
#[tauri::command]
pub async fn open_path_in_file_manager(path: String) -> Result<String, String> {
    let p = normalize_open_path_input(&path);
    if !p.exists() {
        let msg = format!("path does not exist: {path}");
        tracing::warn!(target: targets::GUI, op = "open_path_in_file_manager", "{msg}");
        return Err(msg);
    }
    let open_target = if p.is_file() {
        p.parent()
            .map(|parent| parent.to_path_buf())
            .unwrap_or(p.clone())
    } else {
        p.clone()
    };
    open_in_file_manager(&open_target)?;
    Ok(open_target.display().to_string())
}

/// Strip storage-key prefixes and normalize separators for the current OS.
fn normalize_open_path_input(raw: &str) -> std::path::PathBuf {
    let mut s = raw.trim().to_string();
    if let Some(rest) = s.strip_prefix("cwd/") {
        s = rest.to_string();
    }
    #[cfg(windows)]
    {
        // Explorer is more reliable with backslashes; Path::exists accepts both.
        s = s.replace('/', "\\");
    }
    std::path::PathBuf::from(s)
}

fn open_in_file_manager(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let arg = path.to_string_lossy().replace('/', "\\");
        std::process::Command::new("explorer")
            .arg(arg)
            .spawn()
            .map_err(|e| {
                let msg = format!("open explorer failed: {e}");
                tracing::warn!(target: targets::GUI, op = "open_in_file_manager", "{msg}");
                msg
            })?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| {
                let msg = format!("open failed: {e}");
                tracing::warn!(target: targets::GUI, op = "open_in_file_manager", "{msg}");
                msg
            })?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| {
                let msg = format!("xdg-open failed: {e}");
                tracing::warn!(target: targets::GUI, op = "open_in_file_manager", "{msg}");
                msg
            })?;
        Ok(())
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = path;
        let msg = "open file manager unsupported on this platform".to_string();
        tracing::warn!(target: targets::GUI, op = "open_in_file_manager", "{msg}");
        Err(msg)
    }
}

#[cfg(test)]
mod tests;
