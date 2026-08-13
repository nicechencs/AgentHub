use super::*;
use crate::adapters::AgentAdapter;
use crate::models::{
    AuthState, Capability, CapabilityState, DetectResult, DetectStatus, InstallChannel, RunOptions,
    RunSpec,
};
use serde_json::json;
use std::path::{Path, PathBuf};
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

    // Re-importing the unchanged live snapshot is idempotent: it reuses the
    // same canonical live row instead of creating another UUID row.
    let imported_again = svc
        .import_live(AgentId::Claude, Some("Imported live"))
        .unwrap();
    assert_eq!(imported_again.id, imported.id);
    assert_eq!(svc.list(Some(AgentId::Claude)).unwrap().len(), 2);
}

#[test]
fn import_live_updates_existing_live_row_but_never_manual_provider() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "before"}}),
    };
    let (_root, _db, svc, adapter, _backups) = live_svc(AgentId::Claude, live.clone());

    let imported = svc
        .import_live(AgentId::Claude, Some("Live snapshot"))
        .unwrap();
    let manual = input("manual", AgentId::Claude, "Manual", false);
    svc.create(&manual).unwrap();

    let changed = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "after"}});
    *adapter.config.lock().unwrap() = AgentConfig {
        agent: AgentId::Claude,
        raw: changed.clone(),
    };

    let refreshed = svc.import_live(AgentId::Claude, None).unwrap();
    assert_eq!(refreshed.id, imported.id);
    assert_eq!(refreshed.settings_config, changed);
    assert_eq!(refreshed.name, "Live snapshot");
    assert!(refreshed.is_current);

    let manual_after = svc.get("manual", Some(AgentId::Claude)).unwrap();
    assert_eq!(manual_after.settings_config, manual.settings_config);
    assert!(!manual_after.is_current);
    assert_eq!(svc.list(Some(AgentId::Claude)).unwrap().len(), 2);
}

#[test]
fn import_live_change_and_restore_roundtrip_keeps_canonical_live_row() {
    let config_a = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({
            "models": [{"id": "a", "apiKey": "secret-a"}],
            "settings": {"endpoint": "https://a.example"}
        }),
    };
    let (_root, _db, svc, adapter, _backups) = live_svc(AgentId::Claude, config_a.clone());
    let imported_a = svc.import_live(AgentId::Claude, Some("Live A")).unwrap();

    let config_b = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({
            "models": [{"id": "b", "apiKey": "secret-b"}],
            "settings": {"endpoint": "https://b.example"}
        }),
    };
    *adapter.config.lock().unwrap() = config_b.clone();
    let imported_b = svc.import_live(AgentId::Claude, None).unwrap();
    assert_eq!(imported_b.id, imported_a.id);
    assert_eq!(imported_b.settings_config, config_b.raw);

    *adapter.config.lock().unwrap() = config_a.clone();
    let restored_a = svc.import_live(AgentId::Claude, None).unwrap();
    assert_eq!(restored_a.id, imported_a.id);
    assert_eq!(restored_a.settings_config, config_a.raw);
    assert_eq!(restored_a.name, "Live A");
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
fn live_config_snapshot_restores_exact_config_without_serializing_it() {
    let original = AgentConfig {
        agent: AgentId::Codex,
        raw: json!({"format": "toml", "content": "api_key = 'original-secret'"}),
    };
    let (_root, _db, service, adapter, _backups) = live_svc(AgentId::Codex, original.clone());
    let snapshot = service
        .capture_live_config_snapshot(AgentId::Codex)
        .unwrap();
    adapter
        .write_config(&AgentConfig {
            agent: AgentId::Codex,
            raw: json!({"format": "toml", "content": "api_key = 'bridge-secret'"}),
        })
        .unwrap();

    service.restore_live_config_snapshot(&snapshot).unwrap();
    assert_eq!(adapter.config(), original);
    assert!(!format!("{snapshot:?}").contains("original-secret"));
}

#[test]
fn switching_away_from_reference_provider_scrubs_backfill_before_db_write() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.kimi.com/coding/",
                "ANTHROPIC_AUTH_TOKEN": "live-kimi-secret"
            }
        }),
    };
    let (_root, _db, svc, adapter, _backups) = live_svc(AgentId::Claude, live);
    let mut source = input("kimi-source", AgentId::Kimi, "Kimi membership", false);
    source.settings_config = json!({"apiKey": "source-kimi-secret"});
    source.meta = json!({"preset": "kimi-code-membership"});
    svc.create(&source).unwrap();

    let mut reference = input("claude-reference", AgentId::Claude, "Generated", true);
    reference.settings_config = json!({"env": {
        "ANTHROPIC_BASE_URL": "https://api.kimi.com/coding/",
        "ANTHROPIC_AUTH_TOKEN": "$AGENTHUB_CONNECTION_SECRET$"
    }});
    reference.meta = json!({
        "generatedBy": "adapter",
        "adapterRuleId": "kimi-membership-to-claude-v1",
        "adapterRuleVersion": 1,
        "adapterSecretMode": "source_reference",
        "adapterSourceRef": {"kind": "provider", "id": "kimi-source"}
    });
    svc.create(&reference).unwrap();
    let mut target = input("claude-manual", AgentId::Claude, "Manual", false);
    target.settings_config = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "manual-target-secret"}});
    svc.create(&target).unwrap();

    svc.switch("claude-manual", AgentId::Claude).unwrap();
    let stored_reference = svc.get("claude-reference", Some(AgentId::Claude)).unwrap();
    let stored = serde_json::to_string(&stored_reference.settings_config).unwrap();
    assert!(!stored.contains("live-kimi-secret"));
    assert_eq!(
        stored_reference.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "$AGENTHUB_CONNECTION_SECRET$"
    );
    assert_eq!(
        stored_reference.settings_config["env"]["ANTHROPIC_BASE_URL"],
        "https://api.kimi.com/coding/"
    );
    assert_eq!(adapter.config().raw, target.settings_config);
}

#[test]
fn selecting_current_reference_refreshes_secret_from_source() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {
            "ANTHROPIC_BASE_URL": "https://api.kimi.com/coding/",
            "ANTHROPIC_AUTH_TOKEN": "old-live-secret"
        }}),
    };
    let (_root, _db, svc, adapter, _backups) = live_svc(AgentId::Claude, live);
    let mut source = input("kimi-source", AgentId::Kimi, "Kimi membership", false);
    source.settings_config = json!({"apiKey": "rotated-source-secret"});
    source.meta = json!({"preset": "kimi-code-membership"});
    svc.create(&source).unwrap();

    let mut reference = input("claude-reference", AgentId::Claude, "Generated", true);
    reference.settings_config = json!({"env": {
        "ANTHROPIC_BASE_URL": "https://api.kimi.com/coding/",
        "ANTHROPIC_AUTH_TOKEN": "$AGENTHUB_CONNECTION_SECRET$"
    }});
    reference.meta = json!({
        "generatedBy": "adapter",
        "adapterRuleId": "kimi-membership-to-claude-v1",
        "adapterRuleVersion": 1,
        "adapterSecretMode": "source_reference",
        "adapterSourceRef": {"kind": "provider", "id": "kimi-source"}
    });
    svc.create(&reference).unwrap();

    let result = svc.switch("claude-reference", AgentId::Claude).unwrap();
    assert_eq!(
        adapter.config().raw["env"]["ANTHROPIC_AUTH_TOKEN"],
        "rotated-source-secret"
    );
    assert_eq!(
        result.provider.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "$AGENTHUB_CONNECTION_SECRET$"
    );
    assert!(!serde_json::to_string(&result.provider.settings_config)
        .unwrap()
        .contains("old-live-secret"));
}

#[test]
fn codex_local_token_adapter_provider_create_update_switch_and_readback_never_use_upstream_key() {
    let upstream_key = "kimi-upstream-key-must-not-reach-codex";
    let initial = AgentConfig {
        agent: AgentId::Codex,
        raw: json!({}),
    };
    let (_root, _db, svc, adapter, _backups) = live_svc(AgentId::Codex, initial);
    let mut bridge = input("codex-kimi-bridge", AgentId::Codex, "Kimi Bridge", false);
    bridge.settings_config = json!({
        "format": "toml",
        "content": "model_provider = \"agenthub_kimi_bridge\"\n[model_providers.agenthub_kimi_bridge]\nbase_url = \"http://127.0.0.1:43121/v1\"\nwire_api = \"responses\"\n",
        "auth": { "OPENAI_API_KEY": "local-bridge-token-v1" },
    });
    bridge.meta = json!({
        "preset": "openai-compatible",
        "generatedBy": "adapter",
        "adapterRuleId": "kimi-membership-to-codex-v1",
        "adapterRuleVersion": 1,
        "adapterSecretMode": "local_token",
        "adapterProfileId": "bridge-profile",
        "adapterSourceRef": {"kind": "provider", "id": "kimi-source"},
    });
    svc.create(&bridge).unwrap();

    let first = svc.switch("codex-kimi-bridge", AgentId::Codex).unwrap();
    assert_eq!(
        adapter.config().raw["auth"]["OPENAI_API_KEY"],
        "local-bridge-token-v1"
    );
    assert!(!serde_json::to_string(&adapter.config().raw)
        .unwrap()
        .contains(upstream_key));
    assert!(!serde_json::to_string(&first.provider)
        .unwrap()
        .contains(upstream_key));

    bridge.settings_config["content"] = json!("model_provider = \"agenthub_kimi_bridge\"\n[model_providers.agenthub_kimi_bridge]\nbase_url = \"http://127.0.0.1:43122/v1\"\nwire_api = \"responses\"\n");
    bridge.settings_config["auth"]["OPENAI_API_KEY"] = json!("local-bridge-token-v2");
    svc.update(&bridge).unwrap();
    let second = svc.switch("codex-kimi-bridge", AgentId::Codex).unwrap();
    let readback = svc.get("codex-kimi-bridge", Some(AgentId::Codex)).unwrap();
    assert_eq!(
        adapter.config().raw["auth"]["OPENAI_API_KEY"],
        "local-bridge-token-v2"
    );
    assert!(adapter.config().raw["content"]
        .as_str()
        .unwrap()
        .contains("43122"));
    assert_eq!(
        readback.settings_config["auth"]["OPENAI_API_KEY"],
        "local-bridge-token-v2"
    );
    for value in [
        serde_json::to_string(&second.provider).unwrap(),
        serde_json::to_string(&readback).unwrap(),
        serde_json::to_string(&adapter.config().raw).unwrap(),
    ] {
        assert!(!value.contains(upstream_key));
    }
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
fn saga_guard_reuses_the_live_lock_and_rejects_the_wrong_agent() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({}),
    };
    let (_root, db, svc, adapter, backups_root) = live_svc(AgentId::Claude, live.clone());
    svc.create(&input("c1", AgentId::Claude, "Target", false))
        .unwrap();

    let guard = svc.begin_live_saga(AgentId::Claude).unwrap();
    assert_eq!(guard.agent(), AgentId::Claude);
    for result in [
        svc.create(&input("c2", AgentId::Claude, "Blocked create", false))
            .map(|_| ()),
        svc.update(&input("c1", AgentId::Claude, "Blocked update", false))
            .map(|_| ()),
        svc.upsert(&input("c1", AgentId::Claude, "Blocked upsert", false))
            .map(|_| ()),
        svc.delete("c1", AgentId::Claude),
    ] {
        assert_eq!(result.unwrap_err().code(), "provider.lock");
    }

    let snapshot = svc
        .capture_live_config_snapshot_with_guard(&guard, AgentId::Claude)
        .unwrap();
    assert_eq!(
        svc.capture_live_config_snapshot_with_guard(&guard, AgentId::Codex)
            .unwrap_err()
            .code(),
        "invalid_arg"
    );
    assert_eq!(
        svc.switch_with_guard(&guard, "c1", AgentId::Codex)
            .unwrap_err()
            .code(),
        "invalid_arg"
    );
    for result in [
        svc.create_with_guard(&guard, &input("wrong", AgentId::Codex, "Wrong", false))
            .map(|_| ()),
        svc.update_with_guard(&guard, &input("c1", AgentId::Codex, "Wrong", false))
            .map(|_| ()),
        svc.upsert_with_guard(&guard, &input("c1", AgentId::Codex, "Wrong", false))
            .map(|_| ()),
        svc.delete_with_guard(&guard, "c1", AgentId::Codex),
    ] {
        assert_eq!(result.unwrap_err().code(), "invalid_arg");
    }
    let other_service = ProviderService::new(db);
    assert_eq!(
        other_service
            .create_with_guard(
                &guard,
                &input("other", AgentId::Claude, "Wrong service", false)
            )
            .unwrap_err()
            .code(),
        "invalid_arg"
    );

    svc.switch_with_guard(&guard, "c1", AgentId::Claude)
        .unwrap();
    svc.restore_live_config_snapshot_with_guard(&guard, &snapshot)
        .unwrap();
    assert_eq!(adapter.config(), live);
    assert!(backups_root
        .parent()
        .unwrap()
        .join("locks")
        .join("provider-claude.lock")
        .exists());
    drop(guard);
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

#[test]
fn updating_current_provider_writes_new_pool_value_not_stale_live() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "old-live"}}),
    };
    let (_root, _db, svc, adapter, _backups) = live_svc(AgentId::Claude, live);
    let mut current = input("c1", AgentId::Claude, "Current", true);
    current.settings_config = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "stale-pool"}});
    svc.create(&current).unwrap();
    assert_eq!(
        adapter.config().raw["env"]["ANTHROPIC_AUTH_TOKEN"],
        "old-live",
        "create of a current row stays pool-only"
    );

    let mut updated = current.clone();
    updated.name = "Current rotated".into();
    updated.settings_config = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "new-key"}});
    let stored = svc.update(&updated).unwrap();
    assert!(stored.is_current);
    assert_eq!(stored.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"], "new-key");
    assert_eq!(
        adapter.config().raw["env"]["ANTHROPIC_AUTH_TOKEN"],
        "new-key",
        "saving the current provider must apply the new pool value"
    );
}

#[test]
fn updating_non_current_provider_does_not_touch_live() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "live-key"}}),
    };
    let (_root, _db, svc, adapter, _backups) = live_svc(AgentId::Claude, live.clone());
    let mut current = input("c1", AgentId::Claude, "Current", true);
    current.settings_config = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "live-key"}});
    svc.create(&current).unwrap();
    let mut spare = input("c2", AgentId::Claude, "Spare", false);
    spare.settings_config = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "spare-old"}});
    svc.create(&spare).unwrap();
    let writes_before = adapter.write_attempts.load(Ordering::SeqCst);

    spare.name = "Spare rotated".into();
    spare.settings_config = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "spare-new"}});
    let stored = svc.update(&spare).unwrap();
    assert!(!stored.is_current);
    assert_eq!(stored.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"], "spare-new");
    assert_eq!(adapter.config(), live);
    assert_eq!(adapter.write_attempts.load(Ordering::SeqCst), writes_before);
}
