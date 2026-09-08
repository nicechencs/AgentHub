//! Native file picker (skill zip and other path fields).

use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use agenthub_core::logging::targets;

use crate::state::AppState;
use crate::tray_i18n::{language_from_hub, TrayUiLanguage};

use super::pick_directory::{file_path_to_display_string, starting_directory};

pub(crate) const DEFAULT_PICK_FILE_TITLE: &str = "选择文件";
const DEFAULT_PICK_FILE_TITLE_EN: &str = "Select file";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickFileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

pub(crate) fn pick_file_default_title(lang: TrayUiLanguage) -> &'static str {
    match lang {
        TrayUiLanguage::En => DEFAULT_PICK_FILE_TITLE_EN,
        TrayUiLanguage::Zh => DEFAULT_PICK_FILE_TITLE,
    }
}

/// Invoke: `pick_file` — system file picker. `Ok(None)` means cancelled.
#[tauri::command]
pub async fn pick_file(
    app: AppHandle,
    title: Option<String>,
    default_path: Option<String>,
    filters: Option<Vec<PickFileFilter>>,
) -> Result<Option<String>, String> {
    pick_file_with(
        app,
        title.as_deref(),
        default_path.as_deref(),
        filters.as_deref(),
    )
}

pub(crate) fn pick_file_with(
    app: AppHandle,
    title: Option<&str>,
    default_path: Option<&str>,
    filters: Option<&[PickFileFilter]>,
) -> Result<Option<String>, String> {
    let mut dialog = app.dialog().file();
    let fallback_title = title.unwrap_or_else(|| {
        let lang = match app.try_state::<AppState>() {
            Some(state) => language_from_hub(state.hub().ok()),
            None => TrayUiLanguage::Zh,
        };
        pick_file_default_title(lang)
    });
    dialog = dialog.set_title(fallback_title);
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    if let Some(start) = starting_directory(default_path) {
        dialog = dialog.set_directory(start);
    }
    if let Some(filters) = filters {
        for filter in filters {
            let name = filter.name.trim();
            if name.is_empty() || filter.extensions.is_empty() {
                continue;
            }
            let extensions: Vec<&str> = filter
                .extensions
                .iter()
                .map(|ext| ext.trim())
                .filter(|ext| !ext.is_empty())
                .collect();
            if extensions.is_empty() {
                continue;
            }
            dialog = dialog.add_filter(name, &extensions);
        }
    }

    match dialog.blocking_pick_file() {
        None => Ok(None),
        Some(path) => match file_path_to_display_string(path) {
            Ok(s) => Ok(Some(s)),
            Err(e) => {
                tracing::warn!(
                    target: targets::GUI,
                    op = "pick_file",
                    error = %e,
                    "file picker path conversion failed"
                );
                Err(e)
            }
        },
    }
}

#[cfg(test)]
mod tests;
