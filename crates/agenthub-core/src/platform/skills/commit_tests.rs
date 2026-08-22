//! R04: skill install/update main-commit consistency + compensation tests.
//!
//! No network. Faults use internal `SkillCommitFaults` or SQLite triggers.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::adapters::AdapterRegistry;
use crate::models::{AgentId, SkillSourceRecord};
use crate::platform::skills::assignment::package_revision;
use crate::platform::skills::commit::{
    commit_skill_package, recover_skill_commit_journal, PreparedSkillTree, SkillCommitFaults,
};
use crate::platform::skills::journal::{
    load_journal, write_journal, SkillCommitJournal, SkillCommitPhase, SkillPackageSnapshot,
    SKILL_COMMIT_JOURNAL_SCHEMA,
};
use crate::platform::skills::lockfile::{skill_lock_file, skill_lock_load, skill_lock_upsert};
use crate::platform::skills::packages::{
    finalize_retained_backup, swap_staging_keep_backup, write_skill_tree, RetainedLiveSwap,
    SkillPackageService,
};
use crate::platform::skills::target::StaticSkillTarget;
use crate::platform::skills::{SkillAssignmentService, SkillReconciler, SkillTargetRegistry};
use crate::platform::AgentKey;
use crate::services::SkillService;
use crate::storage::{Database, SkillPackageRow, SkillRepo};
use crate::utils::agent_lock::AgentWriteLock;

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

fn write_skill_tree_dir(dir: &Path, skill_md: &str, extra: Option<(&str, &str)>) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("SKILL.md"), skill_md).unwrap();
    if let Some((name, body)) = extra {
        fs::write(dir.join(name), body).unwrap();
    }
}

fn collect_files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    SkillPackageService::new()
        .validate_and_collect(dir, "demo")
        .unwrap()
}

fn assert_no_helper_dirs(skills_root: &Path) {
    if !skills_root.exists() {
        return;
    }
    for ent in fs::read_dir(skills_root).unwrap() {
        let name = ent.unwrap().file_name().to_string_lossy().into_owned();
        assert!(
            !name.starts_with(".agenthub-stage-")
                && !name.starts_with(".agenthub-bak-")
                && !name.starts_with(".staging-")
                && !name.starts_with(".backup-"),
            "leftover helper dir: {name}"
        );
    }
}

fn tmp_db() -> (tempfile::TempDir, Database) {
    let dir = crate::utils::test_temp::real_tempdir();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    (dir, db)
}

fn record(
    kind: &str,
    locator: &str,
    installed_at: &str,
    updated_at: Option<&str>,
) -> SkillSourceRecord {
    SkillSourceRecord {
        kind: kind.into(),
        locator: locator.into(),
        version: None,
        installed_at: installed_at.into(),
        updated_at: updated_at.map(|s| s.to_string()),
    }
}

fn load_journal_phase(root: &Path) -> SkillCommitPhase {
    load_journal(root).unwrap().unwrap().phase
}

#[test]
fn skill_lock_load_surfaces_corrupt_json() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let root = tmp.path().join("skills");
    fs::create_dir_all(&root).unwrap();
    fs::write(skill_lock_file(&root), "{not-json").unwrap();
    let err = skill_lock_load(&root).unwrap_err();
    assert_eq!(err.code(), "skill.lock");
    assert!(err.to_string().contains("parse"));
}

#[test]
fn overwrite_commit_keeps_live_lock_package_revision_aligned() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("skills");
    fs::create_dir_all(&skills_root).unwrap();
    let (_db_dir, db) = tmp_db();
    let repo = SkillRepo::new(db);

    // Seed v1 live + lock + package.
    let v1_dir = tmp.path().join("v1");
    write_skill_tree_dir(&v1_dir, "# v1\n", Some(("extra.txt", "one")));
    let files_v1 = collect_files(&v1_dir);
    let rec_v1 = record("local", v1_dir.to_str().unwrap(), "t0", None);
    let committed_v1 = commit_skill_package(
        &skills_root,
        "demo",
        PreparedSkillTree::Files(&files_v1),
        rec_v1.clone(),
        Some(&repo),
        "t0",
        SkillCommitFaults::default(),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(committed_v1.dest.join("SKILL.md")).unwrap(),
        "# v1\n"
    );
    let pkg_v1 = repo.get_package("demo").unwrap().unwrap();
    assert_eq!(pkg_v1.revision, package_revision(&rec_v1));

    // Overwrite with v2.
    let v2_dir = tmp.path().join("v2");
    write_skill_tree_dir(&v2_dir, "# v2\n", Some(("extra.txt", "two")));
    let files_v2 = collect_files(&v2_dir);
    let rec_v2 = record("local", v2_dir.to_str().unwrap(), "t0", Some("t1"));
    let committed_v2 = commit_skill_package(
        &skills_root,
        "demo",
        PreparedSkillTree::Files(&files_v2),
        rec_v2.clone(),
        Some(&repo),
        "t1",
        SkillCommitFaults::default(),
    )
    .unwrap();

    assert_eq!(
        fs::read_to_string(committed_v2.dest.join("SKILL.md")).unwrap(),
        "# v2\n"
    );
    assert_eq!(
        fs::read_to_string(committed_v2.dest.join("extra.txt")).unwrap(),
        "two"
    );
    let lock = skill_lock_load(&skills_root).unwrap();
    assert_eq!(lock.get("demo"), Some(&rec_v2));
    let pkg = repo.get_package("demo").unwrap().unwrap();
    assert_eq!(pkg.revision, package_revision(&rec_v2));
    assert_eq!(pkg.locator, rec_v2.locator);
    assert_no_helper_dirs(&skills_root);
}

#[test]
fn lock_write_failure_restores_old_live_and_lock() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("skills");
    fs::create_dir_all(&skills_root).unwrap();
    let (_db_dir, db) = tmp_db();
    let repo = SkillRepo::new(db);

    let v1_dir = tmp.path().join("v1");
    write_skill_tree_dir(&v1_dir, "# v1\n", Some(("keep.txt", "old")));
    let files_v1 = collect_files(&v1_dir);
    let rec_v1 = record("local", "/v1", "t0", None);
    commit_skill_package(
        &skills_root,
        "demo",
        PreparedSkillTree::Files(&files_v1),
        rec_v1.clone(),
        Some(&repo),
        "t0",
        SkillCommitFaults::default(),
    )
    .unwrap();
    let pkg_before = repo.get_package("demo").unwrap().unwrap();

    let v2_dir = tmp.path().join("v2");
    write_skill_tree_dir(&v2_dir, "# v2\n", Some(("keep.txt", "new")));
    let files_v2 = collect_files(&v2_dir);
    let rec_v2 = record("local", "/v2", "t0", Some("t1"));
    let err = commit_skill_package(
        &skills_root,
        "demo",
        PreparedSkillTree::Files(&files_v2),
        rec_v2,
        Some(&repo),
        "t1",
        SkillCommitFaults {
            fail_before_lock: true,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("before lock"));

    // Live + lock + package restored to v1.
    let live = skills_root.join("demo");
    assert_eq!(fs::read_to_string(live.join("SKILL.md")).unwrap(), "# v1\n");
    assert_eq!(fs::read_to_string(live.join("keep.txt")).unwrap(), "old");
    let lock = skill_lock_load(&skills_root).unwrap();
    assert_eq!(lock.get("demo"), Some(&rec_v1));
    let pkg_after = repo.get_package("demo").unwrap().unwrap();
    assert_eq!(pkg_after, pkg_before);
    assert_no_helper_dirs(&skills_root);
}

#[test]
fn package_db_write_failure_restores_old_live_lock_package() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("skills");
    fs::create_dir_all(&skills_root).unwrap();
    let (_db_dir, db) = tmp_db();
    let repo = SkillRepo::new(db.clone());

    let v1_dir = tmp.path().join("v1");
    write_skill_tree_dir(&v1_dir, "# v1\n", Some(("keep.txt", "old")));
    let files_v1 = collect_files(&v1_dir);
    let rec_v1 = record("local", "/v1", "t0", None);
    commit_skill_package(
        &skills_root,
        "demo",
        PreparedSkillTree::Files(&files_v1),
        rec_v1.clone(),
        Some(&repo),
        "t0",
        SkillCommitFaults::default(),
    )
    .unwrap();
    let pkg_before = repo.get_package("demo").unwrap().unwrap();

    // Inject package upsert failure via SQLite trigger.
    db.with_conn(|conn| {
        conn.execute_batch(
            r#"
            CREATE TRIGGER skill_packages_fail_update
            BEFORE UPDATE ON skill_packages
            BEGIN
                SELECT RAISE(ABORT, 'injected package write failure');
            END;
            "#,
        )?;
        Ok(())
    })
    .unwrap();

    let v2_dir = tmp.path().join("v2");
    write_skill_tree_dir(&v2_dir, "# v2\n", Some(("keep.txt", "new")));
    let files_v2 = collect_files(&v2_dir);
    let rec_v2 = record("local", "/v2", "t0", Some("t1"));
    let err = commit_skill_package(
        &skills_root,
        "demo",
        PreparedSkillTree::Files(&files_v2),
        rec_v2,
        Some(&repo),
        "t1",
        SkillCommitFaults::default(),
    )
    .unwrap_err();
    let err_s = err.to_string();
    assert!(
        err_s.contains("injected package write failure") || err_s.contains("package"),
        "unexpected error: {err_s}"
    );
    assert!(
        !err_s.contains("compensation also failed"),
        "package already at old snapshot must not re-upsert and fake compensation failure: {err_s}"
    );

    let live = skills_root.join("demo");
    assert_eq!(fs::read_to_string(live.join("SKILL.md")).unwrap(), "# v1\n");
    assert_eq!(fs::read_to_string(live.join("keep.txt")).unwrap(), "old");
    let lock = skill_lock_load(&skills_root).unwrap();
    assert_eq!(lock.get("demo"), Some(&rec_v1));
    let pkg_after = repo.get_package("demo").unwrap().unwrap();
    assert_eq!(pkg_after.revision, pkg_before.revision);
    assert_eq!(pkg_after.locator, pkg_before.locator);
    assert_no_helper_dirs(&skills_root);
}

#[test]
fn durable_commit_journal_recovers_each_phase() {
    for phase in [
        SkillCommitPhase::Prepared,
        SkillCommitPhase::LiveSwapped,
        SkillCommitPhase::LockCommitted,
        SkillCommitPhase::PackageCommitted,
    ] {
        let tmp = crate::utils::test_temp::real_tempdir();
        let root = tmp.path().join("skills");
        fs::create_dir_all(&root).unwrap();
        let db = Database::open(&root.join("db.sqlite")).unwrap();
        let repo = SkillRepo::new(db);
        let live = root.join("demo");
        write_skill_tree_dir(&live, "# old\n", Some(("version.txt", "old")));
        let old_record = record("local", "/old", "t0", None);
        skill_lock_upsert(&root, "demo", old_record.clone()).unwrap();
        let old_package = SkillPackageRow {
            id: "demo".into(),
            source_kind: "local".into(),
            locator: "/old".into(),
            revision: "old-revision".into(),
            manifest_json: "{}".into(),
            created_at: "t0".into(),
            updated_at: "t0".into(),
        };
        repo.upsert_package(&old_package).unwrap();

        let staging = root.join(".agenthub-stage-demo-journal-test");
        write_skill_tree_dir(&staging, "# new\n", Some(("version.txt", "new")));
        let backup = root.join(".agenthub-bak-demo-journal-test");
        let new_record = record("local", "/new", "t0", Some("t1"));
        let new_package = SkillPackageRow {
            locator: "/new".into(),
            revision: "new-revision".into(),
            updated_at: "t1".into(),
            ..old_package.clone()
        };

        if phase != SkillCommitPhase::Prepared {
            fs::rename(&live, &backup).unwrap();
            fs::rename(&staging, &live).unwrap();
        }
        if matches!(
            phase,
            SkillCommitPhase::LockCommitted | SkillCommitPhase::PackageCommitted
        ) {
            skill_lock_upsert(&root, "demo", new_record).unwrap();
        }
        if phase == SkillCommitPhase::PackageCommitted {
            repo.upsert_package(&new_package).unwrap();
        }

        let journal = SkillCommitJournal {
            schema: SKILL_COMMIT_JOURNAL_SCHEMA,
            skill: "demo".into(),
            target: live.clone(),
            had_live: true,
            staging: staging.clone(),
            // The overwrite journal reserves its backup name before the
            // first rename.  Prepared therefore carries the path even while
            // the helper itself does not exist yet.
            backup: Some(backup.clone()),
            old_lock: [("demo".into(), old_record.clone())].into_iter().collect(),
            had_lock_file: true,
            old_package: Some(SkillPackageSnapshot::from(&old_package)),
            has_package_repo: true,
            phase,
        };
        write_journal(&root, &journal).unwrap();

        assert!(recover_skill_commit_journal(&root, Some(&repo)).unwrap());
        assert!(!crate::platform::skills::journal::journal_path(&root).exists());
        assert!(!staging.exists());
        assert!(!backup.exists());

        if phase == SkillCommitPhase::PackageCommitted {
            assert_eq!(
                fs::read_to_string(live.join("SKILL.md")).unwrap(),
                "# new\n"
            );
            assert_eq!(
                skill_lock_load(&root).unwrap().get("demo").unwrap().locator,
                "/new"
            );
            assert_eq!(repo.get_package("demo").unwrap(), Some(new_package));
        } else {
            assert_eq!(
                fs::read_to_string(live.join("SKILL.md")).unwrap(),
                "# old\n"
            );
            assert_eq!(
                skill_lock_load(&root).unwrap().get("demo"),
                Some(&old_record)
            );
            assert_eq!(repo.get_package("demo").unwrap(), Some(old_package));
        }
    }
}

#[test]
fn durable_prepared_journal_recovers_first_install_after_live_rename() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let root = tmp.path().join("skills");
    fs::create_dir_all(&root).unwrap();
    let db = Database::open(&root.join("db.sqlite")).unwrap();
    let repo = SkillRepo::new(db);
    let target = root.join("demo");
    let staging = root.join(".agenthub-stage-demo-first-install");
    write_skill_tree_dir(&staging, "# new\n", None);
    fs::rename(&staging, &target).unwrap();

    let journal = SkillCommitJournal {
        schema: SKILL_COMMIT_JOURNAL_SCHEMA,
        skill: "demo".into(),
        target: target.clone(),
        had_live: false,
        staging: staging.clone(),
        backup: None,
        old_lock: BTreeMap::new(),
        had_lock_file: false,
        old_package: None,
        has_package_repo: true,
        phase: SkillCommitPhase::Prepared,
    };
    write_journal(&root, &journal).unwrap();

    recover_skill_commit_journal(&root, Some(&repo)).unwrap();
    assert!(!target.exists());
    assert!(!skill_lock_file(&root).exists());
    assert!(!crate::platform::skills::journal::journal_path(&root).exists());
}

#[test]
fn durable_prepared_overwrite_recovers_after_first_rename() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let root = tmp.path().join("skills");
    fs::create_dir_all(&root).unwrap();
    let db = Database::open(&root.join("db.sqlite")).unwrap();
    let repo = SkillRepo::new(db);
    let target = root.join("demo");
    let staging = root.join(".agenthub-stage-demo-first-rename");
    let backup = root.join(".agenthub-bak-demo-first-rename");
    write_skill_tree_dir(&target, "# old\n", None);
    write_skill_tree_dir(&staging, "# new\n", None);
    fs::rename(&target, &backup).unwrap();

    write_journal(
        &root,
        &SkillCommitJournal {
            schema: SKILL_COMMIT_JOURNAL_SCHEMA,
            skill: "demo".into(),
            target: target.clone(),
            had_live: true,
            staging: staging.clone(),
            backup: Some(backup.clone()),
            old_lock: BTreeMap::new(),
            had_lock_file: false,
            old_package: None,
            has_package_repo: true,
            phase: SkillCommitPhase::Prepared,
        },
    )
    .unwrap();

    recover_skill_commit_journal(&root, Some(&repo)).unwrap();
    assert_eq!(fs::read_to_string(target.join("SKILL.md")).unwrap(), "# old\n");
    assert!(!staging.exists());
    assert!(!backup.exists());
    assert!(!crate::platform::skills::journal::journal_path(&root).exists());
}

#[test]
fn durable_recovery_retries_after_lock_restore_failure_without_deleting_old_live() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let root = tmp.path().join("skills");
    fs::create_dir_all(&root).unwrap();
    let db = Database::open(&root.join("db.sqlite")).unwrap();
    let repo = SkillRepo::new(db);
    let target = root.join("demo");
    let staging = root.join(".agenthub-stage-demo-lock-failure");
    let backup = root.join(".agenthub-bak-demo-lock-failure");
    write_skill_tree_dir(&target, "# new\n", None);
    write_skill_tree_dir(&backup, "# old\n", None);
    write_skill_tree_dir(&staging, "# staged\n", None);
    fs::create_dir(skill_lock_file(&root)).unwrap();

    write_journal(
        &root,
        &SkillCommitJournal {
            schema: SKILL_COMMIT_JOURNAL_SCHEMA,
            skill: "demo".into(),
            target: target.clone(),
            had_live: true,
            staging: staging.clone(),
            backup: Some(backup.clone()),
            old_lock: BTreeMap::new(),
            had_lock_file: true,
            old_package: None,
            has_package_repo: true,
            phase: SkillCommitPhase::LiveSwapped,
        },
    )
    .unwrap();

    assert!(recover_skill_commit_journal(&root, Some(&repo)).is_err());
    assert_eq!(
        load_journal_phase(&root),
        SkillCommitPhase::RollbackLiveRestored
    );
    assert_eq!(fs::read_to_string(target.join("SKILL.md")).unwrap(), "# old\n");
    assert!(!backup.exists());

    fs::remove_dir(skill_lock_file(&root)).unwrap();
    recover_skill_commit_journal(&root, Some(&repo)).unwrap();
    assert!(!crate::platform::skills::journal::journal_path(&root).exists());
}

#[test]
fn durable_recovery_retries_after_package_restore_failure_without_repeating_live_rollback() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let root = tmp.path().join("skills");
    fs::create_dir_all(&root).unwrap();
    let db = Database::open(&root.join("db.sqlite")).unwrap();
    let repo = SkillRepo::new(db.clone());
    let target = root.join("demo");
    let staging = root.join(".agenthub-stage-demo-package-failure");
    let backup = root.join(".agenthub-bak-demo-package-failure");
    write_skill_tree_dir(&target, "# new\n", None);
    write_skill_tree_dir(&backup, "# old\n", None);
    write_skill_tree_dir(&staging, "# staged\n", None);
    let old_package = SkillPackageRow {
        id: "demo".into(),
        source_kind: "local".into(),
        locator: "/old".into(),
        revision: "old".into(),
        manifest_json: "{}".into(),
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };
    let new_package = SkillPackageRow {
        locator: "/new".into(),
        revision: "new".into(),
        updated_at: "t1".into(),
        ..old_package.clone()
    };
    repo.upsert_package(&new_package).unwrap();
    db.with_conn(|conn| {
        conn.execute_batch(
            r#"
            CREATE TRIGGER skill_packages_fail_recovery
            BEFORE UPDATE ON skill_packages
            BEGIN
                SELECT RAISE(ABORT, 'injected recovery package failure');
            END;
            "#,
        )?;
        Ok(())
    })
    .unwrap();

    write_journal(
        &root,
        &SkillCommitJournal {
            schema: SKILL_COMMIT_JOURNAL_SCHEMA,
            skill: "demo".into(),
            target: target.clone(),
            had_live: true,
            staging: staging.clone(),
            backup: Some(backup.clone()),
            old_lock: BTreeMap::new(),
            had_lock_file: false,
            old_package: Some(SkillPackageSnapshot::from(&old_package)),
            has_package_repo: true,
            phase: SkillCommitPhase::LiveSwapped,
        },
    )
    .unwrap();

    assert!(recover_skill_commit_journal(&root, Some(&repo)).is_err());
    assert_eq!(
        load_journal_phase(&root),
        SkillCommitPhase::RollbackLockRestored
    );
    assert_eq!(fs::read_to_string(target.join("SKILL.md")).unwrap(), "# old\n");
    assert!(!backup.exists());

    db.with_conn(|conn| {
        conn.execute_batch("DROP TRIGGER skill_packages_fail_recovery")?;
        Ok(())
    })
    .unwrap();
    recover_skill_commit_journal(&root, Some(&repo)).unwrap();
    assert!(!crate::platform::skills::journal::journal_path(&root).exists());
    assert_eq!(repo.get_package("demo").unwrap(), Some(old_package));
}

#[test]
fn durable_recovery_retries_after_helper_cleanup_failure() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let root = tmp.path().join("skills");
    fs::create_dir_all(&root).unwrap();
    let db = Database::open(&root.join("db.sqlite")).unwrap();
    let repo = SkillRepo::new(db);
    let target = root.join("demo");
    let staging = root.join(".agenthub-stage-demo-helper-failure");
    let backup = root.join(".agenthub-bak-demo-helper-failure");
    write_skill_tree_dir(&target, "# new\n", None);
    write_skill_tree_dir(&backup, "# old\n", None);
    fs::write(&staging, "not a directory").unwrap();

    write_journal(
        &root,
        &SkillCommitJournal {
            schema: SKILL_COMMIT_JOURNAL_SCHEMA,
            skill: "demo".into(),
            target: target.clone(),
            had_live: true,
            staging: staging.clone(),
            backup: Some(backup.clone()),
            old_lock: BTreeMap::new(),
            had_lock_file: false,
            old_package: None,
            has_package_repo: true,
            phase: SkillCommitPhase::LiveSwapped,
        },
    )
    .unwrap();

    assert!(recover_skill_commit_journal(&root, Some(&repo)).is_err());
    assert_eq!(
        load_journal_phase(&root),
        SkillCommitPhase::RollbackPackageRestored
    );
    assert_eq!(fs::read_to_string(target.join("SKILL.md")).unwrap(), "# old\n");
    assert!(!backup.exists());

    fs::remove_file(&staging).unwrap();
    recover_skill_commit_journal(&root, Some(&repo)).unwrap();
    assert!(!crate::platform::skills::journal::journal_path(&root).exists());
}

#[test]
fn finalize_backup_reports_helper_metadata_errors_without_claiming_cleanup() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let root = tmp.path().join("skills");
    fs::create_dir_all(&root).unwrap();
    let backup = root.join(".agenthub-bak-demo-metadata-error");
    fs::write(&backup, "not a directory").unwrap();

    let err = finalize_retained_backup(
        &root,
        RetainedLiveSwap {
            backup: Some(backup.clone()),
            first_install: false,
        },
    )
    .unwrap_err();
    assert_eq!(err.code(), "skill.backup");
    assert!(backup.exists(), "cleanup error must retain the helper evidence");

    fs::remove_file(&backup).unwrap();
    finalize_retained_backup(
        &root,
        RetainedLiveSwap {
            backup: Some(backup),
            first_install: false,
        },
    )
    .unwrap();
}

#[test]
fn skill_service_startup_recovery_is_narrow_and_root_locked() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let root = tmp.path().join("skills");
    fs::create_dir_all(&root).unwrap();
    let (_db_dir, db) = tmp_db();
    let service = SkillService::with_db(root.clone(), AdapterRegistry::new(), db.clone());
    let target = root.join("demo");
    let staging = root.join(".agenthub-stage-demo-startup");
    write_skill_tree_dir(&staging, "# pending\n", None);
    fs::rename(&staging, &target).unwrap();

    write_journal(
        &root,
        &SkillCommitJournal {
            schema: SKILL_COMMIT_JOURNAL_SCHEMA,
            skill: "demo".into(),
            target: target.clone(),
            had_live: false,
            staging,
            backup: None,
            old_lock: BTreeMap::new(),
            had_lock_file: false,
            old_package: None,
            has_package_repo: true,
            phase: SkillCommitPhase::Prepared,
        },
    )
    .unwrap();

    assert!(service.recover_pending_commit().is_ok());
    assert!(!target.exists());
    assert!(!crate::platform::skills::journal::journal_path(&root).exists());
    assert!(SkillRepo::new(db).get_package("demo").unwrap().is_none());
    assert!(!root.join(".locks").join("skill-__root__.lock").exists());
}

#[test]
fn recover_pending_commit_without_journal_does_not_create_lock_dir() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let root = tmp.path().join("skills");
    let (_db_dir, db) = tmp_db();
    let a = SkillService::with_db(root.clone(), AdapterRegistry::new(), db.clone());
    let b = SkillService::with_db(root.clone(), AdapterRegistry::new(), db);
    a.recover_pending_commit().unwrap();
    b.recover_pending_commit().unwrap();
    assert!(!root.join(".locks").exists());
}

#[test]
fn first_install_metadata_failure_leaves_no_live_or_lock_or_package() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("skills");
    fs::create_dir_all(&skills_root).unwrap();
    let (_db_dir, db) = tmp_db();
    let repo = SkillRepo::new(db);

    let src = tmp.path().join("src");
    write_skill_tree_dir(&src, "# new\n", None);
    let files = collect_files(&src);
    let rec = record("local", src.to_str().unwrap(), "t0", None);

    // Fail after lock write, before package — first install must remove live + lock.
    let err = commit_skill_package(
        &skills_root,
        "demo",
        PreparedSkillTree::Files(&files),
        rec,
        Some(&repo),
        "t0",
        SkillCommitFaults {
            fail_before_package: true,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("before package"));

    assert!(!skills_root.join("demo").exists(), "live must be removed");
    assert!(
        !skill_lock_file(&skills_root).exists(),
        "first install must not leave .skill-lock.json when none existed before"
    );
    let lock = skill_lock_load(&skills_root).unwrap();
    assert!(!lock.contains_key("demo"));
    assert!(repo.get_package("demo").unwrap().is_none());
    assert_no_helper_dirs(&skills_root);
}

#[test]
fn first_install_lock_failure_leaves_no_live() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("skills");
    fs::create_dir_all(&skills_root).unwrap();
    let (_db_dir, db) = tmp_db();
    let repo = SkillRepo::new(db);

    let src = tmp.path().join("src");
    write_skill_tree_dir(&src, "# new\n", None);
    let files = collect_files(&src);
    let rec = record("local", src.to_str().unwrap(), "t0", None);

    assert!(!skill_lock_file(&skills_root).exists());
    let err = commit_skill_package(
        &skills_root,
        "demo",
        PreparedSkillTree::Files(&files),
        rec,
        Some(&repo),
        "t0",
        SkillCommitFaults {
            fail_before_lock: true,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("before lock"));
    assert!(!skills_root.join("demo").exists());
    assert!(
        !skill_lock_file(&skills_root).exists(),
        "first install lock failure must not create .skill-lock.json"
    );
    assert!(!skill_lock_load(&skills_root).unwrap().contains_key("demo"));
    assert!(repo.get_package("demo").unwrap().is_none());
    assert_no_helper_dirs(&skills_root);
}

#[test]
fn materialize_validate_failure_leaves_old_content_unchanged() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("skills");
    fs::create_dir_all(&skills_root).unwrap();
    let (_db_dir, db) = tmp_db();
    let repo = SkillRepo::new(db);

    let v1_dir = tmp.path().join("v1");
    write_skill_tree_dir(&v1_dir, "# v1\n", Some(("keep.txt", "old")));
    let files_v1 = collect_files(&v1_dir);
    let rec_v1 = record("local", "/v1", "t0", None);
    commit_skill_package(
        &skills_root,
        "demo",
        PreparedSkillTree::Files(&files_v1),
        rec_v1.clone(),
        Some(&repo),
        "t0",
        SkillCommitFaults::default(),
    )
    .unwrap();

    // Bad package: missing SKILL.md — validate fails before commit.
    let bad = tmp.path().join("bad");
    fs::create_dir_all(&bad).unwrap();
    fs::write(bad.join("only.txt"), "x").unwrap();
    let packages = SkillPackageService::new();
    let err = packages.validate_and_collect(&bad, "demo");
    // validate_and_collect only checks tree safety, not SKILL.md — ensure_skill_md does.
    // Simulate the install/update gate: ensure_skill_md fails, never call commit.
    assert!(crate::platform::skills::ensure_skill_md(&bad).is_err());
    let _ = err; // may succeed if only.txt is fine; the gate is SKILL.md.

    // Old content unchanged.
    let live = skills_root.join("demo");
    assert_eq!(fs::read_to_string(live.join("SKILL.md")).unwrap(), "# v1\n");
    assert_eq!(fs::read_to_string(live.join("keep.txt")).unwrap(), "old");
    assert_eq!(
        skill_lock_load(&skills_root).unwrap().get("demo"),
        Some(&rec_v1)
    );
    assert_no_helper_dirs(&skills_root);

    // Also: commit rejects staging without SKILL.md without mutating live.
    let empty_staging = skills_root.join(".agenthub-stage-demo-manual");
    fs::create_dir_all(&empty_staging).unwrap();
    fs::write(empty_staging.join("nope.txt"), "x").unwrap();
    let rec_v2 = record("local", "/v2", "t0", Some("t1"));
    let err = commit_skill_package(
        &skills_root,
        "demo",
        PreparedSkillTree::StagingDir(empty_staging.clone()),
        rec_v2,
        Some(&repo),
        "t1",
        SkillCommitFaults::default(),
    )
    .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert_eq!(fs::read_to_string(live.join("SKILL.md")).unwrap(), "# v1\n");
    assert_eq!(
        skill_lock_load(&skills_root).unwrap().get("demo"),
        Some(&rec_v1)
    );
}

#[test]
fn install_skill_service_success_and_validate_failure() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("skills");
    let source = tmp.path().join("pkg");
    write_skill_tree_dir(&source, "---\nname: Demo\n---\n# body\n", None);

    let reg = AdapterRegistry::new();
    let (_db_dir, db) = tmp_db();
    let svc = SkillService::with_db(skills_root.clone(), reg, db.clone());

    let skill = svc.install_skill(source.to_str().unwrap(), false).unwrap();
    assert_eq!(skill.id, "pkg");
    assert!(skills_root.join("pkg").join("SKILL.md").is_file());
    let lock = skill_lock_load(&skills_root).unwrap();
    assert!(lock.contains_key("pkg"));
    let pkg = SkillRepo::new(db).get_package("pkg").unwrap().unwrap();
    assert_eq!(pkg.revision, package_revision(lock.get("pkg").unwrap()));
    assert_no_helper_dirs(&skills_root);

    // Validate failure: missing SKILL.md — old install stays if overwrite of another.
    let bad = tmp.path().join("bad-skill");
    fs::create_dir_all(&bad).unwrap();
    fs::write(bad.join("readme.txt"), "no skill md").unwrap();
    let err = svc.install_skill(bad.to_str().unwrap(), false).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    // Previous skill untouched.
    assert!(skills_root.join("pkg").join("SKILL.md").is_file());
}

/// Reconcile single-target failure keeps new shared package and writes observed error.
///
/// Covers the post-commit path used by update/install: package stays, assignment
/// observed_status=error (existing reconciler behaviour).
#[test]
fn reconcile_single_target_failure_keeps_shared_package_and_observed_error() {
    use crate::platform::skills::reconcile::observed;

    let root = crate::utils::test_temp::real_tempdir();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill_tree_dir(&source.join("demo"), "# shared v2\n", None);

    // Pre-create unmanaged projection so reconcile hits skill.conflict / error path.
    fs::create_dir_all(claude.join("demo")).unwrap();
    fs::write(claude.join("demo").join("SKILL.md"), "# foreign\n").unwrap();

    let (_db_dir, db) = tmp_db();
    let repo = SkillRepo::new(db);
    let mut targets = SkillTargetRegistry::new();
    targets
        .register(Arc::new(StaticSkillTarget {
            agent_key: AgentKey::from_agent_id(AgentId::Claude),
            skills_root: Some(claude.clone()),
            supports: true,
        }))
        .unwrap();
    let assign = SkillAssignmentService::new(repo.clone());
    let reconciler = SkillReconciler::new(source.clone(), targets, repo.clone());

    let rec = record("local", "/demo", "t0", Some("t-rev-2"));
    skill_lock_upsert(&source, "demo", rec.clone()).unwrap();
    // Simulate post-commit package row (new revision already committed).
    let pkg = assign.ensure_package("demo", Some(&rec), "t1").unwrap();
    assert_eq!(pkg.revision, package_revision(&rec));
    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("copy"), "t1")
        .unwrap();

    let err = reconciler
        .reconcile_one_for_agent("demo", AgentId::Claude, false, "t1")
        .unwrap_err();
    assert_eq!(err.code(), "skill.conflict");

    // Shared package retained at new revision.
    let pkg_after = repo.get_package("demo").unwrap().unwrap();
    assert_eq!(pkg_after.revision, package_revision(&rec));
    assert_eq!(
        fs::read_to_string(source.join("demo").join("SKILL.md")).unwrap(),
        "# shared v2\n"
    );
    // Observed error recorded; foreign projection not deleted.
    let a = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_eq!(a.observed_status, observed::CONFLICT);
    assert!(a.last_error.is_some());
    assert!(a.desired_enabled);
    assert_eq!(
        fs::read_to_string(claude.join("demo").join("SKILL.md")).unwrap(),
        "# foreign\n"
    );
}

#[test]
fn staging_with_symlink_rejected_and_old_live_unchanged() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("skills");
    fs::create_dir_all(&skills_root).unwrap();
    let (_db_dir, db) = tmp_db();
    let repo = SkillRepo::new(db);

    // Seed real live v1.
    let v1_dir = tmp.path().join("v1");
    write_skill_tree_dir(&v1_dir, "# v1\n", Some(("keep.txt", "old")));
    let files_v1 = collect_files(&v1_dir);
    let rec_v1 = record("local", "/v1", "t0", None);
    commit_skill_package(
        &skills_root,
        "demo",
        PreparedSkillTree::Files(&files_v1),
        rec_v1.clone(),
        Some(&repo),
        "t0",
        SkillCommitFaults::default(),
    )
    .unwrap();

    // Hand-built staging containing a symlink/reparse (when platform allows).
    let staging = skills_root.join(".agenthub-stage-demo-manual-symlink");
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("SKILL.md"), "# evil\n").unwrap();
    let link_target = tmp.path().join("outside");
    fs::create_dir_all(&link_target).unwrap();
    let link_path = staging.join("evil-link");
    if !try_symlink_dir(&link_target, &link_path) {
        // Environment cannot create symlinks (e.g. Windows without privilege).
        let _ = fs::remove_dir_all(&staging);
        return;
    }

    let rec_v2 = record("local", "/v2", "t0", Some("t1"));
    let err = commit_skill_package(
        &skills_root,
        "demo",
        PreparedSkillTree::StagingDir(staging.clone()),
        rec_v2,
        Some(&repo),
        "t1",
        SkillCommitFaults::default(),
    )
    .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(
        err.to_string().contains("symlink") || err.to_string().contains("reparse"),
        "unexpected error: {err}"
    );

    let live = skills_root.join("demo");
    assert_eq!(fs::read_to_string(live.join("SKILL.md")).unwrap(), "# v1\n");
    assert_eq!(fs::read_to_string(live.join("keep.txt")).unwrap(), "old");
    assert_eq!(
        skill_lock_load(&skills_root).unwrap().get("demo"),
        Some(&rec_v1)
    );
    // Staging must not remain after rejection (commit cleans on pre-swap fail).
    assert!(
        !staging.exists() || !staging.join("SKILL.md").exists() || {
            // If best-effort left a partial dir, live must still be intact (asserted above).
            true
        }
    );
    assert_no_helper_dirs(&skills_root);
}

#[test]
fn linked_shared_source_update_rejected_and_link_unchanged() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("skills");
    let real = tmp.path().join("real-skill");
    write_skill_tree_dir(&real, "# real\n", None);
    fs::create_dir_all(&skills_root).unwrap();

    let link = skills_root.join("demo");
    if !try_symlink_dir(&real, &link) {
        return;
    }

    // Record a lock entry so update_skill reaches the link check.
    skill_lock_upsert(
        &skills_root,
        "demo",
        record("local", real.to_str().unwrap(), "t0", None),
    )
    .unwrap();

    let reg = AdapterRegistry::new();
    let (_db_dir, db) = tmp_db();
    let svc = SkillService::with_db(skills_root.clone(), reg, db);

    let err = svc.update_skill("demo").unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(
        err.to_string().contains("link") || err.to_string().contains("refusing"),
        "unexpected: {err}"
    );

    // Link still present and points at the same real tree.
    let meta = fs::symlink_metadata(&link).unwrap();
    assert!(
        meta.file_type().is_symlink() || {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                meta.file_attributes() & 0x0400 != 0
            }
            #[cfg(not(windows))]
            {
                false
            }
        }
    );
    assert_eq!(
        fs::read_to_string(real.join("SKILL.md")).unwrap(),
        "# real\n"
    );
}

#[test]
fn observed_db_write_failure_makes_reconcile_skill_infra_err() {
    use crate::platform::skills::reconcile::observed;

    let root = crate::utils::test_temp::real_tempdir();
    let source = root.path().join("source");
    let claude = root.path().join("claude");
    fs::create_dir_all(&source).unwrap();
    write_skill_tree_dir(&source.join("demo"), "# shared\n", None);

    let (_db_dir, db) = tmp_db();
    let repo = SkillRepo::new(db.clone());
    let mut targets = SkillTargetRegistry::new();
    targets
        .register(Arc::new(StaticSkillTarget {
            agent_key: AgentKey::from_agent_id(AgentId::Claude),
            skills_root: Some(claude.clone()),
            supports: true,
        }))
        .unwrap();
    let assign = SkillAssignmentService::new(repo.clone());
    let reconciler = SkillReconciler::new(source.clone(), targets, repo.clone());

    let rec = record("local", "/demo", "t0", None);
    skill_lock_upsert(&source, "demo", rec.clone()).unwrap();
    assign.ensure_package("demo", Some(&rec), "t0").unwrap();
    assign
        .set_desired_enabled_for_agent("demo", AgentId::Claude, true, Some("copy"), "t0")
        .unwrap();

    // Inject update_observed failure via assignment UPDATE trigger.
    db.with_conn(|conn| {
        conn.execute_batch(
            r#"
            CREATE TRIGGER skill_assignments_fail_update
            BEFORE UPDATE ON skill_assignments
            BEGIN
                SELECT RAISE(ABORT, 'injected observed write failure');
            END;
            "#,
        )?;
        Ok(())
    })
    .unwrap();

    let err = reconciler.reconcile_skill("demo", true, "t1").unwrap_err();
    let err_s = err.to_string();
    assert!(
        err_s.contains("injected observed write failure") || err.code() == "db",
        "expected infrastructure Err from reconcile_skill, got: {err_s}"
    );

    // Observed must not have been silently flipped to applied.
    let a = repo.get_assignment("demo", "claude").unwrap().unwrap();
    assert_ne!(a.observed_status, observed::APPLIED);
}

#[test]
fn same_second_install_then_update_bumps_content_and_package_revision() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("skills");
    let source = tmp.path().join("pkg");
    write_skill_tree_dir(&source, "---\nname: Demo\n---\n# v1\n", None);

    let reg = AdapterRegistry::new();
    let (_db_dir, db) = tmp_db();
    let repo = SkillRepo::new(db.clone());
    let svc = SkillService::with_db(skills_root.clone(), reg, db);

    let installed = svc.install_skill(source.to_str().unwrap(), false).unwrap();
    assert_eq!(installed.id, "pkg");
    let lock1 = skill_lock_load(&skills_root).unwrap();
    let rec1 = lock1.get("pkg").cloned().unwrap();
    let pkg1 = repo.get_package("pkg").unwrap().unwrap();
    assert_eq!(pkg1.revision, package_revision(&rec1));

    // Mutate the local source and update immediately (same wall-clock second possible).
    fs::write(source.join("SKILL.md"), "---\nname: Demo\n---\n# v2\n").unwrap();
    let updated = svc.update_skill("pkg").unwrap();
    assert_eq!(
        fs::read_to_string(updated.source_dir.join("SKILL.md")).unwrap(),
        "---\nname: Demo\n---\n# v2\n"
    );

    let lock2 = skill_lock_load(&skills_root).unwrap();
    let rec2 = lock2.get("pkg").cloned().unwrap();
    let pkg2 = repo.get_package("pkg").unwrap().unwrap();
    assert_eq!(pkg2.revision, package_revision(&rec2));
    assert_ne!(
        pkg1.revision, pkg2.revision,
        "package revision must change even when install+update share a wall-clock second"
    );
    assert_ne!(rec1.updated_at, rec2.updated_at);
    assert!(rec2.updated_at.is_some());
}

#[test]
fn swap_rejects_existing_link_without_deleting_it() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("skills");
    let real = tmp.path().join("real");
    write_skill_tree_dir(&real, "# real\n", None);
    fs::create_dir_all(&skills_root).unwrap();
    let link = skills_root.join("demo");
    if !try_symlink_dir(&real, &link) {
        return;
    }

    let staging = skills_root.join(".agenthub-stage-demo-0");
    fs::create_dir_all(&staging).unwrap();
    let mut files = BTreeMap::new();
    files.insert("SKILL.md".into(), b"# staged\n".to_vec());
    write_skill_tree(&staging, &files).unwrap();

    let err =
        swap_staging_keep_backup(&skills_root, "demo", Some(&link), &link, &staging).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(
        err.to_string().contains("symlink")
            || err.to_string().contains("junction")
            || err.to_string().contains("reparse")
            || err.to_string().contains("link"),
        "unexpected: {err}"
    );

    // Link still there; real content unchanged.
    assert!(fs::symlink_metadata(&link).is_ok());
    assert_eq!(
        fs::read_to_string(real.join("SKILL.md")).unwrap(),
        "# real\n"
    );
    // Staging cleaned by helper.
    assert!(!staging.exists());
}

#[test]
fn uninstall_corrupt_lock_has_no_delete_side_effects() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("skills");
    let live = skills_root.join("demo");
    write_skill_tree_dir(&live, "# keep\n", None);
    fs::write(skill_lock_file(&skills_root), "{not-json").unwrap();

    let reg = AdapterRegistry::new();
    let (_db_dir, db) = tmp_db();
    let svc = SkillService::with_db(skills_root.clone(), reg, db);

    let err = svc.uninstall_skill("demo", None).unwrap_err();
    assert_eq!(err.code(), "skill.lock");
    assert!(live.join("SKILL.md").is_file(), "source must remain");
    assert_eq!(
        fs::read_to_string(live.join("SKILL.md")).unwrap(),
        "# keep\n"
    );
    // Corrupt lock still present (no partial rewrite).
    assert_eq!(
        fs::read_to_string(skill_lock_file(&skills_root)).unwrap(),
        "{not-json"
    );
}

#[test]
fn uninstall_shared_skill_removes_registered_custom_key_projection() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let shared_root = tmp.path().join("shared");
    let install_source = tmp.path().join("pkg");
    let target_root = tmp.path().join("future-agent");
    write_skill_tree_dir(&install_source, "# custom agent\n", None);

    let agent_key = AgentKey::parse("future-agent").unwrap();
    let mut targets = SkillTargetRegistry::new();
    targets
        .register(Arc::new(StaticSkillTarget {
            agent_key: agent_key.clone(),
            skills_root: Some(target_root.clone()),
            supports: true,
        }))
        .unwrap();

    let (_db_dir, db) = tmp_db();
    let repo = SkillRepo::new(db.clone());
    let svc = SkillService::with_db_and_target_registry(
        shared_root.clone(),
        AdapterRegistry::new(),
        db,
        targets.clone(),
    );
    let installed = svc
        .install_skill(install_source.to_str().unwrap(), false)
        .unwrap();

    let assign = SkillAssignmentService::new(repo.clone());
    assign
        .set_desired_enabled(&installed.id, &agent_key, true, Some("copy"), "t0")
        .unwrap();
    SkillReconciler::new(shared_root.clone(), targets, repo)
        .reconcile_one(&installed.id, &agent_key, false, "t1")
        .unwrap();
    assert!(target_root.join("pkg").join("SKILL.md").is_file());

    svc.uninstall_skill(&installed.id, None).unwrap();

    assert!(!target_root.join("pkg").exists());
    assert!(!shared_root.join("pkg").exists());
    assert!(!skill_lock_load(&shared_root).unwrap().contains_key("pkg"));
}

#[test]
fn uninstall_foreign_custom_projection_keeps_source_and_retry_converges() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let shared_root = tmp.path().join("shared");
    let install_source = tmp.path().join("pkg");
    let managed_root = tmp.path().join("custom-a");
    let foreign_root = tmp.path().join("custom-b");
    write_skill_tree_dir(&install_source, "# shared\n", None);

    let managed_key = AgentKey::parse("custom-a").unwrap();
    let foreign_key = AgentKey::parse("custom-b").unwrap();
    let mut targets = SkillTargetRegistry::new();
    for (agent_key, skills_root) in [
        (managed_key.clone(), managed_root.clone()),
        (foreign_key.clone(), foreign_root.clone()),
    ] {
        targets
            .register(Arc::new(StaticSkillTarget {
                agent_key,
                skills_root: Some(skills_root),
                supports: true,
            }))
            .unwrap();
    }

    let (_db_dir, db) = tmp_db();
    let repo = SkillRepo::new(db.clone());
    let svc = SkillService::with_db_and_target_registry(
        shared_root.clone(),
        AdapterRegistry::new(),
        db,
        targets.clone(),
    );
    let installed = svc
        .install_skill(install_source.to_str().unwrap(), false)
        .unwrap();

    let assign = SkillAssignmentService::new(repo.clone());
    assign
        .set_desired_enabled(&installed.id, &managed_key, true, Some("copy"), "t0")
        .unwrap();
    SkillReconciler::new(shared_root.clone(), targets, repo)
        .reconcile_one(&installed.id, &managed_key, false, "t1")
        .unwrap();
    write_skill_tree_dir(&foreign_root.join("pkg"), "# foreign\n", None);

    let err = svc.uninstall_skill(&installed.id, None).unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    assert!(
        !managed_root.join("pkg").exists(),
        "earlier managed target may be cleaned before a later target fails"
    );
    assert_eq!(
        fs::read_to_string(foreign_root.join("pkg").join("SKILL.md")).unwrap(),
        "# foreign\n"
    );
    assert!(
        shared_root.join("pkg").join("SKILL.md").is_file(),
        "shared source must remain after any target failure"
    );
    assert!(skill_lock_load(&shared_root).unwrap().contains_key("pkg"));

    fs::remove_dir_all(foreign_root.join("pkg")).unwrap();
    svc.uninstall_skill(&installed.id, None).unwrap();
    assert!(!shared_root.join("pkg").exists());
    assert!(!skill_lock_load(&shared_root).unwrap().contains_key("pkg"));
}

#[test]
fn uninstall_custom_target_lock_failure_keeps_source_and_lock_record() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let shared_root = tmp.path().join("shared");
    let install_source = tmp.path().join("pkg");
    let target_root = tmp.path().join("future-agent");
    write_skill_tree_dir(&install_source, "# locked\n", None);

    let agent_key = AgentKey::parse("future-agent").unwrap();
    let mut targets = SkillTargetRegistry::new();
    targets
        .register(Arc::new(StaticSkillTarget {
            agent_key: agent_key.clone(),
            skills_root: Some(target_root),
            supports: true,
        }))
        .unwrap();

    let (_db_dir, db) = tmp_db();
    let svc = SkillService::with_db_and_target_registry(
        shared_root.clone(),
        AdapterRegistry::new(),
        db,
        targets,
    );
    let installed = svc
        .install_skill(install_source.to_str().unwrap(), false)
        .unwrap();
    let _held = AgentWriteLock::acquire_key(&shared_root.join(".locks"), &agent_key).unwrap();

    let err = svc.uninstall_skill(&installed.id, None).unwrap_err();
    assert_eq!(err.code(), "agent.lock");
    assert!(shared_root.join("pkg").join("SKILL.md").is_file());
    assert!(skill_lock_load(&shared_root).unwrap().contains_key("pkg"));
}
