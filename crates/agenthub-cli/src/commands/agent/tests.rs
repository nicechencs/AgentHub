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
