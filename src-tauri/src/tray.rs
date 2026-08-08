//! System tray: show main window / quit.
//! Paired with single-instance focus and optional close-to-tray hide.

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};

use crate::state::AppState;

const TRAY_ID: &str = "main";
pub(crate) const MENU_SHOW: &str = "tray-show";
pub(crate) const MENU_QUIT: &str = "tray-quit";

/// Pure menu-id → action mapping (unit-tested).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrayMenuAction {
    Show,
    Quit,
    Ignore,
}

pub(crate) fn tray_menu_action(id: &str) -> TrayMenuAction {
    match id {
        MENU_SHOW => TrayMenuAction::Show,
        MENU_QUIT => TrayMenuAction::Quit,
        _ => TrayMenuAction::Ignore,
    }
}

/// Whether a tray click should surface the main window.
pub(crate) fn tray_click_should_show(button: MouseButton, state: MouseButtonState) -> bool {
    matches!(
        (button, state),
        (MouseButton::Left, MouseButtonState::Up)
    )
}

/// Bring the main window to the foreground (show / unminimize / focus).
pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Create the tray icon with context menu. Call once from app setup.
pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show_i = MenuItem::with_id(app, MENU_SHOW, "打开 AgentHub", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, MENU_QUIT, "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("AgentHub")
        // Left click focuses the window; right click opens the menu.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match tray_menu_action(event.id.as_ref()) {
            TrayMenuAction::Show => show_main_window(app),
            TrayMenuAction::Quit => {
                if let Some(state) = app.try_state::<AppState>() {
                    state.request_exit();
                }
                app.exit(0);
            }
            TrayMenuAction::Ignore => {}
        })
        .on_tray_icon_event(|tray, event| {
            let show = match event {
                TrayIconEvent::Click {
                    button,
                    button_state,
                    ..
                } => tray_click_should_show(button, button_state),
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => true,
                _ => false,
            };
            if show {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    let _tray = builder.build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_menu_ids_map_to_actions() {
        assert_eq!(tray_menu_action(MENU_SHOW), TrayMenuAction::Show);
        assert_eq!(tray_menu_action(MENU_QUIT), TrayMenuAction::Quit);
        assert_eq!(tray_menu_action("unknown"), TrayMenuAction::Ignore);
        assert_eq!(tray_menu_action(""), TrayMenuAction::Ignore);
    }

    #[test]
    fn left_click_up_shows_window() {
        assert!(tray_click_should_show(
            MouseButton::Left,
            MouseButtonState::Up
        ));
        assert!(!tray_click_should_show(
            MouseButton::Left,
            MouseButtonState::Down
        ));
        assert!(!tray_click_should_show(
            MouseButton::Right,
            MouseButtonState::Up
        ));
    }
}
