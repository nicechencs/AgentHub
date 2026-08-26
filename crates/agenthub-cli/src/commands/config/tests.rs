use super::*;
use agenthub_core::AgentHub;

#[test]
fn visible_keys_cover_whitelist_and_app_version() {
    let keys = visible_config_keys();
    for expected in [
        "theme",
        "language",
        "log_level",
        "log_retention_days",
        "skill_market_source",
        "close_to_tray",
        "usage_collect_interval_min",
        "app_version",
    ] {
        assert!(keys.contains(&expected), "missing {expected}");
    }
}

#[test]
fn get_set_roundtrip_and_reject_secret_key() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    get(&hub, None, OutputFormat::Quiet).unwrap();
    get(&hub, Some("app_version"), OutputFormat::Quiet).unwrap();
    set(&hub, "theme", "dark", OutputFormat::Quiet).unwrap();
    assert_eq!(hub.settings().get("theme").unwrap().as_deref(), Some("dark"));
    assert_eq!(
        set(&hub, "api_key", "sk-secret", OutputFormat::Quiet)
            .unwrap_err()
            .code(),
        "invalid_arg"
    );
    assert_eq!(
        set(&hub, "app_version", "9.9.9", OutputFormat::Quiet)
            .unwrap_err()
            .code(),
        "invalid_arg"
    );
}

#[test]
fn path_quiet_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    path(&hub, OutputFormat::Quiet).unwrap();
}
