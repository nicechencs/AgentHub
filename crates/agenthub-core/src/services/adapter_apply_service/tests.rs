use super::*;
use crate::adapters::AgentAdapter;
use crate::models::Provider;
use crate::models::{
    AgentConfig, AuthState, Capability, CapabilityState, DetectResult, DetectStatus,
    InstallChannel, RunOptions, RunSpec,
};
use crate::storage::{ActiveBindingRepo, ProviderRepo};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct FakeClaudeAdapter {
    config: Mutex<AgentConfig>,
    writes: AtomicUsize,
    fail_on_write: AtomicUsize,
    fail_writes: Mutex<Vec<usize>>,
}

impl FakeClaudeAdapter {
    fn new() -> Self {
        Self {
            config: Mutex::new(AgentConfig {
                agent: AgentId::Claude,
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
fn remove_rejects_current_generated_provider_without_deleting_profile() {
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

    let error = service.remove(&profile.id).unwrap_err();
    assert_eq!(error.code(), "unsupported");
    assert!(error.to_string().contains("Connections"));
    assert!(AdapterProfileRepo::new(db.clone())
        .get(&profile.id)
        .unwrap()
        .is_some());
    assert!(ProviderRepo::new(db)
        .get_by_id(&provider.id)
        .unwrap()
        .is_some());
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
            }
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
fn pi_remove_rejects_current_generated_provider_without_deleting_profile() {
    let (_dir, db) = test_db();
    let source_id = "kimi-source";
    let profile = active_pi_kimi_profile(source_id);
    let provider = generated_pi_kimi_provider(&profile.id, source_id, true);
    AdapterProfileRepo::new(db.clone())
        .create(&profile)
        .unwrap();
    ProviderRepo::new(db.clone()).create(&provider).unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        tempfile::tempdir().unwrap().path().join("backups"),
    );

    let error = service.remove(&profile.id).unwrap_err();
    assert_eq!(error.code(), "unsupported");
    assert!(error.to_string().contains("Connections"));
    assert!(AdapterProfileRepo::new(db.clone())
        .get(&profile.id)
        .unwrap()
        .is_some());
    assert!(ProviderRepo::new(db)
        .get_by_id(&provider.id)
        .unwrap()
        .is_some());
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
fn pi_account_source_apply_is_rejected_without_side_effects() {
    let (dir, db) = test_db();
    crate::storage::AccountRepo::new(db.clone())
        .create(&crate::models::Account {
            id: "anthropic-account".into(),
            agent_id: AgentId::Claude,
            kind: crate::models::AccountKind::ApiKey,
            label: "Anthropic key".into(),
            credentials: json!({"api_key": "sk-anthropic-secret"}),
            extra: json!({"provider": "anthropic"}),
            status: "active".into(),
            is_current: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        })
        .unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        dir.path().join("backups"),
    );

    let error = service
        .apply(&AdapterApplyRequest {
            source_kind: AdapterSourceKind::Account,
            source_id: "anthropic-account".into(),
            target_agent_id: AgentId::Pi,
        })
        .unwrap_err();
    assert_eq!(error.code(), "unsupported");
    assert!(AdapterProfileRepo::new(db.clone())
        .list(None, None, None)
        .unwrap()
        .is_empty());
    assert_eq!(ProviderRepo::new(db).list(None).unwrap().len(), 0);
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

    assert!(service
        .apply(&request("ds-source", AgentId::Dsh))
        .is_err());
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

    assert!(service
        .apply(&request("dsh-only", AgentId::Dsh))
        .is_err());
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
    assert!(!serde_json::to_string(&result).unwrap().contains("sk-from-host"));
}

#[test]
fn deepseek_api_to_claude_apply_is_rejected() {
    let (dir, db) = test_db();
    ProviderRepo::new(db.clone())
        .create(&deepseek_api_source("ds-source", "sk-deepseek-secret"))
        .unwrap();
    let service = AdapterApplyService::new(
        db.clone(),
        AdapterRegistry::new(),
        dir.path().join("backups"),
    );

    let error = service
        .apply(&request("ds-source", AgentId::Claude))
        .unwrap_err();
    assert_eq!(error.code(), "unsupported");
    assert!(AdapterProfileRepo::new(db.clone())
        .list(None, None, None)
        .unwrap()
        .is_empty());
}
