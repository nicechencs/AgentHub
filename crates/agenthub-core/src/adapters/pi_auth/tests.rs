use super::*;

#[test]
fn expand_splits_multi_provider_auth() {
    let body = json!({
        "xai": { "type": "oauth", "access": "a1", "refresh": "r1", "expires": 1785682457104i64 },
        "anthropic": { "type": "oauth", "access": "a2", "refresh": "r2", "expires": 1785682457104i64 },
        "openai": { "type": "api_key", "key": "sk-test-key-123456" }
    });
    let accounts = expand_auth_to_live_accounts(&body).unwrap();
    assert_eq!(accounts.len(), 3);
    assert!(accounts
        .iter()
        .any(|a| { a.credentials.get("provider").and_then(|v| v.as_str()) == Some("xai") }));
    assert!(accounts
        .iter()
        .any(|a| { a.credentials.get("provider").and_then(|v| v.as_str()) == Some("anthropic") }));
    let openai = accounts
        .iter()
        .find(|a| a.credentials.get("provider").and_then(|v| v.as_str()) == Some("openai"))
        .unwrap();
    assert_eq!(openai.kind, AccountKind::ApiKey);
}

#[test]
fn oauth_entry_shape_matches_pi() {
    let entry = pi_oauth_entry_from_tokens("at", Some("rt"), None, Some(3600));
    assert_eq!(entry.get("type").and_then(|v| v.as_str()), Some("oauth"));
    assert_eq!(entry.get("access").and_then(|v| v.as_str()), Some("at"));
    assert_eq!(entry.get("refresh").and_then(|v| v.as_str()), Some("rt"));
    assert!(entry.get("expires").and_then(|v| v.as_i64()).unwrap() > 0);
}

#[test]
fn live_account_flattens_refresh_for_auth_key() {
    let entry = json!({
        "type": "oauth",
        "access": "acc",
        "refresh": "ref-token-xyz",
        "expires": 1785682457104i64
    });
    let live = live_account_for_provider("xai", &entry).unwrap();
    assert_eq!(
        live.credentials
            .get("refresh_token")
            .and_then(|v| v.as_str()),
        Some("ref-token-xyz")
    );
    assert_eq!(
        live.credentials.get("provider").and_then(|v| v.as_str()),
        Some("xai")
    );
    assert!(live.label_hint.unwrap().contains("xai"));
}

fn make_jwt(claims: Value) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(claims.to_string().as_bytes());
    format!("{header}.{payload}.sig")
}

#[test]
fn live_account_from_openai_codex_keeps_chatgpt_account_id() {
    let id_token = make_jwt(json!({
        "email": "pi-codex@example.com",
        "https://api.openai.com/auth": {
            "chatgpt_account_id": "acc-pi",
            "chatgpt_plan_type": "plus"
        }
    }));
    let live = live_account_from_oauth_tokens(
        "openai-codex",
        "opaque-access",
        Some("rt"),
        None,
        Some(3600),
        Some(&id_token),
    )
    .unwrap();
    assert_eq!(
        live.credentials.get("account_id").and_then(|v| v.as_str()),
        Some("acc-pi")
    );
    assert_eq!(
        live.credentials.get("id_token").and_then(|v| v.as_str()),
        Some(id_token.as_str())
    );
    assert_eq!(
        live.extra.get("accountId").and_then(|v| v.as_str()),
        Some("acc-pi")
    );
    assert_eq!(
        live.extra.get("subscription").and_then(|v| v.as_str()),
        Some("plus")
    );
    assert_eq!(
        live.credentials
            .pointer("/body/openai-codex/id_token")
            .and_then(|v| v.as_str()),
        None,
        "id_token must stay on the pool row, not Pi auth.json body"
    );
}
