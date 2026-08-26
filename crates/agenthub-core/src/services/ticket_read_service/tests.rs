use super::*;
use crate::models::{
    parse_ticket_id, Account, AccountKind, AdapterProfile, AdapterProfileMode,
    AdapterProfileStatus, AdapterRoute, AdapterSourceKind, AdapterSupport, AgentId,
    PersistedTicketSurface, Provider, Ticket, TicketBindingRoute, TicketCredentialClass,
    TicketPlanRequest, TicketProtocol, TicketSurface, PROJECTION_NOT_A_TICKET,
};
use crate::services::ConnectionService;
use crate::storage::{AccountRepo, ActiveBindingRepo, AdapterProfileRepo, Database, ProviderRepo};

fn test_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("ticket-read.db")).unwrap();
    (dir, db)
}

fn provider(id: &str, agent: AgentId, name: &str, preset: &str, current: bool) -> Provider {
    Provider {
        id: id.into(),
        agent_id: agent,
        name: name.into(),
        settings_config: serde_json::json!({"api_key": "must-not-leak"}),
        meta: serde_json::json!({"preset": preset}),
        is_current: current,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

fn account(id: &str, agent: AgentId, kind: AccountKind, label: &str, current: bool) -> Account {
    Account {
        id: id.into(),
        agent_id: agent,
        kind,
        label: label.into(),
        credentials: serde_json::json!({"format": "auth_json", "tokens": {"access_token": "x"}}),
        extra: serde_json::json!({}),
        status: "active".into(),
        is_current: current,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

fn profile(
    id: &str,
    source_kind: AdapterSourceKind,
    source_id: &str,
    target: AgentId,
    route: AdapterRoute,
    generated: Option<&str>,
    port: Option<u16>,
) -> AdapterProfile {
    AdapterProfile {
        id: id.into(),
        name: format!("profile-{id}"),
        source_kind,
        source_id: source_id.into(),
        target_agent_id: target,
        route,
        mode: AdapterProfileMode::Api,
        status: AdapterProfileStatus::Active,
        rule_id: "rule".into(),
        rule_version: "v1".into(),
        generated_provider_id: generated.map(str::to_owned),
        local_port: port,
        auto_start: false,
        last_error_code: None,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

#[test]
fn generated_projection_providers_are_excluded_from_tickets() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "kimi-src",
            AgentId::Kimi,
            "Kimi membership",
            "kimi-code-membership",
            false,
        ))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "proj-claude",
            AgentId::Claude,
            "Generated Claude",
            "custom",
            true,
        ))
        .unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&profile(
            "p1",
            AdapterSourceKind::Provider,
            "kimi-src",
            AgentId::Claude,
            AdapterRoute::NativeEndpoint,
            Some("proj-claude"),
            None,
        ))
        .unwrap();

    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    let ids: Vec<_> = wallet.tickets.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, vec!["provider:kimi-src"]);
    assert!(!ids.iter().any(|id| id.contains("proj-claude")));
    assert!(wallet.surface_groups.iter().all(|group| group
        .members
        .iter()
        .all(|member| member.source_id != "proj-claude")));
}

#[test]
fn generated_projection_accounts_are_excluded_from_tickets() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "claude-bridge-acc".into(),
            agent_id: AgentId::Claude,
            kind: AccountKind::ApiKey,
            label: "loopback leftover".into(),
            credentials: serde_json::json!({
                "format": "api_key",
                "api_key": "ahb_local",
                "base_url": "http://127.0.0.1:43081"
            }),
            extra: serde_json::json!({"source": "live"}),
            status: "active".into(),
            is_current: false,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&account(
            "codex-oauth",
            AgentId::Codex,
            AccountKind::Oauth,
            "me@example.com",
            false,
        ))
        .unwrap();

    let wallet = TicketReadService::new(db.clone()).list_wallet().unwrap();
    let ids: Vec<_> = wallet.tickets.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, vec!["account:codex-oauth"]);

    let error = TicketReadService::new(db)
        .plan(&TicketPlanRequest {
            ticket_id: "account:claude-bridge-acc".into(),
            target_agent_id: AgentId::Pi,
        })
        .unwrap_err();
    assert!(error.to_string().contains(PROJECTION_NOT_A_TICKET));
}

#[test]
fn current_generated_provider_does_not_hide_unrelated_account_tickets() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&account(
            "claude-oauth",
            AgentId::Claude,
            AccountKind::Oauth,
            "a@x.com",
            false,
        ))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "claude-gen".into(),
            agent_id: AgentId::Claude,
            name: "Generated Claude".into(),
            settings_config: serde_json::json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "http://127.0.0.1:43081",
                    "ANTHROPIC_AUTH_TOKEN": "ahb_local"
                }
            }),
            meta: serde_json::json!({
                "generatedBy": "adapter",
                "adapterBridge": { "loopbackOnly": true }
            }),
            is_current: true,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();

    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    let ids: Vec<_> = wallet.tickets.iter().map(|t| t.id.as_str()).collect();
    assert!(
        ids.contains(&"account:claude-oauth"),
        "source oauth must stay listed while a generated provider is current: {ids:?}"
    );
    assert!(!ids.iter().any(|id| *id == "provider:claude-gen"));
}

#[test]
fn current_generated_provider_becomes_active_reshape_or_bridge_on_source_ticket() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "kimi-src",
            AgentId::Kimi,
            "Kimi membership",
            "kimi-code-membership",
            false,
        ))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "proj-claude",
            AgentId::Claude,
            "Generated",
            "custom",
            true,
        ))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "proj-codex",
            AgentId::Codex,
            "Generated bridge",
            "custom",
            true,
        ))
        .unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&profile(
            "reshape-p",
            AdapterSourceKind::Provider,
            "kimi-src",
            AgentId::Claude,
            AdapterRoute::NativeEndpoint,
            Some("proj-claude"),
            None,
        ))
        .unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&profile(
            "bridge-p",
            AdapterSourceKind::Provider,
            "kimi-src",
            AgentId::Codex,
            AdapterRoute::LocalBridge,
            Some("proj-codex"),
            Some(43121),
        ))
        .unwrap();

    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    assert_eq!(wallet.tickets.len(), 1);

    let reshape = wallet
        .bindings
        .iter()
        .find(|b| b.profile_id.as_deref() == Some("reshape-p"))
        .expect("reshape binding");
    assert_eq!(reshape.ticket_id, "provider:kimi-src");
    assert_eq!(reshape.agent_id, AgentId::Claude);
    assert_eq!(reshape.route, TicketBindingRoute::Reshape);
    assert!(reshape.active);
    assert!(reshape.bridge.is_none());

    let bridge = wallet
        .bindings
        .iter()
        .find(|b| b.profile_id.as_deref() == Some("bridge-p"))
        .expect("bridge binding");
    assert_eq!(bridge.route, TicketBindingRoute::Bridge);
    assert!(bridge.active);
    assert_eq!(
        bridge.bridge,
        Some(TicketBridgeRuntime {
            port: Some(43121),
            running: false,
        })
    );
}

#[test]
fn ordinary_current_provider_and_account_produce_native_active_bindings() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "anth",
            AgentId::Claude,
            "Anthropic",
            "anthropic",
            true,
        ))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&account(
            "codex-oauth",
            AgentId::Codex,
            AccountKind::Oauth,
            "me@example.com",
            true,
        ))
        .unwrap();

    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    assert!(wallet
        .tickets
        .iter()
        .any(|t| t.id == "provider:anth" && t.surface == TicketSurface::AnthropicApi));
    assert!(wallet.tickets.iter().any(|t| {
        t.id == "account:codex-oauth"
            && t.surface == TicketSurface::CodexChatgptSubscription
            && t.credential_class == TicketCredentialClass::Oauth
            && t.speaks
                == vec![
                    TicketProtocol::OpenaiResponses,
                    TicketProtocol::OpenaiCodexPkce,
                ]
    }));

    let provider_binding = wallet
        .bindings
        .iter()
        .find(|b| b.ticket_id == "provider:anth")
        .unwrap();
    assert_eq!(provider_binding.route, TicketBindingRoute::Native);
    assert!(provider_binding.active);
    assert_eq!(provider_binding.agent_id, AgentId::Claude);

    let account_binding = wallet
        .bindings
        .iter()
        .find(|b| b.ticket_id == "account:codex-oauth")
        .unwrap();
    assert_eq!(account_binding.route, TicketBindingRoute::Native);
    assert!(account_binding.active);
    assert_eq!(account_binding.agent_id, AgentId::Codex);
}

#[test]
fn claude_and_grok_oauth_accounts_have_subscription_surfaces_and_contract_speaks() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&account(
            "claude-oauth",
            AgentId::Claude,
            AccountKind::Oauth,
            "Claude subscription",
            false,
        ))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&account(
            "grok-oauth",
            AgentId::Grok,
            AccountKind::Oauth,
            "Grok subscription",
            false,
        ))
        .unwrap();

    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    let claude = wallet
        .tickets
        .iter()
        .find(|ticket| ticket.id == "account:claude-oauth")
        .unwrap();
    assert_eq!(claude.surface, TicketSurface::ClaudeSubscription);
    assert!(claude.speaks.contains(&TicketProtocol::AnthropicMessages));
    assert!(claude.speaks.contains(&TicketProtocol::AnthropicPkce));

    let grok = wallet
        .tickets
        .iter()
        .find(|ticket| ticket.id == "account:grok-oauth")
        .unwrap();
    assert_eq!(grok.surface, TicketSurface::GrokXaiSubscription);
    assert!(grok.speaks.contains(&TicketProtocol::OpenaiResponses));
    assert!(grok.speaks.contains(&TicketProtocol::OpenaiChat));
    assert!(grok.speaks.contains(&TicketProtocol::XaiDeviceCode));
}

#[test]
fn unknown_surface_ticket_has_empty_speaks() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "relay",
            AgentId::Claude,
            "Custom relay",
            "openai-compatible",
            false,
        ))
        .unwrap();

    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    let ticket = wallet
        .tickets
        .iter()
        .find(|t| t.id == "provider:relay")
        .unwrap();
    assert_eq!(ticket.surface, TicketSurface::Unknown);
    assert!(ticket.speaks.is_empty());
    assert_eq!(ticket.credential_class, TicketCredentialClass::ApiKey);
}

#[test]
fn custom_remote_without_official_evidence_stays_unknown() {
    let (_dir, db) = test_db();
    let mut row = provider(
        "codex-mytokens",
        AgentId::Codex,
        "OpenAI · gpt-5.5",
        "openai-compatible",
        false,
    );
    row.settings_config = serde_json::json!({
        "format": "toml",
        "content": "model_provider = \"OpenAI\"\nmodel = \"gpt-5.5\"\n\n[model_providers.OpenAI]\nname = \"OpenAI\"\nbase_url = \"https://mytokens.cc/v1\"\n"
    });
    ProviderRepo::new(db.clone()).create(&row).unwrap();

    let wallet = TicketReadService::new(db.clone()).list_wallet().unwrap();
    let ticket = wallet
        .tickets
        .iter()
        .find(|t| t.id == "provider:codex-mytokens")
        .unwrap();
    assert_eq!(ticket.surface, TicketSurface::Unknown);
    assert!(ticket.speaks.is_empty());
    assert_eq!(ticket.credential_class, TicketCredentialClass::ApiKey);
    assert_ne!(ticket.label, "未识别");

    let stored = ProviderRepo::new(db)
        .get_by_id("codex-mytokens")
        .unwrap()
        .unwrap();
    assert!(stored.meta.get("surface").is_none());
}

#[test]
fn list_wallet_does_not_reclassify_persisted_unknown_provider_surface() {
    let (_dir, db) = test_db();
    let mut row = provider(
        "relay-unknown",
        AgentId::Codex,
        "OpenAI-compatible relay",
        "openai-compatible",
        false,
    );
    row.meta["surface"] = serde_json::json!("unknown");
    row.settings_config = serde_json::json!({
        "base_url": "https://api.openai.com.evil.example/v1",
        "comment": "https://api.openai.com/v1"
    });
    ProviderRepo::new(db.clone()).create(&row).unwrap();

    let wallet = TicketReadService::new(db.clone()).list_wallet().unwrap();
    let ticket = wallet
        .tickets
        .iter()
        .find(|ticket| ticket.id == "provider:relay-unknown")
        .unwrap();
    assert_eq!(ticket.surface, TicketSurface::Unknown);
    assert!(ticket.speaks.is_empty());

    let stored = ProviderRepo::new(db)
        .get_by_id("relay-unknown")
        .unwrap()
        .unwrap();
    assert_eq!(stored.meta["surface"], "unknown");
}

#[test]
fn applying_or_failed_bridge_without_port_is_hidden_from_usage_lines() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&account(
            "grok-live",
            AgentId::Grok,
            AccountKind::Oauth,
            "Grok subscription",
            true,
        ))
        .unwrap();
    let mut applying = profile(
        "adapter-grok-claude-applying",
        AdapterSourceKind::Account,
        "grok-live",
        AgentId::Claude,
        AdapterRoute::LocalBridge,
        Some("claude-grok-adapter-bridge"),
        None,
    );
    applying.status = AdapterProfileStatus::Applying;
    applying.last_error_code = Some("retryable:adapter.bridge_projection".into());
    AdapterProfileRepo::new(db.clone())
        .create(&applying)
        .unwrap();

    let wallet = TicketReadService::new(db.clone()).list_wallet().unwrap();
    assert!(
        wallet
            .bindings
            .iter()
            .all(|binding| binding.profile_id.as_deref() != Some("adapter-grok-claude-applying")),
        "applying-without-port must not appear as 正用于: {:?}",
        wallet.bindings
    );
    assert!(wallet.bindings.iter().any(|binding| {
        binding.ticket_id == "account:grok-live"
            && binding.agent_id == AgentId::Grok
            && binding.active
    }));

    let mut failed = applying;
    failed.status = AdapterProfileStatus::NeedsAttention;
    AdapterProfileRepo::new(db.clone()).update(&failed).unwrap();
    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    assert!(
        wallet
            .bindings
            .iter()
            .all(|binding| binding.profile_id.as_deref() != Some("adapter-grok-claude-applying")),
        "failed-without-port must not appear as 正用于: {:?}",
        wallet.bindings
    );
}

#[test]
fn profile_with_missing_source_row_is_skipped() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "orphan-proj",
            AgentId::Claude,
            "Orphan projection",
            "custom",
            true,
        ))
        .unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&profile(
            "orphan-p",
            AdapterSourceKind::Provider,
            "deleted-source",
            AgentId::Claude,
            AdapterRoute::ConfigSync,
            Some("orphan-proj"),
            None,
        ))
        .unwrap();

    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    assert!(
        wallet.tickets.is_empty(),
        "projection excluded and no source"
    );
    assert!(
        wallet.bindings.is_empty(),
        "must not synthesize ghost bindings: {:?}",
        wallet.bindings
    );
}

#[test]
fn each_agent_has_at_most_one_active_binding_provider_wins() {
    let (_dir, db) = test_db();
    // Both current on Claude — the Claude OAuth account is now a subscription
    // ticket, but provider still wins the active binding.
    AccountRepo::new(db.clone())
        .create(&account(
            "claude-acct",
            AgentId::Claude,
            AccountKind::Oauth,
            "oauth",
            true,
        ))
        .unwrap();
    // Force oauth account classify as Other (not Codex) so surface is unknown.
    let mut acct = AccountRepo::new(db.clone())
        .get_by_id("claude-acct")
        .unwrap()
        .unwrap();
    acct.credentials = serde_json::json!({"token": "x"});
    AccountRepo::new(db.clone()).update(&acct).unwrap();

    ProviderRepo::new(db.clone())
        .create(&provider(
            "anth",
            AgentId::Claude,
            "Anthropic",
            "anthropic",
            true,
        ))
        .unwrap();

    // Inactive profile on Claude from a real source (not current).
    ProviderRepo::new(db.clone())
        .create(&provider(
            "kimi-src",
            AgentId::Kimi,
            "Kimi",
            "kimi-code-membership",
            false,
        ))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "proj-idle",
            AgentId::Claude,
            "Idle projection",
            "custom",
            false,
        ))
        .unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&profile(
            "idle-p",
            AdapterSourceKind::Provider,
            "kimi-src",
            AgentId::Claude,
            AdapterRoute::NativeEndpoint,
            Some("proj-idle"),
            None,
        ))
        .unwrap();

    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    let claude_active: Vec<_> = wallet
        .bindings
        .iter()
        .filter(|b| b.agent_id == AgentId::Claude && b.active)
        .collect();
    assert_eq!(claude_active.len(), 1);
    assert_eq!(claude_active[0].ticket_id, "provider:anth");
    assert_eq!(claude_active[0].route, TicketBindingRoute::Native);

    let idle = wallet
        .bindings
        .iter()
        .find(|b| b.profile_id.as_deref() == Some("idle-p"))
        .unwrap();
    assert!(!idle.active);
    assert_eq!(idle.route, TicketBindingRoute::Reshape);
}

#[test]
fn plan_ticket_parses_id_and_delegates() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "kimi-src",
            AgentId::Kimi,
            "Kimi",
            "kimi-code-membership",
            false,
        ))
        .unwrap();
    let service = TicketReadService::new(db);

    let plan = service
        .plan(&TicketPlanRequest {
            ticket_id: "provider:kimi-src".into(),
            target_agent_id: AgentId::Claude,
        })
        .unwrap();
    assert_eq!(plan.analysis.route, AdapterRoute::NativeEndpoint);
    assert_eq!(plan.analysis.support, AdapterSupport::Stable);
    assert!(plan.can_apply);
    assert_eq!(plan.target_agent_id, AgentId::Claude);
}

#[test]
fn plan_ticket_rejects_invalid_prefix_and_missing_row() {
    let (_dir, db) = test_db();
    let service = TicketReadService::new(db);

    let bad_prefix = service.plan(&TicketPlanRequest {
        ticket_id: "credential:x".into(),
        target_agent_id: AgentId::Claude,
    });
    assert!(matches!(bad_prefix, Err(AppError::InvalidArg(_))));

    let no_colon = service.plan(&TicketPlanRequest {
        ticket_id: "provider".into(),
        target_agent_id: AgentId::Claude,
    });
    assert!(matches!(no_colon, Err(AppError::InvalidArg(_))));

    let missing = service.plan(&TicketPlanRequest {
        ticket_id: "provider:does-not-exist".into(),
        target_agent_id: AgentId::Claude,
    });
    assert!(matches!(missing, Err(AppError::NotFound(_))));
}

#[test]
fn wallet_wire_shape_uses_camel_case_and_kebab_surfaces() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "kimi-src",
            AgentId::Kimi,
            "Kimi",
            "kimi-code-membership",
            false,
        ))
        .unwrap();
    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    let v = serde_json::to_value(&wallet).unwrap();
    let ticket = &v["tickets"][0];
    assert_eq!(ticket["id"], "provider:kimi-src");
    assert_eq!(ticket["sourceKind"], "provider");
    assert_eq!(ticket["sourceId"], "kimi-src");
    assert_eq!(ticket["agentId"], "kimi");
    assert_eq!(ticket["surface"], "kimi-code-membership");
    assert_eq!(ticket["credentialClass"], "api_key");
    assert_eq!(
        ticket["speaks"],
        serde_json::json!(["anthropic-messages", "openai-chat"])
    );
    assert_eq!(ticket["importedFrom"], "kimi");
    let group = &v["surfaceGroups"][0];
    assert_eq!(group["surface"], "kimi-code-membership");
    assert_eq!(group["credentialClass"], "api_key");
    assert_eq!(group["members"][0]["ticketId"], "provider:kimi-src");
    assert_eq!(group["members"][0]["sourceKind"], "provider");
    assert_eq!(group["members"][0]["sourceId"], "kimi-src");
    assert!(!serde_json::to_string(&wallet)
        .unwrap()
        .contains("must-not-leak"));
}

#[test]
fn parse_ticket_id_helpers() {
    assert_eq!(
        parse_ticket_id("account:abc").unwrap(),
        (AdapterSourceKind::Account, "abc".into())
    );
    assert_eq!(
        parse_ticket_id("provider:x:y").unwrap(),
        (AdapterSourceKind::Provider, "x:y".into())
    );
    assert!(parse_ticket_id("foo:bar").is_err());
    assert!(parse_ticket_id("provider:").is_err());
}

#[test]
fn ticket_surface_serde_matches_wire() {
    assert_eq!(
        serde_json::to_value(TicketSurface::CodexChatgptSubscription).unwrap(),
        serde_json::json!("codex-chatgpt-subscription")
    );
    assert_eq!(
        serde_json::to_value(TicketSurface::ClaudeSubscription).unwrap(),
        serde_json::json!("claude-subscription")
    );
    assert_eq!(
        serde_json::to_value(TicketSurface::GrokXaiSubscription).unwrap(),
        serde_json::json!("grok-xai-subscription")
    );
    assert_eq!(
        serde_json::to_value(TicketCredentialClass::ApiKey).unwrap(),
        serde_json::json!("api_key")
    );
    assert_eq!(
        serde_json::to_value(TicketBindingRoute::Reshape).unwrap(),
        serde_json::json!("reshape")
    );
    assert_eq!(
        TicketSurface::parse("kimi-code-membership"),
        Some(TicketSurface::KimiCodeMembership)
    );
    assert_eq!(
        TicketSurface::parse("openai-api"),
        Some(TicketSurface::OpenaiApi)
    );
    assert_eq!(TicketSurface::parse("xai-api"), Some(TicketSurface::XaiApi));
    assert_eq!(
        TicketSurface::parse("glm-coding-plan"),
        Some(TicketSurface::GlmCodingPlan)
    );
    assert_eq!(
        TicketSurface::parse("deepseek-api"),
        Some(TicketSurface::DeepseekApi)
    );
    assert_eq!(
        TicketSurface::parse("claude-subscription"),
        Some(TicketSurface::ClaudeSubscription)
    );
    assert_eq!(
        TicketSurface::parse("grok-xai-subscription"),
        Some(TicketSurface::GrokXaiSubscription)
    );
    assert_eq!(
        TicketSurface::OpenaiApi.speaks(),
        &[TicketProtocol::OpenaiChat]
    );
    assert_eq!(
        TicketSurface::GlmCodingPlan.speaks(),
        &[
            TicketProtocol::AnthropicMessages,
            TicketProtocol::OpenaiChat,
            TicketProtocol::OpenaiResponses
        ]
    );
    assert_eq!(
        TicketSurface::DeepseekApi.speaks(),
        &[
            TicketProtocol::AnthropicMessages,
            TicketProtocol::OpenaiChat,
            TicketProtocol::OpenaiResponses
        ]
    );
    assert_eq!(TicketSurface::parse("not-a-surface"), None);
    assert_eq!(
        TicketSurface::from_persisted_json(&serde_json::json!({})),
        PersistedTicketSurface::Missing
    );
    assert_eq!(
        TicketSurface::from_persisted_json(&serde_json::json!({"other": 1})),
        PersistedTicketSurface::Missing
    );
    assert_eq!(
        TicketSurface::from_persisted_json(&serde_json::json!({"surface": "anthropic-api"})),
        PersistedTicketSurface::Known(TicketSurface::AnthropicApi)
    );
    assert_eq!(
        TicketSurface::from_persisted_json(&serde_json::json!({"surface": "unknown"})),
        PersistedTicketSurface::Known(TicketSurface::Unknown)
    );
    assert_eq!(
        TicketSurface::from_persisted_json(&serde_json::json!({"surface": "future-surface-v2"})),
        PersistedTicketSurface::Unrecognized
    );
    assert_eq!(
        TicketSurface::from_persisted_json(&serde_json::json!({"surface": 1})),
        PersistedTicketSurface::Unrecognized
    );
}

#[test]
fn list_wallet_classifies_missing_surface_without_writing_back() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "anth",
            AgentId::Claude,
            "Anthropic",
            "anthropic",
            false,
        ))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&account(
            "codex-oauth",
            AgentId::Codex,
            AccountKind::Oauth,
            "me@example.com",
            false,
        ))
        .unwrap();

    let service = TicketReadService::new(db.clone());
    let first = service.list_wallet().unwrap();
    let anth = first
        .tickets
        .iter()
        .find(|t| t.id == "provider:anth")
        .unwrap();
    assert_eq!(anth.surface, TicketSurface::AnthropicApi);
    let oauth = first
        .tickets
        .iter()
        .find(|t| t.id == "account:codex-oauth")
        .unwrap();
    assert_eq!(oauth.surface, TicketSurface::CodexChatgptSubscription);

    let stored_provider = ProviderRepo::new(db.clone())
        .get_by_id("anth")
        .unwrap()
        .unwrap();
    assert!(stored_provider.meta.get("surface").is_none());
    let stored_account = AccountRepo::new(db.clone())
        .get_by_id("codex-oauth")
        .unwrap()
        .unwrap();
    assert!(stored_account.extra.get("surface").is_none());

    let second = TicketReadService::new(db).list_wallet().unwrap();
    assert!(second
        .tickets
        .iter()
        .any(|t| t.id == "provider:anth" && t.surface == TicketSurface::AnthropicApi));
    assert!(second.tickets.iter().any(|t| {
        t.id == "account:codex-oauth" && t.surface == TicketSurface::CodexChatgptSubscription
    }));
}

#[test]
fn list_wallet_prefers_persisted_surface_over_classify() {
    let (_dir, db) = test_db();
    let mut row = provider(
        "relay",
        AgentId::Claude,
        "Custom relay",
        "openai-compatible",
        false,
    );
    row.meta = serde_json::json!({
        "preset": "openai-compatible",
        "surface": "kimi-code-membership"
    });
    ProviderRepo::new(db.clone()).create(&row).unwrap();

    let wallet = TicketReadService::new(db.clone()).list_wallet().unwrap();
    let ticket = wallet
        .tickets
        .iter()
        .find(|t| t.id == "provider:relay")
        .unwrap();
    assert_eq!(ticket.surface, TicketSurface::KimiCodeMembership);
    assert_eq!(
        ticket.speaks,
        vec![
            TicketProtocol::AnthropicMessages,
            TicketProtocol::OpenaiChat
        ]
    );
    let stored = ProviderRepo::new(db).get_by_id("relay").unwrap().unwrap();
    assert_eq!(stored.meta["surface"], "kimi-code-membership");
}

#[test]
fn list_wallet_unrecognized_surface_displays_unknown_without_overwrite() {
    let (_dir, db) = test_db();
    let mut row = provider(
        "relay",
        AgentId::Claude,
        "Custom relay",
        "openai-compatible",
        true,
    );
    row.settings_config = serde_json::json!({
        "api_key": "must-not-leak",
        "keep": true
    });
    row.meta = serde_json::json!({
        "preset": "openai-compatible",
        "surface": "future-surface-v2"
    });
    ProviderRepo::new(db.clone()).create(&row).unwrap();

    let wallet = TicketReadService::new(db.clone()).list_wallet().unwrap();
    let ticket = wallet
        .tickets
        .iter()
        .find(|t| t.id == "provider:relay")
        .unwrap();
    assert_eq!(ticket.surface, TicketSurface::Unknown);
    assert!(ticket.speaks.is_empty());

    let stored = ProviderRepo::new(db).get_by_id("relay").unwrap().unwrap();
    assert_eq!(stored.meta["surface"], "future-surface-v2");
    assert!(stored.is_current);
    assert_eq!(stored.settings_config["keep"], true);
    assert_eq!(stored.name, "Custom relay");
}

#[test]
fn list_wallet_surface_backfill_does_not_touch_current_or_settings() {
    let (_dir, db) = test_db();
    let mut row = provider("anth", AgentId::Claude, "Anthropic", "anthropic", true);
    row.settings_config = serde_json::json!({
        "api_key": "must-not-leak",
        "base_url": "https://keep.example"
    });
    ProviderRepo::new(db.clone()).create(&row).unwrap();

    let wallet = TicketReadService::new(db.clone()).list_wallet().unwrap();
    assert_eq!(
        wallet
            .tickets
            .iter()
            .find(|t| t.id == "provider:anth")
            .unwrap()
            .surface,
        TicketSurface::AnthropicApi
    );
    let stored = ProviderRepo::new(db).get_by_id("anth").unwrap().unwrap();
    assert!(stored.meta.get("surface").is_none());
    assert!(stored.is_current);
    assert_eq!(stored.settings_config["base_url"], "https://keep.example");
    assert_eq!(stored.name, "Anthropic");
}

#[test]
fn list_wallet_unrecognized_account_surface_skips_writeback() {
    let (_dir, db) = test_db();
    let mut row = account(
        "legacy",
        AgentId::Codex,
        AccountKind::Oauth,
        "me@example.com",
        true,
    );
    row.extra = serde_json::json!({"surface": "future-account-surface"});
    AccountRepo::new(db.clone()).create(&row).unwrap();

    let wallet = TicketReadService::new(db.clone()).list_wallet().unwrap();
    let ticket = wallet
        .tickets
        .iter()
        .find(|t| t.id == "account:legacy")
        .unwrap();
    assert_eq!(ticket.surface, TicketSurface::Unknown);

    let stored = AccountRepo::new(db).get_by_id("legacy").unwrap().unwrap();
    assert_eq!(stored.extra["surface"], "future-account-surface");
    assert!(stored.is_current);
}

#[test]
fn list_wallet_unknown_surface_reclassifies_and_only_writes_known_result() {
    let (_dir, db) = test_db();
    let mut claude = account(
        "claude-unknown",
        AgentId::Claude,
        AccountKind::Oauth,
        "Claude",
        false,
    );
    claude.extra = serde_json::json!({"surface": "unknown"});
    AccountRepo::new(db.clone()).create(&claude).unwrap();

    let mut relay = account(
        "relay-unknown",
        AgentId::Kimi,
        AccountKind::Oauth,
        "Relay",
        false,
    );
    relay.credentials = serde_json::json!({"token": "must-not-leak"});
    relay.extra = serde_json::json!({"surface": "unknown", "keep": true});
    AccountRepo::new(db.clone()).create(&relay).unwrap();

    let wallet = TicketReadService::new(db.clone()).list_wallet().unwrap();
    assert_eq!(
        wallet
            .tickets
            .iter()
            .find(|ticket| ticket.id == "account:claude-unknown")
            .unwrap()
            .surface,
        TicketSurface::ClaudeSubscription
    );
    assert_eq!(
        wallet
            .tickets
            .iter()
            .find(|ticket| ticket.id == "account:relay-unknown")
            .unwrap()
            .surface,
        TicketSurface::Unknown
    );

    let stored_claude = AccountRepo::new(db.clone())
        .get_by_id("claude-unknown")
        .unwrap()
        .unwrap();
    assert_eq!(stored_claude.extra["surface"], "unknown");
    let stored_relay = AccountRepo::new(db)
        .get_by_id("relay-unknown")
        .unwrap()
        .unwrap();
    assert_eq!(stored_relay.extra["surface"], "unknown");
    assert_eq!(stored_relay.extra["keep"], true);
}

#[test]
fn plan_ticket_rejects_generated_projection_provider() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "kimi-src",
            AgentId::Kimi,
            "Kimi",
            "kimi-code-membership",
            false,
        ))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "proj-claude",
            AgentId::Claude,
            "Generated",
            "custom",
            false,
        ))
        .unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&profile(
            "p1",
            AdapterSourceKind::Provider,
            "kimi-src",
            AgentId::Claude,
            AdapterRoute::NativeEndpoint,
            Some("proj-claude"),
            None,
        ))
        .unwrap();

    let service = TicketReadService::new(db);
    let err = service
        .plan(&TicketPlanRequest {
            ticket_id: "provider:proj-claude".into(),
            target_agent_id: AgentId::Pi,
        })
        .unwrap_err();
    assert!(matches!(err, AppError::InvalidArg(_)));
    assert!(err.to_string().contains(PROJECTION_NOT_A_TICKET));
}

#[test]
fn plan_ticket_rejects_generated_by_adapter_meta() {
    let (_dir, db) = test_db();
    let mut generated = provider(
        "orphan-gen",
        AgentId::Claude,
        "Orphan generated",
        "custom",
        false,
    );
    generated.meta = serde_json::json!({
        "preset": "custom",
        "generatedBy": "adapter"
    });
    ProviderRepo::new(db.clone()).create(&generated).unwrap();

    let err = TicketReadService::new(db)
        .plan(&TicketPlanRequest {
            ticket_id: "provider:orphan-gen".into(),
            target_agent_id: AgentId::Pi,
        })
        .unwrap_err();
    assert!(matches!(err, AppError::InvalidArg(_)));
    assert!(err.to_string().contains(PROJECTION_NOT_A_TICKET));
}

const CODEX_LEFTOVER_TOML: &str = r#"model_provider = "agenthub_grok_bridge"
model = "grok-4"
preferred_auth_method = "apikey"

[model_providers.agenthub_grok_bridge]
name = "AgentHub Grok Route"
base_url = "http://127.0.0.1:43121/v1"
wire_api = "responses"
"#;

#[test]
fn orphan_generated_by_adapter_provider_is_not_a_ticket() {
    let (_dir, db) = test_db();
    let mut generated = provider(
        "orphan-gen",
        AgentId::Claude,
        "Orphan generated",
        "custom",
        true,
    );
    generated.meta = serde_json::json!({
        "preset": "custom",
        "generatedBy": "adapter"
    });
    ProviderRepo::new(db.clone()).create(&generated).unwrap();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "kimi-src",
            AgentId::Kimi,
            "Kimi membership",
            "kimi-code-membership",
            false,
        ))
        .unwrap();

    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    let ids: Vec<_> = wallet.tickets.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, vec!["provider:kimi-src"]);
    assert!(!wallet
        .tickets
        .iter()
        .any(|ticket| ticket.source_id == "orphan-gen"));
    assert!(!wallet
        .bindings
        .iter()
        .any(|binding| binding.ticket_id == "provider:orphan-gen"));
}

#[test]
fn orphan_codex_leftover_provider_is_not_a_ticket() {
    let (_dir, db) = test_db();
    let mut leftover = provider(
        "codex-leftover",
        AgentId::Codex,
        "AgentHub Grok Route",
        "custom",
        true,
    );
    leftover.settings_config = serde_json::json!({
        "format": "toml",
        "content": CODEX_LEFTOVER_TOML
    });
    leftover.meta = serde_json::json!({
        "adapterBridge": { "loopbackOnly": true }
    });
    ProviderRepo::new(db.clone()).create(&leftover).unwrap();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "kimi-src",
            AgentId::Kimi,
            "Kimi membership",
            "kimi-code-membership",
            false,
        ))
        .unwrap();

    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    let ids: Vec<_> = wallet.tickets.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, vec!["provider:kimi-src"]);
    assert!(!wallet
        .tickets
        .iter()
        .any(|ticket| ticket.source_id == "codex-leftover"));
    assert!(!wallet
        .bindings
        .iter()
        .any(|binding| binding.ticket_id == "provider:codex-leftover"));
}

#[test]
fn orphan_codex_leftover_toml_only_is_not_a_ticket() {
    let (_dir, db) = test_db();
    let mut leftover = provider(
        "codex-leftover",
        AgentId::Codex,
        "AgentHub Grok Route",
        "custom",
        false,
    );
    leftover.settings_config = serde_json::json!({
        "format": "toml",
        "content": CODEX_LEFTOVER_TOML
    });
    leftover.meta = serde_json::json!({"preset": "custom"});
    ProviderRepo::new(db.clone()).create(&leftover).unwrap();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "kimi-src",
            AgentId::Kimi,
            "Kimi membership",
            "kimi-code-membership",
            false,
        ))
        .unwrap();

    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    let ids: Vec<_> = wallet.tickets.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, vec!["provider:kimi-src"]);
    assert!(!wallet
        .tickets
        .iter()
        .any(|ticket| ticket.source_id == "codex-leftover"));
    assert!(!wallet
        .bindings
        .iter()
        .any(|binding| binding.ticket_id == "provider:codex-leftover"));
}

#[test]
fn plan_ticket_rejects_codex_leftover_toml_only() {
    let (_dir, db) = test_db();
    let mut leftover = provider(
        "codex-leftover",
        AgentId::Codex,
        "AgentHub Grok Route",
        "custom",
        false,
    );
    leftover.settings_config = serde_json::json!({
        "format": "toml",
        "content": CODEX_LEFTOVER_TOML
    });
    leftover.meta = serde_json::json!({"preset": "custom"});
    ProviderRepo::new(db.clone()).create(&leftover).unwrap();

    let err = TicketReadService::new(db)
        .plan(&TicketPlanRequest {
            ticket_id: "provider:codex-leftover".into(),
            target_agent_id: AgentId::Pi,
        })
        .unwrap_err();
    assert!(matches!(err, AppError::InvalidArg(_)));
    assert!(err.to_string().contains(PROJECTION_NOT_A_TICKET));
}

#[test]
fn current_codex_leftover_does_not_become_native_ticket_when_oauth_account_current() {
    let (_dir, db) = test_db();
    let mut leftover = provider(
        "codex-leftover",
        AgentId::Codex,
        "AgentHub Grok Route",
        "custom",
        true,
    );
    leftover.settings_config = serde_json::json!({
        "format": "toml",
        "content": CODEX_LEFTOVER_TOML
    });
    leftover.meta = serde_json::json!({"preset": "custom"});
    ProviderRepo::new(db.clone()).create(&leftover).unwrap();
    AccountRepo::new(db.clone())
        .create(&account(
            "codex-oauth",
            AgentId::Codex,
            AccountKind::Oauth,
            "me@example.com",
            true,
        ))
        .unwrap();

    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    assert!(wallet
        .tickets
        .iter()
        .any(|ticket| ticket.id == "account:codex-oauth"));
    assert!(!wallet
        .tickets
        .iter()
        .any(|ticket| ticket.source_id == "codex-leftover"));
    assert!(!wallet
        .bindings
        .iter()
        .any(|binding| binding.ticket_id == "provider:codex-leftover"));
    assert!(!wallet
        .bindings
        .iter()
        .any(|binding| binding.agent_id == AgentId::Codex && binding.active));
    assert!(!wallet
        .bindings
        .iter()
        .any(|binding| binding.ticket_id == "account:codex-oauth" && binding.active));
}

#[test]
fn imported_api_key_provider_is_still_a_ticket() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "openai-key",
            AgentId::Codex,
            "OpenAI API",
            "openai",
            false,
        ))
        .unwrap();

    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    let ticket = wallet
        .tickets
        .iter()
        .find(|ticket| ticket.id == "provider:openai-key")
        .expect("imported API key remains a ticket");
    assert_eq!(ticket.surface, TicketSurface::OpenaiApi);
    assert_eq!(ticket.credential_class, TicketCredentialClass::ApiKey);
}

fn sample_ticket(
    id: &str,
    kind: AdapterSourceKind,
    source_id: &str,
    agent: AgentId,
    label: &str,
    surface: TicketSurface,
    class: TicketCredentialClass,
) -> Ticket {
    Ticket {
        id: id.into(),
        source_kind: kind,
        source_id: source_id.into(),
        agent_id: agent,
        label: label.into(),
        surface,
        credential_class: class,
        speaks: surface.speaks().to_vec(),
        imported_from: Some(agent),
    }
}

#[test]
fn same_surface_two_accounts_form_one_group_in_ticket_id_order() {
    let groups = group_ticket_surface_members(&[
        sample_ticket(
            "account:g2",
            AdapterSourceKind::Account,
            "g2",
            AgentId::Grok,
            "b@x.com",
            TicketSurface::GrokXaiSubscription,
            TicketCredentialClass::Oauth,
        ),
        sample_ticket(
            "account:g1",
            AdapterSourceKind::Account,
            "g1",
            AgentId::Grok,
            "a@x.com",
            TicketSurface::GrokXaiSubscription,
            TicketCredentialClass::Oauth,
        ),
    ]);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].surface, TicketSurface::GrokXaiSubscription);
    assert_eq!(groups[0].credential_class, TicketCredentialClass::Oauth);
    let ids: Vec<_> = groups[0]
        .members
        .iter()
        .map(|member| member.ticket_id.as_str())
        .collect();
    assert_eq!(ids, vec!["account:g1", "account:g2"]);
}

#[test]
fn different_surfaces_do_not_share_a_group() {
    let groups = group_ticket_surface_members(&[
        sample_ticket(
            "provider:kimi",
            AdapterSourceKind::Provider,
            "kimi",
            AgentId::Kimi,
            "Kimi",
            TicketSurface::KimiCodeMembership,
            TicketCredentialClass::ApiKey,
        ),
        sample_ticket(
            "provider:anth",
            AdapterSourceKind::Provider,
            "anth",
            AgentId::Claude,
            "Anth",
            TicketSurface::AnthropicApi,
            TicketCredentialClass::ApiKey,
        ),
    ]);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].surface, TicketSurface::AnthropicApi);
    assert_eq!(groups[1].surface, TicketSurface::KimiCodeMembership);
    assert_eq!(groups[0].members.len(), 1);
    assert_eq!(groups[1].members.len(), 1);
}

#[test]
fn account_and_provider_of_same_surface_mix_in_one_group() {
    let groups = group_ticket_surface_members(&[
        sample_ticket(
            "provider:kimi-src",
            AdapterSourceKind::Provider,
            "kimi-src",
            AgentId::Kimi,
            "Kimi membership",
            TicketSurface::KimiCodeMembership,
            TicketCredentialClass::ApiKey,
        ),
        sample_ticket(
            "account:kimi-key",
            AdapterSourceKind::Account,
            "kimi-key",
            AgentId::Kimi,
            "Kimi key",
            TicketSurface::KimiCodeMembership,
            TicketCredentialClass::ApiKey,
        ),
    ]);
    assert_eq!(groups.len(), 1);
    let ids: Vec<_> = groups[0]
        .members
        .iter()
        .map(|member| member.ticket_id.as_str())
        .collect();
    assert_eq!(ids, vec!["account:kimi-key", "provider:kimi-src"]);
}

#[test]
fn unknown_surface_and_unknown_credential_class_are_not_grouped() {
    let groups = group_ticket_surface_members(&[
        sample_ticket(
            "provider:relay",
            AdapterSourceKind::Provider,
            "relay",
            AgentId::Claude,
            "relay",
            TicketSurface::Unknown,
            TicketCredentialClass::ApiKey,
        ),
        sample_ticket(
            "account:odd",
            AdapterSourceKind::Account,
            "odd",
            AgentId::Pi,
            "odd",
            TicketSurface::AnthropicApi,
            TicketCredentialClass::Unknown,
        ),
    ]);
    assert!(groups.is_empty());
}

#[test]
fn list_wallet_groups_known_surfaces_and_excludes_unknown_and_projections() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&account(
            "grok-a",
            AgentId::Grok,
            AccountKind::Oauth,
            "a@x.com",
            false,
        ))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&account(
            "grok-b",
            AgentId::Grok,
            AccountKind::Oauth,
            "b@x.com",
            false,
        ))
        .unwrap();
    let mut kimi_key = account(
        "kimi-key",
        AgentId::Kimi,
        AccountKind::ApiKey,
        "Kimi key",
        false,
    );
    kimi_key.credentials = serde_json::json!({"api_key": "sk-x"});
    kimi_key.extra = serde_json::json!({"surface": "kimi-code-membership"});
    AccountRepo::new(db.clone()).create(&kimi_key).unwrap();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "kimi-src",
            AgentId::Kimi,
            "Kimi membership",
            "kimi-code-membership",
            false,
        ))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "relay",
            AgentId::Claude,
            "Custom relay",
            "openai-compatible",
            false,
        ))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "proj-claude",
            AgentId::Claude,
            "Generated",
            "custom",
            true,
        ))
        .unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&profile(
            "p1",
            AdapterSourceKind::Provider,
            "kimi-src",
            AgentId::Claude,
            AdapterRoute::NativeEndpoint,
            Some("proj-claude"),
            None,
        ))
        .unwrap();

    let wallet = TicketReadService::new(db).list_wallet().unwrap();
    assert!(wallet
        .tickets
        .iter()
        .any(|t| t.surface == TicketSurface::Unknown));
    assert!(!wallet.tickets.iter().any(|t| t.source_id == "proj-claude"));
    let grok = wallet
        .surface_groups
        .iter()
        .find(|g| {
            g.surface == TicketSurface::GrokXaiSubscription
                && g.credential_class == TicketCredentialClass::Oauth
        })
        .expect("grok oauth group");
    let grok_ids: Vec<_> = grok.members.iter().map(|m| m.ticket_id.as_str()).collect();
    assert_eq!(grok_ids, vec!["account:grok-a", "account:grok-b"]);
    let kimi = wallet
        .surface_groups
        .iter()
        .find(|g| {
            g.surface == TicketSurface::KimiCodeMembership
                && g.credential_class == TicketCredentialClass::ApiKey
        })
        .expect("kimi api group");
    let kimi_ids: Vec<_> = kimi.members.iter().map(|m| m.ticket_id.as_str()).collect();
    assert_eq!(kimi_ids, vec!["account:kimi-key", "provider:kimi-src"]);
    assert!(wallet
        .surface_groups
        .iter()
        .all(|g| g.surface != TicketSurface::Unknown));
}

/// Wallet-derived connection pointer for one Agent.
/// Native tickets map 1:1 to account_id / provider_id.
/// Reshape/bridge map to the generated projection id (the live current row),
/// not the source ticket.
fn wallet_active_connection_pointer(
    wallet: &crate::models::TicketWallet,
    profiles: &[AdapterProfile],
    agent: AgentId,
) -> (Option<String>, Option<String>) {
    let Some(binding) = wallet
        .bindings
        .iter()
        .find(|binding| binding.agent_id == agent && binding.active)
    else {
        return (None, None);
    };
    match binding.route {
        TicketBindingRoute::Native => match parse_ticket_id(&binding.ticket_id) {
            Ok((AdapterSourceKind::Account, id)) => (Some(id), None),
            Ok((AdapterSourceKind::Provider, id)) => (None, Some(id)),
            Err(_) => (None, None),
        },
        TicketBindingRoute::Reshape | TicketBindingRoute::Bridge => {
            let generated = binding.profile_id.as_deref().and_then(|profile_id| {
                profiles
                    .iter()
                    .find(|profile| profile.id == profile_id)
                    .and_then(|profile| profile.generated_provider_id.clone())
            });
            (None, generated)
        }
    }
}

fn raw_active_pointer(db: &Database, agent: AgentId) -> (Option<String>, Option<String>) {
    match ActiveBindingRepo::new(db.clone())
        .get(agent.as_str())
        .unwrap()
    {
        Some(row) => (row.account_id, row.provider_id),
        None => (None, None),
    }
}

#[test]
fn ticket_connection_active_binding_matches_wallet_native_account() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&account(
            "codex-oauth",
            AgentId::Codex,
            AccountKind::Oauth,
            "me@example.com",
            false,
        ))
        .unwrap();
    ConnectionService::new(db.clone())
        .record_account_active(AgentId::Codex, "codex-oauth")
        .unwrap();

    let wallet = TicketReadService::new(db.clone()).list_wallet().unwrap();
    let derived = wallet_active_connection_pointer(&wallet, &[], AgentId::Codex);
    let pointer = raw_active_pointer(&db, AgentId::Codex);
    assert_eq!(derived, (Some("codex-oauth".into()), None));
    assert_eq!(derived, pointer);
}

#[test]
fn ticket_connection_active_binding_matches_wallet_native_provider() {
    let (_dir, db) = test_db();
    let row = ProviderRepo::new(db.clone())
        .create(&provider(
            "anth",
            AgentId::Claude,
            "Anthropic",
            "anthropic",
            false,
        ))
        .unwrap();
    ConnectionService::new(db.clone())
        .activate_provider(AgentId::Claude, "anth", &row.updated_at, "t-act")
        .unwrap();

    let wallet = TicketReadService::new(db.clone()).list_wallet().unwrap();
    let derived = wallet_active_connection_pointer(&wallet, &[], AgentId::Claude);
    let pointer = raw_active_pointer(&db, AgentId::Claude);
    assert_eq!(derived, (None, Some("anth".into())));
    assert_eq!(derived, pointer);
}

#[test]
fn ticket_connection_active_binding_matches_wallet_reshape_projection() {
    let (_dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&provider(
            "kimi-src",
            AgentId::Kimi,
            "Kimi membership",
            "kimi-code-membership",
            false,
        ))
        .unwrap();
    let generated = ProviderRepo::new(db.clone())
        .create(&provider(
            "proj-claude",
            AgentId::Claude,
            "Generated",
            "custom",
            false,
        ))
        .unwrap();
    let reshape = profile(
        "reshape-p",
        AdapterSourceKind::Provider,
        "kimi-src",
        AgentId::Claude,
        AdapterRoute::NativeEndpoint,
        Some("proj-claude"),
        None,
    );
    AdapterProfileRepo::new(db.clone())
        .create(&reshape)
        .unwrap();
    ConnectionService::new(db.clone())
        .activate_provider(
            AgentId::Claude,
            "proj-claude",
            &generated.updated_at,
            "t-act",
        )
        .unwrap();

    let wallet = TicketReadService::new(db.clone()).list_wallet().unwrap();
    let derived = wallet_active_connection_pointer(&wallet, &[reshape], AgentId::Claude);
    let pointer = raw_active_pointer(&db, AgentId::Claude);
    assert_eq!(derived, (None, Some("proj-claude".into())));
    assert_eq!(derived, pointer);
    let active = wallet
        .bindings
        .iter()
        .find(|binding| binding.agent_id == AgentId::Claude && binding.active)
        .expect("claude active");
    assert_eq!(active.ticket_id, "provider:kimi-src");
    assert_eq!(active.route, TicketBindingRoute::Reshape);
}

/// Detector for D3: mutating only `accounts.is_current` (AccountRepo.update,
/// which does not dual-write `agent_active_bindings`) must diverge from the
/// raw pointer. The committed assertion is the divergence itself. To demo a
/// red PR: change the final `assert_ne` to `assert_eq` after the same mutate.
#[test]
fn ticket_connection_wallet_vs_pointer_detects_one_sided_is_current_drift() {
    let (_dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&account(
            "acc-a",
            AgentId::Claude,
            AccountKind::Oauth,
            "a@x.com",
            false,
        ))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&account(
            "acc-b",
            AgentId::Claude,
            AccountKind::Oauth,
            "b@x.com",
            false,
        ))
        .unwrap();
    ConnectionService::new(db.clone())
        .record_account_active(AgentId::Claude, "acc-a")
        .unwrap();

    let before = TicketReadService::new(db.clone()).list_wallet().unwrap();
    assert_eq!(
        wallet_active_connection_pointer(&before, &[], AgentId::Claude),
        raw_active_pointer(&db, AgentId::Claude)
    );

    let mut b = AccountRepo::new(db.clone())
        .get_by_id("acc-b")
        .unwrap()
        .unwrap();
    b.is_current = true;
    AccountRepo::new(db.clone()).update(&b).unwrap();

    let after = TicketReadService::new(db.clone()).list_wallet().unwrap();
    let derived = wallet_active_connection_pointer(&after, &[], AgentId::Claude);
    let pointer = raw_active_pointer(&db, AgentId::Claude);
    assert_eq!(derived, (Some("acc-b".into()), None));
    assert_eq!(pointer, (Some("acc-a".into()), None));
    assert_ne!(
        derived, pointer,
        "one-sided is_current write must diverge from agent_active_bindings"
    );
}
