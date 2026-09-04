//! Reconciler honors `assignment.projection_mode=link`.
//!
//! Tempdir + temp DB only. Does not scan or mutate the real `~/.agents` tree.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::fs_safe::is_link_or_reparse;
use super::ownership::ownership_marker_path;
use crate::adapters::{AdapterRegistry, AgentAdapter};
use crate::error::AppError;
use crate::models::{
    AgentConfig, AgentId, AuthState, Capability, CapabilityState, DetectResult, DetectStatus,
    InstallChannel, RunOptions, RunSpec, SkillProjectionMode,
};
use crate::platform::skills::{
    SkillAssignmentService, SkillReconciler, SkillTargetRegistry, StaticSkillTarget,
};
use crate::platform::AgentKey;
use crate::services::SkillService;
use crate::storage::{Database, SkillRepo};

struct FakeAdapter {
    id: AgentId,
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
            notes: vec![],
            extra_copies: Vec::new(),
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
        cap.fake_state(&[Capability::Skills])
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

fn tmp_db() -> (tempfile::TempDir, Database) {
    let dir = crate::utils::test_temp::real_tempdir();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    (dir, db)
}

fn setup() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    PathBuf,
    PathBuf,
    Database,
    SkillAssignmentService,
    SkillReconciler,
    SkillRepo,
) {
    let root = crate::utils::test_temp::real_tempdir();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&claude).unwrap();
    write_skill(&source, "demo", "# demo\n");

    let (db_dir, db) = tmp_db();
    let mut targets = SkillTargetRegistry::new();
    targets
        .register(Arc::new(StaticSkillTarget {
            agent_key: AgentKey::from_agent_id(AgentId::Claude),
            skills_root: Some(claude.clone()),
            supports: true,
        }))
        .unwrap();
    let repo = SkillRepo::new(db.clone());
    let assign = SkillAssignmentService::new(repo.clone());
    let reconciler = SkillReconciler::new(source.clone(), targets, repo.clone());
    assign.ensure_package("demo", None, "t0").unwrap();
    (root, db_dir, source, claude, db, assign, reconciler, repo)
}

fn is_dir_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| is_link_or_reparse(&m))
        .unwrap_or(false)
}

fn try_symlink_dir(target: &Path, link: &Path) -> bool {
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link).is_ok()
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
}

fn make_svc_db(source: PathBuf, claude: PathBuf, db: Database) -> SkillService {
    let mut reg = AdapterRegistry::new();
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Claude,
        skills_root: Some(claude),
    }));
    SkillService::with_db(source, reg, db)
}

#[test]
fn link_mode_creates_symlink_when_target_missing() {
    let (_root, _db_dir, _source, claude, _db, assign, reconciler, repo) = setup();
    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("link"), "t0")
        .unwrap();
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, "t1")
        .unwrap();

    let target = claude.join("demo");
    assert!(is_dir_symlink(&target), "missing target must become a link");
    assert!(target.join("SKILL.md").is_file());
    assert!(
        !ownership_marker_path(&claude, "demo").exists(),
        "true link must not write a copy marker"
    );
    let row = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(row.observed_status, "applied");
    assert_eq!(row.projection_mode, "link");
    assert_eq!(row.applied_revision.as_deref(), Some("1"));
}

#[test]
fn link_mode_is_idempotent_and_clears_stale_copy_marker() {
    let (_root, _db_dir, source, claude, _db, assign, reconciler, repo) = setup();
    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("link"), "t0")
        .unwrap();
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, "t1")
        .unwrap();

    super::ownership::write_copy_ownership_marker(&claude, "demo", "stale", "deadbeef").unwrap();
    assert!(ownership_marker_path(&claude, "demo").is_file());

    assign.ensure_package("demo", None, "t2").unwrap();
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, true, "t2")
        .unwrap();

    let target = claude.join("demo");
    assert!(is_dir_symlink(&target));
    assert!(
        !ownership_marker_path(&claude, "demo").exists(),
        "stale copy marker on a correct link must be cleared"
    );
    let row = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(row.observed_status, "applied");
    assert_eq!(row.applied_revision.as_deref(), Some("1"));
    assert!(source.join("demo").join("SKILL.md").is_file());
}

#[test]
fn link_mode_rejects_foreign_link_without_mutation() {
    let (_root, _db_dir, _source, claude, _db, assign, reconciler, repo) = setup();
    let foreign = claude.join("elsewhere");
    fs::create_dir_all(&foreign).unwrap();
    fs::write(foreign.join("SKILL.md"), "# other\n").unwrap();
    if !try_symlink_dir(&foreign, &claude.join("demo")) {
        return;
    }

    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("link"), "t0")
        .unwrap();
    let err = reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, true, "t1")
        .unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    assert!(is_dir_symlink(&claude.join("demo")));
    assert_eq!(
        fs::read_to_string(claude.join("demo").join("SKILL.md")).unwrap(),
        "# other\n"
    );
    let row = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(row.observed_status, "conflict");
}

#[test]
fn link_mode_rejects_unmanaged_directory_even_with_force() {
    let (_root, _db_dir, _source, claude, _db, assign, reconciler, repo) = setup();
    write_skill(&claude, "demo", "# user owned\n");

    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("link"), "t0")
        .unwrap();
    let err = reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, true, "t1")
        .unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    assert!(!is_dir_symlink(&claude.join("demo")));
    assert_eq!(
        fs::read_to_string(claude.join("demo").join("SKILL.md")).unwrap(),
        "# user owned\n"
    );
    let row = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(row.observed_status, "conflict");
}

#[test]
fn link_mode_converts_managed_copy_to_symlink() {
    let (_root, _db_dir, _source, claude, _db, assign, reconciler, repo) = setup();
    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("copy"), "t0")
        .unwrap();
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, "t1")
        .unwrap();
    assert!(!is_dir_symlink(&claude.join("demo")));
    assert!(ownership_marker_path(&claude, "demo").is_file());

    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("link"), "t2")
        .unwrap();
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, "t3")
        .unwrap();

    assert!(
        is_dir_symlink(&claude.join("demo")),
        "verified managed copy must be replaced by a link"
    );
    assert!(
        !ownership_marker_path(&claude, "demo").exists(),
        "copy marker must be cleared after a successful convert"
    );
    let row = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(row.observed_status, "applied");
    assert_eq!(row.projection_mode, "link");
}

#[test]
fn copy_mode_still_copies_and_leaves_correct_link_untouched() {
    let (_root, _db_dir, source, claude, _db, assign, reconciler, _repo) = setup();
    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("copy"), "t0")
        .unwrap();
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, "t1")
        .unwrap();
    assert!(!is_dir_symlink(&claude.join("demo")));
    assert!(ownership_marker_path(&claude, "demo").is_file());

    fs::remove_dir_all(claude.join("demo")).unwrap();
    if !try_symlink_dir(&source.join("demo"), &claude.join("demo")) {
        return;
    }
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, "t2")
        .unwrap();
    assert!(
        is_dir_symlink(&claude.join("demo")),
        "copy path must no-op a correct source link"
    );
}

#[test]
fn disable_then_sync_preserves_link_mode_and_recreates_link() {
    let (_root, _db_dir, source, claude, db, assign, reconciler, repo) = setup();
    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("link"), "t0")
        .unwrap();
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, "t1")
        .unwrap();
    assert!(is_dir_symlink(&claude.join("demo")));

    let svc = make_svc_db(source, claude.clone(), db);
    svc.disable("demo", AgentId::Claude).unwrap();
    assert!(!claude.join("demo").exists());
    let after_disable = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(after_disable.projection_mode, "link");
    assert!(!after_disable.desired_enabled);

    svc.sync("demo", AgentId::Claude, false).unwrap();
    assert!(
        is_dir_symlink(&claude.join("demo")),
        "re-enable via sync must rebuild a link"
    );
    let after_sync = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(after_sync.projection_mode, "link");
    assert_eq!(after_sync.observed_status, "applied");
}

#[test]
fn sync_with_mode_link_converts_managed_copy() {
    let (_root, _db_dir, source, claude, db, assign, reconciler, repo) = setup();
    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("copy"), "t0")
        .unwrap();
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, "t1")
        .unwrap();
    assert!(!is_dir_symlink(&claude.join("demo")));

    let svc = make_svc_db(source, claude.clone(), db);
    svc.sync_with_mode(
        "demo",
        AgentId::Claude,
        false,
        Some(SkillProjectionMode::Link),
    )
    .unwrap();
    assert!(
        is_dir_symlink(&claude.join("demo")),
        "explicit link mode must replace a managed copy"
    );
    let row = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(row.projection_mode, "link");
}

#[test]
fn sync_with_mode_copy_replaces_correct_link() {
    let (_root, _db_dir, source, claude, db, assign, reconciler, repo) = setup();
    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("link"), "t0")
        .unwrap();
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, "t1")
        .unwrap();
    assert!(
        is_dir_symlink(&claude.join("demo")),
        "setup must produce a directory link before copy-replace"
    );

    let svc = make_svc_db(source, claude.clone(), db);
    svc.sync_with_mode(
        "demo",
        AgentId::Claude,
        false,
        Some(SkillProjectionMode::Copy),
    )
    .unwrap();
    assert!(
        !is_dir_symlink(&claude.join("demo")),
        "explicit copy mode must replace a correct source link"
    );
    assert!(claude.join("demo").is_dir());
    let row = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(row.projection_mode, "copy");
}

#[test]
fn sync_does_not_rewrite_existing_link_mode_to_copy() {
    let (_root, _db_dir, source, claude, db, assign, reconciler, repo) = setup();
    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("link"), "t0")
        .unwrap();
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, "t1")
        .unwrap();

    let svc = make_svc_db(source, claude.clone(), db);
    svc.sync("demo", AgentId::Claude, false).unwrap();
    let row = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(row.projection_mode, "link");
    assert!(is_dir_symlink(&claude.join("demo")));
}

#[test]
fn set_desired_enabled_none_keeps_existing_link_mode() {
    let (_root, _db_dir, _source, _claude, _db, assign, _reconciler, repo) = setup();
    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("link"), "t0")
        .unwrap();
    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, false, None, "t1")
        .unwrap();
    let row = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(row.projection_mode, "link");
    assert!(!row.desired_enabled);
}

#[test]
fn link_fallback_copy_stays_desired_link_and_can_be_disabled() {
    use super::ownership::finalize_link_projection_ownership;
    use super::packages::{materialize_projection, validate_and_collect_source};

    let (_root, _db_dir, source, claude, _db, assign, reconciler, repo) = setup();
    let files = validate_and_collect_source(&source.join("demo"), "demo").unwrap();
    materialize_projection(&claude, "demo", &claude.join("demo"), &files, None).unwrap();
    finalize_link_projection_ownership(&claude, "demo", &claude.join("demo"), true, "1").unwrap();
    assert!(ownership_marker_path(&claude, "demo").is_file());

    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("link"), "t0")
        .unwrap();
    // Convert should succeed on unix (real symlink). If it stays a managed copy
    // that is also acceptable fallback; either way disable must work.
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, "t1")
        .unwrap();
    let row = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(row.projection_mode, "link");
    assert_eq!(row.observed_status, "applied");

    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, false, None, "t2")
        .unwrap();
    reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, "t3")
        .unwrap();
    assert!(!claude.join("demo").exists());
    assert!(!ownership_marker_path(&claude, "demo").exists());
    let disabled = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(disabled.projection_mode, "link");
    assert_eq!(disabled.observed_status, "absent");
}
