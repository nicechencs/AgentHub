use super::*;
use serde_json::json;

#[test]
fn account_kind_parse_and_serde() {
    assert_eq!(AccountKind::parse("oauth"), Some(AccountKind::Oauth));
    assert_eq!(AccountKind::parse("APIKEY"), Some(AccountKind::ApiKey));
    assert_eq!(AccountKind::parse("api_key"), Some(AccountKind::ApiKey));
    assert_eq!(AccountKind::parse("nope"), None);
    assert_eq!(
        serde_json::to_string(&AccountKind::ApiKey).unwrap(),
        "\"apikey\""
    );
    assert_eq!(
        serde_json::from_str::<AccountKind>("\"oauth\"").unwrap(),
        AccountKind::Oauth
    );
}

#[test]
fn account_redacted_masks_credentials() {
    let a = Account {
        id: "a1".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::ApiKey,
        label: "xai-••••e41d".into(),
        credentials: json!({"format": "api_key", "api_key": "xai-secret-value"}),
        extra: json!({"token": "t", "note": "ok"}),
        status: "active".into(),
        is_current: true,
        created_at: "t0".into(),
        updated_at: "t1".into(),
    };
    let r = a.redacted();
    assert_eq!(r.credentials["api_key"], "***");
    assert_eq!(r.extra["token"], "***");
    assert_eq!(r.extra["note"], "ok");
    assert_eq!(r.extra["secretTail"], "**alue");
    assert_eq!(a.credentials["api_key"], "xai-secret-value");
}

#[test]
fn account_redacted_recovers_secret_tail_from_stored_identity_label() {
    let a = Account {
        id: "grok-old".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::ApiKey,
        label: "API Key".into(),
        credentials: json!({"format": "api_key", "api_key": "***"}),
        extra: json!({ "identityLabel": "xai-••••8660 (API Key)" }),
        status: "active".into(),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t1".into(),
    };
    let r = a.redacted();
    assert_eq!(r.credentials["api_key"], "***");
    assert_eq!(r.extra["secretTail"], "**8660");
    assert_eq!(r.extra["identityLabel"], "xai-••••8660 (API Key)");
}

#[test]
fn account_redacted_does_not_invent_tail_from_kind_name() {
    let a = Account {
        id: "grok-empty".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::ApiKey,
        label: "API Key".into(),
        credentials: json!({"format": "api_key", "api_key": "***"}),
        extra: json!({ "identityLabel": "API Key" }),
        status: "active".into(),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t1".into(),
    };
    let r = a.redacted();
    assert!(r.extra.get("secretTail").is_none());
}

fn sample_account() -> Account {
    Account {
        id: "a1".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::Oauth,
        label: "grok-oauth".into(),
        credentials: json!({
            "format": "auth_json",
            "provider": "xai",
            "email": "creds@example.com",
            "env_key": "XAI_API_KEY",
        }),
        extra: json!({
            "source": "oauth_pkce",
            "provider": "xai",
            "email": "extra@example.com",
            "identityLabel": "Nice",
            "subscription": "SuperGrok",
            "health": "configured",
            "authHealth": "verified",
            "authSource": "live",
            "liveRevision": "r1",
            "tokenExpired": false,
            "expiresAt": "2026-09-01T00:00:00Z",
            "quota5hPct": 40,
            "quota7dPct": 80,
            "quotaResetIn": "2h00m 后重置",
            "keepUnknown": true,
        }),
        status: "active".into(),
        is_current: true,
        created_at: "t0".into(),
        updated_at: "t1".into(),
    }
}

#[test]
fn known_json_keys_are_read_through_accessors() {
    let a = sample_account();
    assert_eq!(a.source(), Some("oauth_pkce"));
    assert_eq!(a.extra_provider(), Some("xai"));
    assert_eq!(a.extra_email(), Some("extra@example.com"));
    assert_eq!(a.identity_label(), Some("Nice"));
    assert_eq!(a.subscription(), Some("SuperGrok"));
    assert_eq!(a.extra_health(), Some("configured"));
    assert_eq!(a.extra_auth_health(), Some("verified"));
    assert_eq!(a.extra_auth_source(), Some("live"));
    assert_eq!(a.extra_live_revision(), Some("r1"));
    assert_eq!(a.token_expired(), Some(false));
    assert_eq!(a.extra_expires_at(), Some("2026-09-01T00:00:00Z"));
    assert_eq!(a.quota_5h_pct(), Some(40));
    assert_eq!(a.quota_7d_pct(), Some(80));
    assert_eq!(a.quota_reset_in(), Some("2h00m 后重置"));
    assert_eq!(a.credential_format(), Some("auth_json"));
    assert_eq!(a.credential_provider(), Some("xai"));
    assert_eq!(a.credential_email(), Some("creds@example.com"));
    assert_eq!(a.credential_env_key(), Some("XAI_API_KEY"));
    assert_eq!(a.extra["keepUnknown"], true);
}

#[test]
fn missing_json_keys_are_none_and_unknown_keys_stay_on_value() {
    let a = Account {
        id: "a2".into(),
        agent_id: AgentId::Claude,
        kind: AccountKind::ApiKey,
        label: "key".into(),
        credentials: json!({"api_key": "sk"}),
        extra: json!({"custom": 1}),
        status: "active".into(),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t1".into(),
    };
    assert_eq!(a.source(), None);
    assert_eq!(a.extra_provider(), None);
    assert_eq!(a.identity_label(), None);
    assert_eq!(a.token_expired(), None);
    assert_eq!(a.quota_5h_pct(), None);
    assert_eq!(a.credential_format(), None);
    assert_eq!(a.credential_env_key(), None);
    assert_eq!(a.extra["custom"], 1);
    assert_eq!(a.credentials["api_key"], "sk");
}
