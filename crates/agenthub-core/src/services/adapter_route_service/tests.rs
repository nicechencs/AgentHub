use super::*;
use crate::models::{
    Account, AccountKind, AdapterRoute, AdapterServiceImpact, AdapterSupport, Provider,
};
use crate::storage::{AccountRepo, Database, ProviderRepo};

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
    assert_eq!(
        codex.service_impact,
        AdapterServiceImpact::RequiresLocalBridge
    );
    assert!(codex.can_apply);
    assert_eq!(codex.changes.len(), 2);
    assert_eq!(codex.changes[0].target, "codex");
    assert_eq!(codex.changes[0].field, "provider");
    assert_eq!(
        codex.changes[0].value.as_deref(),
        Some("AgentHub Kimi 本地桥接")
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

    let provider_plan = service
        .plan(&request(
            AdapterSourceKind::Provider,
            "anthropic-provider",
            AgentId::Pi,
        ))
        .unwrap();
    assert_eq!(provider_plan.analysis.route, AdapterRoute::ConfigSync);
    assert_eq!(provider_plan.analysis.support, AdapterSupport::Stable);
    assert!(provider_plan.can_apply);
    assert_eq!(provider_plan.changes[0].value.as_deref(), Some("anthropic"));
    assert!(provider_plan.changes[1].secret);
    assert!(provider_plan.changes[1].value.is_none());

    let account_plan = service
        .plan(&request(
            AdapterSourceKind::Account,
            "anthropic-account",
            AgentId::Pi,
        ))
        .unwrap();
    assert_eq!(account_plan.analysis.route, AdapterRoute::ConfigSync);
    assert_eq!(account_plan.analysis.support, AdapterSupport::Stable);
    assert!(
        !account_plan.can_apply,
        "account-sourced Anthropic → Pi stays preview-only"
    );
    assert_eq!(account_plan.changes[0].value.as_deref(), Some("anthropic"));
    assert!(account_plan.changes[1].secret);
    assert!(account_plan.changes[1].value.is_none());
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
    assert!(codex_to_claude.analysis.reason.contains("当前不支持"));
    assert!(codex_to_claude.analysis.reason.contains("门禁"));
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
fn codex_auth_json_account_to_claude_is_matrix_closed() {
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
    assert_eq!(plan.analysis.route, AdapterRoute::Unsupported);
    assert_eq!(plan.analysis.support, AdapterSupport::Unsupported);
    assert!(
        !plan.can_apply,
        "Codex OAuth → Claude must keep can_apply=false"
    );
    assert_eq!(
        plan.analysis.gate_kind,
        crate::models::AdapterGateKind::SubscriptionCandidate
    );
    assert_eq!(
        plan.analysis.rule_id.as_deref(),
        Some("codex-subscription-to-claude-app-server-v0")
    );
    assert_eq!(
        plan.analysis.reason,
        crate::models::CODEX_SUBSCRIPTION_TO_CLAUDE_REASON
    );
    assert!(plan.changes.is_empty());
    assert!(plan.analysis.actions.is_empty());
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
    assert_eq!(matrix.route, AdapterRoute::Unsupported);
    assert!(!matrix.can_apply);
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
    assert_eq!(plan.analysis.rule_id.as_deref(), Some("deepseek-api-to-dsh-v1"));
    assert_eq!(plan.changes[0].field, "provider");
    assert_eq!(plan.changes[0].value.as_deref(), Some("deepseek-official"));
    assert_eq!(plan.changes[1].field, "apiKeyEnv");
    assert_eq!(plan.changes[2].field, "apiKey");
    assert!(plan.changes[2].secret);
    assert!(plan.changes[2].value.is_none());
    assert!(!serde_json::to_string(&plan).unwrap().contains("must-not-leak"));
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
    assert_eq!(plan.analysis.rule_id.as_deref(), Some("deepseek-api-to-dsh-v1"));
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
fn deepseek_to_claude_plan_is_unsupported() {
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
    assert_eq!(plan.analysis.route, AdapterRoute::Unsupported);
    assert!(!plan.can_apply);
    assert!(plan.analysis.reason.contains("另立项"));
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
