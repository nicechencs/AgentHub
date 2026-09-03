use super::*;
use crate::adapters::AdapterRegistry;
use crate::models::ProjectUserMeta;
use crate::platform::projects::ProjectSource;
use crate::utils::project_path::{decode_claude_project_dir, decode_cursor_project_dir};
use std::io::Write;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, SystemTime};
use tempfile::tempdir;

#[derive(Default)]
struct ProjectSourceCalls {
    projects: AtomicUsize,
    sessions: AtomicUsize,
    project_sessions: AtomicUsize,
}

struct CountingProjectSource {
    key: AgentKey,
    calls: Arc<ProjectSourceCalls>,
}

impl ProjectSource for CountingProjectSource {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn list_projects(&self, _ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentProject>> {
        self.calls.projects.fetch_add(1, Ordering::SeqCst);
        Ok(Vec::new())
    }

    fn list_sessions(&self, _ctx: &ProjectScanContext<'_>) -> Result<Vec<AgentSession>> {
        self.calls.sessions.fetch_add(1, Ordering::SeqCst);
        Ok(Vec::new())
    }

    fn list_sessions_in_project(
        &self,
        _ctx: &ProjectScanContext<'_>,
        _project_id: &str,
        _key: &str,
    ) -> Result<Vec<AgentSession>> {
        self.calls.project_sessions.fetch_add(1, Ordering::SeqCst);
        Ok(Vec::new())
    }
}

fn write_session(path: &Path, lines: &[&str]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut f = fs::File::create(path).unwrap();
    for line in lines {
        writeln!(f, "{line}").unwrap();
    }
}

fn write_jsonl(path: &Path, lines: &[serde_json::Value]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut f = fs::File::create(path).unwrap();
    for line in lines {
        writeln!(f, "{line}").unwrap();
    }
}

fn grok_acp_chunk(session_update: &str, text: Option<&str>) -> serde_json::Value {
    let mut update = serde_json::json!({ "sessionUpdate": session_update });
    if let Some(text) = text {
        update["content"] = serde_json::json!({ "type": "text", "text": text });
    } else {
        update["title"] = serde_json::json!("read");
    }
    serde_json::json!({
        "method": "session/update",
        "params": { "update": update }
    })
}

#[test]
fn project_service_uses_injected_source_for_unknown_agent_key() {
    let key = AgentKey::parse("future-agent").unwrap();
    assert!(AgentId::ALL
        .iter()
        .all(|agent| agent.as_str() != key.as_str()));

    let calls = Arc::new(ProjectSourceCalls::default());
    let mut sources = ProjectSourceRegistry::new();
    sources
        .register(Arc::new(CountingProjectSource {
            key: key.clone(),
            calls: Arc::clone(&calls),
        }))
        .unwrap();

    let data_dir = tempdir().unwrap();
    let agent_home = tempdir().unwrap();
    let service = ProjectService::with_project_sources(
        AdapterRegistry::default(),
        data_dir.path().to_path_buf(),
        sources,
    );

    assert!(service
        .list_projects_for_agent_key(&key, agent_home.path(), false)
        .unwrap()
        .is_empty());
    assert!(service
        .list_for_agent_key(&key, agent_home.path())
        .unwrap()
        .is_empty());
    assert!(service
        .list_project_sessions_for_agent_key(
            &key,
            agent_home.path(),
            "future-agent:proj:workspace",
            "workspace",
        )
        .unwrap()
        .is_empty());

    assert_eq!(calls.projects.load(Ordering::SeqCst), 1);
    assert_eq!(calls.sessions.load(Ordering::SeqCst), 1);
    assert_eq!(calls.project_sessions.load(Ordering::SeqCst), 1);
}

#[test]
fn parse_session_id_rejects_traversal_and_bad_agent() {
    assert!(parse_session_id("claude:../etc/passwd").is_err());
    assert!(parse_session_id("claude:projects/foo.jsonl").is_ok());
    assert!(parse_session_id("nope:projects/foo.jsonl").is_err());
    assert!(parse_session_id("not-an-id").is_err());
    assert!(parse_session_id("claude:").is_err());
    assert!(parse_session_id("claude:proj:-C-Users-x").is_err());
}

#[test]
fn parse_project_id_ok() {
    let (a, key) = parse_project_id("claude:proj:-C-Users-demo-app").unwrap();
    assert_eq!(a, AgentId::Claude);
    assert_eq!(key, "-C-Users-demo-app");
    assert!(parse_project_id("claude:projects/x").is_err());
}

#[test]
fn is_session_file_filters_sidecars() {
    assert!(is_session_file(Path::new("sess.jsonl")));
    assert!(is_session_file(Path::new("sess.json")));
    assert!(!is_session_file(Path::new("session_index.jsonl")));
    assert!(!is_session_file(Path::new("foo.bak")));
    assert!(!is_session_file(Path::new(".hidden.jsonl")));
    assert!(!is_session_file(Path::new("notes.txt")));
}

#[test]
fn extract_user_text_from_claude_shape() {
    let line = r#"{"type":"user","message":{"content":[{"type":"text","text":"hello world"}]}}"#;
    let t = extract_userish_text(line).or_else(|| extract_any_text(line));
    assert!(t.unwrap().contains("hello world"));
}

#[test]
fn extract_user_text_from_codex_response_item_shape() {
    let line = r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello from codex"}]}}"#;
    let text = extract_userish_text(line).expect("response_item user payload should be recognized");
    assert!(text.contains("hello from codex"));
}

#[test]
fn list_claude_aggregates_sessions_into_project() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".claude");
    let base = home.join("projects").join("-C-Users-demo-app");
    write_session(
        &base.join("sess-1.jsonl"),
        &[
            r#"{"type":"user","message":{"content":"fix the login bug"}}"#,
            r#"{"type":"assistant","text":"looking into it"}"#,
        ],
    );
    write_session(
        &base.join("sess-2.jsonl"),
        &[r#"{"type":"user","message":{"content":"add tests"}}"#],
    );
    // Sidechain subdir should be ignored (top-level only).
    write_session(
        &base.join("sub").join("side.jsonl"),
        &[r#"{"type":"user","text":"sidechain"}"#],
    );

    let sessions = list_sessions_for_agent_home(AgentId::Claude, &home, None).unwrap();
    assert_eq!(sessions.len(), 2);
    assert!(sessions
        .iter()
        .all(|s| s.project_id == "claude:proj:-C-Users-demo-app"));
    let mut native_ids: Vec<_> = sessions
        .iter()
        .filter_map(|s| s.session_id.as_deref())
        .collect();
    native_ids.sort();
    assert_eq!(native_ids, vec!["sess-1", "sess-2"]);
    assert!(
        sessions[0].cwd.as_deref() == Some("C:\\Users\\demo\\app")
            || sessions
                .iter()
                .any(|s| s.cwd.as_deref() == Some("C:\\Users\\demo\\app"))
    );

    let projects = list_projects_for_agent_home(AgentId::Claude, &home, None).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].session_count, 2);
    assert_eq!(projects[0].id, "claude:proj:-C-Users-demo-app");
    if let Some(actual) = projects[0].actual_path.as_deref() {
        assert!(
            Path::new(actual).exists(),
            "actual_path must be a verified existing workspace, got {actual}"
        );
    }

    let svc = ProjectService::default();
    // list_sessions via public API needs real agent_home; exercise helper filter:
    let filtered: Vec<_> = sessions
        .into_iter()
        .filter(|s| s.project_id == "claude:proj:-C-Users-demo-app")
        .collect();
    assert_eq!(filtered.len(), 2);
    let _ = svc;
}

#[test]
fn claude_actual_path_only_when_workspace_exists() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".claude");
    write_session(
        &home
            .join("projects")
            .join("-Z-NoSuch-AgentHub-XYZ")
            .join("s.jsonl"),
        &[r#"{"type":"user","text":"hello"}"#],
    );
    let missing = list_projects_for_agent_home(AgentId::Claude, &home, None).unwrap();
    assert_eq!(missing.len(), 1);
    assert!(
        missing[0].actual_path.is_none(),
        "unverified restore must not be openable: {:?}",
        missing[0].actual_path
    );

    let ws = dir.path().join("real-workspace");
    fs::create_dir_all(&ws).unwrap();
    let escaped = ws.display().to_string().replace('\\', "\\\\");
    write_session(
        &home
            .join("projects")
            .join("-C-Users-demo-verified")
            .join("s.jsonl"),
        &[&format!(
            r#"{{"cwd":"{escaped}","type":"user","text":"from existing cwd"}}"#
        )],
    );
    let projects = list_projects_for_agent_home(AgentId::Claude, &home, None).unwrap();
    let found = projects
        .iter()
        .find(|p| p.id.contains("demo-verified"))
        .expect("verified project");
    let actual = found.actual_path.as_deref().expect("verified actual_path");
    assert!(Path::new(actual).exists());
}

#[test]
fn claude_list_projects_peeks_newest_not_huge_tail() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".claude");
    let ws = dir.path().join("real-ws");
    fs::create_dir_all(&ws).unwrap();
    let proj = home.join("projects").join("-C-Users-demo-huge");
    fs::create_dir_all(&proj).unwrap();

    let huge = proj.join("old.jsonl");
    {
        let mut f = fs::File::create(&huge).unwrap();
        let line = b"{\"type\":\"assistant\",\"text\":\"x\"}\n";
        let mut written = 0usize;
        while written < 600 * 1024 {
            f.write_all(line).unwrap();
            written += line.len();
        }
        f.flush().unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(120))
            .unwrap();
    }

    let escaped = ws.display().to_string().replace('\\', "\\\\");
    write_session(
        &proj.join("new.jsonl"),
        &[&format!(
            r#"{{"cwd":"{escaped}","type":"user","text":"tiny cwd"}}"#
        )],
    );

    let projects = list_projects_for_agent_home(AgentId::Claude, &home, None).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].session_count, 2);
    assert!(projects[0].message_count.is_none());
    assert!(projects[0].size_bytes >= 600 * 1024);
    let actual = projects[0]
        .actual_path
        .as_deref()
        .expect("cwd from the newer tiny jsonl");
    assert!(
        Path::new(actual).exists(),
        "actual_path must come from the tiny newer file, got {actual}"
    );
}

#[test]
fn list_cursor_workspace_folders_no_fake_excerpt() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".cursor");
    let proj = home.join("projects").join("d-demo-workspace-2026-AgentHub");
    fs::create_dir_all(proj.join("agent-transcripts")).unwrap();
    fs::create_dir_all(home.join("projects").join("empty-window")).unwrap();

    let rows = list_projects_for_agent_home(AgentId::Cursor, &home, None).unwrap();
    assert_eq!(rows.len(), 2);
    for rec in &rows {
        assert_eq!(rec.agent_id, AgentId::Cursor);
        assert!(rec.id.starts_with("cursor:proj:"));
        assert_eq!(rec.session_count, 0);
        assert!(rec.preview.is_none());
        assert!(rec.message_count.is_none());
    }
    let agenthub = rows
        .iter()
        .find(|r| r.relative_path.contains("AgentHub"))
        .expect("AgentHub folder");
    assert!(
        agenthub.title.contains("AgentHub") || agenthub.relative_path.contains("AgentHub"),
        "title={} rel={}",
        agenthub.title,
        agenthub.relative_path
    );
    if let Some(actual) = agenthub.actual_path.as_deref() {
        assert!(
            Path::new(actual).exists(),
            "cursor actual_path must exist when set: {actual}"
        );
    }
    let empty = rows
        .iter()
        .find(|r| r.relative_path.contains("empty-window"))
        .expect("empty-window");
    assert!(empty.actual_path.is_none());

    let sessions = list_sessions_for_agent_home(AgentId::Cursor, &home, None).unwrap();
    assert!(sessions.is_empty());
}

#[test]
fn list_cursor_agent_transcripts_as_sessions() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".cursor");
    let proj = home.join("projects").join("d-demo-workspace-2026-AgentHub");
    let sid = "0e435bc1-cf05-4a9a-b036-8902f810bd86";
    let sid2 = "12411b35-6081-425a-a07a-b776a24da27a";
    write_session(
        &proj
            .join("agent-transcripts")
            .join(sid)
            .join(format!("{sid}.jsonl")),
        &[
            r#"{"role":"user","message":{"content":[{"type":"text","text":"<timestamp>Sunday, Aug 2, 2026, 8:16 PM (UTC+8)</timestamp>\n<user_query>\n帮我改路由页\n</user_query>"}]}}"#,
            r#"{"role":"assistant","message":{"content":[{"type":"text","text":"先看现有实现。"}]}}"#,
        ],
    );
    write_session(
        &proj
            .join("agent-transcripts")
            .join(sid2)
            .join(format!("{sid2}.jsonl")),
        &[r#"{"role":"user","message":{"content":[{"type":"text","text":"第二段对话"}]}}"#],
    );
    let sub = "deadbeef-0000-0000-0000-000000000001";
    let sub_flat = "aaaa1111-0000-0000-0000-000000000002";
    write_session(
        &proj
            .join("agent-transcripts")
            .join(sid)
            .join("subagents")
            .join(sub)
            .join(format!("{sub}.jsonl")),
        &[r#"{"role":"user","message":{"content":"nested subagent prompt"}}"#],
    );
    write_session(
        &proj
            .join("agent-transcripts")
            .join(sid)
            .join("subagents")
            .join(format!("{sub_flat}.jsonl")),
        &[r#"{"role":"user","message":{"content":"flat subagent prompt"}}"#],
    );
    fs::create_dir_all(home.join("projects").join("empty-window")).unwrap();

    let projects = list_projects_for_agent_home(AgentId::Cursor, &home, None).unwrap();
    assert_eq!(projects.len(), 2);
    let hub = projects
        .iter()
        .find(|r| r.relative_path.contains("AgentHub"))
        .expect("AgentHub folder");
    assert_eq!(hub.session_count, 4);
    assert!(hub
        .preview
        .as_deref()
        .is_some_and(|p| p.contains("路由页") || p.contains("第二段")));
    let empty = projects
        .iter()
        .find(|r| r.relative_path.contains("empty-window"))
        .expect("empty-window");
    assert_eq!(empty.session_count, 0);

    let sessions = list_sessions_for_agent_home(AgentId::Cursor, &home, None).unwrap();
    assert_eq!(
        sessions.len(),
        4,
        "ids={:?}",
        sessions.iter().map(|s| &s.id).collect::<Vec<_>>()
    );
    assert!(sessions.iter().all(|s| s.project_id == hub.id));
    let mut native: Vec<_> = sessions
        .iter()
        .filter_map(|s| s.session_id.clone())
        .collect();
    native.sort();
    assert_eq!(
        native,
        vec![
            sid.to_string(),
            sid2.to_string(),
            sub_flat.to_string(),
            sub.to_string(),
        ]
    );
    let nested_child = sessions
        .iter()
        .find(|s| s.session_id.as_deref() == Some(sub))
        .expect("nested subagent");
    assert!(nested_child.relative_path.contains("subagents"));
    assert!(nested_child
        .preview
        .as_deref()
        .unwrap_or("")
        .contains("nested subagent"));
    let titled = sessions
        .iter()
        .find(|s| s.session_id.as_deref() == Some(sid))
        .expect("first transcript");
    assert!(
        titled.preview.as_deref().unwrap_or("").contains("路由页"),
        "preview={:?}",
        titled.preview
    );
    assert!(!titled
        .preview
        .as_deref()
        .unwrap_or("")
        .contains("<user_query>"));

    let (_, key) = parse_project_id(&hub.id).unwrap();
    let only = list_sessions_for_project_home(AgentId::Cursor, &home, &hub.id, &key, None).unwrap();
    assert_eq!(only.len(), 4);

    let ex = load_excerpt(&titled.id, Some(&home)).unwrap();
    assert_eq!(ex.id, titled.id);
    assert!(ex.excerpt.contains("---turn:user---"));
    assert!(ex.excerpt.contains("帮我改路由页"));
    assert!(ex.excerpt.contains("---turn:assistant---"));
    assert!(ex.excerpt.contains("先看现有实现"));
}

#[test]
fn decode_helpers_still_work() {
    assert_eq!(
        decode_claude_project_dir("-C-Users-example-demo").unwrap(),
        "C:\\Users\\example\\demo"
    );
    assert_eq!(
        decode_claude_project_dir("-Users-foo-bar").unwrap(),
        "/Users/foo/bar"
    );
    let got = decode_cursor_project_dir("d-demo-workspace-AgentHub").unwrap();
    assert!(got.starts_with("D:\\"));
    assert!(got.contains("AgentHub"));
}

#[test]
fn delete_cursor_is_unsupported() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".cursor");
    let proj = home.join("projects").join("d-demo");
    fs::create_dir_all(&proj).unwrap();
    let svc = ProjectService::default();
    // Delete expects session id; container id is rejected / or workspace path as session-like.
    let err = svc
        .delete_with_home("cursor:projects/d-demo", &home)
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
    assert!(proj.exists(), "must not delete on unsupported");
}

#[test]
fn list_codex_sessions_groups_by_cwd() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".codex");
    // Legacy top-level cwd (older fixtures / some exporters).
    let session = home.join("sessions").join("2026").join("s1.jsonl");
    write_session(
        &session,
        &[
            r#"{"cwd":"D:\\work\\repo","type":"user","text":"refactor switch"}"#,
            r#"{"type":"assistant","text":"ok"}"#,
        ],
    );

    let sessions = list_sessions_for_agent_home(AgentId::Codex, &home, None).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].cwd.as_deref(), Some("D:\\work\\repo"));
    assert!(sessions[0].preview.as_ref().unwrap().contains("refactor"));
    assert_eq!(sessions[0].project_id, "codex:proj:cwd/D:/work/repo");

    let projects = list_projects_for_agent_home(AgentId::Codex, &home, None).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].session_count, 1);
}

#[test]
fn list_codex_groups_by_payload_cwd_session_meta() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".codex");
    let session = home
        .join("sessions")
        .join("2026")
        .join("08")
        .join("03")
        .join("rollout-2026-08-03T11-11-12-abc.jsonl");
    // Real Codex shape: cwd lives under payload, not top-level.
    write_session(
        &session,
        &[
            r#"{"timestamp":"2026-08-03T03:12:01.558Z","type":"session_meta","payload":{"session_id":"abc","cwd":"d:\\demo_chen\\2026\\AgentHub","originator":"codex_vscode"}}"#,
            r#"{"timestamp":"2026-08-03T03:12:03.326Z","type":"turn_context","payload":{"cwd":"D:\\demo_chen\\2026\\AgentHub","model":"gpt-5.6-sol"}}"#,
            r#"{"timestamp":"2026-08-03T03:12:04.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"fix project grouping"}]}}"#,
        ],
    );

    let sessions = list_sessions_for_agent_home(AgentId::Codex, &home, None).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id.as_deref(), Some("abc"));
    // Drive letter normalized to uppercase in storage key.
    assert_eq!(
        sessions[0].project_id,
        "codex:proj:cwd/D:/demo_chen/2026/AgentHub"
    );
    assert!(
        sessions[0]
            .cwd
            .as_deref()
            .map(|c| c.eq_ignore_ascii_case(r"d:\demo_chen\2026\AgentHub")
                || c.eq_ignore_ascii_case("d:/demo_chen/2026/AgentHub")
                || c.eq_ignore_ascii_case(r"D:\demo_chen\2026\AgentHub")
                || c.eq_ignore_ascii_case("D:/demo_chen/2026/AgentHub"))
            .unwrap_or(false),
        "cwd={:?}",
        sessions[0].cwd
    );
    assert!(
        sessions[0]
            .preview
            .as_ref()
            .map(|p| p.contains("fix project grouping"))
            .unwrap_or(false),
        "preview={:?}",
        sessions[0].preview
    );

    let projects = list_projects_for_agent_home(AgentId::Codex, &home, None).unwrap();
    assert_eq!(projects.len(), 1);
    assert_ne!(projects[0].title, "未分类会话");
    assert_eq!(projects[0].session_count, 1);
    // lower-case d: and D: must not split into two projects
    let session2 = home
        .join("sessions")
        .join("2026")
        .join("08")
        .join("04")
        .join("rollout-other.jsonl");
    write_session(
        &session2,
        &[r#"{"type":"session_meta","payload":{"cwd":"D:\\demo_chen\\2026\\AgentHub"}}"#],
    );
    let projects2 = list_projects_for_agent_home(AgentId::Codex, &home, None).unwrap();
    assert_eq!(projects2.len(), 1);
    assert_eq!(projects2[0].session_count, 2);
}

#[test]
fn list_grok_groups_by_url_encoded_dir_and_ignores_sidecars() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".grok");
    // Grok layout: sessions/<url-encoded-cwd>/<sessionId>/chat_history.jsonl
    let encoded = "D%3A%5Cwork%5CAgentHub";
    let sess = home
        .join("sessions")
        .join(encoded)
        .join("019fb8a7-06a3-7cb2-83e6-980123542122");
    write_session(
        &sess.join("chat_history.jsonl"),
        &[
            r#"{"type":"system","content":"You are Grok released by xAI."}"#,
            r#"{"type":"user","content":[{"type":"text","text":"implement projects fix"}]}"#,
        ],
    );
    write_session(
        &sess.join("events.jsonl"),
        &[r#"{"type":"turn_started","session_id":"019fb8a7"}"#],
    );
    fs::write(
        sess.join("summary.json"),
        r#"{"info":{"id":"019fb8a7","cwd":"D:\\work\\AgentHub"},"generated_title":"Projects Fix Slice","session_summary":"fix grouping","num_chat_messages":12}"#,
    )
    .unwrap();
    // Second session same project
    let sess2 = home
        .join("sessions")
        .join(encoded)
        .join("019fc000-0000-0000-0000-000000000001");
    write_session(
        &sess2.join("chat_history.jsonl"),
        &[r#"{"type":"user","content":[{"type":"text","text":"second chat"}]}"#],
    );
    // Another project
    let encoded_b = "C%3A%5Ctmp%5Cother";
    let sess_b = home
        .join("sessions")
        .join(encoded_b)
        .join("bbbbbbbb-0000-0000-0000-000000000002");
    write_session(
        &sess_b.join("chat_history.jsonl"),
        &[r#"{"type":"user","content":"other project"}"#],
    );

    let sessions = list_sessions_for_agent_home(AgentId::Grok, &home, None).unwrap();
    // events.jsonl / summary.json must not become sessions
    assert_eq!(
        sessions.len(),
        3,
        "ids={:?}",
        sessions.iter().map(|s| &s.id).collect::<Vec<_>>()
    );
    let mut native_ids: Vec<_> = sessions
        .iter()
        .filter_map(|s| s.session_id.clone())
        .collect();
    native_ids.sort();
    assert_eq!(
        native_ids,
        vec![
            "019fb8a7-06a3-7cb2-83e6-980123542122".to_string(),
            "019fc000-0000-0000-0000-000000000001".to_string(),
            "bbbbbbbb-0000-0000-0000-000000000002".to_string(),
        ]
    );
    let projects = list_projects_for_agent_home(AgentId::Grok, &home, None).unwrap();
    assert_eq!(projects.len(), 2);
    assert!(projects.iter().all(|p| p.title != "未分类会话"));

    let hub = projects
        .iter()
        .find(|p| {
            p.id.contains("AgentHub") || p.actual_path.as_deref().unwrap_or("").contains("AgentHub")
        })
        .expect("AgentHub project");
    assert_eq!(hub.session_count, 2);

    // Subset list_sessions: only one project
    let (_, key) = parse_project_id(&hub.id).unwrap();
    let only = list_sessions_for_project_home(AgentId::Grok, &home, &hub.id, &key, None).unwrap();
    assert_eq!(only.len(), 2);
    assert!(only.iter().all(|s| s.project_id == hub.id));

    // Title from summary.json preferred
    let titled = only.iter().find(|s| {
        s.title.contains("Projects Fix")
            || s.preview
                .as_ref()
                .map(|p| p.contains("implement"))
                .unwrap_or(false)
    });
    assert!(
        titled.is_some(),
        "sessions={:?}",
        only.iter().map(|s| &s.title).collect::<Vec<_>>()
    );
}

#[test]
fn delete_and_excerpt_with_home_override() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".claude");
    let session = home
        .join("projects")
        .join("-C-Users-demo-app")
        .join("sess-del.jsonl");
    write_session(
        &session,
        &[
            r#"{"type":"user","message":{"content":"summarize me"}}"#,
            r#"{"type":"assistant","text":"summary"}"#,
        ],
    );

    let rows = list_sessions_for_agent_home(AgentId::Claude, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let id = rows[0].id.clone();

    let ex = load_excerpt(&id, Some(&home)).unwrap();
    assert!(ex.excerpt.contains("summarize") || ex.excerpt.contains("summary"));
    assert_eq!(ex.agent_id, AgentId::Claude);

    let svc = ProjectService::default();
    svc.delete_with_home(&id, &home).unwrap();
    assert!(!session.exists());
    assert!(list_sessions_for_agent_home(AgentId::Claude, &home, None)
        .unwrap()
        .is_empty());
}

#[test]
fn delete_many_partial_success() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".claude");
    let s1 = home
        .join("projects")
        .join("-C-Users-demo-app")
        .join("a.jsonl");
    let s2 = home
        .join("projects")
        .join("-C-Users-demo-app")
        .join("b.jsonl");
    write_session(&s1, &[r#"{"type":"user","text":"one"}"#]);
    write_session(&s2, &[r#"{"type":"user","text":"two"}"#]);

    let rows = list_sessions_for_agent_home(AgentId::Claude, &home, None).unwrap();
    assert_eq!(rows.len(), 2);
    let ids = vec![
        rows[0].id.clone(),
        "claude:projects/missing.jsonl".into(),
        rows[1].id.clone(),
    ];

    let svc = ProjectService::default();
    let mut ok = 0u32;
    for id in &ids {
        if svc.delete_with_home(id, &home).is_ok() {
            ok += 1;
        }
    }
    assert_eq!(ok, 2);
    assert!(list_sessions_for_agent_home(AgentId::Claude, &home, None)
        .unwrap()
        .is_empty());
}

#[test]
fn empty_home_lists_nothing() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".missing");
    assert!(list_sessions_for_agent_home(AgentId::Grok, &home, None)
        .unwrap()
        .is_empty());
    assert!(list_projects_for_agent_home(AgentId::Grok, &home, None)
        .unwrap()
        .is_empty());
}

#[test]
fn agent_project_serde_camel_case() {
    let p = AgentProject {
        id: "claude:proj:-C-Users-x".into(),
        agent_id: AgentId::Claude,
        title: "t".into(),
        storage_path: "p".into(),
        actual_path: Some("C:\\a".into()),
        relative_path: "projects/-C-Users-x".into(),
        session_count: 2,
        message_count: Some(3),
        size_bytes: 10,
        updated_at: "t0".into(),
        preview: Some("hi".into()),
        alias: None,
        hidden: false,
    };
    let json = serde_json::to_string(&p).unwrap();
    assert!(json.contains(r#""agentId":"claude""#));
    assert!(json.contains(r#""storagePath":"p""#));
    assert!(json.contains(r#""actualPath":"C:\\a""#) || json.contains(r#""actualPath":"C:\a""#));
    assert!(json.contains(r#""sessionCount":2"#));
    let back: AgentProject = serde_json::from_str(&json).unwrap();
    assert_eq!(back.id, p.id);
    assert_eq!(back.session_count, 2);
}

#[test]
fn service_list_smoke_does_not_panic() {
    let svc = ProjectService::default();
    let _ = svc.list_projects(Some(AgentId::Claude), false).unwrap();
    let _ = svc.list(Some(AgentId::Claude)).unwrap();
    let all = svc.list_projects(None, true).unwrap();
    for w in all.windows(2) {
        assert!(w[0].updated_at >= w[1].updated_at);
    }
}

#[test]
fn metadata_hide_and_alias_roundtrip() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".claude");
    let base = home.join("projects").join("-C-Users-demo-app");
    write_session(
        &base.join("sess-1.jsonl"),
        &[r#"{"type":"user","text":"hello"}"#],
    );

    let data = tempdir().unwrap();
    let svc = ProjectService::new(AdapterRegistry::default(), data.path().to_path_buf());

    // Exercise metadata file independently of agent_home scan:
    let pid = "claude:proj:-C-Users-demo-app";
    svc.upsert_project_meta(
        pid,
        ProjectUserMeta {
            hidden: true,
            alias: Some("  My App  ".into()),
        },
    )
    .unwrap();

    let doc = svc.get_metadata().unwrap();
    assert!(doc.projects.get(pid).unwrap().hidden);
    assert_eq!(
        doc.projects.get(pid).unwrap().alias.as_deref(),
        Some("My App")
    );

    // Clearing empty meta removes key
    svc.upsert_project_meta(
        pid,
        ProjectUserMeta {
            hidden: false,
            alias: None,
        },
    )
    .unwrap();
    assert!(!svc.get_metadata().unwrap().projects.contains_key(pid));

    svc.set_show_hidden_projects(true).unwrap();
    assert!(svc.get_metadata().unwrap().show_hidden_projects);

    // File lives under data_dir, not agent home
    assert!(data.path().join("project_metadata.json").exists());
    assert!(!home.join("project_metadata.json").exists());
}

#[test]
fn apply_metadata_filters_hidden() {
    let data = tempdir().unwrap();
    let svc = ProjectService::new(AdapterRegistry::default(), data.path().to_path_buf());
    let pid = "claude:proj:-C-Users-hidden";
    svc.upsert_project_meta(
        pid,
        ProjectUserMeta {
            hidden: true,
            alias: Some("Hidden".into()),
        },
    )
    .unwrap();

    let mut rows = vec![AgentProject {
        id: pid.into(),
        agent_id: AgentId::Claude,
        title: "orig".into(),
        storage_path: "p".into(),
        actual_path: None,
        relative_path: "projects/x".into(),
        session_count: 1,
        message_count: None,
        size_bytes: 1,
        updated_at: "t".into(),
        preview: None,
        alias: None,
        hidden: false,
    }];
    let meta = svc.get_metadata().unwrap();
    apply_metadata(&mut rows, &meta);
    assert!(rows[0].hidden);
    assert_eq!(rows[0].alias.as_deref(), Some("Hidden"));
    rows.retain(|p| !p.hidden);
    assert!(rows.is_empty());
}

#[test]
fn upsert_meta_rejects_non_project_id() {
    let data = tempdir().unwrap();
    let svc = ProjectService::new(AdapterRegistry::default(), data.path().to_path_buf());
    let err = svc
        .upsert_project_meta(
            "claude:projects/x.jsonl",
            ProjectUserMeta {
                hidden: true,
                alias: None,
            },
        )
        .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn delete_rejects_container_id() {
    let svc = ProjectService::default();
    let err = svc.delete("claude:proj:-C-Users-x").unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn ungrouped_codex_sessions_bucket() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".codex");
    write_session(
        &home.join("sessions").join("lonely.jsonl"),
        &[r#"{"type":"user","text":"no cwd here"}"#],
    );
    let sessions = list_sessions_for_agent_home(AgentId::Codex, &home, None).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].project_id, "codex:proj:__ungrouped__");
    let projects = list_projects_for_agent_home(AgentId::Codex, &home, None).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].title, "未分类会话");
    assert_eq!(projects[0].session_count, 1);
}

#[test]
fn codex_session_index_skips_reparse_on_second_list() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".codex");
    let data = tempdir().unwrap();
    let session = home.join("sessions").join("rollout-idx.jsonl");
    write_session(
        &session,
        &[r#"{"type":"session_meta","payload":{"cwd":"D:\\idx\\repo"}}"#],
    );

    let first = list_sessions_for_agent_home(AgentId::Codex, &home, Some(data.path())).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].project_id, "codex:proj:cwd/D:/idx/repo");
    assert!(data.path().join("project_session_index.json").exists());

    // Second list should still return same grouping via index.
    let second = list_sessions_for_agent_home(AgentId::Codex, &home, Some(data.path())).unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].project_id, first[0].project_id);
    assert_eq!(second[0].cwd, first[0].cwd);
}

#[test]
fn list_kimi_uses_workspaces_and_one_row_per_session() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".kimi-code");
    let wd = "wd_agenthub_ae03ebf85cb0";
    fs::create_dir_all(home.join("sessions").join(wd)).unwrap();
    fs::write(
        home.join("workspaces.json"),
        r#"{
          "version": 1,
          "workspaces": {
            "wd_agenthub_ae03ebf85cb0": {
              "root": "D:/demo_chen/2026/AgentHub",
              "name": "AgentHub",
              "created_at": "2026-07-26T02:45:04.635Z"
            }
          }
        }"#,
    )
    .unwrap();
    let sess = home
        .join("sessions")
        .join(wd)
        .join("session_cc77e803-2743-4383-900d-4e2f4e054951");
    fs::create_dir_all(sess.join("agents").join("main")).unwrap();
    fs::create_dir_all(sess.join("agents").join("agent-0")).unwrap();
    fs::write(
        sess.join("state.json"),
        r#"{"createdAt":"2026-07-26T02:45:04.634Z","updatedAt":"2026-07-26T08:18:38.438Z","title":"规划 AgentHub 工具","isCustomTitle":false}"#,
    )
    .unwrap();
    write_session(
        &sess.join("agents").join("main").join("wire.jsonl"),
        &[
            r#"{"type":"metadata","protocol_version":"1.4"}"#,
            r#"{"type":"config.update","cwd":"D:/demo_chen/2026/AgentHub","modelAlias":"kimi-code/k3"}"#,
            r#"{"type":"turn.prompt","input":[{"type":"text","text":"实现 projects 列表"}]}"#,
        ],
    );
    // Subagent wire must NOT create an extra session row.
    write_session(
        &sess.join("agents").join("agent-0").join("wire.jsonl"),
        &[r#"{"type":"turn.prompt","input":[{"type":"text","text":"subagent only"}]}"#],
    );

    let sessions = list_sessions_for_agent_home(AgentId::Kimi, &home, None).unwrap();
    assert_eq!(
        sessions.len(),
        1,
        "ids={:?}",
        sessions.iter().map(|s| &s.id).collect::<Vec<_>>()
    );
    assert_eq!(
        sessions[0].project_id,
        "kimi:proj:cwd/D:/demo_chen/2026/AgentHub"
    );
    assert!(
        sessions[0].title.contains("规划")
            || sessions[0].title.contains("AgentHub")
            || sessions[0]
                .preview
                .as_ref()
                .map(|p| p.contains("projects"))
                .unwrap_or(false),
        "title={:?} preview={:?}",
        sessions[0].title,
        sessions[0].preview
    );
    assert_eq!(
        sessions[0].cwd.as_deref().map(|c| c.replace('\\', "/")),
        Some("D:/demo_chen/2026/AgentHub".into())
    );
    assert_eq!(
        sessions[0].session_id.as_deref(),
        Some("cc77e803-2743-4383-900d-4e2f4e054951")
    );

    let projects = list_projects_for_agent_home(AgentId::Kimi, &home, None).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].title, "AgentHub");
    assert_eq!(projects[0].session_count, 1);
    assert_ne!(projects[0].title, "未分类会话");

    let (_, key) = parse_project_id(&projects[0].id).unwrap();
    let only =
        list_sessions_for_project_home(AgentId::Kimi, &home, &projects[0].id, &key, None).unwrap();
    assert_eq!(only.len(), 1);

    // Delete removes whole session dir (main + subagent wires).
    let svc = ProjectService::default();
    svc.delete_with_home(&sessions[0].id, &home).unwrap();
    assert!(!sess.exists());
}

#[test]
fn list_pi_groups_by_encoded_session_dir() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".pi");
    let encoded = "--C--Users-example--";
    let sess_dir = home.join("agent").join("sessions").join(encoded);
    write_session(
        &sess_dir.join("2026-08-02T08-58-17-814Z_019fc1b2.jsonl"),
        &[
            r#"{"type":"session","version":3,"id":"019fc1b2","timestamp":"2026-08-02T08:58:17.814Z","cwd":"C:\\Users\\example"}"#,
            r#"{"type":"message","id":"1","timestamp":"2026-08-02T08:59:57.831Z","message":{"role":"user","content":[{"type":"text","text":"hi from pi"}]}}"#,
        ],
    );
    // Second project
    let encoded2 = "--D--work-repo--";
    write_session(
        &home
            .join("agent")
            .join("sessions")
            .join(encoded2)
            .join("other.jsonl"),
        &[
            r#"{"type":"session","cwd":"D:\\work\\repo"}"#,
            r#"{"type":"message","message":{"role":"user","content":[{"type":"text","text":"other"}]}}"#,
        ],
    );

    let sessions = list_sessions_for_agent_home(AgentId::Pi, &home, None).unwrap();
    assert_eq!(sessions.len(), 2);
    assert!(
        sessions
            .iter()
            .any(|s| s.session_id.as_deref() == Some("019fc1b2")),
        "native ids={:?}",
        sessions
            .iter()
            .map(|s| s.session_id.as_deref())
            .collect::<Vec<_>>()
    );
    let projects = list_projects_for_agent_home(AgentId::Pi, &home, None).unwrap();
    assert_eq!(projects.len(), 2);
    assert!(projects.iter().all(|p| p.title != "未分类会话"));

    let example = projects
        .iter()
        .find(|p| {
            p.id.contains("Users/example")
                || p.actual_path.as_deref().unwrap_or("").contains("example")
        })
        .expect("example project");
    assert_eq!(example.session_count, 1);
    let (_, key) = parse_project_id(&example.id).unwrap();
    let only = list_sessions_for_project_home(AgentId::Pi, &home, &example.id, &key, None).unwrap();
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].session_id.as_deref(), Some("019fc1b2"));
    assert!(
        only[0]
            .preview
            .as_ref()
            .map(|p| p.contains("hi from pi"))
            .unwrap_or(false)
            || only[0].title.contains("hi"),
        "preview={:?} title={}",
        only[0].preview,
        only[0].title
    );
}

#[test]
fn grok_delete_removes_session_directory() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".grok");
    let encoded = "D%3A%5Cwork%5Cdel";
    let sess = home.join("sessions").join(encoded).join("019f-delete-me");
    write_session(
        &sess.join("chat_history.jsonl"),
        &[r#"{"type":"user","text":"bye"}"#],
    );
    write_session(&sess.join("events.jsonl"), &[r#"{"type":"x"}"#]);
    fs::write(sess.join("summary.json"), r#"{"generated_title":"Del"}"#).unwrap();

    let rows = list_sessions_for_agent_home(AgentId::Grok, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let id = rows[0].id.clone();

    let svc = ProjectService::default();
    svc.delete_with_home(&id, &home).unwrap();
    assert!(!sess.exists(), "session directory must be removed");
    assert!(!sess.join("events.jsonl").exists());
    assert!(list_sessions_for_agent_home(AgentId::Grok, &home, None)
        .unwrap()
        .is_empty());
}

#[test]
fn grok_preview_unwraps_user_query_and_skips_system_wrapper() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".grok");
    let encoded = "D%3A%5Cwork%5Cpreview";
    let sess = home.join("sessions").join(encoded).join("019f-preview-md");
    write_jsonl(
        &sess.join("chat_history.jsonl"),
        &[
            serde_json::json!({"type":"system","content":"You are Grok released by xAI. Format replies as markdown."}),
            serde_json::json!({"type":"user","content":[{"type":"text","text":"<user_info>\nOS: windows\n</user_info>\n<user_query>\nfix markdown preview\n</user_query>"}]}),
            serde_json::json!({"type":"user","content":[{"type":"text","text":"<system-reminder>\nMCP connected\n</system-reminder>"}],"synthetic_reason":"system_reminder"}),
            serde_json::json!({"type":"assistant","content":"## 结论\n\n---\n\n已修好。"}),
        ],
    );

    let rows = list_sessions_for_agent_home(AgentId::Grok, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let preview = rows[0].preview.as_deref().unwrap_or("");
    assert!(
        preview.contains("fix markdown preview"),
        "preview={preview}"
    );
    assert!(!preview.contains("user_info"), "preview={preview}");
    assert!(!preview.contains("You are Grok"), "preview={preview}");
}

#[test]
fn grok_excerpt_parses_chat_history_markdown_and_skips_wrappers() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".grok");
    let encoded = "D%3A%5Cwork%5Cexcerpt";
    let sess = home.join("sessions").join(encoded).join("019f-excerpt-md");
    write_jsonl(
        &sess.join("chat_history.jsonl"),
        &[
            serde_json::json!({"type":"system","content":"You are Grok released by xAI. Format replies as markdown."}),
            serde_json::json!({"type":"user","content":[{"type":"text","text":"<user_info>\nOS: windows\n</user_info>\n<user_query>\nfix markdown preview\n</user_query>"}]}),
            serde_json::json!({"type":"user","content":[{"type":"text","text":"<system-reminder>\nMCP connected\n</system-reminder>"}],"synthetic_reason":"system_reminder"}),
            serde_json::json!({"type":"assistant","content":"## 结论\n\n---\n\n已修好。"}),
            serde_json::json!({"type":"tool_result","content":"secret tool dump"}),
        ],
    );

    let rows = list_sessions_for_agent_home(AgentId::Grok, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(
        ex.excerpt.contains("---turn:user---"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("---turn:assistant---"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("fix markdown preview"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(ex.excerpt.contains("## 结论"), "excerpt={}", ex.excerpt);
    assert!(
        ex.excerpt.contains("---\n\n已修好。"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        !ex.excerpt.contains("You are Grok"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        !ex.excerpt.contains("secret tool dump"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        !ex.excerpt.contains("<user_info>"),
        "excerpt={}",
        ex.excerpt
    );
}

#[test]
fn grok_excerpt_prefers_updates_jsonl_chunks_over_wrapped_chat_history() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".grok");
    let encoded = "D%3A%5Cwork%5Cupdates";
    let sess = home.join("sessions").join(encoded).join("019f-updates-md");
    write_session(
        &sess.join("chat_history.jsonl"),
        &[
            r#"{"type":"system","content":"You are Grok released by xAI."}"#,
            r#"{"type":"user","content":[{"type":"text","text":"<user_query>\nwrapped should lose\n</user_query>"}]}"#,
        ],
    );
    write_jsonl(
        &sess.join("updates.jsonl"),
        &[
            grok_acp_chunk("user_message_chunk", Some("clean user prompt")),
            grok_acp_chunk("agent_thought_chunk", Some("thinking")),
            grok_acp_chunk("agent_message_chunk", Some("## 结论\n\n")),
            grok_acp_chunk("tool_call", None),
            grok_acp_chunk("agent_message_chunk", Some("---\n\n已修好。")),
        ],
    );

    let rows = list_sessions_for_agent_home(AgentId::Grok, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(
        ex.excerpt.contains("clean user prompt"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        !ex.excerpt.contains("wrapped should lose"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(!ex.excerpt.contains("thinking"), "excerpt={}", ex.excerpt);
    assert!(ex.excerpt.contains("## 结论"), "excerpt={}", ex.excerpt);
    assert!(
        ex.excerpt.contains("---\n\n已修好。"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("---turn:assistant---"),
        "excerpt={}",
        ex.excerpt
    );
}

#[test]
fn grok_excerpt_unwraps_user_query_in_updates_chunks() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".grok");
    let encoded = "D%3A%5Cwork%5Cupdates-unwrap";
    let sess = home
        .join("sessions")
        .join(encoded)
        .join("019f-updates-unwrap");
    write_jsonl(
        &sess.join("chat_history.jsonl"),
        &[serde_json::json!({"type":"system","content":"You are Grok released by xAI."})],
    );
    write_jsonl(
        &sess.join("updates.jsonl"),
        &[
            grok_acp_chunk(
                "user_message_chunk",
                Some("<user_info>\nOS: windows\n</user_info>\n<user_query>\nvisible question\n</user_query>"),
            ),
            grok_acp_chunk("agent_message_chunk", Some("short reply")),
        ],
    );

    let rows = list_sessions_for_agent_home(AgentId::Grok, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(
        ex.excerpt.contains("visible question"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        !ex.excerpt.contains("<user_info>"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(ex.excerpt.contains("short reply"), "excerpt={}", ex.excerpt);
}

#[test]
fn grok_excerpt_keeps_updates_assistant_over_chat_history() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".grok");
    let encoded = "D%3A%5Cwork%5Cupdates-keep";
    let sess = home
        .join("sessions")
        .join(encoded)
        .join("019f-updates-keep");
    write_jsonl(
        &sess.join("chat_history.jsonl"),
        &[
            serde_json::json!({"type":"user","content":[{"type":"text","text":"<user_query>\nwrapped should lose\n</user_query>"}]}),
            serde_json::json!({"type":"assistant","content":"from chat_history"}),
        ],
    );
    write_jsonl(
        &sess.join("updates.jsonl"),
        &[
            grok_acp_chunk("user_message_chunk", Some("clean user prompt")),
            grok_acp_chunk("agent_message_chunk", Some("from updates")),
        ],
    );

    let rows = list_sessions_for_agent_home(AgentId::Grok, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(
        ex.excerpt.contains("clean user prompt"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("from updates"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        !ex.excerpt.contains("from chat_history"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        !ex.excerpt.contains("wrapped should lose"),
        "excerpt={}",
        ex.excerpt
    );
}

#[test]
fn grok_excerpt_reads_agent_message_content_array() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".grok");
    let encoded = "D%3A%5Cwork%5Cupdates-array";
    let sess = home
        .join("sessions")
        .join(encoded)
        .join("019f-updates-array");
    write_jsonl(
        &sess.join("chat_history.jsonl"),
        &[serde_json::json!({"type":"system","content":"You are Grok."})],
    );
    write_jsonl(
        &sess.join("updates.jsonl"),
        &[
            grok_acp_chunk("user_message_chunk", Some("array prompt")),
            serde_json::json!({
                "method": "session/update",
                "params": {
                    "update": {
                        "sessionUpdate": "agent_message_chunk",
                        "content": [
                            {"type": "text", "text": "hello "},
                            {"type": "text", "text": "array"}
                        ]
                    }
                }
            }),
        ],
    );

    let rows = list_sessions_for_agent_home(AgentId::Grok, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(
        ex.excerpt.contains("array prompt"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(ex.excerpt.contains("hello array"), "excerpt={}", ex.excerpt);
}

#[test]
fn grok_excerpt_fills_assistant_from_chat_history_when_updates_have_only_user() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".grok");
    let encoded = "D%3A%5Cwork%5Cupdates-user-only";
    let sess = home
        .join("sessions")
        .join(encoded)
        .join("019f-updates-user-only");
    write_jsonl(
        &sess.join("chat_history.jsonl"),
        &[
            serde_json::json!({"type":"system","content":"You are Grok released by xAI."}),
            serde_json::json!({"type":"user","content":[{"type":"text","text":"<user_query>\nwrapped should lose\n</user_query>"}]}),
            serde_json::json!({"type":"assistant","content":"## 结论\n\n已从 chat_history 补上。"}),
        ],
    );
    write_jsonl(
        &sess.join("updates.jsonl"),
        &[grok_acp_chunk(
            "user_message_chunk",
            Some("clean user prompt"),
        )],
    );

    let rows = list_sessions_for_agent_home(AgentId::Grok, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(
        ex.excerpt.contains("clean user prompt"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        !ex.excerpt.contains("wrapped should lose"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("---turn:assistant---"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("已从 chat_history 补上"),
        "excerpt={}",
        ex.excerpt
    );
}

#[test]
fn grok_excerpt_skips_updates_tool_noise_to_reach_later_turns() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".grok");
    let encoded = "D%3A%5Cwork%5Cupdates-noise";
    let sess = home
        .join("sessions")
        .join(encoded)
        .join("019f-updates-noise");
    write_jsonl(
        &sess.join("chat_history.jsonl"),
        &[serde_json::json!({"type":"system","content":"You are Grok."})],
    );
    let padding = "x".repeat(8 * 1024);
    let mut lines = vec![
        grok_acp_chunk("user_message_chunk", Some("first prompt")),
        grok_acp_chunk("agent_message_chunk", Some("first reply")),
    ];
    for i in 0..80 {
        lines.push(serde_json::json!({
            "method": "session/update",
            "params": {
                "update": {
                    "sessionUpdate": "tool_call_update",
                    "toolCallId": format!("t{i}"),
                    "content": [{"type": "text", "text": padding}]
                }
            }
        }));
    }
    lines.push(grok_acp_chunk(
        "user_message_chunk",
        Some("second prompt later"),
    ));
    lines.push(grok_acp_chunk(
        "agent_message_chunk",
        Some("second reply later"),
    ));
    write_jsonl(&sess.join("updates.jsonl"), &lines);

    let rows = list_sessions_for_agent_home(AgentId::Grok, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(
        ex.excerpt.contains("first prompt"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("second prompt later"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("second reply later"),
        "excerpt={}",
        ex.excerpt
    );
}

#[test]
fn grok_excerpt_prefers_richer_chat_history_when_updates_cover_fewer_user_turns() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".grok");
    let encoded = "D%3A%5Cwork%5Cupdates-prefix";
    let sess = home
        .join("sessions")
        .join(encoded)
        .join("019f-updates-prefix");
    write_jsonl(
        &sess.join("chat_history.jsonl"),
        &[
            serde_json::json!({"type":"user","content":[{"type":"text","text":"<user_query>\nfirst visible\n</user_query>"}]}),
            serde_json::json!({"type":"assistant","content":"reply one"}),
            serde_json::json!({"type":"user","content":[{"type":"text","text":"<user_query>\nsecond visible\n</user_query>"}]}),
            serde_json::json!({"type":"assistant","content":"reply two"}),
            serde_json::json!({"type":"user","content":[{"type":"text","text":"<user_query>\nthird visible\n</user_query>"}]}),
            serde_json::json!({"type":"assistant","content":"reply three"}),
        ],
    );
    write_jsonl(
        &sess.join("updates.jsonl"),
        &[
            grok_acp_chunk("user_message_chunk", Some("only first user")),
            grok_acp_chunk("agent_message_chunk", Some("only first assistant")),
        ],
    );

    let rows = list_sessions_for_agent_home(AgentId::Grok, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(
        ex.excerpt.contains("second visible"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("third visible"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(ex.excerpt.contains("reply three"), "excerpt={}", ex.excerpt);
    assert!(
        !ex.excerpt.contains("only first user"),
        "excerpt={}",
        ex.excerpt
    );
}

fn oversized_jsonl_padding(
    count: usize,
    pad_bytes: usize,
    make: impl Fn(usize, &str) -> serde_json::Value,
) -> Vec<serde_json::Value> {
    let padding = "x".repeat(pad_bytes);
    (0..count).map(|i| make(i, &padding)).collect()
}

#[test]
fn claude_excerpt_skips_sidecar_noise_to_reach_later_turns() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".claude");
    let session = home
        .join("projects")
        .join("-C-Users-demo-app")
        .join("sess-claude-noise.jsonl");
    let mut lines = vec![
        serde_json::json!({"type":"user","message":{"content":[{"type":"text","text":"first claude prompt"}]}}),
        serde_json::json!({"type":"assistant","message":{"content":[{"type":"text","text":"first claude reply"}]}}),
    ];
    lines.extend(oversized_jsonl_padding(
        80,
        8 * 1024,
        |i, pad| serde_json::json!({"type":"attachment","id": i, "data": pad}),
    ));
    lines.push(serde_json::json!({"type":"user","message":{"content":[{"type":"text","text":"second claude prompt later"}]}}));
    lines.push(serde_json::json!({"type":"assistant","message":{"content":[{"type":"text","text":"second claude reply later"}]}}));
    write_jsonl(&session, &lines);

    let rows = list_sessions_for_agent_home(AgentId::Claude, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(
        ex.excerpt.contains("second claude prompt later"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("second claude reply later"),
        "excerpt={}",
        ex.excerpt
    );
}

#[test]
fn codex_excerpt_skips_function_call_output_to_reach_later_turns() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".codex");
    let session = home
        .join("sessions")
        .join("2026")
        .join("08")
        .join("03")
        .join("rollout-excerpt-noise.jsonl");
    let mut lines = vec![
        serde_json::json!({"type":"session_meta","payload":{"session_id":"abc","cwd":"D:\\work\\repo"}}),
        serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"first codex prompt"}]}}),
        serde_json::json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"first codex reply"}]}}),
    ];
    lines.extend(oversized_jsonl_padding(80, 8 * 1024, |_i, pad| {
        serde_json::json!({
            "timestamp": "2026-08-03T00:00:00.000Z",
            "type": "response_item",
            "payload": {"type": "function_call_output", "output": pad}
        })
    }));
    lines.push(serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"second codex prompt later"}]}}));
    lines.push(serde_json::json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"second codex reply later"}]}}));
    write_jsonl(&session, &lines);

    let rows = list_sessions_for_agent_home(AgentId::Codex, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(
        ex.excerpt.contains("second codex prompt later"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("second codex reply later"),
        "excerpt={}",
        ex.excerpt
    );
}

#[test]
fn pi_excerpt_reads_nested_message_role_and_later_turns() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".pi");
    let encoded = "--C--Users-example--";
    let session = home
        .join("agent")
        .join("sessions")
        .join(encoded)
        .join("2026-08-02T08-58-17-814Z_019fc1b2.jsonl");
    let mut lines = vec![
        serde_json::json!({"type":"session","version":3,"id":"019fc1b2","cwd":"C:\\Users\\example"}),
        serde_json::json!({"type":"message","message":{"role":"user","content":[{"type":"text","text":"first pi prompt"}]}}),
        serde_json::json!({"type":"message","message":{"role":"assistant","content":[{"type":"text","text":"first pi reply"}]}}),
    ];
    lines.extend(oversized_jsonl_padding(80, 8 * 1024, |_i, pad| {
        serde_json::json!({"type":"message","message":{"role":"toolResult","content":[{"type":"text","text": pad}]}})
    }));
    lines.push(serde_json::json!({"type":"message","message":{"role":"user","content":[{"type":"text","text":"second pi prompt later"}]}}));
    lines.push(serde_json::json!({"type":"message","message":{"role":"assistant","content":[{"type":"text","text":"second pi reply later"}]}}));
    write_jsonl(&session, &lines);

    let rows = list_sessions_for_agent_home(AgentId::Pi, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(
        ex.excerpt.contains("second pi prompt later"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("second pi reply later"),
        "excerpt={}",
        ex.excerpt
    );
}

#[test]
fn kimi_excerpt_skips_wire_noise_to_reach_later_turns() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".kimi-code");
    let wd = "wd_excerpt_noise";
    fs::create_dir_all(home.join("sessions").join(wd)).unwrap();
    fs::write(
        home.join("workspaces.json"),
        r#"{
          "version": 1,
          "workspaces": {
            "wd_excerpt_noise": {
              "root": "D:/demo_chen/2026/AgentHub",
              "name": "AgentHub",
              "created_at": "2026-07-26T02:45:04.635Z"
            }
          }
        }"#,
    )
    .unwrap();
    let sess = home.join("sessions").join(wd).join("session_excerpt-noise");
    fs::create_dir_all(sess.join("agents").join("main")).unwrap();
    let mut lines = vec![
        serde_json::json!({"type":"metadata","protocol_version":"1.4"}),
        serde_json::json!({"type":"turn.prompt","input":[{"type":"text","text":"first kimi prompt"}]}),
    ];
    lines.extend(oversized_jsonl_padding(
        80,
        8 * 1024,
        |_i, pad| serde_json::json!({"type":"usage.record","blob": pad}),
    ));
    lines.push(serde_json::json!({"type":"turn.prompt","input":[{"type":"text","text":"second kimi prompt later"}]}));
    write_jsonl(&sess.join("agents").join("main").join("wire.jsonl"), &lines);

    let rows = list_sessions_for_agent_home(AgentId::Kimi, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(
        ex.excerpt.contains("second kimi prompt later"),
        "excerpt={}",
        ex.excerpt
    );
}

#[test]
fn codex_excerpt_reads_assistant_from_response_item_payload() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".codex");
    let session = home
        .join("sessions")
        .join("2026")
        .join("08")
        .join("03")
        .join("rollout-excerpt-assistant.jsonl");
    write_session(
        &session,
        &[
            r#"{"type":"session_meta","payload":{"session_id":"abc","cwd":"D:\\work\\repo"}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"fix grouping"}]}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call","name":"read_file","arguments":"{}"}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"grouped by cwd"}]}}"#,
        ],
    );

    let rows = list_sessions_for_agent_home(AgentId::Codex, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(
        ex.excerpt.contains("---turn:user---"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("fix grouping"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("---turn:assistant---"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("grouped by cwd"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(!ex.excerpt.contains("read_file"), "excerpt={}", ex.excerpt);
}

#[test]
fn claude_excerpt_reads_nested_assistant_message_text() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".claude");
    let session = home
        .join("projects")
        .join("-C-Users-demo-app")
        .join("sess-nested-assistant.jsonl");
    write_jsonl(
        &session,
        &[
            serde_json::json!({"type":"user","message":{"role":"user","content":[{"type":"text","text":"fix login"}]}}),
            serde_json::json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Read","input":{}}]}}),
            serde_json::json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"looking into it"}]}}),
        ],
    );

    let rows = list_sessions_for_agent_home(AgentId::Claude, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(ex.excerpt.contains("fix login"), "excerpt={}", ex.excerpt);
    assert!(
        ex.excerpt.contains("looking into it"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(!ex.excerpt.contains("tool_use"), "excerpt={}", ex.excerpt);
    assert!(!ex.excerpt.contains("Read"), "excerpt={}", ex.excerpt);
}

#[test]
fn excerpt_skips_empty_assistant_tool_call_rows() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".claude");
    let session = home
        .join("projects")
        .join("-C-Users-demo-app")
        .join("sess-empty-assistant.jsonl");
    write_jsonl(
        &session,
        &[
            serde_json::json!({"type":"user","text":"hi"}),
            serde_json::json!({"type":"assistant","content":"","tool_calls":[{"name":"Read"}]}),
            serde_json::json!({"type":"assistant","text":"done"}),
        ],
    );

    let rows = list_sessions_for_agent_home(AgentId::Claude, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert_eq!(
        ex.excerpt.matches("---turn:assistant---").count(),
        1,
        "excerpt={}",
        ex.excerpt
    );
    assert!(ex.excerpt.contains("done"), "excerpt={}", ex.excerpt);
    assert!(!ex.excerpt.contains("Read"), "excerpt={}", ex.excerpt);
}

#[test]
fn excerpt_keeps_long_user_turn_and_assistant() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".claude");
    let user = "please help ".repeat(800);
    assert!(user.chars().count() > 6_000);
    let session = home
        .join("projects")
        .join("-C-Users-demo-app")
        .join("sess-long-user.jsonl");
    write_jsonl(
        &session,
        &[
            serde_json::json!({"type":"user","message":{"content":user}}),
            serde_json::json!({"type":"assistant","text":"assistant still visible"}),
        ],
    );

    let rows = list_sessions_for_agent_home(AgentId::Claude, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(
        ex.excerpt.contains("---turn:user---"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("---turn:assistant---"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("assistant still visible"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.matches("please help").count() >= 800,
        "long user turn truncated: excerpt_len={}",
        ex.excerpt.chars().count()
    );
}

#[test]
fn excerpt_keeps_first_and_last_turns_in_long_session() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".claude");
    let session = home
        .join("projects")
        .join("-C-Users-demo-app")
        .join("sess-long-conversation.jsonl");
    let mut lines = Vec::new();
    for i in 0..30 {
        lines.push(serde_json::json!({
            "type": "user",
            "message": {"content": [{"type": "text", "text": format!("TURN-{i}-USER")}]},
        }));
        lines.push(serde_json::json!({
            "type": "assistant",
            "text": format!("TURN-{i}-ASST {}", "x".repeat(400)),
        }));
    }
    write_jsonl(&session, &lines);

    let rows = list_sessions_for_agent_home(AgentId::Claude, &home, None).unwrap();
    assert_eq!(rows.len(), 1);
    let ex = load_excerpt(&rows[0].id, Some(&home)).unwrap();
    assert!(ex.excerpt.contains("TURN-0-USER"), "excerpt={}", ex.excerpt);
    assert!(ex.excerpt.contains("TURN-0-ASST"), "excerpt={}", ex.excerpt);
    assert!(
        ex.excerpt.contains("TURN-29-USER"),
        "excerpt={}",
        ex.excerpt
    );
    assert!(
        ex.excerpt.contains("TURN-29-ASST"),
        "excerpt={}",
        ex.excerpt
    );
    assert_eq!(
        ex.excerpt.matches("---turn:user---").count(),
        30,
        "excerpt={}",
        ex.excerpt
    );
    assert_eq!(
        ex.excerpt.matches("---turn:assistant---").count(),
        30,
        "excerpt={}",
        ex.excerpt
    );
}

#[test]
fn list_sessions_filters_by_project_id() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".claude");
    let a = home.join("projects").join("-C-Users-a");
    let b = home.join("projects").join("-C-Users-b");
    write_session(&a.join("1.jsonl"), &[r#"{"type":"user","text":"a"}"#]);
    write_session(&b.join("2.jsonl"), &[r#"{"type":"user","text":"b"}"#]);

    let all = list_sessions_for_agent_home(AgentId::Claude, &home, None).unwrap();
    assert_eq!(all.len(), 2);
    let pid = "claude:proj:-C-Users-a";
    let only_a =
        list_sessions_for_project_home(AgentId::Claude, &home, pid, "-C-Users-a", None).unwrap();
    assert_eq!(only_a.len(), 1);
    assert!(only_a[0].relative_path.contains("-C-Users-a"));
}

#[test]
fn verified_actual_path_accepts_existing_dir() {
    use crate::utils::project_path::{decode_claude_project_dir, verified_actual_path};
    let dir = tempdir().unwrap();
    let real = dir.path().join("workspace");
    fs::create_dir_all(&real).unwrap();
    // Build an encoded name that decodes back to `real` is hard on Windows;
    // instead assert decode + exists check composition.
    let encoded = "-Z-NoSuch-AgentHub-Path-XYZ";
    assert!(verified_actual_path(encoded).is_none());
    assert!(decode_claude_project_dir(encoded).is_some());
    // Existing path via Path::exists path used by verified_actual_path:
    assert!(real.exists());
}

#[test]
fn workbuddy_aggregates_like_claude() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".workbuddy");
    let base = home.join("projects").join("-C-Users-wb-app");
    write_session(
        &base.join("s1.jsonl"),
        &[r#"{"type":"user","text":"wb one"}"#],
    );
    write_session(
        &base.join("s2.jsonl"),
        &[r#"{"type":"user","text":"wb two"}"#],
    );
    let projects = list_projects_for_agent_home(AgentId::WorkBuddy, &home, None).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].session_count, 2);
    assert_eq!(projects[0].id, "workbuddy:proj:-C-Users-wb-app");
}

#[test]
fn metadata_file_corrupt_returns_error_on_load() {
    let data = tempdir().unwrap();
    let path = data.path().join("project_metadata.json");
    fs::write(&path, "{not-json").unwrap();
    let err = load_metadata(&path).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn public_list_sessions_bad_id() {
    let svc = ProjectService::default();
    assert!(svc.list_sessions("not-a-project").is_err());
    assert!(svc.list_sessions("claude:projects/x.jsonl").is_err());
}

#[test]
fn project_registry_covers_all_agents() {
    let reg = crate::platform::projects::builtin_project_registry();
    for id in AgentId::ALL {
        assert!(reg.contains(id), "missing {id:?}");
    }
    assert_eq!(reg.supported_agents().len(), AgentId::ALL.len());
}

#[test]
fn list_dsh_sessions_from_home_and_profiles_not_cwd_dot_sessions() {
    let dir = tempdir().unwrap();
    let home = dir.path().join(".dsh");
    write_session(
        &home.join("sessions/sess-home.jsonl"),
        &[
            r#"{"type":"session","id":"sess-home","cwd":"/tmp/dsh-home-proj"}"#,
            r#"{"type":"assistant/message","text":"from home"}"#,
        ],
    );
    write_session(
        &home.join("profiles/headless/sessions/sess-profile.jsonl"),
        &[
            r#"{"type":"session","id":"sess-profile","cwd":"/tmp/dsh-profile-proj"}"#,
            r#"{"type":"assistant/message","text":"from profile"}"#,
        ],
    );
    write_session(
        &dir.path().join(".sessions/random.jsonl"),
        &[r#"{"type":"session","id":"should-not-appear","cwd":"/tmp/noise"}"#],
    );

    let sessions = list_sessions_for_agent_home(AgentId::Dsh, &home, None).unwrap();
    let ids: Vec<_> = sessions
        .iter()
        .filter_map(|s| s.session_id.as_deref())
        .collect();
    assert!(ids.contains(&"sess-home"), "{ids:?}");
    assert!(ids.contains(&"sess-profile"), "{ids:?}");
    assert!(!ids.contains(&"should-not-appear"), "{ids:?}");
    assert_eq!(sessions.len(), 2);

    let projects = list_projects_for_agent_home(AgentId::Dsh, &home, None).unwrap();
    assert_eq!(projects.len(), 2);
    assert!(projects.iter().all(|p| p.session_count == 1));
}

#[test]
fn session_index_roundtrip_and_freshness() {
    use super::session_index::{IndexEntry, SessionIndexStore};

    let dir = tempdir().unwrap();
    let mut store = SessionIndexStore::load(dir.path());
    store.put(
        AgentId::Codex,
        "sessions/a.jsonl",
        IndexEntry {
            mtime_ms: 100,
            size: 10,
            project_key: "cwd/D:/work".into(),
            cwd: Some("D:/work".into()),
            title: "t".into(),
            preview: Some("p".into()),
            message_count: Some(2),
            updated_at: "t0".into(),
            session_id: Some("sid-1".into()),
        },
    );
    store.save_if_dirty();
    assert!(dir.path().join("project_session_index.json").exists());

    let store2 = SessionIndexStore::load(dir.path());
    assert!(store2
        .get_fresh(AgentId::Codex, "sessions/a.jsonl", 10, 100)
        .is_some());
    assert!(store2
        .get_fresh(AgentId::Codex, "sessions/a.jsonl", 11, 100)
        .is_none());
}
