//! Path contribution registry tests (separate from production modules).

use std::ffi::OsString;
use std::path::PathBuf;

use crate::models::AgentId;
use crate::platform::paths::{builtin_path_registry, resolve_agent_config_dir, resolve_agent_home};

fn restore_env(key: &str, prev: Option<OsString>) {
    match prev {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
}

#[test]
fn every_agent_has_path_contribution() {
    let reg = builtin_path_registry();
    for id in AgentId::ALL {
        assert!(reg.contains(id), "missing path for {id:?}");
        let home = reg.get(id).unwrap().home_dir().unwrap();
        assert!(!home.as_os_str().is_empty());
    }
}

#[test]
fn resolve_agent_home_matches_registry() {
    let reg = builtin_path_registry();
    for id in AgentId::ALL {
        let via_fn = resolve_agent_home(id).unwrap();
        let via_reg = reg.get(id).unwrap().home_dir().unwrap();
        assert_eq!(via_fn, via_reg, "{id:?}");
    }
}

#[test]
fn claude_home_honors_claude_config_dir_env() {
    let expected = PathBuf::from(if cfg!(windows) {
        r"D:\tmp\agenthub-claude-config-test"
    } else {
        "/tmp/agenthub-claude-config-test"
    });
    let prev = std::env::var_os("CLAUDE_CONFIG_DIR");
    std::env::set_var("CLAUDE_CONFIG_DIR", &expected);
    let home = resolve_agent_home(AgentId::Claude).unwrap();
    restore_env("CLAUDE_CONFIG_DIR", prev);
    assert_eq!(home, expected);
}

#[test]
fn pi_config_dir_defaults_to_home_agent_and_honors_env() {
    let prev = std::env::var_os("PI_CODING_AGENT_DIR");
    std::env::remove_var("PI_CODING_AGENT_DIR");
    let default_cfg = resolve_agent_config_dir(AgentId::Pi).unwrap();
    let home = resolve_agent_home(AgentId::Pi).unwrap();
    assert_eq!(default_cfg, home.join("agent"));

    let expected = PathBuf::from(if cfg!(windows) {
        r"D:\tmp\agenthub-pi-agent-test"
    } else {
        "/tmp/agenthub-pi-agent-test"
    });
    std::env::set_var("PI_CODING_AGENT_DIR", &expected);
    let overridden = resolve_agent_config_dir(AgentId::Pi).unwrap();
    restore_env("PI_CODING_AGENT_DIR", prev);
    assert_eq!(overridden, expected);
}

#[test]
fn workbuddy_config_dir_honors_env_overrides() {
    let prev_wb = std::env::var_os("WORKBUDDY_CONFIG_DIR");
    let prev_cb = std::env::var_os("CODEBUDDY_CONFIG_DIR");
    std::env::remove_var("WORKBUDDY_CONFIG_DIR");
    std::env::remove_var("CODEBUDDY_CONFIG_DIR");

    let default_cfg = resolve_agent_config_dir(AgentId::WorkBuddy).unwrap();
    let home = resolve_agent_home(AgentId::WorkBuddy).unwrap();
    assert_eq!(default_cfg, home);

    let expected = PathBuf::from(if cfg!(windows) {
        r"D:\tmp\agenthub-workbuddy-config-test"
    } else {
        "/tmp/agenthub-workbuddy-config-test"
    });
    std::env::set_var("WORKBUDDY_CONFIG_DIR", &expected);
    let overridden = resolve_agent_config_dir(AgentId::WorkBuddy).unwrap();
    restore_env("WORKBUDDY_CONFIG_DIR", prev_wb);
    restore_env("CODEBUDDY_CONFIG_DIR", prev_cb);
    assert_eq!(overridden, expected);
}

#[test]
fn codex_and_grok_config_dir_equals_home() {
    assert_eq!(
        resolve_agent_config_dir(AgentId::Codex).unwrap(),
        resolve_agent_home(AgentId::Codex).unwrap()
    );
    assert_eq!(
        resolve_agent_config_dir(AgentId::Grok).unwrap(),
        resolve_agent_home(AgentId::Grok).unwrap()
    );
}
