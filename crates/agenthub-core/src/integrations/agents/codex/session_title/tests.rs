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
