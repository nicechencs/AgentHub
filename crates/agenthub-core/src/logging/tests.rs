use super::*;
use std::io::Write;

use crate::storage::Database;
use crate::utils::paths::db_path;

#[test]
fn parse_level_accepts_canonical() {
    assert_eq!(parse_level("info").unwrap(), Level::INFO);
    assert_eq!(parse_level("WARN").unwrap(), Level::WARN);
    assert_eq!(parse_level("warning").unwrap(), Level::WARN);
    assert_eq!(parse_level("trace").unwrap(), Level::TRACE);
    assert!(parse_level("verbose").is_err());
    assert!(parse_level("").is_err());
}

#[test]
fn parse_retention_bounds() {
    assert_eq!(parse_retention_days("1").unwrap(), 1);
    assert_eq!(parse_retention_days("14").unwrap(), 14);
    assert_eq!(parse_retention_days("365").unwrap(), 365);
    assert!(parse_retention_days("0").is_err());
    assert!(parse_retention_days("999").is_err());
    assert!(parse_retention_days("x").is_err());
}

#[test]
fn parse_log_filename_date_variants() {
    assert_eq!(
        parse_log_filename_date("agenthub.2026-08-02"),
        Some(NaiveDate::from_ymd_opt(2026, 8, 2).unwrap())
    );
    assert_eq!(
        parse_log_filename_date("agenthub.2026-08-02.log"),
        Some(NaiveDate::from_ymd_opt(2026, 8, 2).unwrap())
    );
    assert_eq!(
        parse_log_filename_date("agenthub-2026-08-02.log"),
        Some(NaiveDate::from_ymd_opt(2026, 8, 2).unwrap())
    );
    assert!(parse_log_filename_date("other.log").is_none());
    assert!(parse_log_filename_date("agenthub").is_none());
}

#[test]
fn purge_deletes_old_files_keeps_recent() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("agenthub.2020-01-01");
    let mut f = fs::File::create(&old).unwrap();
    writeln!(f, "old").unwrap();

    let today = Local::now().format("%Y-%m-%d");
    let recent = dir.path().join(format!("agenthub.{today}.log"));
    let mut f2 = fs::File::create(&recent).unwrap();
    writeln!(f2, "recent").unwrap();

    let old_log = dir.path().join("agenthub.2020-01-02.log");
    let mut f3 = fs::File::create(&old_log).unwrap();
    writeln!(f3, "old log").unwrap();

    // non-log file should be ignored (not counted as deleted)
    let other = dir.path().join("readme.txt");
    fs::write(&other, "x").unwrap();

    let stats = purge_old_logs(dir.path(), 14);
    assert_eq!(stats.deleted, 2);
    assert!(!old.exists());
    assert!(!old_log.exists());
    assert!(recent.exists());
    assert!(other.exists());
    assert!(stats.kept >= 1);
}

#[test]
fn load_log_prefs_defaults_when_db_missing() {
    let dir = tempfile::tempdir().unwrap();
    let (level, days) = load_log_prefs(dir.path());
    assert_eq!(level, "info");
    assert_eq!(days, 14);
}

#[test]
fn load_log_prefs_reads_settings_and_rejects_invalid_level() {
    let dir = tempfile::tempdir().unwrap();
    ensure_data_layout(dir.path()).unwrap();
    let db = Database::open(&db_path(dir.path())).unwrap();
    db.set_setting("log_level", "debug").unwrap();
    db.set_setting("log_retention_days", "30").unwrap();
    drop(db);

    let (level, days) = load_log_prefs(dir.path());
    assert_eq!(level, "debug");
    assert_eq!(days, 30);

    // invalid level falls back to default info while retention still reads
    let db = Database::open(&db_path(dir.path())).unwrap();
    db.set_setting("log_level", "nope").unwrap();
    db.set_setting("log_retention_days", "7").unwrap();
    drop(db);
    let (level2, days2) = load_log_prefs(dir.path());
    assert_eq!(level2, "info");
    assert_eq!(days2, 7);
}

#[test]
fn init_logging_is_idempotent_and_creates_logs_dir() {
    let dir = tempfile::tempdir().unwrap();
    ensure_data_layout(dir.path()).unwrap();
    let cfg = LogConfig {
        data_dir: dir.path().to_path_buf(),
        level: "info".into(),
        retention_days: 14,
        console: false,
        console_level: None,
        shell: "cli",
        version: "0.0.0-test",
    };
    // First call may succeed or no-op if process already initialized subscriber.
    let _ = init_logging(cfg.clone());
    // Second call must not error.
    init_logging(cfg).unwrap();
    assert!(logs_dir(dir.path()).is_dir());
}

#[test]
fn today_log_stem_format() {
    let stem = today_log_stem();
    assert!(stem.starts_with("agenthub."));
    assert!(stem.ends_with(".log"));
    assert_eq!(stem.len(), "agenthub.YYYY-MM-DD.log".len());
}

fn captured_has_op(logs: &str, op: &str) -> bool {
    logs.contains(&format!("op=\"{op}\""))
}

#[test]
fn chat_helpers_record_send_and_stop_ops() {
    let ((), logs) = with_captured_logs(|| {
        log_chat_info("send", "conv-1", Some("codex"), "send start");
        log_chat_info("send", "conv-1", Some("codex"), "send ok");
        log_chat_error(
            "send_fail",
            "conv-1",
            Some("codex"),
            Some("chat.runtime"),
            "send failed",
        );
        log_chat_info("stop", "conv-1", Some("codex"), "stop ok");
        log_chat_error("stop_fail", "conv-1", Some("codex"), None, "stop failed");
    });
    assert!(logs.contains("core.chat"), "logs:\n{logs}");
    assert!(captured_has_op(&logs, "send"), "logs:\n{logs}");
    assert!(captured_has_op(&logs, "send_fail"), "logs:\n{logs}");
    assert!(captured_has_op(&logs, "stop"), "logs:\n{logs}");
    assert!(captured_has_op(&logs, "stop_fail"), "logs:\n{logs}");
    assert!(logs.contains("conv-1"));
    assert!(logs.contains("codex"));
    assert!(!logs.contains("sk-"), "must not log keys: {logs}");
}
