//! P12 assignment + reconcile tests (tempfile + temp DB only).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::adapters::{AdapterRegistry, AgentAdapter};
use crate::error::AppError;
use crate::models::{
    AgentConfig, AgentId, AuthState, Capability, CapabilityState, DetectResult, DetectStatus,
    InstallChannel, RunOptions, RunSpec, SkillSourceRecord,
};
use crate::platform::skills::{
    bootstrap_skill_assignments, SkillAssignmentService, SkillReconciler, SkillTargetRegistry,
    StaticSkillTarget,
};
use crate::platform::AgentKey;
use crate::services::SkillService;
use crate::storage::{Database, SkillRepo};

struct FakeAdapter {
    id: AgentId,
    supports: bool,
    skills_root: Option<PathBuf>,
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
            notes: vec![], extra_copies: Vec::new(),
        }
    }

    fn install_channels(&self) -> Vec<InstallChannel> {
        vec![]
    }

    fn read_config(&self) -> crate::error::Result<AgentConfig> {
        Err(AppError::Unsupported("fake".into()))
    }

    fn read_auth(&self) -> crate::error::Result<AuthState> {
        Err(AppError::Unsupported("fake".into()))
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        match cap {
            Capability::Skills if self.supports => CapabilityState::full(),
            Capability::Skills => CapabilityState::unsupported("fake skills unsupported"),
            _ => CapabilityState::unsupported("fake"),
        }
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        self.skills_root.clone()
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        vec![]
    }

    fn build_run_spec(
        &self,
        _binary: &Path,
        _prompt: &str,
        _opts: &RunOptions,
    ) -> crate::error::Result<RunSpec> {
        Err(AppError::Unsupported("fake".into()))
    }
}

fn write_skill(dir: &Path, id: &str, body: &str) {
    let skill = dir.join(id);
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), body).unwrap();
}

fn make_registry(claude: PathBuf, codex: PathBuf) -> AdapterRegistry {
    let mut reg = AdapterRegistry::new();
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Claude,
        supports: true,
        skills_root: Some(claude),
    }));
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Codex,
        supports: true,
        skills_root: Some(codex),
    }));
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Kimi,
        supports: false,
        skills_root: None,
    }));
    reg
}

fn tmp_db() -> (tempfile::TempDir, Database) {
    let dir = crate::utils::test_temp::real_tempdir();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    (dir, db)
}

#[test]
fn migration_applies_skill_assignment_tables_on_database_open() {
    let (_dir, db) = tmp_db();
    db.with_conn(|conn| {
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = '00010_skill_assignments'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(n, 1);
        let tables: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('skill_packages','skill_assignments')",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(tables, 2);
        Ok(())
    })
    .unwrap();
}

#[test]
fn enable_disable_desired_flips_and_observed() {
    let root = crate::utils::test_temp::real_tempdir();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    let codex = root.path().join("codex");
    fs::create_dir_all(&source).unwrap();
    write_skill(
        &source,
        "demo",
        "---\nname: Demo\ndescription: d\n---\n\n# x\n",
    );

    let (_db_dir, db) = tmp_db();
    let reg = make_registry(claude.clone(), codex);
    let svc = SkillService::with_db(source.clone(), reg, db.clone());

    svc.sync("demo", AgentId::Claude, false).unwrap();
    assert!(claude.join("demo").join("SKILL.md").is_file());

    let repo = SkillRepo::new(db.clone());
    let a = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert!(a.desired_enabled);
    assert_eq!(a.observed_status, "applied");
    assert!(a.last_error.is_none());
    assert!(a.applied_revision.is_some());

    // Idempotent reconcile / re-sync
    svc.sync("demo", AgentId::Claude, false).unwrap();
    let a2 = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert!(a2.desired_enabled);
    assert_eq!(a2.observed_status, "applied");

    svc.disable("demo", AgentId::Claude).unwrap();
    assert!(!claude.join("demo").exists());
    let a3 = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert!(!a3.desired_enabled);
    assert_eq!(a3.observed_status, "absent");
    assert!(a3.applied_revision.is_none());
}

#[test]
fn unsupported_agent_records_observed_and_keeps_desired() {
    let root = crate::utils::test_temp::real_tempdir();
    let source = root.path().join("source");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# skill\n");

    let (_db_dir, db) = tmp_db();
    let reg = make_registry(root.path().join("c"), root.path().join("x"));
    let svc = SkillService::with_db(source, reg, db.clone());

    let err = svc.sync("demo", AgentId::Kimi, false).unwrap_err();
    assert_eq!(err.code(), "unsupported");

    let repo = SkillRepo::new(db);
    let a = repo.get_assignment("demo", "kimi").unwrap().unwrap();
    assert!(a.desired_enabled, "desired kept on failure");
    assert_eq!(a.observed_status, "unsupported");
    assert!(
        a.applied_revision.is_none(),
        "never claim applied on failure"
    );
    assert!(a.last_error.is_some());
}

#[test]
fn conflict_without_force_keeps_desired_and_fs() {
    let root = crate::utils::test_temp::real_tempdir();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# source\n");
    // Foreign target content
    write_skill(&claude, "demo", "# foreign unmanaged\n");

    let (_db_dir, db) = tmp_db();
    let reg = make_registry(claude.clone(), root.path().join("codex"));
    let svc = SkillService::with_db(source, reg, db.clone());

    let err = svc.sync("demo", AgentId::Claude, false).unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    // Unmanaged content must remain
    let body = fs::read_to_string(claude.join("demo").join("SKILL.md")).unwrap();
    assert!(body.contains("foreign"));

    let repo = SkillRepo::new(db);
    let a = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert!(a.desired_enabled);
    assert_eq!(a.observed_status, "conflict");
    assert!(a.applied_revision.is_none());
    assert!(a.last_error.is_some());
}

#[test]
fn force_cannot_take_over_foreign_directory() {
    let root = crate::utils::test_temp::real_tempdir();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# source body\n");
    write_skill(&claude, "demo", "# foreign\n");

    let (_db_dir, db) = tmp_db();
    let reg = make_registry(claude.clone(), root.path().join("codex"));
    let svc = SkillService::with_db(source, reg, db.clone());

    let err = svc.sync("demo", AgentId::Claude, true).unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    assert!(
        !err.to_string().contains("use force to replace"),
        "must not prompt force takeover of ordinary dirs: {err}"
    );
    let body = fs::read_to_string(claude.join("demo").join("SKILL.md")).unwrap();
    assert!(body.contains("foreign"), "foreign content preserved");
    let a = SkillRepo::new(db)
        .get_assignment("demo", "claude")
        .unwrap()
        .unwrap();
    assert_eq!(a.observed_status, "conflict");
    assert!(a.desired_enabled);
}

#[test]
fn force_refreshes_verified_managed_copy_after_source_update() {
    let root = crate::utils::test_temp::real_tempdir();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# v1\n");

    let (_db_dir, db) = tmp_db();
    let reg = make_registry(claude.clone(), root.path().join("codex"));
    let svc = SkillService::with_db(source.clone(), reg, db.clone());

    svc.sync("demo", AgentId::Claude, false).unwrap();
    write_skill(&source, "demo", "# v2 source\n");

    let err = svc.sync("demo", AgentId::Claude, false).unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    assert!(err.to_string().contains("use force to refresh"));

    svc.sync("demo", AgentId::Claude, true).unwrap();
    let body = fs::read_to_string(claude.join("demo").join("SKILL.md")).unwrap();
    assert!(body.contains("v2 source"));
    let a = SkillRepo::new(db)
        .get_assignment("demo", "claude")
        .unwrap()
        .unwrap();
    assert_eq!(a.observed_status, "applied");
}

#[test]
fn reconcile_is_idempotent() {
    let root = crate::utils::test_temp::real_tempdir();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# s\n");

    let (_db_dir, db) = tmp_db();
    let reg = make_registry(claude.clone(), root.path().join("codex"));
    let targets = SkillTargetRegistry::from_adapter_registry(&reg).unwrap();
    let repo = SkillRepo::new(db);
    let assign = SkillAssignmentService::new(repo.clone());
    let reconciler = SkillReconciler::new(source.clone(), targets, repo.clone());

    let now = "t0";
    assign.ensure_package("demo", None, now).unwrap();
    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("copy"), now)
        .unwrap();
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, now)
        .unwrap();
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, "t1")
        .unwrap();
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, "t2")
        .unwrap();

    assert!(claude.join("demo").join("SKILL.md").is_file());
    let a = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(a.observed_status, "applied");
    assert!(a.desired_enabled);
}

#[test]
fn bootstrap_imports_managed_copy_not_unmanaged() {
    use crate::platform::skills::ownership::{fingerprint_tree_at, write_copy_ownership_marker};

    let root = crate::utils::test_temp::real_tempdir();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    let codex = root.path().join("codex");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "managed", "# same\n");
    write_skill(&source, "foreign", "# src\n");
    // Managed: copy with valid ownership marker (byte-identical alone is NOT enough).
    write_skill(&claude, "managed", "# same\n");
    let managed_fp = fingerprint_tree_at(&claude.join("managed")).unwrap();
    write_copy_ownership_marker(&claude, "managed", "v1", &managed_fp).unwrap();
    // Byte-identical without marker must NOT be imported.
    write_skill(&codex, "managed", "# same\n");
    // Unmanaged: different content — must not become desired assignment
    write_skill(&codex, "foreign", "# other content\n");

    // Lock only for managed
    let lock = serde_json::json!({
        "managed": {
            "kind": "local",
            "locator": "/tmp/managed",
            "version": "v1",
            "installedAt": "t0",
            "updatedAt": null
        },
        "foreign": {
            "kind": "local",
            "locator": "/tmp/foreign",
            "version": null,
            "installedAt": "t0",
            "updatedAt": null
        }
    });
    fs::write(
        source.join(".skill-lock.json"),
        serde_json::to_string_pretty(&lock).unwrap(),
    )
    .unwrap();

    let (_db_dir, db) = tmp_db();
    let mut targets = SkillTargetRegistry::new();
    targets
        .register(Arc::new(StaticSkillTarget {
            agent_key: AgentKey::from_agent_id(AgentId::Claude),
            skills_root: Some(claude),
            supports: true,
        }))
        .unwrap();
    targets
        .register(Arc::new(StaticSkillTarget {
            agent_key: AgentKey::from_agent_id(AgentId::Codex),
            skills_root: Some(codex.clone()),
            supports: true,
        }))
        .unwrap();
    let repo = SkillRepo::new(db);
    let report = bootstrap_skill_assignments(&source, &targets, &repo, "now").unwrap();
    assert!(report.packages_ensured >= 2);
    assert!(report.assignments_imported >= 1);

    let managed = repo.get_assignment("managed", "claude").unwrap().unwrap();
    assert!(managed.desired_enabled);
    assert_eq!(managed.observed_status, "applied");
    assert_eq!(managed.applied_revision.as_deref(), Some("v1"));

    // Byte-identical at codex without marker must not be imported
    let managed_codex = repo.get_assignment("managed", "codex").unwrap();
    assert!(
        managed_codex.is_none() || managed_codex.as_ref().unwrap().observed_status != "applied",
        "byte-identical without marker must not be imported: {managed_codex:?}"
    );

    // Foreign unmanaged at codex must not be imported as applied
    let foreign = repo.get_assignment("foreign", "codex").unwrap();
    assert!(
        foreign.is_none()
            || (!foreign.as_ref().unwrap().desired_enabled
                && foreign.as_ref().unwrap().observed_status != "applied"),
        "unmanaged projection must not be imported as applied: {foreign:?}"
    );
    // FS content preserved
    assert!(codex.join("foreign").join("SKILL.md").is_file());

    // Second bootstrap is idempotent
    let report2 = bootstrap_skill_assignments(&source, &targets, &repo, "now2").unwrap();
    assert_eq!(report2.assignments_imported, 0);
}

#[test]
fn skill_service_without_db_keeps_fs_only_path() {
    let root = crate::utils::test_temp::real_tempdir();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# s\n");
    let reg = make_registry(claude.clone(), root.path().join("codex"));
    let svc = SkillService::new(source, reg);
    assert!(svc.db().is_none());
    svc.sync("demo", AgentId::Claude, false).unwrap();
    assert!(claude.join("demo").join("SKILL.md").is_file());
    svc.disable("demo", AgentId::Claude).unwrap();
    assert!(!claude.join("demo").exists());
}

#[test]
fn agent_key_target_registry_and_reconcile_are_open_and_stable() {
    let root = crate::utils::test_temp::real_tempdir();
    let source = root.path().join("source");
    let future_root = root.path().join("future");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# future agent skill");

    let future = AgentKey::parse("future-agent").unwrap();
    let earlier = AgentKey::parse("alpha-agent").unwrap();
    let mut targets = SkillTargetRegistry::new();
    targets
        .register(Arc::new(StaticSkillTarget {
            agent_key: future.clone(),
            skills_root: Some(future_root.clone()),
            supports: true,
        }))
        .unwrap();
    targets
        .register(Arc::new(StaticSkillTarget {
            agent_key: earlier.clone(),
            skills_root: None,
            supports: false,
        }))
        .unwrap();

    assert_eq!(targets.get(&future).unwrap().agent_key(), future);
    let ordered: Vec<_> = targets.all().map(|target| target.agent_key()).collect();
    assert_eq!(ordered, vec![future.clone(), earlier.clone()]);

    let duplicate = targets
        .register(Arc::new(StaticSkillTarget {
            agent_key: future.clone(),
            skills_root: Some(root.path().join("duplicate")),
            supports: true,
        }))
        .unwrap_err();
    assert_eq!(duplicate.code(), "skill.target_duplicate");
    let ordered_after_duplicate: Vec<_> = targets.all().map(|target| target.agent_key()).collect();
    assert_eq!(ordered_after_duplicate, vec![future.clone(), earlier]);

    let (_db_dir, db) = tmp_db();
    let repo = SkillRepo::new(db);
    let assign = SkillAssignmentService::new(repo.clone());
    assign.ensure_package("demo", None, "t0").unwrap();
    assign
        .set_desired_enabled("demo", &future, true, Some("copy"), "t0")
        .unwrap();

    let reconciler = SkillReconciler::new(source, targets, repo);
    reconciler
        .reconcile_one("demo", &future, false, "t1")
        .unwrap();
    assert!(future_root.join("demo").join("SKILL.md").is_file());

    let outcomes = reconciler.reconcile_skill("demo", false, "t2").unwrap();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].0, future);
    assert!(outcomes[0].1.is_ok());
}

#[test]
fn reconcile_skill_reports_unregistered_valid_agent_key() {
    let root = crate::utils::test_temp::real_tempdir();
    let source = root.path().join("source");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# skill");

    let (_db_dir, db) = tmp_db();
    let repo = SkillRepo::new(db);
    let assign = SkillAssignmentService::new(repo.clone());
    let unknown = AgentKey::parse("unregistered-agent").unwrap();
    assign.ensure_package("demo", None, "t0").unwrap();
    assign
        .set_desired_enabled("demo", &unknown, true, Some("copy"), "t0")
        .unwrap();

    let reconciler = SkillReconciler::new(source, SkillTargetRegistry::new(), repo.clone());
    let outcomes = reconciler.reconcile_skill("demo", false, "t1").unwrap();
    assert_eq!(outcomes.len(), 1, "valid unknown key must not be skipped");
    assert_eq!(outcomes[0].0, unknown);
    assert_eq!(outcomes[0].1.as_ref().unwrap_err().code(), "unsupported");

    let row = repo
        .get_assignment("demo", "unregistered-agent")
        .unwrap()
        .unwrap();
    assert_eq!(row.observed_status, "unsupported");
    assert!(row.desired_enabled);
}

#[test]
fn reconcile_skill_rejects_invalid_database_agent_key() {
    let root = crate::utils::test_temp::real_tempdir();
    let source = root.path().join("source");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# skill");

    let (_db_dir, db) = tmp_db();
    let assign = SkillAssignmentService::new(SkillRepo::new(db.clone()));
    assign.ensure_package("demo", None, "t0").unwrap();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO skill_assignments (
                skill_package_id, agent_key, desired_enabled, projection_mode,
                observed_status, updated_at
             ) VALUES (?1, ?2, 1, 'copy', 'pending', 't0')",
            rusqlite::params!["demo", "Invalid_Key"],
        )?;
        Ok(())
    })
    .unwrap();

    let reconciler = SkillReconciler::new(source, SkillTargetRegistry::new(), SkillRepo::new(db));
    let err = reconciler.reconcile_skill("demo", false, "t1").unwrap_err();
    assert_eq!(err.code(), "skill.assignment_data");
}

#[test]
fn ensure_package_from_lock_record() {
    let (_db_dir, db) = tmp_db();
    let assign = SkillAssignmentService::new(SkillRepo::new(db.clone()));
    let rec = SkillSourceRecord {
        kind: "git".into(),
        locator: "https://example.com/skill.git".into(),
        version: Some("abc".into()),
        installed_at: "t0".into(),
        updated_at: None,
    };
    let pkg = assign.ensure_package("demo", Some(&rec), "t1").unwrap();
    assert_eq!(pkg.revision, "abc");
    assert_eq!(pkg.source_kind, "git");
    // Idempotent
    let pkg2 = assign.ensure_package("demo", Some(&rec), "t2").unwrap();
    assert_eq!(pkg2.id, pkg.id);
    assert_eq!(pkg2.created_at, pkg.created_at);
}
