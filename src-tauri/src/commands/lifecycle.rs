//! Process lifecycle commands that must stay behind the desktop shutdown
//! coordinator instead of exposing raw process-plugin capabilities to pages.

use tauri::{AppHandle, State};

use crate::state::AppState;
use crate::tray::{request_app_restart, ExitRequestDisposition};

/// Request the post-update relaunch. If bridges are active the shared native
/// impact prompt is shown; the actual relaunch happens only after the user's
/// stop-and-restart choice drains every listener.
#[tauri::command]
pub async fn request_controlled_restart(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if state.exit_coordinator().shutdown_in_progress() {
        return Err("应用正在退出或重启中".into());
    }
    match request_app_restart(&app) {
        ExitRequestDisposition::Ignored => Err("无法协调应用重启".into()),
        ExitRequestDisposition::ConfirmationPending
        | ExitRequestDisposition::CoordinatedShutdown => Ok(()),
    }
}
