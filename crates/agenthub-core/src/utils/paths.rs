//! Home / data-dir resolution.
//! Never use the HOME env var on Windows (Git Bash may inject a wrong value).

use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::models::AgentId;

/// Resolve user home via `dirs::home_dir()` only.
pub fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| AppError::message("paths.home", "cannot resolve home directory"))
}

/// Default data dir: `~/.agenthub`
pub fn default_data_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".agenthub"))
}

/// Resolve data dir with L0 priority:
/// 1. explicit override (CLI `--data-dir`)
/// 2. `AGENTHUB_HOME` env (absolute or `~` / `~/...`; never raw `HOME`)
/// 3. `~/.agenthub`
pub fn resolve_data_dir(override_dir: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = override_dir {
        return Ok(p.to_path_buf());
    }
    if let Ok(v) = std::env::var("AGENTHUB_HOME") {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return expand_user_path(trimmed);
        }
    }
    default_data_dir()
}

/// Resolve a data directory to an absolute filesystem identity once at the
/// composition boundary.  Unlike `resolve_data_dir`, this never consults an
/// environment variable and therefore remains stable if the process cwd or
/// environment changes later.
pub fn normalize_data_dir(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let lexical = normalize_data_dir_lexically(&absolute)?;
    path_identity(&lexical)
}

/// Expand leading `~` via `dirs::home_dir()` (not the HOME env var).
pub fn expand_user_path(raw: &str) -> Result<PathBuf> {
    if raw == "~" {
        return home_dir();
    }
    if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        let mut path = home_dir()?;
        for part in rest.split(['/', '\\']) {
            if !part.is_empty() && part != "." {
                path.push(part);
            }
        }
        return Ok(path);
    }
    Ok(PathBuf::from(raw))
}

pub fn ensure_data_layout(data_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(data_dir)?;
    std::fs::create_dir_all(data_dir.join("backups").join("live"))?;
    std::fs::create_dir_all(data_dir.join("backups").join("db"))?;
    std::fs::create_dir_all(data_dir.join("exports"))?;
    std::fs::create_dir_all(data_dir.join("logs"))?;
    std::fs::create_dir_all(data_dir.join("cache"))?;
    std::fs::create_dir_all(data_dir.join("usage-gateway"))?;
    Ok(())
}

pub fn db_path(data_dir: &Path) -> PathBuf {
    data_dir.join("agenthub.db")
}

pub fn backups_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("backups")
}

pub fn logs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

/// Spool directory for per-request gateway usage events (`gateway-*.jsonl`),
/// written by the local bridge and ingested into the `gateway_usage` table by
/// the usage collect pipeline.
pub fn usage_gateway_dir() -> Result<PathBuf> {
    Ok(resolve_data_dir(None)?.join("usage-gateway"))
}

/// Disposable sqlite file for Activity / route monitoring traces.
///
/// Lives under `{data_dir}/cache/route-traces.db`, separate from `agenthub.db`.
/// Deleting it only clears monitoring history. A leftover
/// `cache/route-traces.json` is imported once then removed.
pub fn route_traces_persist_path() -> Result<PathBuf> {
    Ok(resolve_data_dir(None)?
        .join("cache")
        .join("route-traces.db"))
}

/// Typical live config roots per agent (may not exist yet).
///
/// Agent-specific roots live in [`crate::platform::paths`] contributions.
/// This function is a façade so call sites stay stable.
pub fn agent_home(agent: AgentId) -> Result<PathBuf> {
    crate::platform::paths::resolve_agent_home(agent)
}

/// Resolve the fixed, built-in root for an agent.  Unlike [`agent_home`], this
/// never follows an agent-owned environment override.
pub fn default_agent_home(agent: AgentId) -> Result<PathBuf> {
    crate::platform::paths::resolve_default_agent_home(agent)
}

/// Whether the resolved agent root is the contribution's fixed default.
pub fn agent_home_is_default(agent: AgentId) -> Result<bool> {
    crate::platform::paths::agent_home_is_default(agent)
}

/// Whether the resolved live config directory is the contribution's fixed default.
pub fn agent_config_dir_is_default(agent: AgentId) -> Result<bool> {
    crate::platform::paths::agent_config_dir_is_default(agent)
}

/// Directory to open in the OS file manager for manual verification.
///
/// Prefer the directory that actually holds settings/credentials when it differs
/// from [`agent_home`] (Pi → `~/.pi/agent`, WorkBuddy env overrides).
pub fn agent_config_dir(agent: AgentId) -> Result<PathBuf> {
    crate::platform::paths::resolve_agent_config_dir(agent)
}

/// Resolved config / login file paths for UI display (honors env overrides).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLivePaths {
    pub config: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<String>,
    pub open_dir: String,
}

/// Join `agent_config_dir` / `agent_home` with native filenames and display with `~`.
pub fn agent_live_paths(agent: AgentId) -> Result<AgentLivePaths> {
    let home = agent_home(agent)?;
    let config_dir = agent_config_dir(agent)?;
    let open_dir = display_user_path(&config_dir)?;
    let join = |dir: &Path, name: &str| -> Result<String> { display_user_path(&dir.join(name)) };
    Ok(match agent {
        AgentId::Claude => AgentLivePaths {
            config: join(&config_dir, "settings.json")?,
            auth: Some(join(&config_dir, ".credentials.json")?),
            extra: vec![display_user_path(&home_dir()?.join(".claude.json"))?],
            open_dir,
        },
        AgentId::Codex => AgentLivePaths {
            config: join(&config_dir, "config.toml")?,
            auth: Some(join(&config_dir, "auth.json")?),
            extra: Vec::new(),
            open_dir,
        },
        AgentId::Kimi => AgentLivePaths {
            config: join(&home, "config.toml")?,
            auth: Some(display_user_path(
                &home.join("credentials").join("kimi-code.json"),
            )?),
            extra: Vec::new(),
            open_dir,
        },
        AgentId::Grok => AgentLivePaths {
            config: join(&config_dir, "config.toml")?,
            auth: Some(join(&config_dir, "auth.json")?),
            extra: Vec::new(),
            open_dir,
        },
        AgentId::Pi => AgentLivePaths {
            config: join(&config_dir, "settings.json")?,
            auth: Some(join(&config_dir, "auth.json")?),
            extra: vec![join(&config_dir, "models.json")?],
            open_dir,
        },
        AgentId::WorkBuddy => AgentLivePaths {
            config: join(&config_dir, "settings.json")?,
            auth: None,
            extra: vec![
                join(&config_dir, "models.json")?,
                join(&config_dir, ".mcp.json")?,
            ],
            open_dir,
        },
        AgentId::Cursor => AgentLivePaths {
            config: "无稳定 provider 配置文件".into(),
            auth: None,
            extra: Vec::new(),
            open_dir,
        },
        AgentId::Dsh => AgentLivePaths {
            config: join(&home, "cordis.patch.yml")?,
            auth: Some(join(&home, ".credentials.yaml")?),
            extra: Vec::new(),
            open_dir,
        },
        AgentId::Zcode => AgentLivePaths {
            config: join(&home, "v2/config.json")?,
            auth: Some(join(&home, "v2/config.json")?),
            extra: vec![join(&home, "cli/config.json")?],
            open_dir,
        },
    })
}

fn display_user_path(path: &Path) -> Result<String> {
    let home = home_dir()?;
    let rendered = if let Ok(rest) = path.strip_prefix(&home) {
        format!("~/{}", rest.display())
    } else {
        path.display().to_string()
    };
    Ok(rendered.replace('\\', "/"))
}

/// Compatibility boundary for callers that do not own the resolved data dir.
///
/// A purge target cannot be validated safely without the actual data directory
/// belonging to the open database. In particular, consulting `AGENTHUB_HOME`
/// here would validate against a guessed root and could authorize deletion of
/// another database's files. Callers must use
/// [`validate_config_purge_target_with_data_dir`] instead.
pub fn validate_config_purge_target(_path: &Path) -> Result<PathBuf> {
    Err(AppError::InvalidArg(
        "cannot validate config purge without the actual AgentHub data directory".into(),
    ))
}

/// Validate a recursive purge target against the actual AgentHub data dir.
///
/// The caller must provide the data dir resolved by the owning `AgentHub`.
/// This deliberately rejects both ancestors and descendants of that dir so a
/// custom data-dir override cannot be bypassed by an alias or a nested target.
pub fn validate_config_purge_target_with_data_dir(
    path: &Path,
    actual_data_dir: &Path,
) -> Result<PathBuf> {
    let display = normalized_display_path(path);
    let target = normalize_absolute_path(path).map_err(|reason| {
        AppError::InvalidArg(format!(
            "unsafe config purge path {}: {reason}",
            display.display()
        ))
    })?;

    if is_filesystem_root(&target) {
        return Err(AppError::InvalidArg(format!(
            "unsafe config purge path {}: filesystem root is not allowed",
            target.display()
        )));
    }

    // Resolve all protected locations once for this validation. The owning
    // AgentHub supplies `actual_data_dir`, including an explicit override.
    let home = absolute_for_comparison(&home_dir()?)?;
    let current = absolute_for_comparison(&std::env::current_dir()?)?;
    let data_dir = normalize_absolute_path(actual_data_dir).map_err(|reason| {
        AppError::InvalidArg(format!(
            "invalid actual AgentHub data directory {}: {reason}",
            actual_data_dir.display()
        ))
    })?;

    // Reject links before identity resolution.  Existing aliases such as
    // Windows 8.3 names and case variants are then folded by canonicalize.
    ensure_no_symlink_in_existing_prefix(&target)?;
    match std::fs::symlink_metadata(&target) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(AppError::InvalidArg(format!(
                "unsafe config purge path {}: cannot inspect target: {e}",
                target.display()
            )))
        }
        Ok(meta) if is_link_or_reparse(&meta) => {
            return Err(AppError::InvalidArg(format!(
                "unsafe config purge path {}: target is a symlink or reparse point",
                target.display()
            )))
        }
        Ok(meta) if !meta.is_dir() => {
            return Err(AppError::InvalidArg(format!(
                "unsafe config purge path {}: target is not a directory",
                target.display()
            )))
        }
        Ok(_) => {}
    }

    let target_identity = path_identity(&target)?;
    let home_identity = path_identity(&home)?;
    let current_identity = path_identity(&current)?;
    let data_identity = path_identity(&data_dir)?;

    if is_same_or_ancestor(&target_identity, &home_identity) {
        return Err(AppError::InvalidArg(format!(
            "unsafe config purge path {}: user home or one of its parents is not allowed",
            target.display()
        )));
    }
    if is_same_or_ancestor(&target_identity, &current_identity) {
        return Err(AppError::InvalidArg(format!(
            "unsafe config purge path {}: current directory or one of its parents is not allowed",
            target.display()
        )));
    }
    if paths_overlap(&target_identity, &data_identity) {
        return Err(AppError::InvalidArg(format!(
            "unsafe config purge path {}: AgentHub data directory or an overlapping path is not allowed",
            target.display()
        )));
    }

    Ok(target)
}

/// Validate that an agent purge resolves to its fixed, dedicated default root.
/// Any agent-owned environment override is rejected even when it happens to
/// point back at the default path.
pub fn validate_default_agent_config_purge_target(
    agent: AgentId,
    actual_data_dir: &Path,
) -> Result<PathBuf> {
    let requested = agent_home(agent)?;
    let display = normalized_display_path(&requested);
    if !agent_home_is_default(agent)? || !agent_config_dir_is_default(agent)? {
        return Err(AppError::InvalidArg(format!(
            "unsafe config purge path {}: custom agent config directory overrides cannot be purged",
            display.display()
        )));
    }

    let target = validate_config_purge_target_with_data_dir(&requested, actual_data_dir)?;
    let default = default_agent_home(agent)?;
    if !same_path_identity(&target, &default)? {
        return Err(AppError::InvalidArg(format!(
            "unsafe config purge path {}: only the agent's fixed default directory may be purged",
            target.display()
        )));
    }
    Ok(target)
}

/// Compare two paths using filesystem identity where possible, including
/// Windows case/8.3 aliases and the canonical identity of existing parents.
pub fn same_path_identity(left: &Path, right: &Path) -> Result<bool> {
    Ok(path_identity(left)? == path_identity(right)?)
}

fn normalize_absolute_path(path: &Path) -> std::result::Result<PathBuf, &'static str> {
    if !path.is_absolute() {
        return Err("path must be absolute");
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => return Err("parent-directory components are not allowed"),
            Component::Normal(name) => normalized.push(name),
        }
    }
    Ok(normalized)
}

fn absolute_for_comparison(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return normalize_absolute_path(path).map_err(|reason| {
            AppError::InvalidArg(format!(
                "invalid protected path {}: {reason}",
                path.display()
            ))
        });
    }

    let current = std::env::current_dir()?;
    normalize_absolute_path(&current.join(path)).map_err(|reason| {
        AppError::InvalidArg(format!(
            "invalid protected path {}: {reason}",
            path.display()
        ))
    })
}

fn normalized_display_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        normalize_absolute_path(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        absolute_for_comparison(path).unwrap_or_else(|_| path.to_path_buf())
    }
}

fn normalize_data_dir_lexically(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(AppError::InvalidArg(format!(
                        "invalid data directory {}: parent escapes filesystem root",
                        path.display()
                    )));
                }
            }
            Component::Normal(name) => normalized.push(name),
        }
    }
    if !normalized.is_absolute() {
        return Err(AppError::InvalidArg(format!(
            "invalid data directory {}: path must be absolute",
            path.display()
        )));
    }
    Ok(normalized)
}

/// Return a comparison identity without following a symlink in the purge
/// target. Existing prefixes are checked by the caller before this helper is
/// used for a target; protected locations may themselves be aliases and are
/// intentionally canonicalized here.
fn path_identity(path: &Path) -> Result<PathBuf> {
    let normalized = normalize_absolute_path(path).map_err(|reason| {
        AppError::InvalidArg(format!(
            "cannot resolve path identity {}: {reason}",
            path.display()
        ))
    })?;
    let mut existing = normalized.clone();
    let mut missing = Vec::new();

    loop {
        match std::fs::symlink_metadata(&existing) {
            Ok(_) => {
                let mut identity = std::fs::canonicalize(&existing).map_err(|e| {
                    AppError::InvalidArg(format!(
                        "cannot resolve path identity {}: {e}",
                        path.display()
                    ))
                })?;
                for component in missing.iter().rev() {
                    identity.push(component);
                }
                // Windows canonicalize adds `\\?\`; simplify for user-facing
                // data_dir. dunce keeps the prefix when the path cannot be
                // represented safely (long paths, reserved device names).
                let identity = dunce::simplified(&identity).to_path_buf();
                return normalize_absolute_path(&identity).map_err(|reason| {
                    AppError::InvalidArg(format!(
                        "cannot normalize path identity {}: {reason}",
                        path.display()
                    ))
                });
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let Some(name) = existing.file_name() else {
                    return Err(AppError::InvalidArg(format!(
                        "cannot resolve path identity {}: no existing parent",
                        path.display()
                    )));
                };
                missing.push(name.to_os_string());
                if !existing.pop() {
                    return Err(AppError::InvalidArg(format!(
                        "cannot resolve path identity {}: no existing parent",
                        path.display()
                    )));
                }
            }
            Err(e) => {
                return Err(AppError::InvalidArg(format!(
                    "cannot inspect path identity {}: {e}",
                    path.display()
                )))
            }
        }
    }
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    is_same_or_ancestor(left, right) || is_same_or_ancestor(right, left)
}

fn is_filesystem_root(path: &Path) -> bool {
    path.file_name().is_none()
}

/// Whether `candidate` equals `descendant` or is one of its parents.
fn is_same_or_ancestor(candidate: &Path, descendant: &Path) -> bool {
    let candidate = path_component_keys(candidate);
    let descendant = path_component_keys(descendant);
    candidate.len() <= descendant.len()
        && candidate
            .iter()
            .zip(descendant.iter())
            .all(|(left, right)| left == right)
}

fn path_component_keys(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| {
            let value = component.as_os_str().to_string_lossy();
            #[cfg(windows)]
            {
                value.to_ascii_lowercase()
            }
            #[cfg(not(windows))]
            {
                value.into_owned()
            }
        })
        .collect()
}

/// Treat every link-like Windows reparse point as unsafe for recursive purge.
fn is_link_or_reparse(meta: &std::fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }

    #[cfg(not(windows))]
    {
        false
    }
}

/// Walk existing path prefixes without following links. Missing suffixes are
/// safe because no later component can currently redirect traversal.
fn ensure_no_symlink_in_existing_prefix(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            Component::RootDir => current.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(AppError::InvalidArg(format!(
                    "unsafe config purge path {}: parent-directory components are not allowed",
                    path.display()
                )))
            }
            Component::Normal(name) => {
                current.push(name);
                match std::fs::symlink_metadata(&current) {
                    Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
                    Err(e) => {
                        return Err(AppError::InvalidArg(format!(
                            "unsafe config purge path {}: cannot inspect path component: {e}",
                            current.display()
                        )))
                    }
                    Ok(meta) if is_link_or_reparse(&meta) => {
                        return Err(AppError::InvalidArg(format!(
                            "unsafe config purge path {}: path traverses a symlink or reparse point at {}",
                            path.display(),
                            current.display()
                        )))
                    }
                    Ok(_) => {}
                }
            }
        }
    }
    Ok(())
}

/// First non-empty path from a (possibly comma-separated) env var.
///
/// Used by path contributions (CLAUDE_CONFIG_DIR, PI_CODING_AGENT_DIR, …).
pub fn first_env_path(key: &str) -> Option<PathBuf> {
    let v = std::env::var(key).ok()?;
    let raw = v.split(',').map(str::trim).find(|s| !s.is_empty())?;
    Some(PathBuf::from(raw))
}

/// Reject path injection characters used in shell contexts.
pub fn is_safe_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    !s.chars()
        .any(|c| matches!(c, '&' | '|' | ';' | '`' | '\n' | '\r' | '$'))
}

#[cfg(test)]
mod tests;
