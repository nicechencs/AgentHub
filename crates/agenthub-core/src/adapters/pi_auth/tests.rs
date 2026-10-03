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

#[test]
fn remove_auth_entry_only_when_unchanged_and_keeps_other_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.json");
    let body = json!({
        "anthropic": { "type": "oauth", "access": "a", "refresh": "r", "future": [1, 2] },
        "xai": { "type": "oauth", "access": "x", "refresh": "xr" },
        "openai": { "type": "api_key", "key": "sk-fake-openai-0000" }
    });
    write_verified_auth_json(&path, &body).unwrap();

    let other = json!({ "type": "oauth", "access": "x2", "refresh": "xr2" });
    assert!(!remove_auth_entry_if_unchanged(&path, "xai", &other).unwrap());
    assert!(!remove_auth_entry_if_unchanged(&path, "missing", &other).unwrap());
    assert_eq!(read_auth_json_file(&path).unwrap(), body);

    assert!(remove_auth_entry_if_unchanged(&path, "xai", &body["xai"]).unwrap());
    let after = read_auth_json_file(&path).unwrap();
    let keys: Vec<&String> = after.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["anthropic", "openai"], "order kept");
    assert_eq!(after["anthropic"], body["anthropic"]);
    assert_eq!(after["openai"], body["openai"]);
}

#[test]
fn restore_auth_entry_only_fills_a_missing_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.json");
    let body = json!({
        "anthropic": { "type": "oauth", "access": "a", "refresh": "r" },
        "xai": { "type": "oauth", "access": "x-new", "refresh": "xr-new" }
    });
    write_verified_auth_json(&path, &body).unwrap();
    let old_xai = json!({ "type": "oauth", "access": "x", "refresh": "xr" });

    assert!(!restore_auth_entry_if_missing(&path, "xai", &old_xai).unwrap());
    assert_eq!(
        read_auth_json_file(&path).unwrap(),
        body,
        "newer entry kept"
    );

    let deepseek = json!({ "type": "api_key", "key": "sk-fake-deepseek-0000" });
    assert!(restore_auth_entry_if_missing(&path, "deepseek", &deepseek).unwrap());
    let after = read_auth_json_file(&path).unwrap();
    assert_eq!(after["deepseek"], deepseek);
    assert_eq!(after["anthropic"], body["anthropic"]);
    assert_eq!(after["xai"], body["xai"]);
}
