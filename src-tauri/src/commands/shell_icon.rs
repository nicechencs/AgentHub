//! Runtime window / tray icon (product mark tinted to the current accent).
//!
//! The bundled `.ico` / installer shortcut stays the default indigo. This only
//! retints the running window (taskbar button) and the tray icon.

use tauri::{image::Image, AppHandle, Manager};

use crate::tray;

pub(crate) const SHELL_ICON_MIN: u32 = 16;
pub(crate) const SHELL_ICON_MAX: u32 = 256;

pub(crate) fn validate_shell_icon_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<(), String> {
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

pub(crate) fn apply_shell_icon(
    app: &AppHandle,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
) -> Result<(), String> {
    validate_shell_icon_rgba(&rgba, width, height)?;
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

/// Invoke: `set_shell_icon` — RGBA (row-major) for the running window + tray.
#[tauri::command]
pub fn set_shell_icon(app: AppHandle, rgba: Vec<u8>, width: u32, height: u32) -> Result<(), String> {
    apply_shell_icon(&app, rgba, width, height).map_err(|msg| {
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
