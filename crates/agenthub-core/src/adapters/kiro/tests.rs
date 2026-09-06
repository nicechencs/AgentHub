use std::path::{Path, PathBuf};

use crate::models::{
    AccountKind, AgentConfig, AgentId, AuthHealth, Capability, CapabilityLevel, DetectResult,
    DetectStatus, RunOptions,
};

use super::*;

#[test]
fn build_run_spec_headless_chat() {
    let adapter = KiroAdapter;
    let bin = PathBuf::from("kiro-cli");
    let spec = adapter
        .build_run_spec(&bin, "hello", &RunOptions::default())
        .unwrap();
    assert_eq!(spec.agent, AgentId::Kiro);
    assert_eq!(spec.program, bin);
    assert_eq!(spec.args, vec!["chat", "--no-interactive", "hello"]);
}

#[test]
fn build_run_spec_allow_dangerous_adds_trust_all_tools() {
    let adapter = KiroAdapter;
    let mut opts = RunOptions::default();
    opts.allow_dangerous = true;
    let spec = adapter
        .build_run_spec(Path::new("kiro-cli"), "x", &opts)
        .unwrap();
    assert!(spec.args.iter().any(|a| a == "--trust-all-tools"));
    assert_eq!(spec.args.last().map(String::as_str), Some("x"));
}

#[test]
fn write_config_is_fail_closed() {
    let err = KiroAdapter
        .write_config(&AgentConfig {
            agent: AgentId::Kiro,
            raw: serde_json::json!({}),
        })
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
}

#[test]
fn account_switch_and_session_resume_blocked() {
    assert!(KiroAdapter
        .capability(Capability::AccountSwitch)
        .is_blocked());
    assert!(KiroAdapter
        .capability(Capability::SessionResume)
        .is_blocked());
    assert_eq!(
        KiroAdapter.capability(Capability::DangerousMode).level,
        CapabilityLevel::Partial
    );
}

#[test]
fn skills_dir_is_none_until_paths_verified() {
    assert!(KiroAdapter.skills_dir().is_none());
}

#[test]
fn build_api_key_account_pool_only() {
    let acc = KiroAdapter
        .build_api_key_account("kiro-secret-key")
        .unwrap();
    assert_eq!(acc.agent, AgentId::Kiro);
    assert_eq!(acc.kind, AccountKind::ApiKey);
    let err = KiroAdapter.apply_account(&acc).unwrap_err();
    assert_eq!(err.code(), "unsupported");
}

#[test]
fn kiro_status_health_parses_explicit_lines() {
    assert_eq!(
        kiro_status_health("logged in as demo"),
        AuthHealth::Verified
    );
    assert_eq!(
        kiro_status_health("Not authenticated"),
        AuthHealth::NeedsLogin
    );
    assert_eq!(kiro_status_health("something else"), AuthHealth::Unknown);
}

#[test]
fn desktop_ide_copy_does_not_count_as_installed() {
    let mut result = DetectResult {
        agent: AgentId::Kiro,
        status: DetectStatus::NotFound,
        version: None,
        binary_path: None,
        channel: None,
        env_ready: true,
        notes: Vec::new(),
        extra_copies: Vec::new(),
    };
    attach_kiro_desktop_copy(&mut result);
    assert_eq!(result.status, DetectStatus::NotFound);
    assert!(result.binary_path.is_none());
    assert!(result.extra_copies.iter().all(|c| c.kind == "desktop"));
}

#[test]
fn detect_does_not_treat_ide_as_cli() {
    let r = KiroAdapter.detect();
    if r.status == DetectStatus::Installed {
        let path = r.binary_path.as_ref().unwrap();
        assert!(
            path_looks_like_kiro_cli(path),
            "installed path must be kiro-cli: {}",
            path.display()
        );
    } else {
        assert_eq!(r.status, DetectStatus::NotFound);
        assert!(r.binary_path.is_none());
    }
    assert!(
        r.extra_copies.iter().all(|c| c.kind == "desktop"),
        "IDE/desktop must stay extra: {:?}",
        r.extra_copies
    );
}

#[test]
fn install_channels_native_only() {
    let channels = KiroAdapter.install_channels();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].id, "native");
    #[cfg(windows)]
    assert!(channels[0]
        .requires
        .contains(&crate::models::RuntimeId::PowerShell));
    #[cfg(not(windows))]
    assert!(
        !channels[0]
            .requires
            .contains(&crate::models::RuntimeId::PowerShell),
        "macOS/Linux native channel must not require PowerShell"
    );
}
