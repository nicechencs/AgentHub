use super::*;

#[test]
fn agent_id_parse_as_str_roundtrip() {
    for id in AgentId::ALL {
        let s = id.as_str();
        assert_eq!(AgentId::parse(s), Some(id));
    }
    assert_eq!(AgentId::parse("Claude"), Some(AgentId::Claude));
    assert_eq!(AgentId::parse("  CODEx  "), Some(AgentId::Codex));
    assert_eq!(AgentId::parse("kimi"), Some(AgentId::Kimi));
    assert_eq!(AgentId::parse("grok"), Some(AgentId::Grok));
    assert_eq!(AgentId::parse("pi"), Some(AgentId::Pi));
    assert_eq!(AgentId::parse("  PI  "), Some(AgentId::Pi));
    assert_eq!(AgentId::parse("workbuddy"), Some(AgentId::WorkBuddy));
    assert_eq!(AgentId::parse("  WorkBuddy  "), Some(AgentId::WorkBuddy));
    assert_eq!(AgentId::parse("cursor"), Some(AgentId::Cursor));
    assert_eq!(AgentId::parse("  Cursor  "), Some(AgentId::Cursor));
    assert_eq!(AgentId::parse("cursor-agent"), Some(AgentId::Cursor));
    let expected = AgentId::expected_list();
    assert!(expected.contains("pi"));
    assert!(expected.contains("workbuddy"));
    assert!(expected.contains("cursor"));
    assert_eq!(expected, "claude|codex|kimi|grok|pi|workbuddy|cursor");
}

#[test]
fn agent_id_parse_rejects_invalid() {
    assert_eq!(AgentId::parse(""), None);
    assert_eq!(AgentId::parse("unknown"), None);
    assert_eq!(AgentId::parse("claude-code"), None);
    assert_eq!(AgentId::parse("gpt"), None);
}

#[test]
fn agent_id_parse_required_and_optional() {
    assert_eq!(AgentId::parse_required("GROK").unwrap(), AgentId::Grok);
    assert_eq!(AgentId::parse_optional(None).unwrap(), None);
    assert_eq!(AgentId::parse_optional(Some("")).unwrap(), None);
    assert_eq!(
        AgentId::parse_optional(Some("  claude  ")).unwrap(),
        Some(AgentId::Claude)
    );
    let err = AgentId::parse_required("not-an-agent").unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(err.to_string().contains("expected:"));
}

#[test]
fn auth_state_legacy_json_defaults_new_fields() {
    let state: AuthState = serde_json::from_value(serde_json::json!({
        "agent": "claude",
        "kind": "oauth",
        "summary": "legacy",
        "hasCredentials": true,
        "revision": "legacy-revision"
    }))
    .unwrap();
    assert_eq!(state.health, AuthHealth::Unknown);
    assert_eq!(state.source, None);
    assert_eq!(state.revision.as_deref(), Some("legacy-revision"));
}
