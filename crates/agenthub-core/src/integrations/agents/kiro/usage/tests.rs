use std::fs;
use std::path::Path;

use super::collect_kiro_usage;

fn write_session(home: &Path, name: &str, body: &str) {
    let dir = home.join("sessions").join("cli");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(name), body).unwrap();
}

fn session_json(turns: &str) -> String {
    format!(
        r#"{{
  "session_id": "sess-a",
  "session_state": {{
    "conversation_metadata": {{
      "user_turn_metadatas": [{turns}]
    }},
    "rts_model_state": {{
      "model_info": {{ "model_id": "auto", "context_window_tokens": 200000 }}
    }}
  }}
}}"#
    )
}

fn turn(
    end: &str,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    msg: &str,
) -> String {
    format!(
        r#"{{
      "end_timestamp": "{end}",
      "input_token_count": {input},
      "output_token_count": {output},
      "cache_read_input_token_count": {cache_read},
      "cache_write_input_token_count": {cache_write},
      "model": "auto",
      "message_ids": ["{msg}"]
    }}"#
    )
}

#[test]
fn harvests_ended_turns_including_zero_tokens() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    write_session(
        home,
        "sess-a.json",
        &session_json(&format!(
            "{},{}",
            turn("2026-09-07T01:15:06Z", 0, 0, 0, 0, "m0"),
            turn("2026-09-07T01:16:00Z", 120, 30, 10, 4, "m1"),
        )),
    );
    write_session(
        home,
        "ignore.jsonl",
        r#"{"kind":"Prompt"}
"#,
    );

    let events = collect_kiro_usage(home);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].session_id.as_deref(), Some("sess-a"));
    assert_eq!(events[0].input_tokens, 0);
    assert_eq!(events[0].raw_hash, "kiro:sess-a:m0");
    assert_eq!(events[1].input_tokens, 120);
    assert_eq!(events[1].output_tokens, 30);
    assert_eq!(events[1].cache_read_tokens, 10);
    assert_eq!(events[1].cache_creation_tokens, 4);
    assert_eq!(events[1].model, "auto");
    assert_eq!(events[1].raw_hash, "kiro:sess-a:m1");
    assert!(events.iter().all(|ev| ev.cost_usd.is_none()));
}

#[test]
fn skips_unfinished_turns_and_editor_trees() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    write_session(
        home,
        "open.json",
        &session_json(
            r#"{
      "input_token_count": 9,
      "output_token_count": 1,
      "model": "auto",
      "message_ids": ["open"]
    }"#,
        ),
    );
    let ide = home.join("sessions").join("abc").join("sess_x");
    fs::create_dir_all(&ide).unwrap();
    fs::write(
        ide.join("messages.jsonl"),
        r#"{"payload":{"type":"user"}}
"#,
    )
    .unwrap();

    assert!(collect_kiro_usage(home).is_empty());
}

#[test]
fn missing_cli_dir_is_empty() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(collect_kiro_usage(tmp.path()).is_empty());
}

#[test]
fn collect_pipeline_harvests_registered_source() {
    use crate::models::AgentId;
    use crate::platform::usage::collect_for_agent_id;
    use crate::storage::{Database, UsageRepo};
    use crate::utils::test_env::{lock_test_env, EnvVarGuard};

    let _lock = lock_test_env();
    let tmp = tempfile::tempdir().unwrap();
    write_session(
        tmp.path(),
        "sess-a.json",
        &session_json(&turn("2026-09-07T01:15:06Z", 80, 12, 0, 0, "m-pipe")),
    );
    let _home = EnvVarGuard::set("KIRO_HOME", tmp.path());
    let dbdir = tempfile::tempdir().unwrap();
    let db = Database::open(&dbdir.path().join("u.db")).unwrap();
    let repo = UsageRepo::new(db);
    let stats = collect_for_agent_id(AgentId::Kiro, &repo).unwrap();
    assert_eq!(stats.events.len(), 1);
    assert_eq!(stats.events[0].input_tokens, 80);
    assert_eq!(stats.events[0].output_tokens, 12);
    assert_eq!(stats.events[0].raw_hash, "kiro:sess-a:m-pipe");
}

#[test]
fn collect_second_pass_dedupes_unchanged_harvest_rows() {
    use crate::models::AgentId;
    use crate::platform::usage::builtin_usage_registry;
    use crate::services::UsageService;
    use crate::utils::test_env::{lock_test_env, EnvVarGuard};

    let _lock = lock_test_env();
    let tmp = tempfile::tempdir().unwrap();
    write_session(
        tmp.path(),
        "sess-a.json",
        &session_json(&turn("2026-09-07T01:15:06Z", 80, 12, 0, 0, "m-pipe")),
    );
    let _home = EnvVarGuard::set("KIRO_HOME", tmp.path());
    let dbdir = tempfile::tempdir().unwrap();
    let db = crate::storage::Database::open(&dbdir.path().join("u.db")).unwrap();
    let service = UsageService::with_registry(db, builtin_usage_registry().clone());
    let first = service.collect(Some(AgentId::Kiro)).unwrap();
    assert_eq!(first.inserted, 1);
    let second = service.collect(Some(AgentId::Kiro)).unwrap();
    assert_eq!(second.inserted, 0);
}
