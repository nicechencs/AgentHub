//! Path contribution registry tests (separate from production modules).

use std::ffi::OsString;
use std::path::PathBuf;

use crate::models::AgentId;
use crate::platform::paths::{builtin_path_registry, resolve_agent_config_dir, resolve_agent_home};
use crate::utils::paths::{
    home_dir, validate_config_purge_target, validate_config_purge_target_with_data_dir,
    validate_default_agent_config_purge_target,
};

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
    let _guard = crate::utils::test_env::lock_test_env();
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
fn codex_home_honors_codex_home_env() {
    let _guard = crate::utils::test_env::lock_test_env();
    let _codex = crate::integrations::agents::codex::leftover::lock_codex_home();
    let expected = PathBuf::from(if cfg!(windows) {
        r"D:\tmp\agenthub-codex-home-test"
    } else {
        "/tmp/agenthub-codex-home-test"
    });
    let prev = std::env::var_os("CODEX_HOME");
    std::env::set_var("CODEX_HOME", &expected);
    let home = resolve_agent_home(AgentId::Codex).unwrap();
    restore_env("CODEX_HOME", prev);
    assert_eq!(home, expected);
}

#[test]
fn codex_home_blank_env_falls_back_to_dot_codex() {
    let _guard = crate::utils::test_env::lock_test_env();
    let _codex = crate::integrations::agents::codex::leftover::lock_codex_home();
    let prev = std::env::var_os("CODEX_HOME");
    std::env::set_var("CODEX_HOME", " , ");
    let home = resolve_agent_home(AgentId::Codex).unwrap();
    restore_env("CODEX_HOME", prev);
    assert_eq!(
        home,
        crate::utils::paths::home_dir().unwrap().join(".codex")
    );
}

#[test]
fn pi_config_dir_defaults_to_home_agent_and_honors_env() {
    let _guard = crate::utils::test_env::lock_test_env();
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
    let _guard = crate::utils::test_env::lock_test_env();
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
fn grok_home_honors_grok_home_env() {
    let _guard = crate::utils::test_env::lock_test_env();
    let expected = PathBuf::from(if cfg!(windows) {
        r"D:\tmp\agenthub-grok-home-test"
    } else {
        "/tmp/agenthub-grok-home-test"
    });
    let prev = std::env::var_os("GROK_HOME");
    std::env::set_var("GROK_HOME", &expected);
    let home = resolve_agent_home(AgentId::Grok).unwrap();
    restore_env("GROK_HOME", prev);
    assert_eq!(home, expected);
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

#[test]
fn config_purge_rejects_relative_path() {
    let data_dir = tempfile::tempdir().unwrap();
    let error = validate_config_purge_target_with_data_dir(
        std::path::Path::new("relative-config"),
        data_dir.path(),
    )
    .expect_err("relative purge paths must fail closed");
    assert!(error.to_string().contains("absolute"));

    let target = tempfile::tempdir().unwrap();
    let error = validate_config_purge_target_with_data_dir(
        target.path(),
        std::path::Path::new("relative-agenthub-data"),
    )
    .expect_err("relative actual data dirs must fail closed");
    assert!(error.to_string().contains("actual AgentHub data directory"));
}

#[test]
fn config_purge_rejects_broad_protected_directories() {
    let home = home_dir().unwrap();
    let home_parent = home.parent().expect("home has a parent");
    let data_guard = tempfile::tempdir().unwrap();
    let data = data_guard.path();
    let data_parent = data.parent().expect("data dir has a parent");
    let current = std::env::current_dir().unwrap();

    for path in [
        home.as_path(),
        home_parent,
        data,
        data_parent,
        current.as_path(),
    ] {
        assert!(
            validate_config_purge_target_with_data_dir(path, data).is_err(),
            "protected path unexpectedly accepted: {}",
            path.display()
        );
    }
}

#[test]
fn config_purge_rejects_symlink_ancestor() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let link = root.path().join("config-link");

    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    #[cfg(windows)]
    if std::os::windows::fs::symlink_dir(outside.path(), &link).is_err() {
        // Symlink creation requires Developer Mode or SeCreateSymbolicLinkPrivilege.
        return;
    }

    let target = link.join("nested-config");
    let error = validate_config_purge_target_with_data_dir(&target, data_dir.path())
        .expect_err("purge must not traverse a symlink/reparse ancestor");
    assert!(error.to_string().contains("symlink") || error.to_string().contains("reparse"));
}

#[test]
fn config_purge_accepts_safe_custom_temp_directory() {
    let root = tempfile::tempdir().unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let target = root.path().join("agent-config");
    std::fs::create_dir(&target).unwrap();

    let validated = validate_config_purge_target_with_data_dir(&target, data_dir.path())
        .expect("safe temp config directory");
    assert_eq!(validated, target);
}

#[test]
fn config_purge_without_actual_data_dir_fails_closed() {
    let target = tempfile::tempdir().unwrap();
    let error = validate_config_purge_target(target.path())
        .expect_err("purge validation must not guess the data directory");
    assert!(error.to_string().contains("actual AgentHub data directory"));
}

#[test]
fn config_purge_rejects_actual_data_dir_descendants_and_ancestors() {
    let root = tempfile::tempdir().unwrap();
    let data_dir = root.path().join("actual-agenthub");
    std::fs::create_dir(&data_dir).unwrap();

    let nested = data_dir.join("nested-config");
    assert!(validate_config_purge_target_with_data_dir(&nested, &data_dir).is_err());
    assert!(validate_config_purge_target_with_data_dir(root.path(), &data_dir).is_err());
}

#[test]
fn default_agent_purge_accepts_only_fixed_home() {
    let data_dir = tempfile::tempdir().unwrap();
    let target = validate_default_agent_config_purge_target(AgentId::Codex, data_dir.path())
        .expect("fixed default agent home should be accepted");
    assert_eq!(
        target,
        crate::utils::paths::default_agent_home(AgentId::Codex).unwrap()
    );
}

#[test]
fn default_agent_purge_rejects_pi_coding_agent_dir_override() {
    let _guard = crate::utils::test_env::lock_test_env();
    let data_dir = tempfile::tempdir().unwrap();
    let prev = std::env::var_os("PI_CODING_AGENT_DIR");
    std::env::set_var(
        "PI_CODING_AGENT_DIR",
        if cfg!(windows) {
            r"D:\tmp\agenthub-pi-purge-test"
        } else {
            "/tmp/agenthub-pi-purge-test"
        },
    );
    let error = validate_default_agent_config_purge_target(AgentId::Pi, data_dir.path())
        .expect_err("PI_CODING_AGENT_DIR must fail closed before purge");
    restore_env("PI_CODING_AGENT_DIR", prev);
    assert_eq!(error.code(), "invalid_arg");
    assert!(error.to_string().contains("custom agent config"));
}

#[test]
fn default_agent_purge_rejects_workbuddy_config_dir_overrides() {
    let _guard = crate::utils::test_env::lock_test_env();
    let data_dir = tempfile::tempdir().unwrap();
    let prev_wb = std::env::var_os("WORKBUDDY_CONFIG_DIR");
    let prev_cb = std::env::var_os("CODEBUDDY_CONFIG_DIR");
    std::env::remove_var("CODEBUDDY_CONFIG_DIR");
    std::env::set_var(
        "WORKBUDDY_CONFIG_DIR",
        if cfg!(windows) {
            r"D:\tmp\agenthub-workbuddy-purge-test"
        } else {
            "/tmp/agenthub-workbuddy-purge-test"
        },
    );
    let workbuddy_error =
        validate_default_agent_config_purge_target(AgentId::WorkBuddy, data_dir.path())
            .expect_err("WORKBUDDY_CONFIG_DIR must fail closed before purge");

    std::env::remove_var("WORKBUDDY_CONFIG_DIR");
    std::env::set_var(
        "CODEBUDDY_CONFIG_DIR",
        if cfg!(windows) {
            r"D:\tmp\agenthub-codebuddy-purge-test"
        } else {
            "/tmp/agenthub-codebuddy-purge-test"
        },
    );
    let codebuddy_error =
        validate_default_agent_config_purge_target(AgentId::WorkBuddy, data_dir.path())
            .expect_err("CODEBUDDY_CONFIG_DIR must fail closed before purge");
    restore_env("WORKBUDDY_CONFIG_DIR", prev_wb);
    restore_env("CODEBUDDY_CONFIG_DIR", prev_cb);

    assert_eq!(workbuddy_error.code(), "invalid_arg");
    assert!(workbuddy_error.to_string().contains("custom agent config"));
    assert_eq!(codebuddy_error.code(), "invalid_arg");
    assert!(codebuddy_error.to_string().contains("custom agent config"));
}
