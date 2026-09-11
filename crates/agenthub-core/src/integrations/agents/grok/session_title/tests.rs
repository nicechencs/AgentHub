use std::path::Path;

use tempfile::tempdir;

use crate::platform::session_title::SessionTitleSource;

use super::GrokSessionTitle;

fn summary(home: &Path, project: &str, session_id: &str, body: &str) {
    let dir = home.join("sessions").join(project).join(session_id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("summary.json"), body).unwrap();
}

#[test]
fn finds_the_generated_title_under_the_session_directory() {
    let home = tempdir().unwrap();
    summary(
        home.path(),
        "C%3A%5Cdemo",
        "session-a",
        r#"{"generated_title":"ship the sidebar","session_summary":"older copy"}"#,
    );

    assert_eq!(
        GrokSessionTitle
            .title_for(home.path(), "session-a")
            .unwrap()
            .as_deref(),
        Some("ship the sidebar")
    );
}

#[test]
fn falls_back_to_the_older_summary_key() {
    let home = tempdir().unwrap();
    summary(
        home.path(),
        "C%3A%5Cdemo",
        "session-a",
        r#"{"session_summary":"older copy"}"#,
    );

    assert_eq!(
        GrokSessionTitle
            .title_for(home.path(), "session-a")
            .unwrap()
            .as_deref(),
        Some("older copy")
    );
}

#[test]
fn searches_every_project_directory_and_ignores_unknown_ids() {
    let home = tempdir().unwrap();
    summary(home.path(), "project-one", "session-a", r#"{"generated_title":"one"}"#);
    summary(home.path(), "project-two", "session-b", r#"{"generated_title":"two"}"#);

    assert_eq!(
        GrokSessionTitle
            .title_for(home.path(), "session-b")
            .unwrap()
            .as_deref(),
        Some("two")
    );
    assert_eq!(GrokSessionTitle.title_for(home.path(), "session-c").unwrap(), None);
    assert_eq!(GrokSessionTitle.title_for(home.path(), "").unwrap(), None);
}

#[test]
fn blank_and_broken_summaries_have_no_title() {
    let home = tempdir().unwrap();
    summary(home.path(), "project-one", "session-a", r#"{"generated_title":"   "}"#);
    summary(home.path(), "project-two", "session-b", "not json");

    assert_eq!(GrokSessionTitle.title_for(home.path(), "session-a").unwrap(), None);
    assert_eq!(GrokSessionTitle.title_for(home.path(), "session-b").unwrap(), None);
}
