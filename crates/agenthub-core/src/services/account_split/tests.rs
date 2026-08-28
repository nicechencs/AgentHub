use serde_json::json;

use crate::models::{Account, AccountKind, AgentId};

use super::{is_mixed_live_bundle, split_mixed_account};

fn grok_bundle() -> Account {
    Account {
        id: "grok-mixed".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::ApiKey,
        label: "API Key".into(),
        credentials: json!({
            "format": "grok_bundle",
            "api_key": "xai-file-key-12345678",
            "content": "[model.\"grok\"]\napi_key = \"xai-file-key-12345678\"\n",
            "auth": {
                "https://auth.x.ai::b1a00492-073a-47ea-816f-4c329264a828": {
                    "email": "a@example.com",
                    "refresh_token": "rt-oauth-1",
                    "access_token": "at-oauth-1"
                }
            }
        }),
        extra: json!({ "source": "config.toml+auth.json" }),
        status: "active".into(),
        is_current: true,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

#[test]
fn plain_api_key_is_not_a_mixed_bundle() {
    let credentials = json!({ "format": "api_key", "api_key": "xai-1" });
    assert!(!is_mixed_live_bundle(&credentials));
}

#[test]
fn grok_bundle_splits_oauth_and_api_key() {
    let split = split_mixed_account(&grok_bundle());
    assert_eq!(split.len(), 2);
    assert_eq!(split[0].id, "grok-mixed");
    assert_eq!(split[0].kind, AccountKind::Oauth);
    assert_eq!(split[0].credentials["format"], "auth_json");
    assert!(split[0].is_current);
    assert!(!split[0]
        .credentials
        .to_string()
        .contains("xai-file-key-12345678"));

    assert_eq!(split[1].kind, AccountKind::ApiKey);
    assert_ne!(split[1].id, "grok-mixed");
    assert!(!split[1].is_current);
    assert_eq!(split[1].credentials["format"], "api_key");
    assert_eq!(split[1].credentials["api_key"], "xai-file-key-12345678");
}

#[test]
fn grok_bundle_with_only_api_key_rewrites_format() {
    let mut account = grok_bundle();
    account.credentials.as_object_mut().unwrap().remove("auth");
    let split = split_mixed_account(&account);
    assert_eq!(split.len(), 1);
    assert_eq!(split[0].id, "grok-mixed");
    assert_eq!(split[0].kind, AccountKind::ApiKey);
    assert_eq!(split[0].credentials["format"], "api_key");
    assert_eq!(split[0].credentials["api_key"], "xai-file-key-12345678");
}

#[test]
fn kimi_bundle_splits_oauth_and_api_key() {
    let account = Account {
        id: "kimi-mixed".into(),
        agent_id: AgentId::Kimi,
        kind: AccountKind::ApiKey,
        label: "API Key".into(),
        credentials: json!({
            "format": "kimi_bundle",
            "api_key": "sk-kimi-secret-12345678",
            "content": "default_model = \"kimi-k2\"\n",
            "credentials_file": {
                "access_token": "kimi-at",
                "refresh_token": "kimi-rt"
            }
        }),
        extra: json!({}),
        status: "active".into(),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };
    let split = split_mixed_account(&account);
    assert_eq!(split.len(), 2);
    assert_eq!(split[0].kind, AccountKind::Oauth);
    assert_eq!(split[0].credentials["format"], "credentials_json");
    assert_eq!(split[1].kind, AccountKind::ApiKey);
    assert_eq!(split[1].credentials["format"], "api_key");
}

#[test]
fn claude_bundle_splits_oauth_and_api_key() {
    let account = Account {
        id: "claude-mixed".into(),
        agent_id: AgentId::Claude,
        kind: AccountKind::ApiKey,
        label: "API Key".into(),
        credentials: json!({
            "format": "claude_bundle",
            "api_key": "sk-ant-secret-12345678",
            "env_key": "ANTHROPIC_AUTH_TOKEN",
            "content": "{ \"env\": { \"ANTHROPIC_AUTH_TOKEN\": \"sk-ant-secret-12345678\" } }",
            "body": { "claudeAiOauth": { "accessToken": "claude-at" } }
        }),
        extra: json!({}),
        status: "active".into(),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };
    let split = split_mixed_account(&account);
    assert_eq!(split.len(), 2);
    assert_eq!(split[0].kind, AccountKind::Oauth);
    assert_eq!(split[0].credentials["format"], "credentials_json");
    assert_eq!(split[1].kind, AccountKind::ApiKey);
    assert_eq!(split[1].credentials["api_key"], "sk-ant-secret-12345678");
    assert!(split[1].credentials.get("body").is_none());
}
