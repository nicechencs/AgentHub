//! Settings / path info / open logs directory — thin wrappers over SettingsService.

pub mod pick_directory;
pub mod pick_file;

use tauri::{AppHandle, State};

use crate::commands::{map_err_string, with_hub_blocking};
use crate::file_manager::open_in_file_manager;
use crate::state::AppState;
use agenthub_core::logging::targets;
use agenthub_core::models::{AppSettings, PathInfo};
use agenthub_core::utils::redact::sanitize_gui_last4;

/// Invoke: `get_app_settings` — L1 settings (theme / language / log_*).
#[tauri::command]
pub async fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, |hub| {
        hub.settings()
            .get_all()
            .map_err(|e| map_err_string("get_app_settings", e))
    })
    .await
}

/// Invoke: `get_path_info` — data_dir / db / backups / logs paths.
#[tauri::command]
pub async fn get_path_info(state: State<'_, AppState>) -> Result<PathInfo, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, |hub| Ok(hub.settings().path_info())).await
}

/// Invoke: `set_setting` — whitelist key (theme|language|log_level|log_retention_days|close_to_tray|…).
#[tauri::command]
pub async fn set_setting(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    let hub = state.hub_arc()?;
    let key_for_sync = key.clone();
    let value_for_sync = value.clone();
    with_hub_blocking(hub, move |hub| {
        hub.settings()
            .set(&key, &value)
            .map_err(|e| map_err_string("set_setting", e))
    })
    .await?;

    // Keep in-process close-to-tray flag in sync so the next window close uses it.
    state.sync_setting_flag(&key_for_sync, &value_for_sync);
    if key_for_sync == "language" {
        crate::tray::rebuild_tray_menu(&app, &value_for_sync);
        crate::shell_open_chat::register_best_effort(crate::tray_i18n::parse_tray_language(
            &value_for_sync,
        ));
    }
    Ok(())
}

/// Invoke: `open_external_url` — open http(s) URL in the system default browser.
///
/// Tauri webview does not honor `window.open` for external sites; GUI must use this.
#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("url is empty".into());
    }
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        let msg = format!("only http(s) URLs are allowed: {url}");
        tracing::warn!(target: targets::GUI, op = "open_external_url", "{msg}");
        return Err(msg);
    }
    agenthub_core::oauth::open_in_browser(url).map_err(|e| {
        let msg = e.to_string();
        tracing::warn!(target: targets::GUI, op = "open_external_url", error = %msg, "open browser failed");
        msg
    })
}

/// Invoke: `log_gui_event` — append a GUI op line (no raw secrets).
#[tauri::command]
pub async fn log_gui_event(
    op: String,
    agent: Option<String>,
    last4: Option<String>,
    profile_id: Option<String>,
    route: Option<String>,
    code: Option<String>,
) -> Result<(), String> {
    let op = op.trim().to_string();
    if op.is_empty() {
        return Err("op is empty".into());
    }
    let last4 = sanitize_gui_last4(last4.as_deref());
    tracing::info!(
        target: targets::GUI,
        module = targets::GUI,
        op = %op,
        agent = agent.as_deref().unwrap_or("-"),
        last4 = last4.as_str(),
        profile_id = profile_id.as_deref().unwrap_or("-"),
        route = route.as_deref().unwrap_or("-"),
        code = code.as_deref().unwrap_or(""),
        "gui event"
    );
    Ok(())
}

/// Invoke: `open_logs_dir` — ensure logs dir exists and open in file manager.
#[tauri::command]
pub async fn open_logs_dir(state: State<'_, AppState>) -> Result<String, String> {
    let hub = state.hub_arc()?;
    with_hub_blocking(hub, |hub| {
        let info = hub.settings().path_info();
        let path = std::path::PathBuf::from(&info.logs_dir);
        if !path.exists() {
            std::fs::create_dir_all(&path).map_err(|e| {
                let msg = e.to_string();
                tracing::warn!(
                    target: targets::GUI,
                    op = "open_logs_dir",
                    error = %msg,
                    "create logs dir failed"
                );
                msg
            })?;
        }
        open_in_file_manager(&path)?;
        Ok(path.display().to_string())
    })
    .await
}

#[cfg(test)]
mod tests;
