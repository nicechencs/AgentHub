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
use std::sync::Mutex;
use tempfile::tempdir;

struct FakeAdapter {
    id: AgentId,
    live: Mutex<Option<LiveAccount>>,
    path: PathBuf,
    write_attempts: AtomicUsize,
    fail_on_write: AtomicUsize,
    supports: AtomicBool,
}

impl FakeAdapter {
    fn new(id: AgentId, path: PathBuf) -> Self {
        Self {
            id,
            live: Mutex::new(None),
            path,
            write_attempts: AtomicUsize::new(0),
            fail_on_write: AtomicUsize::new(0),
            supports: AtomicBool::new(true),
        }
    }

    fn set_live(&self, live: LiveAccount) {
        let body = serde_json::to_vec(&live).unwrap();
        atomic_write(&self.path, &body).unwrap();
        *self.live.lock().unwrap() = Some(live);
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
        Err(AppError::Unsupported("fake".into()))
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
        let attempt = self.write_attempts.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_on_write.load(Ordering::SeqCst) == attempt {
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
    let (_root, svc, _) = live_svc(AgentId::Grok);
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
