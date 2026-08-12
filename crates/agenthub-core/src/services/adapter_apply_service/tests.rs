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
