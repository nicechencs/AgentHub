//! Skill package/assignment repository tests (separate file).

use crate::storage::{Database, SkillAssignmentRow, SkillPackageRow, SkillRepo};

fn tmp_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    (dir, db)
}

fn sample_package(id: &str) -> SkillPackageRow {
    SkillPackageRow {
        id: id.into(),
        source_kind: "local".into(),
        locator: "/tmp/skill".into(),
        revision: "rev-1".into(),
        manifest_json: r#"{"kind":"local"}"#.into(),
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

#[test]
fn migration_creates_skill_tables() {
    let (_dir, db) = tmp_db();
    db.with_conn(|conn| {
        let packages: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='skill_packages'",
            [],
            |r| r.get(0),
        )?;
        let assignments: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='skill_assignments'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(packages, 1);
        assert_eq!(assignments, 1);
        let version: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = '00010_skill_assignments'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(version, 1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn package_upsert_get_list_delete() {
    let (_dir, db) = tmp_db();
    let repo = SkillRepo::new(db);
    let saved = repo.upsert_package(&sample_package("demo")).unwrap();
    assert_eq!(saved.id, "demo");
    assert_eq!(repo.get_package("demo").unwrap().unwrap().revision, "rev-1");

    let mut next = saved;
    next.revision = "rev-2".into();
    next.updated_at = "t1".into();
    let updated = repo.upsert_package(&next).unwrap();
    assert_eq!(updated.revision, "rev-2");
    assert_eq!(updated.created_at, "t0");

    assert_eq!(repo.list_packages().unwrap().len(), 1);
    repo.delete_package("demo").unwrap();
    assert!(repo.get_package("demo").unwrap().is_none());
}

#[test]
fn assignment_requires_package_and_valid_agent_key() {
    let (_dir, db) = tmp_db();
    let repo = SkillRepo::new(db);
    let row = SkillAssignmentRow {
        skill_package_id: "missing".into(),
        agent_key: "claude".into(),
        desired_enabled: true,
        projection_mode: "copy".into(),
        applied_revision: None,
        observed_status: "pending".into(),
        last_error: None,
        updated_at: "t0".into(),
    };
    assert_eq!(
        repo.upsert_assignment(&row).unwrap_err().code(),
        "not_found"
    );

    repo.upsert_package(&sample_package("demo")).unwrap();
    let bad = SkillAssignmentRow {
        skill_package_id: "demo".into(),
        agent_key: "NOT VALID".into(),
        desired_enabled: true,
        projection_mode: "copy".into(),
        applied_revision: None,
        observed_status: "pending".into(),
        last_error: None,
        updated_at: "t0".into(),
    };
    assert_eq!(
        repo.upsert_assignment(&bad).unwrap_err().code(),
        "invalid_arg"
    );
}

#[test]
fn assignment_upsert_observed_and_indexes() {
    let (_dir, db) = tmp_db();
    let repo = SkillRepo::new(db);
    repo.upsert_package(&sample_package("demo")).unwrap();

    let row = SkillAssignmentRow {
        skill_package_id: "demo".into(),
        agent_key: "claude".into(),
        desired_enabled: true,
        projection_mode: "copy".into(),
        applied_revision: None,
        observed_status: "pending".into(),
        last_error: None,
        updated_at: "t0".into(),
    };
    let saved = repo.upsert_assignment(&row).unwrap();
    assert!(saved.desired_enabled);
    assert_eq!(saved.observed_status, "pending");

    let observed = repo
        .update_observed("demo", "claude", "applied", Some("rev-1"), None, "t1")
        .unwrap();
    assert_eq!(observed.observed_status, "applied");
    assert_eq!(observed.applied_revision.as_deref(), Some("rev-1"));
    assert!(observed.desired_enabled);

    let for_agent = repo.list_assignments_for_agent("claude").unwrap();
    assert_eq!(for_agent.len(), 1);
    let for_skill = repo.list_assignments_for_skill("demo").unwrap();
    assert_eq!(for_skill.len(), 1);

    // desired flip keeps package
    let disabled = SkillAssignmentRow {
        desired_enabled: false,
        observed_status: "pending".into(),
        applied_revision: observed.applied_revision.clone(),
        updated_at: "t2".into(),
        ..row
    };
    let d = repo.upsert_assignment(&disabled).unwrap();
    assert!(!d.desired_enabled);
}
