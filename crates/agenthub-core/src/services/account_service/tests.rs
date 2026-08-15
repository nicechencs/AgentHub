use super::*;
use crate::adapters::AgentAdapter;
use crate::models::{
    AuthState, Capability, CapabilityState, DetectResult, DetectStatus, InstallChannel, RunOptions,
    RunSpec,
};
use crate::utils::atomic::atomic_write;
use serde_json::json;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Barrier, Mutex};
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
fn import_live_keeps_same_identity_different_tokens() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    // First authorization
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

    // Second authorization for same person, different ticket
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
    assert_ne!(
        first.id, second.id,
        "different tokens must remain separate rows"
    );
    let list = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(list.len(), 2);
    assert!(second.is_current);
    // older authorization still present
    assert!(list.iter().any(|a| a.id == first.id));
    assert!(list.iter().all(|a| a.extra.get("identityLabel").is_some()));
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
fn list_retains_new_grant_without_overwriting_multiple_same_identity_grants() {
    let (_root, svc, adapter) = live_svc(AgentId::Grok);
    let make_live = |key: &str| LiveAccount {
        agent: AgentId::Grok,
        kind: AccountKind::Oauth,
        credentials: json!({
            "format": "auth_json",
            "body": {"email": "same@example.com", "user_id": "same-user", "key": key}
        }),
        label_hint: Some("same@example.com".into()),
        extra: json!({}),
    };

    adapter.set_live(make_live("grant-a"));
    let first = svc.import_live(AgentId::Grok, None).unwrap();
    adapter.set_live(make_live("grant-b"));
    let second = svc.import_live(AgentId::Grok, None).unwrap();

    // A third token for the same identity could be a new grant rather than a
    // rotation. Reconcile must preserve both existing authorizations while
    // making the newly observed live grant current for a single-current agent.
    adapter.set_live(make_live("grant-c"));
    let rows = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .find(|row| row.id == first.id)
            .unwrap()
            .credentials["body"]["key"],
        "grant-a"
    );
    assert_eq!(
        rows.iter()
            .find(|row| row.id == second.id)
            .unwrap()
            .credentials["body"]["key"],
        "grant-b"
    );
    let observed = rows
        .iter()
        .find(|row| row.credentials["body"]["key"] == "grant-c")
        .unwrap();
    assert!(
        observed.is_current,
        "current must match the external live grant"
    );
    assert_eq!(rows.iter().filter(|row| row.is_current).count(), 1);

    // The created live grant becomes an exact match; later reads must not
    // create a duplicate pool row.
    let repeated = svc.list(Some(AgentId::Grok)).unwrap();
    assert_eq!(repeated.len(), 3);
    assert_eq!(
        repeated
            .iter()
            .filter(|row| row.credentials["body"]["key"] == "grant-c")
            .count(),
        1
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
    // Different authorization for same person — must survive cleanup
    let other_auth = Account {
        id: "grok-live-other".into(),
        agent_id: AgentId::Grok,
        kind: AccountKind::Oauth,
        label: "a@example.com".into(),
        credentials: json!({
            "format": "auth_json",
            "body": {
                "provider": {
                    "email": "a@example.com",
                    "key": "other-ticket"
                }
            }
        }),
        extra: json!({"source": "live", "identityLabel": "a@example.com"}),
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
    assert_eq!(list.len(), 2, "other valid authorization must remain");
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
    assert_eq!(second.credentials["body"]["access_token"], "at-2-rotated");
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
            "body": { "provider": { "email": "a@x.com", "key": "tok-b" } }
        }),
        label_hint: Some("a@x.com".into()),
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
        "other authorization must remain after switch"
    );
    let b_row = list.iter().find(|a| a.id == auth_b.id).unwrap();
    assert!(!b_row.is_current);
    assert_eq!(b_row.credentials["body"]["provider"]["key"], "tok-b");
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
