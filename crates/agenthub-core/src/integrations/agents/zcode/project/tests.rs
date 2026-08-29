use std::path::Path;

use rusqlite::Connection;

use super::{list_zcode_projects, list_zcode_sessions, load_zcode_excerpt};

fn seed_tasks(home: &Path, rows: &[(&str, &str, &str, i64, i64)]) {
    let dir = home.join("v2");
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("tasks-index.sqlite");
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(
        "CREATE TABLE tasks (
            workspace_key TEXT,
            workspace_path TEXT,
            task_id TEXT,
            title TEXT,
            updated_at INTEGER,
            created_at INTEGER,
            deleted INTEGER,
            searchable_text TEXT
        );",
    )
    .unwrap();
    for (ws, id, title, updated, deleted) in rows {
        conn.execute(
            "INSERT INTO tasks (workspace_key, workspace_path, task_id, title, updated_at, created_at, deleted)
             VALUES (?1, ?1, ?2, ?3, ?4, ?4, ?5)",
            rusqlite::params![ws, id, title, updated, deleted],
        )
        .unwrap();
    }
}

fn seed_cli_sessions(home: &Path, rows: &[(&str, &str, &str, Option<&str>, i64)]) {
    let dir = home.join("cli").join("db");
    std::fs::create_dir_all(&dir).unwrap();
    let conn = Connection::open(dir.join("db.sqlite")).unwrap();
    conn.execute_batch(
        "CREATE TABLE session (
            id TEXT,
            directory TEXT,
            path TEXT,
            title TEXT,
            time_updated INTEGER,
            time_created INTEGER,
            parent_id TEXT,
            task_type TEXT
        );",
    )
    .unwrap();
    for (id, dir, title, parent, updated) in rows {
        conn.execute(
            "INSERT INTO session (id, directory, path, title, time_updated, time_created, parent_id, task_type)
             VALUES (?1, ?2, ?2, ?3, ?4, ?4, ?5, 'interactive')",
            rusqlite::params![id, dir, title, updated, parent],
        )
        .unwrap();
    }
}

#[test]
fn groups_tasks_by_workspace_and_skips_deleted() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    seed_tasks(
        home,
        &[
            (
                r"D:\demo\AgentHub",
                "sess_keep",
                "keep title",
                1_787_950_000_000,
                0,
            ),
            (
                r"D:\demo\AgentHub",
                "sess_other",
                "other",
                1_787_960_000_000,
                0,
            ),
            (
                r"D:\demo\AgentHub",
                "sess_gone",
                "gone",
                1_787_970_000_000,
                1,
            ),
            (
                r"C:\Users\x\.zcode\workspace\default",
                "sess_hi",
                "hi",
                1_787_940_000_000,
                0,
            ),
        ],
    );

    let projects = list_zcode_projects(home);
    assert_eq!(projects.len(), 2);
    let hub = projects
        .iter()
        .find(|p| p.storage_path.contains("AgentHub"))
        .expect("AgentHub workspace");
    assert_eq!(hub.session_count, 2);
    assert_eq!(hub.agent_id, crate::models::AgentId::Zcode);
    assert!(hub.id.starts_with("zcode:proj:cwd/"));

    let sessions = list_zcode_sessions(home, None);
    assert_eq!(sessions.len(), 2 + 1);
    assert!(sessions.iter().all(|s| s.session_id.is_some()));
    assert!(sessions.iter().all(|s| s.id.starts_with("zcode:v2/task/")));
    assert!(!sessions
        .iter()
        .any(|s| s.session_id.as_deref() == Some("sess_gone")));

    let only_hub = list_zcode_sessions(home, Some(hub.id.strip_prefix("zcode:proj:").unwrap()));
    assert_eq!(only_hub.len(), 2);
}

#[test]
fn falls_back_to_cli_sessions_when_task_index_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    seed_cli_sessions(
        home,
        &[
            (
                "sess_root",
                r"D:\demo\AgentHub",
                "root task",
                None,
                1_787_950_000_000,
            ),
            (
                "sess_child",
                r"D:\demo\AgentHub",
                "child",
                Some("sess_root"),
                1_787_951_000_000,
            ),
        ],
    );

    let projects = list_zcode_projects(home);
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].session_count, 1);
    let sessions = list_zcode_sessions(home, None);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id.as_deref(), Some("sess_root"));
}

#[test]
fn missing_home_is_empty() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(list_zcode_projects(tmp.path()).is_empty());
    assert!(list_zcode_sessions(tmp.path(), None).is_empty());
}

fn seed_cli_transcript(home: &Path, session_id: &str) {
    let dir = home.join("cli").join("db");
    std::fs::create_dir_all(&dir).unwrap();
    let conn = Connection::open(dir.join("db.sqlite")).unwrap();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS message (
            id TEXT,
            session_id TEXT,
            sequence INTEGER,
            data TEXT
        );
        CREATE TABLE IF NOT EXISTS part (
            id TEXT,
            message_id TEXT,
            session_id TEXT,
            sequence INTEGER,
            data TEXT
        );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO message (id, session_id, sequence, data) VALUES
            ('msg_u', ?1, 0, '{\"role\":\"user\"}'),
            ('msg_a', ?1, 1, '{\"role\":\"assistant\"}')",
        [session_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO part (id, message_id, session_id, sequence, data) VALUES
            ('p1', 'msg_u', ?1, 0, '{\"type\":\"text\",\"text\":\"hello from user\"}'),
            ('p2', 'msg_a', ?1, 0, '{\"type\":\"reasoning\",\"text\":\"hidden\"}'),
            ('p3', 'msg_a', ?1, 1, '{\"type\":\"tool\",\"tool\":\"Bash\"}'),
            ('p4', 'msg_a', ?1, 2, '{\"type\":\"text\",\"text\":\"hello from assistant\"}')",
        [session_id],
    )
    .unwrap();
}

#[test]
fn excerpt_reads_text_parts_and_skips_tools() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    seed_tasks(
        home,
        &[(
            r"D:\demo\AgentHub",
            "sess_keep",
            "keep title",
            1_787_950_000_000,
            0,
        )],
    );
    seed_cli_transcript(home, "sess_keep");
    let ex = load_zcode_excerpt(home, "zcode:v2/task/sess_keep", "v2/task/sess_keep").unwrap();
    assert_eq!(ex.id, "zcode:v2/task/sess_keep");
    assert_eq!(ex.title, "keep title");
    assert!(ex.excerpt.contains("---turn:user---"));
    assert!(ex.excerpt.contains("hello from user"));
    assert!(ex.excerpt.contains("---turn:assistant---"));
    assert!(ex.excerpt.contains("hello from assistant"));
    assert!(!ex.excerpt.contains("hidden"));
    assert!(!ex.excerpt.contains("Bash"));
}

#[test]
fn excerpt_unknown_session_is_not_found() {
    let tmp = tempfile::tempdir().unwrap();
    let err =
        load_zcode_excerpt(tmp.path(), "zcode:v2/task/missing", "v2/task/missing").unwrap_err();
    assert_eq!(err.code(), "not_found");
}
