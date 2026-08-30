//! Runtime window / tray / desktop-shortcut icon (product mark tinted to accent).
//!
//! The bundled installer `.ico` stays the default indigo. This retints the
//! running window (taskbar), the tray icon, and Windows shortcuts that already
//! point at this executable (Desktop / Start menu).

use std::path::PathBuf;

use tauri::{image::Image, AppHandle, Manager, State};

use crate::state::AppState;
use crate::tray;

mod ico;
mod shortcuts;
#[cfg(windows)]
mod desktop;

pub(crate) const SHELL_ICON_MIN: u32 = 16;
pub(crate) const SHELL_ICON_MAX: u32 = 256;

pub(crate) fn validate_shell_icon_rgba(rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
    if width != height {
        return Err(format!("shell icon must be square, got {width}x{height}"));
    }
    if !(SHELL_ICON_MIN..=SHELL_ICON_MAX).contains(&width) {
        return Err(format!(
            "shell icon size {width} out of range {SHELL_ICON_MIN}..={SHELL_ICON_MAX}"
        ));
    }
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(|| "shell icon size overflow".to_string())?;
    if rgba.len() != expected {
        return Err(format!(
            "shell icon rgba length {} != {expected}",
            rgba.len()
        ));
    }
    Ok(())
}

pub(crate) fn sanitize_accent_id(id: &str) -> Result<&str, String> {
    if !id.is_empty()
        && id.len() <= 16
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b == b'-' || b == b'_')
    {
        Ok(id)
    } else {
        Err("invalid accent id".into())
    }
}

fn shell_icon_file(state: &AppState, accent_id: &str) -> Result<PathBuf, String> {
    let data_dir = state.hub()?.settings().path_info().data_dir;
    Ok(PathBuf::from(data_dir)
        .join("cache")
        .join(format!("shell-icon-{accent_id}.ico")))
}

fn persist_desktop_icon(
    state: &AppState,
    accent_id: &str,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<(), String> {
    let id = sanitize_accent_id(accent_id)?;
    let path = shell_icon_file(state, id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let bytes = ico::encode_ico_rgba(rgba, width, height)?;
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    desktop::publish_shortcut_icon(&path)?;
    let _ = path;
    Ok(())
}

pub(crate) fn apply_shell_icon(
    app: &AppHandle,
    state: &AppState,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    accent_id: &str,
) -> Result<(), String> {
    validate_shell_icon_rgba(&rgba, width, height)?;
    if let Err(e) = persist_desktop_icon(state, accent_id, &rgba, width, height) {
        tracing::warn!(
            target: agenthub_core::logging::targets::GUI,
            op = "set_shell_icon",
            error = %e,
            "desktop shortcut icon update failed"
        );
    }
    let mut last_err: Option<String> = None;
    if let Some(window) = app.get_webview_window("main") {
        if let Err(e) = window.set_icon(Image::new_owned(rgba.clone(), width, height)) {
            last_err = Some(format!("window icon: {e}"));
        }
    }
    if let Some(tray_icon) = app.tray_by_id(tray::TRAY_ID) {
        if let Err(e) = tray_icon.set_icon(Some(Image::new_owned(rgba, width, height))) {
            last_err = Some(format!("tray icon: {e}"));
        }
    }
    match last_err {
        Some(msg) => Err(msg),
        None => Ok(()),
    }
}

/// Invoke: `set_shell_icon` — RGBA (row-major) for the running window, tray, and shortcuts.
#[tauri::command]
pub fn set_shell_icon(
    app: AppHandle,
    state: State<'_, AppState>,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    accent_id: String,
) -> Result<(), String> {
    apply_shell_icon(&app, &state, rgba, width, height, &accent_id).map_err(|msg| {
        tracing::warn!(
            target: agenthub_core::logging::targets::GUI,
            op = "set_shell_icon",
            error = %msg,
            "set shell icon failed"
        );
        msg
    })
}

#[cfg(test)]
mod tests;
