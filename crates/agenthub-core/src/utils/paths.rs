//! Home / data-dir resolution.
//! Never use the HOME env var on Windows (Git Bash may inject a wrong value).

use std::path::{Path, PathBuf};

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

/// Expand leading `~` via `dirs::home_dir()` (not the HOME env var).
fn expand_user_path(raw: &str) -> Result<PathBuf> {
    if raw == "~" {
        return home_dir();
    }
    if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        return Ok(home_dir()?.join(rest));
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

/// Typical live config roots per agent (may not exist yet).
///
/// Agent-specific roots live in [`crate::platform::paths`] contributions.
/// This function is a façade so call sites stay stable.
pub fn agent_home(agent: AgentId) -> Result<PathBuf> {
    crate::platform::paths::resolve_agent_home(agent)
}

/// Directory to open in the OS file manager for manual verification.
///
/// Prefer the directory that actually holds settings/credentials when it differs
/// from [`agent_home`] (Pi → `~/.pi/agent`, WorkBuddy env overrides).
pub fn agent_config_dir(agent: AgentId) -> Result<PathBuf> {
    crate::platform::paths::resolve_agent_config_dir(agent)
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
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize env mutation so parallel cargo tests do not race.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn resolve_data_dir_prefers_override() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("AGENTHUB_HOME");
        std::env::set_var("AGENTHUB_HOME", "C:\\should-not-use");
        let override_dir = PathBuf::from("D:\\tmp\\agenthub-override");
        let got = resolve_data_dir(Some(&override_dir)).expect("override resolve");
        assert_eq!(got, override_dir);
        restore_env("AGENTHUB_HOME", prev);
    }

    #[test]
    fn resolve_data_dir_uses_agenthub_home() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("AGENTHUB_HOME");
        let expected = PathBuf::from("D:\\tmp\\agenthub-home-test");
        std::env::set_var("AGENTHUB_HOME", &expected);
        let got = resolve_data_dir(None).expect("env resolve");
        assert_eq!(got, expected);
        restore_env("AGENTHUB_HOME", prev);
    }

    #[test]
    fn resolve_data_dir_ignores_empty_agenthub_home() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("AGENTHUB_HOME");
        std::env::set_var("AGENTHUB_HOME", "   ");
        let got = resolve_data_dir(None).expect("default resolve");
        let expected = default_data_dir().expect("default data dir");
        assert_eq!(got, expected);
        restore_env("AGENTHUB_HOME", prev);
    }

    #[test]
    fn resolve_data_dir_expands_tilde_in_agenthub_home() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("AGENTHUB_HOME");
        std::env::set_var("AGENTHUB_HOME", "~\\.agenthub-tilde-test");
        let got = resolve_data_dir(None).expect("tilde resolve");
        let expected = home_dir().expect("home").join(".agenthub-tilde-test");
        assert_eq!(got, expected);
        restore_env("AGENTHUB_HOME", prev);
    }

    #[test]
    fn is_safe_path_rejects_shell_metacharacters() {
        assert!(is_safe_path(Path::new("C:\\Users\\demo\\.agenthub")));
        assert!(is_safe_path(Path::new("/home/user/.agenthub")));
        assert!(!is_safe_path(Path::new("C:\\tmp&evil")));
        assert!(!is_safe_path(Path::new("C:\\tmp|pipe")));
        assert!(!is_safe_path(Path::new("C:\\tmp;cmd")));
        assert!(!is_safe_path(Path::new("C:\\tmp`tick")));
        assert!(!is_safe_path(Path::new("C:\\tmp$VAR")));
    }

    #[test]
    fn agent_home_claude_honors_claude_config_dir() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("CLAUDE_CONFIG_DIR");
        let expected = PathBuf::from("D:\\tmp\\claude-config-override");
        std::env::set_var("CLAUDE_CONFIG_DIR", &expected);
        let got = agent_home(AgentId::Claude).expect("claude home");
        assert_eq!(got, expected);
        // comma-separated: first wins
        std::env::set_var("CLAUDE_CONFIG_DIR", "D:\\tmp\\claude-a,D:\\tmp\\claude-b");
        let first = agent_home(AgentId::Claude).expect("first path");
        assert_eq!(first, PathBuf::from("D:\\tmp\\claude-a"));
        restore_env("CLAUDE_CONFIG_DIR", prev);
    }

    #[test]
    fn agent_config_dir_pi_uses_agent_subdir_or_env() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("PI_CODING_AGENT_DIR");
        std::env::remove_var("PI_CODING_AGENT_DIR");
        let default = agent_config_dir(AgentId::Pi).expect("pi config dir");
        assert_eq!(default, agent_home(AgentId::Pi).unwrap().join("agent"));
        let expected = PathBuf::from("D:\\tmp\\pi-agent-override");
        std::env::set_var("PI_CODING_AGENT_DIR", &expected);
        assert_eq!(agent_config_dir(AgentId::Pi).expect("pi env"), expected);
        restore_env("PI_CODING_AGENT_DIR", prev);
    }

    fn restore_env(key: &str, prev: Option<std::ffi::OsString>) {
        match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
}
