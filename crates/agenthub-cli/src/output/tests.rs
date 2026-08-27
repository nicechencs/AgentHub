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

    let guide = InstallOutcome::failure(
        "agent_install",
        vec![],
        "workbuddy 已打开官网安装页，请完成安装后重启 AgentHub",
    )
    .with_code("setup_guide", None);
    assert_eq!(map_install_failure(&guide).code(), "setup_guide");
    assert!(!guide.message.contains("失败"));
}

#[test]
fn emit_install_outcome_quiet_keeps_map_behavior() {
    let ok = InstallOutcome {
        ok: true,
        action: "agent_install".into(),
        logs: vec!["should-not-leak-on-quiet".into()],
        message: "installed".into(),
        ..InstallOutcome::default()
    };
    emit_install_outcome(&ok, OutputFormat::Quiet).unwrap();

    let env = InstallOutcome::failure(
        "agent_install",
        vec!["should-not-leak-on-quiet".into()],
        "环境未就绪",
    )
    .with_code(
        "env.not_ready",
        Some(serde_json::json!({"agent":"codex","missing":["nodejs"]})),
    );
    let err = emit_install_outcome(&env, OutputFormat::Quiet).unwrap_err();
    assert_eq!(err.code(), map_install_failure(&env).code());
    assert_eq!(err.details()["agent"], "codex");

    let unsupported = InstallOutcome::failure("env_install", vec!["log".into()], "no powershell")
        .with_code("unsupported", None);
    assert_eq!(
        emit_install_outcome(&unsupported, OutputFormat::Quiet)
            .unwrap_err()
            .code(),
        map_install_failure(&unsupported).code()
    );

    let other = InstallOutcome::failure("agent_install", vec!["log".into()], "npm failed");
    assert_eq!(
        emit_install_outcome(&other, OutputFormat::Quiet)
            .unwrap_err()
            .code(),
        map_install_failure(&other).code()
    );
}
