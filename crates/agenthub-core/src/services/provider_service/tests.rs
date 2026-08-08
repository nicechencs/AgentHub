use super::*;
use crate::adapters::AgentAdapter;
use crate::models::{
    AuthState, Capability, CapabilityState, DetectResult, DetectStatus, InstallChannel, RunOptions,
    RunSpec,
};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

struct FakeAdapter {
    id: AgentId,
    config: Mutex<AgentConfig>,
    config_path: PathBuf,
    write_attempts: AtomicUsize,
    fail_on_write: AtomicUsize,
    unsupported_write: AtomicBool,
}

impl FakeAdapter {
    fn new(id: AgentId, config: AgentConfig, config_path: PathBuf) -> Self {
        Self {
            id,
            config: Mutex::new(config),
            config_path,
            write_attempts: AtomicUsize::new(0),
            fail_on_write: AtomicUsize::new(0),
            unsupported_write: AtomicBool::new(false),
        }
    }

    fn config(&self) -> AgentConfig {
        self.config.lock().unwrap().clone()
    }

    fn fail_on_write(&self, attempt: usize) {
        self.fail_on_write.store(attempt, Ordering::SeqCst);
    }

    fn make_write_unsupported(&self) {
        self.unsupported_write.store(true, Ordering::SeqCst);
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

    fn read_config(&self) -> Result<AgentConfig> {
        Ok(self.config())
    }

    fn write_config(&self, config: &AgentConfig) -> Result<()> {
        let attempt = self.write_attempts.fetch_add(1, Ordering::SeqCst) + 1;
        if self.unsupported_write.load(Ordering::SeqCst) {
            return Err(AppError::Unsupported("fake writer disabled".into()));
        }
        if self.fail_on_write.load(Ordering::SeqCst) == attempt {
            return Err(AppError::message(
                "test.write",
                format!("injected write failure {attempt}"),
            ));
        }
        let bytes = serde_json::to_vec(config)?;
        crate::utils::atomic::atomic_write(&self.config_path, &bytes)?;
        *self.config.lock().unwrap() = config.clone();
        Ok(())
    }

    fn read_auth(&self) -> Result<AuthState> {
        Err(AppError::Unsupported("fake".into()))
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        match cap {
            Capability::ConfigWrite => CapabilityState::full(),
            Capability::LiveBackup => CapabilityState::full(),
            _ => CapabilityState::unsupported("fake"),
        }
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        None
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        vec![self.config_path.clone()]
    }

    fn build_run_spec(&self, _binary: &Path, _prompt: &str, _opts: &RunOptions) -> Result<RunSpec> {
        Err(AppError::Unsupported("fake".into()))
    }
}

fn svc() -> (tempfile::TempDir, ProviderService) {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("ah.db")).unwrap();
    (dir, ProviderService::new(db))
}

fn live_svc(
    adapter_id: AgentId,
    config: AgentConfig,
) -> (
    tempfile::TempDir,
    Database,
    ProviderService,
    Arc<FakeAdapter>,
    PathBuf,
) {
    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let config_path = root.path().join("live").join("config.json");
    if !live_config_is_empty(&config.raw) {
        let bytes = serde_json::to_vec(&config).unwrap();
        crate::utils::atomic::atomic_write(&config_path, &bytes).unwrap();
    }
    let adapter = Arc::new(FakeAdapter::new(adapter_id, config, config_path));
    let mut registry = AdapterRegistry::new();
    registry.register(adapter.clone());
    let backups_root = root.path().join("backups");
    let service = ProviderService::with_live(db.clone(), registry, backups_root.clone());
    (root, db, service, adapter, backups_root)
}

fn seed(svc: &ProviderService, id: &str, agent: AgentId, name: &str, current: bool) {
    svc.repo()
        .upsert(&Provider {
            id: id.into(),
            agent_id: agent,
            name: name.into(),
            settings_config: json!({
                "api_key": "sk-live-secret",
                "base_url": "https://relay.example.com"
            }),
            meta: json!({"token": "meta-secret", "note": "n"}),
            is_current: current,
            created_at: "2026-03-01 10:00:00".into(),
            updated_at: "2026-03-02 11:00:00".into(),
        })
        .unwrap();
}

fn input(id: &str, agent: AgentId, name: &str, current: bool) -> ProviderInput {
    ProviderInput {
        id: id.into(),
        agent_id: agent,
        name: name.into(),
        settings_config: json!({"api_key": "sk-live-secret", "base_url": "https://x"}),
        meta: json!({"note": "n"}),
        is_current: current,
    }
}

#[test]
fn list_empty_and_deterministic_order() {
    let (_dir, svc) = svc();
    assert!(svc.list(None).unwrap().is_empty());

    // Insert out of product order: grok, claude, kimi, codex
    seed(&svc, "g1", AgentId::Grok, "Zed", false);
    seed(&svc, "c1", AgentId::Claude, "Beta", true);
    seed(&svc, "k1", AgentId::Kimi, "Alpha", false);
    seed(&svc, "x1", AgentId::Codex, "Alpha", false);
    seed(&svc, "c0", AgentId::Claude, "Alpha", false);

    let all = svc.list(None).unwrap();
    let keys: Vec<(&str, &str)> = all
        .iter()
        .map(|p| (p.agent_id.as_str(), p.name.as_str()))
        .collect();
    assert_eq!(
        keys,
        vec![
            ("claude", "Alpha"),
            ("claude", "Beta"),
            ("codex", "Alpha"),
            ("kimi", "Alpha"),
            ("grok", "Zed"),
        ]
    );

    let claude = svc.list(Some(AgentId::Claude)).unwrap();
    assert_eq!(claude.len(), 2);
    assert_eq!(claude[0].id, "c0");
    assert_eq!(claude[1].id, "c1");
}

#[test]
fn get_by_id_and_name_and_errors() {
    let (_dir, svc) = svc();
    seed(&svc, "id-claude", AgentId::Claude, "Shared", true);
    seed(&svc, "id-codex", AgentId::Codex, "Shared", false);
    seed(&svc, "id-unique", AgentId::Grok, "OnlyGrok", false);

    let by_id = svc.get("id-claude", None).unwrap();
    assert_eq!(by_id.name, "Shared");
    assert!(by_id.is_current);

    let by_name = svc.get("OnlyGrok", None).unwrap();
    assert_eq!(by_name.id, "id-unique");

    // Agent filter disambiguates name.
    let scoped = svc.get("Shared", Some(AgentId::Codex)).unwrap();
    assert_eq!(scoped.id, "id-codex");

    // Ambiguous without agent.
    let err = svc.get("Shared", None).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(err.to_string().contains("ambiguous"));

    // Missing.
    let err = svc.get("nope", None).unwrap_err();
    assert_eq!(err.code(), "not_found");

    // Id exists but wrong agent filter → not found.
    let err = svc.get("id-claude", Some(AgentId::Grok)).unwrap_err();
    assert_eq!(err.code(), "not_found");

    let err = svc.get("   ", None).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn get_prefers_id_over_name_collision() {
    let (_dir, svc) = svc();
    // One row's id equals another row's name.
    seed(&svc, "collision", AgentId::Claude, "Other", false);
    seed(&svc, "other-id", AgentId::Codex, "collision", false);

    let p = svc.get("collision", None).unwrap();
    assert_eq!(p.id, "collision");
    assert_eq!(p.agent_id, AgentId::Claude);
}

#[test]
fn validation_rejects_empty_whitespace_overlong_control_and_non_objects() {
    let (_dir, svc) = svc();

    let cases: Vec<ProviderInput> = vec![
        {
            let mut i = input("ok", AgentId::Claude, "N", false);
            i.id = String::new();
            i
        },
        {
            let mut i = input("ok", AgentId::Claude, "N", false);
            i.id = "  padded  ".into();
            i
        },
        {
            let mut i = input("ok", AgentId::Claude, "N", false);
            i.id = "a".repeat(MAX_PROVIDER_ID_LEN + 1);
            i
        },
        {
            let mut i = input("ok", AgentId::Claude, "N", false);
            i.id = "bad\nid".into();
            i
        },
        {
            let mut i = input("ok", AgentId::Claude, "N", false);
            i.name = String::new();
            i
        },
        {
            let mut i = input("ok", AgentId::Claude, "N", false);
            i.name = " leading".into();
            i
        },
        {
            let mut i = input("ok", AgentId::Claude, "N", false);
            i.name = "trail ".into();
            i
        },
        {
            let mut i = input("ok", AgentId::Claude, "N", false);
            i.name = "n".repeat(MAX_PROVIDER_NAME_LEN + 1);
            i
        },
        {
            let mut i = input("ok", AgentId::Claude, "N", false);
            i.name = "no\u{0007}pe".into();
            i
        },
        {
            let mut i = input("ok", AgentId::Claude, "N", false);
            i.settings_config = json!(["not", "object"]);
            i
        },
        {
            let mut i = input("ok", AgentId::Claude, "N", false);
            i.settings_config = json!("string");
            i
        },
        {
            let mut i = input("ok", AgentId::Claude, "N", false);
            i.meta = json!(null);
            i
        },
        {
            let mut i = input("ok", AgentId::Claude, "N", false);
            i.meta = json!(42);
            i
        },
    ];

    for (idx, case) in cases.iter().enumerate() {
        let err = svc.create(case).unwrap_err();
        assert_eq!(err.code(), "invalid_arg", "create case {idx}: {err}");
        let err = svc.update(case).unwrap_err();
        assert_eq!(err.code(), "invalid_arg", "update case {idx}: {err}");
        let err = svc.upsert(case).unwrap_err();
        assert_eq!(err.code(), "invalid_arg", "upsert case {idx}: {err}");
    }

    // No rows written by failed creates.
    assert!(svc.list(None).unwrap().is_empty());

    // delete validates id
    assert_eq!(
        svc.delete("", AgentId::Claude).unwrap_err().code(),
        "invalid_arg"
    );
    assert_eq!(
        svc.delete("  x  ", AgentId::Claude).unwrap_err().code(),
        "invalid_arg"
    );
    assert_eq!(
        svc.delete(&"a".repeat(MAX_PROVIDER_ID_LEN + 1), AgentId::Claude)
            .unwrap_err()
            .code(),
        "invalid_arg"
    );
    assert_eq!(
        svc.delete("a\tb", AgentId::Claude).unwrap_err().code(),
        "invalid_arg"
    );
}

#[test]
fn create_update_upsert_delete_crud_and_errors() {
    let (_dir, svc) = svc();

    let created = svc
        .create(&input("p1", AgentId::Claude, "Alpha", false))
        .unwrap();
    assert_eq!(created.id, "p1");
    assert_eq!(created.name, "Alpha");
    assert!(!created.created_at.is_empty());
    assert_eq!(created.created_at, created.updated_at);
    // Secrets remain unredacted at service boundary (CLI redacts).
    assert_eq!(created.settings_config["api_key"], "sk-live-secret");

    // Duplicate create.
    let err = svc
        .create(&input("p1", AgentId::Claude, "Other", false))
        .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert_eq!(svc.get("p1", None).unwrap().name, "Alpha");

    // Update happy path.
    let mut upd = input("p1", AgentId::Claude, "Alpha2", true);
    upd.settings_config = json!({"api_key": "sk-new", "base_url": "https://y"});
    upd.meta = json!({"note": "updated"});
    let updated = svc.update(&upd).unwrap();
    assert_eq!(updated.name, "Alpha2");
    assert!(updated.is_current);
    assert_eq!(updated.created_at, created.created_at);
    assert!(updated.updated_at >= created.updated_at);
    assert_eq!(updated.settings_config["api_key"], "sk-new");

    // Update missing.
    let err = svc
        .update(&input("missing", AgentId::Claude, "X", false))
        .unwrap_err();
    assert_eq!(err.code(), "not_found");

    // Upsert existing preserves created_at.
    let up = input("p1", AgentId::Claude, "Alpha3", false);
    let upserted = svc.upsert(&up).unwrap();
    assert_eq!(upserted.name, "Alpha3");
    assert_eq!(upserted.created_at, created.created_at);
    assert!(!upserted.is_current);

    // Upsert insert path.
    let inserted = svc
        .upsert(&input("p2", AgentId::Codex, "Beta", true))
        .unwrap();
    assert_eq!(inserted.id, "p2");
    assert!(inserted.is_current);

    // Delete.
    svc.delete("p2", AgentId::Codex).unwrap();
    assert_eq!(svc.get("p2", None).unwrap_err().code(), "not_found");
    let err = svc.delete("p2", AgentId::Codex).unwrap_err();
    assert_eq!(err.code(), "not_found");

    // p1 still present.
    assert_eq!(svc.list(None).unwrap().len(), 1);
}

#[test]
fn update_upsert_reject_agent_id_change_without_mutation() {
    let (_dir, svc) = svc();
    let created = svc
        .create(&input("p1", AgentId::Claude, "A", true))
        .unwrap();

    let bad = input("p1", AgentId::Grok, "Hijack", true);
    assert_eq!(svc.update(&bad).unwrap_err().code(), "invalid_arg");
    assert_eq!(svc.upsert(&bad).unwrap_err().code(), "invalid_arg");

    let stored = svc.get("p1", None).unwrap();
    assert_eq!(stored.agent_id, AgentId::Claude);
    assert_eq!(stored.name, "A");
    assert!(stored.is_current);
    assert_eq!(stored.created_at, created.created_at);
    assert_eq!(stored.updated_at, created.updated_at);
}

#[test]
fn is_current_uniqueness_and_cross_agent_independence() {
    let (_dir, svc) = svc();
    svc.create(&input("c1", AgentId::Claude, "One", true))
        .unwrap();
    svc.create(&input("c2", AgentId::Claude, "Two", false))
        .unwrap();
    svc.create(&input("x1", AgentId::Codex, "X", true)).unwrap();

    assert!(svc.get("c1", None).unwrap().is_current);
    assert!(!svc.get("c2", None).unwrap().is_current);
    assert!(svc.get("x1", None).unwrap().is_current);

    svc.update(&input("c2", AgentId::Claude, "Two", true))
        .unwrap();
    assert!(!svc.get("c1", None).unwrap().is_current);
    assert!(svc.get("c2", None).unwrap().is_current);
    assert!(svc.get("x1", None).unwrap().is_current);

    svc.upsert(&input("c3", AgentId::Claude, "Three", true))
        .unwrap();
    let claude_currents: Vec<_> = svc
        .list(Some(AgentId::Claude))
        .unwrap()
        .into_iter()
        .filter(|p| p.is_current)
        .map(|p| p.id)
        .collect();
    assert_eq!(claude_currents, vec!["c3".to_string()]);
    assert!(svc.get("x1", None).unwrap().is_current);
}

#[test]
fn failed_writes_do_not_mutate_existing_rows() {
    let (_dir, svc) = svc();
    let original = svc
        .create(&input("p1", AgentId::Claude, "Keep", true))
        .unwrap();

    // Invalid create (bad name) leaves table as-is.
    let mut bad = input("p2", AgentId::Claude, "x", true);
    bad.name = " bad".into();
    assert_eq!(svc.create(&bad).unwrap_err().code(), "invalid_arg");
    assert_eq!(svc.list(None).unwrap().len(), 1);

    // Duplicate create does not overwrite.
    let mut dup = input("p1", AgentId::Claude, "Overwrite", false);
    dup.settings_config = json!({"api_key": "other"});
    assert_eq!(svc.create(&dup).unwrap_err().code(), "invalid_arg");
    let stored = svc.get("p1", None).unwrap();
    assert_eq!(stored.name, "Keep");
    assert_eq!(stored.settings_config["api_key"], "sk-live-secret");
    assert!(stored.is_current);
    assert_eq!(stored.updated_at, original.updated_at);

    // Invalid update rejected before/without storage change.
    let mut bad_upd = input("p1", AgentId::Claude, "Keep", false);
    bad_upd.meta = json!([]);
    assert_eq!(svc.update(&bad_upd).unwrap_err().code(), "invalid_arg");
    let stored = svc.get("p1", None).unwrap();
    assert!(stored.is_current);
    assert_eq!(stored.meta["note"], "n");
}

#[test]
fn boundary_lengths_accepted() {
    let (_dir, svc) = svc();
    let id = "i".repeat(MAX_PROVIDER_ID_LEN);
    let name = "n".repeat(MAX_PROVIDER_NAME_LEN);
    let p = svc
        .create(&input(&id, AgentId::Kimi, &name, false))
        .unwrap();
    assert_eq!(p.id.chars().count(), MAX_PROVIDER_ID_LEN);
    assert_eq!(p.name.chars().count(), MAX_PROVIDER_NAME_LEN);
    // Empty objects are valid.
    let mut empty_obj = input("empty-json", AgentId::Kimi, "E", false);
    empty_obj.settings_config = json!({});
    empty_obj.meta = json!({});
    svc.create(&empty_obj).unwrap();
}

#[test]
fn delete_is_agent_scoped() {
    let (_dir, svc) = svc();
    svc.create(&input("p1", AgentId::Claude, "One", true))
        .unwrap();

    let error = svc.delete("p1", AgentId::Codex).unwrap_err();
    assert_eq!(error.code(), "not_found");
    assert!(svc.get("p1", None).unwrap().is_current);

    svc.delete("p1", AgentId::Claude).unwrap();
    assert_eq!(svc.get("p1", None).unwrap_err().code(), "not_found");
}

#[test]
fn import_live_preserves_full_secrets_and_marks_new_row_current() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://relay.example.com",
                "ANTHROPIC_AUTH_TOKEN": "live-secret"
            },
            "long": "x".repeat(1_000)
        }),
    };
    let (_root, _db, svc, _adapter, _backups) = live_svc(AgentId::Claude, live.clone());
    svc.create(&input("old", AgentId::Claude, "Old", true))
        .unwrap();

    let imported = svc
        .import_live(AgentId::Claude, Some("Imported live"))
        .unwrap();
    assert_eq!(imported.agent_id, AgentId::Claude);
    assert_eq!(imported.name, "Imported live");
    assert!(imported.id.starts_with("claude-live-"));
    assert!(imported.is_current);
    assert_eq!(imported.settings_config, live.raw);
    assert_eq!(imported.meta, json!({"source": "live"}));
    assert_eq!(
        imported.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "live-secret"
    );
    assert_eq!(
        imported.settings_config["long"].as_str().unwrap().len(),
        1_000
    );
    assert_eq!(
        imported.redacted().settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "***"
    );
    assert!(!svc.get("old", None).unwrap().is_current);
}

#[test]
fn import_live_rejects_empty_or_wrong_agent_without_rows() {
    let empty = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({}),
    };
    let (_root, _db, svc, _adapter, _backups) = live_svc(AgentId::Claude, empty);
    assert_eq!(
        svc.import_live(AgentId::Claude, None).unwrap_err().code(),
        "not_found"
    );
    assert!(svc.list(None).unwrap().is_empty());

    let mismatch = AgentConfig {
        agent: AgentId::Codex,
        raw: json!({"format": "toml", "content": "model = 'x'"}),
    };
    let (_root, _db, svc, _adapter, _backups) = live_svc(AgentId::Claude, mismatch);
    assert_eq!(
        svc.import_live(AgentId::Claude, None).unwrap_err().code(),
        "invalid_arg"
    );
    assert!(svc.list(None).unwrap().is_empty());
}

#[test]
fn switch_backfills_snapshots_writes_and_selects_transactionally() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "manual-live-secret"}}),
    };
    let live_bytes = serde_json::to_vec(&live).unwrap();
    let (_root, _db, svc, adapter, _backups) = live_svc(AgentId::Claude, live.clone());

    svc.create(&input("c1", AgentId::Claude, "Current", true))
        .unwrap();
    let mut target = input("c2", AgentId::Claude, "Target", false);
    target.settings_config = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "target-secret"}});
    svc.create(&target).unwrap();
    svc.create(&input("x1", AgentId::Codex, "Other agent", true))
        .unwrap();

    let result = svc.switch("Target", AgentId::Claude).unwrap();
    assert_eq!(result.provider.id, "c2");
    assert!(result.provider.is_current);
    assert_eq!(result.backfilled_provider_id.as_deref(), Some("c1"));
    let snapshot = result
        .backup
        .as_ref()
        .expect("existing live config was snapshotted");
    assert_eq!(snapshot.kind, BackupKind::AutoSwitch);
    assert_eq!(snapshot.agent_id, Some(AgentId::Claude));
    assert_eq!(snapshot.files, vec!["config.json"]);
    assert_eq!(
        std::fs::read(PathBuf::from(&snapshot.path).join("config.json")).unwrap(),
        live_bytes
    );

    assert_eq!(adapter.config().raw, target.settings_config);
    assert_eq!(adapter.write_attempts.load(Ordering::SeqCst), 1);
    let old = svc.get("c1", None).unwrap();
    assert!(!old.is_current);
    assert_eq!(old.settings_config, live.raw);
    assert!(svc.get("c2", None).unwrap().is_current);
    assert!(svc.get("x1", None).unwrap().is_current);
    assert_eq!(
        result.redacted().provider.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "***"
    );
}

#[test]
fn switching_already_current_provider_keeps_backfilled_live_value() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "manual-change"}}),
    };
    let (_root, _db, svc, adapter, _backups) = live_svc(AgentId::Claude, live.clone());
    let mut stale = input("c1", AgentId::Claude, "Current", true);
    stale.settings_config = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "stale"}});
    svc.create(&stale).unwrap();

    let result = svc.switch("c1", AgentId::Claude).unwrap();
    assert_eq!(result.backfilled_provider_id.as_deref(), Some("c1"));
    assert_eq!(result.provider.settings_config, live.raw);
    assert_eq!(adapter.config(), live);
}

#[test]
fn failed_live_write_leaves_db_and_live_unchanged_and_releases_lock() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "before"}}),
    };
    let (_root, _db, svc, adapter, backups_root) = live_svc(AgentId::Claude, live.clone());
    let before = svc
        .create(&input("c1", AgentId::Claude, "Current", true))
        .unwrap();
    svc.create(&input("c2", AgentId::Claude, "Target", false))
        .unwrap();
    adapter.fail_on_write(1);

    let error = svc.switch("c2", AgentId::Claude).unwrap_err();
    assert_eq!(error.code(), "test.write");
    assert_eq!(adapter.write_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(adapter.config(), live);
    assert_eq!(svc.get("c1", None).unwrap(), before);
    assert!(!svc.get("c2", None).unwrap().is_current);
    assert_eq!(
        svc.backup.as_ref().unwrap().list(None).unwrap().len(),
        1,
        "pre-write snapshot remains available after a failed write"
    );
    assert!(!backups_root
        .parent()
        .unwrap()
        .join("locks")
        .join("provider-claude.lock")
        .exists());
}

#[test]
fn database_failure_restores_live_and_rolls_back_backfill() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "before"}}),
    };
    let (_root, db, svc, adapter, _backups) = live_svc(AgentId::Claude, live.clone());
    let before = svc
        .create(&input("c1", AgentId::Claude, "Current", true))
        .unwrap();
    let mut target = input("c2", AgentId::Claude, "Target", false);
    target.settings_config = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "after"}});
    svc.create(&target).unwrap();
    db.with_conn(|conn| {
        conn.execute_batch(
            r#"
            CREATE TRIGGER fail_provider_service_switch
            BEFORE UPDATE OF is_current ON providers
            WHEN NEW.id = 'c2' AND NEW.is_current = 1
            BEGIN
                SELECT RAISE(ABORT, 'injected service switch failure');
            END;
            "#,
        )?;
        Ok(())
    })
    .unwrap();

    let error = svc.switch("c2", AgentId::Claude).unwrap_err();
    assert_eq!(error.code(), "db");
    assert_eq!(adapter.write_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(adapter.config(), live);
    assert_eq!(svc.get("c1", None).unwrap(), before);
    assert!(!svc.get("c2", None).unwrap().is_current);
}

#[test]
fn switch_fails_closed_when_live_dependencies_or_lock_are_unavailable() {
    let (_dir, svc) = svc();
    svc.create(&input("c1", AgentId::Claude, "Current", true))
        .unwrap();
    assert_eq!(
        svc.switch("c1", AgentId::Claude).unwrap_err().code(),
        "unsupported"
    );

    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {}}),
    };
    let (_root, _db, svc, adapter, backups_root) = live_svc(AgentId::Claude, live);
    svc.create(&input("c1", AgentId::Claude, "Current", true))
        .unwrap();
    let lock_dir = backups_root.parent().unwrap().join("locks");
    std::fs::create_dir_all(&lock_dir).unwrap();
    std::fs::write(lock_dir.join("provider-claude.lock"), b"held").unwrap();

    let error = svc.switch("c1", AgentId::Claude).unwrap_err();
    assert_eq!(error.code(), "provider.lock");
    assert_eq!(adapter.write_attempts.load(Ordering::SeqCst), 0);
    assert!(svc.backup.as_ref().unwrap().list(None).unwrap().is_empty());
}

#[test]
fn backup_failure_rolls_back_backfill_without_touching_live() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "manual-live"}}),
    };
    let (_root, _db, svc, adapter, backups_root) = live_svc(AgentId::Claude, live.clone());
    let before = svc
        .create(&input("c1", AgentId::Claude, "Current", true))
        .unwrap();
    svc.create(&input("c2", AgentId::Claude, "Target", false))
        .unwrap();
    std::fs::write(&backups_root, b"blocks snapshot directory").unwrap();

    let error = svc.switch("c2", AgentId::Claude).unwrap_err();
    assert_eq!(error.code(), "io");
    assert_eq!(adapter.write_attempts.load(Ordering::SeqCst), 0);
    assert_eq!(adapter.config(), live);
    assert_eq!(svc.get("c1", None).unwrap(), before);
    assert!(!svc.get("c2", None).unwrap().is_current);
}

#[test]
fn switch_without_live_file_has_no_backup_or_backfill() {
    let empty = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({}),
    };
    let (_root, _db, svc, adapter, _backups) = live_svc(AgentId::Claude, empty);
    let mut target = input("c2", AgentId::Claude, "Target", false);
    target.settings_config = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "target"}});
    svc.create(&target).unwrap();

    let result = svc.switch("c2", AgentId::Claude).unwrap();
    assert!(result.backup.is_none());
    assert!(result.backfilled_provider_id.is_none());
    assert!(result.provider.is_current);
    assert_eq!(adapter.config().raw, target.settings_config);
}

#[test]
fn import_live_uses_same_agent_lock_and_unregistered_agents_fail_closed() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "live"}}),
    };
    let (_root, _db, svc, _adapter, backups_root) = live_svc(AgentId::Claude, live);
    let lock_dir = backups_root.parent().unwrap().join("locks");
    std::fs::create_dir_all(&lock_dir).unwrap();
    std::fs::write(lock_dir.join("provider-claude.lock"), b"held").unwrap();
    assert_eq!(
        svc.import_live(AgentId::Claude, None).unwrap_err().code(),
        "provider.lock"
    );
    assert!(svc.list(None).unwrap().is_empty());

    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("ah.db")).unwrap();
    let unregistered =
        ProviderService::with_live(db, AdapterRegistry::new(), dir.path().join("backups"));
    unregistered
        .create(&input("g1", AgentId::Grok, "Target", false))
        .unwrap();
    assert_eq!(
        unregistered
            .import_live(AgentId::Grok, None)
            .unwrap_err()
            .code(),
        "not_found"
    );
    assert_eq!(
        unregistered.switch("g1", AgentId::Grok).unwrap_err().code(),
        "not_found"
    );
}

#[test]
fn unsupported_apply_attempts_live_and_db_compensation() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "before"}}),
    };
    let (_root, _db, svc, adapter, _backups) = live_svc(AgentId::Claude, live.clone());
    let before = svc
        .create(&input("c1", AgentId::Claude, "Current", true))
        .unwrap();
    svc.create(&input("c2", AgentId::Claude, "Target", false))
        .unwrap();
    adapter.make_write_unsupported();

    let error = svc.switch("c2", AgentId::Claude).unwrap_err();
    assert_eq!(error.code(), "provider.switch.rollback");
    assert_eq!(adapter.write_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(adapter.config(), live);
    assert_eq!(svc.get("c1", None).unwrap(), before);
    assert!(!svc.get("c2", None).unwrap().is_current);
    assert!(!error.to_string().contains("ANTHROPIC_AUTH_TOKEN"));
}

fn lock_path(dir: &Path, agent: AgentId) -> PathBuf {
    dir.join(format!("provider-{}.lock", agent.as_str()))
}

fn write_lock_fixture(path: &Path, pid: u32, created_unix_ms: u64, token: &str) {
    std::fs::write(
        path,
        format!("pid={pid}\ncreated_unix_ms={created_unix_ms}\ntoken={token}\n"),
    )
    .unwrap();
}

#[test]
fn lock_owner_parse_roundtrip_and_rejects_malformed() {
    let owner = LockOwner {
        pid: 4242,
        created_unix_ms: 1_700_000_000_000,
        token: "tok-abc".into(),
    };
    let parsed = LockOwner::parse(&owner.serialize()).unwrap();
    assert_eq!(parsed, owner);

    assert!(LockOwner::parse("held").is_none());
    assert!(LockOwner::parse("pid=1\ncreated_unix_ms=2\n").is_none());
    assert!(LockOwner::parse("pid=x\ncreated_unix_ms=1\ntoken=t\n").is_none());
    assert!(LockOwner::parse("pid=1\ncreated_unix_ms=nope\ntoken=t\n").is_none());
    assert!(LockOwner::parse("pid=1\ncreated_unix_ms=1\ntoken=\n").is_none());
}

#[test]
fn stale_lock_reclaimed_when_owner_pid_is_dead() {
    let dir = tempdir().unwrap();
    let path = lock_path(dir.path(), AgentId::Claude);
    // pid 0 is never a live owner in our probe.
    write_lock_fixture(&path, 0, unix_now_ms(), "dead-owner");

    let lock = ProviderSwitchLock::acquire(dir.path(), AgentId::Claude).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    let owner = LockOwner::parse(&raw).unwrap();
    assert_eq!(owner.pid, std::process::id());
    assert_ne!(owner.token, "dead-owner");
    drop(lock);
    assert!(!path.exists());
}

#[test]
fn stale_lock_reclaimed_when_ttl_exceeded() {
    let dir = tempdir().unwrap();
    let path = lock_path(dir.path(), AgentId::Codex);
    // Current process is alive, but created far beyond the conservative TTL.
    let ancient = unix_now_ms().saturating_sub(PROVIDER_LOCK_TTL.as_millis() as u64 + 60_000);
    write_lock_fixture(&path, std::process::id(), ancient, "expired-owner");

    let lock = ProviderSwitchLock::acquire(dir.path(), AgentId::Codex).unwrap();
    let owner = LockOwner::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(owner.pid, std::process::id());
    assert_ne!(owner.token, "expired-owner");
    drop(lock);
    assert!(!path.exists());
}

#[test]
fn active_lock_with_live_owner_is_still_rejected() {
    let dir = tempdir().unwrap();
    let path = lock_path(dir.path(), AgentId::Grok);
    write_lock_fixture(&path, std::process::id(), unix_now_ms(), "live-owner-token");

    let err = ProviderSwitchLock::acquire(dir.path(), AgentId::Grok).unwrap_err();
    assert_eq!(err.code(), "provider.lock");
    // Fixture must remain untouched.
    let owner = LockOwner::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(owner.token, "live-owner-token");
}

#[test]
fn old_guard_drop_does_not_remove_new_owner_lock() {
    let dir = tempdir().unwrap();
    let path = lock_path(dir.path(), AgentId::Kimi);

    let mut old_guard = ProviderSwitchLock::acquire(dir.path(), AgentId::Kimi).unwrap();
    let old_token = old_guard.token.clone();

    // Simulate another process replacing the lock: close our handle (Windows
    // cannot replace an open file) without running Drop's ownership check,
    // then write a new owner record under the same path.
    drop(old_guard.file.take());
    write_lock_fixture(
        &path,
        std::process::id(),
        unix_now_ms(),
        "replacement-owner",
    );

    // Drop must not delete the replacement owner's lock.
    drop(old_guard);
    assert!(path.exists());
    let owner = LockOwner::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(owner.token, "replacement-owner");
    assert_ne!(owner.token, old_token);
}

#[test]
fn malformed_lock_is_fail_closed_and_not_reclaimed() {
    let dir = tempdir().unwrap();
    let path = lock_path(dir.path(), AgentId::Claude);
    std::fs::write(&path, b"held").unwrap();

    let err = ProviderSwitchLock::acquire(dir.path(), AgentId::Claude).unwrap_err();
    assert_eq!(err.code(), "provider.lock");
    assert_eq!(std::fs::read(&path).unwrap(), b"held");

    // Partial metadata also fails closed.
    std::fs::write(&path, b"pid=1\ntoken=only\n").unwrap();
    let err = ProviderSwitchLock::acquire(dir.path(), AgentId::Claude).unwrap_err();
    assert_eq!(err.code(), "provider.lock");
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains("token=only"));
}

#[test]
fn concurrent_guards_mutually_exclude_same_agent() {
    let dir = tempdir().unwrap();
    let first = ProviderSwitchLock::acquire(dir.path(), AgentId::Claude).unwrap();
    let err = ProviderSwitchLock::acquire(dir.path(), AgentId::Claude).unwrap_err();
    assert_eq!(err.code(), "provider.lock");
    // Different agents do not share a lock file.
    let other = ProviderSwitchLock::acquire(dir.path(), AgentId::Codex).unwrap();
    drop(first);
    // After release, same agent can acquire again.
    let second = ProviderSwitchLock::acquire(dir.path(), AgentId::Claude).unwrap();
    drop(other);
    drop(second);
}
