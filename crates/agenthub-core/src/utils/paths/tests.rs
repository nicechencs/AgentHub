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
