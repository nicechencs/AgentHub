use super::*;
use crate::output::map_install_failure;
use agenthub_core::models::InstallOutcome;
use agenthub_core::AgentHub;

#[test]
fn install_rejects_unknown_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let err = install(&hub, "python", "", OutputFormat::Quiet).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(err.to_string().contains("nodejs"));
}

#[test]
fn list_quiet_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    list(&hub, OutputFormat::Quiet).unwrap();
}

#[test]
fn powershell_install_is_unsupported() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let err = install(&hub, "powershell", "", OutputFormat::Quiet).unwrap_err();
    assert_eq!(err.code(), "unsupported");
}

#[test]
fn map_install_failure_missing_winget_is_env_not_ready() {
    let details = serde_json::json!({
        "agent": null,
        "channel": "winget",
        "missing": ["nodejs"],
        "remediations": [
            { "kind": "hint", "url": "https://nodejs.org/", "text": "Install Node.js LTS" }
        ],
        "hint": "未找到 winget。请手动安装 Node.js LTS 后重新检测。"
    });
    let outcome = InstallOutcome::failure(
        "env_install",
        vec![],
        "未找到 winget。请手动安装 Node.js LTS 后重新检测。",
    )
    .with_code("env.not_ready", Some(details));
    let err = map_install_failure(&outcome);
    assert_eq!(err.code(), "env.not_ready");
    assert_eq!(err.details()["channel"], "winget");
    assert_eq!(err.details()["missing"][0], "nodejs");
    assert!(err.details()["remediations"].is_array());
}

#[test]
fn map_install_failure_missing_brew_is_env_not_ready() {
    let details = serde_json::json!({
        "agent": null,
        "channel": "brew",
        "missing": ["git"],
        "remediations": [
            { "kind": "brew", "command": "brew install git", "url": "https://git-scm.com/downloads" }
        ],
        "hint": "未找到 Homebrew。请先安装 Homebrew（https://brew.sh/）后重试。"
    });
    let outcome = InstallOutcome::failure(
        "env_install",
        vec![],
        "未找到 Homebrew。请先安装 Homebrew（https://brew.sh/）后重试。",
    )
    .with_code("env.not_ready", Some(details));
    let err = map_install_failure(&outcome);
    assert_eq!(err.code(), "env.not_ready");
    assert_eq!(err.details()["channel"], "brew");
    assert_eq!(err.details()["missing"][0], "git");
    assert_eq!(err.details()["remediations"][0]["kind"], "brew");
}

#[test]
fn install_unsupported_channel_is_unsupported() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let err = install(&hub, "nodejs", "apt", OutputFormat::Quiet).unwrap_err();
    assert_eq!(err.code(), "unsupported");
}
