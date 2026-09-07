use super::*;
use crate::models::AccountKind;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::json;

fn make_jwt(claims: Value) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
    let payload = URL_SAFE_NO_PAD.encode(claims.to_string().as_bytes());
    format!("{header}.{payload}.sig")
}

fn base_account(agent: AgentId, label: &str, credentials: Value, extra: Value) -> Account {
    Account {
        id: format!("{agent}-1"),
        agent_id: agent,
        kind: AccountKind::Oauth,
        label: label.into(),
        credentials,
        extra,
        status: "active".into(),
        is_current: true,
        created_at: "t".into(),
        updated_at: "t".into(),
    }
}

#[test]
fn heals_codex_legacy_pkce_bundle_into_auth_json() {
    let mut acc = base_account(
        AgentId::Codex,
        "codex-oauth",
        json!({
            "type": "oauth",
            "provider": "codex",
            "access_token": "at-legacy",
            "refresh_token": "rt-legacy",
            "id_token": "idt-legacy",
            "account_id": "acc-legacy",
            "email": "legacy@example.com"
        }),
        json!({ "source": "oauth_pkce" }),
    );
    assert!(heal_account_identity(&mut acc));
    assert_eq!(
        acc.credentials.get("format").and_then(|v| v.as_str()),
        Some("auth_json")
    );
    assert_eq!(
        acc.credentials
            .pointer("/body/tokens/access_token")
            .and_then(|v| v.as_str()),
        Some("at-legacy")
    );
    assert_eq!(
        acc.credentials
            .pointer("/body/tokens/refresh_token")
            .and_then(|v| v.as_str()),
        Some("rt-legacy")
    );
    assert_eq!(
        acc.credentials.get("email").and_then(|v| v.as_str()),
        Some("legacy@example.com")
    );
}

#[test]
fn heals_codex_tokens_id_token_email_and_plan() {
    let exp = chrono::Utc::now().timestamp() + 6 * 3600;
    let id_token = make_jwt(json!({
        "email": "41375197@qq.com",
        "sub": "google-oauth2|123",
        "exp": exp,
        "https://api.openai.com/auth": {
            "chatgpt_account_id": "fcf2a4f8-bbff-4598-910d-067e947e229c",
            "chatgpt_plan_type": "prolite",
            "user_id": "user-x"
        }
    }));
    let access = make_jwt(json!({ "sub": "user-x", "exp": exp }));
    let mut acc = base_account(
        AgentId::Codex,
        "codex-oauth",
        json!({
            "format": "auth_json",
            "body": {
                "tokens": {
                    "id_token": id_token,
                    "access_token": access,
                    "refresh_token": "rt",
                    "account_id": "fcf2a4f8-bbff-4598-910d-067e947e229c"
                }
            }
        }),
        // Bad prior identity: account UUID mistaken for identity.
        json!({
            "source": "live",
            "identityLabel": "fcf2a4f8-bbff-4598-910d-067e947e229c"
        }),
    );
    assert!(needs_identity_heal(&acc));
    assert!(heal_account_identity(&mut acc));
    assert_eq!(acc.label, "41375197@qq.com");
    assert_eq!(
        acc.extra.get("email").and_then(|v| v.as_str()),
        Some("41375197@qq.com")
    );
    assert_eq!(
        acc.extra.get("identityLabel").and_then(|v| v.as_str()),
        Some("41375197@qq.com")
    );
    assert_eq!(
        acc.extra.get("subscription").and_then(|v| v.as_str()),
        Some("prolite")
    );
    // JWT exp should surface as expiresAt so the UI can show remaining time.
    assert!(acc
        .extra
        .get("expiresAt")
        .and_then(|v| v.as_str())
        .is_some());
    assert_eq!(
        acc.extra.get("tokenExpired").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert!(!needs_identity_heal(&acc));
}

#[test]
fn upgrades_grok_placeholder_label_when_email_already_in_extra() {
    let mut acc = base_account(
        AgentId::Grok,
        "grok-oauth",
        json!({
            "format": "auth_json",
            "body": {
                "https://auth.x.ai::client": {
                    "email": "user@example.com",
                    "user_id": "u-1",
                    "refresh_token": "rt"
                }
            }
        }),
        json!({
            "source": "live",
            "email": "user@example.com",
            "identityLabel": "user@example.com"
        }),
    );
    // Even with email present, placeholder label must be upgraded.
    assert!(needs_identity_heal(&acc));
    assert!(heal_account_identity(&mut acc));
    assert_eq!(acc.label, "user@example.com");
    assert!(!needs_identity_heal(&acc));
}

#[test]
fn heals_pi_auth_json_blob_from_xai_access_jwt() {
    let access = make_jwt(json!({
        "sub": "36b45542-a4c3-4a5d-b4d9-1c685d10dcd9",
        "tier": 5,
        "team_id": "team-1"
    }));
    let mut acc = base_account(
        AgentId::Pi,
        "pi:xai (oauth)",
        json!({
            "format": "auth_json",
            "body": {
                "xai": {
                    "type": "oauth",
                    "access": access,
                    "refresh": "rt",
                    "expires": 1785682457104i64
                }
            }
        }),
        json!({"source":"live","identityLabel":"pi:xai (oauth)"}),
    );
    assert!(heal_account_identity(&mut acc));
    assert!(acc.label.contains("xai"));
    assert_eq!(
        acc.extra.get("subscription").and_then(|v| v.as_str()),
        Some("tier 5")
    );
    assert_eq!(
        acc.extra.get("provider").and_then(|v| v.as_str()),
        Some("xai")
    );
}
