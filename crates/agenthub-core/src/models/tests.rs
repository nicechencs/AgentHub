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
    assert_eq!(AgentId::parse("dsh"), Some(AgentId::Dsh));
    assert_eq!(AgentId::parse("  DSH  "), Some(AgentId::Dsh));
    assert_eq!(AgentId::parse("deepseek-harness"), Some(AgentId::Dsh));
    let expected = AgentId::expected_list();
    assert!(expected.contains("pi"));
    assert!(expected.contains("workbuddy"));
    assert!(expected.contains("cursor"));
    assert!(expected.contains("dsh"));
    assert_eq!(expected, "claude|codex|kimi|grok|pi|workbuddy|cursor|dsh");
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

#[test]
fn oauth_account_redacted_adds_refresh_token_preview() {
    let secret = "rt-abcdefghijklmnopqrstuvwxyz";
    let a = Account {
        id: "a1".into(),
        agent_id: AgentId::Codex,
        kind: AccountKind::Oauth,
        label: "c@x.com".into(),
        credentials: serde_json::json!({
            "format": "auth_json",
            "body": { "tokens": { "refresh_token": secret, "access_token": "at-secret" } }
        }),
        extra: serde_json::json!({ "email": "c@x.com" }),
        status: "active".into(),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t1".into(),
    };
    let r = a.redacted();
    assert_eq!(r.credentials["body"]["tokens"]["refresh_token"], "***");
    let preview = r.extra["refreshTokenPreview"].as_str().expect("preview");
    assert!(preview.contains("••••"));
    assert!(!preview.contains(secret));
    assert_eq!(preview, crate::utils::redact::mask_secret_preview(secret));
    assert_eq!(r.extra["secretTail"], "**wxyz");
    let dumped = serde_json::to_string(&r).unwrap();
    assert!(!dumped.contains(secret));

    let key = Account {
        kind: AccountKind::ApiKey,
        credentials: serde_json::json!({ "api_key": "sk-secret-value-here" }),
        extra: serde_json::json!({}),
        ..a
    };
    let redacted_key = key.redacted();
    assert!(redacted_key.extra.get("refreshTokenPreview").is_none());
    assert_eq!(redacted_key.extra["secretTail"], "**here");
}
