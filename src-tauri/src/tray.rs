//! System tray: show main window / quit.
//! Paired with single-instance focus and optional close-to-tray hide.

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::exit_coordinator::{
    exit_impact_action, CoordinatedShutdownAction, ExitImpactAction, ExitImpactChoice,
    ExitPreparation,
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

/// How a user-originated exit request was handled.  Window-close handling uses
/// this to keep the window alive while an asynchronous native prompt is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExitRequestDisposition {
    /// A bridge-impact prompt is now visible (or was already visible).
    ConfirmationPending,
    /// Shutdown was already delegated to [`ExitCoordinator`], or this request
    /// was ignored because it is already draining.
    CoordinatedShutdown,
    /// Application state was unexpectedly unavailable.
    Ignored,
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
    matches!((button, state), (MouseButton::Left, MouseButtonState::Up))
}

/// Bring the main window to the foreground (show / unminimize / focus).
pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Route a tray-triggered quit through the shared graceful shutdown path.
///
/// If local bridges are active, an asynchronous native dialog gives the user
/// three outcomes: hide to tray, stop bridges and exit, or cancel.  Tauri's
/// Rust-native dialog callback is boolean, so the latter two "continue"
/// outcomes are presented in a small second prompt; this preserves a real
/// cancel path rather than treating a closed dialog as hide-to-tray.
/// AppState is managed before the tray is created; if it is unexpectedly
/// absent, avoid an uncoordinated direct exit and leave a diagnostic instead.
pub(crate) fn request_app_exit<R: Runtime>(app: &AppHandle<R>) -> ExitRequestDisposition {
    request_app_shutdown(app, CoordinatedShutdownAction::Exit)
}

/// Route an update-triggered relaunch through exactly the same impact prompt
/// and bridge drain as a normal app exit.
pub(crate) fn request_app_restart<R: Runtime>(app: &AppHandle<R>) -> ExitRequestDisposition {
    request_app_shutdown(app, CoordinatedShutdownAction::Restart)
}

fn request_app_shutdown<R: Runtime>(
    app: &AppHandle<R>,
    action: CoordinatedShutdownAction,
) -> ExitRequestDisposition {
    let Some(state) = app.try_state::<AppState>() else {
        tracing::warn!(target: "gui", op = "exit", "tray quit ignored because application state is unavailable");
        return ExitRequestDisposition::Ignored;
    };

    if state.exit_coordinator().shutdown_in_progress() {
        return ExitRequestDisposition::CoordinatedShutdown;
    }

    let preparation = state.exit_coordinator().prepare_exit(&state.bridge_host());
    if crate::exit_coordinator::ExitCoordinator::requires_impact_confirmation(preparation) {
        if !state.begin_exit_confirmation() {
            return ExitRequestDisposition::ConfirmationPending;
        }
        show_bridge_impact_prompt(app.clone(), preparation, action);
        return ExitRequestDisposition::ConfirmationPending;
    }

    request_coordinated_shutdown(app, action);
    ExitRequestDisposition::CoordinatedShutdown
}

fn request_coordinated_shutdown<R: Runtime>(app: &AppHandle<R>, action: CoordinatedShutdownAction) {
    let Some(state) = app.try_state::<AppState>() else {
        tracing::warn!(target: "gui", op = "exit", "coordinated exit ignored because application state is unavailable");
        return;
    };
    state.request_exit();
    match action {
        CoordinatedShutdownAction::Exit => {
            let _ = state
                .exit_coordinator()
                .request_exit(app.clone(), state.bridge_host());
        }
        CoordinatedShutdownAction::Restart => {
            let _ = state
                .exit_coordinator()
                .request_restart(app.clone(), state.bridge_host());
        }
    }
}

fn show_bridge_impact_prompt<R: Runtime>(
    app: AppHandle<R>,
    preparation: ExitPreparation,
    action: CoordinatedShutdownAction,
) {
    let bridge_text = match preparation.active_bridge_count {
        Some(count) => format!("{count} 个本机桥正在运行。"),
        None => "本机桥状态暂时无法读取。".to_owned(),
    };
    let (action_label, impact_label) = match action {
        CoordinatedShutdownAction::Exit => ("停止服务并退出", "停止服务并退出会中断"),
        CoordinatedShutdownAction::Restart => ("停止服务并重启", "停止服务并重启会中断"),
    };
    let callback_app = app.clone();
    app.dialog()
        .message(format!(
            "{bridge_text}\n{impact_label}这些本地 Connections。也可以让它们继续在托盘中运行，或取消本次操作。"
        ))
        .title("本机桥正在运行")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            action_label.to_owned(),
            "继续运行…".to_owned(),
        ))
        .show(move |stop_and_exit| {
            if stop_and_exit {
                apply_exit_impact_choice(
                    callback_app,
                    ExitImpactChoice::StopBridgesAndExit,
                    action,
                );
            } else {
                show_continue_running_prompt(callback_app, action);
            }
        });
}

fn show_continue_running_prompt<R: Runtime>(app: AppHandle<R>, action: CoordinatedShutdownAction) {
    let message = match action {
        CoordinatedShutdownAction::Exit => {
            "选择“隐藏到托盘”会保留正在运行的本机桥和 Connections；也可以取消本次退出。"
        }
        CoordinatedShutdownAction::Restart => {
            "选择“隐藏到托盘”会保留正在运行的本机桥和 Connections，并暂不重启；也可以取消本次重启。"
        }
    };
    let callback_app = app.clone();
    app.dialog()
        .message(message)
        .title("继续运行本机桥？")
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "隐藏到托盘".to_owned(),
            "取消".to_owned(),
        ))
        .show(move |hide_to_tray| {
            apply_exit_impact_choice(
                callback_app,
                if hide_to_tray {
                    ExitImpactChoice::HideToTray
                } else {
                    ExitImpactChoice::Cancel
                },
                action,
            );
        });
}

fn apply_exit_impact_choice<R: Runtime>(
    app: AppHandle<R>,
    choice: ExitImpactChoice,
    action: CoordinatedShutdownAction,
) {
    let Some(state) = app.try_state::<AppState>() else {
        tracing::warn!(target: "gui", op = "exit", "bridge-impact choice ignored because application state is unavailable");
        return;
    };
    state.finish_exit_confirmation();
    match exit_impact_action(choice) {
        ExitImpactAction::HideToTray => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
        }
        ExitImpactAction::RequestCoordinatedExit => request_coordinated_shutdown(&app, action),
        ExitImpactAction::None => {}
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
                let _ = request_app_exit(app);
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
