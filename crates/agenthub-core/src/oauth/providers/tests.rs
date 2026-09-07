use super::*;

#[test]
fn claude_authorize_url_contains_pkce_and_client() {
    let url = CLAUDE.build_authorize_url("http://127.0.0.1:12345/callback", "st", "ch");
    assert!(url.contains(&format!("client_id={}", CLAUDE.client_id)));
    assert!(url.contains("code_challenge=ch"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("code=true"));
}

#[test]
fn claude_loopback_redirect_uses_ipv4_callback() {
    assert_eq!(
        CLAUDE.loopback_redirect_uri(12345),
        "http://127.0.0.1:12345/callback"
    );
}

#[test]
fn codex_authorize_url_matches_registered_cli_loopback() {
    let redirect = CODEX.loopback_redirect_uri(1455);
    assert_eq!(redirect, "http://localhost:1455/auth/callback");
    let url = CODEX.build_authorize_url(&redirect, "st", "ch");
    assert!(url.contains(&format!("client_id={}", CODEX_CLIENT_ID)));
    assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback"));
    assert!(url.contains("id_token_add_organizations=true"));
    assert!(url.contains("codex_cli_simplified_flow=true"));
    assert!(url.contains("originator=codex_cli_rs"));
    assert!(url.contains("api.connectors.read"));
    assert!(!url.contains("127.0.0.1"));
    assert!(!url.contains("/callback&"));
}

#[test]
fn pi_openai_codex_shares_codex_authorize_registration() {
    let redirect = PI_OPENAI_CODEX.loopback_redirect_uri(1455);
    assert_eq!(redirect, CODEX.loopback_redirect_uri(1455));
    let url = PI_OPENAI_CODEX.build_authorize_url(&redirect, "st", "ch");
    assert!(url.contains("id_token_add_organizations=true"));
    assert!(url.contains("codex_cli_simplified_flow=true"));
    assert!(url.contains("originator=codex_cli_rs"));
}

#[test]
fn oauth_provider_for_known_agents() {
    assert!(oauth_provider_for(AgentId::Claude).is_some());
    assert!(oauth_provider_for(AgentId::Codex).is_some());
    assert!(oauth_provider_for(AgentId::Grok).is_some());
    assert!(oauth_provider_for(AgentId::Kimi).is_none());
}

#[test]
fn bundle_from_token_json_sets_email_label_from_claude_account() {
    let body = json!({
        "access_token": "at-1",
        "refresh_token": "rt-1",
        "expires_in": 3600,
        "account": {
            "uuid": "acct-1",
            "email_address": "me@anthropic.test"
        },
        "organization": { "uuid": "org-1" }
    });
    let bundle = CLAUDE.bundle_from_token_json(body).expect("bundle");
    assert_eq!(bundle.label_hint.as_deref(), Some("me@anthropic.test"));
    assert_eq!(
        bundle.credentials.get("email").and_then(|v| v.as_str()),
        Some("me@anthropic.test")
    );
    assert_eq!(
        bundle.extra.get("email").and_then(|v| v.as_str()),
        Some("me@anthropic.test")
    );
    assert_eq!(
        bundle.extra.get("identityLabel").and_then(|v| v.as_str()),
        Some("me@anthropic.test")
    );
    assert_eq!(
        bundle
            .credentials
            .get("organization_id")
            .and_then(|v| v.as_str()),
        Some("org-1")
    );
}

#[test]
fn bundle_from_token_json_fallback_label_when_no_identity() {
    let body = json!({
        "access_token": "opaque-token",
        "refresh_token": "rt",
        "expires_in": 60
    });
    let bundle = XAI.bundle_from_token_json(body).expect("bundle");
    assert_eq!(bundle.label_hint.as_deref(), Some("Grok · OAuth"));
    assert!(bundle.credentials.get("email").is_none());
}
