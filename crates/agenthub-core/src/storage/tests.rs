use super::*;

#[test]
fn database_open_creates_schema_and_settings_roundtrip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("agenthub.db");
    let db = Database::open(&path).expect("open db");
    assert!(path.exists());
    db.ping().expect("ping");

    // Migration seeds theme/language/log_level/log_retention_days.
    assert_eq!(
        db.get_setting("theme").expect("get seeded").as_deref(),
        Some("system")
    );
    assert_eq!(
        db.get_setting("log_level")
            .expect("get log_level")
            .as_deref(),
        Some("info")
    );
    assert_eq!(
        db.get_setting("log_retention_days")
            .expect("get retention")
            .as_deref(),
        Some("14")
    );
    let settings = db.load_app_settings().expect("load settings");
    assert_eq!(settings.log_level, "info");
    assert_eq!(settings.log_retention_days, 14);
    assert_eq!(db.get_setting("missing_key").expect("get missing"), None);

    assert!(
        db.load_app_settings().expect("load").close_to_tray,
        "default close_to_tray is true when key missing"
    );
    assert!(
        db.load_app_settings()
            .expect("load copies")
            .keep_live_file_copies,
        "default keep_live_file_copies is true"
    );
    db.set_setting("keep_live_file_copies", "false")
        .expect("set copies off");
    assert!(
        !db.load_app_settings()
            .expect("load copies off")
            .keep_live_file_copies
    );
    db.set_setting("keep_live_file_copies", "true")
        .expect("set copies on");
    db.set_setting("close_to_tray", "false").expect("set close");
    assert!(!db.load_app_settings().expect("load false").close_to_tray);
    db.set_setting("close_to_tray", "true")
        .expect("set close true");
    assert!(db.load_app_settings().expect("load true").close_to_tray);
    // Loose false tokens
    db.set_setting("close_to_tray", "off").expect("set off");
    assert!(!db.load_app_settings().expect("load off").close_to_tray);
    db.set_setting("close_to_tray", "no").expect("set no");
    assert!(!db.load_app_settings().expect("load no").close_to_tray);

    db.set_setting("theme", "dark").expect("set");
    assert_eq!(
        db.get_setting("theme").expect("get after set").as_deref(),
        Some("dark")
    );
    db.set_setting("theme", "light").expect("update");
    assert_eq!(
        db.get_setting("theme")
            .expect("get after update")
            .as_deref(),
        Some("light")
    );
    db.set_setting("custom_key", "custom_value")
        .expect("insert");
    assert_eq!(
        db.get_setting("custom_key").expect("get custom").as_deref(),
        Some("custom_value")
    );

    // Schema migration marker should exist after open.
    db.with_conn(|conn| {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = '0001_init'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 1);
        let settings_ok: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='settings'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(settings_ok, 1);
        Ok(())
    })
    .expect("schema checks");
}

#[test]
fn peek_settings_reads_written_keys_and_empty_when_missing() {
    let missing = tempfile::tempdir().expect("tempdir");
    let empty = peek_settings(&missing.path().join("agenthub.db"), &["log_level"]);
    assert!(empty.is_empty());

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("agenthub.db");
    let db = Database::open(&path).expect("open db");
    db.set_setting("log_level", "debug").expect("set level");
    db.set_setting("log_retention_days", "21")
        .expect("set days");
    drop(db);

    let values = peek_settings(&path, &["log_level", "log_retention_days", "missing"]);
    assert_eq!(values.get("log_level").map(String::as_str), Some("debug"));
    assert_eq!(
        values.get("log_retention_days").map(String::as_str),
        Some("21")
    );
    assert!(!values.contains_key("missing"));
}

#[test]
fn load_app_settings_ignores_invalid_log_prefs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("agenthub.db");
    let db = Database::open(&path).expect("open db");

    db.set_setting("log_level", "nope")
        .expect("set invalid level");
    db.set_setting("log_retention_days", "999")
        .expect("set invalid days");
    let settings = db.load_app_settings().expect("load invalid");
    assert_eq!(settings.log_level, "info");
    assert_eq!(settings.log_retention_days, 14);

    db.set_setting("log_level", " DEBUG ")
        .expect("set mixed case");
    db.set_setting("log_retention_days", "30")
        .expect("set valid days");
    let settings = db.load_app_settings().expect("load valid");
    assert_eq!(settings.log_level, "debug");
    assert_eq!(settings.log_retention_days, 30);
}
