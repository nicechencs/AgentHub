use super::*;
use crate::adapters::AgentAdapter;
use crate::models::{
    AuthState, Capability, CapabilityState, DetectResult, DetectStatus, InstallChannel, Provider,
    RunOptions, RunSpec,
};
use crate::storage::ProviderRepo;
use crate::utils::atomic::atomic_write;
use serde_json::json;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use tempfile::tempdir;

struct FakeAdapter {
    id: AgentId,
    live: Mutex<Option<LiveAccount>>,
    path: PathBuf,
    write_attempts: AtomicUsize,
    fail_on_write: AtomicUsize,
    fail_writes: Mutex<Vec<usize>>,
    supports: AtomicBool,
    auth_state: Mutex<Option<AuthState>>,
    revision_sequence: Mutex<Vec<String>>,
    auth_read_count: AtomicUsize,
    auth_read_hook: Mutex<Option<(usize, Box<dyn FnOnce() + Send>)>>,
    reject_api_key_apply: AtomicBool,
}

impl FakeAdapter {
    fn new(id: AgentId, path: PathBuf) -> Self {
        Self {
            id,
            live: Mutex::new(None),
            path,
            write_attempts: AtomicUsize::new(0),
            fail_on_write: AtomicUsize::new(0),
            fail_writes: Mutex::new(Vec::new()),
            supports: AtomicBool::new(true),
            auth_state: Mutex::new(None),
            revision_sequence: Mutex::new(Vec::new()),
            auth_read_count: AtomicUsize::new(0),
            auth_read_hook: Mutex::new(None),
            reject_api_key_apply: AtomicBool::new(false),
        }
    }

    fn reject_api_key_apply(&self) {
        self.reject_api_key_apply.store(true, Ordering::SeqCst);
    }

    fn set_live(&self, live: LiveAccount) {
        let body = serde_json::to_vec(&live).unwrap();
        atomic_write(&self.path, &body).unwrap();
        *self.live.lock().unwrap() = Some(live);
    }

    fn set_auth_state(&self, state: AuthState) {
        *self.auth_state.lock().unwrap() = Some(state);
    }

    fn set_revision_sequence(&self, revisions: &[&str]) {
        *self.revision_sequence.lock().unwrap() = revisions
            .iter()
            .map(|revision| (*revision).to_owned())
            .collect();
    }

    fn fail_writes_on(&self, attempts: &[usize]) {
        *self.fail_writes.lock().unwrap() = attempts.to_vec();
    }

    fn on_auth_read(&self, count: usize, hook: impl FnOnce() + Send + 'static) {
        *self.auth_read_hook.lock().unwrap() = Some((count, Box::new(hook)));
    }
}

impl AgentAdapter for FakeAdapter {
    fn id(&self) -> AgentId {
        self.id
    }

    fn detect(&self) -> DetectResult {
        DetectResult {
            agent: self.id,
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

    fn read_config(&self) -> Result<crate::models::AgentConfig> {
        Err(AppError::Unsupported("fake".into()))
    }

    fn read_auth(&self) -> Result<AuthState> {
        let mut state = self
            .auth_state
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| AppError::Unsupported("fake".into()))?;
        let count = self.auth_read_count.fetch_add(1, Ordering::SeqCst) + 1;
        let hook = {
            let mut scheduled = self.auth_read_hook.lock().unwrap();
            match scheduled.as_ref() {
                Some((scheduled_count, _)) if *scheduled_count == count => {
                    scheduled.take().map(|(_, hook)| hook)
                }
                _ => None,
            }
        };
        if let Some(hook) = hook {
            hook();
        }
        let revision = {
            let mut revisions = self.revision_sequence.lock().unwrap();
            (!revisions.is_empty()).then(|| revisions.remove(0))
        };
        if let Some(revision) = revision {
            state.revision = Some(revision);
        }
        Ok(state)
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        match cap {
            Capability::AccountSwitch if self.supports.load(Ordering::SeqCst) => {
                CapabilityState::full()
            }
            Capability::AccountSwitch => CapabilityState::unsupported("fake account switch off"),
            Capability::LiveBackup => CapabilityState::full(),
            _ => CapabilityState::unsupported("fake"),
        }
    }

    fn read_account(&self) -> Result<LiveAccount> {
        self.live
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| AppError::NotFound("no live account".into()))
    }

    fn apply_account(&self, account: &LiveAccount) -> Result<()> {
        if self.reject_api_key_apply.load(Ordering::SeqCst)
            && account
                .credentials
                .get("format")
                .and_then(|value| value.as_str())
                == Some("api_key")
        {
            return Err(AppError::Unsupported(
                "Codex live apply for API key accounts is not supported".into(),
            ));
        }
        let attempt = self.write_attempts.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_on_write.load(Ordering::SeqCst) == attempt
            || self.fail_writes.lock().unwrap().contains(&attempt)
        {
            return Err(AppError::message(
                "test.write",
                format!("injected write failure {attempt}"),
            ));
        }
        let bytes = serde_json::to_vec(account)?;
        atomic_write(&self.path, &bytes)?;
        *self.live.lock().unwrap() = Some(account.clone());
        Ok(())
    }

    fn build_api_key_account(&self, api_key: &str) -> Result<LiveAccount> {
        Ok(LiveAccount {
            agent: self.id,
            kind: AccountKind::ApiKey,
            credentials: json!({"format": "api_key", "api_key": api_key}),
            label_hint: Some(format!("{} (API Key)", mask_secret_preview(api_key))),
            extra: json!({}),
        })
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        None
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        vec![self.path.clone()]
    }

    fn build_run_spec(&self, _binary: &Path, _prompt: &str, _opts: &RunOptions) -> Result<RunSpec> {
        Err(AppError::Unsupported("fake".into()))
    }
}

fn live_svc(agent: AgentId) -> (tempfile::TempDir, AccountService, Arc<FakeAdapter>) {
    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let path = root.path().join("live").join("auth.json");
    let adapter = Arc::new(FakeAdapter::new(agent, path));
    let mut registry = AdapterRegistry::new();
    registry.register(adapter.clone());
    let svc = AccountService::with_live(db, registry, root.path().join("backups"));
    (root, svc, adapter)
}

#[test]
fn add_list_delete_api_key() {
    let (_root, svc, _) = live_svc(AgentId::Grok);
    let a = svc
        .add_api_key(AgentId::Grok, Some("work"), "xai-secret-key-1234")
        .unwrap();
    assert_eq!(a.label, "work");
    assert_eq!(a.kind, AccountKind::ApiKey);
    assert!(!a.is_current);
    assert_eq!(a.credentials["api_key"], "xai-secret-key-1234");

    let list = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(list.len(), 1);
    let redacted = list[0].redacted();
    assert_eq!(redacted.credentials["api_key"], "***");

    svc.delete(&a.id, AgentId::Grok).unwrap();
    assert!(svc.list(Some(AgentId::Grok)).unwrap().is_empty());
}

#[test]
fn api_key_add_persists_explicit_product_marker() {
    let (_root, svc, _) = live_svc(AgentId::Kimi);
    let kimi_api = svc
        .add_api_key_with_env_and_marker(
            AgentId::Kimi,
            Some("open platform"),
            "sk-kimi-api",
            None,
            Some("kimi-api"),
        )
        .unwrap();
    assert_eq!(kimi_api.extra["provider"], "kimi-api");

    let kimi_code = svc
        .add_api_key_with_env_and_marker(
            AgentId::Kimi,
            Some("code membership"),
            "sk-kimi-code",
            None,
            Some("kimi-code-membership"),
        )
        .unwrap();
    assert_eq!(kimi_code.extra["provider"], "kimi-code-membership");
}

#[test]
fn update_api_key_label_and_key() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    let a = svc
        .add_api_key(AgentId::Grok, Some("old-name"), "xai-old-secret-key")
        .unwrap();

    let renamed = svc
        .update_api_key(AgentId::Grok, &a.id, Some("new-name"), None)
        .unwrap();
    assert_eq!(renamed.id, a.id);
    assert_eq!(renamed.label, "new-name");
    assert_eq!(renamed.credentials["api_key"], "xai-old-secret-key");

    let rotated = svc
        .update_api_key(AgentId::Grok, &a.id, None, Some("xai-new-secret-key"))
        .unwrap();
    assert_eq!(rotated.id, a.id);
    assert_eq!(rotated.credentials["api_key"], "xai-new-secret-key");
    // label falls back to masked key when only key is updated
    assert!(rotated.label.contains("API Key") || !rotated.label.is_empty());
    assert_eq!(
        adapter.write_attempts.load(Ordering::SeqCst),
        0,
        "non-current API key updates stay pool-only"
    );
}

#[test]
fn updating_current_api_key_writes_new_credentials_not_stale_live() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    let account = svc
        .add_api_key(AgentId::Claude, Some("work"), "sk-old-secret-key")
        .unwrap();
    svc.switch(&account.id, AgentId::Claude).unwrap();
    assert_eq!(
        adapter.read_account().unwrap().credentials["api_key"],
        "sk-old-secret-key"
    );

    let renamed = svc
        .update_api_key(AgentId::Claude, &account.id, Some("work-renamed"), None)
        .unwrap();
    assert!(renamed.is_current);
    assert_eq!(renamed.label, "work-renamed");
    assert_eq!(
        adapter.read_account().unwrap().credentials["api_key"],
        "sk-old-secret-key",
        "label-only edits must not rewrite live credentials"
    );

    let rotated = svc
        .update_api_key(
            AgentId::Claude,
            &account.id,
            None,
            Some("sk-new-secret-key"),
        )
        .unwrap();
    assert!(rotated.is_current);
    assert_eq!(rotated.credentials["api_key"], "sk-new-secret-key");
    assert_eq!(
        adapter.read_account().unwrap().credentials["api_key"],
        "sk-new-secret-key",
        "saving the current API key must apply the new pool value"
    );
}

#[test]
fn updating_current_api_key_apply_failure_restores_db_and_live() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::ApiKey,
        credentials: json!({"format": "api_key", "api_key": "old-key"}),
        label_hint: Some("old".into()),
        extra: json!({}),
    });
    let current = svc.import_live(AgentId::Claude, Some("old")).unwrap();
    let provider_before = crate::storage::ProviderRepo::new(svc.db.clone())
        .create(&crate::models::Provider {
            id: "provider-before-account-rotation".into(),
            agent_id: AgentId::Claude,
            name: "Provider before account rotation".into(),
            settings_config: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "provider-key"}}),
            meta: json!({}),
            is_current: true,
            created_at: "2026-08-21T00:00:00Z".into(),
            updated_at: "2026-08-21T00:00:00Z".into(),
        })
        .unwrap();
    adapter.fail_writes_on(&[1]);

    let error = svc
        .update_api_key(AgentId::Claude, &current.id, None, Some("new-key"))
        .unwrap_err();
    assert_eq!(error.code(), "test.write");
    assert_eq!(
        adapter.read_account().unwrap().credentials["api_key"],
        "old-key"
    );
    assert_eq!(svc.repo().get_by_id(&current.id).unwrap().unwrap(), current);
    assert_eq!(
        crate::storage::ProviderRepo::new(svc.db.clone())
            .get_by_id(&provider_before.id)
            .unwrap()
            .unwrap(),
        provider_before,
        "account live failure must restore the active provider counterpart"
    );
}

#[test]
fn duplicate_merge_apply_failure_restores_current_source_and_target() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    let source = svc
        .add_api_key(AgentId::Claude, Some("source"), "sk-source-key")
        .unwrap();
    svc.switch(&source.id, AgentId::Claude).unwrap();
    let target = svc
        .add_api_key(AgentId::Claude, Some("target"), "sk-target-key")
        .unwrap();

    let source_before = svc.repo().get_by_id(&source.id).unwrap().unwrap();
    let target_before = svc.repo().get_by_id(&target.id).unwrap().unwrap();
    let binding_before = svc.connections.get_active(AgentId::Claude).unwrap();
    let trash_before = svc.connections.list_trash(Some(AgentId::Claude)).unwrap();
    adapter.fail_writes_on(&[2]);

    let error = svc
        .update_api_key(AgentId::Claude, &source.id, None, Some("sk-target-key"))
        .unwrap_err();
    assert_eq!(error.code(), "test.write");
    assert_eq!(
        svc.repo().get_by_id(&source.id).unwrap().unwrap(),
        source_before
    );
    assert_eq!(
        svc.repo().get_by_id(&target.id).unwrap().unwrap(),
        target_before
    );
    assert_eq!(
        svc.repo().get_current(AgentId::Claude).unwrap().unwrap().id,
        source.id
    );
    assert_eq!(
        adapter.read_account().unwrap().credentials["api_key"],
        "sk-source-key"
    );
    assert_eq!(
        svc.connections.get_active(AgentId::Claude).unwrap(),
        binding_before,
        "duplicate merge compensation must restore the active binding"
    );
    assert_eq!(
        svc.connections.list_trash(Some(AgentId::Claude)).unwrap(),
        trash_before,
        "duplicate merge compensation must not leave a recovery-trash row"
    );
}

#[test]
fn duplicate_merge_delete_failure_restores_current_source_target_binding_and_live() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    let source = svc
        .add_api_key(AgentId::Claude, Some("source"), "sk-source-key")
        .unwrap();
    svc.switch(&source.id, AgentId::Claude).unwrap();
    let target = svc
        .add_api_key(AgentId::Claude, Some("target"), "sk-target-key")
        .unwrap();

    let source_before = svc.repo().get_by_id(&source.id).unwrap().unwrap();
    let target_before = svc.repo().get_by_id(&target.id).unwrap().unwrap();
    let binding_before = svc.connections.get_active(AgentId::Claude).unwrap();
    let live_before = adapter.read_account().unwrap();

    // Target activation and source deletion share one transaction. Force the
    // source delete to fail so the transaction must leave both rows,
    // including the active binding, unchanged.
    svc.db
        .with_conn(|conn| {
            conn.execute_batch(
                "CREATE TRIGGER fail_account_delete BEFORE DELETE ON accounts BEGIN SELECT RAISE(ABORT, 'delete failure'); END;",
            )?;
            Ok(())
        })
        .unwrap();

    let error = svc
        .update_api_key(AgentId::Claude, &source.id, None, Some("sk-target-key"))
        .unwrap_err();
    assert!(error.code().starts_with("db"));
    assert_eq!(
        svc.repo().get_by_id(&source.id).unwrap().unwrap(),
        source_before
    );
    assert_eq!(
        svc.repo().get_by_id(&target.id).unwrap().unwrap(),
        target_before
    );
    assert_eq!(
        svc.repo().get_current(AgentId::Claude).unwrap().unwrap().id,
        source.id
    );
    assert_eq!(
        svc.connections.get_active(AgentId::Claude).unwrap(),
        binding_before
    );
    assert_eq!(adapter.read_account().unwrap(), live_before);
}

#[test]
fn duplicate_merge_mid_cleanup_failure_restores_all_duplicate_rows() {
    use crate::storage::AccountRepo;

    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    let source = svc
        .add_api_key(AgentId::Claude, Some("source"), "sk-source-key")
        .unwrap();
    svc.switch(&source.id, AgentId::Claude).unwrap();
    let target = svc
        .add_api_key(AgentId::Claude, Some("target-a"), "sk-target-key")
        .unwrap();
    let duplicate = AccountRepo::new(svc.db.clone())
        .create(&crate::models::Account {
            id: "claude-acc-target-b".into(),
            agent_id: AgentId::Claude,
            kind: AccountKind::ApiKey,
            label: "target-b".into(),
            credentials: json!({"format": "api_key", "api_key": "sk-target-key"}),
            extra: json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "2000-01-01 00:00:00".into(),
            updated_at: "2000-01-01 00:00:00".into(),
        })
        .unwrap();
    let source_before = svc.repo().get_by_id(&source.id).unwrap().unwrap();
    let target_before = svc.repo().get_by_id(&target.id).unwrap().unwrap();
    let duplicate_before = svc.repo().get_by_id(&duplicate.id).unwrap().unwrap();
    let binding_before = svc.connections.get_active(AgentId::Claude).unwrap();
    let live_before = adapter.read_account().unwrap();

    svc.db
        .with_conn(|conn| {
            conn.execute_batch(
                r#"
                CREATE TEMP TABLE account_delete_count (count INTEGER NOT NULL);
                INSERT INTO account_delete_count VALUES (0);
                CREATE TRIGGER fail_second_account_delete
                BEFORE DELETE ON accounts
                BEGIN
                    UPDATE account_delete_count SET count = count + 1;
                    SELECT CASE WHEN (SELECT count FROM account_delete_count) > 1
                        THEN RAISE(ABORT, 'second duplicate delete failure') END;
                END;
                "#,
            )?;
            Ok(())
        })
        .unwrap();

    let error = svc
        .update_api_key(AgentId::Claude, &source.id, None, Some("sk-target-key"))
        .unwrap_err();
    assert!(error.code().starts_with("db"));
    assert_eq!(
        svc.repo().get_by_id(&source.id).unwrap().unwrap(),
        source_before
    );
    assert_eq!(
        svc.repo().get_by_id(&target.id).unwrap().unwrap(),
        target_before
    );
    assert_eq!(
        svc.repo().get_by_id(&duplicate.id).unwrap().unwrap(),
        duplicate_before
    );
    assert_eq!(
        svc.connections.get_active(AgentId::Claude).unwrap(),
        binding_before
    );
    assert_eq!(adapter.read_account().unwrap(), live_before);
    assert!(svc
        .connections
        .list_trash(Some(AgentId::Claude))
        .unwrap()
        .is_empty());
}

fn install_account_mutation_plan_trigger(svc: &AccountService, body: &str) {
    let sql = String::from(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS account_mutation_plan (
            role TEXT NOT NULL,
            id TEXT NOT NULL,
            expected_updated_at TEXT NOT NULL
        );
        "#,
    ) + body;
    svc.db
        .with_conn(|conn| {
            conn.execute_batch(&sql)?;
            Ok(())
        })
        .unwrap();
}

fn extra_api_key_row(id: &str, label: &str, key: &str) -> crate::models::Account {
    crate::models::Account {
        id: id.into(),
        agent_id: AgentId::Claude,
        kind: AccountKind::ApiKey,
        label: label.into(),
        credentials: json!({"format": "api_key", "api_key": key}),
        extra: json!({}),
        status: "active".into(),
        is_current: false,
        created_at: "2000-01-01 00:00:00".into(),
        updated_at: "2000-01-01 00:00:00".into(),
    }
}

#[test]
fn duplicate_merge_pre_mutation_failure_does_not_rollback_concurrent_writer() {
    let (_root, svc, _) = live_svc(AgentId::Claude);
    let source = svc
        .add_api_key(AgentId::Claude, Some("source"), "sk-source-key")
        .unwrap();
    svc.switch(&source.id, AgentId::Claude).unwrap();
    let source_before = svc.repo().get_by_id(&source.id).unwrap().unwrap();
    let mut external = source_before.clone();
    external.label = "concurrent-writer".into();
    external.updated_at = "concurrent-revision".into();
    svc.repo().update(&external).unwrap();

    let error = svc
        .update_api_key_inner(
            AgentId::Claude,
            &source.id,
            None,
            Some("sk-new-key"),
            &source_before.updated_at,
        )
        .unwrap_err();
    assert_eq!(error.code(), "account.merge.conflict");
    assert_eq!(
        svc.repo().get_by_id(&source.id).unwrap().unwrap(),
        external,
        "pre-commit conflict must not restore a concurrent writer's row"
    );
}

#[test]
fn duplicate_merge_ignores_duplicate_inserted_after_snapshot() {
    use crate::storage::AccountRepo;

    let (_root, svc, _) = live_svc(AgentId::Claude);
    let source = svc
        .add_api_key(AgentId::Claude, Some("source"), "sk-source-key")
        .unwrap();
    svc.switch(&source.id, AgentId::Claude).unwrap();
    let target = svc
        .add_api_key(AgentId::Claude, Some("target"), "sk-target-key")
        .unwrap();
    install_account_mutation_plan_trigger(
        &svc,
        r#"
        CREATE TEMP TRIGGER insert_after_snapshot
        AFTER INSERT ON account_mutation_plan
        WHEN NEW.role = 'target'
        BEGIN
            INSERT INTO accounts (
                id, agent_id, kind, label, credentials, extra,
                status, is_current, created_at, updated_at
            ) VALUES (
                'claude-acc-concurrent-dup',
                'claude',
                'apikey',
                'concurrent-dup',
                '{"format":"api_key","api_key":"sk-target-key"}',
                '{}',
                'active',
                0,
                '2000-01-02 00:00:00',
                '2000-01-02 00:00:00'
            );
        END;
        "#,
    );

    svc.update_api_key(AgentId::Claude, &source.id, None, Some("sk-target-key"))
        .unwrap();
    assert!(svc.repo().get_by_id(&source.id).unwrap().is_none());
    assert!(
        svc.repo()
            .get_by_id(&target.id)
            .unwrap()
            .unwrap()
            .is_current
    );
    assert!(
        AccountRepo::new(svc.db.clone())
            .get_by_id("claude-acc-concurrent-dup")
            .unwrap()
            .is_some(),
        "a duplicate inserted after the frozen snapshot must not be deleted"
    );
}

#[test]
fn duplicate_merge_source_revision_mismatch_fails_before_db_mutation() {
    let (_root, svc, _) = live_svc(AgentId::Claude);
    let source = svc
        .add_api_key(AgentId::Claude, Some("source"), "sk-source-key")
        .unwrap();
    svc.switch(&source.id, AgentId::Claude).unwrap();
    let target = svc
        .add_api_key(AgentId::Claude, Some("target"), "sk-target-key")
        .unwrap();
    let source_before = svc.repo().get_by_id(&source.id).unwrap().unwrap();
    let target_before = svc.repo().get_by_id(&target.id).unwrap().unwrap();
    install_account_mutation_plan_trigger(
        &svc,
        &format!(
            r#"
            CREATE TEMP TRIGGER bump_source_after_snapshot
            AFTER INSERT ON account_mutation_plan
            WHEN NEW.role = 'source'
            BEGIN
                UPDATE accounts SET updated_at = 'concurrent-source' WHERE id = '{id}';
            END;
            "#,
            id = source.id
        ),
    );

    let error = svc
        .update_api_key(AgentId::Claude, &source.id, None, Some("sk-target-key"))
        .unwrap_err();
    assert_eq!(error.code(), "account.merge.conflict");
    assert_eq!(
        svc.repo().get_by_id(&source.id).unwrap().unwrap(),
        source_before
    );
    assert_eq!(
        svc.repo().get_by_id(&target.id).unwrap().unwrap(),
        target_before
    );
}

#[test]
fn duplicate_merge_target_revision_mismatch_fails_before_db_mutation() {
    let (_root, svc, _) = live_svc(AgentId::Claude);
    let source = svc
        .add_api_key(AgentId::Claude, Some("source"), "sk-source-key")
        .unwrap();
    svc.switch(&source.id, AgentId::Claude).unwrap();
    let target = svc
        .add_api_key(AgentId::Claude, Some("target"), "sk-target-key")
        .unwrap();
    let source_before = svc.repo().get_by_id(&source.id).unwrap().unwrap();
    let target_before = svc.repo().get_by_id(&target.id).unwrap().unwrap();
    install_account_mutation_plan_trigger(
        &svc,
        &format!(
            r#"
            CREATE TEMP TRIGGER bump_target_after_snapshot
            AFTER INSERT ON account_mutation_plan
            WHEN NEW.role = 'target'
            BEGIN
                UPDATE accounts SET updated_at = 'concurrent-target' WHERE id = '{id}';
            END;
            "#,
            id = target.id
        ),
    );

    let error = svc
        .update_api_key(AgentId::Claude, &source.id, None, Some("sk-target-key"))
        .unwrap_err();
    assert_eq!(error.code(), "account.merge.conflict");
    assert_eq!(
        svc.repo().get_by_id(&source.id).unwrap().unwrap(),
        source_before
    );
    assert_eq!(
        svc.repo().get_by_id(&target.id).unwrap().unwrap(),
        target_before
    );
}

#[test]
fn duplicate_merge_source_cas_delete_conflict_is_not_pre_mutation_conflict() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    let source = svc
        .add_api_key(AgentId::Claude, Some("source"), "sk-source-key")
        .unwrap();
    svc.switch(&source.id, AgentId::Claude).unwrap();
    let target = svc
        .add_api_key(AgentId::Claude, Some("target"), "sk-target-key")
        .unwrap();
    let source_before = svc.repo().get_by_id(&source.id).unwrap().unwrap();
    let target_before = svc.repo().get_by_id(&target.id).unwrap().unwrap();
    let binding_before = svc.connections.get_active(AgentId::Claude).unwrap();
    let live_before = adapter.read_account().unwrap();
    svc.db
        .with_conn(|conn| {
            conn.execute_batch(&format!(
                r#"
                CREATE TEMP TRIGGER steal_source_after_target
                AFTER UPDATE ON accounts
                WHEN NEW.id = '{target}' AND NEW.is_current = 1
                BEGIN
                    UPDATE accounts SET updated_at = 'stolen-source' WHERE id = '{source}';
                END;
                "#,
                target = target.id,
                source = source.id
            ))?;
            Ok(())
        })
        .unwrap();

    let error = svc
        .update_api_key(AgentId::Claude, &source.id, None, Some("sk-target-key"))
        .unwrap_err();
    assert_eq!(error.code(), "account.merge.delete.conflict");
    assert_ne!(error.code(), "account.merge.conflict");
    assert_eq!(
        svc.repo().get_by_id(&source.id).unwrap().unwrap(),
        source_before
    );
    assert_eq!(
        svc.repo().get_by_id(&target.id).unwrap().unwrap(),
        target_before
    );
    assert_eq!(
        svc.connections.get_active(AgentId::Claude).unwrap(),
        binding_before
    );
    assert_eq!(adapter.read_account().unwrap(), live_before);
}

#[test]
fn duplicate_merge_three_duplicates_mid_cleanup_restores_all_rows() {
    use crate::storage::AccountRepo;

    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    let source = svc
        .add_api_key(AgentId::Claude, Some("source"), "sk-source-key")
        .unwrap();
    svc.switch(&source.id, AgentId::Claude).unwrap();
    let target = svc
        .add_api_key(AgentId::Claude, Some("target-a"), "sk-target-key")
        .unwrap();
    let repo = AccountRepo::new(svc.db.clone());
    let extra_b = repo
        .create(&extra_api_key_row(
            "claude-acc-target-b",
            "target-b",
            "sk-target-key",
        ))
        .unwrap();
    let extra_c = repo
        .create(&extra_api_key_row(
            "claude-acc-target-c",
            "target-c",
            "sk-target-key",
        ))
        .unwrap();
    let source_before = svc.repo().get_by_id(&source.id).unwrap().unwrap();
    let target_before = svc.repo().get_by_id(&target.id).unwrap().unwrap();
    let extra_b_before = svc.repo().get_by_id(&extra_b.id).unwrap().unwrap();
    let extra_c_before = svc.repo().get_by_id(&extra_c.id).unwrap().unwrap();
    let binding_before = svc.connections.get_active(AgentId::Claude).unwrap();
    let live_before = adapter.read_account().unwrap();

    svc.db
        .with_conn(|conn| {
            conn.execute_batch(
                r#"
                CREATE TEMP TABLE account_delete_count (count INTEGER NOT NULL);
                INSERT INTO account_delete_count VALUES (0);
                CREATE TRIGGER fail_second_account_delete
                BEFORE DELETE ON accounts
                BEGIN
                    UPDATE account_delete_count SET count = count + 1;
                    SELECT CASE WHEN (SELECT count FROM account_delete_count) > 1
                        THEN RAISE(ABORT, 'second duplicate delete failure') END;
                END;
                "#,
            )?;
            Ok(())
        })
        .unwrap();

    let error = svc
        .update_api_key(AgentId::Claude, &source.id, None, Some("sk-target-key"))
        .unwrap_err();
    assert!(error.code().starts_with("db"));
    assert_eq!(
        svc.repo().get_by_id(&source.id).unwrap().unwrap(),
        source_before
    );
    assert_eq!(
        svc.repo().get_by_id(&target.id).unwrap().unwrap(),
        target_before
    );
    assert_eq!(
        svc.repo().get_by_id(&extra_b.id).unwrap().unwrap(),
        extra_b_before
    );
    assert_eq!(
        svc.repo().get_by_id(&extra_c.id).unwrap().unwrap(),
        extra_c_before
    );
    assert_eq!(
        svc.connections.get_active(AgentId::Claude).unwrap(),
        binding_before
    );
    assert_eq!(adapter.read_account().unwrap(), live_before);
    assert!(svc
        .connections
        .list_trash(Some(AgentId::Claude))
        .unwrap()
        .is_empty());
}

#[test]
fn duplicate_merge_live_apply_failure_restores_cross_pool_current_and_binding() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    let source = svc
        .add_api_key(AgentId::Claude, Some("source"), "sk-source-key")
        .unwrap();
    svc.switch(&source.id, AgentId::Claude).unwrap();
    let target = svc
        .add_api_key(AgentId::Claude, Some("target"), "sk-target-key")
        .unwrap();
    let provider_before = crate::storage::ProviderRepo::new(svc.db.clone())
        .create(&crate::models::Provider {
            id: "provider-before-duplicate-merge".into(),
            agent_id: AgentId::Claude,
            name: "Provider before merge".into(),
            settings_config: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "provider-key"}}),
            meta: json!({}),
            is_current: true,
            created_at: "2026-08-21T00:00:00Z".into(),
            updated_at: "2026-08-21T00:00:00Z".into(),
        })
        .unwrap();
    let source_before = svc.repo().get_by_id(&source.id).unwrap().unwrap();
    let target_before = svc.repo().get_by_id(&target.id).unwrap().unwrap();
    let binding_before = svc.connections.get_active(AgentId::Claude).unwrap();
    adapter.fail_writes_on(&[2]);

    let error = svc
        .update_api_key(AgentId::Claude, &source.id, None, Some("sk-target-key"))
        .unwrap_err();
    assert_eq!(error.code(), "test.write");
    let restored_source = svc.repo().get_by_id(&source.id).unwrap().unwrap();
    assert_eq!(restored_source.credentials, source_before.credentials);
    assert!(restored_source.is_current);
    let restored_target = svc.repo().get_by_id(&target.id).unwrap().unwrap();
    assert_eq!(restored_target.credentials, target_before.credentials);
    assert!(!restored_target.is_current);
    assert!(crate::storage::ProviderRepo::new(svc.db.clone())
        .get_by_id(&provider_before.id)
        .unwrap()
        .is_some());
    assert_eq!(
        svc.connections.get_active(AgentId::Claude).unwrap(),
        binding_before
    );
    assert_eq!(
        adapter.read_account().unwrap().credentials["api_key"],
        "sk-source-key"
    );
}

#[test]
fn account_compensation_fails_closed_when_another_writer_changes_the_row() {
    let (_root, svc, _) = live_svc(AgentId::Claude);
    let original = svc
        .add_api_key(AgentId::Claude, Some("original"), "sk-original")
        .unwrap();
    let mut external = original.clone();
    external.label = "external-writer".into();
    external.updated_at = "external-revision".into();
    svc.repo().update(&external).unwrap();

    let mut expected_after = original.clone();
    expected_after.label = "mutation-result".into();
    expected_after.updated_at = "mutation-revision".into();
    let error = svc
        .restore_account_rows(
            AgentId::Claude,
            std::slice::from_ref(&original),
            std::slice::from_ref(&expected_after),
            &expected_after,
            &[],
        )
        .unwrap_err();

    assert_eq!(error.code(), "account.current.apply.rollback.database");
    assert_eq!(
        svc.repo().get_by_id(&original.id).unwrap().unwrap(),
        external
    );
}

#[test]
fn updating_current_codex_api_key_account_stays_pool_only() {
    let (_root, svc, adapter) = live_svc(AgentId::Codex);
    adapter.reject_api_key_apply();
    adapter.set_live(LiveAccount {
        agent: AgentId::Codex,
        kind: AccountKind::Oauth,
        credentials: json!({"format": "auth_json", "body": {"token": "oauth-live"}}),
        label_hint: Some("oauth".into()),
        extra: json!({}),
    });
    let imported = svc.import_live(AgentId::Codex, None).unwrap();
    assert!(imported.is_current);

    let account = svc
        .add_api_key(AgentId::Codex, Some("codex-key"), "sk-codex-old")
        .unwrap();
    // Mark the API-key row current in the pool without a live apply; Codex
    // refuses apply_account for format=api_key.
    let mut current = svc.get(&account.id, Some(AgentId::Codex)).unwrap();
    current.is_current = true;
    svc.repo().update(&current).unwrap();
    let writes_before = adapter.write_attempts.load(Ordering::SeqCst);

    let rotated = svc
        .update_api_key(AgentId::Codex, &account.id, None, Some("sk-codex-new"))
        .unwrap();
    assert!(rotated.is_current);
    assert_eq!(rotated.credentials["api_key"], "sk-codex-new");
    assert_eq!(
        adapter.read_account().unwrap().credentials["body"]["token"],
        "oauth-live",
        "unsupported API-key live apply must not rewrite Codex auth.json"
    );
    assert_eq!(adapter.write_attempts.load(Ordering::SeqCst), writes_before);
}

#[test]
fn import_live_heals_codex_email_from_id_token() {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
    let payload = URL_SAFE_NO_PAD.encode(
        json!({
            "email": "imported@example.com",
            "https://api.openai.com/auth": { "chatgpt_account_id": "acc-1" }
        })
        .to_string()
        .as_bytes(),
    );
    let id_token = format!("{header}.{payload}.sig");
    let (_root, svc, adapter) = live_svc(AgentId::Codex);
    adapter.set_live(LiveAccount {
        agent: AgentId::Codex,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "tokens": {
                    "id_token": id_token,
                    "access_token": "at-imported",
                    "refresh_token": "rt-imported-secret"
                }
            }
        }),
        label_hint: None,
        extra: json!({}),
    });
    let imported = svc.import_live(AgentId::Codex, None).unwrap();
    assert_eq!(imported.label, "imported@example.com");
    assert_eq!(
        imported.extra.get("email").and_then(|v| v.as_str()),
        Some("imported@example.com")
    );
}

#[test]
fn import_and_switch_with_single_current() {
    let (_root, svc, adapter) = live_svc(AgentId::Codex);
    adapter.set_live(LiveAccount {
        agent: AgentId::Codex,
        kind: AccountKind::Oauth,
        credentials: json!({"format": "auth_json", "body": {"token": "live-a"}}),
        label_hint: Some("user-a@example.com".into()),
        extra: json!({}),
    });
    let imported = svc.import_live(AgentId::Codex, None).unwrap();
    assert!(imported.is_current);

    let b = svc
        .add_api_key(AgentId::Codex, Some("pool-b"), "sk-pool-bbbbbbbb")
        .unwrap();
    // API key apply is supported by FakeAdapter.
    let switched = svc.switch(&b.id, AgentId::Codex).unwrap();
    assert!(switched.account.is_current);
    assert_eq!(switched.account.id, b.id);
    assert_eq!(
        switched.backfilled_account_id.as_deref(),
        Some(imported.id.as_str())
    );

    let currents: Vec<_> = svc
        .list(Some(AgentId::Codex))
        .unwrap()
        .into_iter()
        .filter(|a| a.is_current)
        .collect();
    assert_eq!(currents.len(), 1);
    assert_eq!(currents[0].id, b.id);
}

#[test]
fn live_write_failure_does_not_update_current() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::ApiKey,
        credentials: json!({"format": "api_key", "api_key": "old-key"}),
        label_hint: Some("old".into()),
        extra: json!({}),
    });
    let current = svc.import_live(AgentId::Grok, Some("old")).unwrap();
    let other = svc
        .add_api_key(AgentId::Grok, Some("new"), "new-key-value-zzzz")
        .unwrap();
    adapter.fail_on_write.store(1, Ordering::SeqCst);

    let err = svc.switch(&other.id, AgentId::Grok).unwrap_err();
    assert_eq!(err.code(), "test.write");

    let still = svc.repo().get_current(AgentId::Grok).unwrap().unwrap();
    assert_eq!(still.id, current.id);
    assert!(still.is_current);
    let other_row = svc.repo().get_by_id(&other.id).unwrap().unwrap();
    assert!(!other_row.is_current);
}

#[test]
fn switch_reports_failed_live_rollback_in_compensated_error() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::ApiKey,
        credentials: json!({"format": "api_key", "api_key": "old-key"}),
        label_hint: Some("old".into()),
        extra: json!({}),
    });
    let current = svc.import_live(AgentId::Grok, Some("old")).unwrap();
    let target = svc
        .add_api_key(AgentId::Grok, Some("new"), "new-key-value-zzzz")
        .unwrap();

    // The target write fails, then rollback of the original live credentials
    // fails too. The service must report the compound outcome, not hide it.
    adapter.fail_writes_on(&[1, 2]);
    let err = svc.switch(&target.id, AgentId::Grok).unwrap_err();
    assert_eq!(err.code(), "account.switch.rollback");
    assert!(err.to_string().contains("live=test.write"));
    assert_eq!(
        svc.repo().get_current(AgentId::Grok).unwrap().unwrap().id,
        current.id
    );
}

#[test]
fn switch_retries_unstable_revision_then_rolls_back_on_post_backup_race() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::ApiKey,
        credentials: json!({"format": "api_key", "api_key": "old-key"}),
        label_hint: Some("old".into()),
        extra: json!({}),
    });
    let current = svc.import_live(AgentId::Grok, Some("old")).unwrap();
    let target = svc
        .add_api_key(AgentId::Grok, Some("new"), "new-key-value-zzzz")
        .unwrap();
    adapter.set_auth_state(AuthState {
        agent: AgentId::Grok,
        kind: Some("api_key".into()),
        summary: "configured".into(),
        has_credentials: true,
        health: crate::models::AuthHealth::Configured,
        source: Some("fake-live-auth".into()),
        revision: Some("unused".into()),
        also_present: Vec::new(),
        secret_hash: None,
    });
    // First stable-read attempt races (r1 -> r2); the second is stable at r2.
    // The revision then changes while the backup runs, before apply_account.
    adapter.set_revision_sequence(&["r1", "r2", "r2", "r2", "r3"]);

    let err = svc.switch(&target.id, AgentId::Grok).unwrap_err();
    assert_eq!(err.code(), "account.live_conflict");
    assert_eq!(adapter.write_attempts.load(Ordering::SeqCst), 0);
    let restored = svc.repo().get_current(AgentId::Grok).unwrap().unwrap();
    assert_eq!(restored.id, current.id);
    assert_eq!(restored.credentials["api_key"], "old-key");
}

#[test]
fn switch_reports_failed_db_rollback_after_revision_conflict() {
    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let path = root.path().join("live").join("auth.json");
    let adapter = Arc::new(FakeAdapter::new(AgentId::Grok, path));
    let mut registry = AdapterRegistry::new();
    registry.register(adapter.clone());
    let svc = AccountService::with_live(db.clone(), registry, root.path().join("backups"));

    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::ApiKey,
        credentials: json!({"format": "api_key", "api_key": "old-key"}),
        label_hint: Some("old".into()),
        extra: json!({}),
    });
    let current = svc.import_live(AgentId::Grok, Some("old")).unwrap();
    let target = svc
        .add_api_key(AgentId::Grok, Some("new"), "new-key-value-zzzz")
        .unwrap();
    adapter.set_auth_state(AuthState {
        agent: AgentId::Grok,
        kind: Some("api_key".into()),
        summary: "configured".into(),
        has_credentials: true,
        health: crate::models::AuthHealth::Configured,
        source: Some("fake-live-auth".into()),
        revision: Some("unused".into()),
        also_present: Vec::new(),
        secret_hash: None,
    });
    adapter.set_revision_sequence(&["r1", "r2", "r2", "r2", "r3"]);

    // Install the trigger exactly before the final revision read. The backfill
    // has already succeeded, while restoring its old timestamp must now fail.
    let original_updated_at = current.updated_at.clone();
    let hook_db = db.clone();
    adapter.on_auth_read(5, move || {
        hook_db
            .with_conn(|conn| {
                conn.execute_batch(&format!(
                    r#"
                    CREATE TRIGGER fail_account_backfill_rollback
                    BEFORE UPDATE OF credentials ON accounts
                    WHEN NEW.updated_at = '{original_updated_at}'
                    BEGIN
                        SELECT RAISE(ABORT, 'injected rollback failure');
                    END;
                    "#
                ))?;
                Ok(())
            })
            .unwrap();
    });

    let err = svc.switch(&target.id, AgentId::Grok).unwrap_err();
    assert_eq!(err.code(), "account.switch.rollback");
    assert!(err.to_string().contains("database=db"));
    assert_eq!(adapter.write_attempts.load(Ordering::SeqCst), 0);
}

#[test]
fn unsupported_agent_returns_clear_error() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    adapter.supports.store(false, Ordering::SeqCst);
    let err = svc.import_live(AgentId::Claude, None).unwrap_err();
    assert_eq!(err.code(), "unsupported");
}

#[test]
fn import_live_dedupes_identical_credentials() {
    let (_root, svc, adapter) = live_svc(AgentId::Codex);
    adapter.set_live(LiveAccount {
        agent: AgentId::Codex,
        kind: AccountKind::Oauth,
        credentials: json!({"format": "auth_json", "body": {"token": "same-live"}}),
        label_hint: Some("user-a@example.com".into()),
        extra: json!({}),
    });
    let first = svc.import_live(AgentId::Codex, None).unwrap();
    let second = svc.import_live(AgentId::Codex, None).unwrap();
    assert_eq!(
        first.id, second.id,
        "re-import same ticket must not create a new row"
    );
    assert!(second.is_current);
    assert_eq!(svc.list(Some(AgentId::Codex)).unwrap().len(), 1);
    assert_eq!(
        second.extra["identityLabel"], "user-a@example.com",
        "identity label stored for UI grouping"
    );
}

#[test]
fn import_live_loopback_upserts_same_slot_when_port_and_token_rotate() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::ApiKey,
        credentials: json!({
            "format": "api_key",
            "api_key": "token-aaa",
            "env_key": "ANTHROPIC_AUTH_TOKEN",
            "base_url": "http://127.0.0.1:43081"
        }),
        label_hint: Some("Claude bridge".into()),
        extra: json!({}),
    });
    let first = svc
        .import_live(AgentId::Claude, Some("Claude bridge"))
        .unwrap();

    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::ApiKey,
        credentials: json!({
            "format": "api_key",
            "api_key": "token-bbb",
            "env_key": "ANTHROPIC_AUTH_TOKEN",
            "base_url": "http://127.0.0.1:44227"
        }),
        label_hint: Some("Claude bridge".into()),
        extra: json!({}),
    });
    let second = svc.import_live(AgentId::Claude, None).unwrap();
    assert_eq!(
        first.id, second.id,
        "rotating loopback port+token must keep one slot"
    );
    assert_eq!(second.credentials["api_key"], "token-bbb");
    assert_eq!(second.credentials["base_url"], "http://127.0.0.1:44227");
    assert_eq!(svc.list(Some(AgentId::Claude)).unwrap().len(), 1);
}

#[test]
fn import_live_remote_api_keys_keep_separate_rows() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::ApiKey,
        credentials: json!({
            "format": "api_key",
            "api_key": "sk-remote-aaa",
            "base_url": "https://api.anthropic.com"
        }),
        label_hint: Some("Anthropic A".into()),
        extra: json!({}),
    });
    let first = svc
        .import_live(AgentId::Claude, Some("Anthropic A"))
        .unwrap();

    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::ApiKey,
        credentials: json!({
            "format": "api_key",
            "api_key": "sk-remote-bbb"
        }),
        label_hint: Some("Anthropic B".into()),
        extra: json!({}),
    });
    let second = svc
        .import_live(AgentId::Claude, Some("Anthropic B"))
        .unwrap();
    assert_ne!(
        first.id, second.id,
        "distinct remote/no-url API keys stay on token fingerprints"
    );
    assert_eq!(svc.list(Some(AgentId::Claude)).unwrap().len(), 2);
}

#[test]
fn import_live_loopback_does_not_swallow_oauth() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "credentials_json",
            "body": {
                "claudeAiOauth": {
                    "accessToken": "oauth-access",
                    "refreshToken": "oauth-refresh"
                }
            }
        }),
        label_hint: Some("Claude OAuth".into()),
        extra: json!({}),
    });
    let oauth = svc
        .import_live(AgentId::Claude, Some("Claude OAuth"))
        .unwrap();

    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::ApiKey,
        credentials: json!({
            "format": "api_key",
            "api_key": "token-bridge",
            "env_key": "ANTHROPIC_AUTH_TOKEN",
            "base_url": "http://127.0.0.1:43081"
        }),
        label_hint: Some("Claude bridge".into()),
        extra: json!({}),
    });
    let loopback = svc
        .import_live(AgentId::Claude, Some("Claude bridge"))
        .unwrap();
    assert_ne!(oauth.id, loopback.id);
    assert_eq!(oauth.kind, AccountKind::Oauth);
    assert_eq!(loopback.kind, AccountKind::ApiKey);
    let list = svc.list(Some(AgentId::Claude)).unwrap();
    assert_eq!(list.len(), 2);
    assert!(list
        .iter()
        .any(|row| row.id == oauth.id && row.kind == AccountKind::Oauth));
    assert_eq!(
        list.iter()
            .filter(
                |row| row.credentials.get("base_url").and_then(|v| v.as_str())
                    == Some("http://127.0.0.1:43081")
            )
            .count(),
        1
    );
}

#[test]
fn import_live_refuses_current_generated_local_route() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    ProviderRepo::new(svc.db.clone())
        .create(&Provider {
            id: "claude-gen".into(),
            agent_id: AgentId::Claude,
            name: "generated".into(),
            settings_config: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "http://127.0.0.1:43081",
                    "ANTHROPIC_AUTH_TOKEN": "ahb_local"
                }
            }),
            meta: json!({
                "generatedBy": "adapter",
                "adapterBridge": { "loopbackOnly": true }
            }),
            is_current: true,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();
    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::ApiKey,
        credentials: json!({
            "format": "api_key",
            "api_key": "ahb_local",
            "env_key": "ANTHROPIC_AUTH_TOKEN",
            "base_url": "http://127.0.0.1:43081"
        }),
        label_hint: Some("Claude bridge".into()),
        extra: json!({}),
    });
    let error = svc.import_live(AgentId::Claude, None).unwrap_err();
    assert_eq!(error.code(), "account.import_projection");
    assert!(svc.list(Some(AgentId::Claude)).unwrap().is_empty());
}

#[test]
fn import_live_refuses_leftover_ahb_bearer_without_current_row() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::ApiKey,
        credentials: json!({
            "format": "api_key",
            "api_key": "ahb_stale_token",
            "base_url": "http://127.0.0.1:43081"
        }),
        label_hint: Some("Claude bridge".into()),
        extra: json!({}),
    });
    let error = svc.import_live(AgentId::Claude, None).unwrap_err();
    assert_eq!(error.code(), "account.import_projection");
    assert!(svc.list(Some(AgentId::Claude)).unwrap().is_empty());
    assert!(svc.live_is_adapter_projection(AgentId::Claude).unwrap());
}

#[test]
fn import_live_overwrites_same_oauth_identity_different_tokens() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "provider": {
                    "email": "a@example.com",
                    "user_id": "uid-1",
                    "key": "token-aaa"
                }
            }
        }),
        label_hint: Some("a@example.com".into()),
        extra: json!({}),
    });
    let first = svc.import_live(AgentId::Grok, None).unwrap();

    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "provider": {
                    "email": "a@example.com",
                    "user_id": "uid-1",
                    "key": "token-bbb"
                }
            }
        }),
        label_hint: Some("a@example.com".into()),
        extra: json!({}),
    });
    let second = svc.import_live(AgentId::Grok, None).unwrap();
    assert_eq!(
        first.id, second.id,
        "same Grok OAuth identity must overwrite the existing row"
    );
    assert_eq!(second.credentials["body"]["provider"]["key"], "token-bbb");
    assert!(second.is_current);
    let list = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, first.id);
    assert_eq!(list[0].credentials["body"]["provider"]["key"], "token-bbb");
    assert!(list.iter().all(|a| a.extra.get("identityLabel").is_some()));
}

#[test]
fn import_live_keeps_cross_agent_same_oauth_identity_separate() {
    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let grok = Arc::new(FakeAdapter::new(
        AgentId::Grok,
        root.path().join("live").join("grok.json"),
    ));
    let claude = Arc::new(FakeAdapter::new(
        AgentId::Claude,
        root.path().join("live").join("claude.json"),
    ));
    let mut registry = AdapterRegistry::new();
    registry.register(grok.clone());
    registry.register(claude.clone());
    let svc = AccountService::with_live(db, registry, root.path().join("backups"));

    grok.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {"email": "a@example.com", "user_id": "uid-1", "key": "grok-token"}
        }),
        label_hint: Some("a@example.com".into()),
        extra: json!({}),
    });
    claude.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "credentials_json",
            "body": {
                "claudeAiOauth": {
                    "accessToken": "claude-access",
                    "refreshToken": "claude-refresh"
                },
                "email": "a@example.com"
            }
        }),
        label_hint: Some("a@example.com".into()),
        extra: json!({}),
    });

    let grok_row = svc.import_live(AgentId::Grok, None).unwrap();
    let claude_row = svc.import_live(AgentId::Claude, None).unwrap();
    assert_ne!(grok_row.id, claude_row.id);
    assert_eq!(svc.list(Some(AgentId::Grok)).unwrap().len(), 1);
    assert_eq!(svc.list(Some(AgentId::Claude)).unwrap().len(), 1);
    let all = svc.list(None).unwrap();
    assert_eq!(all.len(), 2, "same email on Grok and Claude stays two rows");
    assert!(all.iter().any(|row| row.agent_id == AgentId::Grok));
    assert!(all.iter().any(|row| row.agent_id == AgentId::Claude));
}

#[test]
fn hub_pkce_and_cli_import_same_grok_identity_overwrite() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    let first = svc
        .create(AccountInput {
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "a@example.com".into(),
            credentials: json!({
                "format": "auth_json",
                "body": {
                    "email": "a@example.com",
                    "user_id": "uid-1",
                    "key": "pkce-token"
                }
            }),
            extra: json!({ "source": "oauth_pkce" }),
            is_current: false,
        })
        .unwrap();

    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "email": "a@example.com",
                "user_id": "uid-1",
                "key": "cli-token"
            }
        }),
        label_hint: Some("a@example.com".into()),
        extra: json!({}),
    });
    let imported = svc.import_live(AgentId::Grok, None).unwrap();
    assert_eq!(first.id, imported.id);
    assert_eq!(imported.credentials["body"]["key"], "cli-token");
    assert_eq!(svc.list(Some(AgentId::Grok)).unwrap().len(), 1);
}

#[test]
fn hub_pkce_bundle_then_cli_auth_json_same_user_overwrites() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    let first = svc
        .create(AccountInput {
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "Grok · OAuth".into(),
            credentials: json!({
                "type": "oauth",
                "provider": "xai",
                "access_token": "pkce-access",
                "refresh_token": "pkce-refresh",
                "sub": "uid-1"
            }),
            extra: json!({ "source": "oauth_pkce" }),
            is_current: false,
        })
        .unwrap();

    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "https://auth.x.ai::https://api.x.ai": {
                    "email": "a@example.com",
                    "user_id": "uid-1",
                    "key": "cli-token"
                }
            }
        }),
        label_hint: Some("grok-oauth".into()),
        extra: json!({}),
    });
    let imported = svc.import_live(AgentId::Grok, None).unwrap();
    assert_eq!(
        first.id, imported.id,
        "PKCE sub must match CLI user_id as the same Grok person"
    );
    assert_eq!(svc.list(Some(AgentId::Grok)).unwrap().len(), 1);
    let stored = &svc.list(Some(AgentId::Grok)).unwrap()[0];
    assert_eq!(
        stored.credentials["body"]["https://auth.x.ai::https://api.x.ai"]["key"],
        "cli-token"
    );
}

#[test]
fn oauth_identity_does_not_merge_on_display_label_or_cross_bucket() {
    let pkce = json!({
        "type": "oauth",
        "provider": "xai",
        "refresh_token": "rt-pkce",
        "sub": "uid-1"
    });
    let cli = Account {
        id: "cli".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::Oauth,
        label: "Grok · OAuth".into(),
        credentials: json!({
            "format": "auth_json",
            "body": {
                "https://auth.x.ai": {
                    "email": "a@example.com",
                    "user_id": "uid-1"
                }
            }
        }),
        extra: json!({}),
        status: "active".into(),
        is_current: false,
        created_at: "t".into(),
        updated_at: "t".into(),
    };
    assert!(accounts_same_oauth_identity(
        AccountKind::Oauth,
        &pkce,
        &cli
    ));

    let other_email = Account {
        id: "other".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::Oauth,
        label: "Grok · OAuth".into(),
        credentials: json!({
            "format": "auth_json",
            "body": { "email": "b@example.com" }
        }),
        extra: json!({}),
        status: "active".into(),
        is_current: false,
        created_at: "t".into(),
        updated_at: "t".into(),
    };
    assert!(!accounts_same_oauth_identity(
        AccountKind::Oauth,
        &pkce,
        &other_email
    ));

    let unlabeled = json!({
        "type": "oauth",
        "refresh_token": "rt-a"
    });
    let unlabeled_other = Account {
        id: "none".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::Oauth,
        label: "Grok · OAuth".into(),
        credentials: json!({
            "type": "oauth",
            "refresh_token": "rt-b"
        }),
        extra: json!({}),
        status: "active".into(),
        is_current: false,
        created_at: "t".into(),
        updated_at: "t".into(),
    };
    assert!(!accounts_same_oauth_identity(
        AccountKind::Oauth,
        &unlabeled,
        &unlabeled_other
    ));
}

#[test]
fn list_syncs_current_grok_token_rotation_without_creating_account() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "email": "a@example.com",
                "user_id": "uid-1",
                "key": "access-a",
                "refresh_token": "refresh-a"
            }
        }),
        label_hint: Some("a@example.com".into()),
        extra: json!({"source": "auth.json"}),
    });
    let first = svc.import_live(AgentId::Grok, None).unwrap();

    // The CLI refreshes the same grant in place. A list/read must reconcile
    // the current row instead of importing a second authorization.
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "email": "a@example.com",
                "user_id": "uid-1",
                "key": "access-b",
                "refresh_token": "refresh-b"
            }
        }),
        label_hint: Some("a@example.com".into()),
        extra: json!({"source": "auth.json"}),
    });

    let rows = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, first.id);
    assert_eq!(rows[0].credentials["body"]["key"], "access-b");
    assert_eq!(rows[0].credentials["body"]["refresh_token"], "refresh-b");
    assert_eq!(rows[0].extra["source"], "live");
    assert_eq!(rows[0].status, "active");
}

#[test]
fn list_aligns_current_grok_account_for_different_live_identity() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {"email": "a@example.com", "key": "access-a"}
        }),
        label_hint: Some("a@example.com".into()),
        extra: json!({}),
    });
    let first = svc.import_live(AgentId::Grok, None).unwrap();

    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {"email": "b@example.com", "key": "access-b"}
        }),
        label_hint: Some("b@example.com".into()),
        extra: json!({}),
    });

    let rows = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(rows.len(), 2);
    let old = rows.iter().find(|row| row.id == first.id).unwrap();
    assert_eq!(old.credentials["body"]["email"], "a@example.com");
    assert_eq!(old.credentials["body"]["key"], "access-a");
    assert!(
        !old.is_current,
        "the stale DB current must yield to the verified external live login"
    );
    let fresh = rows
        .iter()
        .find(|row| row.credentials["body"]["email"] == "b@example.com")
        .unwrap();
    assert_eq!(fresh.credentials["body"]["key"], "access-b");
    assert!(fresh.is_current, "DB current must match the live identity");
}

#[test]
fn list_collapses_multiple_same_identity_oauth_grants_to_one_row() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    let make_creds = |key: &str| {
        json!({
            "format": "auth_json",
            "body": {"email": "same@example.com", "user_id": "same-user", "key": key}
        })
    };
    for (index, key) in ["grant-a", "grant-b", "grant-c"].iter().enumerate() {
        svc.repo()
            .create(&Account {
                id: format!("grok-grant-{index}"),
                agent_id: AgentId::Grok,
                kind: AccountKind::Oauth,
                label: "same@example.com".into(),
                credentials: make_creds(key),
                extra: json!({"source": "live", "identityLabel": "same@example.com"}),
                status: "active".into(),
                is_current: index == 1,
                created_at: format!("2026-01-0{} 00:00:00.000000", index + 1),
                updated_at: format!("2026-01-0{} 00:00:00.000000", index + 1),
            })
            .unwrap();
    }

    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: make_creds("grant-c"),
        label_hint: Some("same@example.com".into()),
        extra: json!({}),
    });
    let rows = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].credentials["body"]["key"], "grant-c");
    assert!(rows[0].is_current);
    assert_eq!(
        rows[0].id, "grok-grant-1",
        "collapse keeps the current row and overwrites it with live tokens"
    );
    assert!(!rows.iter().any(|row| row.id == "grok-grant-0"));
    assert!(!rows.iter().any(|row| row.id == "grok-grant-2"));

    let repeated = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(repeated.len(), 1);
    assert_eq!(repeated[0].credentials["body"]["key"], "grant-c");
}

fn grok_two_slot_auth_json(uid1_rt: &str, uid2_rt: &str) -> serde_json::Value {
    json!({
        "format": "auth_json",
        "body": {
            "https://auth.x.ai::client": {
                "email": "a@example.com",
                "user_id": "uid-1",
                "key": "at-1",
                "refresh_token": uid1_rt
            },
            "https://auth.x.ai::https://api.x.ai": {
                "email": "b@example.com",
                "user_id": "uid-2",
                "key": "at-2",
                "refresh_token": uid2_rt
            }
        }
    })
}

fn grok_two_slot_uid1_and_uid3() -> serde_json::Value {
    json!({
        "format": "auth_json",
        "body": {
            "https://auth.x.ai::client": {
                "email": "a@example.com",
                "user_id": "uid-1",
                "key": "at-1",
                "refresh_token": "rt-1"
            },
            "https://auth.x.ai::https://api.x.ai": {
                "email": "c@example.com",
                "user_id": "uid-3",
                "key": "at-3",
                "refresh_token": "rt-3"
            }
        }
    })
}

fn seed_two_grok_people(svc: &AccountService, current_uid: &str) {
    for (index, (uid, email, rt)) in [
        ("uid-1", "a@example.com", "rt-1"),
        ("uid-2", "b@example.com", "rt-2"),
    ]
    .into_iter()
    .enumerate()
    {
        svc.repo()
            .create(&Account {
                id: format!("grok-{uid}"),
                agent_id: AgentId::Grok,
                kind: AccountKind::Oauth,
                label: email.into(),
                credentials: json!({
                    "format": "auth_json",
                    "body": {
                        "https://auth.x.ai::client": {
                            "email": email,
                            "user_id": uid,
                            "key": format!("at-{index}"),
                            "refresh_token": rt
                        }
                    }
                }),
                extra: json!({"source": "live", "identityLabel": email}),
                status: "active".into(),
                is_current: uid == current_uid,
                created_at: "2026-01-01 00:00:00.000000".into(),
                updated_at: "2026-01-01 00:00:00.000000".into(),
            })
            .unwrap();
    }
}

#[test]
fn list_keeps_two_grok_oauth_people_from_two_slot_auth_json() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    for (index, (uid, email, rt)) in [
        ("uid-1", "a@example.com", "rt-1"),
        ("uid-2", "b@example.com", "rt-2"),
    ]
    .into_iter()
    .enumerate()
    {
        svc.repo()
            .create(&Account {
                id: format!("grok-{uid}"),
                agent_id: AgentId::Grok,
                kind: AccountKind::Oauth,
                label: email.into(),
                credentials: json!({
                    "format": "auth_json",
                    "body": {
                        "https://auth.x.ai::client": {
                            "email": email,
                            "user_id": uid,
                            "key": format!("at-{index}"),
                            "refresh_token": rt
                        }
                    }
                }),
                extra: json!({"source": "live", "identityLabel": email}),
                status: "active".into(),
                is_current: index == 0,
                created_at: "2026-01-01 00:00:00.000000".into(),
                updated_at: "2026-01-01 00:00:00.000000".into(),
            })
            .unwrap();
    }

    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: grok_two_slot_auth_json("rt-1", "rt-2"),
        label_hint: Some("grok-oauth".into()),
        extra: json!({"source": "auth.json"}),
    });

    let rows = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(rows.len(), 2, "different Grok people stay two pool rows");
    let uid1 = rows.iter().find(|row| row.id == "grok-uid-1").unwrap();
    let uid2 = rows.iter().find(|row| row.id == "grok-uid-2").unwrap();
    assert!(
        !uid1.credentials.to_string().contains("rt-2"),
        "uid-1 row must not copy the sibling refresh_token"
    );
    assert!(
        !uid2.credentials.to_string().contains("rt-1"),
        "uid-2 row must not copy the sibling refresh_token"
    );
    assert!(uid1.credentials.to_string().contains("rt-1"));
    assert!(uid2.credentials.to_string().contains("rt-2"));
    assert!(
        uid1.is_current,
        "list must not steal current from the default ::client person"
    );
    assert!(!uid2.is_current);
}

#[test]
fn import_live_expands_grok_two_slot_auth_json_into_two_rows() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: grok_two_slot_auth_json("rt-1", "rt-2"),
        label_hint: Some("grok-oauth".into()),
        extra: json!({"source": "auth.json"}),
    });

    let imported = svc.import_live(AgentId::Grok, None).unwrap();
    let rows = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(rows.len(), 2, "import_live must create two Grok people");
    assert!(rows.iter().any(|row| row.id == imported.id));
    let uid1 = rows
        .iter()
        .find(|row| row.credentials.to_string().contains("uid-1"))
        .unwrap();
    let uid2 = rows
        .iter()
        .find(|row| row.credentials.to_string().contains("uid-2"))
        .unwrap();
    assert!(!uid1.credentials.to_string().contains("rt-2"));
    assert!(!uid2.credentials.to_string().contains("rt-1"));
    assert!(
        uid1.is_current,
        "empty-pool import must activate the default ::client slot, not last-sorted"
    );
    assert!(!uid2.is_current);
    assert!(imported.is_current);
    assert!(imported.credentials.to_string().contains("uid-1"));
}

#[test]
fn list_keeps_non_default_grok_current_across_two_slots() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    for (index, (uid, email, rt)) in [
        ("uid-1", "a@example.com", "rt-1"),
        ("uid-2", "b@example.com", "rt-2"),
    ]
    .into_iter()
    .enumerate()
    {
        svc.repo()
            .create(&Account {
                id: format!("grok-{uid}"),
                agent_id: AgentId::Grok,
                kind: AccountKind::Oauth,
                label: email.into(),
                credentials: json!({
                    "format": "auth_json",
                    "body": {
                        "https://auth.x.ai::client": {
                            "email": email,
                            "user_id": uid,
                            "key": format!("at-{index}"),
                            "refresh_token": rt
                        }
                    }
                }),
                extra: json!({"source": "live", "identityLabel": email}),
                status: "active".into(),
                is_current: index == 1,
                created_at: "2026-01-01 00:00:00.000000".into(),
                updated_at: "2026-01-01 00:00:00.000000".into(),
            })
            .unwrap();
    }

    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: grok_two_slot_auth_json("rt-1", "rt-2"),
        label_hint: Some("grok-oauth".into()),
        extra: json!({"source": "auth.json"}),
    });

    let rows = svc.list(Some(AgentId::Grok)).unwrap();
    let uid1 = rows.iter().find(|row| row.id == "grok-uid-1").unwrap();
    let uid2 = rows.iter().find(|row| row.id == "grok-uid-2").unwrap();
    assert!(
        uid2.is_current,
        "list must keep a user-chosen non-default Grok current"
    );
    assert!(!uid1.is_current);
}

#[test]
fn import_live_two_slot_keeps_existing_non_default_current() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    for (index, (uid, email, rt)) in [
        ("uid-1", "a@example.com", "rt-1"),
        ("uid-2", "b@example.com", "rt-2"),
    ]
    .into_iter()
    .enumerate()
    {
        svc.repo()
            .create(&Account {
                id: format!("grok-{uid}"),
                agent_id: AgentId::Grok,
                kind: AccountKind::Oauth,
                label: email.into(),
                credentials: json!({
                    "format": "auth_json",
                    "body": {
                        "https://auth.x.ai::https://api.x.ai": {
                            "email": email,
                            "user_id": uid,
                            "key": format!("at-{index}"),
                            "refresh_token": rt
                        }
                    }
                }),
                extra: json!({"source": "live", "identityLabel": email}),
                status: "active".into(),
                is_current: index == 1,
                created_at: "2026-01-01 00:00:00.000000".into(),
                updated_at: "2026-01-01 00:00:00.000000".into(),
            })
            .unwrap();
    }

    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: grok_two_slot_auth_json("rt-1", "rt-2"),
        label_hint: Some("grok-oauth".into()),
        extra: json!({"source": "auth.json"}),
    });

    let imported = svc.import_live(AgentId::Grok, None).unwrap();
    let rows = svc.list(Some(AgentId::Grok)).unwrap();
    let uid1 = rows
        .iter()
        .find(|row| row.credentials.to_string().contains("uid-1"))
        .unwrap();
    let uid2 = rows
        .iter()
        .find(|row| row.credentials.to_string().contains("uid-2"))
        .unwrap();
    assert_eq!(imported.id, uid2.id);
    assert!(
        uid2.is_current,
        "import_live must keep the existing current person when they are still in auth.json"
    );
    assert!(!uid1.is_current);
}

#[test]
fn list_activates_default_grok_slot_when_current_person_left_the_file() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    seed_two_grok_people(&svc, "uid-2");
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: grok_two_slot_uid1_and_uid3(),
        label_hint: Some("grok-oauth".into()),
        extra: json!({"source": "auth.json"}),
    });

    let rows = svc.list(Some(AgentId::Grok)).unwrap();
    let uid1 = rows.iter().find(|row| row.id == "grok-uid-1").unwrap();
    let uid2 = rows.iter().find(|row| row.id == "grok-uid-2").unwrap();
    assert!(
        uid1.is_current,
        "when the current person leaves auth.json, list must activate ::client"
    );
    assert!(!uid2.is_current);
    assert!(rows
        .iter()
        .any(|row| row.credentials.to_string().contains("uid-3")));
}

#[test]
fn import_live_activates_default_grok_slot_when_current_person_left_the_file() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    seed_two_grok_people(&svc, "uid-2");
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: grok_two_slot_uid1_and_uid3(),
        label_hint: Some("grok-oauth".into()),
        extra: json!({"source": "auth.json"}),
    });

    let imported = svc.import_live(AgentId::Grok, None).unwrap();
    let rows = svc.list(Some(AgentId::Grok)).unwrap();
    let uid1 = rows
        .iter()
        .find(|row| row.credentials.to_string().contains("uid-1"))
        .unwrap();
    let uid2 = rows.iter().find(|row| row.id == "grok-uid-2").unwrap();
    assert_eq!(imported.id, uid1.id);
    assert!(
        uid1.is_current,
        "when the current person leaves auth.json, import_live must activate ::client"
    );
    assert!(!uid2.is_current);
}

#[test]
fn grok_same_person_two_auth_json_slots_collapse_to_one_row() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "https://auth.x.ai::client": {
                    "email": "a@example.com",
                    "user_id": "uid-1",
                    "key": "at-client",
                    "refresh_token": "rt-client"
                },
                "https://auth.x.ai::https://api.x.ai": {
                    "email": "a@example.com",
                    "user_id": "uid-1",
                    "key": "at-api",
                    "refresh_token": "rt-api"
                }
            }
        }),
        label_hint: Some("grok-oauth".into()),
        extra: json!({"source": "auth.json"}),
    });

    let _ = svc.import_live(AgentId::Grok, None).unwrap();
    let rows = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(
        rows.len(),
        1,
        "same Grok person across two slots still collapses to one row"
    );
    assert_eq!(rows[0].credentials["user_id"], "uid-1");
}

#[test]
fn grok_bundle_live_does_not_identity_merge_oauth_people() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    svc.repo()
        .create(&Account {
            id: "grok-uid-1".into(),
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "a@example.com".into(),
            credentials: json!({
                "format": "auth_json",
                "body": {
                    "https://auth.x.ai::client": {
                        "email": "a@example.com",
                        "user_id": "uid-1",
                        "key": "at-oauth",
                        "refresh_token": "rt-oauth"
                    }
                }
            }),
            extra: json!({"source": "live", "identityLabel": "a@example.com"}),
            status: "active".into(),
            is_current: true,
            created_at: "2026-01-01 00:00:00.000000".into(),
            updated_at: "2026-01-01 00:00:00.000000".into(),
        })
        .unwrap();
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::ApiKey,
        credentials: json!({
            "format": "grok_bundle",
            "api_key": "xai-file-key",
            "auth": {
                "https://auth.x.ai::client": {
                    "email": "a@example.com",
                    "user_id": "uid-1",
                    "key": "at-file",
                    "refresh_token": "rt-file"
                }
            }
        }),
        label_hint: Some("API Key".into()),
        extra: json!({"source": "config.toml+auth.json"}),
    });

    let rows = svc.list(Some(AgentId::Grok)).unwrap();
    let oauth = rows
        .iter()
        .find(|row| row.kind == AccountKind::Oauth)
        .expect("OAuth person must stay a separate row");
    assert!(oauth.credentials.to_string().contains("rt-oauth"));
    assert!(
        !oauth.credentials.to_string().contains("rt-file"),
        "API key grok_bundle must not copy OAuth file tokens onto the OAuth row"
    );
}

#[test]
fn list_exposes_live_auth_state_only_on_matching_current_account() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    let live = LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::ApiKey,
        credentials: json!({"format": "api_key", "api_key": "live-auth-key"}),
        label_hint: Some("live".into()),
        extra: json!({}),
    };
    adapter.set_live(live);
    let current = svc.import_live(AgentId::Grok, None).unwrap();
    adapter.set_auth_state(AuthState {
        agent: AgentId::Grok,
        kind: Some("api_key".into()),
        summary: "configured".into(),
        has_credentials: true,
        health: crate::models::AuthHealth::Configured,
        source: Some("fake-live-auth".into()),
        revision: Some("opaque-r1".into()),
        also_present: Vec::new(),
        secret_hash: None,
    });

    let row = svc
        .list(Some(AgentId::Grok))
        .unwrap()
        .into_iter()
        .find(|row| row.id == current.id)
        .unwrap();
    assert_eq!(row.extra["authHealth"], "configured");
    assert_eq!(row.extra["authSource"], "fake-live-auth");
    assert_eq!(row.extra["liveRevision"], "opaque-r1");
    let persisted = svc.repo().get_by_id(&current.id).unwrap().unwrap();
    assert!(
        persisted.extra.get("authHealth").is_none(),
        "live AuthState must not be persisted into the pool"
    );
}

#[test]
fn pi_multi_provider_list_and_switch_never_create_combined_pool_row() {
    let (_root, svc, adapter) = live_svc(AgentId::Pi);
    let body = json!({
        "anthropic": {"type": "oauth", "access": "anthropic-access", "refresh": "anthropic-refresh"},
        "xai": {"type": "oauth", "access": "xai-access", "refresh": "xai-refresh"}
    });
    let slots = crate::adapters::pi_auth::expand_auth_to_live_accounts(&body).unwrap();
    for (index, live) in slots.iter().enumerate() {
        let provider = live.credentials["provider"].as_str().unwrap();
        svc.repo()
            .create(&Account {
                id: format!("pi-{provider}"),
                agent_id: AgentId::Pi,
                kind: live.kind,
                label: live.label_hint.clone().unwrap(),
                credentials: live.credentials.clone(),
                extra: live.extra.clone(),
                status: "active".into(),
                is_current: index == 0,
                created_at: "2026-01-01 00:00:00.000000".into(),
                updated_at: "2026-01-01 00:00:00.000000".into(),
            })
            .unwrap();
    }
    adapter.set_live(crate::adapters::pi_auth::combined_live_account(&body).unwrap());

    let listed = svc.list(Some(AgentId::Pi)).unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|row| row.id.starts_with("pi-")));
    let target = listed
        .iter()
        .find(|row| row.credentials["provider"] == "xai")
        .unwrap();
    assert!(
        !target.is_current,
        "syncing another Pi provider must not move the global current marker"
    );

    let switched = svc.switch(&target.id, AgentId::Pi).unwrap();
    assert_eq!(switched.account.id, target.id);
    assert!(switched.account.is_current);
    let after = svc.list(Some(AgentId::Pi)).unwrap();
    assert_eq!(
        after.len(),
        2,
        "combined auth.json must never become a pool row"
    );
    assert_eq!(after.iter().filter(|row| row.is_current).count(), 1);
    assert!(
        after
            .iter()
            .find(|row| row.id == target.id)
            .unwrap()
            .is_current
    );
}

#[test]
fn pi_live_reconcile_keeps_oauth_provider_slots_separate_when_tokens_match() {
    let (_root, svc, adapter) = live_svc(AgentId::Pi);
    let body = json!({
        "anthropic": {"type": "oauth", "email": "shared@example.com", "access": "shared-access", "refresh": "shared-refresh"},
        "xai": {"type": "oauth", "email": "shared@example.com", "access": "shared-access", "refresh": "shared-refresh"}
    });
    adapter.set_live(crate::adapters::pi_auth::combined_live_account(&body).unwrap());

    let rows = svc.list(Some(AgentId::Pi)).unwrap();
    assert_eq!(
        rows.len(),
        2,
        "each Pi OAuth provider owns its own live slot"
    );
    for provider in ["anthropic", "xai"] {
        let row = rows
            .iter()
            .find(|row| row.credentials["provider"] == provider)
            .unwrap_or_else(|| panic!("missing Pi provider slot {provider}"));
        assert_eq!(row.credentials["access_token"], "shared-access");
        assert_eq!(row.credentials["refresh_token"], "shared-refresh");
    }
}

#[test]
fn pi_live_reconcile_keeps_api_key_provider_slots_separate_when_keys_match() {
    let (_root, svc, adapter) = live_svc(AgentId::Pi);
    let body = json!({
        "openai": {"type": "api_key", "email": "shared@example.com", "key": "shared-api-key"},
        "openrouter": {"type": "api_key", "email": "shared@example.com", "key": "shared-api-key"}
    });
    adapter.set_live(crate::adapters::pi_auth::combined_live_account(&body).unwrap());

    let rows = svc.list(Some(AgentId::Pi)).unwrap();
    assert_eq!(
        rows.len(),
        2,
        "each Pi API-key provider owns its own live slot"
    );
    for provider in ["openai", "openrouter"] {
        let row = rows
            .iter()
            .find(|row| row.credentials["provider"] == provider)
            .unwrap_or_else(|| panic!("missing Pi provider slot {provider}"));
        assert_eq!(row.credentials["api_key"], "shared-api-key");
    }
}

#[test]
fn concurrent_live_reconcile_without_lock_dir_creates_one_pool_row() {
    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let path = root.path().join("live").join("auth.json");
    let adapter = Arc::new(FakeAdapter::new(AgentId::Grok, path));
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {"email": "race@example.com", "user_id": "race-user", "key": "race-grant"}
        }),
        label_hint: Some("race@example.com".into()),
        extra: json!({}),
    });
    let mut registry = AdapterRegistry::new();
    registry.register(adapter);
    // with_registry intentionally has no lock_dir; this exercises the
    // process-local per-agent reconcile lock instead of AgentWriteLock.
    let svc = Arc::new(AccountService::with_registry(db, registry));

    let start = Arc::new(Barrier::new(3));
    let left_svc = Arc::clone(&svc);
    let left_start = Arc::clone(&start);
    let left = thread::spawn(move || {
        left_start.wait();
        left_svc.list(Some(AgentId::Grok))
    });
    let right_svc = Arc::clone(&svc);
    let right_start = Arc::clone(&start);
    let right = thread::spawn(move || {
        right_start.wait();
        right_svc.list(Some(AgentId::Grok))
    });
    start.wait();

    assert_eq!(left.join().unwrap().unwrap().len(), 1);
    assert_eq!(right.join().unwrap().unwrap().len(), 1);
    let rows = svc.repo().list(Some(AgentId::Grok)).unwrap();
    assert_eq!(
        rows.len(),
        1,
        "concurrent reconcile must not duplicate UUID rows"
    );
    assert!(rows[0].is_current);
}

#[test]
fn live_reconcile_binding_failure_rolls_back_noncurrent_credential_update() {
    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let path = root.path().join("live").join("auth.json");
    let adapter = Arc::new(FakeAdapter::new(AgentId::Grok, path));
    let mut registry = AdapterRegistry::new();
    registry.register(adapter.clone());
    let svc = AccountService::with_registry(db.clone(), registry);

    let original_current = svc
        .create(AccountInput {
            agent_id: AgentId::Grok,
            kind: AccountKind::ApiKey,
            label: "current".into(),
            credentials: json!({"format": "api_key", "api_key": "current-key"}),
            extra: json!({}),
            is_current: true,
        })
        .unwrap();
    let pending = svc
        .create(AccountInput {
            agent_id: AgentId::Grok,
            kind: AccountKind::ApiKey,
            label: "pending".into(),
            credentials: json!({"format": "api_key", "api_key": "matching-key", "revision": "old"}),
            extra: json!({}),
            is_current: false,
        })
        .unwrap();
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::ApiKey,
        credentials: json!({"format": "api_key", "api_key": "matching-key", "revision": "new"}),
        label_hint: Some("pending".into()),
        extra: json!({}),
    });
    db.with_conn(|conn| {
        let trigger = format!(
            "CREATE TRIGGER fail_live_reconcile_binding \
             BEFORE UPDATE ON agent_active_bindings \
             WHEN NEW.account_id = '{}' \
             BEGIN SELECT RAISE(ABORT, 'injected binding failure'); END;",
            pending.id.replace('\'', "''")
        );
        conn.execute_batch(&trigger)?;
        Ok(())
    })
    .unwrap();

    // list is intentionally best-effort; it reports the stable pool after the
    // failed reconcile rather than surfacing a partially updated authorization.
    let _ = svc.list(Some(AgentId::Grok)).unwrap();
    let pending_after = svc.repo().get_by_id(&pending.id).unwrap().unwrap();
    assert_eq!(pending_after.credentials["revision"], "old");
    assert!(!pending_after.is_current);
    assert_eq!(
        svc.repo().get_current(AgentId::Grok).unwrap().unwrap().id,
        original_current.id
    );
    let binding = crate::services::ConnectionService::new(db)
        .get_active(AgentId::Grok)
        .unwrap()
        .unwrap();
    assert_eq!(
        binding.account_id.as_deref(),
        Some(original_current.id.as_str())
    );
}

#[test]
fn import_live_cleans_only_same_authorization_ticket_dups() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    let ticket = json!({
        "format": "auth_json",
        "body": {
            "provider": {
                "email": "a@example.com",
                "key": "same-ticket"
            }
        }
    });
    // Two legacy rows with identical credentials (true dups)
    let old = Account {
        id: "grok-live-old".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::Oauth,
        label: "a@example.com".into(),
        credentials: ticket.clone(),
        extra: json!({"source": "live"}),
        status: "active".into(),
        is_current: false,
        created_at: "2026-01-01 00:00:00.000000".into(),
        updated_at: "2026-01-01 00:00:00.000000".into(),
    };
    let newer = Account {
        id: "grok-live-new".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::Oauth,
        label: "a@example.com".into(),
        credentials: ticket.clone(),
        extra: json!({"source": "live"}),
        status: "active".into(),
        is_current: true,
        created_at: "2026-01-02 00:00:00.000000".into(),
        updated_at: "2026-01-02 00:00:00.000000".into(),
    };
    // Different person — must survive same-identity cleanup.
    let other_auth = Account {
        id: "grok-live-other".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::Oauth,
        label: "b@example.com".into(),
        credentials: json!({
            "format": "auth_json",
            "body": {
                "provider": {
                    "email": "b@example.com",
                    "key": "other-ticket"
                }
            }
        }),
        extra: json!({"source": "live", "identityLabel": "b@example.com"}),
        status: "active".into(),
        is_current: false,
        created_at: "2026-01-03 00:00:00.000000".into(),
        updated_at: "2026-01-03 00:00:00.000000".into(),
    };
    svc.repo().create(&old).unwrap();
    svc.repo().create(&newer).unwrap();
    svc.repo().create(&other_auth).unwrap();

    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: ticket,
        label_hint: Some("a@example.com".into()),
        extra: json!({}),
    });
    let merged = svc.import_live(AgentId::Grok, None).unwrap();
    assert_eq!(merged.id, "grok-live-new");
    let list = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(list.len(), 2, "a different OAuth identity must remain");
    assert!(list.iter().any(|a| a.id == "grok-live-other"));
    assert!(!list.iter().any(|a| a.id == "grok-live-old"));
}

#[test]
fn add_api_key_dedupes_same_key() {
    let (_root, svc, _) = live_svc(AgentId::Claude);
    let a = svc
        .add_api_key(AgentId::Claude, Some("first"), "sk-same-key-aaaa")
        .unwrap();
    let b = svc
        .add_api_key(AgentId::Claude, Some("second"), "sk-same-key-aaaa")
        .unwrap();
    assert_eq!(a.id, b.id);
    assert_eq!(b.label, "second");
    assert_eq!(svc.list(Some(AgentId::Claude)).unwrap().len(), 1);

    let c = svc
        .add_api_key(AgentId::Claude, Some("other"), "sk-different-bbbb")
        .unwrap();
    assert_ne!(c.id, a.id);
    assert_eq!(svc.list(Some(AgentId::Claude)).unwrap().len(), 2);
}

#[test]
fn authorization_key_distinguishes_tokens_not_email() {
    use crate::adapters::default_authorization_key;
    let a = json!({
        "format": "auth_json",
        "body": { "p": { "email": "u@x.com", "key": "tok-1" } }
    });
    let b = json!({
        "format": "auth_json",
        "body": { "p": { "email": "u@x.com", "key": "tok-2" } }
    });
    let ka = default_authorization_key(AccountKind::Oauth, &a).unwrap();
    let kb = default_authorization_key(AccountKind::Oauth, &b).unwrap();
    assert_ne!(ka, kb);
    assert_eq!(
        default_authorization_key(AccountKind::Oauth, &a).unwrap(),
        ka
    );
}

#[test]
fn create_dedupes_same_oauth_ticket() {
    let (_root, svc, _) = live_svc(AgentId::Codex);
    let creds = json!({
        "format": "auth_json",
        "body": { "refresh_token": "rt-shared", "access_token": "at-1" }
    });
    let first = svc
        .create(AccountInput {
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "user@x.com".into(),
            credentials: creds.clone(),
            extra: json!({}),
            is_current: false,
        })
        .unwrap();
    let second = svc
        .create(AccountInput {
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "user@x.com".into(),
            credentials: json!({
                "format": "auth_json",
                "body": { "refresh_token": "rt-shared", "access_token": "at-2-rotated" }
            }),
            extra: json!({}),
            is_current: true,
        })
        .unwrap();
    // Same refresh_token ⇒ same authorization ticket ⇒ one row
    assert_eq!(first.id, second.id);
    assert!(second.is_current);
    assert_eq!(svc.list(Some(AgentId::Codex)).unwrap().len(), 1);
    assert_eq!(
        second
            .credentials
            .pointer("/body/tokens/access_token")
            .or_else(|| second.credentials.get("access_token"))
            .and_then(|value| value.as_str()),
        Some("at-2-rotated")
    );
}

#[test]
fn create_overwrites_same_oauth_identity_different_tokens() {
    let (_root, svc, _) = live_svc(AgentId::Grok);
    let first = svc
        .create(AccountInput {
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "a@example.com".into(),
            credentials: json!({
                "format": "auth_json",
                "body": {
                    "email": "a@example.com",
                    "refresh_token": "rt-1",
                    "access_token": "at-1"
                }
            }),
            extra: json!({ "source": "oauth_pkce" }),
            is_current: false,
        })
        .unwrap();
    let second = svc
        .create(AccountInput {
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "a@example.com".into(),
            credentials: json!({
                "format": "auth_json",
                "body": {
                    "email": "a@example.com",
                    "refresh_token": "rt-2",
                    "access_token": "at-2"
                }
            }),
            extra: json!({ "source": "oauth_pkce" }),
            is_current: false,
        })
        .unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.credentials["body"]["refresh_token"], "rt-2");
    assert_eq!(svc.list(Some(AgentId::Grok)).unwrap().len(), 1);
}

#[test]
fn create_keeps_unknown_oauth_identity_different_tokens_separate() {
    let (_root, svc, _) = live_svc(AgentId::Codex);
    let first = svc
        .create(AccountInput {
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "Codex · OAuth".into(),
            credentials: json!({
                "format": "auth_json",
                "body": { "refresh_token": "rt-a", "access_token": "at-a" }
            }),
            extra: json!({}),
            is_current: false,
        })
        .unwrap();
    let second = svc
        .create(AccountInput {
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "Codex · OAuth".into(),
            credentials: json!({
                "format": "auth_json",
                "body": { "refresh_token": "rt-b", "access_token": "at-b" }
            }),
            extra: json!({}),
            is_current: false,
        })
        .unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(svc.list(Some(AgentId::Codex)).unwrap().len(), 2);
}

#[test]
fn switch_does_not_delete_other_authorizations() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": { "provider": { "email": "a@x.com", "key": "tok-a" } }
        }),
        label_hint: Some("a@x.com".into()),
        extra: json!({}),
    });
    let auth_a = svc.import_live(AgentId::Grok, None).unwrap();
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": { "provider": { "email": "b@x.com", "key": "tok-b" } }
        }),
        label_hint: Some("b@x.com".into()),
        extra: json!({}),
    });
    let auth_b = svc.import_live(AgentId::Grok, None).unwrap();
    assert_ne!(auth_a.id, auth_b.id);
    assert!(auth_b.is_current);

    // Switch back to first authorization
    let switched = svc.switch(&auth_a.id, AgentId::Grok).unwrap();
    assert_eq!(switched.account.id, auth_a.id);
    assert!(switched.account.is_current);

    let list = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(
        list.len(),
        2,
        "a different OAuth identity must remain after switch"
    );
    let b_row = list.iter().find(|a| a.id == auth_b.id).unwrap();
    assert!(!b_row.is_current);
    assert_eq!(b_row.credentials["body"]["provider"]["key"], "tok-b");
}

#[test]
fn switch_collapses_same_oauth_identity_leftovers_instead_of_identity_conflict() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    let make_row = |id: &str, key: &str, current: bool, ts: &str| Account {
        id: id.into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::Oauth,
        label: "same@example.com".into(),
        credentials: json!({
            "format": "auth_json",
            "body": {"email": "same@example.com", "user_id": "same-user", "key": key}
        }),
        extra: json!({"source": "live", "identityLabel": "same@example.com"}),
        status: "active".into(),
        is_current: current,
        created_at: ts.into(),
        updated_at: ts.into(),
    };
    svc.repo()
        .create(&make_row(
            "grok-grant-a",
            "grant-a",
            true,
            "2026-01-01 00:00:00.000000",
        ))
        .unwrap();
    svc.repo()
        .create(&make_row(
            "grok-grant-b",
            "grant-b",
            false,
            "2026-01-02 00:00:00.000000",
        ))
        .unwrap();
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {"email": "same@example.com", "user_id": "same-user", "key": "grant-c"}
        }),
        label_hint: Some("same@example.com".into()),
        extra: json!({}),
    });

    let switched = svc.switch("grok-grant-b", AgentId::Grok).unwrap();
    assert_eq!(switched.account.credentials["body"]["key"], "grant-c");
    let rows = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].is_current);
    assert_eq!(rows[0].credentials["body"]["key"], "grant-c");
}

#[test]
fn account_and_provider_current_are_mutually_exclusive() {
    use crate::models::{Provider, ProviderInput};
    use crate::services::ProviderService;
    use crate::storage::ProviderRepo;

    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let path = root.path().join("live").join("auth.json");
    let adapter = Arc::new(FakeAdapter::new(AgentId::Claude, path));
    let mut registry = AdapterRegistry::new();
    registry.register(adapter.clone());

    let accounts =
        AccountService::with_live(db.clone(), registry.clone(), root.path().join("backups"));
    let providers = ProviderService::with_live(db.clone(), registry, root.path().join("backups"));

    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::Oauth,
        credentials: json!({"format": "credentials_json", "body": {"token": "oauth-1"}}),
        label_hint: Some("user@example.com".into()),
        extra: json!({}),
    });
    let account = accounts
        .import_live(AgentId::Claude, Some("oauth"))
        .unwrap();
    assert!(account.is_current);

    // Switching / creating a current provider demotes the official account.
    let provider = providers
        .create(&ProviderInput {
            id: "claude-relay".into(),
            agent_id: AgentId::Claude,
            name: "relay".into(),
            settings_config: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "sk-relay"}}),
            meta: json!({}),
            is_current: true,
        })
        .unwrap();
    assert!(provider.is_current);
    let account_after = accounts.repo().get_by_id(&account.id).unwrap().unwrap();
    assert!(
        !account_after.is_current,
        "provider current must demote account current"
    );
    assert!(ProviderRepo::new(db.clone())
        .get_current(AgentId::Claude)
        .unwrap()
        .is_some());

    // Switching account back demotes the provider current (pools remain).
    let switched = accounts.switch(&account.id, AgentId::Claude).unwrap();
    assert!(switched.account.is_current);
    let provider_after = providers.repo().get_by_id(&provider.id).unwrap().unwrap();
    assert!(
        !provider_after.is_current,
        "account current must demote provider current"
    );
    // Provider row still exists for later switch-back.
    let pool: Vec<Provider> = providers.list(Some(AgentId::Claude)).unwrap();
    assert_eq!(pool.len(), 1);
    assert_eq!(pool[0].id, provider.id);
}

const CODEX_LEFTOVER_TOML: &str = r#"model_provider = "agenthub_grok_bridge"
model = "grok-4"
preferred_auth_method = "apikey"

[model_providers.agenthub_grok_bridge]
name = "AgentHub Grok Route"
base_url = "http://127.0.0.1:43121/v1"
wire_api = "responses"
"#;

fn official_codex_oauth_credentials() -> serde_json::Value {
    json!({
        "format": "auth_json",
        "body": {
            "auth_mode": "chatgpt",
            "OPENAI_API_KEY": null,
            "tokens": {
                "access_token": "at-official",
                "refresh_token": "rt-official"
            },
            "last_refresh": "2026-08-20T00:00:00Z"
        },
        "email": "41375197@qq.com"
    })
}

#[test]
fn leftover_shaped_codex_live_does_not_throw_identity_conflict() {
    let _home = crate::integrations::agents::codex::leftover::lock_codex_home();
    let root = tempdir().unwrap();
    let home = root.path().join("home");
    let codex = home.join(".codex");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(codex.join("config.toml"), CODEX_LEFTOVER_TOML).unwrap();
    std::fs::write(
        codex.join("auth.json"),
        r#"{ "OPENAI_API_KEY": "sk-leftover" }"#,
    )
    .unwrap();
    let prev = std::env::var_os("CODEX_HOME");
    std::env::set_var("CODEX_HOME", &codex);

    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let path = root.path().join("live").join("auth.json");
    let adapter = Arc::new(FakeAdapter::new(AgentId::Codex, path));
    adapter.set_live(LiveAccount {
        agent: AgentId::Codex,
        kind: AccountKind::Oauth,
        credentials: json!({"format": "auth_json", "body": {"OPENAI_API_KEY": "sk-leftover"}}),
        label_hint: None,
        extra: json!({}),
    });
    let mut registry = AdapterRegistry::new();
    registry.register(adapter.clone());
    let svc = AccountService::with_live(db, registry, root.path().join("backups"));
    svc.repo()
        .create(&Account {
            id: "codex-official".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "41375197@qq.com".into(),
            credentials: official_codex_oauth_credentials(),
            extra: json!({}),
            status: "active".into(),
            is_current: true,
            created_at: "2026-01-01 00:00:00.000000".into(),
            updated_at: "2026-01-01 00:00:00.000000".into(),
        })
        .unwrap();

    let live = adapter.read_account().unwrap();
    let result = svc.validate_live_switch_identity(adapter.as_ref(), AgentId::Codex, &live);
    match prev {
        Some(value) => std::env::set_var("CODEX_HOME", value),
        None => std::env::remove_var("CODEX_HOME"),
    }
    result.expect("leftover-shaped Codex live must not throw identity_conflict");
}

#[test]
fn custom_remote_api_key_live_does_not_throw_identity_conflict() {
    let _home = crate::integrations::agents::codex::leftover::lock_codex_home();
    let root = tempdir().unwrap();
    let home = root.path().join("home");
    let codex = home.join(".codex");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(
        codex.join("config.toml"),
        r#"model_provider = "OpenAI"
model = "gpt-5.5"

[model_providers.OpenAI]
name = "OpenAI"
base_url = "https://mytokens.cc/v1"
"#,
    )
    .unwrap();
    std::fs::write(
        codex.join("auth.json"),
        r#"{ "OPENAI_API_KEY": "sk-mytokens" }"#,
    )
    .unwrap();
    let prev = std::env::var_os("CODEX_HOME");
    std::env::set_var("CODEX_HOME", &codex);

    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let path = root.path().join("live").join("auth.json");
    let adapter = Arc::new(FakeAdapter::new(AgentId::Codex, path));
    adapter.set_live(LiveAccount {
        agent: AgentId::Codex,
        kind: AccountKind::ApiKey,
        credentials: json!({"format": "auth_json", "body": {"OPENAI_API_KEY": "sk-mytokens"}}),
        label_hint: None,
        extra: json!({}),
    });
    let mut registry = AdapterRegistry::new();
    registry.register(adapter.clone());
    let svc = AccountService::with_live(db, registry, root.path().join("backups"));
    svc.repo()
        .create(&Account {
            id: "codex-official".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "41375197@qq.com".into(),
            credentials: official_codex_oauth_credentials(),
            extra: json!({"email": "41375197@qq.com"}),
            status: "active".into(),
            is_current: false,
            created_at: "2026-01-01 00:00:00.000000".into(),
            updated_at: "2026-01-01 00:00:00.000000".into(),
        })
        .unwrap();

    let live = adapter.read_account().unwrap();
    let result = svc.validate_live_switch_identity(adapter.as_ref(), AgentId::Codex, &live);
    match prev {
        Some(value) => std::env::set_var("CODEX_HOME", value),
        None => std::env::remove_var("CODEX_HOME"),
    }
    result.expect("custom remote API key live must not throw identity_conflict");
}

#[test]
fn leftover_claude_bridge_live_does_not_throw_identity_conflict() {
    let _home = crate::integrations::agents::codex::leftover::lock_codex_home();
    let root = tempdir().unwrap();
    let home = root.path().join("home");
    let codex = home.join(".codex");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(
        codex.join("config.toml"),
        r#"model_provider = "agenthub_claude_bridge"
model = "claude-sonnet-4-20250514"

[model_providers.agenthub_claude_bridge]
name = "AgentHub Claude Route"
base_url = "http://127.0.0.1:33923/v1"
"#,
    )
    .unwrap();
    std::fs::write(
        codex.join("auth.json"),
        r#"{ "OPENAI_API_KEY": "ahb_leftover" }"#,
    )
    .unwrap();
    let prev = std::env::var_os("CODEX_HOME");
    std::env::set_var("CODEX_HOME", &codex);

    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let path = root.path().join("live").join("auth.json");
    let adapter = Arc::new(FakeAdapter::new(AgentId::Codex, path));
    adapter.set_live(LiveAccount {
        agent: AgentId::Codex,
        kind: AccountKind::ApiKey,
        credentials: json!({"format": "auth_json", "body": {"OPENAI_API_KEY": "ahb_leftover"}}),
        label_hint: None,
        extra: json!({}),
    });
    let mut registry = AdapterRegistry::new();
    registry.register(adapter.clone());
    let svc = AccountService::with_live(db, registry, root.path().join("backups"));
    svc.repo()
        .create(&Account {
            id: "codex-official".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "41375197@qq.com".into(),
            credentials: official_codex_oauth_credentials(),
            extra: json!({"email": "41375197@qq.com"}),
            status: "active".into(),
            is_current: true,
            created_at: "2026-01-01 00:00:00.000000".into(),
            updated_at: "2026-01-01 00:00:00.000000".into(),
        })
        .unwrap();

    let live = adapter.read_account().unwrap();
    let result = svc.validate_live_switch_identity(adapter.as_ref(), AgentId::Codex, &live);
    match prev {
        Some(value) => std::env::set_var("CODEX_HOME", value),
        None => std::env::remove_var("CODEX_HOME"),
    }
    result.expect("leftover Claude 本机路由 live must not throw identity_conflict");
}

#[test]
fn switch_official_from_leftover_live_does_not_identity_conflict() {
    let _home = crate::integrations::agents::codex::leftover::lock_codex_home();
    let root = tempdir().unwrap();
    let home = root.path().join("home");
    let codex = home.join(".codex");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(codex.join("config.toml"), CODEX_LEFTOVER_TOML).unwrap();
    std::fs::write(
        codex.join("auth.json"),
        r#"{ "OPENAI_API_KEY": "sk-leftover" }"#,
    )
    .unwrap();
    let prev = std::env::var_os("CODEX_HOME");
    std::env::set_var("CODEX_HOME", &codex);

    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let registry = crate::adapters::register_all();
    let svc = AccountService::with_live(db.clone(), registry, root.path().join("backups"));

    let official = svc
        .repo()
        .create(&Account {
            id: "codex-official".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "41375197@qq.com".into(),
            credentials: official_codex_oauth_credentials(),
            extra: json!({}),
            status: "active".into(),
            is_current: true,
            created_at: "2026-01-01 00:00:00.000000".into(),
            updated_at: "2026-01-01 00:00:00.000000".into(),
        })
        .unwrap();
    crate::storage::ProviderRepo::new(db.clone())
        .create(&crate::models::Provider {
            id: "codex-leftover".into(),
            agent_id: AgentId::Codex,
            name: "AgentHub Grok Route".into(),
            settings_config: json!({
                "format": "toml",
                "content": CODEX_LEFTOVER_TOML
            }),
            meta: json!({
                "generatedBy": "adapter",
                "adapterBridge": { "loopbackOnly": true }
            }),
            is_current: true,
            created_at: "2026-01-01 00:00:00.000000".into(),
            updated_at: "2026-01-01 00:00:00.000000".into(),
        })
        .unwrap();

    let switched = svc.switch(&official.id, AgentId::Codex);
    let config = std::fs::read_to_string(codex.join("config.toml")).unwrap();
    let leftover_after = crate::storage::ProviderRepo::new(db.clone())
        .get_by_id("codex-leftover")
        .unwrap()
        .unwrap();
    let listed = svc.list(Some(AgentId::Codex)).unwrap();
    let leftover_after_list = crate::storage::ProviderRepo::new(db)
        .get_by_id("codex-leftover")
        .unwrap()
        .unwrap();
    match prev {
        Some(value) => std::env::set_var("CODEX_HOME", value),
        None => std::env::remove_var("CODEX_HOME"),
    }

    let switched = switched.expect("switch to 官方登录 must not throw");
    assert!(switched.account.is_current);
    assert_eq!(switched.account.id, official.id);
    assert!(
        !leftover_after.is_current,
        "activate_account must demote leftover 本机路由"
    );
    assert!(
        !config.contains("agenthub_grok_bridge"),
        "leftover keys must be stripped from live config.toml"
    );
    assert!(!config.contains("preferred_auth_method"));
    assert!(!config.contains("127.0.0.1"));
    assert_eq!(listed.iter().filter(|row| row.is_current).count(), 1);
    assert!(
        listed
            .iter()
            .find(|row| row.id == official.id)
            .unwrap()
            .is_current
    );
    assert!(
        !leftover_after_list.is_current,
        "list/sync must not re-promote leftover after official activate"
    );
}

/// Click path: official OAuth row (extra.email) + live mytokens API key files
/// (CodexAdapter::read_account reports Oauth, no identity) + leftover
/// 本机路由 mention. 切换 must apply official tokens, not identity_conflict.
#[test]
fn switch_official_from_apikey_live_with_leftover_mention_does_not_identity_conflict() {
    let _home = crate::integrations::agents::codex::leftover::lock_codex_home();
    let root = tempdir().unwrap();
    let home = root.path().join("home");
    let codex = home.join(".codex");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::write(
        codex.join("config.toml"),
        r#"model_provider = "OpenAI"
model = "gpt-5.5"

[model_providers.OpenAI]
name = "OpenAI"
base_url = "https://mytokens.cc/v1"
# leftover mention only (official subtitle 本机路由 127.0.0.1:33923); not active
"#,
    )
    .unwrap();
    std::fs::write(
        codex.join("auth.json"),
        r#"{ "OPENAI_API_KEY": "sk-mytokens-live" }"#,
    )
    .unwrap();
    let prev = std::env::var_os("CODEX_HOME");
    std::env::set_var("CODEX_HOME", &codex);

    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let registry = crate::adapters::register_all();
    let svc = AccountService::with_live(db.clone(), registry, root.path().join("backups"));

    let official = svc
        .repo()
        .create(&Account {
            id: "codex-official".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "41375197@qq.com".into(),
            credentials: official_codex_oauth_credentials(),
            extra: json!({
                "email": "41375197@qq.com",
                "subtitle": "本机路由 127.0.0.1:33923"
            }),
            status: "active".into(),
            is_current: false,
            created_at: "2026-01-01 00:00:00.000000".into(),
            updated_at: "2026-01-01 00:00:00.000000".into(),
        })
        .unwrap();
    svc.repo()
        .create(&Account {
            id: "codex-mytokens".into(),
            agent_id: AgentId::Codex,
            kind: AccountKind::ApiKey,
            label: "mytokens".into(),
            credentials: json!({
                "format": "auth_json",
                "body": { "OPENAI_API_KEY": "sk-mytokens-live" }
            }),
            extra: json!({}),
            status: "active".into(),
            is_current: true,
            created_at: "2026-01-02 00:00:00.000000".into(),
            updated_at: "2026-01-02 00:00:00.000000".into(),
        })
        .unwrap();

    let switched = svc.switch(&official.id, AgentId::Codex);
    let auth = std::fs::read_to_string(codex.join("auth.json")).unwrap();
    let listed = svc.list(Some(AgentId::Codex)).unwrap();
    match prev {
        Some(value) => std::env::set_var("CODEX_HOME", value),
        None => std::env::remove_var("CODEX_HOME"),
    }

    let switched = switched.expect(
        "official 切换 from API-key live must not return identity_conflict",
    );
    assert!(switched.account.is_current);
    assert_eq!(switched.account.id, official.id);
    assert_eq!(
        switched.account.extra["email"],
        "41375197@qq.com",
        "do not overwrite official identity from API-key live"
    );
    assert!(
        auth.contains("at-official"),
        "switch must apply official tokens to live Codex"
    );
    assert!(
        !auth.contains("sk-mytokens-live"),
        "API-key live must be replaced by official OAuth"
    );
    let official_row = listed.iter().find(|row| row.id == official.id).unwrap();
    assert!(official_row.is_current);
    assert_eq!(official_row.extra["email"], "41375197@qq.com");
    assert_eq!(
        listed.iter().filter(|row| row.is_current).count(),
        1
    );
}

#[test]
fn merge_dedup_delete_failure_is_not_reported_as_success() {
    use crate::storage::AccountRepo;

    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let path = root.path().join("live").join("auth.json");
    let adapter = Arc::new(FakeAdapter::new(AgentId::Claude, path));
    let mut registry = AdapterRegistry::new();
    registry.register(adapter);
    let svc = AccountService::with_registry(db.clone(), registry);

    let first = svc
        .add_api_key(AgentId::Claude, Some("one"), "sk-same-key")
        .unwrap();
    AccountRepo::new(db.clone())
        .create(&Account {
            id: "claude-acc-dup".into(),
            agent_id: AgentId::Claude,
            kind: AccountKind::ApiKey,
            label: "dup".into(),
            credentials: json!({"format": "api_key", "api_key": "sk-same-key"}),
            extra: json!({}),
            status: "active".into(),
            is_current: false,
            created_at: "2000-01-01 00:00:00".into(),
            updated_at: "2000-01-01 00:00:00".into(),
        })
        .unwrap();

    db.with_conn(|c| {
        c.execute_batch(
            r#"
            CREATE TRIGGER fail_account_dedup_delete
            BEFORE DELETE ON accounts
            BEGIN
                SELECT RAISE(ABORT, 'injected account delete failure');
            END;
            "#,
        )?;
        Ok(())
    })
    .unwrap();

    // Re-create with same key merges into primary and must delete the other row.
    let err = svc
        .create(AccountInput {
            agent_id: AgentId::Claude,
            kind: AccountKind::ApiKey,
            label: "merged".into(),
            credentials: json!({"format": "api_key", "api_key": "sk-same-key"}),
            extra: json!({}),
            is_current: false,
        })
        .unwrap_err();
    assert_eq!(
        err.code(),
        "db",
        "dedup delete failure must surface as error, not success"
    );
    // At least one of the pool rows still exists; failure must not wipe state silently.
    let remaining = AccountRepo::new(db).list(Some(AgentId::Claude)).unwrap();
    assert!(
        remaining.len() >= 2 || remaining.iter().any(|a| a.id == first.id),
        "failed dedup must not claim success after partial pool mutation"
    );
    let _ = first;
}

#[test]
fn persist_healed_fields_uses_latest_row_on_cas_conflict() {
    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let path = root.path().join("live").join("auth.json");
    let adapter = Arc::new(FakeAdapter::new(AgentId::Codex, path));
    let mut registry = AdapterRegistry::new();
    registry.register(adapter);
    let repo = crate::storage::AccountRepo::new(db.clone());
    let svc = AccountService::with_registry(db, registry);

    let created = svc
        .create(AccountInput {
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "original".into(),
            credentials: json!({"format": "oauth", "access_token": "tok"}),
            extra: json!({}),
            is_current: true,
        })
        .unwrap();

    let mut winner = created.clone();
    winner.label = "winner".into();
    repo.update_healed_fields(&winner, &created.updated_at, "2026-08-15 00:00:01")
        .unwrap();

    let mut stale = created.clone();
    stale.label = "stale-writer".into();
    let resolved = svc
        .persist_healed_fields(&stale, &created.updated_at)
        .unwrap();
    assert_eq!(resolved.label, "winner");
    assert_eq!(resolved.updated_at, "2026-08-15 00:00:01");
}

#[test]
fn live_api_key_without_identity_is_not_imported() {
    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let path = root.path().join("live").join("settings.json");
    let adapter = Arc::new(FakeAdapter::new(AgentId::Claude, path));
    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::ApiKey,
        credentials: json!({"format": "api_key", "api_key": "sk-no-identity"}),
        label_hint: None,
        extra: json!({}),
    });
    let mut registry = AdapterRegistry::new();
    registry.register(adapter);
    let svc = AccountService::with_registry(db, registry);

    let rows = svc.list(Some(AgentId::Claude)).unwrap();
    assert!(
        rows.is_empty(),
        "API key live snapshots without a stable identity stay fail-closed"
    );
}

#[test]
fn add_api_key_writes_unknown_surface() {
    let (_root, svc, _) = live_svc(AgentId::Grok);
    let added = svc
        .add_api_key(AgentId::Grok, Some("work"), "xai-secret-key-1234")
        .unwrap();
    assert_eq!(added.extra["surface"], "unknown");
    let stored = svc.get(&added.id, Some(AgentId::Grok)).unwrap();
    assert_eq!(stored.extra["surface"], "unknown");
}

#[test]
fn import_live_writes_anthropic_and_grok_subscription_surface() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::ApiKey,
        credentials: json!({
            "format": "api_key",
            "api_key": "sk-ant-live",
            "base_url": "https://api.anthropic.com"
        }),
        label_hint: Some("Anthropic live".into()),
        extra: json!({"provider": "anthropic"}),
    });
    let imported = svc
        .import_live(AgentId::Claude, Some("Anthropic live"))
        .unwrap();
    assert_eq!(imported.extra["surface"], "anthropic-api");
    assert_eq!(imported.extra["source"], "live");

    let (_root, grok_svc, grok) = live_svc(AgentId::Grok);
    grok.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({"refresh_token": "r", "access_token": "a"}),
        label_hint: Some("Grok live".into()),
        extra: json!({}),
    });
    let grok_imported = grok_svc
        .import_live(AgentId::Grok, Some("Grok live"))
        .unwrap();
    assert_eq!(grok_imported.extra["surface"], "grok-xai-subscription");
}

#[test]
fn live_reconcile_new_row_and_rotation_keep_account_surface() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::Oauth,
        credentials: json!({
            "access_token": "access-1",
            "refresh_token": "refresh-1",
            "email": "surface@example.com"
        }),
        label_hint: Some("surface@example.com".into()),
        extra: json!({}),
    });
    let first = svc.list(Some(AgentId::Claude)).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].extra["surface"], "claude-subscription");
    let id = first[0].id.clone();

    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::Oauth,
        credentials: json!({
            "access_token": "access-2",
            "refresh_token": "refresh-2",
            "email": "surface@example.com"
        }),
        label_hint: Some("surface@example.com".into()),
        extra: json!({}),
    });
    let second = svc.list(Some(AgentId::Claude)).unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].id, id);
    assert_eq!(second[0].extra["surface"], "claude-subscription");
}

#[test]
fn live_reconcile_matching_legacy_row_heals_missing_surface() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    let credentials = json!({
        "access_token": "legacy-access",
        "refresh_token": "legacy-refresh",
        "email": "legacy-surface@example.com"
    });
    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::Oauth,
        credentials: credentials.clone(),
        label_hint: Some("legacy-surface@example.com".into()),
        extra: json!({}),
    });
    let legacy = Account {
        id: "legacy-surface-account".into(),
        agent_id: AgentId::Claude,
        kind: AccountKind::Oauth,
        label: "legacy-surface@example.com".into(),
        credentials,
        extra: json!({}),
        status: "active".into(),
        is_current: true,
        created_at: "2026-08-21T00:00:00Z".into(),
        updated_at: "2026-08-21T00:00:00Z".into(),
    };
    svc.repo().create(&legacy).unwrap();
    let rows = svc.list(Some(AgentId::Claude)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].extra["surface"], "claude-subscription");
    assert_eq!(
        svc.repo().get_by_id(&legacy.id).unwrap().unwrap().extra["surface"],
        "claude-subscription"
    );
}

fn spawn_oauth_token_server(access: &str, refresh: &str) -> (u16, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let access = access.to_string();
    let refresh = refresh.to_string();
    let handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut acc = Vec::new();
            loop {
                let mut buf = [0u8; 1024];
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        acc.extend_from_slice(&buf[..n]);
                        if acc.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let body = format!(
                r#"{{"access_token":"{access}","refresh_token":"{refresh}","token_type":"Bearer","expires_in":3600}}"#
            );
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
            let _ = stream.shutdown(std::net::Shutdown::Write);
        }
    });
    (port, handle)
}

#[test]
fn grok_hub_pkce_refresh_updates_pool_without_writing_auth_json() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    let created = svc
        .create(AccountInput {
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "hub-pkce".into(),
            credentials: json!({
                "type": "oauth",
                "provider": "xai",
                "access_token": "old-access",
                "refresh_token": "old-refresh"
            }),
            extra: json!({ "source": "oauth_pkce" }),
            is_current: false,
        })
        .unwrap();
    let (port, server) = spawn_oauth_token_server("new-access", "new-refresh");
    let refreshed = crate::oauth::with_token_url_override(
        format!("http://127.0.0.1:{port}/oauth/token"),
        || svc.refresh_token(&created.id, AgentId::Grok),
    )
    .unwrap();
    let _ = server.join();
    assert_eq!(refreshed.credentials["access_token"], "new-access");
    assert_eq!(refreshed.credentials["refresh_token"], "new-refresh");
    assert_eq!(refreshed.extra["source"], "oauth_refresh");
    assert_eq!(
        refreshed.extra["surface"], "grok-xai-subscription",
        "token refresh must keep the classified ticket surface"
    );
    assert_eq!(
        svc.repo().get_by_id(&created.id).unwrap().unwrap().extra["surface"],
        "grok-xai-subscription"
    );
    assert_eq!(
        adapter.write_attempts.load(Ordering::SeqCst),
        0,
        "hub-owned refresh with no live CLI file must not write grok auth.json"
    );
}

#[test]
fn grok_cli_owned_auth_json_refresh_is_refused_without_token_endpoint() {
    let (_root, svc, _) = live_svc(AgentId::Grok);
    let created = svc
        .create(AccountInput {
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "cli-owned".into(),
            credentials: json!({
                "format": "auth_json",
                "body": {
                    "email": "a@example.com",
                    "key": "cli-access",
                    "refresh_token": "cli-refresh"
                }
            }),
            extra: json!({ "source": "auth.json" }),
            is_current: false,
        })
        .unwrap();
    let err = crate::oauth::with_token_url_override("http://127.0.0.1:1/oauth/token", || {
        svc.refresh_token(&created.id, AgentId::Grok)
    })
    .unwrap_err();
    assert_eq!(err.code(), "unsupported");
    assert!(
        err.to_string().contains("同步当前登录"),
        "cli-owned grok refresh must guide 同步当前登录, got {err}"
    );
}

#[test]
fn cli_owned_follow_rereads_rotated_access_from_temp_auth_json() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "email": "a@example.com",
                "user_id": "uid-1",
                "key": "access-a",
                "refresh_token": "refresh-shared"
            }
        }),
        label_hint: Some("a@example.com".into()),
        extra: json!({"source": "auth.json"}),
    });
    let imported = svc.import_live(AgentId::Grok, None).unwrap();
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "email": "a@example.com",
                "user_id": "uid-1",
                "key": "access-b",
                "refresh_token": "refresh-shared"
            }
        }),
        label_hint: Some("a@example.com".into()),
        extra: json!({"source": "auth.json"}),
    });
    let followed = svc
        .follow_cli_owned_access(&imported.id, AgentId::Grok)
        .unwrap();
    assert_eq!(followed.as_deref(), Some("access-b"));
    let stored = svc.get(&imported.id, Some(AgentId::Grok)).unwrap();
    assert_eq!(stored.credentials["body"]["key"], "access-b");
}

#[test]
fn cli_owned_follow_unchanged_auth_json_does_not_set_token_expired() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "email": "a@example.com",
                "user_id": "uid-1",
                "key": "access-a",
                "refresh_token": "refresh-shared"
            }
        }),
        label_hint: Some("a@example.com".into()),
        extra: json!({"source": "auth.json"}),
    });
    let imported = svc.import_live(AgentId::Grok, None).unwrap();
    let followed = svc
        .follow_cli_owned_access(&imported.id, AgentId::Grok)
        .unwrap();
    assert!(followed.is_none());
    let stored = svc.get(&imported.id, Some(AgentId::Grok)).unwrap();
    assert_ne!(
        stored.extra.get("health").and_then(|v| v.as_str()),
        Some("needs_login")
    );
    assert_ne!(
        stored.extra.get("tokenExpired").and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[test]
fn codex_imported_auth_json_refresh_is_refused() {
    let (_root, svc, _) = live_svc(AgentId::Codex);
    let created = svc
        .create(AccountInput {
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "codex-cli".into(),
            credentials: official_codex_oauth_credentials(),
            extra: json!({ "source": "auth.json" }),
            is_current: false,
        })
        .unwrap();
    let err = crate::oauth::with_token_url_override("http://127.0.0.1:1/oauth/token", || {
        svc.refresh_token(&created.id, AgentId::Codex)
    })
    .unwrap_err();
    assert_eq!(err.code(), "unsupported");
    assert!(err.to_string().contains("同步当前登录"));
}

#[test]
fn stale_snapshot_does_not_overwrite_rotated_key() {
    let (_root, svc, _) = live_svc(AgentId::Claude);
    let created = svc
        .add_api_key(AgentId::Claude, Some("work"), "sk-old-key-aaaa")
        .unwrap();
    let stale = created.clone();
    let rotated = svc
        .update_api_key(AgentId::Claude, &created.id, None, Some("sk-new-key-bbbb"))
        .unwrap();
    assert_eq!(rotated.credentials["api_key"], "sk-new-key-bbbb");

    let mut stale_write = stale.clone();
    stale_write.label = "stale-writer".into();
    let resolved = svc
        .persist_healed_fields(&stale_write, &stale.updated_at)
        .unwrap();
    assert_eq!(resolved.credentials["api_key"], "sk-new-key-bbbb");
    assert_ne!(resolved.label, "stale-writer");
}

#[test]
fn concurrent_add_api_key_same_authorization_keeps_one_row() {
    let (_root, svc, _) = live_svc(AgentId::Claude);
    let svc = Arc::new(svc);
    let start = Arc::new(Barrier::new(3));
    let left_svc = Arc::clone(&svc);
    let left_start = Arc::clone(&start);
    let left = thread::spawn(move || {
        left_start.wait();
        left_svc.add_api_key(AgentId::Claude, Some("left"), "sk-race-key-aaaa")
    });
    let right_svc = Arc::clone(&svc);
    let right_start = Arc::clone(&start);
    let right = thread::spawn(move || {
        right_start.wait();
        right_svc.add_api_key(AgentId::Claude, Some("right"), "sk-race-key-aaaa")
    });
    start.wait();

    let left = left.join().unwrap().unwrap();
    let right = right.join().unwrap().unwrap();
    assert_eq!(left.id, right.id);
    let rows = svc.repo().list(Some(AgentId::Claude)).unwrap();
    assert_eq!(
        rows.len(),
        1,
        "concurrent add of the same key must keep one row"
    );
}

#[test]
fn concurrent_import_live_same_authorization_keeps_one_row() {
    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let path = root.path().join("live").join("auth.json");
    let adapter = Arc::new(FakeAdapter::new(AgentId::Grok, path));
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {"email": "race@example.com", "user_id": "race-user", "key": "race-grant"}
        }),
        label_hint: Some("race@example.com".into()),
        extra: json!({}),
    });
    let mut registry = AdapterRegistry::new();
    registry.register(adapter);
    let svc = Arc::new(AccountService::with_registry(db, registry));
    let start = Arc::new(Barrier::new(3));
    let left_svc = Arc::clone(&svc);
    let left_start = Arc::clone(&start);
    let left = thread::spawn(move || {
        left_start.wait();
        left_svc.import_live(AgentId::Grok, Some("left"))
    });
    let right_svc = Arc::clone(&svc);
    let right_start = Arc::clone(&start);
    let right = thread::spawn(move || {
        right_start.wait();
        right_svc.import_live(AgentId::Grok, Some("right"))
    });
    start.wait();

    let left = left.join().unwrap().unwrap();
    let right = right.join().unwrap().unwrap();
    assert_eq!(left.id, right.id);
    let rows = svc.repo().list(Some(AgentId::Grok)).unwrap();
    assert_eq!(
        rows.len(),
        1,
        "concurrent import of the same authorization must keep one row"
    );
}

#[test]
fn reconcile_does_not_drop_concurrent_refresh_token() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    let live = LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "email": "refresh@example.com",
                "user_id": "refresh-user",
                "key": "grant-old"
            }
        }),
        label_hint: Some("refresh@example.com".into()),
        extra: json!({}),
    };
    adapter.set_live(live.clone());
    let created = svc.import_live(AgentId::Grok, None).unwrap();
    let mut refreshed = created.clone();
    refreshed.credentials["body"]["key"] = json!("grant-new");
    svc.repo()
        .update_healed_fields(&refreshed, &created.updated_at, "2026-08-22 00:00:01")
        .unwrap();

    let persisted = svc
        .persist_reconciled_live_row(AgentId::Grok, created, true, true)
        .unwrap();
    assert_eq!(persisted.credentials["body"]["key"], "grant-new");
}

#[test]
fn unrecognized_surface_survives_merge_and_reconcile() {
    let (_root, svc, adapter) = live_svc(AgentId::Claude);
    let added = svc
        .add_api_key(AgentId::Claude, Some("work"), "sk-surface-key-aaaa")
        .unwrap();
    let mut future = added.clone();
    future.extra["surface"] = json!("future-surface-v9");
    svc.repo()
        .update_healed_fields(&future, &added.updated_at, "2026-08-22 00:00:02")
        .unwrap();

    let merged = svc
        .add_api_key(AgentId::Claude, Some("again"), "sk-surface-key-aaaa")
        .unwrap();
    assert_eq!(merged.extra["surface"], "future-surface-v9");

    adapter.set_live(LiveAccount {
        agent: AgentId::Claude,
        kind: AccountKind::ApiKey,
        credentials: json!({"format": "api_key", "api_key": "sk-surface-key-aaaa"}),
        label_hint: Some("work".into()),
        extra: json!({}),
    });
    let listed = svc.list(Some(AgentId::Claude)).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].extra["surface"], "future-surface-v9");
    assert_eq!(
        svc.repo().get_by_id(&added.id).unwrap().unwrap().extra["surface"],
        "future-surface-v9"
    );
}

fn stamp_file_mtime(path: &Path, updated_at: &str) {
    let dt = super::oauth_file_sync::parse_account_timestamp(updated_at).expect("mtime");
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(std::time::SystemTime::from(dt))
        .unwrap();
}

fn grok_oauth_live(access: &str, refresh: &str) -> LiveAccount {
    LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "email": "a@example.com",
                "user_id": "uid-1",
                "key": access,
                "refresh_token": refresh
            }
        }),
        label_hint: Some("a@example.com".into()),
        extra: json!({"source": "auth.json"}),
    }
}

#[test]
fn oauth_list_does_not_write_cli_file_when_row_is_newer() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(grok_oauth_live("at-file", "rt-file"));
    let imported = svc.import_live(AgentId::Grok, None).unwrap();
    let path = adapter.live_backup_paths()[0].clone();
    stamp_file_mtime(&path, "2020-01-01 00:00:00.000000");

    let mut newer = imported.clone();
    newer.credentials["body"]["key"] = json!("at-row");
    newer.credentials["body"]["refresh_token"] = json!("rt-row");
    svc.repo()
        .update_healed_fields(&newer, &imported.updated_at, "2099-01-01 00:00:00.000000")
        .unwrap();
    let writes_before = adapter.write_attempts.load(Ordering::SeqCst);

    let listed = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].credentials["body"]["refresh_token"], "rt-row");
    let live = adapter.read_account().unwrap();
    assert_eq!(live.credentials["body"]["refresh_token"], "rt-file");
    assert_eq!(live.credentials["body"]["key"], "at-file");
    assert_eq!(
        adapter.write_attempts.load(Ordering::SeqCst),
        writes_before,
        "list reconcile must not write the CLI login file"
    );
}

#[test]
fn oauth_cli_file_newer_than_row_updates_row_file_unchanged() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(grok_oauth_live("at-old", "rt-old"));
    let imported = svc.import_live(AgentId::Grok, None).unwrap();
    let writes_before = adapter.write_attempts.load(Ordering::SeqCst);
    adapter.set_live(grok_oauth_live("at-new", "rt-new"));

    let listed = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, imported.id);
    assert_eq!(listed[0].credentials["body"]["refresh_token"], "rt-new");
    assert_eq!(listed[0].credentials["body"]["key"], "at-new");
    assert_eq!(
        adapter.write_attempts.load(Ordering::SeqCst),
        writes_before,
        "newer CLI file must not be overwritten"
    );
    let live = adapter.read_account().unwrap();
    assert_eq!(live.credentials["body"]["refresh_token"], "rt-new");
}

#[test]
fn oauth_same_rt_access_rotated_newer_file_updates_row() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(grok_oauth_live("at-old", "rt-shared"));
    let imported = svc.import_live(AgentId::Grok, None).unwrap();
    svc.repo()
        .update_healed_fields(
            &imported,
            &imported.updated_at,
            "2020-01-01 00:00:00.000000",
        )
        .unwrap();
    adapter.set_live(grok_oauth_live("at-file", "rt-shared"));
    let writes_before = adapter.write_attempts.load(Ordering::SeqCst);

    let listed = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(listed[0].credentials["body"]["key"], "at-file");
    assert_eq!(listed[0].credentials["body"]["refresh_token"], "rt-shared");
    assert_eq!(adapter.write_attempts.load(Ordering::SeqCst), writes_before);
}

#[test]
fn oauth_different_identity_does_not_write_cli_file() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    let created = svc
        .create(AccountInput {
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "hub-a".into(),
            credentials: json!({
                "type": "oauth",
                "provider": "xai",
                "access_token": "at-a",
                "refresh_token": "rt-a",
                "email": "a@example.com",
                "sub": "uid-a"
            }),
            extra: json!({ "source": "oauth_pkce" }),
            is_current: false,
        })
        .unwrap();
    adapter.set_live(LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {
                "email": "b@example.com",
                "user_id": "uid-b",
                "key": "at-b",
                "refresh_token": "rt-b"
            }
        }),
        label_hint: Some("b@example.com".into()),
        extra: json!({"source": "auth.json"}),
    });
    let path = adapter.live_backup_paths()[0].clone();
    stamp_file_mtime(&path, "2020-01-01 00:00:00.000000");
    svc.repo()
        .update_healed_fields(&created, &created.updated_at, "2099-01-01 00:00:00.000000")
        .unwrap();
    let writes_before = adapter.write_attempts.load(Ordering::SeqCst);

    let listed = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(
        adapter.write_attempts.load(Ordering::SeqCst),
        writes_before,
        "different identity must never write across"
    );
    let live = adapter.read_account().unwrap();
    assert_eq!(live.credentials["body"]["refresh_token"], "rt-b");
    let stored_a = svc.get(&created.id, Some(AgentId::Grok)).unwrap();
    assert_eq!(stored_a.credentials["refresh_token"], "rt-a");
    assert!(
        listed.iter().any(|row| row.id == created.id),
        "row A must stay"
    );
}

#[test]
fn oauth_equal_mtime_different_rt_does_not_overwrite() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(grok_oauth_live("at-row", "rt-row-secret"));
    let imported = svc.import_live(AgentId::Grok, None).unwrap();
    let stamp = "2026-06-15 12:00:00.000000";
    svc.repo()
        .update_healed_fields(&imported, &imported.updated_at, stamp)
        .unwrap();
    adapter.set_live(grok_oauth_live("at-file", "rt-file-secret"));
    stamp_file_mtime(&adapter.live_backup_paths()[0], stamp);
    let writes_before = adapter.write_attempts.load(Ordering::SeqCst);

    let listed = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, imported.id);
    assert_eq!(
        listed[0].credentials["body"]["refresh_token"],
        "rt-row-secret"
    );
    assert_eq!(
        listed[0]
            .extra
            .get("oauthFileSync")
            .and_then(|v| v.as_str()),
        Some("needs_attention")
    );
    assert_eq!(listed[0].updated_at, stamp);
    assert_eq!(adapter.write_attempts.load(Ordering::SeqCst), writes_before);
    let live = adapter.read_account().unwrap();
    assert_eq!(live.credentials["body"]["refresh_token"], "rt-file-secret");

    let listed_again = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(listed_again[0].updated_at, stamp);
    assert_eq!(
        listed_again[0].credentials["body"]["refresh_token"],
        "rt-row-secret"
    );
    assert_eq!(
        listed_again[0]
            .extra
            .get("oauthFileSync")
            .and_then(|v| v.as_str()),
        Some("needs_attention")
    );
    assert_eq!(adapter.write_attempts.load(Ordering::SeqCst), writes_before);
    let live_again = adapter.read_account().unwrap();
    assert_eq!(
        live_again.credentials["body"]["refresh_token"],
        "rt-file-secret"
    );

    let dumped = serde_json::to_string(&listed[0].redacted()).unwrap();
    assert!(
        !dumped.contains("rt-row-secret") && !dumped.contains("rt-file-secret"),
        "redacted list/IPC must not include raw refresh tokens: {dumped}"
    );
}

#[test]
fn grok_hub_pkce_refresh_writes_auth_json_when_same_identity_row_is_newer() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    adapter.set_live(grok_oauth_live("at-file", "old-refresh"));
    stamp_file_mtime(
        &adapter.live_backup_paths()[0],
        "2020-01-01 00:00:00.000000",
    );
    let created = svc
        .create(AccountInput {
            agent_id: AgentId::Grok,
            kind: AccountKind::Oauth,
            label: "hub-pkce".into(),
            credentials: json!({
                "type": "oauth",
                "provider": "xai",
                "access_token": "old-access",
                "refresh_token": "old-refresh",
                "sub": "uid-1"
            }),
            extra: json!({ "source": "oauth_pkce" }),
            is_current: false,
        })
        .unwrap();
    let (port, server) = spawn_oauth_token_server("new-access", "new-refresh");
    let refreshed = crate::oauth::with_token_url_override(
        format!("http://127.0.0.1:{port}/oauth/token"),
        || svc.refresh_token(&created.id, AgentId::Grok),
    )
    .unwrap();
    let _ = server.join();
    assert_eq!(refreshed.credentials["refresh_token"], "new-refresh");
    assert!(
        adapter.write_attempts.load(Ordering::SeqCst) >= 1,
        "same-identity hub refresh must write the CLI login file when the row is newer"
    );
    let live = adapter.read_account().unwrap();
    assert_eq!(live.credentials["refresh_token"], "new-refresh");
    assert_eq!(live.credentials["access_token"], "new-access");
}

#[test]
fn hub_codex_refresh_patches_token_only_auth_json_body() {
    let (_root, svc, adapter) = live_svc(AgentId::Codex);
    adapter.set_live(LiveAccount {
        agent: AgentId::Codex,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "email": "41375197@qq.com",
            "body": {
                "auth_mode": "chatgpt",
                "OPENAI_API_KEY": null,
                "tokens": {
                    "access_token": "at-file",
                    "refresh_token": "old-refresh"
                },
                "last_refresh": "2026-08-20T00:00:00Z"
            }
        }),
        label_hint: Some("codex".into()),
        extra: json!({ "source": "auth.json" }),
    });
    stamp_file_mtime(
        &adapter.live_backup_paths()[0],
        "2020-01-01 00:00:00.000000",
    );
    let created = svc
        .create(AccountInput {
            agent_id: AgentId::Codex,
            kind: AccountKind::Oauth,
            label: "hub-pkce".into(),
            credentials: json!({
                "type": "oauth",
                "provider": "codex",
                "access_token": "at-hub",
                "refresh_token": "old-refresh",
                "email": "41375197@qq.com"
            }),
            extra: json!({ "source": "oauth_pkce" }),
            is_current: false,
        })
        .unwrap();
    let (port, server) = spawn_oauth_token_server("new-access", "new-refresh");
    let refreshed = crate::oauth::with_token_url_override(
        format!("http://127.0.0.1:{port}/oauth/token"),
        || svc.refresh_token(&created.id, AgentId::Codex),
    )
    .unwrap();
    let _ = server.join();
    assert_eq!(refreshed.credentials["refresh_token"], "new-refresh");
    assert!(adapter.write_attempts.load(Ordering::SeqCst) >= 1);
    let live = adapter.read_account().unwrap();
    assert_eq!(
        live.credentials["body"]["tokens"]["refresh_token"],
        "new-refresh"
    );
    assert_eq!(
        live.credentials["body"]["tokens"]["access_token"],
        "new-access"
    );
    assert_eq!(
        live.credentials["body"]["last_refresh"], "2026-08-20T00:00:00Z",
        "token-only patch must keep extra Codex auth.json fields"
    );
}
