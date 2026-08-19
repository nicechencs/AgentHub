use super::*;
use crate::adapters::AgentAdapter;
use crate::models::{Account, AccountKind, AdapterApplyResult, Provider};
use crate::models::{
    AgentConfig, AuthState, Capability, CapabilityState, DetectResult, DetectStatus,
    InstallChannel, RunOptions, RunSpec,
};
use crate::services::adapter_route_constants::{
    DEEPSEEK_API_BASE_URL, DEEPSEEK_CLAUDE_BASE_URL, DEEPSEEK_PI_PROVIDER_SLOT,
    GLM_CLAUDE_BASE_URL, GLM_PI_BASE_URL, GLM_PI_PROVIDER_SLOT, KIMI_CLAUDE_BASE_URL,
    KIMI_GROK_BASE_URL, KIMI_GROK_DEFAULT_MODEL, KIMI_GROK_RULE_ID, OPENAI_GROK_BASE_URL,
    OPENAI_GROK_DEFAULT_MODEL, OPENAI_GROK_RULE_ID,
};
use crate::storage::{AccountRepo, ActiveBindingRepo, ProviderRepo};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct FakeClaudeAdapter {
    agent: AgentId,
    config: Mutex<AgentConfig>,
    writes: AtomicUsize,
    fail_on_write: AtomicUsize,
    fail_writes: Mutex<Vec<usize>>,
}

impl FakeClaudeAdapter {
    fn new() -> Self {
        Self::new_for(AgentId::Claude)
    }

    fn new_for(agent: AgentId) -> Self {
        Self {
            agent,
            config: Mutex::new(AgentConfig {
                agent,
                raw: json!({}),
            }),
            writes: AtomicUsize::new(0),
            fail_on_write: AtomicUsize::new(0),
            fail_writes: Mutex::new(vec![]),
        }
    }

    fn fail_on_write(&self, attempt: usize) {
        self.fail_on_write.store(attempt, Ordering::SeqCst);
    }

    fn fail_writes_on(&self, attempts: &[usize]) {
        *self.fail_writes.lock().unwrap() = attempts.to_vec();
    }
}

impl AgentAdapter for FakeClaudeAdapter {
    fn id(&self) -> AgentId {
        self.agent
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
        let attempt = self.writes.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_on_write.load(Ordering::SeqCst) == attempt
            || self.fail_writes.lock().unwrap().contains(&attempt)
        {
            return Err(AppError::message(
                "test.write",
                format!("injected write failure {attempt}"),
            ));
        }
        *self.config.lock().unwrap() = config.clone();
        Ok(())
    }
    fn read_auth(&self) -> Result<AuthState> {
        Err(AppError::Unsupported("fake".into()))
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
        Err(AppError::Unsupported("fake".into()))
    }
}

fn test_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("adapter-apply.db")).unwrap();
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

fn kimi_account(id: &str, api_key: &str) -> Account {
    Account {
        id: id.into(),
        agent_id: AgentId::Kimi,
        kind: AccountKind::ApiKey,
        label: "Kimi Code membership".into(),
        credentials: json!({
            "format": "api_key",
            "api_key": api_key,
            "provider": "kimi-code-membership",
        }),
        extra: json!({}),
        status: "active".into(),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn kimi_coding_live_import(id: &str, api_key: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Kimi,
        name: "Kimi coding live import".into(),
        settings_config: json!({
            "apiKey": api_key,
            "baseUrl": "https://api.kimi.com/coding/v1",
        }),
        meta: json!({}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn kimi_moonshot(id: &str, api_key: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Kimi,
        name: "Moonshot".into(),
        settings_config: json!({
            "apiKey": api_key,
            "baseUrl": "https://api.moonshot.cn/v1",
        }),
        meta: json!({"preset": "moonshot"}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn kimi_bare(id: &str, api_key: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Kimi,
        name: "Bare Kimi".into(),
        settings_config: json!({"apiKey": api_key}),
        meta: json!({}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn request(source_id: &str, target_agent_id: AgentId) -> AdapterApplyRequest {
    AdapterApplyRequest {
        source_kind: AdapterSourceKind::Provider,
        source_id: source_id.into(),
        target_agent_id,
    }
}

fn account_request(source_id: &str, target_agent_id: AgentId) -> AdapterApplyRequest {
    AdapterApplyRequest {
        source_kind: AdapterSourceKind::Account,
        source_id: source_id.into(),
        target_agent_id,
    }
}

fn generated_provider(profile_id: &str, source_id: &str, current: bool) -> Provider {
    Provider {
        id: stable_id("claude-kimi-adapter", source_id),
        agent_id: AgentId::Claude,
        name: format!("Kimi Code ({})", safe_label(source_id)),
        settings_config: json!({"env": {
            "ANTHROPIC_BASE_URL": KIMI_CLAUDE_BASE_URL,
            "ANTHROPIC_AUTH_TOKEN": CONNECTION_SECRET_MARKER,
        }}),
        meta: json!({
            "preset": "anthropic-compatible",
            "generatedBy": "adapter",
            "adapterRuleId": RULE_ID,
            "adapterRuleVersion": 1,
            "adapterSecretMode": "source_reference",
            "adapterProfileId": profile_id,
            "adapterSourceRef": {"kind": "provider", "id": source_id},
        }),
        is_current: current,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn active_profile(source_id: &str) -> AdapterProfile {
    let profile_id = stable_id("adapter-kimi-claude", source_id);
    AdapterProfile {
        id: profile_id,
        name: format!("Kimi → Claude ({})", safe_label(source_id)),
        source_kind: AdapterSourceKind::Provider,
        source_id: source_id.into(),
        target_agent_id: AgentId::Claude,
        route: AdapterRoute::NativeEndpoint,
        mode: crate::models::AdapterProfileMode::Api,
        status: AdapterProfileStatus::Active,
        rule_id: RULE_ID.into(),
        rule_version: RULE_VERSION.into(),
        generated_provider_id: Some(stable_id("claude-kimi-adapter", source_id)),
        local_port: None,
        auto_start: false,
        last_error_code: None,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

#[test]
fn generated_provider_uses_stable_reference_marker() {
    assert_eq!(
        stable_id("x", "Kimi source!"),
        stable_id("x", "Kimi source!")
    );
    assert_eq!(CONNECTION_SECRET_MARKER, "$AGENTHUB_CONNECTION_SECRET$");
    assert_eq!(safe_label("Kimi source!"), "kimi-source");
}

#[test]
fn missing_membership_secret_creates_no_profile_or_generated_provider() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("kimi-source", ""))
        .unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        dir.path().join("backups"),
    );

    let error = service
        .apply(&request("kimi-source", AgentId::Claude))
        .unwrap_err();
    assert_eq!(error.code(), "invalid_arg");
    assert!(AdapterProfileRepo::new(db.clone())
        .list(None, None, None)
        .unwrap()
        .is_empty());
    assert_eq!(ProviderRepo::new(db).list(None).unwrap().len(), 1);
}

#[test]
fn apply_acquires_the_live_guard_before_reading_or_creating_profile_state() {
    let (dir, db) = test_db();
    let source = kimi_source("kimi-source", "test-kimi-secret");
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    let lock_dir = dir.path().join("locks");
    std::fs::create_dir_all(&lock_dir).unwrap();
    std::fs::write(lock_dir.join("provider-claude.lock"), b"held").unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        dir.path().join("backups"),
    );

    assert_eq!(
        service
            .apply(&request(&source.id, AgentId::Claude))
            .unwrap_err()
            .code(),
        "provider.lock"
    );
    assert!(AdapterProfileRepo::new(db.clone())
        .list(None, None, None)
        .unwrap()
        .is_empty());
    assert!(ProviderRepo::new(db)
        .get_by_id(&stable_id("claude-kimi-adapter", &source.id))
        .unwrap()
        .is_none());
}

#[test]
fn provider_id_collision_is_never_overwritten() {
    let (dir, db) = test_db();
    let source = kimi_source("kimi-source", "test-kimi-secret");
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    let generated_id = stable_id("claude-kimi-adapter", &source.id);
    let collision = Provider {
        id: generated_id.clone(),
        agent_id: AgentId::Claude,
        name: "user owned".into(),
        settings_config: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "user-secret"}}),
        meta: json!({"preset": "custom"}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    };
    ProviderRepo::new(db.clone()).create(&collision).unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        dir.path().join("backups"),
    );

    let error = service
        .apply(&request(&source.id, AgentId::Claude))
        .unwrap_err();
    assert_eq!(error.code(), "adapter.provider_conflict");
    assert_eq!(
        ProviderRepo::new(db)
            .get_by_id(&generated_id)
            .unwrap()
            .unwrap(),
        collision
    );
}

#[test]
fn active_complete_projection_returns_existing_pair_without_switching() {
    let (dir, db) = test_db();
    let source = kimi_source("kimi-source", "test-kimi-secret");
    let profile = active_profile(&source.id);
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    ProviderRepo::new(db.clone())
        .create(&generated_provider(&profile.id, &source.id, true))
        .unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&profile)
        .unwrap();
    // No Claude adapter is registered: attempting a switch would fail. A
    // successful result therefore proves the complete current Active pair is
    // the sole idempotent no-op case.
    let service = AdapterApplyService::new(db, AdapterRegistry::new(), dir.path().join("backups"));

    let result = service
        .apply(&request(&source.id, AgentId::Claude))
        .unwrap();
    assert_eq!(result.profile.status, AdapterProfileStatus::Active);
    assert_eq!(result.provider.id, profile.generated_provider_id.unwrap());
}

#[test]
fn active_complete_projection_ignores_display_name_drift() {
    let (dir, db) = test_db();
    let source = kimi_source("kimi-source", "test-kimi-secret");
    let mut profile = active_profile(&source.id);
    profile.name = "legacy display → Claude".into();
    let mut provider = generated_provider(&profile.id, &source.id, true);
    provider.name = "legacy display".into();
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    ProviderRepo::new(db.clone()).create(&provider).unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&profile)
        .unwrap();
    // Empty registry: a name-only mismatch must not fall through to switch.
    let service = AdapterApplyService::new(db, AdapterRegistry::new(), dir.path().join("backups"));

    let result = service
        .apply(&request(&source.id, AgentId::Claude))
        .unwrap();
    assert_eq!(result.profile.status, AdapterProfileStatus::Active);
    assert_eq!(result.provider.id, provider.id);
    assert_eq!(result.provider.name, "legacy display");
}

#[test]
fn active_demoted_projection_is_switched_back_to_current() {
    let (dir, db) = test_db();
    let source = kimi_source("kimi-source", "test-kimi-secret");
    let profile = active_profile(&source.id);
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    ProviderRepo::new(db.clone())
        .create(&generated_provider(&profile.id, &profile.source_id, false))
        .unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&profile)
        .unwrap();
    let fake = Arc::new(FakeClaudeAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let result = service
        .apply(&request(&source.id, AgentId::Claude))
        .unwrap();
    assert!(result.provider.is_current);
    assert_eq!(fake.writes.load(Ordering::SeqCst), 1);
    assert!(
        ProviderRepo::new(db)
            .get_by_id(&result.provider.id)
            .unwrap()
            .unwrap()
            .is_current
    );
}

#[test]
fn active_mutated_projection_is_repaired_and_switched() {
    let (dir, db) = test_db();
    let source = kimi_source("kimi-source", "test-kimi-secret");
    let profile = active_profile(&source.id);
    let mut mutated = generated_provider(&profile.id, &profile.source_id, true);
    mutated.settings_config["env"]["ANTHROPIC_BASE_URL"] = json!("https://stale.example/");
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    ProviderRepo::new(db.clone()).create(&mutated).unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&profile)
        .unwrap();
    let fake = Arc::new(FakeClaudeAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let result = service
        .apply(&request(&source.id, AgentId::Claude))
        .unwrap();
    let stored = ProviderRepo::new(db)
        .get_by_id(&result.provider.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.settings_config["env"]["ANTHROPIC_BASE_URL"],
        KIMI_CLAUDE_BASE_URL
    );
    assert_eq!(
        stored.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        CONNECTION_SECRET_MARKER
    );
    assert!(stored.is_current);
    assert_eq!(fake.writes.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_switch_restores_a_repaired_current_provider_and_binding() {
    let (dir, db) = test_db();
    let source = kimi_source("kimi-source", "test-kimi-secret");
    let profile = active_profile(&source.id);
    let mut mutated = generated_provider(&profile.id, &profile.source_id, true);
    mutated.settings_config["env"]["ANTHROPIC_BASE_URL"] = json!("https://stale.example/");
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    ProviderRepo::new(db.clone()).create(&mutated).unwrap();
    ActiveBindingRepo::new(db.clone())
        .set_refs(
            "claude",
            None,
            Some(mutated.id.clone()),
            None,
            "before-repair",
        )
        .unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&profile)
        .unwrap();
    let fake = Arc::new(FakeClaudeAdapter::new());
    fake.fail_on_write(1);
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let error = service
        .apply(&request(&source.id, AgentId::Claude))
        .unwrap_err();
    assert_eq!(error.code(), "adapter_apply.failed");

    let restored = ProviderRepo::new(db.clone())
        .get_by_id(&mutated.id)
        .unwrap()
        .unwrap();
    assert_eq!(restored.settings_config, mutated.settings_config);
    assert_eq!(restored.meta, mutated.meta);
    assert!(restored.is_current);
    assert_eq!(
        ActiveBindingRepo::new(db)
            .get("claude")
            .unwrap()
            .unwrap()
            .provider_id
            .as_deref(),
        Some(mutated.id.as_str())
    );
    assert_eq!(fake.read_config().unwrap().raw, json!({}));
}

#[test]
fn finalize_failure_after_first_switch_restores_previous_current_and_live() {
    let (dir, db) = test_db();
    let source = kimi_source("kimi-source", "test-kimi-secret");
    let previous = Provider {
        id: "previous-claude".into(),
        agent_id: AgentId::Claude,
        name: "Previous Claude".into(),
        settings_config: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "previous-secret"}}),
        meta: json!({}),
        is_current: true,
        created_at: "now".into(),
        updated_at: "now".into(),
    };
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    ProviderRepo::new(db.clone()).create(&previous).unwrap();
    ActiveBindingRepo::new(db.clone())
        .set_refs(
            "claude",
            None,
            Some(previous.id.clone()),
            None,
            "before-first-apply",
        )
        .unwrap();
    db.with_conn(|conn| {
        conn.execute_batch(
            r#"
            CREATE TRIGGER fail_adapter_profile_finalize_first
            BEFORE UPDATE OF status ON adapter_profiles
            WHEN NEW.status = 'active'
            BEGIN
                SELECT RAISE(ABORT, 'injected adapter profile finalization failure');
            END;
            "#,
        )?;
        Ok(())
    })
    .unwrap();
    let fake = Arc::new(FakeClaudeAdapter::new());
    fake.write_config(&crate::models::AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "previous-secret"}}),
    })
    .unwrap();
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let error = service
        .apply(&request(&source.id, AgentId::Claude))
        .unwrap_err();
    assert_eq!(error.code(), "adapter.profile_finalize");

    let profiles = AdapterProfileRepo::new(db.clone())
        .list(None, None, None)
        .unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].status, AdapterProfileStatus::NeedsAttention);
    assert_eq!(
        profiles[0].last_error_code.as_deref(),
        Some("adapter.profile_finalize")
    );
    assert_eq!(
        ActiveBindingRepo::new(db.clone())
            .get("claude")
            .unwrap()
            .unwrap()
            .provider_id
            .as_deref(),
        Some(previous.id.as_str())
    );
    assert!(
        ProviderRepo::new(db.clone())
            .get_by_id(&previous.id)
            .unwrap()
            .unwrap()
            .is_current
    );
    // The create was compensated: generated provider must not remain current.
    let generated_id = stable_id("claude-kimi-adapter", &source.id);
    let generated = ProviderRepo::new(db).get_by_id(&generated_id).unwrap();
    assert!(
        generated.is_none()
            || generated
                .as_ref()
                .is_some_and(|provider| !provider.is_current)
    );
    assert_eq!(
        fake.read_config().unwrap().raw["env"]["ANTHROPIC_AUTH_TOKEN"],
        "previous-secret"
    );
}

#[test]
fn finalize_and_rollback_failure_is_reported_as_incomplete_attention() {
    let (dir, db) = test_db();
    let source = kimi_source("kimi-source", "test-kimi-secret");
    let profile = active_profile(&source.id);
    let mut mutated = generated_provider(&profile.id, &profile.source_id, true);
    mutated.settings_config["env"]["ANTHROPIC_BASE_URL"] = json!("https://stale.example/");
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    ProviderRepo::new(db.clone()).create(&mutated).unwrap();
    ActiveBindingRepo::new(db.clone())
        .set_refs(
            "claude",
            None,
            Some(mutated.id.clone()),
            None,
            "before-repair",
        )
        .unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&profile)
        .unwrap();
    db.with_conn(|conn| {
        conn.execute_batch(
            r#"
            CREATE TRIGGER fail_adapter_profile_finalize
            BEFORE UPDATE OF status ON adapter_profiles
            WHEN NEW.status = 'active'
            BEGIN
                SELECT RAISE(ABORT, 'injected adapter profile finalization failure');
            END;
            "#,
        )?;
        Ok(())
    })
    .unwrap();
    let fake = Arc::new(FakeClaudeAdapter::new());
    // The switch succeeds on write one; the repair rollback's live restore is
    // write two and is deliberately made to fail.
    fake.fail_writes_on(&[2]);
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let error = service
        .apply(&request(&source.id, AgentId::Claude))
        .unwrap_err();
    assert_eq!(error.code(), "adapter.rollback_incomplete");

    let attention = AdapterProfileRepo::new(db.clone())
        .get(&profile.id)
        .unwrap()
        .unwrap();
    assert_eq!(attention.status, AdapterProfileStatus::NeedsAttention);
    assert_eq!(
        attention.last_error_code.as_deref(),
        Some("adapter.rollback_incomplete")
    );
    let restored = ProviderRepo::new(db.clone())
        .get_by_id(&mutated.id)
        .unwrap()
        .unwrap();
    assert_eq!(restored.settings_config, mutated.settings_config);
    assert!(restored.is_current);
    assert_eq!(
        ActiveBindingRepo::new(db)
            .get("claude")
            .unwrap()
            .unwrap()
            .provider_id
            .as_deref(),
        Some(mutated.id.as_str())
    );
    assert_eq!(fake.writes.load(Ordering::SeqCst), 2);
}

#[test]
fn active_retry_still_validates_the_membership_secret() {
    let (dir, db) = test_db();
    let source = kimi_source("kimi-source", "");
    let profile = active_profile(&source.id);
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    ProviderRepo::new(db.clone())
        .create(&generated_provider(&profile.id, &profile.source_id, true))
        .unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&profile)
        .unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        dir.path().join("backups"),
    );

    assert_eq!(
        service
            .apply(&request(&source.id, AgentId::Claude))
            .unwrap_err()
            .code(),
        "invalid_arg"
    );
    assert_eq!(
        AdapterProfileRepo::new(db)
            .get(&profile.id)
            .unwrap()
            .unwrap()
            .status,
        AdapterProfileStatus::Active
    );
}

#[test]
fn successful_apply_materializes_secret_only_in_live_config() {
    let (dir, db) = test_db();
    let source = kimi_source("kimi-source", "test-kimi-secret");
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    let fake = Arc::new(FakeClaudeAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let result = service
        .apply(&request(&source.id, AgentId::Claude))
        .unwrap();
    let stored = ProviderRepo::new(db.clone())
        .get_by_id(&result.provider.id)
        .unwrap()
        .unwrap();
    let profile = AdapterProfileRepo::new(db)
        .get(&result.profile.id)
        .unwrap()
        .unwrap();
    let live = fake.read_config().unwrap();

    assert_eq!(live.raw["env"]["ANTHROPIC_AUTH_TOKEN"], "test-kimi-secret");
    assert_eq!(
        stored.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        CONNECTION_SECRET_MARKER
    );
    assert!(!serde_json::to_string(&stored)
        .unwrap()
        .contains("test-kimi-secret"));
    assert!(!serde_json::to_string(&profile)
        .unwrap()
        .contains("test-kimi-secret"));
    assert_eq!(profile.status, AdapterProfileStatus::Active);
    assert!(!profile.auto_start);
    assert_eq!(profile.local_port, None);
    assert!(stored.is_current);
}

#[test]
fn remove_current_generated_provider_without_previous_still_deletes() {
    let (_dir, db) = test_db();
    let source_id = "kimi-source";
    let profile = active_profile(source_id);
    let provider = generated_provider(&profile.id, source_id, true);
    AdapterProfileRepo::new(db.clone())
        .create(&profile)
        .unwrap();
    ProviderRepo::new(db.clone()).create(&provider).unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        tempfile::tempdir().unwrap().path().join("backups"),
    );

    service.remove(&profile.id).unwrap();
    assert!(AdapterProfileRepo::new(db.clone())
        .get(&profile.id)
        .unwrap()
        .is_none());
    assert!(ProviderRepo::new(db)
        .get_by_id(&provider.id)
        .unwrap()
        .is_none());
}

#[test]
fn remove_missing_generated_provider_still_deletes_profile() {
    let (_dir, db) = test_db();
    let profile = active_profile("kimi-source");
    AdapterProfileRepo::new(db.clone())
        .create(&profile)
        .unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        tempfile::tempdir().unwrap().path().join("backups"),
    );

    service.remove(&profile.id).unwrap();
    assert!(AdapterProfileRepo::new(db)
        .get(&profile.id)
        .unwrap()
        .is_none());
}

#[test]
fn remove_is_excluded_by_the_claude_live_write_lock_before_profile_read() {
    let (dir, db) = test_db();
    let profile = active_profile("kimi-source");
    AdapterProfileRepo::new(db.clone())
        .create(&profile)
        .unwrap();
    let lock_dir = dir.path().join("locks");
    std::fs::create_dir_all(&lock_dir).unwrap();
    std::fs::write(lock_dir.join("provider-claude.lock"), b"held").unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        dir.path().join("backups"),
    );

    assert_eq!(
        service.remove(&profile.id).unwrap_err().code(),
        "provider.lock"
    );
    assert!(AdapterProfileRepo::new(db)
        .get(&profile.id)
        .unwrap()
        .is_some());
}

#[test]
fn local_bridge_is_rejected_without_side_effects() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("any-source", "test-kimi-secret"))
        .unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        dir.path().join("backups"),
    );

    let error = service
        .apply(&request("any-source", AgentId::Codex))
        .unwrap_err();
    assert_eq!(error.code(), "unsupported");
    assert!(AdapterProfileRepo::new(db)
        .list(None, None, None)
        .unwrap()
        .is_empty());
}

struct FakePiAdapter {
    config: Mutex<AgentConfig>,
    writes: AtomicUsize,
}

impl FakePiAdapter {
    fn new() -> Self {
        Self {
            config: Mutex::new(AgentConfig {
                agent: AgentId::Pi,
                raw: json!({}),
            }),
            writes: AtomicUsize::new(0),
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
        self.writes.fetch_add(1, Ordering::SeqCst);
        *self.config.lock().unwrap() = config.clone();
        Ok(())
    }
    fn read_auth(&self) -> Result<AuthState> {
        Err(AppError::Unsupported("fake".into()))
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
        Err(AppError::Unsupported("fake".into()))
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

fn generated_pi_kimi_provider(profile_id: &str, source_id: &str, current: bool) -> Provider {
    Provider {
        id: stable_id(PI_KIMI_PROVIDER_PREFIX, source_id),
        agent_id: AgentId::Pi,
        name: format!("Kimi Code ({})", safe_label(source_id)),
        settings_config: json!({
            "models": {
                "providers": {
                    KIMI_PI_PROVIDER_SLOT: {
                        "baseUrl": KIMI_PI_BASE_URL,
                        "apiKey": CONNECTION_SECRET_MARKER,
                        "api": "openai-completions",
                        "models": [{ "id": "kimi-k2.5" }],
                    }
                }
            },
            "settings": { "defaultProvider": KIMI_PI_PROVIDER_SLOT },
        }),
        meta: json!({
            "generatedBy": "adapter",
            "adapterRuleId": KIMI_PI_RULE_ID,
            "adapterRuleVersion": 1,
            "adapterSecretMode": "source_reference",
            "adapterProfileId": profile_id,
            "adapterSourceRef": {"kind": "provider", "id": source_id},
        }),
        is_current: current,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn active_pi_kimi_profile(source_id: &str) -> AdapterProfile {
    let profile_id = stable_id(PI_KIMI_PROFILE_PREFIX, source_id);
    AdapterProfile {
        id: profile_id,
        name: format!("Kimi → Pi ({})", safe_label(source_id)),
        source_kind: AdapterSourceKind::Provider,
        source_id: source_id.into(),
        target_agent_id: AgentId::Pi,
        route: AdapterRoute::ConfigSync,
        mode: crate::models::AdapterProfileMode::Api,
        status: AdapterProfileStatus::Active,
        rule_id: KIMI_PI_RULE_ID.into(),
        rule_version: RULE_VERSION.into(),
        generated_provider_id: Some(stable_id(PI_KIMI_PROVIDER_PREFIX, source_id)),
        local_port: None,
        auto_start: false,
        last_error_code: None,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

#[test]
fn pi_missing_membership_secret_creates_no_profile_or_generated_provider() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("kimi-source", ""))
        .unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        dir.path().join("backups"),
    );

    let error = service
        .apply(&request("kimi-source", AgentId::Pi))
        .unwrap_err();
    assert_eq!(error.code(), "invalid_arg");
    assert!(AdapterProfileRepo::new(db.clone())
        .list(None, None, None)
        .unwrap()
        .is_empty());
    assert_eq!(ProviderRepo::new(db).list(None).unwrap().len(), 1);
}

#[test]
fn pi_provider_id_collision_is_never_overwritten() {
    let (dir, db) = test_db();
    let source = kimi_source("kimi-source", "test-kimi-secret");
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    let generated_id = stable_id(PI_KIMI_PROVIDER_PREFIX, &source.id);
    let collision = Provider {
        id: generated_id.clone(),
        agent_id: AgentId::Pi,
        name: "user owned".into(),
        settings_config: json!({"models": {"providers": {"custom": {"apiKey": "user-secret"}}}}),
        meta: json!({"preset": "custom"}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    };
    ProviderRepo::new(db.clone()).create(&collision).unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        dir.path().join("backups"),
    );

    let error = service
        .apply(&request(&source.id, AgentId::Pi))
        .unwrap_err();
    assert_eq!(error.code(), "adapter.provider_conflict");
    assert_eq!(
        ProviderRepo::new(db)
            .get_by_id(&generated_id)
            .unwrap()
            .unwrap(),
        collision
    );
}

#[test]
fn pi_active_complete_projection_returns_existing_pair_without_switching() {
    let (dir, db) = test_db();
    let source = kimi_source("kimi-source", "test-kimi-secret");
    let profile = active_pi_kimi_profile(&source.id);
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    ProviderRepo::new(db.clone())
        .create(&generated_pi_kimi_provider(&profile.id, &source.id, true))
        .unwrap();
    AdapterProfileRepo::new(db.clone())
        .create(&profile)
        .unwrap();
    let service = AdapterApplyService::new(db, AdapterRegistry::new(), dir.path().join("backups"));

    let result = service.apply(&request(&source.id, AgentId::Pi)).unwrap();
    assert_eq!(result.profile.status, AdapterProfileStatus::Active);
    assert_eq!(result.provider.id, profile.generated_provider_id.unwrap());
    assert_eq!(result.profile.route, AdapterRoute::ConfigSync);
}

#[test]
fn pi_kimi_apply_sets_current_and_keeps_secret_out_of_dto() {
    let (dir, db) = test_db();
    let source = kimi_source("kimi-source", "test-kimi-secret");
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    let fake = Arc::new(FakePiAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let result = service.apply(&request(&source.id, AgentId::Pi)).unwrap();
    let stored = ProviderRepo::new(db.clone())
        .get_by_id(&result.provider.id)
        .unwrap()
        .unwrap();
    let profile = AdapterProfileRepo::new(db)
        .get(&result.profile.id)
        .unwrap()
        .unwrap();
    let live = fake.read_config().unwrap();

    assert!(result.provider.is_current);
    assert!(stored.is_current);
    assert_eq!(profile.status, AdapterProfileStatus::Active);
    assert_eq!(profile.route, AdapterRoute::ConfigSync);
    assert_eq!(profile.rule_id, KIMI_PI_RULE_ID);
    assert_eq!(
        live.raw["models"]["providers"][KIMI_PI_PROVIDER_SLOT]["apiKey"],
        "test-kimi-secret"
    );
    assert_eq!(
        live.raw["models"]["providers"][KIMI_PI_PROVIDER_SLOT]["baseUrl"],
        KIMI_PI_BASE_URL
    );
    assert_eq!(
        stored.settings_config["settings"]["defaultProvider"],
        KIMI_PI_PROVIDER_SLOT
    );
    assert_eq!(
        live.raw["settings"]["defaultProvider"],
        KIMI_PI_PROVIDER_SLOT
    );
    assert_eq!(
        stored.settings_config["models"]["providers"][KIMI_PI_PROVIDER_SLOT]["apiKey"],
        CONNECTION_SECRET_MARKER
    );
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("test-kimi-secret"));
    assert!(!serde_json::to_string(&stored)
        .unwrap()
        .contains("test-kimi-secret"));
    assert!(!serde_json::to_string(&profile)
        .unwrap()
        .contains("test-kimi-secret"));
}

#[test]
fn kimi_account_apply_writes_both_targets_with_account_source_ref() {
    let (dir, db) = test_db();
    AccountRepo::new(db.clone())
        .create(&kimi_account("kimi-account", "test-kimi-account-secret"))
        .unwrap();
    let fake_pi = Arc::new(FakePiAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(fake_pi.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let pi = service
        .apply(&account_request("kimi-account", AgentId::Pi))
        .unwrap();
    let pi_stored = ProviderRepo::new(db.clone())
        .get_by_id(&pi.provider.id)
        .unwrap()
        .unwrap();
    assert_eq!(pi.profile.source_kind, AdapterSourceKind::Account);
    assert_eq!(pi_stored.meta["adapterSourceRef"]["kind"], "account");
    assert_eq!(
        fake_pi.read_config().unwrap().raw["models"]["providers"][KIMI_PI_PROVIDER_SLOT]["apiKey"],
        "test-kimi-account-secret"
    );
    assert!(!serde_json::to_string(&pi_stored)
        .unwrap()
        .contains("test-kimi-account-secret"));

    let (_dir, claude_db) = test_db();
    AccountRepo::new(claude_db.clone())
        .create(&kimi_account("kimi-account", "test-kimi-account-secret"))
        .unwrap();
    let fake_claude = Arc::new(FakeClaudeAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(fake_claude.clone());
    let claude_service = AdapterApplyService::new(
        claude_db.clone(),
        registry,
        dir.path().join("claude-backups"),
    );
    let claude = claude_service
        .apply(&account_request("kimi-account", AgentId::Claude))
        .unwrap();
    assert_eq!(claude.profile.source_kind, AdapterSourceKind::Account);
    assert_eq!(
        fake_claude.read_config().unwrap().raw["env"]["ANTHROPIC_AUTH_TOKEN"],
        "test-kimi-account-secret"
    );
    assert!(!serde_json::to_string(&claude.provider)
        .unwrap()
        .contains("test-kimi-account-secret"));
}

#[test]
fn pi_anthropic_apply_sets_current_and_keeps_secret_out_of_dto() {
    let (dir, db) = test_db();
    let source = anthropic_source("anthropic-source", "sk-anthropic-secret");
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    let fake = Arc::new(FakePiAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let result = service.apply(&request(&source.id, AgentId::Pi)).unwrap();
    let stored = ProviderRepo::new(db)
        .get_by_id(&result.provider.id)
        .unwrap()
        .unwrap();
    let live = fake.read_config().unwrap();

    assert!(result.provider.is_current);
    assert_eq!(result.profile.rule_id, ANTHROPIC_PI_RULE_ID);
    assert_eq!(result.profile.route, AdapterRoute::ConfigSync);
    assert_eq!(
        live.raw["models"]["providers"][ANTHROPIC_PI_PROVIDER_SLOT]["apiKey"],
        "sk-anthropic-secret"
    );
    assert_eq!(
        stored.settings_config["models"]["providers"][ANTHROPIC_PI_PROVIDER_SLOT]["apiKey"],
        CONNECTION_SECRET_MARKER
    );
    assert_eq!(
        stored.settings_config["settings"]["defaultProvider"],
        ANTHROPIC_PI_PROVIDER_SLOT
    );
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("sk-anthropic-secret"));
}

#[test]
fn pi_missing_anthropic_secret_creates_no_profile() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&anthropic_source("anthropic-source", ""))
        .unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        dir.path().join("backups"),
    );

    assert_eq!(
        service
            .apply(&request("anthropic-source", AgentId::Pi))
            .unwrap_err()
            .code(),
        "invalid_arg"
    );
    assert!(AdapterProfileRepo::new(db)
        .list(None, None, None)
        .unwrap()
        .is_empty());
}

#[test]
fn remove_uses_target_agent_lock_not_hardcoded_claude() {
    let (dir, db) = test_db();
    let source_id = "kimi-source";
    let profile = active_pi_kimi_profile(source_id);
    AdapterProfileRepo::new(db.clone())
        .create(&profile)
        .unwrap();
    let lock_dir = dir.path().join("locks");
    std::fs::create_dir_all(&lock_dir).unwrap();
    std::fs::write(lock_dir.join("provider-claude.lock"), b"held").unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        dir.path().join("backups"),
    );
    service.remove(&profile.id).unwrap();
    assert!(AdapterProfileRepo::new(db.clone())
        .get(&profile.id)
        .unwrap()
        .is_none());

    let remaining = active_pi_kimi_profile("other-source");
    AdapterProfileRepo::new(db.clone())
        .create(&remaining)
        .unwrap();
    std::fs::write(lock_dir.join("provider-pi.lock"), b"held").unwrap();
    assert_eq!(
        service.remove(&remaining.id).unwrap_err().code(),
        "provider.lock"
    );
    assert!(AdapterProfileRepo::new(db)
        .get(&remaining.id)
        .unwrap()
        .is_some());
}

#[test]
fn pi_remove_current_restores_previous_and_deletes_projection() {
    let (dir, db) = test_db();
    let source_id = "kimi-source";
    let previous = Provider {
        id: "pi-previous".into(),
        agent_id: AgentId::Pi,
        name: "Previous Pi".into(),
        settings_config: json!({"models": {"providers": {}}}),
        meta: json!({}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    };
    let profile = active_pi_kimi_profile(source_id);
    let mut provider = generated_pi_kimi_provider(&profile.id, source_id, true);
    provider.meta["previousCurrentId"] = json!(previous.id);
    AdapterProfileRepo::new(db.clone())
        .create(&profile)
        .unwrap();
    ProviderRepo::new(db.clone()).create(&previous).unwrap();
    ProviderRepo::new(db.clone()).create(&provider).unwrap();
    let fake = Arc::new(FakePiAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(fake);
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    service.remove(&profile.id).unwrap();
    assert!(AdapterProfileRepo::new(db.clone())
        .get(&profile.id)
        .unwrap()
        .is_none());
    assert!(ProviderRepo::new(db.clone())
        .get_by_id(&provider.id)
        .unwrap()
        .is_none());
    let restored = ProviderRepo::new(db)
        .get_by_id(&previous.id)
        .unwrap()
        .unwrap();
    assert!(restored.is_current);
}

#[test]
fn coding_endpoint_without_preset_applies_to_claude_and_pi_without_leaking_secret() {
    let (dir, db) = test_db();
    let source = kimi_coding_live_import("kimi-live-import", "live-import-secret");
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    let claude = Arc::new(FakeClaudeAdapter::new());
    let pi = Arc::new(FakePiAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(claude.clone());
    registry.register(pi.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let claude_result = service
        .apply(&request(&source.id, AgentId::Claude))
        .unwrap();
    let pi_result = service.apply(&request(&source.id, AgentId::Pi)).unwrap();

    let claude_stored = ProviderRepo::new(db.clone())
        .get_by_id(&claude_result.provider.id)
        .unwrap()
        .unwrap();
    let pi_stored = ProviderRepo::new(db.clone())
        .get_by_id(&pi_result.provider.id)
        .unwrap()
        .unwrap();
    let claude_live = claude.read_config().unwrap();
    let pi_live = pi.read_config().unwrap();

    assert_eq!(claude_result.profile.rule_id, RULE_ID);
    assert_eq!(pi_result.profile.rule_id, KIMI_PI_RULE_ID);
    assert_eq!(
        claude_live.raw["env"]["ANTHROPIC_AUTH_TOKEN"],
        "live-import-secret"
    );
    assert_eq!(
        pi_live.raw["models"]["providers"][KIMI_PI_PROVIDER_SLOT]["apiKey"],
        "live-import-secret"
    );
    assert_eq!(
        claude_stored.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        CONNECTION_SECRET_MARKER
    );
    assert_eq!(
        pi_stored.settings_config["models"]["providers"][KIMI_PI_PROVIDER_SLOT]["apiKey"],
        CONNECTION_SECRET_MARKER
    );
    for payload in [
        serde_json::to_string(&claude_result).unwrap(),
        serde_json::to_string(&pi_result).unwrap(),
        serde_json::to_string(&claude_stored).unwrap(),
        serde_json::to_string(&pi_stored).unwrap(),
    ] {
        assert!(!payload.contains("live-import-secret"), "{payload}");
    }
}

#[test]
fn moonshot_and_bare_kimi_apply_creates_no_profile() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_moonshot("moonshot-source", "moonshot-secret"))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&kimi_bare("bare-source", "bare-secret"))
        .unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        dir.path().join("backups"),
    );

    for (id, target) in [
        ("moonshot-source", AgentId::Claude),
        ("moonshot-source", AgentId::Pi),
        ("bare-source", AgentId::Claude),
        ("bare-source", AgentId::Pi),
    ] {
        assert!(
            service.apply(&request(id, target)).is_err(),
            "{id} -> {target:?}"
        );
    }
    assert!(AdapterProfileRepo::new(db.clone())
        .list(None, None, None)
        .unwrap()
        .is_empty());
    assert_eq!(ProviderRepo::new(db).list(None).unwrap().len(), 2);
}

#[test]
fn pi_anthropic_account_apply_sets_source_ref_account_and_keeps_secret_out() {
    let (dir, db) = test_db();
    crate::storage::AccountRepo::new(db.clone())
        .create(&crate::models::Account {
            id: "anthropic-account".into(),
            agent_id: AgentId::Claude,
            kind: crate::models::AccountKind::ApiKey,
            label: "Anthropic key".into(),
            credentials: json!({
                "format": "api_key",
                "api_key": "sk-anthropic-secret"
            }),
            extra: json!({"provider": "anthropic"}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let fake = Arc::new(FakePiAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let result = service
        .apply(&AdapterApplyRequest {
            source_kind: AdapterSourceKind::Account,
            source_id: "anthropic-account".into(),
            target_agent_id: AgentId::Pi,
        })
        .unwrap();
    let stored = ProviderRepo::new(db)
        .get_by_id(&result.provider.id)
        .unwrap()
        .unwrap();
    let live = fake.read_config().unwrap();

    assert!(result.provider.is_current);
    assert_eq!(result.profile.source_kind, AdapterSourceKind::Account);
    assert_eq!(result.profile.rule_id, ANTHROPIC_PI_RULE_ID);
    assert_eq!(stored.meta["adapterSourceRef"]["kind"], "account");
    assert_eq!(stored.meta["adapterSourceRef"]["id"], "anthropic-account");
    assert_eq!(
        live.raw["models"]["providers"][ANTHROPIC_PI_PROVIDER_SLOT]["apiKey"],
        "sk-anthropic-secret"
    );
    assert_eq!(
        stored.settings_config["models"]["providers"][ANTHROPIC_PI_PROVIDER_SLOT]["apiKey"],
        CONNECTION_SECRET_MARKER
    );
    assert_eq!(
        stored.settings_config["settings"]["defaultProvider"],
        ANTHROPIC_PI_PROVIDER_SLOT
    );
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("sk-anthropic-secret"));
}

#[test]
fn pi_subscription_account_apply_uses_oauth_auth_slot_without_persisting_tokens() {
    let (dir, db) = test_db();
    let accounts = crate::storage::AccountRepo::new(db.clone());
    for (id, agent_id, credentials, slot, access, refresh, rule_id) in [
        (
            "claude-subscription",
            AgentId::Claude,
            json!({
                "format": "oauth",
                "access_token": "claude-access-secret",
                "refresh_token": "claude-refresh-secret",
                "expires_at": "2030-01-01T00:00:00Z"
            }),
            ANTHROPIC_PI_PROVIDER_SLOT,
            "claude-access-secret",
            "claude-refresh-secret",
            "claude-subscription-to-pi-v1",
        ),
        (
            "codex-subscription",
            AgentId::Codex,
            json!({
                "format": "auth_json",
                "tokens": {
                    "access_token": "codex-access-secret",
                    "refresh_token": "codex-refresh-secret"
                }
            }),
            "openai-codex",
            "codex-access-secret",
            "codex-refresh-secret",
            "codex-subscription-to-pi-v1",
        ),
        (
            "grok-subscription",
            AgentId::Grok,
            json!({
                "format": "oauth",
                "access_token": "grok-access-secret",
                "refresh_token": "grok-refresh-secret"
            }),
            XAI_PI_PROVIDER_SLOT,
            "grok-access-secret",
            "grok-refresh-secret",
            "grok-subscription-to-pi-v1",
        ),
    ] {
        accounts
            .create(&crate::models::Account {
                id: id.into(),
                agent_id,
                kind: crate::models::AccountKind::Oauth,
                label: id.into(),
                credentials,
                extra: json!({}),
                status: "active".into(),
                is_current: false,
                created_at: "now".into(),
                updated_at: "now".into(),
            })
            .unwrap();
        let fake = Arc::new(FakePiAdapter::new());
        let mut registry = AdapterRegistry::new();
        registry.register(fake.clone());
        let service = AdapterApplyService::new(
            db.clone(),
            registry,
            dir.path().join(format!("backups-{id}")),
        );

        let result = service
            .apply(&AdapterApplyRequest {
                source_kind: AdapterSourceKind::Account,
                source_id: id.into(),
                target_agent_id: AgentId::Pi,
            })
            .unwrap();
        let stored = ProviderRepo::new(db.clone())
            .get_by_id(&result.provider.id)
            .unwrap()
            .unwrap();
        let live = fake.read_config().unwrap();

        assert_eq!(result.profile.mode, AdapterProfileMode::Oauth);
        assert_eq!(result.profile.rule_id, rule_id);
        assert_eq!(live.raw["auth"][slot]["type"], "oauth");
        assert_eq!(live.raw["auth"][slot]["access"], access);
        assert_eq!(live.raw["auth"][slot]["refresh"], refresh);
        assert_eq!(
            stored.settings_config["auth"][slot]["access"],
            CONNECTION_SECRET_MARKER
        );
        assert_eq!(
            stored.settings_config["auth"][slot]["refresh"],
            CONNECTION_SECRET_MARKER
        );
        assert_eq!(stored.settings_config["settings"]["defaultProvider"], slot);
        assert_eq!(live.raw["settings"]["defaultProvider"], slot);
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains(access));
        assert!(!serialized.contains(refresh));
        assert!(!serde_json::to_string(&stored).unwrap().contains(access));
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

#[test]
fn pi_openai_and_xai_apply_sets_slot_and_keeps_secret_out() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&explicit_api_source(
            "openai-source",
            "openai",
            "OPENAI_API_KEY",
            "sk-openai-secret",
        ))
        .unwrap();
    crate::storage::AccountRepo::new(db.clone())
        .create(&crate::models::Account {
            id: "xai-account".into(),
            agent_id: AgentId::Grok,
            kind: crate::models::AccountKind::ApiKey,
            label: "xAI key".into(),
            credentials: json!({
                "format": "api_key",
                "api_key": "xai-account-secret"
            }),
            extra: json!({"provider": "xai"}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let fake = Arc::new(FakePiAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let openai = service
        .apply(&request("openai-source", AgentId::Pi))
        .unwrap();
    assert_eq!(openai.profile.rule_id, OPENAI_PI_RULE_ID);
    let openai_stored = ProviderRepo::new(db.clone())
        .get_by_id(&openai.provider.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        openai_stored.settings_config["models"]["providers"][OPENAI_PI_PROVIDER_SLOT]["apiKey"],
        CONNECTION_SECRET_MARKER
    );
    assert_eq!(
        openai_stored.settings_config["settings"]["defaultProvider"],
        OPENAI_PI_PROVIDER_SLOT
    );
    assert_eq!(
        fake.read_config().unwrap().raw["models"]["providers"][OPENAI_PI_PROVIDER_SLOT]["apiKey"],
        "sk-openai-secret"
    );

    let xai = service
        .apply(&AdapterApplyRequest {
            source_kind: AdapterSourceKind::Account,
            source_id: "xai-account".into(),
            target_agent_id: AgentId::Pi,
        })
        .unwrap();
    assert_eq!(xai.profile.rule_id, XAI_PI_RULE_ID);
    assert_eq!(xai.profile.source_kind, AdapterSourceKind::Account);
    let xai_stored = ProviderRepo::new(db)
        .get_by_id(&xai.provider.id)
        .unwrap()
        .unwrap();
    assert_eq!(xai_stored.meta["adapterSourceRef"]["kind"], "account");
    assert_eq!(
        xai_stored.settings_config["models"]["providers"][XAI_PI_PROVIDER_SLOT]["apiKey"],
        CONNECTION_SECRET_MARKER
    );
    assert_eq!(
        xai_stored.settings_config["settings"]["defaultProvider"],
        XAI_PI_PROVIDER_SLOT
    );
    assert!(!serde_json::to_string(&openai)
        .unwrap()
        .contains("sk-openai-secret"));
    assert!(!serde_json::to_string(&xai)
        .unwrap()
        .contains("xai-account-secret"));
}

#[test]
fn pi_glm_and_deepseek_apply_sets_custom_provider_contract_and_keeps_secret_out() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&explicit_api_source(
            "glm-source",
            "glm-coding-plan",
            ANTHROPIC_AUTH_TOKEN_ENV,
            "glm-pi-secret",
        ))
        .unwrap();
    crate::storage::AccountRepo::new(db.clone())
        .create(&crate::models::Account {
            id: "deepseek-account".into(),
            agent_id: AgentId::Claude,
            kind: crate::models::AccountKind::ApiKey,
            label: "DeepSeek key".into(),
            credentials: json!({
                "format": "api_key",
                "api_key": "deepseek-pi-secret"
            }),
            extra: json!({"provider": "deepseek-api"}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let fake = Arc::new(FakePiAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let glm = service
        .apply(&AdapterApplyRequest {
            source_kind: AdapterSourceKind::Provider,
            source_id: "glm-source".into(),
            target_agent_id: AgentId::Pi,
        })
        .unwrap();
    let glm_stored = ProviderRepo::new(db.clone())
        .get_by_id(&glm.provider.id)
        .unwrap()
        .unwrap();
    assert_eq!(glm.profile.rule_id, "glm-coding-plan-to-pi-v1");
    assert_eq!(
        glm_stored.settings_config["models"]["providers"][GLM_PI_PROVIDER_SLOT]["baseUrl"],
        GLM_PI_BASE_URL
    );
    assert_eq!(
        glm_stored.settings_config["models"]["providers"][GLM_PI_PROVIDER_SLOT]["api"],
        "openai-completions"
    );
    assert_eq!(
        glm_stored.settings_config["models"]["providers"][GLM_PI_PROVIDER_SLOT]["models"][0]["id"],
        "glm-4.6"
    );
    assert_eq!(
        glm_stored.settings_config["models"]["providers"][GLM_PI_PROVIDER_SLOT]["apiKey"],
        CONNECTION_SECRET_MARKER
    );
    assert_eq!(
        glm_stored.settings_config["settings"]["defaultProvider"],
        GLM_PI_PROVIDER_SLOT
    );
    assert_eq!(
        fake.read_config().unwrap().raw["models"]["providers"][GLM_PI_PROVIDER_SLOT]["apiKey"],
        "glm-pi-secret"
    );

    let deepseek = service
        .apply(&AdapterApplyRequest {
            source_kind: AdapterSourceKind::Account,
            source_id: "deepseek-account".into(),
            target_agent_id: AgentId::Pi,
        })
        .unwrap();
    let deepseek_stored = ProviderRepo::new(db)
        .get_by_id(&deepseek.provider.id)
        .unwrap()
        .unwrap();
    assert_eq!(deepseek.profile.rule_id, "deepseek-api-to-pi-v1");
    assert_eq!(
        deepseek_stored.settings_config["models"]["providers"][DEEPSEEK_PI_PROVIDER_SLOT]
            ["baseUrl"],
        DEEPSEEK_API_BASE_URL
    );
    assert_eq!(
        deepseek_stored.settings_config["models"]["providers"][DEEPSEEK_PI_PROVIDER_SLOT]["api"],
        "openai-completions"
    );
    assert_eq!(
        deepseek_stored.settings_config["models"]["providers"][DEEPSEEK_PI_PROVIDER_SLOT]["models"]
            [0]["id"],
        "deepseek-chat"
    );
    assert_eq!(
        deepseek_stored.settings_config["models"]["providers"][DEEPSEEK_PI_PROVIDER_SLOT]["apiKey"],
        CONNECTION_SECRET_MARKER
    );
    assert_eq!(
        deepseek_stored.settings_config["settings"]["defaultProvider"],
        DEEPSEEK_PI_PROVIDER_SLOT
    );
    assert_eq!(
        fake.read_config().unwrap().raw["models"]["providers"][DEEPSEEK_PI_PROVIDER_SLOT]["apiKey"],
        "deepseek-pi-secret"
    );
    assert!(!serde_json::to_string(&glm)
        .unwrap()
        .contains("glm-pi-secret"));
    assert!(!serde_json::to_string(&deepseek)
        .unwrap()
        .contains("deepseek-pi-secret"));
}

#[test]
fn glm_and_deepseek_claude_apply_writes_rule_base_url_and_keeps_kimi_url() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&explicit_api_source(
            "glm-source",
            "glm-coding-plan",
            ANTHROPIC_AUTH_TOKEN_ENV,
            "glm-secret",
        ))
        .unwrap();
    crate::storage::AccountRepo::new(db.clone())
        .create(&crate::models::Account {
            id: "deepseek-account".into(),
            agent_id: AgentId::Claude,
            kind: crate::models::AccountKind::ApiKey,
            label: "DeepSeek key".into(),
            credentials: json!({
                "format": "api_key",
                "api_key": "deepseek-account-secret"
            }),
            extra: json!({"provider": "deepseek-api"}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("kimi-source", "test-kimi-secret"))
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
    let fake = Arc::new(FakeClaudeAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let glm = service
        .apply(&request("glm-source", AgentId::Claude))
        .unwrap();
    assert_eq!(glm.profile.rule_id, GLM_CLAUDE_RULE_ID);
    let glm_stored = ProviderRepo::new(db.clone())
        .get_by_id(&glm.provider.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        glm_stored.settings_config["env"]["ANTHROPIC_BASE_URL"],
        GLM_CLAUDE_BASE_URL
    );
    assert_eq!(
        glm_stored.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        CONNECTION_SECRET_MARKER
    );
    assert_eq!(
        fake.read_config().unwrap().raw["env"]["ANTHROPIC_AUTH_TOKEN"],
        "glm-secret"
    );
    assert!(!serde_json::to_string(&glm).unwrap().contains("glm-secret"));

    let deepseek = service
        .apply(&AdapterApplyRequest {
            source_kind: AdapterSourceKind::Account,
            source_id: "deepseek-account".into(),
            target_agent_id: AgentId::Claude,
        })
        .unwrap();
    assert_eq!(deepseek.profile.rule_id, DEEPSEEK_CLAUDE_RULE_ID);
    assert_eq!(deepseek.profile.source_kind, AdapterSourceKind::Account);
    let deepseek_stored = ProviderRepo::new(db.clone())
        .get_by_id(&deepseek.provider.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        deepseek_stored.settings_config["env"]["ANTHROPIC_BASE_URL"],
        DEEPSEEK_CLAUDE_BASE_URL
    );
    assert_eq!(deepseek_stored.meta["adapterSourceRef"]["kind"], "account");
    assert!(!serde_json::to_string(&deepseek)
        .unwrap()
        .contains("deepseek-account-secret"));

    let kimi = service
        .apply(&request("kimi-source", AgentId::Claude))
        .unwrap();
    assert_eq!(kimi.profile.rule_id, RULE_ID);
    let kimi_stored = ProviderRepo::new(db.clone())
        .get_by_id(&kimi.provider.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        kimi_stored.settings_config["env"]["ANTHROPIC_BASE_URL"],
        KIMI_CLAUDE_BASE_URL
    );

    assert!(service
        .apply(&request("relay-source", AgentId::Claude))
        .is_err());
}

#[test]
fn glm_claude_finalize_failure_restores_previous_current() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&explicit_api_source(
            "glm-source",
            "glm-coding-plan",
            ANTHROPIC_AUTH_TOKEN_ENV,
            "glm-secret",
        ))
        .unwrap();
    let previous = Provider {
        id: "previous-claude".into(),
        agent_id: AgentId::Claude,
        name: "Previous Claude".into(),
        settings_config: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "previous-secret"}}),
        meta: json!({}),
        is_current: true,
        created_at: "now".into(),
        updated_at: "now".into(),
    };
    ProviderRepo::new(db.clone()).create(&previous).unwrap();
    ActiveBindingRepo::new(db.clone())
        .set_refs(
            "claude",
            None,
            Some(previous.id.clone()),
            None,
            "before-glm-apply",
        )
        .unwrap();
    db.with_conn(|conn| {
        conn.execute_batch(
            r#"
            CREATE TRIGGER fail_adapter_profile_finalize_glm
            BEFORE UPDATE OF status ON adapter_profiles
            WHEN NEW.status = 'active'
            BEGIN
                SELECT RAISE(ABORT, 'injected adapter profile finalization failure');
            END;
            "#,
        )?;
        Ok(())
    })
    .unwrap();
    let fake = Arc::new(FakeClaudeAdapter::new());
    fake.write_config(&crate::models::AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "previous-secret"}}),
    })
    .unwrap();
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let error = service
        .apply(&request("glm-source", AgentId::Claude))
        .unwrap_err();
    assert_eq!(error.code(), "adapter.profile_finalize");
    assert_eq!(
        ActiveBindingRepo::new(db.clone())
            .get("claude")
            .unwrap()
            .unwrap()
            .provider_id
            .as_deref(),
        Some(previous.id.as_str())
    );
    assert!(
        ProviderRepo::new(db)
            .get_by_id(&previous.id)
            .unwrap()
            .unwrap()
            .is_current
    );
    assert_eq!(
        fake.read_config().unwrap().raw["env"]["ANTHROPIC_AUTH_TOKEN"],
        "previous-secret"
    );
}

struct FakeDshAdapter {
    config: Mutex<AgentConfig>,
}

impl FakeDshAdapter {
    fn new() -> Self {
        Self {
            config: Mutex::new(AgentConfig {
                agent: AgentId::Dsh,
                raw: json!({}),
            }),
        }
    }
}

impl AgentAdapter for FakeDshAdapter {
    fn id(&self) -> AgentId {
        AgentId::Dsh
    }
    fn detect(&self) -> DetectResult {
        DetectResult {
            agent: AgentId::Dsh,
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
        Err(AppError::Unsupported("fake".into()))
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
        Err(AppError::Unsupported("fake".into()))
    }
}

fn deepseek_api_source(id: &str, api_key: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Claude,
        name: "DeepSeek API".into(),
        settings_config: json!({"apiKey": api_key}),
        meta: json!({"preset": "deepseek"}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn dsh_home_only_source(id: &str, api_key: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Dsh,
        name: "DSH row without DeepSeek ticket marks".into(),
        settings_config: json!({"apiKey": api_key}),
        meta: json!({}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

#[test]
fn dsh_deepseek_apply_sets_current_and_keeps_secret_out_of_dto() {
    let (dir, db) = test_db();
    let source = deepseek_api_source("ds-source", "sk-deepseek-secret");
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    let fake = Arc::new(FakeDshAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let result = service.apply(&request(&source.id, AgentId::Dsh)).unwrap();
    let stored = ProviderRepo::new(db.clone())
        .get_by_id(&result.provider.id)
        .unwrap()
        .unwrap();
    let profile = AdapterProfileRepo::new(db)
        .get(&result.profile.id)
        .unwrap()
        .unwrap();
    let live = fake.read_config().unwrap();

    assert!(result.provider.is_current);
    assert!(stored.is_current);
    assert_eq!(profile.status, AdapterProfileStatus::Active);
    assert_eq!(profile.route, AdapterRoute::ConfigSync);
    assert_eq!(profile.rule_id, DEEPSEEK_DSH_RULE_ID);
    assert_eq!(live.raw["api_key"], "sk-deepseek-secret");
    assert_eq!(live.raw["provider"], DSH_DEEPSEEK_PROVIDER_SLOT);
    assert_eq!(live.raw["apiKeyEnv"], DSH_API_KEY_ENV);
    assert_eq!(stored.settings_config["api_key"], CONNECTION_SECRET_MARKER);
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("sk-deepseek-secret"));
    assert!(!serde_json::to_string(&stored)
        .unwrap()
        .contains("sk-deepseek-secret"));
    assert!(!serde_json::to_string(&profile)
        .unwrap()
        .contains("sk-deepseek-secret"));
}

#[test]
fn dsh_missing_secret_creates_no_profile() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&deepseek_api_source("ds-source", ""))
        .unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        dir.path().join("backups"),
    );

    assert!(service.apply(&request("ds-source", AgentId::Dsh)).is_err());
    assert!(AdapterProfileRepo::new(db.clone())
        .list(None, None, None)
        .unwrap()
        .is_empty());
    assert_eq!(ProviderRepo::new(db).list(None).unwrap().len(), 1);
}

#[test]
fn dsh_agent_id_alone_does_not_apply() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&dsh_home_only_source("dsh-only", "sk-not-a-ticket"))
        .unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        dir.path().join("backups"),
    );

    assert!(service.apply(&request("dsh-only", AgentId::Dsh)).is_err());
    assert!(AdapterProfileRepo::new(db.clone())
        .list(None, None, None)
        .unwrap()
        .is_empty());
}

#[test]
fn deepseek_host_without_preset_applies_to_dsh() {
    let (dir, db) = test_db();
    let source = Provider {
        id: "ds-host".into(),
        agent_id: AgentId::Claude,
        name: "DeepSeek host".into(),
        settings_config: json!({
            "apiKey": "sk-from-host",
            "baseUrl": "https://api.deepseek.com",
        }),
        meta: json!({}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    };
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    let fake = Arc::new(FakeDshAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db, registry, dir.path().join("backups"));

    let result = service.apply(&request(&source.id, AgentId::Dsh)).unwrap();
    assert_eq!(result.profile.rule_id, DEEPSEEK_DSH_RULE_ID);
    assert_eq!(fake.read_config().unwrap().raw["api_key"], "sk-from-host");
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("sk-from-host"));
}

#[test]
fn glm_and_deepseek_native_codex_apply_materializes_official_responses_config() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&explicit_api_source(
            "glm-codex-source",
            "glm-coding-plan",
            ANTHROPIC_AUTH_TOKEN_ENV,
            "glm-codex-secret",
        ))
        .unwrap();
    crate::storage::AccountRepo::new(db.clone())
        .create(&explicit_api_account(
            "deepseek-codex-account",
            "deepseek-api",
            "deepseek-codex-secret",
        ))
        .unwrap();
    let fake = Arc::new(FakeClaudeAdapter::new_for(AgentId::Codex));
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let glm = service
        .apply(&AdapterApplyRequest {
            source_kind: AdapterSourceKind::Provider,
            source_id: "glm-codex-source".into(),
            target_agent_id: AgentId::Codex,
        })
        .unwrap();
    let deepseek = service
        .apply(&AdapterApplyRequest {
            source_kind: AdapterSourceKind::Account,
            source_id: "deepseek-codex-account".into(),
            target_agent_id: AgentId::Codex,
        })
        .unwrap();

    assert_eq!(glm.profile.rule_id, "glm-coding-plan-to-codex-v1");
    assert_eq!(deepseek.profile.rule_id, "deepseek-api-to-codex-v1");
    let live = fake.read_config().unwrap().raw;
    let live_content = live["content"].as_str().unwrap();
    assert!(live_content.contains("base_url = \"https://api.deepseek.com\""));
    assert!(live_content.contains("model = \"deepseek-v4-flash\""));
    assert!(live_content.contains("wire_api = \"responses\""));
    assert!(live_content.contains("experimental_bearer_token = \"deepseek-codex-secret\""));
    assert_eq!(live["auth"]["OPENAI_API_KEY"], "deepseek-codex-secret");

    for result in [glm, deepseek] {
        let stored = ProviderRepo::new(db.clone())
            .get_by_id(&result.provider.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.settings_config["format"], "toml");
        assert!(stored.settings_config["content"]
            .as_str()
            .unwrap()
            .contains("wire_api = \"responses\""));
        assert_eq!(
            stored.settings_config["auth"]["OPENAI_API_KEY"],
            CONNECTION_SECRET_MARKER
        );
        assert!(!serde_json::to_string(&stored).unwrap().contains("secret"));
    }
}

#[test]
fn deepseek_preset_alias_applies_to_claude() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&deepseek_api_source("ds-source", "sk-deepseek-secret"))
        .unwrap();
    let fake = Arc::new(FakeClaudeAdapter::new());
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db, registry, dir.path().join("backups"));

    let result = service
        .apply(&request("ds-source", AgentId::Claude))
        .unwrap();
    assert_eq!(result.profile.rule_id, DEEPSEEK_CLAUDE_RULE_ID);
    assert_eq!(
        fake.read_config().unwrap().raw["env"]["ANTHROPIC_BASE_URL"],
        DEEPSEEK_CLAUDE_BASE_URL
    );
    assert_eq!(
        fake.read_config().unwrap().raw["env"]["ANTHROPIC_AUTH_TOKEN"],
        "sk-deepseek-secret"
    );
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("sk-deepseek-secret"));
}

fn openai_api_source(id: &str, api_key: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Codex,
        name: "OpenAI API".into(),
        settings_config: json!({"apiKey": api_key}),
        meta: json!({"preset": "openai"}),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn openai_api_account(id: &str, api_key: &str) -> Account {
    Account {
        id: id.into(),
        agent_id: AgentId::Codex,
        kind: AccountKind::ApiKey,
        label: "OpenAI key".into(),
        credentials: json!({
            "format": "api_key",
            "api_key": api_key,
            "provider": "openai",
        }),
        extra: json!({"provider": "openai"}),
        status: "active".into(),
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn assert_grok_toml(content: &str, alias: &str, base_url: &str, model: &str, api_key: &str) {
    let doc: toml_edit::DocumentMut = content.parse().expect("grok toml");
    assert_eq!(doc["models"]["default"].as_str(), Some(alias));
    let entry = &doc["model"][alias];
    assert_eq!(entry["base_url"].as_str(), Some(base_url));
    assert_eq!(entry["model"].as_str(), Some(model));
    assert_eq!(entry["api_backend"].as_str(), Some("chat_completions"));
    assert_eq!(entry["api_key"].as_str(), Some(api_key));
}

fn assert_grok_apply(
    db: &Database,
    fake: &FakeClaudeAdapter,
    result: &AdapterApplyResult,
    rule_id: &str,
    alias: &str,
    base_url: &str,
    model: &str,
    secret: &str,
) {
    assert_eq!(result.profile.rule_id, rule_id);
    assert_eq!(result.profile.route, AdapterRoute::NativeEndpoint);
    let stored = ProviderRepo::new(db.clone())
        .get_by_id(&result.provider.id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.settings_config["format"], "toml");
    assert_grok_toml(
        stored.settings_config["content"].as_str().unwrap(),
        alias,
        base_url,
        model,
        CONNECTION_SECRET_MARKER,
    );
    let live = fake.read_config().unwrap().raw;
    assert_eq!(live["format"], "toml");
    assert_grok_toml(
        live["content"].as_str().unwrap(),
        alias,
        base_url,
        model,
        secret,
    );
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(!serialized.contains(secret));
    assert!(!serde_json::to_string(&stored).unwrap().contains(secret));
}

#[test]
fn apply_kimi_membership_and_openai_api_to_grok() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("kimi-source", "kimi-grok-secret"))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&openai_api_source("openai-source", "openai-grok-secret"))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&kimi_account("kimi-account", "kimi-account-secret"))
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&openai_api_account(
            "openai-account",
            "openai-account-secret",
        ))
        .unwrap();
    let fake = Arc::new(FakeClaudeAdapter::new_for(AgentId::Grok));
    let mut registry = AdapterRegistry::new();
    registry.register(fake.clone());
    let service = AdapterApplyService::new(db.clone(), registry, dir.path().join("backups"));

    let kimi = service
        .apply(&request("kimi-source", AgentId::Grok))
        .unwrap();
    assert_grok_apply(
        &db,
        fake.as_ref(),
        &kimi,
        KIMI_GROK_RULE_ID,
        "agenthub_kimi",
        KIMI_GROK_BASE_URL,
        KIMI_GROK_DEFAULT_MODEL,
        "kimi-grok-secret",
    );

    let openai = service
        .apply(&request("openai-source", AgentId::Grok))
        .unwrap();
    assert_grok_apply(
        &db,
        fake.as_ref(),
        &openai,
        OPENAI_GROK_RULE_ID,
        "agenthub_openai",
        OPENAI_GROK_BASE_URL,
        OPENAI_GROK_DEFAULT_MODEL,
        "openai-grok-secret",
    );

    let kimi_account_result = service
        .apply(&account_request("kimi-account", AgentId::Grok))
        .unwrap();
    assert_grok_apply(
        &db,
        fake.as_ref(),
        &kimi_account_result,
        KIMI_GROK_RULE_ID,
        "agenthub_kimi",
        KIMI_GROK_BASE_URL,
        KIMI_GROK_DEFAULT_MODEL,
        "kimi-account-secret",
    );

    let openai_account_result = service
        .apply(&account_request("openai-account", AgentId::Grok))
        .unwrap();
    assert_grok_apply(
        &db,
        fake.as_ref(),
        &openai_account_result,
        OPENAI_GROK_RULE_ID,
        "agenthub_openai",
        OPENAI_GROK_BASE_URL,
        OPENAI_GROK_DEFAULT_MODEL,
        "openai-account-secret",
    );
}

#[test]
fn apply_rejects_local_bridge_and_non_grok_sources() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&kimi_source("kimi-source", "kimi-secret"))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&kimi_moonshot("moonshot-source", "moonshot-secret"))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&kimi_bare("bare-source", "bare-secret"))
        .unwrap();
    ProviderRepo::new(db.clone())
        .create(&explicit_api_source(
            "xai-source",
            "xai",
            "XAI_API_KEY",
            "xai-secret",
        ))
        .unwrap();
    let fake = Arc::new(FakeClaudeAdapter::new_for(AgentId::Grok));
    let mut registry = AdapterRegistry::new();
    registry.register(fake);
    let service = AdapterApplyService::new(db, registry, dir.path().join("backups"));

    let local_bridge = service
        .apply(&request("kimi-source", AgentId::Codex))
        .unwrap_err();
    assert!(matches!(local_bridge, AppError::Unsupported(_)));

    for source in ["moonshot-source", "bare-source", "xai-source"] {
        let err = service.apply(&request(source, AgentId::Grok)).unwrap_err();
        assert!(
            matches!(err, AppError::Unsupported(_)),
            "{source} should not apply to Grok: {err}"
        );
    }
}
