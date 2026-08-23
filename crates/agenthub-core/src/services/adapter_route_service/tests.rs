use super::*;
use crate::models::{
    Account, AccountKind, AdapterCapabilityDecision, AdapterMaturity, AdapterProfile,
    AdapterProfileMode, AdapterProfileStatus, AdapterRoute, AdapterRouteAnalysis,
    AdapterRouteRequest, AdapterServiceImpact, AdapterSourceKind, AdapterSupport, AgentId,
    Provider, ADAPTER_CAPABILITY_MATRIX,
};
use crate::services::adapter_apply_service::apply_request_supported;
use crate::services::AdapterApplyService;
use crate::storage::{AccountRepo, AdapterProfileRepo, Database, ProviderRepo};

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
            credentials: serde_json::json!({"api_key": "must-not-leak"}),
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

fn api_key_account(id: &str, provider: &str) -> Account {
    Account {
        id: id.into(),
        agent_id: AgentId::Claude,
        kind: AccountKind::ApiKey,
        label: format!("{provider} key"),
        credentials: serde_json::json!({"api_key": "must-not-leak"}),
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
    assert_eq!(openai_grok.analysis.route, AdapterRoute::NativeEndpoint);
    assert_eq!(
        openai_grok.analysis.rule_id.as_deref(),
        Some("openai-api-to-grok-v1")
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
    assert_eq!(codex_to_claude.analysis.route, AdapterRoute::Unsupported);
    assert_eq!(
        codex_to_claude.analysis.support,
        AdapterSupport::Unsupported
    );
    assert!(!codex_to_claude.can_apply);
    assert!(codex_to_claude.analysis.reason.contains("现在还接不到"));
    assert!(codex_to_claude.analysis.reason.contains("不会改配置"));
    assert_eq!(
        codex_to_claude.analysis.gate_kind,
        crate::models::AdapterGateKind::SubscriptionCandidate
    );
    assert!(codex_to_claude.changes.is_empty());
    assert!(codex_to_claude.analysis.actions.is_empty());
    assert_eq!(codex_to_claude.service_impact, AdapterServiceImpact::None);

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

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedContractFile {
    cases: Vec<SharedContractCase>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedContractCase {
    id: String,
    source: SharedContractSource,
    target: AgentId,
    expect: SharedContractExpect,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedContractSource {
    kind: AdapterSourceKind,
    agent_id: AgentId,
    preset: Option<String>,
    account_kind: Option<AccountKind>,
    #[allow(dead_code)]
    credential_format: Option<String>,
    extra: Option<serde_json::Value>,
    credentials: Option<serde_json::Value>,
}

/// Production apply entry for a route surface. Distinct from `canApply`:
/// `local_bridge` is plan-open but must not go through `AdapterApplyService`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum SharedContractApplyPath {
    Native,
    LocalBridge,
    ConfigSync,
    Rejected,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SharedContractExpect {
    route: AdapterRoute,
    support: AdapterSupport,
    can_apply: bool,
    #[serde(default)]
    reuse_path: Option<crate::models::AdapterReusePath>,
    apply_path: SharedContractApplyPath,
    rule_id: Option<String>,
    gate_kind: crate::models::AdapterGateKind,
    reason: String,
}

fn shared_capability_contract() -> SharedContractFile {
    serde_json::from_str(include_str!(
        "../../../../../src/dev/mocks/fixtures/adapter-capability-contract.json"
    ))
    .expect("shared adapter capability contract")
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
        match case.source.kind {
            AdapterSourceKind::Provider => {
                ProviderRepo::new(db.clone())
                    .create(&provider(
                        &source_id,
                        case.source.agent_id,
                        case.source.preset.as_deref().unwrap_or("default"),
                    ))
                    .unwrap();
            }
            AdapterSourceKind::Account => {
                AccountRepo::new(db.clone())
                    .create(&Account {
                        id: source_id.clone(),
                        agent_id: case.source.agent_id,
                        kind: case.source.account_kind.unwrap_or(AccountKind::Oauth),
                        label: case.id.clone(),
                        credentials: case
                            .source
                            .credentials
                            .unwrap_or_else(|| serde_json::json!({})),
                        extra: case.source.extra.unwrap_or_else(|| serde_json::json!({})),
                        status: "active".into(),
                        is_current: false,
                        created_at: "now".into(),
                        updated_at: "now".into(),
                    })
                    .unwrap();
            }
        }
        let service = AdapterRouteService::new(db);
        let req = request(case.source.kind, &source_id, case.target);
        let analysis = service.analyze(&req).unwrap();
        let plan = service.plan(&req).unwrap();
        assert_eq!(analysis.route, case.expect.route, "{}", case.id);
        assert_eq!(analysis.support, case.expect.support, "{}", case.id);
        assert_eq!(analysis.rule_id, case.expect.rule_id, "{}", case.id);
        assert_eq!(analysis.gate_kind, case.expect.gate_kind, "{}", case.id);
        assert_eq!(analysis.reason, case.expect.reason, "{}", case.id);
        assert_eq!(plan.can_apply, case.expect.can_apply, "{}", case.id);
        if let Some(reuse_path) = case.expect.reuse_path {
            assert_eq!(plan.reuse_path, reuse_path, "{}", case.id);
        }

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
