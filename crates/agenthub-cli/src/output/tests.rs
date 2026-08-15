use super::*;
use agenthub_core::models::InstallOutcome;

#[test]
fn json_and_quiet_errors_do_not_expose_arbitrary_messages() {
    let error = AppError::message("provider.switch.apply", "secret=sk-sensitive");

    let json = render_error(&error, OutputFormat::Json).unwrap();
    assert!(!json.contains("sk-sensitive"));
    assert!(json.contains("provider.switch.apply"));
    assert_eq!(render_error(&error, OutputFormat::Quiet), None);
    let table = render_error(&error, OutputFormat::Table).unwrap();
    assert!(!table.contains("sk-sensitive"));
    assert!(table.contains("sk-***") || table.contains("***"));
    assert!(table.contains("provider.switch.apply"));
}

#[test]
fn json_error_includes_env_not_ready_details() {
    let details = serde_json::json!({
        "agent": "codex",
        "channel": "npm",
        "missing": ["nodejs"],
        "hint": "re-run with --install-deps"
    });
    let error = AppError::EnvNotReady(details.to_string());
    let json = render_error(&error, OutputFormat::Json).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["code"], "env.not_ready");
    assert_eq!(value["details"]["agent"], "codex");
    assert_eq!(value["details"]["channel"], "npm");
    assert_eq!(value["details"]["missing"][0], "nodejs");
}

#[test]
fn assume_yes_skips_terminal_prompt() {
    confirm("provider write", true).unwrap();
}

#[test]
fn map_install_failure_uses_structured_codes() {
    let env = InstallOutcome::failure("agent_install", vec![], "环境未就绪").with_code(
        "env.not_ready",
        Some(serde_json::json!({"agent":"codex","missing":["nodejs"]})),
    );
    let err = map_install_failure(&env);
    assert_eq!(err.code(), "env.not_ready");
    assert_eq!(err.details()["agent"], "codex");

    let unsupported = InstallOutcome::failure("env_install", vec![], "no powershell")
        .with_code("unsupported", None);
    assert_eq!(map_install_failure(&unsupported).code(), "unsupported");

    let other = InstallOutcome::failure("agent_install", vec![], "npm failed");
    assert_eq!(map_install_failure(&other).code(), "install.failed");
}
