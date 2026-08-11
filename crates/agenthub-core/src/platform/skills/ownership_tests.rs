//! R03 ownership marker + safe delete tests (tempfile only, no network / user dirs).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::ownership::finalize_link_projection_ownership;
use super::packages::{materialize_projection, validate_and_collect_source};
use crate::adapters::{AdapterRegistry, AgentAdapter};
use crate::error::AppError;
use crate::models::{
    AgentConfig, AgentId, AuthState, Capability, CapabilityState, DetectResult, DetectStatus,
    InstallChannel, RunOptions, RunSpec,
};
use crate::platform::skills::ownership::{
    fingerprint_tree_at, is_managed_projection, ownership_marker_path, ownership_store_dir,
    unproject_with_recycler, verify_copy_ownership, write_copy_ownership_marker,
    SkillOwnershipMarker, OWNERSHIP_FORMAT_VERSION,
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
            Capability::Skills => CapabilityState::full(),
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

fn make_svc(source: PathBuf, claude: PathBuf) -> SkillService {
    let mut reg = AdapterRegistry::new();
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Claude,
        skills_root: Some(claude),
    }));
    SkillService::new(source, reg)
}

fn make_svc_db(source: PathBuf, claude: PathBuf, db: Database) -> SkillService {
    let mut reg = AdapterRegistry::new();
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Claude,
        skills_root: Some(claude),
    }));
    SkillService::with_db(source, reg, db)
}

fn tmp_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    (dir, db)
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

// ---------------------------------------------------------------------------
// 1. Byte-identical without marker: not bootstrap-imported; force cannot claim,
// but legacy disable can safely recycle it.
// ---------------------------------------------------------------------------

#[test]
fn byte_identical_without_marker_not_managed() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# body\n");
    write_skill(&claude, "demo", "# body\n"); // byte-identical, no marker

    assert!(
        !is_managed_projection(
            &source.join("demo"),
            &claude,
            "demo",
            &claude.join("demo"),
            None
        ),
        "byte-identical without marker must not be managed"
    );

    let svc = make_svc(source.clone(), claude.clone());
    let body_before = fs::read_to_string(claude.join("demo").join("SKILL.md")).unwrap();

    // force cannot claim unmarked directories either.
    let err2 = svc.sync("demo", AgentId::Claude, false).unwrap_err();
    assert_eq!(err2.code(), "skill.conflict");
    let err3 = svc.sync("demo", AgentId::Claude, true).unwrap_err();
    assert_eq!(err3.code(), "skill.conflict");
    assert!(
        !err3.to_string().contains("use force to replace"),
        "must not prompt force takeover: {err3}"
    );
    assert_eq!(
        fs::read_to_string(claude.join("demo").join("SKILL.md")).unwrap(),
        body_before
    );

    svc.disable("demo", AgentId::Claude).unwrap();
    assert!(
        !claude.join("demo").exists(),
        "legacy exact copy is recycled"
    );
    assert!(
        !ownership_marker_path(&claude, "demo").exists(),
        "legacy copy has no marker left behind"
    );
}

// ---------------------------------------------------------------------------
// 2. Platform copy writes marker; disable removes target+marker; re-disable ok
// ---------------------------------------------------------------------------

#[test]
fn platform_copy_marker_and_disable_idempotent() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# platform\n");

    let svc = make_svc(source, claude.clone());
    svc.sync("demo", AgentId::Claude, false).unwrap();

    let marker = ownership_marker_path(&claude, "demo");
    assert!(
        marker.is_file(),
        "platform sync must write ownership marker"
    );
    assert!(claude.join("demo").join("SKILL.md").is_file());

    svc.disable("demo", AgentId::Claude).unwrap();
    assert!(!claude.join("demo").exists());
    assert!(!marker.exists(), "marker cleared after disable");

    // Second disable is idempotent
    svc.disable("demo", AgentId::Claude).unwrap();
}

// ---------------------------------------------------------------------------
// 3. Managed copy tampered → disable conflict, content kept
// ---------------------------------------------------------------------------

#[test]
fn tampered_managed_copy_disable_conflicts() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# original\n");

    let svc = make_svc(source, claude.clone());
    svc.sync("demo", AgentId::Claude, false).unwrap();

    // User modifies the projection after platform created it.
    fs::write(claude.join("demo").join("SKILL.md"), "# user edited\n").unwrap();

    let err = svc.disable("demo", AgentId::Claude).unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    assert_eq!(
        fs::read_to_string(claude.join("demo").join("SKILL.md")).unwrap(),
        "# user edited\n"
    );
    assert!(
        ownership_marker_path(&claude, "demo").is_file(),
        "marker remains when delete is refused"
    );
}

#[test]
fn unmarked_copy_with_different_content_cannot_be_disabled() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# source\n");
    write_skill(&claude, "demo", "# different\n");

    let svc = make_svc(source, claude.clone());
    let err = svc.disable("demo", AgentId::Claude).unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    assert!(claude.join("demo").join("SKILL.md").is_file());
}

#[test]
fn recycler_failure_keeps_verified_copy_and_marker() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# platform\n");

    let svc = make_svc(source.clone(), claude.clone());
    svc.sync("demo", AgentId::Claude, false).unwrap();
    let marker = ownership_marker_path(&claude, "demo");
    let agent = AgentKey::from_agent_id(AgentId::Claude);
    let err = unproject_with_recycler(
        &claude,
        "demo",
        &source.join("demo"),
        &claude.join("demo"),
        &agent,
        |_| Err(AppError::message("skill.recycle", "recycle unavailable")),
    )
    .unwrap_err();

    assert_eq!(err.code(), "skill.recycle");
    assert!(claude.join("demo").join("SKILL.md").is_file());
    assert!(marker.is_file(), "marker survives a failed recycle");
}

#[test]
fn verified_marker_is_cleared_before_recycler_runs() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# platform\n");

    let svc = make_svc(source.clone(), claude.clone());
    svc.sync("demo", AgentId::Claude, false).unwrap();
    let marker = ownership_marker_path(&claude, "demo");
    let agent = AgentKey::from_agent_id(AgentId::Claude);
    unproject_with_recycler(
        &claude,
        "demo",
        &source.join("demo"),
        &claude.join("demo"),
        &agent,
        |target| {
            assert!(
                !marker.exists(),
                "canonical marker must be gone before recycle can succeed"
            );
            fs::remove_dir_all(target).map_err(AppError::from)
        },
    )
    .unwrap();

    assert!(!claude.join("demo").exists());
    assert!(!marker.exists());
}

// ---------------------------------------------------------------------------
// 4. Valid marker bootstrap import + idempotent re-bootstrap
// ---------------------------------------------------------------------------

#[test]
fn bootstrap_imports_valid_marker_and_is_idempotent() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# body\n");
    write_skill(&claude, "demo", "# body\n");
    let fp = fingerprint_tree_at(&claude.join("demo")).unwrap();
    write_copy_ownership_marker(&claude, "demo", "rev-a", &fp).unwrap();

    let lock = serde_json::json!({
        "demo": {
            "kind": "local",
            "locator": "/tmp/demo",
            "version": "rev-a",
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
    let repo = SkillRepo::new(db);
    let report = bootstrap_skill_assignments(&source, &targets, &repo, "now").unwrap();
    assert_eq!(report.assignments_imported, 1);
    let a = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert!(a.desired_enabled);
    assert_eq!(a.observed_status, "applied");
    assert_eq!(a.applied_revision.as_deref(), Some("rev-a"));

    let report2 = bootstrap_skill_assignments(&source, &targets, &repo, "now2").unwrap();
    assert_eq!(report2.assignments_imported, 0);
}

// ---------------------------------------------------------------------------
// 5. Wrong target / revision / fingerprint → reject
// ---------------------------------------------------------------------------

#[test]
fn marker_field_mismatches_reject_operations() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# body\n");
    write_skill(&claude, "demo", "# body\n");
    let fp = fingerprint_tree_at(&claude.join("demo")).unwrap();

    // Wrong revision
    write_copy_ownership_marker(&claude, "demo", "wrong-rev", &fp).unwrap();
    assert!(!is_managed_projection(
        &source.join("demo"),
        &claude,
        "demo",
        &claude.join("demo"),
        Some("right-rev")
    ));

    // Wrong fingerprint
    write_copy_ownership_marker(&claude, "demo", "right-rev", "deadbeef").unwrap();
    let err = verify_copy_ownership(&claude, "demo", &claude.join("demo"), Some("right-rev"))
        .unwrap_err();
    assert_eq!(err.code(), "skill.conflict");

    // Correct marker
    write_copy_ownership_marker(&claude, "demo", "right-rev", &fp).unwrap();
    let m =
        verify_copy_ownership(&claude, "demo", &claude.join("demo"), Some("right-rev")).unwrap();
    assert_eq!(m.format_version, OWNERSHIP_FORMAT_VERSION);
    assert_eq!(m.skill_id, "demo");

    // target_relative_path mismatch via hand-written marker
    let bad = SkillOwnershipMarker {
        format_version: OWNERSHIP_FORMAT_VERSION,
        skill_id: "demo".into(),
        target_relative_path: "other".into(),
        projection_mode: "copy".into(),
        applied_revision: "right-rev".into(),
        content_fingerprint: fp.clone(),
    };
    let path = ownership_marker_path(&claude, "demo");
    fs::write(&path, serde_json::to_vec_pretty(&bad).unwrap()).unwrap();
    let err = verify_copy_ownership(&claude, "demo", &claude.join("demo"), None).unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
}

// ---------------------------------------------------------------------------
// 6. Correct link removed without touching source; foreign link not deleted
// ---------------------------------------------------------------------------

#[test]
fn link_ownership_and_foreign_link_conflict() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    let foreign = root.path().join("foreign-target");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&claude).unwrap();
    write_skill(&source, "demo", "# source stays\n");
    write_skill(&foreign, "demo", "# foreign\n");

    let svc = make_svc(source.clone(), claude.clone());

    // Correct link → source
    let link = claude.join("demo");
    if !try_symlink_dir(&source.join("demo"), &link) {
        // Environment cannot create symlinks (e.g. Windows without privilege).
        return;
    }
    assert!(is_managed_projection(
        &source.join("demo"),
        &claude,
        "demo",
        &link,
        None
    ));
    svc.disable("demo", AgentId::Claude).unwrap();
    assert!(!link.exists());
    assert!(
        source.join("demo").join("SKILL.md").is_file(),
        "source must survive link removal"
    );

    // Foreign link → does not resolve to source
    if !try_symlink_dir(&foreign.join("demo"), &link) {
        return;
    }
    let err = svc.disable("demo", AgentId::Claude).unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    assert!(link.exists(), "foreign link must not be deleted");
    assert!(foreign.join("demo").join("SKILL.md").is_file());
}

// ---------------------------------------------------------------------------
// 7. Symlink/reparse on ownership path rejects operations
// ---------------------------------------------------------------------------

#[test]
fn ownership_store_symlink_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    let decoy = root.path().join("decoy");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&claude).unwrap();
    fs::create_dir_all(&decoy).unwrap();
    write_skill(&source, "demo", "# s\n");

    // Create .agenthub as a symlink → ownership store unsafe.
    let agenthub = claude.join(".agenthub");
    if !try_symlink_dir(&decoy, &agenthub) {
        return;
    }

    let svc = make_svc(source, claude);
    // First projection may create skills under claude, but marker write must fail.
    let err = svc.sync("demo", AgentId::Claude, false);
    // Either materialize succeeds and marker fails, or path checks fail earlier.
    assert!(err.is_err());
    let e = err.unwrap_err();
    assert!(
        e.code() == "invalid_arg" || e.code() == "skill.ownership" || e.code() == "io",
        "unexpected code: {} / {}",
        e.code(),
        e
    );
}

// ---------------------------------------------------------------------------
// force cannot claim unmarked directory without rematerialize (still conflict on disable)
// + reconcile unproject conflict → observed conflict
// ---------------------------------------------------------------------------

#[test]
fn force_cannot_claim_unmarked_directory() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# body\n");
    write_skill(&claude, "demo", "# body\n");

    let (_db_dir, db) = tmp_db();
    let svc = make_svc_db(source.clone(), claude.clone(), db.clone());

    let body_before = fs::read_to_string(claude.join("demo").join("SKILL.md")).unwrap();
    let err = svc.sync("demo", AgentId::Claude, true).unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    assert_eq!(
        fs::read_to_string(claude.join("demo").join("SKILL.md")).unwrap(),
        body_before
    );
    assert!(
        !ownership_marker_path(&claude, "demo").is_file(),
        "force must not stamp a marker onto unmarked content"
    );
}

#[test]
fn force_refreshes_verified_managed_after_source_change() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# v1\n");

    let svc = make_svc(source.clone(), claude.clone());
    svc.sync("demo", AgentId::Claude, false).unwrap();
    assert!(ownership_marker_path(&claude, "demo").is_file());

    write_skill(&source, "demo", "# v2\n");
    let err = svc.sync("demo", AgentId::Claude, false).unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    assert!(err.to_string().contains("use force to refresh"));

    svc.sync("demo", AgentId::Claude, true).unwrap();
    assert_eq!(
        fs::read_to_string(claude.join("demo").join("SKILL.md")).unwrap(),
        "# v2\n"
    );
}

#[test]
fn force_foreign_link_remains_conflict() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    let foreign = root.path().join("foreign");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&claude).unwrap();
    write_skill(&source, "demo", "# source\n");
    write_skill(&foreign, "demo", "# foreign\n");

    let link = claude.join("demo");
    if !try_symlink_dir(&foreign.join("demo"), &link) {
        return;
    }
    let svc = make_svc(source, claude);
    for force in [false, true] {
        let err = svc.sync("demo", AgentId::Claude, force).unwrap_err();
        assert_eq!(err.code(), "skill.conflict", "force={force}");
        assert!(link.exists(), "foreign link must remain (force={force})");
    }
}

#[test]
fn malformed_marker_blocks_delete_and_records_conflict() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# s\n");
    write_skill(&claude, "demo", "# s\n");

    // Hand-written garbage marker
    let marker = ownership_marker_path(&claude, "demo");
    fs::create_dir_all(marker.parent().unwrap()).unwrap();
    fs::write(&marker, b"{not-json").unwrap();

    let (_db_dir, db) = tmp_db();
    let svc = make_svc_db(source.clone(), claude.clone(), db.clone());
    let err = svc.disable("demo", AgentId::Claude).unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    assert!(claude.join("demo").join("SKILL.md").is_file());
    assert!(marker.is_file());

    // Assignment path: desired disable records observed conflict
    let mut reg = AdapterRegistry::new();
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Claude,
        skills_root: Some(claude.clone()),
    }));
    let targets = SkillTargetRegistry::from_adapter_registry(&reg).unwrap();
    let repo = SkillRepo::new(db);
    let assign = SkillAssignmentService::new(repo.clone());
    let reconciler = SkillReconciler::new(source, targets, repo.clone());
    let now = "t0";
    assign.ensure_package("demo", None, now).unwrap();
    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, false, None, now)
        .unwrap();
    let err = reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, now)
        .unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    let a = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(a.observed_status, "conflict");
}

#[test]
fn link_fallback_copy_writes_marker_and_disables() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&claude).unwrap();
    write_skill(&source, "demo", "# fallback body\n");

    // Simulate create_projection_link copy fallback (SkillLinkKind::None).
    let files = validate_and_collect_source(&source.join("demo"), "demo").unwrap();
    materialize_projection(&claude, "demo", &claude.join("demo"), &files, None).unwrap();
    finalize_link_projection_ownership(&claude, "demo", &claude.join("demo"), true, "1").unwrap();

    assert!(
        ownership_marker_path(&claude, "demo").is_file(),
        "copy fallback must write ownership marker"
    );

    let svc = make_svc(source, claude.clone());
    svc.disable("demo", AgentId::Claude).unwrap();
    assert!(!claude.join("demo").exists());
    assert!(!ownership_marker_path(&claude, "demo").exists());
}

#[test]
fn non_default_revision_in_marker_and_bootstrap() {
    use crate::platform::skills::ownership::{fingerprint_tree_at, write_copy_ownership_marker};

    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# body\n");

    // Lock with non-default version.
    let lock = serde_json::json!({
        "demo": {
            "kind": "local",
            "locator": "/tmp/demo",
            "version": "rev-42",
            "installedAt": "t0",
            "updatedAt": null
        }
    });
    fs::write(
        source.join(".skill-lock.json"),
        serde_json::to_string_pretty(&lock).unwrap(),
    )
    .unwrap();

    let svc = make_svc(source.clone(), claude.clone());
    svc.sync("demo", AgentId::Claude, false).unwrap();

    let marker_raw = fs::read_to_string(ownership_marker_path(&claude, "demo")).unwrap();
    let marker: serde_json::Value = serde_json::from_str(&marker_raw).unwrap();
    assert_eq!(marker["appliedRevision"], "rev-42");

    // Bootstrap import with matching revision.
    let (_db_dir, db) = tmp_db();
    let mut targets = SkillTargetRegistry::new();
    targets
        .register(Arc::new(StaticSkillTarget {
            agent_key: AgentKey::from_agent_id(AgentId::Claude),
            skills_root: Some(claude.clone()),
            supports: true,
        }))
        .unwrap();
    let repo = SkillRepo::new(db);
    // Fresh skills root content already has marker from sync; bootstrap into DB.
    let report = bootstrap_skill_assignments(&source, &targets, &repo, "now").unwrap();
    assert!(report.assignments_imported >= 1);
    let a = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(a.applied_revision.as_deref(), Some("rev-42"));
    assert_eq!(a.observed_status, "applied");

    // Also prove hand-written marker with rev-42 is accepted for import path.
    let fp = fingerprint_tree_at(&claude.join("demo")).unwrap();
    write_copy_ownership_marker(&claude, "demo", "rev-42", &fp).unwrap();
    assert!(is_managed_projection(
        &source.join("demo"),
        &claude,
        "demo",
        &claude.join("demo"),
        Some("rev-42")
    ));
}

#[test]
fn reconcile_unproject_conflict_records_observed_conflict() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill(&source, "demo", "# s\n");
    // Unmanaged foreign directory
    write_skill(&claude, "demo", "# foreign\n");

    let (_db_dir, db) = tmp_db();
    let mut reg = AdapterRegistry::new();
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Claude,
        skills_root: Some(claude.clone()),
    }));
    let targets = SkillTargetRegistry::from_adapter_registry(&reg).unwrap();
    let repo = SkillRepo::new(db);
    let assign = SkillAssignmentService::new(repo.clone());
    let reconciler = SkillReconciler::new(source, targets, repo.clone());

    let now = "t0";
    assign.ensure_package("demo", None, now).unwrap();
    // Pretend desired disabled so unproject runs
    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, false, None, now)
        .unwrap();

    let err = reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, now)
        .unwrap_err();
    assert_eq!(err.code(), "skill.conflict");

    let a = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(a.observed_status, "conflict");
    assert!(!a.desired_enabled);
    assert!(claude.join("demo").join("SKILL.md").is_file());
}

#[test]
fn ownership_store_dir_layout() {
    let root = tempfile::tempdir().unwrap();
    let skills = root.path().join("skills");
    assert_eq!(
        ownership_store_dir(&skills),
        skills.join(".agenthub").join("skill-ownership")
    );
    assert_eq!(
        ownership_marker_path(&skills, "my-skill"),
        skills
            .join(".agenthub")
            .join("skill-ownership")
            .join("my-skill.json")
    );
}
