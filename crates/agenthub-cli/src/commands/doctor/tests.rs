use super::*;
use agenthub_core::AgentHub;

#[test]
fn doctor_result_warnings_stay_success() {
    doctor_result(true).unwrap();
    assert_eq!(doctor_result(false).unwrap_err().code(), "doctor.failed");
}

#[test]
fn doctor_report_includes_lock_section_field() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    let report = hub.doctor();
    assert!(report.locks.is_empty());
    assert!(report.db_ok);
    let _ = run(&hub, OutputFormat::Quiet).unwrap();
}
