use std::path::{Path, PathBuf};

use tempfile::tempdir;

use crate::platform::session_title::SessionTitleSource;

use super::DshSessionTitle;

fn session_dir(root: &Path, project: &str, session_id: &str) -> PathBuf {
    let dir = root.join(project).join(session_id);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_log(dir: &Path, name: &str, rows: &[&str]) {
    std::fs::write(dir.join(name), rows.join("\n")).unwrap();
}

fn write_zstd_log(dir: &Path, name: &str, rows: &[&str]) {
    let plain = rows.join("\n");
    let compressed = zstd::stream::encode_all(plain.as_bytes(), 3).unwrap();
    std::fs::write(dir.join(name), compressed).unwrap();
}

const TITLE_ROW: &str = r#"{"type":"session/title","seq":1,"data":{"title":"把历史栏收窄"}}"#;
const FALLBACK_ROW: &str = r#"{"type":"session/title","seq":1,"data":{"title":"你是调查员。目标：彻底","source":{"kind":"fallback"}}}"#;

#[test]
fn reads_the_title_row_from_a_plain_log() {
    let home = tempdir().unwrap();
    let dir = session_dir(&home.path().join("sessions"), "--c--demo--", "session-a");
    write_log(&dir, "session.v1.jsonl", &[TITLE_ROW]);

    assert_eq!(
        DshSessionTitle
            .title_for(home.path(), "session-a")
            .unwrap()
            .as_deref(),
        Some("把历史栏收窄")
    );
}

#[test]
fn reads_the_title_row_from_a_compressed_log() {
    let home = tempdir().unwrap();
    let dir = session_dir(&home.path().join("sessions"), "--c--demo--", "session-a");
    write_zstd_log(&dir, "session.v1.jsonl.zstd", &[TITLE_ROW]);

    assert_eq!(
        DshSessionTitle
            .title_for(home.path(), "session-a")
            .unwrap()
            .as_deref(),
        Some("把历史栏收窄")
    );
}

#[test]
fn finds_a_title_in_any_generation_and_profile_root() {
    let home = tempdir().unwrap();
    let dir = session_dir(&home.path().join("sessions"), "--c--demo--", "session-a");
    write_log(&dir, "session.v1.jsonl", &["not json"]);
    write_log(&dir, "session.v2.jsonl", &[TITLE_ROW]);

    let profile = session_dir(
        &home.path().join("profiles").join("work").join("sessions"),
        "_no-cwd",
        "session-b",
    );
    write_log(&profile, "session.v1.jsonl", &[TITLE_ROW]);

    assert_eq!(
        DshSessionTitle
            .title_for(home.path(), "session-a")
            .unwrap()
            .as_deref(),
        Some("把历史栏收窄")
    );
    assert_eq!(
        DshSessionTitle
            .title_for(home.path(), "session-b")
            .unwrap()
            .as_deref(),
        Some("把历史栏收窄")
    );
}

#[test]
fn keeps_only_real_titles_and_ignores_unknown_sessions() {
    let home = tempdir().unwrap();
    let dir = session_dir(&home.path().join("sessions"), "--c--demo--", "session-a");
    write_log(&dir, "session.v1.jsonl", &[FALLBACK_ROW]);

    assert_eq!(DshSessionTitle.title_for(home.path(), "session-a").unwrap(), None);
    assert_eq!(DshSessionTitle.title_for(home.path(), "session-b").unwrap(), None);
    assert_eq!(DshSessionTitle.title_for(home.path(), "").unwrap(), None);
}
