use tempfile::tempdir;

use crate::platform::session_title::SessionTitleSource;

use super::CodexSessionTitle;

fn index(home: &std::path::Path, rows: &[&str]) {
    std::fs::write(home.join("session_index.jsonl"), rows.join("\n")).unwrap();
}

#[test]
fn reads_the_thread_name_of_the_matching_thread() {
    let home = tempdir().unwrap();
    index(
        home.path(),
        &[
            r#"{"id":"thread-a","thread_name":"first title","updated_at":"2026-01-01T00:00:00Z"}"#,
            r#"{"id":"thread-b","thread_name":"other title","updated_at":"2026-01-01T00:00:00Z"}"#,
        ],
    );

    assert_eq!(
        CodexSessionTitle
            .title_for(home.path(), "thread-a")
            .unwrap()
            .as_deref(),
        Some("first title")
    );
}

#[test]
fn newest_row_wins_when_codex_renames_a_thread() {
    let home = tempdir().unwrap();
    index(
        home.path(),
        &[
            r#"{"id":"thread-a","thread_name":"rough draft","updated_at":"2026-01-01T00:00:00Z"}"#,
            r#"{"id":"thread-a","thread_name":"","updated_at":"2026-01-01T00:01:00Z"}"#,
            r#"{"id":"thread-a","thread_name":"final title","updated_at":"2026-01-01T00:02:00Z"}"#,
        ],
    );

    assert_eq!(
        CodexSessionTitle
            .title_for(home.path(), "thread-a")
            .unwrap()
            .as_deref(),
        Some("final title")
    );
}

#[test]
fn unknown_thread_ids_and_missing_stores_have_no_title() {
    let home = tempdir().unwrap();
    assert_eq!(CodexSessionTitle.title_for(home.path(), "thread-a").unwrap(), None);

    index(home.path(), &[r#"{"id":"thread-b","thread_name":"other"}"#]);
    assert_eq!(CodexSessionTitle.title_for(home.path(), "thread-a").unwrap(), None);
    assert_eq!(CodexSessionTitle.title_for(home.path(), "   ").unwrap(), None);
}

#[test]
fn broken_rows_do_not_hide_a_good_one() {
    let home = tempdir().unwrap();
    index(
        home.path(),
        &[
            "not json",
            r#"{"id":"thread-a"}"#,
            r#"{"id":"thread-a","thread_name":"kept"}"#,
        ],
    );

    assert_eq!(
        CodexSessionTitle
            .title_for(home.path(), "thread-a")
            .unwrap()
            .as_deref(),
        Some("kept")
    );
}

/// Fixture standing in for `~/.codex/sqlite/<name>.db`.
fn catalog(home: &std::path::Path, name: &str, rows: &[(&str, &str, &str)]) {
    let dir = home.join("sqlite");
    std::fs::create_dir_all(&dir).unwrap();
    let conn = rusqlite::Connection::open(dir.join(name)).unwrap();
    conn.execute_batch(
        "CREATE TABLE local_thread_catalog (
             host_id TEXT,
             thread_id TEXT,
             display_title TEXT,
             source_updated_at TEXT
         );",
    )
    .unwrap();
    for (thread_id, display_title, updated) in rows {
        conn.execute(
            "INSERT INTO local_thread_catalog (host_id, thread_id, display_title, source_updated_at)
             VALUES ('host', ?1, ?2, ?3)",
            rusqlite::params![thread_id, display_title, updated],
        )
        .unwrap();
    }
}

#[test]
fn reads_the_catalog_title_app_server_reconciled() {
    let home = tempdir().unwrap();
    catalog(
        home.path(),
        "codex-dev.db",
        &[
            ("thread-a", "tidy the rail", "2026-01-01T00:00:00Z"),
            ("thread-b", "other thread", "2026-01-01T00:00:00Z"),
        ],
    );

    assert_eq!(
        CodexSessionTitle
            .title_for(home.path(), "thread-a")
            .unwrap()
            .as_deref(),
        Some("tidy the rail")
    );
}

#[test]
fn catalog_beats_the_ide_index_and_blank_rows_fall_through_to_it() {
    let home = tempdir().unwrap();
    catalog(
        home.path(),
        "codex.db",
        &[
            ("thread-a", "catalog title", "2026-01-01T00:00:00Z"),
            ("thread-b", "   ", "2026-01-01T00:00:00Z"),
        ],
    );
    index(
        home.path(),
        &[
            r#"{"id":"thread-a","thread_name":"index title"}"#,
            r#"{"id":"thread-b","thread_name":"index fallback"}"#,
        ],
    );

    assert_eq!(
        CodexSessionTitle
            .title_for(home.path(), "thread-a")
            .unwrap()
            .as_deref(),
        Some("catalog title")
    );
    assert_eq!(
        CodexSessionTitle
            .title_for(home.path(), "thread-b")
            .unwrap()
            .as_deref(),
        Some("index fallback")
    );
}

#[test]
fn newest_catalog_row_wins_when_codex_renames_a_thread() {
    let home = tempdir().unwrap();
    catalog(
        home.path(),
        "codex-dev.db",
        &[
            ("thread-a", "rough draft", "2026-01-01T00:00:00Z"),
            ("thread-a", "final title", "2026-01-02T00:00:00Z"),
        ],
    );

    assert_eq!(
        CodexSessionTitle
            .title_for(home.path(), "thread-a")
            .unwrap()
            .as_deref(),
        Some("final title")
    );
}

#[test]
fn unrelated_files_and_broken_databases_are_skipped() {
    let home = tempdir().unwrap();
    let dir = home.path().join("sqlite");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("notes.txt"), "not a database").unwrap();
    std::fs::write(dir.join("broken.db"), "not a database").unwrap();
    // A database without the catalog table must not hide the later one.
    rusqlite::Connection::open(dir.join("other.db"))
        .unwrap()
        .execute_batch("CREATE TABLE unrelated (id TEXT);")
        .unwrap();

    assert_eq!(CodexSessionTitle.title_for(home.path(), "thread-a").unwrap(), None);

    catalog(home.path(), "codex-dev.db", &[("thread-a", "later db wins", "")]);
    assert_eq!(
        CodexSessionTitle
            .title_for(home.path(), "thread-a")
            .unwrap()
            .as_deref(),
        Some("later db wins")
    );
}
