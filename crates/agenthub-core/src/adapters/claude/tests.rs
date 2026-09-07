use super::*;
use serde_json::json;

#[test]
fn parse_claude_oauth_accepts_camel_and_dotted_keys() {
    let a = parse_claude_oauth_json(
        r#"{"claudeAiOauth":{"accessToken":"tok-aaa","expiresAt":9999999999}}"#,
        ClaudeOauthSource::CredentialsFile,
    )
    .expect("camel key");
    assert_eq!(a.access_token, "tok-aaa");
    assert!(!a.expired);
    assert_eq!(a.source, ClaudeOauthSource::CredentialsFile);

    let b = parse_claude_oauth_json(
        r#"{"claude.ai_oauth":{"accessToken":"tok-bbb","expiresAt":1}}"#,
        ClaudeOauthSource::CredentialsFile,
    )
    .expect("dotted key");
    assert_eq!(b.access_token, "tok-bbb");
    assert!(b.expired);
}

#[test]
fn parse_claude_oauth_rejects_missing_or_empty_token() {
    assert!(
        parse_claude_oauth_json(r#"{"mcpOAuth":{}}"#, ClaudeOauthSource::CredentialsFile).is_none()
    );
    assert!(parse_claude_oauth_json(
        r#"{"claudeAiOauth":{"accessToken":""}}"#,
        ClaudeOauthSource::CredentialsFile
    )
    .is_none());
}

#[test]
fn is_token_expired_handles_millis_and_iso() {
    assert_eq!(is_expired(&json!(1)), Some(true));
    assert_eq!(is_expired(&json!(9_999_999_999_u64)), Some(false));
    // small epoch numbers are seconds (year 1970), not millis
    assert_eq!(is_expired(&json!(1_000_u64)), Some(true));
    assert_eq!(is_expired(&json!("2099-01-01T00:00:00.000Z")), Some(false));
    assert_eq!(is_expired(&json!("2000-01-01T00:00:00Z")), Some(true));
}

#[test]
fn read_account_persists_loopback_base_url_from_settings() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "sk-bridge",
    "ANTHROPIC_BASE_URL": "http://127.0.0.1:43081"
  }
}
"#,
    )
    .expect("write");
    let account = read_settings_api_key_account(&path)
        .expect("read")
        .expect("api key account");
    assert_eq!(account.agent, AgentId::Claude);
    assert_eq!(account.kind, AccountKind::ApiKey);
    assert_eq!(account.credentials["format"], "api_key");
    assert_eq!(account.credentials["api_key"], "sk-bridge");
    assert_eq!(account.credentials["env_key"], "ANTHROPIC_AUTH_TOKEN");
    assert_eq!(account.credentials["base_url"], "http://127.0.0.1:43081");
}

#[test]
fn write_claude_settings_token_restores_base_url_when_present() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("settings.json");
    write_claude_settings_token(
        &path,
        "ANTHROPIC_AUTH_TOKEN",
        "sk-bridge",
        Some("http://127.0.0.1:43081"),
    )
    .expect("write");
    let text = std::fs::read_to_string(&path).expect("read");
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-bridge");
    assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:43081");

    write_claude_settings_token(&path, "ANTHROPIC_AUTH_TOKEN", "sk-plain", None)
        .expect("write without url");
    let text = std::fs::read_to_string(&path).expect("read");
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-plain");
    assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:43081");
}

#[test]
fn clear_claude_settings_api_auth_removes_token_and_base_url() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{
  "model": "sonnet",
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "sk-test",
    "ANTHROPIC_BASE_URL": "https://relay.example.com",
    "OTHER": "keep"
  }
}
"#,
    )
    .expect("write");
    clear_claude_settings_api_auth(&path).expect("clear");
    let text = std::fs::read_to_string(&path).expect("read");
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(v["model"], "sonnet");
    assert_eq!(v["env"]["OTHER"], "keep");
    assert!(v["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
    assert!(v["env"].get("ANTHROPIC_BASE_URL").is_none());
    assert!(read_claude_settings_token(&path)
        .expect("read token")
        .is_none());
}

#[test]
fn normalize_oauth_credentials_from_pkce_bundle() {
    let bundle = json!({
        "type": "oauth",
        "provider": "claude",
        "access_token": "at-claude",
        "refresh_token": "rt-claude",
        "expires_at": "2026-08-20T00:00:00+00:00",
        "email": "ada@example.com",
    });
    let normalized = normalize_oauth_credentials(&bundle).unwrap();
    assert_eq!(
        normalized.get("format").and_then(|v| v.as_str()),
        Some("credentials_json")
    );
    assert_eq!(
        normalized
            .pointer("/body/claudeAiOauth/accessToken")
            .and_then(|v| v.as_str()),
        Some("at-claude")
    );
    assert_eq!(
        normalized
            .pointer("/body/claudeAiOauth/refreshToken")
            .and_then(|v| v.as_str()),
        Some("rt-claude")
    );
    assert_eq!(
        normalized
            .pointer("/body/claudeAiOauth/expiresAt")
            .and_then(|v| v.as_i64()),
        Some(1_787_184_000_000)
    );
    assert_eq!(
        normalized.get("email").and_then(|v| v.as_str()),
        Some("ada@example.com")
    );
    assert!(normalized.get("secretTail").is_none());
    assert!(normalized.get("last4").is_none());
}

#[test]
fn normalize_oauth_credentials_keeps_valid_credentials_json() {
    let already = json!({
        "format": "credentials_json",
        "body": {
            "claudeAiOauth": {
                "accessToken": "at-keep",
                "refreshToken": "rt-keep",
                "expiresAt": 1_755_648_000_000_i64
            }
        }
    });
    let normalized = normalize_oauth_credentials(&already).unwrap();
    assert_eq!(normalized, already);
}

#[test]
fn normalize_oauth_credentials_requires_access_token() {
    let err = normalize_oauth_credentials(&json!({
        "type": "oauth",
        "provider": "claude",
        "refresh_token": "rt-only"
    }))
    .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(err.to_string().contains("access_token"));
}

#[test]
fn apply_account_writes_official_credentials_and_clears_leftover_api_env() {
    let _lock = CLAUDE_HOME_LOCK.lock().expect("claude home lock");
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("claude");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("settings.json"),
        r#"{
  "model": "sonnet",
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "sk-leftover",
    "ANTHROPIC_API_KEY": "sk-relay",
    "ANTHROPIC_BASE_URL": "https://relay.example.com",
    "OTHER": "keep"
  }
}
"#,
    )
    .unwrap();
    let prev = std::env::var_os("CLAUDE_CONFIG_DIR");
    std::env::set_var("CLAUDE_CONFIG_DIR", &home);
    let account = LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::Oauth,
        credentials: json!({
            "type": "oauth",
            "access_token": "at-official",
            "refresh_token": "rt-official",
            "expires_at": "2099-01-01T00:00:00Z"
        }),
        label_hint: Some("Claude oauth".into()),
        extra: json!({}),
    };
    let result = ClaudeAdapter.apply_account(&account);
    match prev {
        Some(value) => std::env::set_var("CLAUDE_CONFIG_DIR", value),
        None => std::env::remove_var("CLAUDE_CONFIG_DIR"),
    }
    result.unwrap();

    let written = std::fs::read_to_string(home.join(".credentials.json")).unwrap();
    let body: serde_json::Value = serde_json::from_str(&written).unwrap();
    assert_eq!(body["claudeAiOauth"]["accessToken"], "at-official");
    assert_eq!(body["claudeAiOauth"]["refreshToken"], "rt-official");
    assert!(body["claudeAiOauth"].get("expiresAt").is_some());

    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(home.join("settings.json")).unwrap())
            .unwrap();
    assert_eq!(settings["model"], "sonnet");
    assert_eq!(settings["env"]["OTHER"], "keep");
    assert!(settings["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
    assert!(settings["env"].get("ANTHROPIC_API_KEY").is_none());
    assert!(settings["env"].get("ANTHROPIC_BASE_URL").is_none());
}

static CLAUDE_HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
