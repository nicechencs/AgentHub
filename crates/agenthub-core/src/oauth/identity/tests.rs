use super::*;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

fn make_jwt(claims: Value) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(claims.to_string().as_bytes());
    format!("{header}.{payload}.sig")
}

#[test]
fn decode_jwt_payload_reads_claims() {
    let jwt = make_jwt(json!({"email": "a@example.com", "sub": "user-1"}));
    let claims = decode_jwt_payload(&jwt).expect("payload");
    assert_eq!(claims["email"], "a@example.com");
    assert_eq!(claims["sub"], "user-1");
}

#[test]
fn extract_codex_from_id_token() {
    let id_token = make_jwt(json!({
        "email": "codex@example.com",
        "sub": "user-xyz",
        "https://api.openai.com/auth": {
            "chatgpt_account_id": "acc-1",
            "chatgpt_plan_type": "plus",
            "user_id": "user-xyz",
            "organizations": [
                {"id": "org-default", "is_default": true, "title": "Personal"}
            ]
        }
    }));
    let body = json!({
        "access_token": "opaque",
        "refresh_token": "rt",
        "id_token": id_token,
        "expires_in": 3600
    });
    let id = extract_oauth_identity(
        "codex",
        &body,
        body.get("access_token").and_then(|v| v.as_str()),
        body.get("id_token").and_then(|v| v.as_str()),
    );
    assert_eq!(id.email.as_deref(), Some("codex@example.com"));
    assert_eq!(id.account_id.as_deref(), Some("acc-1"));
    assert_eq!(id.subscription.as_deref(), Some("plus"));
    assert_eq!(id.organization_id.as_deref(), Some("org-default"));
    assert_eq!(id.display_label().as_deref(), Some("codex@example.com"));
}

#[test]
fn extract_pi_openai_codex_from_id_token() {
    let id_token = make_jwt(json!({
        "email": "pi-codex@example.com",
        "https://api.openai.com/auth": {
            "chatgpt_account_id": "acc-pi",
            "chatgpt_plan_type": "plus"
        }
    }));
    let body = json!({
        "access_token": "opaque",
        "id_token": id_token,
    });
    for provider in ["openai-codex", "pi-openai-codex"] {
        let id = extract_oauth_identity(
            provider,
            &body,
            body.get("access_token").and_then(|v| v.as_str()),
            body.get("id_token").and_then(|v| v.as_str()),
        );
        assert_eq!(
            id.email.as_deref(),
            Some("pi-codex@example.com"),
            "{provider}"
        );
        assert_eq!(id.account_id.as_deref(), Some("acc-pi"), "{provider}");
        assert_eq!(id.subscription.as_deref(), Some("plus"), "{provider}");
    }
    assert_eq!(
        chatgpt_account_id_from_token(&id_token).as_deref(),
        Some("acc-pi")
    );
    assert!(is_openai_codex_identity_provider("openai-codex"));
    assert!(is_openai_codex_identity_provider("pi-openai-codex"));
    assert!(!is_openai_codex_identity_provider("anthropic"));
}

#[test]
fn extract_claude_from_nested_account() {
    let body = json!({
        "access_token": "at",
        "refresh_token": "rt",
        "account": {
            "uuid": "acct-uuid",
            "email_address": "claude@example.com"
        },
        "organization": { "uuid": "org-uuid" }
    });
    let id = extract_oauth_identity("claude", &body, Some("at"), None);
    assert_eq!(id.email.as_deref(), Some("claude@example.com"));
    assert_eq!(id.account_id.as_deref(), Some("acct-uuid"));
    assert_eq!(id.organization_id.as_deref(), Some("org-uuid"));
}

#[test]
fn extract_grok_from_access_token_jwt() {
    let access = make_jwt(json!({
        "email": "grok@example.com",
        "sub": "grok-sub",
        "team_id": "team-1"
    }));
    let body = json!({
        "access_token": access,
        "refresh_token": "rt",
        "token_type": "Bearer"
    });
    let id = extract_oauth_identity(
        "xai",
        &body,
        body.get("access_token").and_then(|v| v.as_str()),
        None,
    );
    assert_eq!(id.email.as_deref(), Some("grok@example.com"));
    assert_eq!(id.subject.as_deref(), Some("grok-sub"));
    assert_eq!(id.account_id.as_deref(), Some("team-1"));
}

#[test]
fn apply_identity_writes_credentials_fields() {
    let mut map = Map::new();
    map.insert("access_token".into(), json!("at"));
    let id = OAuthIdentity {
        email: Some("u@x.com".into()),
        subject: Some("sub".into()),
        account_id: Some("a1".into()),
        organization_id: Some("o1".into()),
        subscription: Some("plus".into()),
    };
    apply_identity_to_credentials(&mut map, &id);
    assert_eq!(map.get("email").and_then(|v| v.as_str()), Some("u@x.com"));
    assert_eq!(map.get("sub").and_then(|v| v.as_str()), Some("sub"));
    assert_eq!(
        map.get("organization_id").and_then(|v| v.as_str()),
        Some("o1")
    );
    assert_eq!(map.get("plan_type").and_then(|v| v.as_str()), Some("plus"));
}

#[test]
fn merge_missing_preserves_existing() {
    let mut a = OAuthIdentity {
        email: Some("old@x.com".into()),
        subject: None,
        ..Default::default()
    };
    let b = OAuthIdentity {
        email: Some("new@x.com".into()),
        subject: Some("s".into()),
        ..Default::default()
    };
    a.merge_missing(&b);
    assert_eq!(a.email.as_deref(), Some("old@x.com"));
    assert_eq!(a.subject.as_deref(), Some("s"));
}
