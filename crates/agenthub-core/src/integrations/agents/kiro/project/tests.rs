use std::fs;
use std::path::Path;

use crate::utils::project_path::cwd_storage_key;

use super::{list_kiro_projects, list_kiro_sessions, load_kiro_excerpt};

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn cli_session(cwd: &str, title: &str, session_id: &str, turns: &str) -> String {
    format!(
        r#"{{
  "session_id": "{session_id}",
  "cwd": {cwd},
  "updated_at": "2026-09-07T01:15:06Z",
  "title": "{title}",
  "session_state": {{
    "conversation_metadata": {{
      "user_turn_metadatas": [{turns}]
    }}
  }}
}}"#
    )
}

#[test]
fn groups_cli_and_editor_by_cwd_and_skips_empty_stub() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let hub = r"D:\demo_chen\2026\AgentHub";
    write(
        &home.join("sessions/cli/aaaa.json"),
        &cli_session(
            &serde_json::to_string(hub).unwrap(),
            "hi",
            "aaaa",
            r#"{"end_timestamp":"2026-09-07T01:15:06Z","end_reason":"UserTurnEnd"}"#,
        ),
    );
    write(
        &home.join("sessions/cli/aaaa.jsonl"),
        r#"{"version":"v1","kind":"Prompt","data":{"content":[{"kind":"text","data":"hi"}]}}
{"version":"v1","kind":"AssistantMessage","data":{"content":[{"kind":"thinking","data":{}},{"kind":"text","data":"Hello from CLI"}]}}
"#,
    );
    write(
        &home.join("sessions/cli/empty.json"),
        &cli_session(
            &serde_json::to_string(r"C:\Users\chen").unwrap(),
            "",
            "empty",
            "",
        ),
    );
    write(&home.join("sessions/cli/empty.jsonl"), "");

    let editor = home
        .join("sessions")
        .join("60e03a25accc47f3")
        .join("sess_db5bb099-6dbc-43be-8d22-cff1211025d7");
    write(
        &editor.join("session.json"),
        r#"{
  "id": "sess_db5bb099-6dbc-43be-8d22-cff1211025d7",
  "title": "New Session",
  "workspacePaths": ["D:\\demo_chen\\2026\\AgentHub"],
  "lastModifiedAt": "2026-09-06T15:26:37.026Z"
}"#,
    );
    write(
        &editor.join("messages.jsonl"),
        r#"{"id":"u1","timestamp":"2026-09-06T15:26:37.073Z","payload":{"type":"user","content":"reply with only the word hi"}}
{"id":"t1","payload":{"type":"tool_call","toolName":"read"}}
{"id":"a1","payload":{"type":"assistant","content":"hi"}}
"#,
    );

    let projects = list_kiro_projects(home);
    assert_eq!(projects.len(), 1, "{projects:?}");
    assert_eq!(projects[0].session_count, 2);
    assert_eq!(
        projects[0].id,
        format!("kiro:proj:{}", cwd_storage_key(hub))
    );
    assert_eq!(projects[0].title, "AgentHub");

    let sessions = list_kiro_sessions(home, None);
    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().all(|s| s.agent_id.as_str() == "kiro"));
    let cli = sessions
        .iter()
        .find(|s| s.session_id.as_deref() == Some("aaaa"))
        .unwrap();
    assert_eq!(cli.title, "hi");
    assert_eq!(cli.relative_path, "sessions/cli/aaaa.json");
    assert_eq!(cli.preview.as_deref(), Some("hi"));
    let editor_row = sessions
        .iter()
        .find(|s| s.session_id.as_deref() == Some("sess_db5bb099-6dbc-43be-8d22-cff1211025d7"))
        .unwrap();
    assert_eq!(
        editor_row.preview.as_deref(),
        Some("reply with only the word hi")
    );
    assert!(editor_row.relative_path.ends_with("messages.jsonl"));

    let only = list_kiro_sessions(
        home,
        Some(projects[0].id.strip_prefix("kiro:proj:").unwrap()),
    );
    assert_eq!(only.len(), 2);
    assert!(list_kiro_sessions(home, Some("cwd/missing")).is_empty());
}

#[test]
fn excerpt_reads_cli_jsonl_and_editor_messages() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    write(
        &home.join("sessions/cli/bbbb.json"),
        &cli_session(
            &serde_json::to_string(r"D:\demo\Hub").unwrap(),
            "ask",
            "bbbb",
            r#"{"end_timestamp":"2026-09-07T01:15:06Z"}"#,
        ),
    );
    write(
        &home.join("sessions/cli/bbbb.jsonl"),
        r#"{"version":"v1","kind":"Prompt","data":{"content":[{"kind":"text","data":"ping"}]}}
{"version":"v1","kind":"AssistantMessage","data":{"content":[{"kind":"text","data":"pong"}]}}
"#,
    );
    let editor = home.join("sessions/abc/sess_x");
    write(
        &editor.join("session.json"),
        r#"{"id":"sess_x","title":"Review crash","workspacePaths":["D:\\demo\\Hub"]}"#,
    );
    write(
        &editor.join("messages.jsonl"),
        r#"{"payload":{"type":"user","content":"look at this"}}
{"payload":{"type":"assistant","content":"looked"}}
"#,
    );

    let cli = load_kiro_excerpt(
        home,
        "kiro:sessions/cli/bbbb.json",
        "sessions/cli/bbbb.json",
    )
    .unwrap();
    assert!(cli.excerpt.contains("---turn:user---"));
    assert!(cli.excerpt.contains("ping"));
    assert!(cli.excerpt.contains("pong"));
    assert_eq!(cli.title, "ask");

    let ed = load_kiro_excerpt(
        home,
        "kiro:sessions/abc/sess_x/messages.jsonl",
        "sessions/abc/sess_x/messages.jsonl",
    )
    .unwrap();
    assert!(ed.excerpt.contains("look at this"));
    assert!(ed.excerpt.contains("looked"));
    assert!(!ed.excerpt.contains("tool_call"));
    assert_eq!(ed.title, "Review crash");
}

#[test]
fn missing_home_and_unknown_excerpt() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(list_kiro_projects(tmp.path()).is_empty());
    assert!(list_kiro_sessions(tmp.path(), None).is_empty());
    let err = load_kiro_excerpt(
        tmp.path(),
        "kiro:sessions/cli/no.json",
        "sessions/cli/no.json",
    )
    .unwrap_err();
    assert_eq!(err.code(), "not_found");
}
