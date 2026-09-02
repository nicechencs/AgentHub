use super::*;
use crate::error::AppError;
use crate::models::{
    Account, AccountKind, AdapterApplyPlan, AdapterCapabilityDecision, AdapterGateKind,
    AdapterMaturity, AdapterProfile, AdapterProfileMode, AdapterProfileStatus, AdapterReusePath,
    AdapterRoute, AdapterRouteAnalysis, AdapterRouteRequest, AdapterServiceImpact,
    AdapterSourceKind, AdapterSupport, AgentId, Provider, ADAPTER_CAPABILITY_MATRIX,
};
use crate::services::adapter_apply_service::apply_request_supported;
use crate::services::AdapterApplyService;
use crate::storage::{AccountRepo, AdapterProfileRepo, Database, ProviderRepo};
use std::collections::BTreeSet;
use std::path::PathBuf;

fn test_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("adapter-route.db")).unwrap();
    (dir, db)
}

fn provider(id: &str, agent_id: AgentId, preset: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id,
        name: format!("{preset} source"),
        settings_config: serde_json::json!({"api_key": "must-not-leak"}),
        meta: serde_json::json!({"preset": preset}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn kimi_coding_provider_without_preset(id: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Kimi,
        name: "Kimi coding live import".into(),
        settings_config: serde_json::json!({
            "format": "toml",
            "content": "base_url = \"https://api.kimi.com/coding/v1\"\napi_key = \"must-not-leak\"\n"
        }),
        meta: serde_json::json!({}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn kimi_membership_account(id: &str, kind: AccountKind, tagged: bool) -> Account {
    Account {
        id: id.into(),
        agent_id: AgentId::Kimi,
        kind,
        label: "Kimi Code membership".into(),
        credentials: serde_json::json!({
            "format": if kind == AccountKind::ApiKey { "api_key" } else { "oauth" },
            "api_key": if kind == AccountKind::ApiKey { "must-not-leak" } else { "" },
            "provider": if tagged { "kimi-code-membership" } else { "kimi" },
        }),
        extra: serde_json::json!({}),
        status: "active".into(),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn request(
    source_kind: AdapterSourceKind,
    source_id: &str,
    target_agent_id: AgentId,
) -> AdapterRouteRequest {
    AdapterRouteRequest {
        source_kind,
        source_id: source_id.into(),
        target_agent_id,
    }
}

#[test]
fn kimi_membership_routes_to_all_three_targets_and_plans_without_secret() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "random-kimi-provider-id",
            AgentId::Kimi,
            "kimi-code-membership",
        ))
        .unwrap();
    let service = AdapterRouteService::new(db);

    let claude = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "random-kimi-provider-id",
            AgentId::Claude,
        ))
        .unwrap();
    assert_eq!(claude.analysis.route, AdapterRoute::NativeEndpoint);
    assert_eq!(claude.analysis.support, AdapterSupport::Stable);
    assert_eq!(claude.maturity, AdapterMaturity::Stable);
    assert_eq!(
        claude.reuse_path,
        crate::models::AdapterReusePath::ApiEndpoint
    );
    assert_eq!(claude.service_impact, AdapterServiceImpact::None);
    assert!(claude.can_apply);
    assert_eq!(claude.changes[0].field, "baseUrl");
    assert_eq!(
        claude.changes[0].value.as_deref(),
        Some("https://api.kimi.com/coding/")
    );
    assert_eq!(claude.changes[1].field, "claudeAuthEnv");
    assert_eq!(
        claude.changes[1].value.as_deref(),
        Some("ANTHROPIC_AUTH_TOKEN")
    );
    assert_eq!(claude.changes[2].field, "apiKey");
    assert!(claude.changes[2].secret);
    assert!(claude.changes[2].value.is_none());
    assert!(claude.analysis.evidence[0].url.starts_with("https://"));

    let codex = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "random-kimi-provider-id",
            AgentId::Codex,
        ))
        .unwrap();
    assert_eq!(codex.analysis.route, AdapterRoute::LocalBridge);
    assert_eq!(codex.analysis.support, AdapterSupport::Experimental);
    assert_eq!(codex.maturity, AdapterMaturity::Experimental);
    assert_eq!(
        codex.service_impact,
        AdapterServiceImpact::RequiresLocalBridge
    );
    assert_eq!(
        codex.reuse_path,
        crate::models::AdapterReusePath::LocalBridge
    );
    assert!(codex.can_apply);
    assert_eq!(codex.changes.len(), 2);
    assert_eq!(codex.changes[0].target, "codex");
    assert_eq!(codex.changes[0].field, "provider");
    assert_eq!(
        codex.changes[0].value.as_deref(),
        Some("AgentHub Kimi 本机路由")
    );
    assert_eq!(codex.changes[1].field, "baseUrl");
    assert_eq!(
        codex.changes[1].value.as_deref(),
        Some("http://127.0.0.1:<本机端口>/v1")
    );
    assert!(codex.changes.iter().all(|change| !change.secret));

    let pi = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "random-kimi-provider-id",
            AgentId::Pi,
        ))
        .unwrap();
    assert_eq!(pi.analysis.route, AdapterRoute::ConfigSync);
    assert_eq!(pi.analysis.support, AdapterSupport::Stable);
    assert_eq!(pi.maturity, AdapterMaturity::Stable);
    assert_eq!(pi.reuse_path, crate::models::AdapterReusePath::ApiEndpoint);
    assert!(pi.can_apply);
    assert_eq!(pi.changes[0].field, "provider");
    assert_eq!(pi.changes[0].value.as_deref(), Some("kimi-for-coding"));
    assert_eq!(pi.changes[1].field, "apiKey");
    assert!(pi.changes[1].secret);
    assert!(pi.changes[1].value.is_none());

    let serialized = serde_json::to_string(&claude).unwrap();
    assert!(!serialized.contains("must-not-leak"));
    assert!(claude
        .analysis
        .actions
        .iter()
        .filter(|action| action.secret)
        .all(|action| action.value.is_none()));
}

#[test]
fn kimi_membership_account_uses_provider_edges_but_managed_oauth_stays_closed() {
    let (_dir, db) = test_db();
    let accounts = AccountRepo::new(db.clone());
    accounts
        .create(&kimi_membership_account(
            "kimi-account",
            AccountKind::ApiKey,
            true,
        ))
        .unwrap();
    accounts
        .create(&kimi_membership_account(
            "kimi-bare-account",
            AccountKind::ApiKey,
            false,
        ))
        .unwrap();
    accounts
        .create(&kimi_membership_account(
            "kimi-oauth-account",
            AccountKind::Oauth,
            true,
        ))
        .unwrap();
    let service = AdapterRouteService::new(db);

    for target in [AgentId::Claude, AgentId::Pi, AgentId::Codex] {
        let plan = service
            .plan(&request(AdapterSourceKind::Account, "kimi-account", target))
            .unwrap();
        assert!(plan.can_apply, "{target}");
        assert_eq!(
            plan.analysis.reason,
            match target {
                AgentId::Claude => "用这份 Kimi Code 会员接到 Claude，只改地址和模型。",
                AgentId::Pi => "把这份 Kimi Code 会员写进 Pi 认的登录位置。",
                AgentId::Codex => "Kimi Code 会员接到 Codex 需要本机转发。",
                _ => unreachable!(),
            }
        );
    }

    let bare = service
        .plan(&request(
            AdapterSourceKind::Account,
            "kimi-bare-account",
            AgentId::Claude,
        ))
        .unwrap();
    assert!(!bare.can_apply);
    assert!(bare.reason.contains("Kimi Code 会员"));

    let oauth = service
        .plan(&request(
            AdapterSourceKind::Account,
            "kimi-oauth-account",
            AgentId::Pi,
        ))
        .unwrap();
    assert!(!oauth.can_apply);
    assert_eq!(oauth.analysis.route, AdapterRoute::Unsupported);
}

#[test]
fn anthropic_provider_and_explicit_api_key_account_plan_for_pi() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "anthropic-provider",
            AgentId::Claude,
            "anthropic",
        ))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "anthropic-account".into(),
            agent_id: AgentId::Claude,
            kind: AccountKind::ApiKey,
            label: "Anthropic key".into(),
            credentials: serde_json::json!({"format": "api_key", "api_key": "must-not-leak"}),
            extra: serde_json::json!({"provider": "anthropic"}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterRouteService::new(db);

    let provider_req = request(
        AdapterSourceKind::Provider,
        "anthropic-provider",
        AgentId::Pi,
    );
    let account_req = request(AdapterSourceKind::Account, "anthropic-account", AgentId::Pi);
    let provider_analysis = service.analyze(&provider_req).unwrap();
    let account_analysis = service.analyze(&account_req).unwrap();
    assert_eq!(account_analysis.route, provider_analysis.route);
    assert_eq!(account_analysis.support, provider_analysis.support);
    assert_eq!(account_analysis.reason, provider_analysis.reason);

    let provider_plan = service.plan(&provider_req).unwrap();
    assert_eq!(provider_plan.analysis.route, AdapterRoute::ConfigSync);
    assert_eq!(provider_plan.analysis.support, AdapterSupport::Stable);
    assert_eq!(provider_plan.maturity, AdapterMaturity::Stable);
    assert!(provider_plan.can_apply);
    assert_eq!(provider_plan.changes[0].value.as_deref(), Some("anthropic"));
    assert!(provider_plan.changes[1].secret);
    assert!(provider_plan.changes[1].value.is_none());

    let account_plan = service.plan(&account_req).unwrap();
    assert_eq!(account_plan.analysis.route, provider_plan.analysis.route);
    assert_eq!(
        account_plan.analysis.support,
        provider_plan.analysis.support
    );
    assert_eq!(
        account_plan.analysis.reason, provider_plan.analysis.reason,
        "Account and same-surface Provider share the matrix reason gist"
    );
    assert_eq!(account_plan.maturity, provider_plan.maturity);
    assert!(
        account_plan.can_apply,
        "Anthropic API account → Pi is a bind implementation"
    );
    assert_eq!(account_plan.reason, account_plan.analysis.reason);
    assert_eq!(account_plan.changes[0].value.as_deref(), Some("anthropic"));
    assert!(account_plan.changes[1].secret);
    assert!(account_plan.changes[1].value.is_none());
}

#[test]
fn anthropic_account_without_api_key_format_is_unwritable() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "anthropic-bare".into(),
            agent_id: AgentId::Claude,
            kind: AccountKind::ApiKey,
            label: "Anthropic key".into(),
            credentials: serde_json::json!({"api_key": "must-not-leak"}),
            extra: serde_json::json!({"provider": "anthropic"}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let plan = AdapterRouteService::new(db)
        .plan(&request(
            AdapterSourceKind::Account,
            "anthropic-bare",
            AgentId::Pi,
        ))
        .unwrap();
    assert!(!plan.can_apply);
}

#[test]
fn grok_subscription_masked_access_token_is_unwritable() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "grok-masked".into(),
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "Grok subscription".into(),
            credentials: serde_json::json!({
                "format": "oauth",
                "access_token": "***"
            }),
            extra: serde_json::json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let plan = AdapterRouteService::new(db)
        .plan(&request(
            AdapterSourceKind::Account,
            "grok-masked",
            AgentId::Claude,
        ))
        .unwrap();
    assert_eq!(plan.analysis.route, AdapterRoute::LocalBridge);
    assert!(!plan.can_apply);
}

fn api_key_account(id: &str, provider: &str) -> Account {
    Account {
        id: id.into(),
        agent_id: AgentId::Claude,
        kind: AccountKind::ApiKey,
        label: format!("{provider} key"),
        credentials: serde_json::json!({"format": "api_key", "api_key": "must-not-leak"}),
        extra: serde_json::json!({"provider": provider}),
        status: "active".into(),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

#[test]
fn openai_and_xai_explicit_markers_plan_for_pi_and_reject_custom_relays() {
    let (_dir, db) = test_db();
    let providers = ProviderRepo::new(db.clone());
    providers
        .create(&provider("openai-provider", AgentId::Codex, "openai"))
        .unwrap();
    providers
        .create(&provider("xai-provider", AgentId::Grok, "xai"))
        .unwrap();
    providers
        .create(&Provider {
            id: "openai-host".into(),
            agent_id: AgentId::Claude,
            name: "OpenAI host import".into(),
            settings_config: serde_json::json!({
                "baseUrl": "https://api.openai.com/v1"
            }),
            meta: serde_json::json!({}),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    providers
        .create(&provider(
            "relay-provider",
            AgentId::Claude,
            "openai-compatible",
        ))
        .unwrap();
    providers
        .create(&provider(
            "glm-provider",
            AgentId::Claude,
            "glm-coding-plan",
        ))
        .unwrap();
    providers
        .create(&provider(
            "deepseek-provider",
            AgentId::Claude,
            "deepseek-api",
        ))
        .unwrap();
    let accounts = AccountRepo::new(db.clone());
    accounts
        .create(&api_key_account("openai-account", "openai"))
        .unwrap();
    accounts
        .create(&api_key_account("xai-account", "xai"))
        .unwrap();
    let service = AdapterRouteService::new(db);

    for (source_kind, source_id, slot, rule) in [
        (
            AdapterSourceKind::Provider,
            "openai-provider",
            "openai",
            "openai-api-to-pi-v1",
        ),
        (
            AdapterSourceKind::Account,
            "openai-account",
            "openai",
            "openai-api-to-pi-v1",
        ),
        (
            AdapterSourceKind::Provider,
            "openai-host",
            "openai",
            "openai-api-to-pi-v1",
        ),
        (
            AdapterSourceKind::Provider,
            "xai-provider",
            "xai",
            "xai-api-to-pi-v1",
        ),
        (
            AdapterSourceKind::Account,
            "xai-account",
            "xai",
            "xai-api-to-pi-v1",
        ),
    ] {
        let plan = service
            .plan(&request(source_kind, source_id, AgentId::Pi))
            .unwrap();
        assert_eq!(plan.analysis.route, AdapterRoute::ConfigSync, "{source_id}");
        assert!(plan.can_apply, "{source_id}");
        assert_eq!(plan.analysis.rule_id.as_deref(), Some(rule), "{source_id}");
        assert_eq!(plan.changes[0].value.as_deref(), Some(slot), "{source_id}");
        assert!(plan.changes[1].secret, "{source_id}");
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("must-not-leak"));
    }

    let relay = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "relay-provider",
            AgentId::Pi,
        ))
        .unwrap();
    assert_eq!(relay.analysis.route, AdapterRoute::Unsupported);
    assert!(!relay.can_apply);
    assert!(relay.analysis.rule_id.is_none());
    assert_eq!(
        service
            .classify_source_product(AdapterSourceKind::Provider, "relay-provider")
            .unwrap(),
        crate::models::AdapterSourceProduct::Other
    );

    let glm = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "glm-provider",
            AgentId::Pi,
        ))
        .unwrap();
    assert_eq!(
        service
            .classify_source_product(AdapterSourceKind::Provider, "glm-provider")
            .unwrap(),
        crate::models::AdapterSourceProduct::GlmCodingPlan
    );
    assert!(glm.can_apply);
    assert_eq!(glm.analysis.route, AdapterRoute::ConfigSync);
    assert_eq!(
        glm.analysis.rule_id.as_deref(),
        Some("glm-coding-plan-to-pi-v1")
    );
    assert_eq!(glm.changes[0].value.as_deref(), Some("glm-coding-plan"));

    let deepseek = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "deepseek-provider",
            AgentId::Pi,
        ))
        .unwrap();
    assert_eq!(
        service
            .classify_source_product(AdapterSourceKind::Provider, "deepseek-provider")
            .unwrap(),
        crate::models::AdapterSourceProduct::DeepseekApi
    );
    assert!(deepseek.can_apply);
    assert_eq!(deepseek.analysis.route, AdapterRoute::ConfigSync);
    assert_eq!(
        deepseek.analysis.rule_id.as_deref(),
        Some("deepseek-api-to-pi-v1")
    );
    assert_eq!(deepseek.changes[0].value.as_deref(), Some("deepseek"));

    accounts
        .create(&api_key_account("glm-account", "glm-coding-plan"))
        .unwrap();
    accounts
        .create(&api_key_account("deepseek-account", "deepseek-api"))
        .unwrap();
    for (source_id, rule, slot) in [
        ("glm-account", "glm-coding-plan-to-pi-v1", "glm-coding-plan"),
        ("deepseek-account", "deepseek-api-to-pi-v1", "deepseek"),
    ] {
        let plan = service
            .plan(&request(AdapterSourceKind::Account, source_id, AgentId::Pi))
            .unwrap();
        assert!(plan.can_apply, "{source_id}");
        assert_eq!(plan.analysis.rule_id.as_deref(), Some(rule));
        assert_eq!(plan.changes[0].value.as_deref(), Some(slot));
        assert!(plan.changes[1].secret);
    }
    for (source_kind, source_id, rule, base_url) in [
        (
            AdapterSourceKind::Provider,
            "glm-provider",
            "glm-coding-plan-to-claude-v1",
            crate::services::adapter_route_constants::GLM_CLAUDE_BASE_URL,
        ),
        (
            AdapterSourceKind::Account,
            "glm-account",
            "glm-coding-plan-to-claude-v1",
            crate::services::adapter_route_constants::GLM_CLAUDE_BASE_URL,
        ),
        (
            AdapterSourceKind::Provider,
            "deepseek-provider",
            "deepseek-api-to-claude-v1",
            crate::services::adapter_route_constants::DEEPSEEK_CLAUDE_BASE_URL,
        ),
        (
            AdapterSourceKind::Account,
            "deepseek-account",
            "deepseek-api-to-claude-v1",
            crate::services::adapter_route_constants::DEEPSEEK_CLAUDE_BASE_URL,
        ),
    ] {
        let plan = service
            .plan(&request(source_kind, source_id, AgentId::Claude))
            .unwrap();
        assert_eq!(
            plan.analysis.route,
            AdapterRoute::NativeEndpoint,
            "{source_id}"
        );
        assert_eq!(
            plan.analysis.support,
            AdapterSupport::Experimental,
            "{source_id}"
        );
        assert!(plan.can_apply, "{source_id}");
        assert_eq!(plan.analysis.rule_id.as_deref(), Some(rule), "{source_id}");
        assert_eq!(
            plan.changes[0].value.as_deref(),
            Some(base_url),
            "{source_id}"
        );
        assert!(plan.changes[2].secret, "{source_id}");
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("must-not-leak"));
    }

    let kimi_claude = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "relay-provider",
            AgentId::Claude,
        ))
        .unwrap();
    assert!(!kimi_claude.can_apply, "unknown relay must not bind Claude");

    let openai_grok = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "openai-provider",
            AgentId::Grok,
        ))
        .unwrap();
    assert!(openai_grok.can_apply);
    assert_eq!(openai_grok.analysis.route, AdapterRoute::LocalBridge);
    assert_eq!(
        openai_grok.analysis.rule_id.as_deref(),
        Some("openai-api-to-grok-bridge-v1")
    );

    let xai_grok = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "xai-provider",
            AgentId::Grok,
        ))
        .unwrap();
    assert!(!xai_grok.can_apply);
    assert_eq!(
        xai_grok.analysis.reason,
        crate::models::SAME_PROTOCOL_NO_EDGE_REASON
    );
    assert_eq!(xai_grok.analysis.reason, "这条接法还没做好，现在接不上。");
    assert!(!xai_grok.analysis.reason.contains("仅支持预览"));
}

#[test]
fn ambiguous_custom_relay_is_closed_by_plan_and_bind_preflight() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "ambiguous-relay".into(),
            agent_id: AgentId::Claude,
            name: "Ambiguous relay".into(),
            settings_config: serde_json::json!({
                "apiKey": "must-not-leak",
                "baseUrl": "https://api.openai.com/v1",
                "base_url": "https://relay.example/v1"
            }),
            meta: serde_json::json!({"preset": "openai-compatible"}),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();

    let plan = AdapterRouteService::new(db)
        .plan(&request(
            AdapterSourceKind::Provider,
            "ambiguous-relay",
            AgentId::Codex,
        ))
        .unwrap();
    assert_eq!(plan.analysis.route, AdapterRoute::LocalBridge);
    assert!(!plan.can_apply);
    assert_eq!(
        plan.reason,
        crate::services::adapter_route_constants::UNKNOWN_CUSTOM_RELAY_REASON
    );
    assert!(!serde_json::to_string(&plan)
        .unwrap()
        .contains("must-not-leak"));
}

#[test]
fn non_openai_provider_tags_cannot_be_promoted_by_base_urls() {
    let (_dir, db) = test_db();
    let providers = ProviderRepo::new(db.clone());
    for (id, agent_id, preset) in [
        ("spoofed-anthropic", AgentId::Claude, "anthropic"),
        ("spoofed-xai", AgentId::Grok, "xai"),
        ("spoofed-glm", AgentId::Claude, "glm-coding-plan"),
        ("spoofed-deepseek", AgentId::Claude, "deepseek-api"),
        ("spoofed-kimi", AgentId::Kimi, "kimi-code-membership"),
    ] {
        providers
            .create(&Provider {
                id: id.into(),
                agent_id,
                name: id.into(),
                settings_config: serde_json::json!({
                    "base_url": "https://api.openai.com.evil.example/v1"
                }),
                meta: serde_json::json!({ "preset": preset }),
                is_current: false,
                created_at: "now".into(),
                updated_at: "now".into(),
            })
            .unwrap();
    }

    let service = AdapterRouteService::new(db);
    for (id, expected) in [
        (
            "spoofed-anthropic",
            crate::models::AdapterSourceProduct::AnthropicApi,
        ),
        ("spoofed-xai", crate::models::AdapterSourceProduct::XaiApi),
        (
            "spoofed-glm",
            crate::models::AdapterSourceProduct::GlmCodingPlan,
        ),
        (
            "spoofed-deepseek",
            crate::models::AdapterSourceProduct::DeepseekApi,
        ),
        (
            "spoofed-kimi",
            crate::models::AdapterSourceProduct::KimiCodeMembership,
        ),
    ] {
        assert_eq!(
            service
                .classify_source_product(AdapterSourceKind::Provider, id)
                .unwrap(),
            expected,
            "{id}"
        );
    }
}

#[test]
fn spoofed_official_openai_tags_are_closed_by_plan() {
    let (_dir, db) = test_db();
    let providers = ProviderRepo::new(db.clone());
    for (id, preset, base_url) in [
        ("spoofed-openai", "openai", "https://relay.example/v1"),
        (
            "spoofed-openrouter",
            "openrouter",
            "https://api.openai.com/v1",
        ),
    ] {
        providers
            .create(&Provider {
                id: id.into(),
                agent_id: AgentId::Codex,
                name: id.into(),
                settings_config: serde_json::json!({ "base_url": base_url }),
                meta: serde_json::json!({ "preset": preset }),
                is_current: false,
                created_at: "now".into(),
                updated_at: "now".into(),
            })
            .unwrap();
    }

    let service = AdapterRouteService::new(db);
    for id in ["spoofed-openai", "spoofed-openrouter"] {
        let plan = service
            .plan(&request(AdapterSourceKind::Provider, id, AgentId::Pi))
            .unwrap();
        assert_eq!(plan.analysis.route, AdapterRoute::Unsupported, "{id}");
        assert!(!plan.can_apply, "{id}");
    }
}

#[test]
fn claude_subscription_account_to_pi_is_writable_but_same_edge_is_closed() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "claude-oauth".into(),
            agent_id: AgentId::Claude,
            kind: AccountKind::Oauth,
            label: "Claude login".into(),
            credentials: serde_json::json!({
                "format": "credentials_json",
                "access_token": "claude-access"
            }),
            extra: serde_json::json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "anthropic-account".into(),
            agent_id: AgentId::Claude,
            kind: AccountKind::ApiKey,
            label: "Anthropic key".into(),
            credentials: serde_json::json!({
                "format": "api_key",
                "api_key": "must-not-leak"
            }),
            extra: serde_json::json!({"provider": "anthropic"}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterRouteService::new(db);

    let other = service
        .plan(&request(
            AdapterSourceKind::Account,
            "claude-oauth",
            AgentId::Pi,
        ))
        .unwrap();
    assert!(
        other.can_apply,
        "Claude subscription account → Pi is writable"
    );
    assert_eq!(
        other.analysis.rule_id.as_deref(),
        Some("claude-subscription-to-pi-v1")
    );

    let anthropic_to_claude = service
        .plan(&request(
            AdapterSourceKind::Account,
            "anthropic-account",
            AgentId::Claude,
        ))
        .unwrap();
    assert!(
        !anthropic_to_claude.can_apply,
        "Anthropic account → Claude has no bind implementation"
    );
}

#[test]
fn anthropic_provider_and_account_plan_for_codex_and_stay_closed_for_claude() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "anthropic-provider",
            AgentId::Claude,
            "anthropic",
        ))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&api_key_account("anthropic-account", "anthropic"))
        .unwrap();
    let service = AdapterRouteService::new(db);

    for (source_kind, source_id) in [
        (AdapterSourceKind::Provider, "anthropic-provider"),
        (AdapterSourceKind::Account, "anthropic-account"),
    ] {
        let plan = service
            .plan(&request(source_kind, source_id, AgentId::Codex))
            .unwrap();
        assert_eq!(
            plan.analysis.route,
            AdapterRoute::LocalBridge,
            "{source_id}"
        );
        assert_eq!(
            plan.analysis.support,
            AdapterSupport::Experimental,
            "{source_id}"
        );
        assert_eq!(plan.maturity, AdapterMaturity::Experimental, "{source_id}");
        assert!(plan.can_apply, "{source_id}");
        assert_eq!(
            plan.analysis.rule_id.as_deref(),
            Some("anthropic-api-to-codex-v1"),
            "{source_id}"
        );
        assert_eq!(
            plan.changes[0].value.as_deref(),
            Some("AgentHub Anthropic 本机路由"),
            "{source_id}"
        );
        assert_eq!(
            plan.service_impact,
            AdapterServiceImpact::RequiresLocalBridge,
            "{source_id}"
        );
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("must-not-leak"));

        let claude = service
            .plan(&request(source_kind, source_id, AgentId::Claude))
            .unwrap();
        assert!(!claude.can_apply, "{source_id} → Claude stays closed");
        assert_eq!(claude.analysis.route, AdapterRoute::Unsupported);
    }
}

#[test]
fn openai_provider_and_account_plan_for_codex_local_bridge() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider("openai-provider", AgentId::Codex, "openai"))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&api_key_account("openai-account", "openai"))
        .unwrap();
    let service = AdapterRouteService::new(db);

    for (source_kind, source_id) in [
        (AdapterSourceKind::Provider, "openai-provider"),
        (AdapterSourceKind::Account, "openai-account"),
    ] {
        let plan = service
            .plan(&request(source_kind, source_id, AgentId::Codex))
            .unwrap();
        assert_eq!(
            plan.analysis.route,
            AdapterRoute::LocalBridge,
            "{source_id}"
        );
        assert_eq!(
            plan.analysis.support,
            AdapterSupport::Experimental,
            "{source_id}"
        );
        assert_eq!(plan.maturity, AdapterMaturity::Experimental, "{source_id}");
        assert!(plan.can_apply, "{source_id}");
        assert_eq!(
            plan.analysis.rule_id.as_deref(),
            Some("openai-api-to-codex-v1"),
            "{source_id}"
        );
        assert_eq!(
            plan.changes[0].value.as_deref(),
            Some("AgentHub OpenAI 本机路由"),
            "{source_id}"
        );
        assert_eq!(
            plan.service_impact,
            AdapterServiceImpact::RequiresLocalBridge,
            "{source_id}"
        );
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("must-not-leak"));
    }
}

#[test]
fn kimi_coding_endpoint_without_preset_classifies_as_membership() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_coding_provider_without_preset("kimi-live-import"))
        .unwrap();
    let service = AdapterRouteService::new(db);

    let claude = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "kimi-live-import",
            AgentId::Claude,
        ))
        .unwrap();
    assert_eq!(claude.analysis.route, AdapterRoute::NativeEndpoint);
    assert!(claude.can_apply);
    assert_eq!(
        claude.analysis.rule_id.as_deref(),
        Some("kimi-membership-to-claude-v1")
    );

    let codex = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "kimi-live-import",
            AgentId::Codex,
        ))
        .unwrap();
    assert_eq!(codex.analysis.route, AdapterRoute::LocalBridge);
    assert!(codex.can_apply);

    let pi = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "kimi-live-import",
            AgentId::Pi,
        ))
        .unwrap();
    assert_eq!(pi.analysis.route, AdapterRoute::ConfigSync);
    assert!(pi.can_apply);
}

#[test]
fn plan_any_ticket_to_cursor_uses_no_writer_reason() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "kimi-membership",
            AgentId::Kimi,
            "kimi-code-membership",
        ))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "anthropic-provider",
            AgentId::Claude,
            "anthropic",
        ))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&provider("openai-provider", AgentId::Codex, "openai"))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&provider("xai-provider", AgentId::Grok, "xai"))
        .unwrap();
    let service = AdapterRouteService::new(db);

    for source_id in [
        "kimi-membership",
        "anthropic-provider",
        "openai-provider",
        "xai-provider",
    ] {
        let plan = service
            .plan(&request(
                AdapterSourceKind::Provider,
                source_id,
                AgentId::Cursor,
            ))
            .unwrap();
        assert!(!plan.can_apply, "{source_id}");
        assert_eq!(
            plan.analysis.route,
            AdapterRoute::Unsupported,
            "{source_id}"
        );
        assert!(
            plan.reason.contains("不能写入配置"),
            "{source_id}: {}",
            plan.reason
        );
        assert_eq!(
            plan.reason,
            crate::models::AGENT_NO_WRITER_REASON,
            "{source_id}"
        );
        assert!(
            !plan.reason.contains("仅支持预览"),
            "Cursor must not use source-product copy: {}",
            plan.reason
        );
    }
}

#[test]
fn plan_kimi_ticket_to_grok_is_writable_native_endpoint() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "kimi-membership",
            AgentId::Kimi,
            "kimi-code-membership",
        ))
        .unwrap();
    let service = AdapterRouteService::new(db);

    let plan = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "kimi-membership",
            AgentId::Grok,
        ))
        .unwrap();
    assert!(plan.can_apply);
    assert_eq!(plan.analysis.route, AdapterRoute::NativeEndpoint);
    assert_eq!(
        plan.analysis.rule_id.as_deref(),
        Some("kimi-membership-to-grok-v1")
    );
    assert_eq!(
        plan.reuse_path,
        crate::models::AdapterReusePath::ApiEndpoint
    );
    assert_eq!(
        plan.changes[0].value.as_deref(),
        Some("https://api.kimi.com/coding/v1")
    );
    assert_eq!(plan.changes[2].field, "apiBackend");
    assert!(plan.changes[3].secret);
}

#[test]
fn kimi_custom_mytokens_is_openai_compat_local_bridge() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "qa-kimi".into(),
            agent_id: AgentId::Kimi,
            name: "QA Kimi manual".into(),
            settings_config: serde_json::json!({
                "format": "toml",
                "content": "default_model = \"kimi-k2\"\ndefault_provider = \"moonshot\"\n\n[providers.moonshot]\nbase_url = \"https://mytokens.cc/v1\"\napi_key = \"sk-test-8660\"\n"
            }),
            meta: serde_json::json!({ "preset": "custom" }),
            is_current: true,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterRouteService::new(db);
    let plan = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "qa-kimi",
            AgentId::Claude,
        ))
        .unwrap();
    assert_eq!(plan.analysis.route, AdapterRoute::LocalBridge);
    assert!(plan.can_apply, "{}", plan.analysis.reason);
    assert_eq!(
        plan.analysis.rule_id.as_deref(),
        Some("openai-api-to-claude-v1")
    );
}

#[test]
fn unsupported_and_missing_sources_have_no_changes() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider("moonshot-provider", AgentId::Kimi, "moonshot"))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "codex-account".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "Codex account".into(),
            credentials: serde_json::json!({"refresh_token": "must-not-leak"}),
            extra: serde_json::json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterRouteService::new(db);

    let unsupported = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "moonshot-provider",
            AgentId::Claude,
        ))
        .unwrap();
    assert_eq!(unsupported.analysis.route, AdapterRoute::Unsupported);
    assert_eq!(unsupported.analysis.support, AdapterSupport::Unsupported);
    assert_eq!(unsupported.maturity, AdapterMaturity::None);
    assert!(!unsupported.can_apply);
    assert!(unsupported.changes.is_empty());
    assert_eq!(unsupported.service_impact, AdapterServiceImpact::None);
    assert!(
        unsupported.analysis.reason.contains("Kimi Code 会员"),
        "moonshot must not use the opaque generic Other reason: {}",
        unsupported.analysis.reason
    );
    assert!(unsupported.analysis.reason.contains("api.kimi.com/coding"));
    assert_eq!(
        unsupported.analysis.evidence[0].url,
        "https://github.com/nicechencs/AgentHub/blob/release/docs/provider-api-oauth-adaptation.md"
    );
    assert_eq!(
        unsupported.analysis.evidence[0].label,
        "AgentHub：厂商、API 与 OAuth 适配规则"
    );

    let codex_to_claude = service
        .plan(&request(
            AdapterSourceKind::Account,
            "codex-account",
            AgentId::Claude,
        ))
        .unwrap();
    assert_eq!(codex_to_claude.analysis.route, AdapterRoute::LocalBridge);
    assert_eq!(
        codex_to_claude.analysis.support,
        AdapterSupport::Experimental
    );
    assert!(
        !codex_to_claude.can_apply,
        "refresh-only Codex oauth cannot write until access_token is present"
    );
    assert_eq!(
        codex_to_claude.analysis.rule_id.as_deref(),
        Some("codex-subscription-to-claude-responses-v1")
    );

    let missing = service.analyze(&request(
        AdapterSourceKind::Provider,
        "not-found",
        AgentId::Claude,
    ));
    assert!(matches!(missing, Err(AppError::NotFound(_))));
}

#[test]
fn codex_auth_json_account_to_claude_is_writable_local_bridge() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "codex-auth-json".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "ChatGPT subscription".into(),
            credentials: serde_json::json!({
                "format": "auth_json",
                "tokens": {"access_token": "must-not-leak", "refresh_token": "must-not-leak"}
            }),
            extra: serde_json::json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterRouteService::new(db);

    let plan = service
        .plan(&request(
            AdapterSourceKind::Account,
            "codex-auth-json",
            AgentId::Claude,
        ))
        .unwrap();
    assert_eq!(plan.analysis.route, AdapterRoute::LocalBridge);
    assert_eq!(plan.analysis.support, AdapterSupport::Experimental);
    assert_eq!(plan.maturity, AdapterMaturity::Experimental);
    assert!(plan.can_apply, "Codex OAuth → Claude Responses is writable");
    assert_eq!(
        plan.analysis.gate_kind,
        crate::models::AdapterGateKind::None
    );
    assert_eq!(
        plan.analysis.rule_id.as_deref(),
        Some("codex-subscription-to-claude-responses-v1")
    );
    assert_eq!(
        plan.analysis.reason,
        crate::models::CODEX_SUBSCRIPTION_TO_CLAUDE_REASON
    );
    assert_eq!(
        plan.reuse_path,
        crate::models::AdapterReusePath::LocalBridge
    );
    assert_eq!(
        plan.service_impact,
        AdapterServiceImpact::RequiresLocalBridge
    );
    assert_eq!(plan.changes[0].target, "claude");
    assert_eq!(plan.changes[0].field, "ANTHROPIC_BASE_URL");
    assert_eq!(
        plan.changes[0].value.as_deref(),
        Some("http://127.0.0.1:<本机端口>")
    );
    assert!(plan.changes[1].secret);
    assert_eq!(plan.changes[1].field, "ANTHROPIC_AUTH_TOKEN");
    assert!(plan.analysis.actions.iter().any(|action| {
        action.kind == "requires_local_bridge" && action.target == "Claude Code"
    }));
    assert!(!serde_json::to_string(&plan)
        .unwrap()
        .contains("must-not-leak"));

    // Matrix unit surface agrees with the service.
    let matrix = crate::models::decide_adapter_capability(
        crate::models::AdapterSourceProduct::CodexChatGptSubscription,
        crate::models::AdapterCredentialClass::OauthAuthJson,
        AgentId::Claude,
    )
    .public_surface();
    assert_eq!(matrix.route, AdapterRoute::LocalBridge);
    assert!(matrix.can_apply);
}

#[test]
fn official_codex_oauth_without_access_token_to_claude_is_unwritable() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "codex-oauth".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "ChatGPT subscription".into(),
            credentials: serde_json::json!({}),
            extra: serde_json::json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let plan = AdapterRouteService::new(db)
        .plan(&request(
            AdapterSourceKind::Account,
            "codex-oauth",
            AgentId::Claude,
        ))
        .unwrap();
    assert_eq!(plan.analysis.route, AdapterRoute::LocalBridge);
    assert_eq!(plan.analysis.support, AdapterSupport::Experimental);
    assert_eq!(
        plan.analysis.rule_id.as_deref(),
        Some("codex-subscription-to-claude-responses-v1")
    );
    assert!(!plan.can_apply);
}

#[test]
fn subscriptions_are_native_pi_reuse_with_opening_bind() {
    let (_dir, db) = test_db();
    let accounts = AccountRepo::new(db.clone());
    for (id, agent_id, credentials) in [
        (
            "claude-subscription",
            AgentId::Claude,
            serde_json::json!({
                "format": "credentials_json",
                "access_token": "claude-access"
            }),
        ),
        (
            "codex-subscription",
            AgentId::Codex,
            serde_json::json!({
                "format": "auth_json",
                "tokens": {"access_token": "must-not-leak"}
            }),
        ),
        (
            "grok-subscription",
            AgentId::Grok,
            serde_json::json!({
                "format": "oauth",
                "access_token": "grok-access"
            }),
        ),
    ] {
        accounts
            .create(&Account {
                id: id.into(),
                agent_id,
                kind: AccountKind::Oauth,
                label: id.into(),
                credentials,
                extra: serde_json::json!({}),
                status: "active".into(),
                is_current: false,
                created_at: "now".into(),
                updated_at: "now".into(),
            })
            .unwrap();
    }

    let service = AdapterRouteService::new(db);
    for (id, rule_id, reason, provider) in [
        (
            "claude-subscription",
            "claude-subscription-to-pi-v1",
            crate::models::CLAUDE_SUBSCRIPTION_TO_PI_REASON,
            "anthropic",
        ),
        (
            "codex-subscription",
            "codex-subscription-to-pi-v1",
            crate::models::CODEX_SUBSCRIPTION_TO_PI_REASON,
            "openai-codex",
        ),
        (
            "grok-subscription",
            "grok-subscription-to-pi-v1",
            crate::models::GROK_SUBSCRIPTION_TO_PI_REASON,
            "xai",
        ),
    ] {
        let plan = service
            .plan(&request(AdapterSourceKind::Account, id, AgentId::Pi))
            .unwrap();
        assert_eq!(plan.analysis.route, AdapterRoute::ConfigSync, "{id}");
        assert_eq!(plan.analysis.support, AdapterSupport::Experimental, "{id}");
        assert!(plan.can_apply, "{id}");
        assert_eq!(
            plan.analysis.gate_kind,
            crate::models::AdapterGateKind::None
        );
        assert_eq!(plan.analysis.rule_id.as_deref(), Some(rule_id), "{id}");
        assert_eq!(plan.reason, reason, "{id}");
        assert_eq!(
            plan.reuse_path,
            crate::models::AdapterReusePath::NativeSubscription,
            "{id}"
        );
        assert_eq!(plan.changes[0].value.as_deref(), Some(provider), "{id}");
        assert_eq!(
            plan.analysis.actions[1].kind, "reference_connection_secret",
            "{id}"
        );
        assert_eq!(
            plan.analysis.actions[1].secret, true,
            "{id}: action should remain a reference"
        );
        let serialized = serde_json::to_value(&plan).unwrap();
        assert_eq!(serialized["reusePath"], "native_subscription");
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("must-not-leak"));
        assert_eq!(
            plan.analysis.evidence[0].label, "Pi custom provider and model configuration",
            "{id}"
        );
        assert_eq!(
            plan.analysis.limitations,
            crate::models::SUBSCRIPTION_PI_APPLY_LIMITS
                .iter()
                .map(|value| (*value).to_owned())
                .collect::<Vec<_>>(),
            "{id}"
        );
    }
}

#[test]
fn official_codex_oauth_to_codex_is_native_self_bind() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "codex-live-1".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "41375197@qq.com".into(),
            credentials: serde_json::json!({
                "format": "auth_json",
                "body": {
                    "auth_mode": "chatgpt",
                    "tokens": {
                        "access_token": "must-not-leak",
                        "refresh_token": "must-not-leak"
                    }
                }
            }),
            extra: serde_json::json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterRouteService::new(db);
    let plan = service
        .plan(&request(
            AdapterSourceKind::Account,
            "codex-live-1",
            AgentId::Codex,
        ))
        .unwrap();
    assert!(plan.can_apply);
    assert_eq!(plan.analysis.route, AdapterRoute::NativeEndpoint);
    assert_eq!(
        plan.analysis.rule_id.as_deref(),
        Some(crate::models::CODEX_SUBSCRIPTION_TO_CODEX_RULE_ID)
    );
    assert_eq!(
        plan.reuse_path,
        crate::models::AdapterReusePath::NativeSubscription
    );
    assert_eq!(
        plan.reason,
        crate::models::CODEX_SUBSCRIPTION_TO_CODEX_REASON
    );
    assert!(!plan.reason.contains("本机路由"));
    assert_eq!(plan.changes[0].field, "login");
    assert_eq!(plan.changes[0].value.as_deref(), Some("官方登录"));
    assert!(!serde_json::to_string(&plan)
        .unwrap()
        .contains("must-not-leak"));
}

#[test]
fn official_codex_oauth_to_grok_kimi_dsh_is_writable_local_bridge() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "codex-live-1".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "chatgpt".into(),
            credentials: serde_json::json!({
                "format": "auth_json",
                "body": {
                    "auth_mode": "chatgpt",
                    "tokens": {
                        "access_token": "must-not-leak",
                        "refresh_token": "must-not-leak"
                    }
                }
            }),
            extra: serde_json::json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterRouteService::new(db);
    for (target, reason, field) in [
        (
            AgentId::Grok,
            crate::models::CODEX_SUBSCRIPTION_TO_GROK_REASON,
            "baseUrl",
        ),
        (
            AgentId::Kimi,
            crate::models::CODEX_SUBSCRIPTION_TO_KIMI_REASON,
            "baseUrl",
        ),
        (
            AgentId::Dsh,
            crate::models::CODEX_SUBSCRIPTION_TO_DSH_REASON,
            "baseURL",
        ),
    ] {
        let plan = service
            .plan(&request(AdapterSourceKind::Account, "codex-live-1", target))
            .unwrap();
        assert_eq!(plan.analysis.route, AdapterRoute::LocalBridge, "{target:?}");
        assert!(plan.can_apply, "{target:?}");
        assert_eq!(plan.reason, reason, "{target:?}");
        assert_eq!(
            plan.reuse_path,
            crate::models::AdapterReusePath::LocalBridge,
            "{target:?}"
        );
        assert!(
            plan.changes.iter().any(|change| change.field == field
                && change
                    .value
                    .as_deref()
                    .is_some_and(|value| value.contains("127.0.0.1"))),
            "{target:?} plan must write loopback {field}: {:?}",
            plan.changes
        );
        if target == AgentId::Grok {
            assert!(
                plan.changes
                    .iter()
                    .any(|change| change.field == "apiBackend"
                        && change.value.as_deref() == Some("responses")),
                "Codex→Grok local_bridge must plan apiBackend=responses: {:?}",
                plan.changes
            );
        }
        assert!(!plan.reason.contains("实验"), "{target:?}");
        assert!(!plan.reason.contains("未验证"), "{target:?}");
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("must-not-leak"));
    }
}

#[test]
fn stopped_grok_claude_route_does_not_block_codex_official_login_binds() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "grok-subscription".into(),
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "Grok subscription".into(),
            credentials: serde_json::json!({
                "format": "oauth",
                "access_token": "grok-access"
            }),
            extra: serde_json::json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "codex-live-1".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "41375197@qq.com".into(),
            credentials: serde_json::json!({
                "format": "auth_json",
                "body": {
                    "auth_mode": "chatgpt",
                    "tokens": {
                        "access_token": "must-not-leak",
                        "refresh_token": "must-not-leak"
                    }
                }
            }),
            extra: serde_json::json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "claude-grok-adapter-bridge-generated".into(),
            agent_id: AgentId::Claude,
            name: "Grok 本机路由".into(),
            settings_config: serde_json::json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "http://127.0.0.1:43121",
                    "ANTHROPIC_AUTH_TOKEN": "ahb_local"
                }
            }),
            meta: serde_json::json!({
                "generatedBy": "adapter",
                "adapterRuleId": "grok-subscription-to-claude-v1",
                "adapterSecretMode": "local_token"
            }),
            is_current: true,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&AdapterProfile {
            id: "adapter-grok-claude-bridge-stopped".into(),
            name: "Grok → Claude 本机路由".into(),
            source_kind: AdapterSourceKind::Account,
            source_id: "grok-subscription".into(),
            target_agent_id: AgentId::Claude,
            route: AdapterRoute::LocalBridge,
            mode: AdapterProfileMode::Oauth,
            status: AdapterProfileStatus::Active,
            rule_id: "grok-subscription-to-claude-v1".into(),
            rule_version: "1".into(),
            generated_provider_id: Some("claude-grok-adapter-bridge-generated".into()),
            local_port: Some(43121),
            auto_start: false,
            last_error_code: None,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterRouteService::new(db);
    let claude = service
        .plan(&request(
            AdapterSourceKind::Account,
            "codex-live-1",
            AgentId::Claude,
        ))
        .unwrap();
    assert!(
        claude.can_apply,
        "stopped Grok→Claude must not block Codex→Claude"
    );
    assert_eq!(claude.analysis.route, AdapterRoute::LocalBridge);
    assert_eq!(
        claude.analysis.rule_id.as_deref(),
        Some("codex-subscription-to-claude-responses-v1")
    );
    for target in [AgentId::Grok, AgentId::Kimi, AgentId::Dsh] {
        let plan = service
            .plan(&request(AdapterSourceKind::Account, "codex-live-1", target))
            .unwrap();
        assert!(plan.can_apply, "{target:?}");
        assert_eq!(plan.analysis.route, AdapterRoute::LocalBridge, "{target:?}");
    }
}

#[test]
fn claude_subscription_to_codex_is_open_but_unwritable() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "claude-subscription".into(),
            agent_id: AgentId::Claude,
            kind: AccountKind::Oauth,
            label: "Claude subscription".into(),
            credentials: serde_json::json!({"format": "credentials_json"}),
            extra: serde_json::json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterRouteService::new(db);
    let plan = service
        .plan(&request(
            AdapterSourceKind::Account,
            "claude-subscription",
            AgentId::Codex,
        ))
        .unwrap();
    assert_eq!(plan.analysis.route, AdapterRoute::LocalBridge);
    assert_eq!(plan.analysis.support, AdapterSupport::Experimental);
    assert_eq!(
        plan.analysis.gate_kind,
        crate::models::AdapterGateKind::PreviewOnly
    );
    assert_eq!(
        plan.analysis.rule_id.as_deref(),
        Some(crate::models::CLAUDE_SUBSCRIPTION_TO_CODEX_RULE_ID)
    );
    assert_eq!(
        plan.reason,
        crate::models::CLAUDE_SUBSCRIPTION_TO_CODEX_REASON
    );
    assert_eq!(plan.maturity, AdapterMaturity::Preview);
    assert!(
        !plan.reason.contains("产品不做"),
        "Claude → Codex is ③-open; reason must not say product-closed"
    );
    assert_eq!(
        plan.reuse_path,
        crate::models::AdapterReusePath::LocalBridge
    );
    assert!(!plan.can_apply);
}

#[test]
fn grok_subscription_to_claude_is_writable_local_bridge() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "grok-subscription".into(),
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "Grok subscription".into(),
            credentials: serde_json::json!({
                "format": "oauth",
                "access_token": "grok-access"
            }),
            extra: serde_json::json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterRouteService::new(db);
    let plan = service
        .plan(&request(
            AdapterSourceKind::Account,
            "grok-subscription",
            AgentId::Claude,
        ))
        .unwrap();
    assert_eq!(plan.analysis.route, AdapterRoute::LocalBridge);
    assert_eq!(plan.analysis.support, AdapterSupport::Experimental);
    assert_eq!(plan.reason, "Grok 登录会经本机路由接到 Claude Code。");
    assert_eq!(
        plan.reuse_path,
        crate::models::AdapterReusePath::LocalBridge
    );
    assert!(plan.can_apply);
    assert_eq!(
        plan.changes[0].value.as_deref(),
        Some("http://127.0.0.1:<本机端口>")
    );
    assert!(plan.changes[1].secret);
}

#[test]
fn grok_subscription_to_claude_without_access_token_is_unwritable() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "grok-subscription".into(),
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "Grok subscription".into(),
            credentials: serde_json::json!({"format": "oauth"}),
            extra: serde_json::json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let plan = AdapterRouteService::new(db)
        .plan(&request(
            AdapterSourceKind::Account,
            "grok-subscription",
            AgentId::Claude,
        ))
        .unwrap();
    assert_eq!(plan.analysis.route, AdapterRoute::LocalBridge);
    assert!(!plan.can_apply);
}

#[test]
fn grok_subscription_to_codex_is_writable_local_bridge() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "grok-subscription".into(),
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "Grok subscription".into(),
            credentials: serde_json::json!({
                "format": "oauth",
                "access_token": "grok-access"
            }),
            extra: serde_json::json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterRouteService::new(db);
    let plan = service
        .plan(&request(
            AdapterSourceKind::Account,
            "grok-subscription",
            AgentId::Codex,
        ))
        .unwrap();
    assert_eq!(plan.analysis.route, AdapterRoute::LocalBridge);
    assert_eq!(plan.analysis.support, AdapterSupport::Experimental);
    assert_eq!(
        plan.reason,
        crate::models::GROK_SUBSCRIPTION_TO_CODEX_REASON
    );
    assert_eq!(
        plan.reuse_path,
        crate::models::AdapterReusePath::LocalBridge
    );
    assert!(plan.can_apply);
    assert_eq!(
        plan.changes[0].value.as_deref(),
        Some("AgentHub Grok 本机路由")
    );
}

#[test]
fn grok_subscription_to_kimi_and_dsh_are_closed() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "grok-subscription".into(),
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "Grok subscription".into(),
            credentials: serde_json::json!({
                "format": "oauth",
                "access_token": "grok-access"
            }),
            extra: serde_json::json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterRouteService::new(db);
    let kimi = service
        .plan(&request(
            AdapterSourceKind::Account,
            "grok-subscription",
            AgentId::Kimi,
        ))
        .unwrap();
    assert_eq!(kimi.analysis.route, AdapterRoute::Unsupported);
    assert!(!kimi.can_apply);
    assert_eq!(kimi.reason, crate::models::GROK_SUBSCRIPTION_TO_KIMI_REASON);

    let dsh = service
        .plan(&request(
            AdapterSourceKind::Account,
            "grok-subscription",
            AgentId::Dsh,
        ))
        .unwrap();
    assert_eq!(dsh.analysis.route, AdapterRoute::Unsupported);
    assert!(!dsh.can_apply);
    assert_eq!(dsh.reason, crate::models::GROK_SUBSCRIPTION_TO_DSH_REASON);
}

#[test]
fn provider_and_oauth_misclassification_is_rejected() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "wrong-agent-kimi",
            AgentId::Claude,
            "kimi-code-membership",
        ))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "wrong-agent-anthropic",
            AgentId::Kimi,
            "anthropic",
        ))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "managed-kimi-oauth".into(),
            agent_id: AgentId::Kimi,
            kind: AccountKind::Oauth,
            label: "Kimi managed OAuth".into(),
            credentials: serde_json::json!({"provider": "kimi", "refresh_token": "must-not-leak"}),
            extra: serde_json::json!({"provider": "kimi"}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterRouteService::new(db);

    for source in [
        request(
            AdapterSourceKind::Provider,
            "wrong-agent-kimi",
            AgentId::Claude,
        ),
        request(
            AdapterSourceKind::Provider,
            "wrong-agent-anthropic",
            AgentId::Pi,
        ),
        request(
            AdapterSourceKind::Account,
            "managed-kimi-oauth",
            AgentId::Pi,
        ),
    ] {
        let analysis = service.analyze(&source).unwrap();
        assert_eq!(analysis.route, AdapterRoute::Unsupported);
        assert_eq!(analysis.support, AdapterSupport::Unsupported);
    }
}

fn deepseek_host_provider(id: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Claude,
        name: "DeepSeek host import".into(),
        settings_config: serde_json::json!({
            "apiKey": "must-not-leak",
            "baseUrl": "https://api.deepseek.com",
        }),
        meta: serde_json::json!({}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

#[test]
fn deepseek_preset_plans_dsh_without_secret() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider("ds-preset", AgentId::Claude, "deepseek"))
        .unwrap();
    let service = AdapterRouteService::new(db);
    let plan = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "ds-preset",
            AgentId::Dsh,
        ))
        .unwrap();
    assert_eq!(plan.analysis.route, AdapterRoute::ConfigSync);
    assert!(plan.can_apply);
    assert_eq!(
        plan.analysis.rule_id.as_deref(),
        Some("deepseek-api-to-dsh-v1")
    );
    assert_eq!(plan.changes[0].field, "provider");
    assert_eq!(plan.changes[0].value.as_deref(), Some("deepseek-official"));
    assert_eq!(plan.changes[1].field, "apiKeyEnv");
    assert_eq!(plan.changes[2].field, "apiKey");
    assert!(plan.changes[2].secret);
    assert!(plan.changes[2].value.is_none());
    assert!(!serde_json::to_string(&plan)
        .unwrap()
        .contains("must-not-leak"));
}

#[test]
fn deepseek_official_host_without_preset_plans_dsh() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&deepseek_host_provider("ds-host"))
        .unwrap();
    let service = AdapterRouteService::new(db);
    let plan = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "ds-host",
            AgentId::Dsh,
        ))
        .unwrap();
    assert_eq!(plan.analysis.route, AdapterRoute::ConfigSync);
    assert!(plan.can_apply);
    assert_eq!(
        plan.analysis.rule_id.as_deref(),
        Some("deepseek-api-to-dsh-v1")
    );
}

#[test]
fn dsh_agent_id_alone_does_not_classify_as_deepseek_api() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider("dsh-only", AgentId::Dsh, "default"))
        .unwrap();
    let service = AdapterRouteService::new(db);
    let plan = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "dsh-only",
            AgentId::Dsh,
        ))
        .unwrap();
    assert_eq!(plan.analysis.route, AdapterRoute::Unsupported);
    assert!(!plan.can_apply);
}

#[test]
fn deepseek_to_claude_plan_is_experimental() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider("ds-preset", AgentId::Claude, "deepseek"))
        .unwrap();
    let service = AdapterRouteService::new(db);
    let plan = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "ds-preset",
            AgentId::Claude,
        ))
        .unwrap();
    assert_eq!(plan.analysis.route, AdapterRoute::NativeEndpoint);
    assert_eq!(plan.analysis.support, AdapterSupport::Experimental);
    assert!(plan.can_apply);
    assert_eq!(
        plan.analysis.rule_id.as_deref(),
        Some("deepseek-api-to-claude-v1")
    );
}

#[test]
fn glm_and_deepseek_codex_provider_and_account_plans_are_native_and_writable() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider("glm-codex", AgentId::Claude, "glm-coding-plan"))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "deepseek-codex".into(),
            agent_id: AgentId::Claude,
            kind: AccountKind::ApiKey,
            label: "DeepSeek".into(),
            credentials: serde_json::json!({
                "format": "api_key",
                "api_key": "must-not-leak"
            }),
            extra: serde_json::json!({ "provider": "deepseek-api" }),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterRouteService::new(db);

    for (kind, id, rule, base_url) in [
        (
            AdapterSourceKind::Provider,
            "glm-codex",
            "glm-coding-plan-to-codex-v1",
            "https://open.bigmodel.cn/api/v1",
        ),
        (
            AdapterSourceKind::Account,
            "deepseek-codex",
            "deepseek-api-to-codex-v1",
            "https://api.deepseek.com",
        ),
    ] {
        let plan = service.plan(&request(kind, id, AgentId::Codex)).unwrap();
        assert_eq!(plan.analysis.route, AdapterRoute::NativeEndpoint);
        assert_eq!(plan.analysis.support, AdapterSupport::Experimental);
        assert!(plan.can_apply);
        assert_eq!(
            plan.reuse_path,
            crate::models::AdapterReusePath::ApiEndpoint
        );
        assert_eq!(plan.analysis.rule_id.as_deref(), Some(rule));
        assert_eq!(plan.service_impact, AdapterServiceImpact::None);
        assert!(plan.changes.iter().any(|change| {
            change.field == "baseUrl" && change.value.as_deref() == Some(base_url)
        }));
        assert!(plan
            .changes
            .iter()
            .all(|change| !change.secret || change.value.is_none()));
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("must-not-leak"));
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedContractFile {
    cases: Vec<SharedContractCase>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedContractCase {
    id: String,
    source: SharedContractSource,
    target: AgentId,
    /// Matrix rule kept in the fixture set even though classify picks a sibling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    documented_rule_id: Option<String>,
    expect: SharedContractExpect,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedContractSource {
    kind: AdapterSourceKind,
    agent_id: AgentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account_kind: Option<AccountKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    credential_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    extra: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    credentials: Option<serde_json::Value>,
}

/// Production apply entry for a route surface. Distinct from `canApply`:
/// `local_bridge` is plan-open but must not go through `AdapterApplyService`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum SharedContractApplyPath {
    Native,
    LocalBridge,
    ConfigSync,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedContractExpect {
    route: AdapterRoute,
    support: AdapterSupport,
    can_apply: bool,
    apply_path: SharedContractApplyPath,
    rule_id: Option<String>,
    gate_kind: AdapterGateKind,
    reason: String,
    reuse_path: AdapterReusePath,
}

const ADAPTER_CAPABILITY_CONTRACT_WATCH: &str =
    include_str!("../../../../../src/dev/mocks/fixtures/adapter-capability-contract.json");

fn adapter_capability_contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/dev/mocks/fixtures/adapter-capability-contract.json")
}

fn shared_capability_contract() -> SharedContractFile {
    let path = adapter_capability_contract_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "read adapter-capability-contract.json from {}: {err}",
            path.display()
        )
    });
    let _ = ADAPTER_CAPABILITY_CONTRACT_WATCH;
    serde_json::from_str(&text).expect("shared adapter capability contract")
}

fn seed_shared_contract_source(db: &Database, case: &SharedContractCase, source_id: &str) {
    match case.source.kind {
        AdapterSourceKind::Provider => {
            ProviderRepo::new(db.clone())
                .create(&provider(
                    source_id,
                    case.source.agent_id,
                    case.source.preset.as_deref().unwrap_or("default"),
                ))
                .unwrap();
        }
        AdapterSourceKind::Account => {
            AccountRepo::new(db.clone())
                .create(&Account {
                    id: source_id.into(),
                    agent_id: case.source.agent_id,
                    kind: case.source.account_kind.unwrap_or(AccountKind::Oauth),
                    label: case.id.clone(),
                    credentials: case
                        .source
                        .credentials
                        .clone()
                        .unwrap_or_else(|| serde_json::json!({})),
                    extra: case
                        .source
                        .extra
                        .clone()
                        .unwrap_or_else(|| serde_json::json!({})),
                    status: "active".into(),
                    is_current: false,
                    created_at: "now".into(),
                    updated_at: "now".into(),
                })
                .unwrap();
        }
    }
}

fn plan_shared_contract_case(case: &SharedContractCase) -> AdapterApplyPlan {
    let (_dir, db) = test_db();
    let source_id = format!("contract-{}", case.id);
    seed_shared_contract_source(&db, case, &source_id);
    AdapterRouteService::new(db)
        .plan(&request(case.source.kind, &source_id, case.target))
        .unwrap_or_else(|err| panic!("{}: plan() failed: {err}", case.id))
}

fn apply_path_from_plan(plan: &AdapterApplyPlan) -> SharedContractApplyPath {
    if !plan.can_apply {
        return SharedContractApplyPath::Rejected;
    }
    match plan.analysis.route {
        AdapterRoute::NativeEndpoint => SharedContractApplyPath::Native,
        AdapterRoute::LocalBridge => SharedContractApplyPath::LocalBridge,
        AdapterRoute::ConfigSync => SharedContractApplyPath::ConfigSync,
        AdapterRoute::Unsupported => SharedContractApplyPath::Rejected,
    }
}

fn expect_from_plan(plan: &AdapterApplyPlan) -> SharedContractExpect {
    SharedContractExpect {
        route: plan.analysis.route,
        support: plan.analysis.support,
        can_apply: plan.can_apply,
        apply_path: apply_path_from_plan(plan),
        rule_id: plan.analysis.rule_id.clone(),
        gate_kind: plan.analysis.gate_kind,
        reason: plan.reason.clone(),
        reuse_path: plan.reuse_path,
    }
}

fn project_shared_capability_contract(inputs: &SharedContractFile) -> SharedContractFile {
    let mut cases: Vec<SharedContractCase> = inputs
        .cases
        .iter()
        .map(|case| SharedContractCase {
            id: case.id.clone(),
            source: case.source.clone(),
            target: case.target,
            documented_rule_id: case
                .documented_rule_id
                .as_deref()
                .filter(|id| !id.is_empty())
                .map(str::to_owned),
            expect: expect_from_plan(&plan_shared_contract_case(case)),
        })
        .collect();
    cases.sort_by(|left, right| left.id.cmp(&right.id));
    SharedContractFile { cases }
}

fn canonical_contract_json(file: &SharedContractFile) -> String {
    let mut text = serde_json::to_string_pretty(file).expect("serialize capability contract");
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n")
}

fn update_adapter_capability_contract_requested() -> bool {
    // Run only `shared_capability_contract_is_kernel_plan_projection` when writing;
    // other tests may still be reading the same fixture.
    matches!(
        std::env::var("UPDATE_ADAPTER_CAPABILITY_CONTRACT")
            .ok()
            .as_deref(),
        Some("1") | Some("true")
    )
}

fn collect_credential_strings(value: &serde_json::Value, key: Option<&str>, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) if !text.is_empty() => {
            if !matches!(key, Some("format") | Some("provider")) {
                out.push(text.clone());
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_credential_strings(item, key, out);
            }
        }
        serde_json::Value::Object(map) => {
            for (child_key, nested) in map {
                collect_credential_strings(nested, Some(child_key), out);
            }
        }
        _ => {}
    }
}

fn assert_analyze_plan_hide_credentials(
    case_id: &str,
    credentials: Option<&serde_json::Value>,
    analysis_json: &str,
    plan_json: &str,
) {
    let mut forbidden = vec!["must-not-leak".to_string()];
    if let Some(credentials) = credentials {
        collect_credential_strings(credentials, None, &mut forbidden);
    }
    for blob in [analysis_json, plan_json] {
        for secret in &forbidden {
            assert!(
                !blob.contains(secret),
                "{case_id}: analyze/plan leaked credential {secret:?}"
            );
        }
    }
}

fn assert_apply_path_consistent(case_id: &str, expect: &SharedContractExpect) {
    match expect.apply_path {
        SharedContractApplyPath::Native => {
            assert!(
                expect.can_apply,
                "{case_id}: applyPath=native requires canApply=true"
            );
            assert_eq!(
                expect.route,
                AdapterRoute::NativeEndpoint,
                "{case_id}: applyPath=native requires native_endpoint"
            );
        }
        SharedContractApplyPath::LocalBridge => {
            assert!(
                expect.can_apply,
                "{case_id}: applyPath=local_bridge requires canApply=true"
            );
            assert_eq!(
                expect.route,
                AdapterRoute::LocalBridge,
                "{case_id}: applyPath=local_bridge requires local_bridge route"
            );
        }
        SharedContractApplyPath::ConfigSync => {
            assert!(
                expect.can_apply,
                "{case_id}: applyPath=config_sync requires canApply=true"
            );
            assert_eq!(
                expect.route,
                AdapterRoute::ConfigSync,
                "{case_id}: applyPath=config_sync requires config_sync route"
            );
        }
        SharedContractApplyPath::Rejected => {
            assert!(
                !expect.can_apply,
                "{case_id}: applyPath=rejected requires canApply=false"
            );
        }
    }
}

#[test]
fn shared_capability_contract_matches_classify_and_plan() {
    let contract = shared_capability_contract();
    for case in contract.cases {
        assert_apply_path_consistent(&case.id, &case.expect);
        let (_dir, db) = test_db();
        let source_id = format!("contract-{}", case.id);
        seed_shared_contract_source(&db, &case, &source_id);
        let service = AdapterRouteService::new(db);
        let req = request(case.source.kind, &source_id, case.target);
        let analysis = service.analyze(&req).unwrap();
        let plan = service.plan(&req).unwrap();
        let analysis_json = serde_json::to_string(&analysis).unwrap();
        let plan_json = serde_json::to_string(&plan).unwrap();
        assert_analyze_plan_hide_credentials(
            &case.id,
            case.source.credentials.as_ref(),
            &analysis_json,
            &plan_json,
        );
        if let Some(documented) = case
            .documented_rule_id
            .as_deref()
            .filter(|id| !id.is_empty())
        {
            assert_ne!(
                analysis.rule_id.as_deref(),
                Some(documented),
                "{}: documented sibling {documented} must not be the classify winner",
                case.id
            );
        }
        assert_eq!(analysis.route, case.expect.route, "{}", case.id);
        assert_eq!(analysis.support, case.expect.support, "{}", case.id);
        assert_eq!(analysis.rule_id, case.expect.rule_id, "{}", case.id);
        assert_eq!(analysis.gate_kind, case.expect.gate_kind, "{}", case.id);
        assert_eq!(analysis.reason, case.expect.reason, "{}", case.id);
        assert_eq!(plan.can_apply, case.expect.can_apply, "{}", case.id);
        assert_eq!(plan.reuse_path, case.expect.reuse_path, "{}", case.id);
        assert_eq!(plan.reason, case.expect.reason, "{}", case.id);
        assert_eq!(
            apply_path_from_plan(&plan),
            case.expect.apply_path,
            "{}",
            case.id
        );

        // applyPath documents production entry: local_bridge is plan-open but
        // never goes through AdapterApplyService (native config write only).
        match case.expect.apply_path {
            SharedContractApplyPath::Native => {
                assert_eq!(analysis.route, AdapterRoute::NativeEndpoint, "{}", case.id);
            }
            SharedContractApplyPath::LocalBridge => {
                assert_eq!(analysis.route, AdapterRoute::LocalBridge, "{}", case.id);
            }
            SharedContractApplyPath::ConfigSync => {
                assert_eq!(analysis.route, AdapterRoute::ConfigSync, "{}", case.id);
            }
            SharedContractApplyPath::Rejected => {
                assert!(!plan.can_apply, "{}", case.id);
            }
        }
    }
}

#[test]
fn shared_capability_contract_rule_ids_match_matrix() {
    let matrix_ids: BTreeSet<&str> = ADAPTER_CAPABILITY_MATRIX
        .iter()
        .map(|cell| cell.rule_id)
        .filter(|id| !id.is_empty())
        .collect();
    let contract = shared_capability_contract();
    let mut fixture_ids = BTreeSet::new();
    for case in &contract.cases {
        if let Some(rule_id) = case.expect.rule_id.as_deref().filter(|id| !id.is_empty()) {
            fixture_ids.insert(rule_id);
        }
        if let Some(rule_id) = case
            .documented_rule_id
            .as_deref()
            .filter(|id| !id.is_empty())
        {
            fixture_ids.insert(rule_id);
        }
    }
    let missing: Vec<_> = matrix_ids.difference(&fixture_ids).copied().collect();
    let extra: Vec<_> = fixture_ids.difference(&matrix_ids).copied().collect();
    assert_eq!(
        matrix_ids, fixture_ids,
        "shared capability contract rule ids drifted from ADAPTER_CAPABILITY_MATRIX; missing={missing:?} extra={extra:?}"
    );
}

#[test]
fn shared_capability_contract_is_kernel_plan_projection() {
    let committed = shared_capability_contract();
    let projected = project_shared_capability_contract(&committed);
    let generated = canonical_contract_json(&projected);
    let path = adapter_capability_contract_path();
    if update_adapter_capability_contract_requested() {
        std::fs::write(&path, &generated).unwrap_or_else(|err| {
            panic!("write {} failed: {err}", path.display());
        });
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!("read {} failed: {err}", path.display());
    });
    assert_eq!(
        normalize_newlines(&on_disk),
        generated,
        "adapter-capability-contract.json must equal AdapterRouteService::plan() on frozen inputs; re-run with UPDATE_ADAPTER_CAPABILITY_CONTRACT=1"
    );

    let mut seen_write_gate_block = false;
    for case in &projected.cases {
        assert_apply_path_consistent(&case.id, &case.expect);
        let plan = plan_shared_contract_case(case);
        let plan_json = serde_json::to_string(&plan).unwrap();
        let analysis_json = serde_json::to_string(&plan.analysis).unwrap();
        assert_analyze_plan_hide_credentials(
            &case.id,
            case.source.credentials.as_ref(),
            &analysis_json,
            &plan_json,
        );
        let expect_json = serde_json::to_string(&case.expect).unwrap();
        assert!(
            !expect_json.contains("must-not-leak"),
            "{}: plan() expect must not contain credential placeholders",
            case.id
        );
        assert_eq!(
            serde_json::to_value(&case.expect).unwrap(),
            serde_json::to_value(&expect_from_plan(&plan)).unwrap(),
            "{}: committed expect must equal plan() output",
            case.id
        );
        if case.expect.route != AdapterRoute::Unsupported
            && case.expect.support != AdapterSupport::Unsupported
            && case.expect.gate_kind == AdapterGateKind::None
            && !case.expect.can_apply
            && case.expect.apply_path == SharedContractApplyPath::Rejected
        {
            seen_write_gate_block = true;
        }
    }
    assert!(
        seen_write_gate_block,
        "golden must include at least one write-gate blocked edge (route open, gateKind=none, canApply=false)"
    );
}

#[test]
fn shared_capability_contract_projection_is_deterministic() {
    let committed = shared_capability_contract();
    let first = canonical_contract_json(&project_shared_capability_contract(&committed));
    let second = canonical_contract_json(&project_shared_capability_contract(&committed));
    assert_eq!(first, second);
}

#[test]
fn shared_capability_contract_hand_edited_expect_does_not_match_plan() {
    let mut tampered = shared_capability_contract();
    let case = tampered
        .cases
        .iter_mut()
        .find(|case| case.expect.can_apply)
        .expect("open golden case");
    case.expect.can_apply = !case.expect.can_apply;
    case.expect.apply_path = SharedContractApplyPath::Rejected;
    let projected = project_shared_capability_contract(&tampered);
    assert_ne!(
        canonical_contract_json(&tampered),
        canonical_contract_json(&projected),
        "hand-edited expect must not equal kernel plan() projection"
    );
}

fn analysis_from_cell(cell: &crate::models::AdapterCapabilityCell) -> AdapterRouteAnalysis {
    let decision = AdapterCapabilityDecision::from_cell(cell);
    AdapterRouteAnalysis {
        route: decision.route,
        support: decision.support,
        reason: cell.reason.into(),
        actions: vec![],
        limitations: vec![],
        evidence: vec![],
        rule_id: Some(cell.rule_id.to_string()),
        gate_kind: decision.gate_kind,
    }
}

#[test]
fn openai_api_to_codex_matrix_write_gate_has_bridge_arm() {
    const RULE: &str = "openai-api-to-codex-v1";
    let cell = ADAPTER_CAPABILITY_MATRIX
        .iter()
        .find(|cell| cell.rule_id == RULE)
        .expect("openai-api-to-codex-v1 cell");
    assert!(cell.can_apply && cell.gates.all_passed());
    assert_eq!(cell.route, AdapterRoute::LocalBridge);
    let analysis = analysis_from_cell(cell);
    let mut any_open = false;
    for &kind in source_kinds_for_rule(RULE) {
        let req = AdapterRouteRequest {
            source_kind: kind,
            source_id: "openai-codex-consistency".into(),
            target_agent_id: cell.key.target,
        };
        assert!(
            bind_implementation_open(&req, &analysis),
            "{kind:?} must open write_gate bind for {RULE}"
        );
        any_open = true;
        assert!(
            !AdapterApplyService::apply_has_arm(RULE, kind, cell.key.target, cell.route),
            "{RULE} is local_bridge and must not have an AdapterApplyService arm"
        );
        assert!(
            crate::services::adapter_bridge_service::live_bridge_rule_projections()
                .any(|(agent, rule_id)| agent == AgentId::Codex && rule_id == RULE),
            "{RULE} must have a live bridge arm"
        );
    }
    assert!(
        any_open,
        "{RULE} has no bind_implementation_open source kind"
    );
}

fn source_kinds_for_rule(rule_id: &str) -> &'static [AdapterSourceKind] {
    match rule_id {
        "claude-subscription-to-pi-v1"
        | "codex-subscription-to-pi-v1"
        | "grok-subscription-to-pi-v1"
        | "codex-subscription-to-claude-responses-v1"
        | "grok-subscription-to-claude-v1"
        | "grok-subscription-to-codex-v1"
        | "codex-subscription-to-grok-v1"
        | "codex-subscription-to-kimi-v1"
        | "codex-subscription-to-dsh-v1"
        | "claude-subscription-to-codex-v1" => &[AdapterSourceKind::Account],
        "deepseek-api-to-dsh-v1" => &[AdapterSourceKind::Provider],
        _ => &[AdapterSourceKind::Provider, AdapterSourceKind::Account],
    }
}

#[test]
fn open_matrix_cells_have_bind_and_apply_arms() {
    for cell in ADAPTER_CAPABILITY_MATRIX {
        if !(cell.can_apply && cell.gates.all_passed()) {
            continue;
        }
        let analysis = analysis_from_cell(cell);
        let mut any_open = false;
        for &kind in source_kinds_for_rule(cell.rule_id) {
            let req = AdapterRouteRequest {
                source_kind: kind,
                source_id: "consistency".into(),
                target_agent_id: cell.key.target,
            };
            if !bind_implementation_open(&req, &analysis) {
                continue;
            }
            any_open = true;
            if cell.route == AdapterRoute::LocalBridge
                || cell.rule_id == crate::models::CODEX_SUBSCRIPTION_TO_CODEX_RULE_ID
            {
                assert!(
                    !AdapterApplyService::apply_has_arm(
                        cell.rule_id,
                        kind,
                        cell.key.target,
                        cell.route,
                    ),
                    "host/account-switch cell {} must not have an apply arm",
                    cell.rule_id
                );
                assert!(
                    !apply_request_supported(
                        kind,
                        cell.key.target,
                        cell.route,
                        Some(cell.rule_id),
                        cell.support,
                        analysis.gate_kind,
                    ),
                    "host/account-switch cell {} must fail ensure_supported",
                    cell.rule_id
                );
                continue;
            }
            assert!(
                AdapterApplyService::apply_has_arm(cell.rule_id, kind, cell.key.target, cell.route,),
                "open cell {} ({:?} -> {:?}) has bind but no apply arm",
                cell.rule_id,
                kind,
                cell.key.target
            );
            assert!(
                apply_request_supported(
                    kind,
                    cell.key.target,
                    cell.route,
                    Some(cell.rule_id),
                    cell.support,
                    analysis.gate_kind,
                ),
                "open cell {} ({:?} -> {:?}) has bind/apply arm but ensure_supported is closed",
                cell.rule_id,
                kind,
                cell.key.target
            );
        }
        assert!(
            any_open,
            "open cell {} has no bind_implementation_open source kind",
            cell.rule_id
        );
    }
}

#[test]
fn closed_or_preview_cells_fail_apply_request_supported() {
    for cell in ADAPTER_CAPABILITY_MATRIX {
        if cell.can_apply && cell.gates.all_passed() {
            continue;
        }
        let decision = AdapterCapabilityDecision::from_cell(cell);
        for &kind in source_kinds_for_rule(cell.rule_id) {
            assert!(
                !apply_request_supported(
                    kind,
                    cell.key.target,
                    decision.route,
                    decision.rule_id,
                    decision.support,
                    decision.gate_kind,
                ),
                "closed/preview cell {} ({:?} -> {:?}) must fail ensure_supported",
                cell.rule_id,
                kind,
                cell.key.target
            );
        }
    }
}

#[test]
fn openrouter_host_classifies_as_openai_api_and_binds_three_clients() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "openrouter".into(),
            agent_id: AgentId::Codex,
            name: "OpenRouter".into(),
            settings_config: serde_json::json!({
                "baseUrl": "https://openrouter.ai/api/v1",
                "apiKey": "must-not-leak"
            }),
            meta: serde_json::json!({"preset": "openrouter"}),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterRouteService::new(db);
    assert_eq!(
        service
            .classify_source_product(AdapterSourceKind::Provider, "openrouter")
            .unwrap(),
        crate::models::AdapterSourceProduct::OpenaiApi
    );
    for (target, rule) in [
        (AgentId::Claude, "openai-api-to-claude-v1"),
        (AgentId::Codex, "openai-api-to-codex-v1"),
        (AgentId::Grok, "openai-api-to-grok-bridge-v1"),
    ] {
        let plan = service
            .plan(&request(AdapterSourceKind::Provider, "openrouter", target))
            .unwrap();
        assert_eq!(plan.analysis.route, AdapterRoute::LocalBridge, "{target:?}");
        assert!(plan.can_apply, "{target:?}");
        assert_eq!(plan.analysis.rule_id.as_deref(), Some(rule), "{target:?}");
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("must-not-leak"));
    }
}
