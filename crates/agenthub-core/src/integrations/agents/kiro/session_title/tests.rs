use std::path::Path;

use tempfile::tempdir;

use crate::platform::session_title::SessionTitleSource;

use super::KiroSessionTitle;

fn cli_session(home: &Path, file_stem: &str, body: &str) {
    let dir = home.join("sessions").join("cli");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{file_stem}.json")), body).unwrap();
}

#[test]
fn reads_the_title_kiro_stored_for_the_session() {
    let home = tempdir().unwrap();
    cli_session(
        home.path(),
        "session-a",
        r#"{"session_id":"session-a","cwd":"C:\\demo","title":"tidy the rail"}"#,
    );

    assert_eq!(
        KiroSessionTitle
            .title_for(home.path(), "session-a")
            .unwrap()
            .as_deref(),
        Some("tidy the rail")
    );
}

#[test]
fn matches_the_file_stem_when_the_record_omits_its_session_id() {
    let home = tempdir().unwrap();
    cli_session(home.path(), "session-a", r#"{"cwd":"C:\\demo","title":"tidy the rail"}"#);

    assert_eq!(
        KiroSessionTitle
            .title_for(home.path(), "session-a")
            .unwrap()
            .as_deref(),
        Some("tidy the rail")
    );
}

#[test]
fn kiro_placeholders_and_unknown_sessions_have_no_title() {
    let home = tempdir().unwrap();
    cli_session(
        home.path(),
        "session-a",
        r#"{"session_id":"session-a","title":"New Session"}"#,
    );
    cli_session(home.path(), "session-b", r#"{"session_id":"session-b","title":"  "}"#);

    assert_eq!(KiroSessionTitle.title_for(home.path(), "session-a").unwrap(), None);
    assert_eq!(KiroSessionTitle.title_for(home.path(), "session-b").unwrap(), None);
    assert_eq!(KiroSessionTitle.title_for(home.path(), "session-c").unwrap(), None);
    assert_eq!(KiroSessionTitle.title_for(home.path(), "  ").unwrap(), None);
}
