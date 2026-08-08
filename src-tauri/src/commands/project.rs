//! Project / session listing commands — thin wrappers over agenthub-core.

use agenthub_core::models::{
    AgentProject, AgentProjectExcerpt, AgentSession, ProjectMetadataFile,
};
use agenthub_core::AgentHub;
use tauri::State;

use crate::commands::{map_err_string, parse_agent_opt, with_hub_blocking};
use crate::state::AppState;

/// Invoke: `list_agent_projects` — project **containers**.
#[tauri::command]
pub async fn list_agent_projects(
    state: State<'_, AppState>,
    agent_id: Option<String>,
    include_hidden: Option<bool>,
) -> Result<Vec<AgentProject>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        list_agent_projects_inner(hub, agent_id.as_deref(), include_hidden.unwrap_or(false))
    })
    .await
}

/// Invoke: `list_agent_project_sessions` — sessions under one container.
#[tauri::command]
pub async fn list_agent_project_sessions(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<AgentSession>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        list_agent_project_sessions_inner(hub, &project_id)
    })
    .await
}

/// Invoke: `get_project_metadata`
#[tauri::command]
pub async fn get_project_metadata(
    state: State<'_, AppState>,
) -> Result<ProjectMetadataFile, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.projects
            .get_metadata()
            .map_err(|e| map_err_string("get_project_metadata", e))
    })
    .await
}

/// Invoke: `upsert_project_meta`
#[tauri::command]
pub async fn upsert_project_meta(
    state: State<'_, AppState>,
    project_id: String,
    hidden: Option<bool>,
    alias: Option<String>,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        let doc = hub
            .projects
            .get_metadata()
            .map_err(|e| map_err_string("upsert_project_meta", e))?;
        let mut meta = doc.projects.get(&project_id).cloned().unwrap_or_default();
        if let Some(h) = hidden {
            meta.hidden = h;
        }
        if let Some(a) = alias {
            let t = a.trim().to_string();
            meta.alias = if t.is_empty() { None } else { Some(t) };
        }
        hub.projects
            .upsert_project_meta(&project_id, meta)
            .map_err(|e| map_err_string("upsert_project_meta", e))
    })
    .await
}

/// Invoke: `set_show_hidden_projects`
#[tauri::command]
pub async fn set_show_hidden_projects(
    state: State<'_, AppState>,
    show: bool,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        hub.projects
            .set_show_hidden_projects(show)
            .map_err(|e| map_err_string("set_show_hidden_projects", e))
    })
    .await
}

/// Invoke: `delete_agent_project` — deletes a **session** by id.
#[tauri::command]
pub async fn delete_agent_project(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| delete_agent_project_inner(hub, &id)).await
}

/// Invoke: `delete_agent_projects`
#[tauri::command]
pub async fn delete_agent_projects(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<u32, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| delete_agent_projects_inner(hub, ids)).await
}

/// Invoke: `get_agent_project_excerpts`
#[tauri::command]
pub async fn get_agent_project_excerpts(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<Vec<AgentProjectExcerpt>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| get_excerpts_inner(hub, ids)).await
}

fn list_agent_projects_inner(
    hub: &AgentHub,
    agent_id: Option<&str>,
    include_hidden: bool,
) -> Result<Vec<AgentProject>, String> {
    let filter = parse_agent_opt(agent_id)?;
    hub.projects
        .list_projects(filter, include_hidden)
        .map_err(|e| map_err_string("list_agent_projects", e))
}

fn list_agent_project_sessions_inner(
    hub: &AgentHub,
    project_id: &str,
) -> Result<Vec<AgentSession>, String> {
    hub.projects
        .list_sessions(project_id)
        .map_err(|e| map_err_string("list_agent_project_sessions", e))
}

fn delete_agent_project_inner(hub: &AgentHub, id: &str) -> Result<(), String> {
    hub.projects
        .delete(id)
        .map_err(|e| map_err_string("delete_agent_project", e))
}

fn delete_agent_projects_inner(hub: &AgentHub, ids: Vec<String>) -> Result<u32, String> {
    hub.projects
        .delete_many(&ids)
        .map_err(|e| map_err_string("delete_agent_projects", e))
}

fn get_excerpts_inner(
    hub: &AgentHub,
    ids: Vec<String>,
) -> Result<Vec<AgentProjectExcerpt>, String> {
    hub.projects
        .excerpts(&ids)
        .map_err(|e| map_err_string("get_agent_project_excerpts", e))
}

#[cfg(test)]
mod tests {
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
        let err = delete_agent_project_inner(&hub, "claude:projects/no-such.jsonl").unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn excerpts_empty_ids_ok() {
        let (_dir, hub) = hub_tmp();
        let rows = get_excerpts_inner(&hub, vec![]).unwrap();
        assert!(rows.is_empty());
        let batch =
            delete_agent_projects_inner(&hub, vec!["claude:projects/x.jsonl".into()]).unwrap();
        assert_eq!(batch, 0);
    }

    #[test]
    fn metadata_upsert_roundtrip() {
        let (dir, hub) = hub_tmp();
        let pid = "claude:proj:-C-Users-demo";
        hub.projects
            .upsert_project_meta(
                pid,
                ProjectUserMeta {
                    hidden: true,
                    alias: Some("Alias".into()),
                },
            )
            .unwrap();
        let doc = hub.projects.get_metadata().unwrap();
        assert!(doc.projects.get(pid).unwrap().hidden);
        assert!(dir.path().join("project_metadata.json").exists());
    }

    #[test]
    fn list_include_hidden_and_show_flag() {
        let (_dir, hub) = hub_tmp();
        let pid = "claude:proj:-C-Users-demo";
        hub.projects
            .upsert_project_meta(
                pid,
                ProjectUserMeta {
                    hidden: true,
                    alias: Some("H".into()),
                },
            )
            .unwrap();
        hub.projects.set_show_hidden_projects(true).unwrap();
        assert!(hub.projects.get_metadata().unwrap().show_hidden_projects);
        // Machine may have no claude projects; just ensure API accepts include_hidden.
        let _ = list_agent_projects_inner(&hub, Some("claude"), true).unwrap();
        let _ = list_agent_projects_inner(&hub, Some("claude"), false).unwrap();
    }
}
