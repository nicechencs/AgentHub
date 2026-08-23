use serde_json::json;

use crate::models::{
    AccountKind, AdapterProfile, AdapterProfileMode, AdapterProfileStatus, AdapterRoute,
    AdapterSourceKind, AgentId, Provider,
};

use super::{
    classify_account_live, classify_provider_config, should_skip_live_reconcile, LiveOrigin,
};

fn profile(port: u16) -> AdapterProfile {
    AdapterProfile {
        id: "profile-1".into(),
        name: "route".into(),
        source_kind: AdapterSourceKind::Account,
        source_id: "src".into(),
        target_agent_id: AgentId::Claude,
        route: AdapterRoute::LocalBridge,
        mode: AdapterProfileMode::Api,
        status: AdapterProfileStatus::Active,
        rule_id: "rule".into(),
        rule_version: "1".into(),
        generated_provider_id: Some("gen-1".into()),
        local_port: Some(port),
        auto_start: false,
        last_error_code: None,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

fn provider(id: &str, current: bool, generated: bool, url: &str, token: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Claude,
        name: id.into(),
        settings_config: json!({
            "env": {
                "ANTHROPIC_BASE_URL": url,
                "ANTHROPIC_AUTH_TOKEN": token
            }
        }),
        meta: if generated {
            json!({
                "generatedBy": "adapter",
                "adapterBridge": { "loopbackOnly": true }
            })
        } else {
            json!({})
        },
        is_current: current,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

#[test]
fn user_loopback_proxy_without_our_markers_is_a_grant() {
    let creds = json!({
        "format": "api_key",
        "api_key": "sk-local-proxy",
        "base_url": "http://127.0.0.1:43081"
    });
    assert_eq!(
        classify_account_live(
            AgentId::Claude,
            AccountKind::ApiKey,
            &creds,
            &[],
            &[],
            false
        ),
        LiveOrigin::UserGrant
    );
}

#[test]
fn current_generated_provider_is_active_projection() {
    let creds = json!({
        "format": "api_key",
        "api_key": "ahb_local",
        "base_url": "http://127.0.0.1:43081"
    });
    let providers = vec![provider(
        "gen-1",
        true,
        true,
        "http://127.0.0.1:43081",
        "ahb_local",
    )];
    assert_eq!(
        classify_account_live(
            AgentId::Claude,
            AccountKind::ApiKey,
            &creds,
            &[],
            &providers,
            false
        ),
        LiveOrigin::ActiveProjection
    );
}

#[test]
fn ahb_bearer_without_current_row_is_leftover_projection() {
    let creds = json!({
        "format": "api_key",
        "api_key": "ahb_stale",
        "base_url": "http://127.0.0.1:43081"
    });
    assert_eq!(
        classify_account_live(
            AgentId::Claude,
            AccountKind::ApiKey,
            &creds,
            &[],
            &[],
            false
        ),
        LiveOrigin::LeftoverProjection
    );
}

#[test]
fn loopback_port_matching_local_bridge_profile_is_projection() {
    let creds = json!({
        "format": "api_key",
        "api_key": "token-aaa",
        "base_url": "http://127.0.0.1:43081"
    });
    assert_eq!(
        classify_account_live(
            AgentId::Claude,
            AccountKind::ApiKey,
            &creds,
            &[profile(43081)],
            &[],
            false
        ),
        LiveOrigin::LeftoverProjection
    );
}

#[test]
fn leftover_toml_does_not_block_oauth_account_import() {
    let creds = json!({
        "format": "auth_json",
        "body": { "refresh_token": "rt", "access_token": "at" }
    });
    assert_eq!(
        classify_account_live(
            AgentId::Codex,
            AccountKind::Oauth,
            &creds,
            &[],
            &[],
            true
        ),
        LiveOrigin::UserGrant
    );
    assert!(!should_skip_live_reconcile(
        AgentId::Codex,
        AccountKind::Oauth,
        &creds,
        &[],
        &[],
        true
    ));
}

#[test]
fn leftover_toml_marks_non_oauth_live_as_projection() {
    let creds = json!({"format": "api_key", "api_key": "sk-leftover"});
    assert_eq!(
        classify_account_live(
            AgentId::Codex,
            AccountKind::ApiKey,
            &creds,
            &[],
            &[],
            true
        ),
        LiveOrigin::LeftoverProjection
    );
}

#[test]
fn provider_config_with_bridge_slug_is_projection() {
    let raw = json!({
        "format": "toml",
        "content": "model_provider = \"agenthub_grok_bridge\"\n"
    });
    assert_eq!(
        classify_provider_config(AgentId::Codex, &raw, &[], &[], false),
        LiveOrigin::LeftoverProjection
    );
}

#[test]
fn current_generated_on_claude_does_not_hide_unrelated_oauth() {
    let oauth = json!({
        "format": "auth_json",
        "body": { "refresh_token": "rt", "access_token": "at", "email": "a@x.com" }
    });
    let providers = vec![provider(
        "gen-1",
        true,
        true,
        "http://127.0.0.1:43081",
        "ahb_local",
    )];
    assert_eq!(
        classify_account_live(
            AgentId::Claude,
            AccountKind::Oauth,
            &oauth,
            &[],
            &providers,
            false
        ),
        LiveOrigin::UserGrant
    );
}
