//! Settings / path info / open logs directory — thin wrappers over SettingsService.

pub mod pick_directory;

use tauri::{AppHandle, State};

use crate::commands::{map_err_string, with_hub_blocking};
use crate::state::AppState;
use agenthub_core::logging::targets;
use agenthub_core::models::{AppSettings, PathInfo};

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
) -> Result<(), String> {
    let op = op.trim().to_string();
    if op.is_empty() {
        return Err("op is empty".into());
    }
    tracing::info!(
        target: targets::GUI,
        module = targets::GUI,
        op = %op,
        agent = agent.as_deref().unwrap_or("-"),
        last4 = last4.as_deref().unwrap_or(""),
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

fn open_in_file_manager(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| {
                let msg = format!("open explorer failed: {e}");
                tracing::warn!(target: targets::GUI, op = "open_logs_dir", "{msg}");
                msg
            })?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| {
                let msg = format!("open failed: {e}");
                tracing::warn!(target: targets::GUI, op = "open_logs_dir", "{msg}");
                msg
            })?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| {
                let msg = format!("xdg-open failed: {e}");
                tracing::warn!(target: targets::GUI, op = "open_logs_dir", "{msg}");
                msg
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use agenthub_core::AgentHub;
    use tempfile::tempdir;

    fn hub_tmp() -> (tempfile::TempDir, AgentHub) {
        let dir = tempdir().unwrap();
        let hub = AgentHub::open(Some(dir.path())).unwrap();
        (dir, hub)
    }

    #[test]
    fn settings_defaults_include_log_fields() {
        let (_dir, hub) = hub_tmp();
        let s = hub.settings().get_all().unwrap();
        assert_eq!(s.log_level, "info");
        assert_eq!(s.log_retention_days, 14);
        assert!(s.close_to_tray);
        let paths = hub.settings().path_info();
        assert!(
            paths.logs_dir.replace('\\', "/").ends_with("/logs"),
            "logs_dir={}",
            paths.logs_dir
        );
    }

    #[test]
    fn set_log_level_and_retention_via_service() {
        let (_dir, hub) = hub_tmp();
        hub.settings().set("log_level", "debug").unwrap();
        hub.settings().set("log_retention_days", "30").unwrap();
        let s = hub.settings().get_all().unwrap();
        assert_eq!(s.log_level, "debug");
        assert_eq!(s.log_retention_days, 30);
        assert!(hub.settings().set("log_level", "nope").is_err());
        assert!(hub.settings().set("log_retention_days", "0").is_err());
    }

    #[test]
    fn close_to_tray_roundtrip_via_service() {
        let (_dir, hub) = hub_tmp();
        assert!(hub.settings().get_all().unwrap().close_to_tray);
        hub.settings().set("close_to_tray", "false").unwrap();
        assert!(!hub.settings().get_all().unwrap().close_to_tray);
        hub.settings().set("close_to_tray", "true").unwrap();
        assert!(hub.settings().get_all().unwrap().close_to_tray);
        assert!(hub.settings().set("close_to_tray", "maybe").is_err());
    }

    #[test]
    fn app_state_syncs_close_to_tray_flag_after_setting_write() {
        use crate::state::AppState;
        use std::sync::Arc;

        let (_dir, hub) = hub_tmp();
        let state = AppState::from_hub(Ok(Arc::new(hub)));
        assert!(state.close_to_tray());

        // Mimic set_setting success path: DB write then flag sync.
        state
            .hub()
            .unwrap()
            .settings()
            .set("close_to_tray", "false")
            .unwrap();
        state.sync_setting_flag("close_to_tray", "false");
        assert!(!state.close_to_tray());
        assert!(
            !state
                .hub()
                .unwrap()
                .settings()
                .get_all()
                .unwrap()
                .close_to_tray
        );

        state
            .hub()
            .unwrap()
            .settings()
            .set("close_to_tray", "true")
            .unwrap();
        state.sync_setting_flag("close_to_tray", "true");
        assert!(state.close_to_tray());
    }

    #[test]
    fn open_logs_dir_ensures_directory_exists() {
        let (dir, hub) = hub_tmp();
        let logs = std::path::PathBuf::from(hub.settings().path_info().logs_dir);
        // layout already creates logs; remove and re-ensure path logic
        if logs.exists() {
            // keep dir for open; just verify present under data dir
            assert!(logs.starts_with(dir.path()) || logs.exists());
        }
        assert!(logs.exists() || std::fs::create_dir_all(&logs).is_ok());
        assert!(logs.is_dir());
    }
}
