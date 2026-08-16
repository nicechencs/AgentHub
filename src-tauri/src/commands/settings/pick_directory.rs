//! Native folder picker for Chat working directory (and other path fields).

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, FilePath};

use agenthub_core::logging::targets;

pub(crate) const DEFAULT_PICK_DIRECTORY_TITLE: &str = "选择工作目录";

/// Invoke: `pick_directory` — system folder picker.
///
/// `Ok(None)` means the user cancelled. A selected path is the OS display form
/// (lossy UTF-8), not a URI.
#[tauri::command]
pub async fn pick_directory(
    app: AppHandle,
    title: Option<String>,
    default_path: Option<String>,
) -> Result<Option<String>, String> {
    pick_directory_with(app, title.as_deref(), default_path.as_deref())
}

pub(crate) fn pick_directory_with(
    app: AppHandle,
    title: Option<&str>,
    default_path: Option<&str>,
) -> Result<Option<String>, String> {
    let mut dialog = app.dialog().file();
    dialog = dialog.set_title(title.unwrap_or(DEFAULT_PICK_DIRECTORY_TITLE));
    dialog = dialog.set_can_create_directories(true);
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    if let Some(start) = starting_directory(default_path) {
        dialog = dialog.set_directory(start);
    }

    match dialog.blocking_pick_folder() {
        None => Ok(None),
        Some(path) => match file_path_to_display_string(path) {
            Ok(s) => Ok(Some(s)),
            Err(e) => {
                tracing::warn!(
                    target: targets::GUI,
                    op = "pick_directory",
                    error = %e,
                    "folder picker path conversion failed"
                );
                Err(e)
            }
        },
    }
}

/// Prefer an existing directory; if the default is a missing leaf, use its parent.
pub(crate) fn starting_directory(default_path: Option<&str>) -> Option<PathBuf> {
    let raw = default_path.map(str::trim).filter(|s| !s.is_empty())?;
    let path = PathBuf::from(raw);
    if path.is_dir() {
        return Some(path);
    }
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty())?;
    Some(parent.to_path_buf())
}

pub(crate) fn file_path_to_display_string(path: FilePath) -> Result<String, String> {
    let buf = path
        .simplified()
        .into_path()
        .map_err(|e| format!("invalid folder path: {e}"))?;
    if buf.as_os_str().is_empty() {
        return Err("empty folder path".into());
    }
    Ok(path_to_display_string(&buf))
}

pub(crate) fn path_to_display_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests;
