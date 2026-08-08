//! Backup Tauri commands — thin wrappers over agenthub-core.
//! Snapshots copy raw file bytes; no encrypt/decrypt or format conversion.

use agenthub_core::models::{BackupKind, BackupRecord};
use agenthub_core::services::RestoreResult;
use agenthub_core::AgentHub;
use tauri::State;

use crate::commands::{map_err_string, parse_agent, parse_agent_opt, with_hub_blocking};
use crate::state::AppState;

/// Invoke: `list_backups`
#[tauri::command]
pub async fn list_backups(
    state: State<'_, AppState>,
    agent_id: Option<String>,
) -> Result<Vec<BackupRecord>, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| list_backups_inner(hub, agent_id.as_deref())).await
}

/// Invoke: `create_backup` — manual snapshot of agent live files.
#[tauri::command]
pub async fn create_backup(
    state: State<'_, AppState>,
    agent_id: String,
    note: Option<String>,
) -> Result<BackupRecord, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| {
        create_backup_inner(hub, &agent_id, note.as_deref())
    })
    .await
}

/// Invoke: `restore_backup` — pre-restore snapshot then apply.
#[tauri::command]
pub async fn restore_backup(
    state: State<'_, AppState>,
    backup_id: String,
) -> Result<RestoreResult, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| restore_backup_inner(hub, &backup_id)).await
}

/// Invoke: `delete_backup`
#[tauri::command]
pub async fn delete_backup(state: State<'_, AppState>, backup_id: String) -> Result<(), String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, move |hub| delete_backup_inner(hub, &backup_id)).await
}

fn list_backups_inner(hub: &AgentHub, agent_id: Option<&str>) -> Result<Vec<BackupRecord>, String> {
    let filter = parse_agent_opt(agent_id)?;
    hub.backups
        .list(filter)
        .map_err(|e| map_err_string("list_backups", e))
}

fn create_backup_inner(
    hub: &AgentHub,
    agent_id: &str,
    note: Option<&str>,
) -> Result<BackupRecord, String> {
    let agent = parse_agent(agent_id)?;
    hub.backups
        .snapshot(agent, BackupKind::Manual, note)
        .map_err(|e| map_err_string("create_backup", e))
}

fn restore_backup_inner(hub: &AgentHub, backup_id: &str) -> Result<RestoreResult, String> {
    hub.backups
        .restore(backup_id)
        .map_err(|e| map_err_string("restore_backup", e))
}

fn delete_backup_inner(hub: &AgentHub, backup_id: &str) -> Result<(), String> {
    hub.backups
        .delete(backup_id)
        .map_err(|e| map_err_string("delete_backup", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn hub_tmp() -> (tempfile::TempDir, AgentHub) {
        let dir = tempdir().unwrap();
        let hub = AgentHub::open(Some(dir.path())).unwrap();
        (dir, hub)
    }

    #[test]
    fn list_empty_and_invalid_agent() {
        let (_dir, hub) = hub_tmp();
        let items = list_backups_inner(&hub, None).unwrap();
        assert!(items.is_empty());
        let err = create_backup_inner(&hub, "bad", None).unwrap_err();
        assert!(err.contains("invalid agent"));
    }

    #[test]
    fn create_then_list_refresh_and_delete() {
        let (_dir, hub) = hub_tmp();
        match create_backup_inner(&hub, "claude", Some("gui-test-manual")) {
            Ok(record) => {
                let listed = list_backups_inner(&hub, Some("claude")).unwrap();
                assert!(
                    listed.iter().any(|b| b.id == record.id),
                    "list after create must include new backup"
                );
                delete_backup_inner(&hub, &record.id).unwrap();
                let after = list_backups_inner(&hub, Some("claude")).unwrap();
                assert!(!after.iter().any(|b| b.id == record.id));
            }
            Err(err) => {
                let lower = err.to_lowercase();
                assert!(
                    lower.contains("not") || lower.contains("no backupable"),
                    "unexpected create error: {err}"
                );
            }
        }

        let err = delete_backup_inner(&hub, "00000000-0000-0000-0000-000000000000").unwrap_err();
        assert!(
            err.to_lowercase().contains("not found") || err.contains("backup"),
            "unexpected delete err: {err}"
        );
        let _ = fs::metadata(_dir.path());
    }

    #[test]
    fn delete_missing_maps_error() {
        let (_dir, hub) = hub_tmp();
        let err = delete_backup_inner(&hub, "does-not-exist").unwrap_err();
        assert!(
            err.to_lowercase().contains("not found") || err.contains("backup"),
            "unexpected err: {err}"
        );
    }

    #[test]
    fn restore_missing_maps_error() {
        let (_dir, hub) = hub_tmp();
        let err = restore_backup_inner(&hub, "missing-id").unwrap_err();
        assert!(
            err.to_lowercase().contains("not found") || err.contains("backup"),
            "unexpected err: {err}"
        );
    }

    #[test]
    fn list_filter_invalid_agent() {
        let (_dir, hub) = hub_tmp();
        let err = list_backups_inner(&hub, Some("xyz")).unwrap_err();
        assert!(err.contains("invalid agent"));
    }
}
