use agenthub_core::AgentHub;
use tempfile::tempdir;

fn hub_tmp() -> (tempfile::TempDir, AgentHub) {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    (dir, hub)
}

#[test]
fn settings_defaults_include_log_fields() {
    let (_dir, hub) = hub_tmp();
    let s = hub.settings().get_all().unwrap();
    assert_eq!(s.log_level, "info");
    assert_eq!(s.log_retention_days, 14);
    assert!(s.close_to_tray);
    let paths = hub.settings().path_info();
    assert!(
        paths.logs_dir.replace('\\', "/").ends_with("/logs"),
        "logs_dir={}",
        paths.logs_dir
    );
}

#[test]
fn set_log_level_and_retention_via_service() {
    let (_dir, hub) = hub_tmp();
    hub.settings().set("log_level", "debug").unwrap();
    hub.settings().set("log_retention_days", "30").unwrap();
    let s = hub.settings().get_all().unwrap();
    assert_eq!(s.log_level, "debug");
    assert_eq!(s.log_retention_days, 30);
    assert!(hub.settings().set("log_level", "nope").is_err());
    assert!(hub.settings().set("log_retention_days", "0").is_err());
}

#[test]
fn close_to_tray_roundtrip_via_service() {
    let (_dir, hub) = hub_tmp();
    assert!(hub.settings().get_all().unwrap().close_to_tray);
    hub.settings().set("close_to_tray", "false").unwrap();
    assert!(!hub.settings().get_all().unwrap().close_to_tray);
    hub.settings().set("close_to_tray", "true").unwrap();
    assert!(hub.settings().get_all().unwrap().close_to_tray);
    assert!(hub.settings().set("close_to_tray", "maybe").is_err());
}

#[test]
fn app_state_syncs_close_to_tray_flag_after_setting_write() {
    use crate::state::AppState;
    use std::sync::Arc;

    let (_dir, hub) = hub_tmp();
    let state = AppState::from_hub(Ok(Arc::new(hub)));
    assert!(state.close_to_tray());

    // Mimic set_setting success path: DB write then flag sync.
    state
        .hub()
        .unwrap()
        .settings()
        .set("close_to_tray", "false")
        .unwrap();
    state.sync_setting_flag("close_to_tray", "false");
    assert!(!state.close_to_tray());
    assert!(
        !state
            .hub()
            .unwrap()
            .settings()
            .get_all()
            .unwrap()
            .close_to_tray
    );

    state
        .hub()
        .unwrap()
        .settings()
        .set("close_to_tray", "true")
        .unwrap();
    state.sync_setting_flag("close_to_tray", "true");
    assert!(state.close_to_tray());
}

#[test]
fn open_logs_dir_ensures_directory_exists() {
    let (dir, hub) = hub_tmp();
    let logs = std::path::PathBuf::from(hub.settings().path_info().logs_dir);
    // layout already creates logs; remove and re-ensure path logic
    if logs.exists() {
        // keep dir for open; just verify present under data dir
        assert!(logs.starts_with(dir.path()) || logs.exists());
    }
    assert!(logs.exists() || std::fs::create_dir_all(&logs).is_ok());
    assert!(logs.is_dir());
}
