use super::*;
use crate::storage::Database;
use crate::utils::paths::ensure_data_layout;

fn svc_tmp() -> (tempfile::TempDir, SettingsService) {
    let dir = tempfile::tempdir().unwrap();
    ensure_data_layout(dir.path()).unwrap();
    let db = Database::open(&crate::utils::paths::db_path(dir.path())).unwrap();
    let svc = SettingsService::new(dir.path().to_path_buf(), db);
    (dir, svc)
}

#[test]
fn path_info_includes_logs_dir() {
    let (dir, svc) = svc_tmp();
    let info = svc.path_info();
    assert!(
        info.data_dir
            .contains(dir.path().file_name().unwrap().to_str().unwrap())
            || info.data_dir == dir.path().display().to_string()
    );
    assert!(info.logs_dir.ends_with("logs") || info.logs_dir.replace('\\', "/").ends_with("/logs"));
    assert!(
        info.db_path.ends_with("agenthub.db")
            || info.db_path.replace('\\', "/").ends_with("agenthub.db")
    );
}

#[test]
fn log_level_and_retention_roundtrip_and_validation() {
    let (_dir, svc) = svc_tmp();
    let defaults = svc.get_all().unwrap();
    assert_eq!(defaults.log_level, "info");
    assert_eq!(defaults.log_retention_days, 14);

    svc.set("log_level", "DEBUG").unwrap();
    svc.set("log_retention_days", "21").unwrap();
    let loaded = svc.get_all().unwrap();
    assert_eq!(loaded.log_level, "debug");
    assert_eq!(loaded.log_retention_days, 21);

    assert!(svc.set("log_level", "verbose").is_err());
    assert!(svc.set("log_retention_days", "0").is_err());
    assert!(svc.set("log_retention_days", "999").is_err());
    assert!(svc.set("not_a_key", "x").is_err());

    // invalid write must not clobber previous good values
    let after = svc.get_all().unwrap();
    assert_eq!(after.log_level, "debug");
    assert_eq!(after.log_retention_days, 21);

    assert_eq!(after.skill_market_source, "auto");
    svc.set("skill_market_source", "skillhub.cn").unwrap();
    assert_eq!(svc.get_all().unwrap().skill_market_source, "skillhub.cn");
    assert!(svc.set("skill_market_source", "nope").is_err());
    assert_eq!(svc.get_all().unwrap().skill_market_source, "skillhub.cn");

    assert!(svc.get_all().unwrap().close_to_tray);
    svc.set("close_to_tray", "false").unwrap();
    assert!(!svc.get_all().unwrap().close_to_tray);
    svc.set("close_to_tray", "1").unwrap();
    assert!(svc.get_all().unwrap().close_to_tray);
    assert!(svc.set("close_to_tray", "maybe").is_err());
    assert!(svc.get_all().unwrap().close_to_tray);
}

#[test]
fn usage_collect_interval_roundtrip_and_validation() {
    let (_dir, svc) = svc_tmp();
    assert_eq!(svc.get_all().unwrap().usage_collect_interval_min, None);

    svc.set("usage_collect_interval_min", "0").unwrap();
    assert_eq!(svc.get_all().unwrap().usage_collect_interval_min, Some(0));
    svc.set("usage_collect_interval_min", "45").unwrap();
    assert_eq!(svc.get_all().unwrap().usage_collect_interval_min, Some(45));
    svc.set("usage_collect_interval_min", "1440").unwrap();
    assert_eq!(
        svc.get_all().unwrap().usage_collect_interval_min,
        Some(1440)
    );

    assert!(svc.set("usage_collect_interval_min", "1441").is_err());
    assert!(svc.set("usage_collect_interval_min", "-1").is_err());
    assert!(svc.set("usage_collect_interval_min", "nope").is_err());
    assert_eq!(
        svc.get_all().unwrap().usage_collect_interval_min,
        Some(1440)
    );
}

#[test]
fn whitelist_get_and_theme() {
    let (_dir, svc) = svc_tmp();
    assert!(svc.get("log_level").unwrap().is_some());
    svc.set("theme", "dark").unwrap();
    assert_eq!(svc.get("theme").unwrap().as_deref(), Some("dark"));
}

#[test]
fn app_version_is_read_only() {
    let (_dir, svc) = svc_tmp();
    assert_eq!(
        svc.get("app_version").unwrap().as_deref(),
        Some(env!("CARGO_PKG_VERSION"))
    );
    let err = svc.set("app_version", "9.9.9").unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(err.to_string().contains("read-only"));
}
