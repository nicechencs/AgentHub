use super::*;
use agenthub_core::models::ProjectUserMeta;
use tempfile::tempdir;

fn hub_tmp() -> (tempfile::TempDir, AgentHub) {
    let dir = tempdir().unwrap();
    let hub = AgentHub::open(Some(dir.path())).unwrap();
    (dir, hub)
}

#[test]
fn list_and_invalid_agent_filter() {
    let (_dir, hub) = hub_tmp();
    let items = list_agent_projects_inner(&hub, Some("claude"), false).unwrap();
    let _ = items;
    let err = list_agent_projects_inner(&hub, Some("not-an-agent"), false).unwrap_err();
    assert!(err.contains("invalid agent") || err.contains("claude|codex"));
}

#[test]
fn list_sessions_bad_project_id() {
    let (_dir, hub) = hub_tmp();
    let err = list_agent_project_sessions_inner(&hub, "not-a-project").unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn delete_missing_maps_error() {
    let (_dir, hub) = hub_tmp();
    let err = delete_agent_session_inner(&hub, "claude:projects/no-such.jsonl").unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn excerpts_empty_ids_ok() {
    let (_dir, hub) = hub_tmp();
    let rows = get_excerpts_inner(&hub, vec![]).unwrap();
    assert!(rows.is_empty());
    let batch = delete_agent_sessions_inner(&hub, vec!["claude:projects/x.jsonl".into()]).unwrap();
    assert_eq!(batch, 0);
}

#[test]
fn metadata_upsert_roundtrip() {
    let (dir, hub) = hub_tmp();
    let pid = "claude:proj:-C-Users-demo";
    hub.projects()
        .upsert_project_meta(
            pid,
            ProjectUserMeta {
                hidden: true,
                alias: Some("Alias".into()),
            },
        )
        .unwrap();
    let doc = hub.projects().get_metadata().unwrap();
    assert!(doc.projects.get(pid).unwrap().hidden);
    assert!(dir.path().join("project_metadata.json").exists());
}

#[test]
fn list_include_hidden_and_show_flag() {
    let (_dir, hub) = hub_tmp();
    let pid = "claude:proj:-C-Users-demo";
    hub.projects()
        .upsert_project_meta(
            pid,
            ProjectUserMeta {
                hidden: true,
                alias: Some("H".into()),
            },
        )
        .unwrap();
    hub.projects().set_show_hidden_projects(true).unwrap();
    assert!(hub.projects().get_metadata().unwrap().show_hidden_projects);
    // Machine may have no claude projects; just ensure API accepts include_hidden.
    let _ = list_agent_projects_inner(&hub, Some("claude"), true).unwrap();
    let _ = list_agent_projects_inner(&hub, Some("claude"), false).unwrap();
}
