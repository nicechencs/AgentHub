use super::*;
use crate::adapters::AgentAdapter;
use crate::models::{
    AuthState, Capability, CapabilityState, DetectResult, DetectStatus, InstallChannel, RunOptions,
    RunSpec,
};
use crate::services::LiveWriteAuthority;
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
            extra_copies: Vec::new(),
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
        cap.fake_state(&[Capability::ConfigWrite, Capability::LiveBackup])
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
                "api_key": format!("sk-{id}-secret"),
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
        settings_config: json!({"api_key": format!("sk-{id}-secret"), "base_url": "https://x"}),
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
    assert!(created.updated_at >= created.created_at);
    assert_eq!(created.meta["surface"], "unknown");
    // Secrets remain unredacted at service boundary (CLI redacts).
    assert_eq!(created.settings_config["api_key"], "sk-p1-secret");

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
    assert_eq!(stored.settings_config["api_key"], "sk-p1-secret");
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
fn import_live_preserves_full_secrets_without_stealing_current() {
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
    assert!(!imported.is_current);
    assert_eq!(imported.settings_config, live.raw);
    assert_eq!(imported.meta["source"], "live");
    assert_eq!(imported.meta["surface"], "anthropic-api");
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
    assert!(svc.get("old", None).unwrap().is_current);

    // Re-importing the unchanged live snapshot is idempotent: it reuses the
    // same canonical live row instead of creating another UUID row.
    let imported_again = svc
        .import_live(AgentId::Claude, Some("Imported live"))
        .unwrap();
    assert_eq!(imported_again.id, imported.id);
    assert_eq!(svc.list(Some(AgentId::Claude)).unwrap().len(), 2);
}

#[test]
fn import_live_loopback_collapses_extra_rows_and_keeps_generated() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({
            "env": {
                "ANTHROPIC_BASE_URL": "http://127.0.0.1:43081",
                "ANTHROPIC_AUTH_TOKEN": "token-aaa"
            }
        }),
    };
    let (_root, _db, svc, adapter, _backups) = live_svc(AgentId::Claude, live);
    let imported = svc
        .import_live(AgentId::Claude, Some("Claude live"))
        .unwrap();

    svc.repo()
        .upsert(&Provider {
            id: "claude-old-loopback".into(),
            agent_id: AgentId::Claude,
            name: "Old loopback leftover".into(),
            settings_config: json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "http://127.0.0.1:11111",
                    "ANTHROPIC_AUTH_TOKEN": "old-leftover"
                }
            }),
            meta: json!({ "source": "live" }),
            is_current: false,
            created_at: "2020-01-01 00:00:00".into(),
            updated_at: "2020-01-01 00:00:00".into(),
        })
        .unwrap();

    let mut leftover_manual = input(
        "claude-manual-loopback",
        AgentId::Claude,
        "Manual loopback",
        false,
    );
    leftover_manual.settings_config = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "http://localhost:22222",
            "ANTHROPIC_AUTH_TOKEN": "manual-leftover"
        }
    });
    leftover_manual.meta = json!({ "source": "manual" });
    svc.create(&leftover_manual).unwrap();

    let mut generated = input("claude-generated", AgentId::Claude, "Generated", false);
    generated.settings_config = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "http://127.0.0.1:33333",
            "ANTHROPIC_AUTH_TOKEN": "generated-token"
        }
    });
    generated.meta = json!({
        "generatedBy": "adapter",
        "adapterRuleId": "kimi-membership-to-claude-v1"
    });
    svc.create(&generated).unwrap();

    let mut remote = input("claude-remote", AgentId::Claude, "Remote", false);
    remote.settings_config = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
            "ANTHROPIC_AUTH_TOKEN": "sk-remote"
        }
    });
    svc.create(&remote).unwrap();

    *adapter.config.lock().unwrap() = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({
            "env": {
                "ANTHROPIC_BASE_URL": "http://127.0.0.1:44227",
                "ANTHROPIC_AUTH_TOKEN": "token-bbb"
            }
        }),
    };
    let refreshed = svc.import_live(AgentId::Claude, None).unwrap();
    assert_eq!(refreshed.id, imported.id);
    assert_eq!(
        refreshed.settings_config["env"]["ANTHROPIC_BASE_URL"],
        "http://127.0.0.1:44227"
    );
    assert_eq!(
        refreshed.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "token-bbb"
    );

    let list = svc.list(Some(AgentId::Claude)).unwrap();
    assert!(svc
        .get("claude-old-loopback", Some(AgentId::Claude))
        .is_err());
    assert!(svc
        .get("claude-manual-loopback", Some(AgentId::Claude))
        .is_err());
    assert!(svc.get("claude-generated", Some(AgentId::Claude)).is_ok());
    assert!(svc.get("claude-remote", Some(AgentId::Claude)).is_ok());
    assert_eq!(
        list.iter()
            .filter(|row| row.meta.get("source").and_then(|v| v.as_str()) == Some("live"))
            .count(),
        1
    );
    assert_eq!(
        list.iter()
            .filter(|row| row.meta.get("generatedBy").and_then(|v| v.as_str()) == Some("adapter"))
            .count(),
        1
    );
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
    assert!(!refreshed.is_current);

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

    assert!(svc.undo_switch(AgentId::Claude).unwrap());
    assert!(svc.get("c1", None).unwrap().is_current);
    assert!(!svc.get("c2", None).unwrap().is_current);
    assert!(
        !svc.undo_switch(AgentId::Claude).unwrap(),
        "undo is one-shot"
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
    let (_root, _db, svc, adapter, _backups_root) = live_svc(AgentId::Claude, live);
    svc.create(&input("c1", AgentId::Claude, "Current", true))
        .unwrap();
    let _held = LiveWriteAuthority::from_database(&_db)
        .acquire(AgentId::Claude)
        .unwrap();

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
    let (_root, _db, svc, _adapter, _backups_root) = live_svc(AgentId::Claude, live);
    let _held = LiveWriteAuthority::from_database(&_db)
        .acquire(AgentId::Claude)
        .unwrap();
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
    assert_eq!(
        stored.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "new-key"
    );
    assert_eq!(
        adapter.config().raw["env"]["ANTHROPIC_AUTH_TOKEN"],
        "new-key",
        "saving the current provider must apply the new pool value"
    );
}

#[test]
fn updating_current_provider_apply_failure_restores_db_and_live() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "old-live"}}),
    };
    let (_root, _db, svc, adapter, _backups) = live_svc(AgentId::Claude, live.clone());
    let current = svc
        .create(&input("c1", AgentId::Claude, "Current", true))
        .unwrap();
    adapter.fail_on_write(1);

    let mut updated = input("c1", AgentId::Claude, "Current rotated", true);
    updated.settings_config = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "new-live"}});
    let error = svc.update(&updated).unwrap_err();
    assert_eq!(error.code(), "test.write");
    assert_eq!(adapter.config(), live);
    assert_eq!(svc.get("c1", None).unwrap(), current);
}

#[test]
fn provider_compensation_fails_closed_when_another_writer_changes_the_row() {
    let (_root, _db, svc, _adapter, _backups) = live_svc(
        AgentId::Claude,
        AgentConfig {
            agent: AgentId::Claude,
            raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "live"}}),
        },
    );
    let original = svc
        .create(&input("c1", AgentId::Claude, "Original", false))
        .unwrap();
    let mut external = original.clone();
    external.name = "external-writer".into();
    external.updated_at = "external-revision".into();
    svc.repo().update(&external).unwrap();

    let mut expected_after = original.clone();
    expected_after.name = "mutation-result".into();
    expected_after.updated_at = "mutation-revision".into();
    let error = svc
        .restore_provider_rows(
            AgentId::Claude,
            std::slice::from_ref(&original),
            std::slice::from_ref(&expected_after),
            &expected_after,
            false,
            std::slice::from_ref(&original.id),
        )
        .unwrap_err();

    assert_eq!(error.code(), "provider.current.apply.rollback.database");
    assert_eq!(
        svc.repo().get_by_id(&original.id).unwrap().unwrap(),
        external
    );
}

#[test]
fn provider_pre_mutation_failure_does_not_rollback_concurrent_writer() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "live"}}),
    };
    let (_root, _db, svc, _adapter, _backups) = live_svc(AgentId::Claude, live);
    let current = svc
        .create(&input("c1", AgentId::Claude, "Current", true))
        .unwrap();
    let mut external = current.clone();
    external.name = "concurrent-writer".into();
    external.updated_at = "concurrent-revision".into();
    svc.repo().update(&external).unwrap();

    let error = svc
        .update(&input("missing-provider", AgentId::Claude, "Missing", true))
        .unwrap_err();
    assert_eq!(error.code(), "not_found");
    assert_eq!(
        svc.repo().get_by_id(&current.id).unwrap().unwrap(),
        external,
        "pre-commit provider failure must not restore a concurrent writer's row"
    );
}

#[test]
fn upserting_current_provider_apply_failure_removes_new_row_and_restores_db() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "old-live"}}),
    };
    let (_root, _db, svc, adapter, _backups) = live_svc(AgentId::Claude, live.clone());
    let current = svc
        .create(&input("c1", AgentId::Claude, "Current", true))
        .unwrap();
    let binding_before = svc.connections.get_active(AgentId::Claude).unwrap();
    adapter.fail_on_write(1);

    let mut target = input("c2", AgentId::Claude, "Target", true);
    target.settings_config = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "new-live"}});
    let error = svc.upsert(&target).unwrap_err();
    assert_eq!(error.code(), "test.write");
    assert_eq!(adapter.config(), live);
    assert_eq!(svc.get("c1", None).unwrap(), current);
    assert!(svc.get("c2", None).is_err());
    assert_eq!(
        svc.connections.get_active(AgentId::Claude).unwrap(),
        binding_before,
        "provider upsert compensation must restore the active binding"
    );
}

#[test]
fn provider_apply_failure_restores_active_account_counterpart_and_binding() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "old-live"}}),
    };
    let (_root, db, svc, adapter, _backups) = live_svc(AgentId::Claude, live.clone());
    let account_repo = crate::storage::AccountRepo::new(db);
    let account = crate::models::Account {
        id: "account-before-provider".into(),
        agent_id: AgentId::Claude,
        kind: crate::models::AccountKind::ApiKey,
        label: "Account before provider".into(),
        credentials: json!({"format": "api_key", "api_key": "account-key"}),
        extra: json!({}),
        status: "active".into(),
        is_current: true,
        created_at: "2026-08-21T00:00:00Z".into(),
        updated_at: "2026-08-21T00:00:00Z".into(),
    };
    let (account_before, binding_before) = svc
        .connections
        .create_and_activate_account(&account)
        .unwrap();
    let provider_before = svc
        .create(&input("c1", AgentId::Claude, "Provider", false))
        .unwrap();

    let mut update = input("c1", AgentId::Claude, "Provider", true);
    update.settings_config = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "new-live"}});
    adapter.fail_on_write(1);
    let error = svc.upsert(&update).unwrap_err();
    assert_eq!(error.code(), "test.write");
    assert_eq!(svc.get("c1", None).unwrap(), provider_before);
    assert_eq!(
        svc.connections.get_active(AgentId::Claude).unwrap(),
        Some(binding_before)
    );
    assert_eq!(
        account_repo.get_by_id(&account_before.id).unwrap().unwrap(),
        account_before
    );
    assert!(
        account_repo
            .get_current(AgentId::Claude)
            .unwrap()
            .unwrap()
            .is_current
    );
    assert_eq!(adapter.config(), live);
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
    assert_eq!(
        stored.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "spare-new"
    );
    assert_eq!(adapter.config(), live);
    assert_eq!(adapter.write_attempts.load(Ordering::SeqCst), writes_before);
}

#[test]
fn create_skips_surface_for_adapter_generated_projection() {
    let (_dir, svc) = svc();
    let created = svc
        .create(&ProviderInput {
            id: "claude-kimi-adapter".into(),
            agent_id: AgentId::Claude,
            name: "Kimi Code (kimi-source)".into(),
            settings_config: json!({"env": {
                "ANTHROPIC_BASE_URL": "https://api.kimi.com/coding/",
                "ANTHROPIC_AUTH_TOKEN": "$AGENTHUB_CONNECTION_SECRET$"
            }}),
            meta: json!({
                "preset": "anthropic-compatible",
                "generatedBy": "adapter",
                "adapterRuleId": "kimi-membership-to-claude-v1",
                "adapterRuleVersion": 1,
                "adapterSecretMode": "source_reference",
                "adapterProfileId": "adapter-kimi-claude",
                "adapterSourceRef": {"kind": "provider", "id": "kimi-source"},
            }),
            is_current: false,
        })
        .unwrap();
    assert!(
        created.meta.get("surface").is_none(),
        "projection create must not stamp surface: {}",
        created.meta
    );
    assert_eq!(created.meta["generatedBy"], "adapter");
    let stored = svc
        .get("claude-kimi-adapter", Some(AgentId::Claude))
        .unwrap();
    assert!(stored.meta.get("surface").is_none());
}

#[test]
fn profile_linked_legacy_projection_skips_surface_precompute() {
    let (_root, db, svc, _adapter, _backups) = live_svc(
        AgentId::Claude,
        AgentConfig {
            agent: AgentId::Claude,
            raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "live"}}),
        },
    );
    let provider = svc
        .create(&input(
            "legacy-profile-provider",
            AgentId::Claude,
            "Legacy",
            false,
        ))
        .unwrap();
    let profile = crate::models::AdapterProfile {
        id: "legacy-profile".into(),
        name: "Legacy projection profile".into(),
        source_kind: crate::models::AdapterSourceKind::Provider,
        source_id: provider.id.clone(),
        target_agent_id: AgentId::Claude,
        route: crate::models::AdapterRoute::ConfigSync,
        mode: crate::models::AdapterProfileMode::Api,
        status: crate::models::AdapterProfileStatus::Active,
        rule_id: "legacy".into(),
        rule_version: "v1".into(),
        generated_provider_id: Some(provider.id.clone()),
        local_port: None,
        auto_start: false,
        last_error_code: None,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };
    crate::storage::AdapterProfileRepo::new(db)
        .create(&profile)
        .unwrap();

    let updated = svc
        .upsert(&input(
            "legacy-profile-provider",
            AgentId::Claude,
            "Legacy renamed",
            false,
        ))
        .unwrap();
    assert!(updated.meta.get("surface").is_none());
    let stored = svc
        .repo()
        .get_by_id("legacy-profile-provider")
        .unwrap()
        .unwrap();
    assert!(
        stored.meta.get("surface").is_none(),
        "legacy projection identified by generated_provider_id must not persist a ticket surface"
    );
}

#[test]
fn create_writes_classified_surface() {
    let (_dir, svc) = svc();
    let created = svc
        .create(&ProviderInput {
            id: "kimi-mem".into(),
            agent_id: AgentId::Kimi,
            name: "Kimi membership".into(),
            settings_config: json!({}),
            meta: json!({"preset": "kimi-code-membership"}),
            is_current: false,
        })
        .unwrap();
    assert_eq!(created.meta["surface"], "kimi-code-membership");
    assert_eq!(created.meta["preset"], "kimi-code-membership");
}

#[test]
fn upsert_writes_kimi_and_unknown_surface() {
    let (_dir, svc) = svc();
    let kimi = svc
        .upsert(&ProviderInput {
            id: "kimi-mem".into(),
            agent_id: AgentId::Kimi,
            name: "Kimi membership".into(),
            settings_config: json!({}),
            meta: json!({"preset": "kimi-code-membership"}),
            is_current: false,
        })
        .unwrap();
    assert_eq!(kimi.meta["surface"], "kimi-code-membership");
    assert_eq!(kimi.meta["preset"], "kimi-code-membership");

    let unknown = svc
        .upsert(&ProviderInput {
            id: "relay".into(),
            agent_id: AgentId::Claude,
            name: "Custom relay".into(),
            settings_config: json!({"base_url": "https://relay.example.com"}),
            meta: json!({"preset": "openai-compatible"}),
            is_current: false,
        })
        .unwrap();
    assert_eq!(unknown.meta["surface"], "unknown");
}

#[test]
fn import_live_writes_kimi_membership_surface() {
    let live = AgentConfig {
        agent: AgentId::Kimi,
        raw: json!({
            "base_url": "https://api.kimi.com/coding/v1",
            "api_key": "kimi-live-key"
        }),
    };
    let (_root, _db, svc, _adapter, _backups) = live_svc(AgentId::Kimi, live);
    let imported = svc.import_live(AgentId::Kimi, Some("Kimi live")).unwrap();
    assert_eq!(imported.meta["source"], "live");
    assert_eq!(imported.meta["surface"], "kimi-code-membership");
}

#[test]
fn import_surface_heal_preserves_persisted_unknown_surface() {
    let live = AgentConfig {
        agent: AgentId::Codex,
        raw: json!({
            "base_url": "https://api.openai.com/v1",
            "api_key": "codex-live-key"
        }),
    };
    let (_root, db, svc, _adapter, _backups) = live_svc(AgentId::Codex, live);
    let imported = svc.import_live(AgentId::Codex, Some("Codex live")).unwrap();
    assert_eq!(imported.meta["surface"], "openai-api");

    let mut authoritative = imported.clone();
    authoritative.meta["surface"] = json!("unknown");
    crate::storage::ProviderRepo::new(db.clone())
        .update(&authoritative)
        .unwrap();

    let healed = svc.import_live(AgentId::Codex, Some("Codex live")).unwrap();
    assert_eq!(healed.meta["surface"], "unknown");
    assert_eq!(
        crate::storage::ProviderRepo::new(db.clone())
            .get_by_id(&imported.id)
            .unwrap()
            .unwrap()
            .meta["surface"],
        "unknown"
    );

    let mut authoritative = imported;
    authoritative.meta["surface"] = json!("future-v9");
    crate::storage::ProviderRepo::new(db.clone())
        .update(&authoritative)
        .unwrap();
    let future = svc.import_live(AgentId::Codex, Some("Codex live")).unwrap();
    assert_eq!(future.meta["surface"], "future-v9");
    assert_eq!(
        crate::storage::ProviderRepo::new(db)
            .get_by_id(&future.id)
            .unwrap()
            .unwrap()
            .meta["surface"],
        "future-v9"
    );
}

#[test]
fn import_live_does_not_persist_ticket_surface_on_generated_provider_id_projection() {
    let live = AgentConfig {
        agent: AgentId::Claude,
        raw: json!({"env": {"ANTHROPIC_AUTH_TOKEN": "live"}}),
    };
    let (_root, db, svc, _adapter, _backups) = live_svc(AgentId::Claude, live.clone());
    let provider = crate::models::Provider {
        id: "legacy-generated-provider".into(),
        agent_id: AgentId::Claude,
        name: "Imported".into(),
        settings_config: live.raw,
        meta: json!({"source": "live"}),
        is_current: true,
        created_at: "2026-08-21T00:00:00Z".into(),
        updated_at: "2026-08-21T00:00:00Z".into(),
    };
    crate::storage::ProviderRepo::new(db.clone())
        .create(&provider)
        .unwrap();
    crate::storage::AdapterProfileRepo::new(db)
        .create(&crate::models::AdapterProfile {
            id: "legacy-generated-profile".into(),
            name: "Legacy generated".into(),
            source_kind: crate::models::AdapterSourceKind::Provider,
            source_id: "kimi-source".into(),
            target_agent_id: AgentId::Claude,
            route: crate::models::AdapterRoute::ConfigSync,
            mode: crate::models::AdapterProfileMode::Api,
            status: crate::models::AdapterProfileStatus::Active,
            rule_id: "legacy".into(),
            rule_version: "v1".into(),
            generated_provider_id: Some(provider.id.clone()),
            local_port: None,
            auto_start: false,
            last_error_code: None,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        })
        .unwrap();

    let imported = svc.import_live(AgentId::Claude, Some("Imported")).unwrap();
    assert!(
        imported.meta.get("surface").is_none(),
        "projection heal must not stamp a ticket surface: {}",
        imported.meta
    );
    let stored = svc.repo().get_by_id(&provider.id).unwrap().unwrap();
    assert!(
        stored.meta.get("surface").is_none(),
        "legacy surface heal must persist the skip to sqlite, not only the in-memory object"
    );
}

#[test]
fn import_live_labels_codex_toml_and_does_not_steal_current() {
    let toml = r#"model_provider = "OpenAI"
model = "gpt-5.5"

[model_providers.OpenAI]
name = "OpenAI"
base_url = "https://mytokens.cc/v1"
wire_api = "responses"
"#;
    let live = AgentConfig {
        agent: AgentId::Codex,
        raw: json!({
            "format": "toml",
            "content": toml,
            "auth": { "OPENAI_API_KEY": "sk-codex-import-fixture" }
        }),
    };
    let (_root, _db, svc, adapter, _backups) = live_svc(AgentId::Codex, live);
    let mut current = input(
        "codex-oauth-standin",
        AgentId::Codex,
        "41375197@qq.com",
        true,
    );
    current.settings_config =
        json!({"api_key": "sk-other-oauth-standin", "base_url": "https://api.openai.com/v1"});
    svc.create(&current).unwrap();

    let imported = svc.import_live(AgentId::Codex, None).unwrap();
    assert!(imported.name.contains("OpenAI"));
    assert!(imported.name.contains("gpt-5.5"));
    assert!(!imported.name.starts_with("Imported "));
    assert_eq!(imported.meta["preset"], "openai-compat");
    assert_eq!(imported.meta["surface"], "unknown");
    assert!(!imported.is_current);
    assert!(svc.get("codex-oauth-standin", None).unwrap().is_current);
    assert_eq!(adapter.write_attempts.load(Ordering::SeqCst), 0);
}

#[test]
fn upsert_same_secret_url_merges_into_existing_row() {
    let secret = "sk-or-v1-fixture-aaaa6aa9-not-real";
    let url = "https://openrouter.ai/api/v1";
    let (_dir, svc) = svc();
    let mut backup = input(
        "openai-compat-openrouter-backup",
        AgentId::Codex,
        "OpenRouter 备选",
        false,
    );
    backup.settings_config = json!({
        "baseURL": url,
        "baseUrl": url,
        "apiKey": secret,
        "api_key": secret,
        "model": "stealth/ox-alpha",
    });
    backup.meta = json!({ "preset": "openrouter" });
    svc.create(&backup).unwrap();

    let mut uuid = input(
        "openai-compat-0e08e310-97ba-4575-a50b-3e3db6eec38c",
        AgentId::Codex,
        "OpenRouter 备选",
        false,
    );
    uuid.settings_config = backup.settings_config.clone();
    uuid.meta = json!({ "preset": "openrouter" });
    let stored = svc.upsert(&uuid).unwrap();
    let listed = svc.list(Some(AgentId::Codex)).unwrap();
    let user_rows: Vec<_> = listed
        .iter()
        .filter(|row| row.meta.get("generatedBy").and_then(|v| v.as_str()) != Some("adapter"))
        .collect();
    assert_eq!(user_rows.len(), 1, "same key+url must merge");
    assert_eq!(user_rows[0].id, stored.id);
    let trash = svc.connections.list_trash(Some(AgentId::Codex)).unwrap();
    assert_eq!(trash.len(), 1);
    let hash = stored.meta["secretHash"].as_str().expect("persisted hash");
    assert_eq!(hash, crate::utils::redact::secret_sha256_hex(secret));
    assert!(!stored.meta.to_string().contains(secret));
}

#[test]
fn list_merges_claude_rows_that_only_differ_by_json_schema() {
    let secret = "sk-fixture-claude-mytokens-272fxxxx";
    let (_dir, svc) = svc();
    let mut with_schema = input("p-schema", AgentId::Claude, "mytokens.cc", false);
    with_schema.settings_config = json!({
        "$schema": "https://json.schemastore.org/claude-code-settings.json",
        "env": {
            "ANTHROPIC_BASE_URL": "https://mytokens.cc",
            "ANTHROPIC_AUTH_TOKEN": secret
        }
    });
    let mut plain = input("p-plain", AgentId::Claude, "mytokens.cc", false);
    plain.settings_config = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://mytokens.cc",
            "ANTHROPIC_AUTH_TOKEN": secret
        }
    });
    svc.create(&with_schema).unwrap();
    svc.create(&plain).unwrap();

    let listed = svc.list(Some(AgentId::Claude)).unwrap();
    let user_rows: Vec<_> = listed
        .iter()
        .filter(|row| row.meta.get("generatedBy").and_then(|v| v.as_str()) != Some("adapter"))
        .collect();
    assert_eq!(user_rows.len(), 1, "same key+url must merge even when one row has $schema");
    let trash = svc.connections.list_trash(Some(AgentId::Claude)).unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(
        crate::services::provider_identity::provider_identity(user_rows[0])
            .expect("identity")
            .base_url,
        "https://mytokens.cc"
    );
}

#[test]
fn heal_secret_url_duplicates_keeps_same_last4_different_names() {
    let secret = "sk-cursor-fixture-xxxx8660";
    let url = "https://api.cursor.com/v1";
    let (_dir, svc) = svc();
    let mut first = input("cursor-mytokens", AgentId::Cursor, "mytokens.cc", false);
    first.settings_config = json!({
        "api_key": secret,
        "base_url": url,
    });
    let mut second = input(
        "cursor-qa-manual",
        AgentId::Cursor,
        "QA Cursor manual",
        false,
    );
    second.settings_config = json!({
        "api_key": secret,
        "base_url": url,
    });
    svc.upsert(&first).unwrap();
    svc.upsert(&second).unwrap();

    let listed = svc.list(Some(AgentId::Cursor)).unwrap();
    let ids: Vec<_> = listed.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(
        listed.len(),
        2,
        "heal must not recycle a distinct named login"
    );
    assert!(ids.contains(&"cursor-mytokens"));
    assert!(ids.contains(&"cursor-qa-manual"));
    assert_eq!(
        crate::utils::redact::mask_secret_tail(secret).as_deref(),
        Some("**8660")
    );
    assert!(svc
        .connections
        .list_trash(Some(AgentId::Cursor))
        .unwrap()
        .is_empty());

    let mut renamed = first.clone();
    renamed.name = "mytokens.cc updated".into();
    let updated = svc.upsert(&renamed).unwrap();
    assert_eq!(updated.id, "cursor-mytokens");
    assert_eq!(updated.name, "mytokens.cc updated");
    assert_eq!(svc.list(Some(AgentId::Cursor)).unwrap().len(), 2);
}

#[test]
fn cursor_switch_fails_closed_with_chinese_reason() {
    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let mut registry = AdapterRegistry::new();
    registry.register(Arc::new(crate::adapters::cursor::CursorAdapter));
    let svc = ProviderService::with_live(db, registry, root.path().join("backups"));
    let mut row = input("cursor-a", AgentId::Cursor, "QA Cursor manual", false);
    row.settings_config = json!({
        "api_key": "sk-cursor-fixture-xxxx8660",
        "base_url": "https://api.cursor.com/v1",
    });
    svc.create(&row).unwrap();

    let error = svc.switch("cursor-a", AgentId::Cursor).unwrap_err();
    assert_eq!(error.code(), "unsupported");
    let message = error.to_string();
    assert!(
        message.contains("CURSOR_API_KEY"),
        "gui must receive the fail-closed reason: {message}"
    );
    assert!(message.contains("Cursor"), "{message}");
    assert!(!svc.get("cursor-a", None).unwrap().is_current);
}

#[test]
fn last4_collision_does_not_merge_different_secrets() {
    let (_dir, svc) = svc();
    let mut a = input("openai-compat-a", AgentId::Codex, "A", false);
    a.settings_config = json!({
        "base_url": "https://openrouter.ai/api/v1",
        "api_key": "sk-or-v1-fixture-aaaa6aa9-not-real"
    });
    let mut b = input("openai-compat-b", AgentId::Codex, "B", false);
    b.settings_config = json!({
        "base_url": "https://openrouter.ai/api/v1",
        "api_key": "sk-or-v1-fixture-bbbb6aa9-not-real"
    });
    svc.create(&a).unwrap();
    svc.create(&b).unwrap();
    assert_eq!(svc.list(Some(AgentId::Codex)).unwrap().len(), 2);
}

#[test]
fn provider_service_and_wallet_preserve_authoritative_unknown_surfaces() {
    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let svc = ProviderService::new(db.clone());

    let unknown = ProviderInput {
        id: "persisted-unknown".into(),
        agent_id: AgentId::Codex,
        name: "Persisted unknown".into(),
        settings_config: json!({ "base_url": "https://relay.example/v1" }),
        meta: json!({ "preset": "openai-compatible", "surface": "unknown" }),
        is_current: false,
    };
    let created_unknown = svc.create(&unknown).unwrap();
    assert_eq!(created_unknown.meta["surface"], "unknown");
    let mut updated_unknown = unknown.clone();
    updated_unknown.name = "Persisted unknown updated".into();
    updated_unknown.meta = json!({ "preset": "openai-compatible" });
    assert_eq!(
        svc.update(&updated_unknown).unwrap().meta["surface"],
        "unknown"
    );
    assert_eq!(
        svc.upsert(&updated_unknown).unwrap().meta["surface"],
        "unknown"
    );

    let unrecognized = ProviderInput {
        id: "persisted-future".into(),
        agent_id: AgentId::Codex,
        name: "Persisted future".into(),
        settings_config: json!({ "base_url": "https://api.openai.com/v1" }),
        meta: json!({ "preset": "openai-compatible", "surface": "future-v9" }),
        is_current: false,
    };
    assert_eq!(
        svc.create(&unrecognized).unwrap().meta["surface"],
        "future-v9"
    );
    let mut updated_unrecognized = unrecognized.clone();
    updated_unrecognized.name = "Persisted future updated".into();
    updated_unrecognized.meta = json!({ "preset": "openai-compatible" });
    assert_eq!(
        svc.update(&updated_unrecognized).unwrap().meta["surface"],
        "future-v9"
    );
    assert_eq!(
        svc.upsert(&updated_unrecognized).unwrap().meta["surface"],
        "future-v9"
    );

    let legacy_missing = Provider {
        id: "persisted-missing".into(),
        agent_id: AgentId::Codex,
        name: "Persisted missing".into(),
        settings_config: json!({ "base_url": "https://api.openai.com/v1" }),
        meta: json!({ "preset": "openai-compatible" }),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };
    svc.repo().create(&legacy_missing).unwrap();
    let legacy_input = ProviderInput {
        id: legacy_missing.id.clone(),
        agent_id: legacy_missing.agent_id,
        name: "Persisted missing updated".into(),
        settings_config: legacy_missing.settings_config.clone(),
        meta: json!({ "preset": "openai-compatible" }),
        is_current: false,
    };
    let legacy_updated = svc.update(&legacy_input).unwrap();
    assert!(legacy_updated.meta.get("surface").is_none());
    assert!(svc
        .repo()
        .get_by_id("persisted-missing")
        .unwrap()
        .unwrap()
        .meta
        .get("surface")
        .is_none());

    let wallet = crate::services::TicketReadService::new(db.clone())
        .list_wallet()
        .unwrap();
    for (id, surface) in [
        ("persisted-unknown", "unknown"),
        ("persisted-future", "unknown"),
    ] {
        let ticket = wallet
            .tickets
            .iter()
            .find(|ticket| ticket.id == format!("provider:{id}"))
            .unwrap();
        assert_eq!(ticket.surface.as_str(), surface, "{id}");
        assert!(ticket.speaks.is_empty(), "{id}");
    }
    assert_eq!(
        svc.repo()
            .get_by_id("persisted-unknown")
            .unwrap()
            .unwrap()
            .meta["surface"],
        "unknown"
    );
    assert_eq!(
        svc.repo()
            .get_by_id("persisted-future")
            .unwrap()
            .unwrap()
            .meta["surface"],
        "future-v9"
    );
}

#[test]
fn persisted_provider_surface_requires_official_openai_evidence() {
    use crate::models::{AgentId, Provider, TicketSurface};

    let provider = |meta, settings_config| Provider {
        id: "relay".into(),
        agent_id: AgentId::Codex,
        name: "relay".into(),
        settings_config,
        meta,
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };

    assert_eq!(
        ProviderService::classify_persisted_provider_surface(&provider(
            serde_json::json!({ "preset": "openai-compatible" }),
            serde_json::json!({ "base_url": "https://relay.example/v1" }),
        )),
        TicketSurface::Unknown
    );
    assert_eq!(
        ProviderService::classify_persisted_provider_surface(&provider(
            serde_json::json!({ "preset": "openrouter" }),
            serde_json::json!({}),
        )),
        TicketSurface::Unknown
    );
    assert_eq!(
        ProviderService::classify_persisted_provider_surface(&provider(
            serde_json::json!({ "preset": "openai-compatible" }),
            serde_json::json!({ "base_url": "https://api.openai.com/v1" }),
        )),
        TicketSurface::OpenaiApi
    );
    assert_eq!(
        ProviderService::classify_persisted_provider_surface(&provider(
            serde_json::json!({ "preset": "openrouter" }),
            serde_json::json!({ "base_url": "https://openrouter.ai/api/v1" }),
        )),
        TicketSurface::OpenaiApi
    );
    assert_eq!(
        ProviderService::classify_persisted_provider_surface(&provider(
            serde_json::json!({ "preset": "openai" }),
            serde_json::json!({ "base_url": "https://openrouter.ai/api/v1" }),
        )),
        TicketSurface::Unknown
    );
    assert_eq!(
        ProviderService::classify_persisted_provider_surface(&provider(
            serde_json::json!({ "preset": "openai-compatible" }),
            serde_json::json!({
                "base_url": "https://api.openai.com/v1",
                "baseUrl": "https://relay.example/v1",
            }),
        )),
        TicketSurface::Unknown
    );
    assert_eq!(
        ProviderService::classify_persisted_provider_surface(&provider(
            serde_json::json!({ "preset": "openai-compatible" }),
            serde_json::json!({ "base_url": "https://api.openai.com.evil.example/v1" }),
        )),
        TicketSurface::Unknown
    );
}
