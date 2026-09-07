use super::*;
use std::fs;
use tempfile::tempdir;

fn hub_tmp() -> (tempfile::TempDir, AgentHub) {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    (dir, hub)
}

#[test]
fn list_empty_and_invalid_agent() {
    let (_dir, hub) = hub_tmp();
    let items = list_backups_inner(&hub, None).unwrap();
    assert!(items.is_empty());
    let err = create_backup_inner(&hub, "bad", None).unwrap_err();
    assert!(err.contains("invalid agent"));
}

#[test]
fn create_then_list_refresh_and_delete() {
    let (_dir, hub) = hub_tmp();
    match create_backup_inner(&hub, "claude", Some("gui-test-manual")) {
        Ok(record) => {
            let listed = list_backups_inner(&hub, Some("claude")).unwrap();
            assert!(
                listed.iter().any(|b| b.record.id == record.id),
                "list after create must include new backup"
            );
            delete_backup_inner(&hub, &record.id).unwrap();
            let after = list_backups_inner(&hub, Some("claude")).unwrap();
            assert!(!after.iter().any(|b| b.record.id == record.id));
        }
        Err(err) => {
            let lower = err.to_lowercase();
            assert!(
                lower.contains("not") || lower.contains("no backupable"),
                "unexpected create error: {err}"
            );
        }
    }

    let err = delete_backup_inner(&hub, "00000000-0000-0000-0000-000000000000").unwrap_err();
    assert!(
        err.to_lowercase().contains("not found") || err.contains("backup"),
        "unexpected delete err: {err}"
    );
    let _ = fs::metadata(_dir.path());
}

#[test]
fn delete_missing_maps_error() {
    let (_dir, hub) = hub_tmp();
    let err = delete_backup_inner(&hub, "does-not-exist").unwrap_err();
    assert!(
        err.to_lowercase().contains("not found") || err.contains("backup"),
        "unexpected err: {err}"
    );
}

#[test]
fn restore_missing_maps_error() {
    let (_dir, hub) = hub_tmp();
    let err = restore_backup_inner(&hub, "missing-id").unwrap_err();
    assert!(
        err.to_lowercase().contains("not found") || err.contains("backup"),
        "unexpected err: {err}"
    );
}

#[test]
fn list_filter_invalid_agent() {
    let (_dir, hub) = hub_tmp();
    let err = list_backups_inner(&hub, Some("xyz")).unwrap_err();
    assert!(err.contains("invalid agent"));
}

#[test]
fn inspect_missing_maps_error() {
    let (_dir, hub) = hub_tmp();
    let err = inspect_backup_inner(&hub, "missing-id").unwrap_err();
    assert!(
        err.to_lowercase().contains("not found") || err.contains("backup"),
        "unexpected err: {err}"
    );
}
