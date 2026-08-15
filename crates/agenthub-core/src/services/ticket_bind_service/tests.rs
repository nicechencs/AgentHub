use super::*;
use crate::adapters::{AdapterRegistry, AgentAdapter};
use crate::error::Result;
use crate::models::{
    ticket_id, Account, AccountKind, AgentConfig, AgentId, AuthState, Capability, CapabilityState,
    DetectResult, DetectStatus, InstallChannel, Provider, RunOptions, RunSpec, TicketBindingRoute,
    TicketPlanRequest, TicketUnbindRequest, PROJECTION_NOT_A_TICKET,
};
use crate::storage::{AccountRepo, AdapterProfileRepo, Database, ProviderRepo};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

struct FakeClaudeAdapter {
    config: Mutex<AgentConfig>,
}

impl FakeClaudeAdapter {
    fn new() -> Self {
        Self {
            config: Mutex::new(AgentConfig {
                agent: AgentId::Claude,
                raw: json!({}),
            }),
        }
    }
}

impl AgentAdapter for FakeClaudeAdapter {
    fn id(&self) -> AgentId {
        AgentId::Claude
    }
    fn detect(&self) -> DetectResult {
        DetectResult {
            agent: AgentId::Claude,
            status: DetectStatus::NotFound,
            version: None,
            binary_path: None,
            channel: None,
            env_ready: true,
            notes: vec![],
        }
    }
    fn install_channels(&self) -> Vec<InstallChannel> {
        vec![]
    }
    fn read_config(&self) -> Result<AgentConfig> {
        Ok(self.config.lock().unwrap().clone())
    }
    fn write_config(&self, config: &AgentConfig) -> Result<()> {
        *self.config.lock().unwrap() = config.clone();
        Ok(())
    }
    fn read_auth(&self) -> Result<AuthState> {
        Err(crate::error::AppError::Unsupported("fake".into()))
    }
    fn capability(&self, capability: Capability) -> CapabilityState {
        match capability {
            Capability::ConfigWrite | Capability::LiveBackup => CapabilityState::full(),
            _ => CapabilityState::unsupported("fake"),
        }
    }
    fn skills_dir(&self) -> Option<PathBuf> {
        None
    }
    fn live_backup_paths(&self) -> Vec<PathBuf> {
        vec![]
    }
    fn build_run_spec(
        &self,
        _binary: &Path,
        _prompt: &str,
        _options: &RunOptions,
    ) -> Result<RunSpec> {
        Err(crate::error::AppError::Unsupported("fake".into()))
    }
}

struct FakePiAdapter {
    config: Mutex<AgentConfig>,
}

impl FakePiAdapter {
    fn new() -> Self {
        Self {
            config: Mutex::new(AgentConfig {
                agent: AgentId::Pi,
                raw: json!({}),
            }),
        }
    }
}

impl AgentAdapter for FakePiAdapter {
    fn id(&self) -> AgentId {
        AgentId::Pi
    }
    fn detect(&self) -> DetectResult {
        DetectResult {
            agent: AgentId::Pi,
            status: DetectStatus::NotFound,
            version: None,
            binary_path: None,
            channel: None,
            env_ready: true,
            notes: vec![],
        }
    }
    fn install_channels(&self) -> Vec<InstallChannel> {
        vec![]
    }
    fn read_config(&self) -> Result<AgentConfig> {
        Ok(self.config.lock().unwrap().clone())
    }
    fn write_config(&self, config: &AgentConfig) -> Result<()> {
        *self.config.lock().unwrap() = config.clone();
        Ok(())
    }
    fn read_auth(&self) -> Result<AuthState> {
        Err(crate::error::AppError::Unsupported("fake".into()))
    }
    fn capability(&self, capability: Capability) -> CapabilityState {
        match capability {
            Capability::ConfigWrite | Capability::LiveBackup => CapabilityState::full(),
            _ => CapabilityState::unsupported("fake"),
        }
    }
    fn skills_dir(&self) -> Option<PathBuf> {
        None
    }
    fn live_backup_paths(&self) -> Vec<PathBuf> {
        vec![]
    }
    fn build_run_spec(
        &self,
        _binary: &Path,
        _prompt: &str,
        _options: &RunOptions,
    ) -> Result<RunSpec> {
        Err(crate::error::AppError::Unsupported("fake".into()))
    }
}

fn test_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("ticket-bind.db")).unwrap();
    (dir, db)
}

fn kimi_source(id: &str, api_key: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Kimi,
        name: "Kimi membership".into(),
        settings_config: json!({"apiKey": api_key}),
        meta: json!({"preset": "kimi-code-membership"}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn anthropic_source(id: &str, api_key: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Claude,
        name: "Anthropic API".into(),
        settings_config: json!({"env": { "ANTHROPIC_AUTH_TOKEN": api_key }}),
        meta: json!({"preset": "anthropic"}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn explicit_api_source(id: &str, preset: &str, env_key: &str, api_key: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Claude,
        name: format!("{preset} API"),
        settings_config: json!({"env": { env_key: api_key }}),
        meta: json!({"preset": preset}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn explicit_api_account(id: &str, provider: &str, api_key: &str) -> Account {
    Account {
        id: id.into(),
        agent_id: AgentId::Claude,
        kind: AccountKind::ApiKey,
        label: format!("{provider} key"),
        credentials: json!({"format": "api_key", "api_key": api_key}),
        extra: json!({"provider": provider}),
        status: "active".into(),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn anthropic_account(id: &str, api_key: &str) -> Account {
    Account {
        id: id.into(),
        agent_id: AgentId::Claude,
        kind: AccountKind::ApiKey,
        label: "Anthropic key".into(),
        credentials: json!({"format": "api_key", "api_key": api_key}),
        extra: json!({"provider": "anthropic"}),
        status: "active".into(),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn subscription_account(
    id: &str,
    agent_id: AgentId,
    access: &str,
    refresh: &str,
    codex_auth_json: bool,
) -> Account {
    let credentials = if codex_auth_json {
        json!({
            "format": "auth_json",
            "tokens": { "access_token": access, "refresh_token": refresh }
        })
    } else {
        json!({
            "format": "oauth",
            "access_token": access,
            "refresh_token": refresh
        })
    };
    Account {
        id: id.into(),
        agent_id,
        kind: AccountKind::Oauth,
        label: format!("{id} subscription"),
        credentials,
        extra: json!({}),
        status: "active".into(),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn bind_service(
    db: Database,
    backups: PathBuf,
    agents: Vec<Arc<dyn AgentAdapter>>,
) -> TicketBindService {
    let mut registry = AdapterRegistry::new();
    for agent in agents {
        registry.register(agent);
    }
    TicketBindService::new(db, registry, backups)
}

#[test]
fn bind_kimi_provider_to_claude_returns_active_reshape() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("kimi-source", "test-kimi-secret"))
        .unwrap();
    let service = bind_service(
        db,
        dir.path().join("backups"),
        vec![Arc::new(FakeClaudeAdapter::new())],
    );

    let binding = service
        .bind(&TicketPlanRequest {
            ticket_id: ticket_id(AdapterSourceKind::Provider, "kimi-source"),
            target_agent_id: AgentId::Claude,
        })
        .unwrap();
    assert_eq!(binding.ticket_id, "provider:kimi-source");
    assert_eq!(binding.agent_id, AgentId::Claude);
    assert_eq!(binding.route, TicketBindingRoute::Reshape);
    assert!(binding.active);
    assert!(binding.profile_id.is_some());
    assert!(binding.bridge.is_none());
}

#[test]
fn bind_anthropic_provider_to_pi_returns_active_reshape() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&anthropic_source("anthropic-source", "sk-anthropic-secret"))
        .unwrap();
    let service = bind_service(
        db,
        dir.path().join("backups"),
        vec![Arc::new(FakePiAdapter::new())],
    );

    let binding = service
        .bind(&TicketPlanRequest {
            ticket_id: ticket_id(AdapterSourceKind::Provider, "anthropic-source"),
            target_agent_id: AgentId::Pi,
        })
        .unwrap();
    assert_eq!(binding.ticket_id, "provider:anthropic-source");
    assert_eq!(binding.agent_id, AgentId::Pi);
    assert_eq!(binding.route, TicketBindingRoute::Reshape);
    assert!(binding.active);
}

#[test]
fn bind_anthropic_account_to_pi_requires_can_apply_and_keeps_account_source_ref() {
    let (dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&anthropic_account("anthropic-account", "sk-account-secret"))
        .unwrap();
    let tickets = TicketReadService::new(db.clone());
    let plan = tickets
        .plan(&TicketPlanRequest {
            ticket_id: ticket_id(AdapterSourceKind::Account, "anthropic-account"),
            target_agent_id: AgentId::Pi,
        })
        .unwrap();
    assert!(plan.can_apply);

    let service = bind_service(
        db.clone(),
        dir.path().join("backups"),
        vec![Arc::new(FakePiAdapter::new())],
    );
    let binding = service
        .bind(&TicketPlanRequest {
            ticket_id: ticket_id(AdapterSourceKind::Account, "anthropic-account"),
            target_agent_id: AgentId::Pi,
        })
        .unwrap();
    assert!(binding.active);
    assert_eq!(binding.ticket_id, "account:anthropic-account");

    let profile = AdapterProfileRepo::new(db.clone())
        .get(binding.profile_id.as_deref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(profile.source_kind, AdapterSourceKind::Account);
    let generated = ProviderRepo::new(db)
        .get_by_id(profile.generated_provider_id.as_deref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(generated.meta["adapterSourceRef"]["kind"], "account");
    assert_eq!(
        generated.meta["adapterSourceRef"]["id"],
        "anthropic-account"
    );
    assert!(!serde_json::to_string(&generated)
        .unwrap()
        .contains("sk-account-secret"));
}

#[test]
fn bind_openai_and_xai_provider_and_account_to_pi_then_unbind() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&explicit_api_source(
            "openai-source",
            "openai",
            "OPENAI_API_KEY",
            "sk-openai-secret",
        ))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&explicit_api_account(
            "xai-account",
            "xai",
            "xai-account-secret",
        ))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "relay-source".into(),
            agent_id: AgentId::Claude,
            name: "Custom relay".into(),
            settings_config: json!({"apiKey": "relay-secret", "baseUrl": "https://relay.example/v1"}),
            meta: json!({"preset": "openai-compatible"}),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = bind_service(
        db.clone(),
        dir.path().join("backups"),
        vec![Arc::new(FakePiAdapter::new())],
    );

    let openai = service
        .bind(&TicketPlanRequest {
            ticket_id: ticket_id(AdapterSourceKind::Provider, "openai-source"),
            target_agent_id: AgentId::Pi,
        })
        .unwrap();
    assert!(openai.active);
    assert_eq!(openai.route, TicketBindingRoute::Reshape);
    let openai_profile = AdapterProfileRepo::new(db.clone())
        .get(openai.profile_id.as_deref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(openai_profile.rule_id, "openai-api-to-pi-v1");

    service
        .unbind(&TicketUnbindRequest {
            ticket_id: ticket_id(AdapterSourceKind::Provider, "openai-source"),
            agent_id: AgentId::Pi,
        })
        .unwrap();

    let xai = service
        .bind(&TicketPlanRequest {
            ticket_id: ticket_id(AdapterSourceKind::Account, "xai-account"),
            target_agent_id: AgentId::Pi,
        })
        .unwrap();
    assert!(xai.active);
    assert_eq!(xai.ticket_id, "account:xai-account");
    let xai_profile = AdapterProfileRepo::new(db.clone())
        .get(xai.profile_id.as_deref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(xai_profile.rule_id, "xai-api-to-pi-v1");
    assert_eq!(xai_profile.source_kind, AdapterSourceKind::Account);

    let relay = service.bind(&TicketPlanRequest {
        ticket_id: ticket_id(AdapterSourceKind::Provider, "relay-source"),
        target_agent_id: AgentId::Pi,
    });
    assert!(relay.is_err(), "unknown custom relay must not bind");
}

#[test]
fn bind_claude_codex_and_grok_subscriptions_to_pi_then_unbind() {
    let (dir, db) = test_db();
    let accounts = AccountRepo::new(db.clone());
    for (account, rule_id) in [
        (
            subscription_account(
                "claude-subscription",
                AgentId::Claude,
                "claude-access-secret",
                "claude-refresh-secret",
                false,
            ),
            "claude-subscription-to-pi-v1",
        ),
        (
            subscription_account(
                "codex-subscription",
                AgentId::Codex,
                "codex-access-secret",
                "codex-refresh-secret",
                true,
            ),
            "codex-subscription-to-pi-v1",
        ),
        (
            subscription_account(
                "grok-subscription",
                AgentId::Grok,
                "grok-access-secret",
                "grok-refresh-secret",
                false,
            ),
            "grok-subscription-to-pi-v1",
        ),
    ] {
        accounts.create(&account).unwrap();
        let service = bind_service(
            db.clone(),
            dir.path().join(format!("backups-{}", account.id)),
            vec![Arc::new(FakePiAdapter::new())],
        );
        let ticket = ticket_id(AdapterSourceKind::Account, &account.id);
        let plan = service
            .tickets
            .plan(&TicketPlanRequest {
                ticket_id: ticket.clone(),
                target_agent_id: AgentId::Pi,
            })
            .unwrap();
        assert!(plan.can_apply, "{}", account.id);

        let binding = service
            .bind(&TicketPlanRequest {
                ticket_id: ticket.clone(),
                target_agent_id: AgentId::Pi,
            })
            .unwrap();
        assert!(binding.active);
        let profile = AdapterProfileRepo::new(db.clone())
            .get(binding.profile_id.as_deref().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(profile.mode, crate::models::AdapterProfileMode::Oauth);
        assert_eq!(profile.rule_id, rule_id);

        service
            .unbind(&TicketUnbindRequest {
                ticket_id: ticket,
                agent_id: AgentId::Pi,
            })
            .unwrap();
        assert!(
            AdapterProfileRepo::new(db.clone())
                .get(&profile.id)
                .unwrap()
                .is_none(),
            "{} profile should be deleted",
            account.id
        );
    }
}

#[test]
fn bind_projection_ticket_is_rejected() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("kimi-source", "test-kimi-secret"))
        .unwrap();
    let service = bind_service(
        db.clone(),
        dir.path().join("backups"),
        vec![Arc::new(FakeClaudeAdapter::new())],
    );
    let binding = service
        .bind(&TicketPlanRequest {
            ticket_id: ticket_id(AdapterSourceKind::Provider, "kimi-source"),
            target_agent_id: AgentId::Claude,
        })
        .unwrap();
    let generated_id = AdapterProfileRepo::new(db.clone())
        .get(binding.profile_id.as_deref().unwrap())
        .unwrap()
        .unwrap()
        .generated_provider_id
        .unwrap();

    let error = service
        .bind(&TicketPlanRequest {
            ticket_id: ticket_id(AdapterSourceKind::Provider, &generated_id),
            target_agent_id: AgentId::Pi,
        })
        .unwrap_err();
    assert_eq!(error.code(), "invalid_arg");
    assert!(error.to_string().contains(PROJECTION_NOT_A_TICKET));
}

#[test]
fn unbind_current_restores_previous_and_keeps_source_ticket_out_of_wallet_projection() {
    let (dir, db) = test_db();
    let previous = Provider {
        id: "pi-previous".into(),
        agent_id: AgentId::Pi,
        name: "Previous Pi".into(),
        settings_config: json!({"models": {"providers": {}}}),
        meta: json!({}),
        is_current: true,
        created_at: "now".into(),
        updated_at: "now".into(),
    };
    ProviderRepo::new(db.clone()).create(&previous).unwrap();
    ProviderRepo::new(db.clone())
        .create(&anthropic_source("anthropic-source", "sk-anthropic-secret"))
        .unwrap();
    let service = bind_service(
        db.clone(),
        dir.path().join("backups"),
        vec![Arc::new(FakePiAdapter::new())],
    );

    let ticket = ticket_id(AdapterSourceKind::Provider, "anthropic-source");
    let binding = service
        .bind(&TicketPlanRequest {
            ticket_id: ticket.clone(),
            target_agent_id: AgentId::Pi,
        })
        .unwrap();
    assert!(binding.active);
    let generated_id = AdapterProfileRepo::new(db.clone())
        .get(binding.profile_id.as_deref().unwrap())
        .unwrap()
        .unwrap()
        .generated_provider_id
        .unwrap();

    service
        .unbind(&TicketUnbindRequest {
            ticket_id: ticket.clone(),
            agent_id: AgentId::Pi,
        })
        .unwrap();

    let wallet = TicketReadService::new(db.clone()).list_wallet().unwrap();
    let ticket_ids: Vec<_> = wallet.tickets.iter().map(|t| t.id.as_str()).collect();
    assert!(ticket_ids.contains(&ticket.as_str()));
    assert!(!ticket_ids.iter().any(|id| id.contains(&generated_id)));
    assert!(wallet
        .bindings
        .iter()
        .all(|row| row.ticket_id != ticket || row.agent_id != AgentId::Pi));

    let restored = ProviderRepo::new(db.clone())
        .get_by_id("pi-previous")
        .unwrap()
        .unwrap();
    assert!(restored.is_current);
    assert!(ProviderRepo::new(db.clone())
        .get_by_id(&generated_id)
        .unwrap()
        .is_none());
    assert!(AdapterProfileRepo::new(db)
        .list(None, None, None)
        .unwrap()
        .is_empty());
}

#[test]
fn bind_glm_and_deepseek_to_claude_then_unbind_rejects_unknown_relay() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&explicit_api_source(
            "glm-source",
            "glm-coding-plan",
            "ANTHROPIC_AUTH_TOKEN",
            "glm-secret",
        ))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&explicit_api_account(
            "deepseek-account",
            "deepseek-api",
            "deepseek-account-secret",
        ))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&Provider {
            id: "relay-source".into(),
            agent_id: AgentId::Claude,
            name: "Custom relay".into(),
            settings_config: json!({"apiKey": "relay-secret", "baseUrl": "https://relay.example/v1"}),
            meta: json!({"preset": "openai-compatible"}),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let tickets = TicketReadService::new(db.clone());
    assert!(
        tickets
            .plan(&TicketPlanRequest {
                ticket_id: ticket_id(AdapterSourceKind::Provider, "glm-source"),
                target_agent_id: AgentId::Claude,
            })
            .unwrap()
            .can_apply
    );
    assert!(
        tickets
            .plan(&TicketPlanRequest {
                ticket_id: ticket_id(AdapterSourceKind::Account, "deepseek-account"),
                target_agent_id: AgentId::Claude,
            })
            .unwrap()
            .can_apply
    );

    let service = bind_service(
        db.clone(),
        dir.path().join("backups"),
        vec![Arc::new(FakeClaudeAdapter::new())],
    );
    let glm = service
        .bind(&TicketPlanRequest {
            ticket_id: ticket_id(AdapterSourceKind::Provider, "glm-source"),
            target_agent_id: AgentId::Claude,
        })
        .unwrap();
    assert!(glm.active);
    assert_eq!(glm.route, TicketBindingRoute::Reshape);
    let glm_profile = AdapterProfileRepo::new(db.clone())
        .get(glm.profile_id.as_deref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(glm_profile.rule_id, "glm-coding-plan-to-claude-v1");

    service
        .unbind(&TicketUnbindRequest {
            ticket_id: ticket_id(AdapterSourceKind::Provider, "glm-source"),
            agent_id: AgentId::Claude,
        })
        .unwrap();

    let deepseek = service
        .bind(&TicketPlanRequest {
            ticket_id: ticket_id(AdapterSourceKind::Account, "deepseek-account"),
            target_agent_id: AgentId::Claude,
        })
        .unwrap();
    assert!(deepseek.active);
    assert_eq!(deepseek.ticket_id, "account:deepseek-account");
    let deepseek_profile = AdapterProfileRepo::new(db.clone())
        .get(deepseek.profile_id.as_deref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(deepseek_profile.rule_id, "deepseek-api-to-claude-v1");
    assert_eq!(deepseek_profile.source_kind, AdapterSourceKind::Account);

    let relay = service.bind(&TicketPlanRequest {
        ticket_id: ticket_id(AdapterSourceKind::Provider, "relay-source"),
        target_agent_id: AgentId::Claude,
    });
    assert!(relay.is_err(), "unknown custom relay must not bind");
}
