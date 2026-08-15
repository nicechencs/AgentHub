use super::*;

#[test]
fn lifecycle_key_parser_accepts_open_keys_and_legacy_case() {
    assert_eq!(
        parse_lifecycle_agent_key("demo-agent").unwrap().as_str(),
        "demo-agent"
    );
    assert_eq!(
        parse_lifecycle_agent_key("  CODEX  ").unwrap().as_str(),
        "codex"
    );
    assert!(parse_lifecycle_agent_key("Future-Agent").is_err());
}

#[test]
fn uninstall_purge_without_yes_requires_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let err = uninstall(&hub, "codex", true, false, OutputFormat::Quiet).unwrap_err();
    assert_eq!(err.code(), "confirmation_required");
}

#[test]
fn capabilities_markdown_only_for_table() {
    assert!(!should_print_capabilities_markdown(
        true,
        OutputFormat::Quiet
    ));
    assert!(!should_print_capabilities_markdown(
        true,
        OutputFormat::Json
    ));
    assert!(should_print_capabilities_markdown(
        true,
        OutputFormat::Table
    ));
    assert!(!should_print_capabilities_markdown(
        false,
        OutputFormat::Table
    ));
}

#[test]
fn capabilities_quiet_does_not_error() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    capabilities(&hub, OutputFormat::Quiet, None, true).unwrap();
    capabilities(&hub, OutputFormat::Quiet, None, false).unwrap();
}
