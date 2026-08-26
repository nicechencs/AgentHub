use super::{load, save, snapshot_path};
use crate::DoctorReport;

fn empty_report() -> DoctorReport {
    DoctorReport {
        data_dir: "/tmp/agenthub-test".into(),
        runtimes: vec![],
        agents: vec![],
        capabilities: Default::default(),
        usage_health: vec![],
        paths: crate::models::PathInfo {
            data_dir: "/tmp/agenthub-test".into(),
            db_path: "/tmp/agenthub-test/db.sqlite".into(),
            backups_dir: "/tmp/agenthub-test/backups".into(),
            logs_dir: "/tmp/agenthub-test/logs".into(),
        },
        db_ok: true,
        ok: true,
        warnings: vec!["agent claude not installed".into()],
        version: "0.0.0-test".into(),
        locks: vec![],
    }
}

#[test]
fn doctor_snapshot_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let report = empty_report();
    save(dir.path(), &report);
    assert!(snapshot_path(dir.path()).is_file());
    let loaded = load(dir.path()).expect("snapshot");
    assert_eq!(loaded.version, report.version);
    assert_eq!(loaded.warnings, report.warnings);
    assert!(loaded.ok);
}

#[test]
fn doctor_snapshot_missing_is_none() {
    let dir = tempfile::tempdir().unwrap();
    assert!(load(dir.path()).is_none());
}

#[test]
fn doctor_snapshot_corrupt_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = snapshot_path(dir.path());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"not-json").unwrap();
    assert!(load(dir.path()).is_none());
}
