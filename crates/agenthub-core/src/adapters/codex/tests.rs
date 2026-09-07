use super::*;
use serde_json::json;

#[test]
fn extract_settings_openai_api_key_reads_auth_block() {
    let raw = json!({
        "format": "toml",
        "content": "model = \"gpt-5\"\n",
        "auth": { "OPENAI_API_KEY": "sk-test-key" }
    });
    assert_eq!(
        extract_settings_openai_api_key(&raw).as_deref(),
        Some("sk-test-key")
    );
    assert!(extract_settings_openai_api_key(&json!({"format":"toml","content":"x"})).is_none());
    assert!(extract_settings_openai_api_key(&json!({"auth":{"OPENAI_API_KEY":""}})).is_none());
}

#[test]
fn write_codex_api_key_auth_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.json");
    write_codex_api_key_auth(&path, "sk-from-pool").unwrap();
    let auth: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(auth["OPENAI_API_KEY"], "sk-from-pool");
    assert_eq!(
        read_live_openai_api_key(&path).unwrap().as_deref(),
        Some("sk-from-pool")
    );
}

#[test]
fn oauth_to_api_key_rollback_restores_auth_json_blob() {
    let _guard = crate::integrations::agents::codex::leftover::lock_codex_home();
    let dir = tempfile::tempdir().unwrap();
    let codex = dir.path().join(".codex");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(codex.join("config.toml"), "model = \"gpt-5\"\n").unwrap();
    let oauth_blob = r#"{
  "OPENAI_API_KEY": null,
  "auth_mode": "chatgpt",
  "tokens": {
    "access_token": "at-oauth-keep",
    "refresh_token": "rt-oauth-keep"
  }
}"#;
    std::fs::write(codex.join("auth.json"), oauth_blob).unwrap();
    let prev = std::env::var_os("CODEX_HOME");
    std::env::set_var("CODEX_HOME", &codex);

    let adapter = CodexAdapter;
    let live_before = adapter.read_config().expect("read oauth live");
    assert!(
        live_before.raw.get(LIVE_AUTH_FILE_KEY).is_some(),
        "snapshot must carry raw auth.json for rollback"
    );
    assert!(
        extract_settings_openai_api_key(&live_before.raw).is_none(),
        "oauth blob must stay out of the auth pool field"
    );

    let api_target = AgentConfig {
        agent: AgentId::Codex,
        raw: json!({
            "format": "toml",
            "content": "model = \"gpt-5\"\n",
            "auth": { "OPENAI_API_KEY": "sk-switch-target" }
        }),
    };
    adapter
        .write_config(&api_target)
        .expect("switch to api key");
    let overwritten: Value =
        serde_json::from_str(&std::fs::read_to_string(codex.join("auth.json")).unwrap()).unwrap();
    assert_eq!(overwritten["OPENAI_API_KEY"], "sk-switch-target");
    assert!(overwritten.get("tokens").is_none());

    // Compensation path used by switch_saga: write_config(&live_before).
    adapter
        .write_config(&live_before)
        .expect("rollback must restore oauth auth.json");
    let restored = std::fs::read_to_string(codex.join("auth.json")).unwrap();
    let restored_json: Value = serde_json::from_str(restored.trim()).unwrap();
    assert_eq!(
        restored_json["tokens"]["access_token"], "at-oauth-keep",
        "oauth tokens must come back after failed API-key switch compensation"
    );
    assert!(restored_json.get("OPENAI_API_KEY").unwrap().is_null());

    let mut scrubbed = live_before.raw.clone();
    strip_codex_live_auth_file(&mut scrubbed);
    assert!(
        scrubbed.get(LIVE_AUTH_FILE_KEY).is_none(),
        "pool backfill must strip the ephemeral auth file key"
    );

    match prev {
        Some(value) => std::env::set_var("CODEX_HOME", value),
        None => std::env::remove_var("CODEX_HOME"),
    }
}

#[test]
fn read_live_openai_api_key_ignores_oauth_null() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.json");
    std::fs::write(
        &path,
        r#"{ "OPENAI_API_KEY": null, "auth_mode": "chatgpt", "tokens": {} }"#,
    )
    .unwrap();
    assert!(read_live_openai_api_key(&path).unwrap().is_none());
}

#[test]
fn normalize_oauth_credentials_from_pkce_bundle() {
    let bundle = json!({
        "type": "oauth",
        "provider": "codex",
        "access_token": "at-1",
        "refresh_token": "rt-1",
        "id_token": "idt-1",
        "account_id": "acc-1",
        "email": "user@example.com",
        "expires_at": "2026-08-20T00:00:00+00:00",
        "raw": {
            "access_token": "at-1",
            "refresh_token": "rt-1",
            "id_token": "idt-1"
        }
    });
    let normalized = normalize_oauth_credentials(&bundle).unwrap();
    assert_eq!(
        normalized.get("format").and_then(|v| v.as_str()),
        Some("auth_json")
    );
    assert_eq!(
        normalized
            .pointer("/body/auth_mode")
            .and_then(|v| v.as_str()),
        Some("chatgpt")
    );
    assert!(normalized
        .pointer("/body/OPENAI_API_KEY")
        .unwrap()
        .is_null());
    assert_eq!(
        normalized
            .pointer("/body/tokens/access_token")
            .and_then(|v| v.as_str()),
        Some("at-1")
    );
    assert_eq!(
        normalized
            .pointer("/body/tokens/refresh_token")
            .and_then(|v| v.as_str()),
        Some("rt-1")
    );
    assert_eq!(
        normalized
            .pointer("/body/tokens/account_id")
            .and_then(|v| v.as_str()),
        Some("acc-1")
    );
    assert_eq!(
        normalized.get("email").and_then(|v| v.as_str()),
        Some("user@example.com")
    );
    assert_eq!(
        normalized.get("refresh_token").and_then(|v| v.as_str()),
        Some("rt-1")
    );
}

#[test]
fn normalize_oauth_credentials_keeps_valid_auth_json() {
    let already = json!({
        "format": "auth_json",
        "body": {
            "auth_mode": "chatgpt",
            "OPENAI_API_KEY": null,
            "tokens": {
                "access_token": "at-keep",
                "refresh_token": "rt-keep"
            },
            "last_refresh": "2026-08-01T00:00:00Z"
        },
        "email": "keep@example.com"
    });
    let normalized = normalize_oauth_credentials(&already).unwrap();
    assert_eq!(normalized, already);
}

#[test]
fn normalize_oauth_credentials_requires_access_token() {
    let err = normalize_oauth_credentials(&json!({
        "type": "oauth",
        "provider": "codex",
        "refresh_token": "rt-only"
    }))
    .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(err.to_string().contains("access_token"));
}

#[test]
fn apply_account_strips_leftover_bridge_keys() {
    let _guard = crate::integrations::agents::codex::leftover::lock_codex_home();
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let codex = home.join(".codex");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(
        codex.join("config.toml"),
        r#"model_provider = "agenthub_grok_bridge"
preferred_auth_method = "apikey"

[model_providers.agenthub_grok_bridge]
base_url = "http://127.0.0.1:43121/v1"
wire_api = "responses"
"#,
    )
    .unwrap();
    let prev = std::env::var_os("CODEX_HOME");
    std::env::set_var("CODEX_HOME", &codex);
    let account = LiveAccount {
        agent: AgentId::Codex,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "auth_mode": "chatgpt",
                "OPENAI_API_KEY": null,
                "tokens": {
                    "access_token": "at-official",
                    "refresh_token": "rt-official"
                },
                "last_refresh": "2026-08-20T00:00:00Z"
            },
            "email": "41375197@qq.com"
        }),
        label_hint: Some("41375197@qq.com".into()),
        extra: json!({}),
    };
    let result = CodexAdapter.apply_account(&account);
    match prev {
        Some(value) => std::env::set_var("CODEX_HOME", value),
        None => std::env::remove_var("CODEX_HOME"),
    }
    result.unwrap();
    let stored = std::fs::read_to_string(codex.join("config.toml")).unwrap();
    assert!(!stored.contains("agenthub_grok_bridge"));
    assert!(!stored.contains("preferred_auth_method"));
    assert!(!stored.contains("127.0.0.1"));
}

#[test]
fn apply_account_clears_openrouter_env_key_so_official_login_does_not_need_that_variable() {
    let _guard = crate::integrations::agents::codex::leftover::lock_codex_home();
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let codex = home.join(".codex");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(
        codex.join("config.toml"),
        r#"model_provider = "openrouter"
model = "stealth/ox-alpha"
preferred_auth_method = "apikey"

[model_providers.openrouter]
base_url = "https://openrouter.ai/api/v1"
env_key = "OPENROUTER_API_KEY"
wire_api = "responses"
"#,
    )
    .unwrap();
    let prev = std::env::var_os("CODEX_HOME");
    std::env::set_var("CODEX_HOME", &codex);
    let account = LiveAccount {
        agent: AgentId::Codex,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "auth_mode": "chatgpt",
                "tokens": {
                    "access_token": "at-official",
                    "refresh_token": "rt-official"
                }
            }
        }),
        label_hint: Some("41375197@qq.com".into()),
        extra: json!({}),
    };
    let result = CodexAdapter.apply_account(&account);
    match prev {
        Some(value) => std::env::set_var("CODEX_HOME", value),
        None => std::env::remove_var("CODEX_HOME"),
    }
    result.unwrap();
    let stored = std::fs::read_to_string(codex.join("config.toml")).unwrap();
    assert!(!stored.contains("model_provider ="), "{stored}");
    assert!(!stored.contains("stealth/ox-alpha"), "{stored}");
    assert!(!stored.contains("preferred_auth_method"), "{stored}");
    assert!(stored.contains("[model_providers.openrouter]"), "{stored}");
    let auth: Value =
        serde_json::from_str(&std::fs::read_to_string(codex.join("auth.json")).unwrap()).unwrap();
    assert_eq!(auth["tokens"]["access_token"], "at-official");
}
