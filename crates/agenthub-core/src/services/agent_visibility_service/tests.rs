use super::*;
use crate::models::AgentId;
use tempfile::tempdir;

#[test]
fn missing_file_defaults_empty() {
    let dir = tempdir().unwrap();
    let svc = AgentVisibilityService::new(dir.path().to_path_buf());
    assert!(svc.list_hidden_agents().unwrap().is_empty());
    assert!(!svc.is_hidden(AgentId::Claude).unwrap());
}

#[test]
fn set_and_unset_are_idempotent() {
    let dir = tempdir().unwrap();
    let svc = AgentVisibilityService::new(dir.path().to_path_buf());

    svc.set_agent_hidden(AgentId::Claude, true).unwrap();
    svc.set_agent_hidden(AgentId::Claude, true).unwrap();
    assert_eq!(svc.list_hidden_agents().unwrap(), vec!["claude"]);
    assert!(svc.is_hidden(AgentId::Claude).unwrap());

    svc.set_agent_hidden(AgentId::Claude, false).unwrap();
    svc.set_agent_hidden(AgentId::Claude, false).unwrap();
    assert!(svc.list_hidden_agents().unwrap().is_empty());
    assert!(!svc.is_hidden(AgentId::Claude).unwrap());
}

#[test]
fn unknown_ids_survive_roundtrip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("agent_visibility.json");
    std::fs::write(
        &path,
        r#"{"version":1,"hiddenAgentIds":["future-agent","claude"]}"#,
    )
    .unwrap();

    let svc = AgentVisibilityService::new(dir.path().to_path_buf());
    let ids = svc.list_hidden_agents().unwrap();
    assert_eq!(ids, vec!["future-agent", "claude"]);

    svc.set_agent_hidden(AgentId::Grok, true).unwrap();
    let again = svc.list_hidden_agents().unwrap();
    assert!(again.contains(&"future-agent".to_string()));
    assert!(again.contains(&"claude".to_string()));
    assert!(again.contains(&"grok".to_string()));
}

#[test]
fn cursor_alias_normalizes_on_read() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("agent_visibility.json"),
        r#"{"version":1,"hiddenAgentIds":["cursor-agent"]}"#,
    )
    .unwrap();
    let svc = AgentVisibilityService::new(dir.path().to_path_buf());
    assert_eq!(svc.list_hidden_agents().unwrap(), vec!["cursor"]);
    assert!(svc.is_hidden(AgentId::Cursor).unwrap());
}

#[test]
fn file_lives_under_data_dir() {
    let dir = tempdir().unwrap();
    let svc = AgentVisibilityService::new(dir.path().to_path_buf());
    svc.set_agent_hidden(AgentId::Pi, true).unwrap();
    assert!(dir.path().join("agent_visibility.json").exists());
}

#[test]
fn invalid_json_is_invalid_arg() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("agent_visibility.json"), "{not-json").unwrap();
    let svc = AgentVisibilityService::new(dir.path().to_path_buf());
    let err = svc.list_hidden_agents().unwrap_err();
    match err {
        AppError::InvalidArg(msg) => assert!(msg.contains("agent_visibility.json")),
        other => panic!("expected InvalidArg, got {other:?}"),
    }
}

#[test]
fn concurrent_hides_do_not_drop_ids() {
    let dir = tempdir().unwrap();
    let svc = std::sync::Arc::new(AgentVisibilityService::new(dir.path().to_path_buf()));
    let a = std::thread::spawn({
        let svc = svc.clone();
        move || svc.set_agent_hidden(AgentId::Claude, true)
    });
    let b = std::thread::spawn({
        let svc = svc.clone();
        move || svc.set_agent_hidden(AgentId::Grok, true)
    });
    a.join().unwrap().unwrap();
    b.join().unwrap().unwrap();
    let mut ids = svc.list_hidden_agents().unwrap();
    ids.sort();
    assert_eq!(ids, vec!["claude", "grok"]);
}

#[test]
fn empty_file_defaults() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("agent_visibility.json"), "  \n").unwrap();
    let svc = AgentVisibilityService::new(dir.path().to_path_buf());
    assert!(svc.list_hidden_agents().unwrap().is_empty());
}
