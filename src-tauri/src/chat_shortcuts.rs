//! Window-menu accelerators for Chat.
//!
//! WebKitGTK / WebView2 swallow Ctrl/Cmd+N (browser "new window") before JS
//! sees a keydown. A hidden window menu claims the chord at the OS layer and
//! emits [`CHAT_SHORTCUT_EVENT`] for the frontend.
//!
//! Replacing the default menu without Edit items also drops Select All / Cut /
//! Copy / Paste. Keep a predefined Edit submenu so Ctrl/Cmd+A/C/X/V still work
//! in the composer.

use serde::Serialize;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Emitter, Manager, Runtime,
};

use crate::state::AppState;
use crate::tray_i18n::{language_from_hub, parse_tray_language, TrayUiLanguage};

pub(crate) const MENU_NEW_CHAT: &str = "chat-new";
pub(crate) const CHAT_SHORTCUT_EVENT: &str = "chat-shortcut";
pub(crate) const ACTION_NEW_CHAT: &str = "newChat";
pub(crate) const NEW_CHAT_ACCELERATOR: &str = "CmdOrCtrl+N";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatShortcutPayload {
    pub action: &'static str,
}

pub(crate) fn new_chat_menu_label(lang: TrayUiLanguage) -> &'static str {
    match lang {
        TrayUiLanguage::Zh => "新建对话",
        TrayUiLanguage::En => "New chat",
    }
}

pub(crate) fn chat_menu_submenu_label(lang: TrayUiLanguage) -> &'static str {
    match lang {
        TrayUiLanguage::Zh => "对话",
        TrayUiLanguage::En => "Chat",
    }
}

pub(crate) struct EditMenuCopy {
    pub submenu: &'static str,
    pub cut: &'static str,
    pub copy: &'static str,
    pub paste: &'static str,
    pub select_all: &'static str,
}

pub(crate) fn edit_menu_copy(lang: TrayUiLanguage) -> EditMenuCopy {
    match lang {
        TrayUiLanguage::Zh => EditMenuCopy {
            submenu: "编辑",
            cut: "剪切",
            copy: "复制",
            paste: "粘贴",
            select_all: "全选",
        },
        TrayUiLanguage::En => EditMenuCopy {
            submenu: "Edit",
            cut: "Cut",
            copy: "Copy",
            paste: "Paste",
            select_all: "Select All",
        },
    }
}

pub(crate) fn chat_shortcut_menu_action(id: &str) -> Option<&'static str> {
    match id {
        MENU_NEW_CHAT => Some(ACTION_NEW_CHAT),
        _ => None,
    }
}

pub(crate) fn emit_if_new_chat<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let Some(action) = chat_shortcut_menu_action(id) else {
        return;
    };
    let _ = app.emit(CHAT_SHORTCUT_EVENT, ChatShortcutPayload { action });
}

fn build_new_chat_menu<R: Runtime>(
    app: &AppHandle<R>,
    lang: TrayUiLanguage,
) -> tauri::Result<Menu<R>> {
    let edit = edit_menu_copy(lang);
    let cut = PredefinedMenuItem::cut(app, Some(edit.cut))?;
    let copy = PredefinedMenuItem::copy(app, Some(edit.copy))?;
    let paste = PredefinedMenuItem::paste(app, Some(edit.paste))?;
    let select_all = PredefinedMenuItem::select_all(app, Some(edit.select_all))?;
    let edit_menu = Submenu::with_items(
        app,
        edit.submenu,
        true,
        &[&cut, &copy, &paste, &select_all],
    )?;
    let item = MenuItem::with_id(
        app,
        MENU_NEW_CHAT,
        new_chat_menu_label(lang),
        true,
        Some(NEW_CHAT_ACCELERATOR),
    )?;
    let chat_menu = Submenu::with_items(app, chat_menu_submenu_label(lang), true, &[&item])?;
    Menu::with_items(app, &[&edit_menu, &chat_menu])
}

fn hide_main_menu_bar<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.hide_menu() {
            tracing::warn!(
                target: "gui",
                op = "chat_shortcuts",
                error = %error,
                "hide window menu bar failed"
            );
        }
    }
}

/// Attach CmdOrCtrl+N, then hide the bar on Linux/Windows so chrome stays the same.
pub(crate) fn setup_new_chat_accel<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let state = app.try_state::<AppState>();
    let lang = language_from_hub(state.as_ref().and_then(|s| s.hub().ok()));
    let menu = build_new_chat_menu(app, lang)?;
    app.set_menu(menu)?;
    hide_main_menu_bar(app);
    Ok(())
}

pub(crate) fn rebuild_new_chat_accel<R: Runtime>(app: &AppHandle<R>, language: &str) {
    let lang = parse_tray_language(language);
    match build_new_chat_menu(app, lang) {
        Ok(menu) => {
            if let Err(error) = app.set_menu(menu) {
                tracing::warn!(
                    target: "gui",
                    op = "chat_shortcuts",
                    error = %error,
                    "rebuild new-chat accelerator failed"
                );
                return;
            }
            hide_main_menu_bar(app);
        }
        Err(error) => {
            tracing::warn!(
                target: "gui",
                op = "chat_shortcuts",
                error = %error,
                "rebuild new-chat accelerator failed"
            );
        }
    }
}

#[cfg(test)]
mod tests;
