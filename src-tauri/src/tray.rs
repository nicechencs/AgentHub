//! System tray: show / open routes / start-stop local bridges / quit.
//! Paired with single-instance focus and optional close-to-tray hide.

use agenthub_core::adapter_control::AdapterControl;
use agenthub_core::models::{AdapterProfileFilter, AdapterRoute};
use serde::Serialize;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::commands::{map_err_string, with_hub_blocking};
use crate::exit_coordinator::{
    exit_impact_action, CoordinatedShutdownAction, ExitImpactAction, ExitImpactChoice,
    ExitPreparation,
};
use crate::state::AppState;
use crate::tray_i18n::{
    parse_tray_language, tray_dialog_copy, tray_menu_copy, TrayMenuCopy, TrayUiLanguage,
};

const TRAY_ID: &str = "main";
pub(crate) const MENU_SHOW: &str = "tray-show";
pub(crate) const MENU_OPEN_ROUTES: &str = "tray-open-routes";
pub(crate) const MENU_START_ROUTES: &str = "tray-start-routes";
pub(crate) const MENU_STOP_ROUTES: &str = "tray-stop-routes";
pub(crate) const MENU_QUIT: &str = "tray-quit";

const TRAY_NAVIGATE_EVENT: &str = "tray-navigate";
const TRAY_NAVIGATE_PATH: &str = "/routes";

/// Pure menu-id → action mapping (unit-tested).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrayMenuAction {
    Show,
    OpenRoutes,
    StartRoutes,
    StopRoutes,
    Quit,
    Ignore,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrayNavigatePayload {
    path: &'static str,
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
        MENU_OPEN_ROUTES => TrayMenuAction::OpenRoutes,
        MENU_START_ROUTES => TrayMenuAction::StartRoutes,
        MENU_STOP_ROUTES => TrayMenuAction::StopRoutes,
        MENU_QUIT => TrayMenuAction::Quit,
        _ => TrayMenuAction::Ignore,
    }
}

/// `(profile_id, running)` → ids to start (not running) or stop (running).
pub(crate) fn tray_bridge_batch_ids<'a>(
    profiles: impl IntoIterator<Item = (&'a str, bool)>,
    start: bool,
) -> Vec<String> {
    profiles
        .into_iter()
        .filter(|(_, running)| if start { !*running } else { *running })
        .map(|(id, _)| id.to_owned())
        .collect()
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
    let copy = tray_dialog_copy(current_tray_language(&app));
    let bridge_text = match preparation.active_bridge_count {
        Some(count) => copy.running_count_line(count),
        None => copy.status_unavailable.to_owned(),
    };
    let (action_label, impact_label) = match action {
        CoordinatedShutdownAction::Exit => (copy.stop_and_quit, copy.impact_quit),
        CoordinatedShutdownAction::Restart => (copy.stop_and_restart, copy.impact_restart),
    };
    let callback_app = app.clone();
    app.dialog()
        .message(format!("{bridge_text}\n{impact_label}"))
        .title(copy.running_title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            action_label.to_owned(),
            copy.keep_running_ellipsis.to_owned(),
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
    let copy = tray_dialog_copy(current_tray_language(&app));
    let message = match action {
        CoordinatedShutdownAction::Exit => copy.hide_body_exit,
        CoordinatedShutdownAction::Restart => copy.hide_body_restart,
    };
    let callback_app = app.clone();
    app.dialog()
        .message(message)
        .title(copy.continue_title)
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(
            copy.hide_to_tray.to_owned(),
            copy.cancel.to_owned(),
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

fn emit_tray_navigate<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = app.emit(
        TRAY_NAVIGATE_EVENT,
        &TrayNavigatePayload {
            path: TRAY_NAVIGATE_PATH,
        },
    ) {
        tracing::warn!(
            target: "gui",
            op = "tray_navigate",
            error = %error,
            "emit tray-navigate failed"
        );
    }
}

fn spawn_tray_bridge_batch<R: Runtime>(app: AppHandle<R>, start: bool) {
    tauri::async_runtime::spawn(async move {
        // Extract owned handles before any `.await` — Tauri `State` is not Send.
        let extracted = app.try_state::<AppState>().and_then(|state| {
            Some((
                state.hub_arc().ok()?,
                state.bridge_host(),
                state.adapter_control().ok()?,
            ))
        });
        let Some((hub, host, control)) = extracted else {
            tracing::warn!(
                target: "gui",
                op = "tray_routes",
                "tray start/stop ignored because application state is unavailable"
            );
            return;
        };

        let profiles = match with_hub_blocking(hub, move |hub| {
            hub.adapter_apply
                .list_filtered(&AdapterProfileFilter {
                    route: Some(AdapterRoute::LocalBridge),
                    ..AdapterProfileFilter::default()
                })
                .map_err(|err| map_err_string("tray_list_local_bridges", err))
        })
        .await
        {
            Ok(profiles) => profiles,
            Err(error) => {
                tracing::warn!(
                    target: "gui",
                    op = "tray_routes",
                    error = %error,
                    "tray list local bridges failed"
                );
                return;
            }
        };

        if profiles.is_empty() {
            tracing::info!(
                target: "gui",
                op = "tray_routes",
                start,
                "no local bridge profiles; tray start/stop is a no-op"
            );
            return;
        }

        let ids = tray_bridge_batch_ids(
            profiles.iter().map(|profile| {
                let running = host
                    .status(&profile.id)
                    .ok()
                    .flatten()
                    .is_some_and(|status| status.running);
                (profile.id.as_str(), running)
            }),
            start,
        );

        for id in ids {
            let result = if start {
                control.start_bridge(id.clone()).await
            } else {
                control.stop_bridge(id.clone()).await
            };
            if let Err(error) = result {
                tracing::warn!(
                    target: "gui",
                    op = "tray_routes",
                    profile_id = %id,
                    start,
                    error = %error,
                    "tray bridge batch item failed"
                );
            }
        }
    });
}

fn current_tray_language<R: Runtime>(app: &AppHandle<R>) -> TrayUiLanguage {
    let raw = app
        .try_state::<AppState>()
        .and_then(|state| {
            state
                .hub()
                .ok()
                .and_then(|hub| hub.settings.get("language").ok().flatten())
        })
        .unwrap_or_default();
    parse_tray_language(&raw)
}

fn build_tray_menu<R: Runtime>(
    app: &AppHandle<R>,
    copy: &TrayMenuCopy,
) -> tauri::Result<Menu<R>> {
    let show_i = MenuItem::with_id(app, MENU_SHOW, copy.show, true, None::<&str>)?;
    let open_routes_i =
        MenuItem::with_id(app, MENU_OPEN_ROUTES, copy.open_routes, true, None::<&str>)?;
    let start_routes_i =
        MenuItem::with_id(app, MENU_START_ROUTES, copy.start_routes, true, None::<&str>)?;
    let stop_routes_i =
        MenuItem::with_id(app, MENU_STOP_ROUTES, copy.stop_routes, true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, MENU_QUIT, copy.quit, true, None::<&str>)?;
    Menu::with_items(
        app,
        &[
            &show_i,
            &open_routes_i,
            &start_routes_i,
            &stop_routes_i,
            &quit_i,
        ],
    )
}

/// Rebuild tray labels after a successful `language` settings write.
pub(crate) fn rebuild_tray_menu<R: Runtime>(app: &AppHandle<R>, language: &str) {
    let copy = tray_menu_copy(parse_tray_language(language));
    let menu = match build_tray_menu(app, &copy) {
        Ok(menu) => menu,
        Err(error) => {
            tracing::warn!(
                target: "gui",
                op = "tray_i18n",
                error = %error,
                "rebuild tray menu failed"
            );
            return;
        }
    };
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        tracing::warn!(
            target: "gui",
            op = "tray_i18n",
            "rebuild tray menu skipped because tray icon is unavailable"
        );
        return;
    };
    if let Err(error) = tray.set_menu(Some(menu)) {
        tracing::warn!(
            target: "gui",
            op = "tray_i18n",
            error = %error,
            "set tray menu failed"
        );
    }
}

/// Create the tray icon with context menu. Call once from app setup.
pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let copy = tray_menu_copy(current_tray_language(app));
    let menu = build_tray_menu(app, &copy)?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("AgentHub")
        // Left click focuses the window; right click opens the menu.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match tray_menu_action(event.id.as_ref()) {
            TrayMenuAction::Show => show_main_window(app),
            TrayMenuAction::OpenRoutes => {
                show_main_window(app);
                emit_tray_navigate(app);
            }
            TrayMenuAction::StartRoutes => spawn_tray_bridge_batch(app.clone(), true),
            TrayMenuAction::StopRoutes => spawn_tray_bridge_batch(app.clone(), false),
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
        assert_eq!(
            tray_menu_action(MENU_OPEN_ROUTES),
            TrayMenuAction::OpenRoutes
        );
        assert_eq!(
            tray_menu_action(MENU_START_ROUTES),
            TrayMenuAction::StartRoutes
        );
        assert_eq!(
            tray_menu_action(MENU_STOP_ROUTES),
            TrayMenuAction::StopRoutes
        );
        assert_eq!(tray_menu_action(MENU_QUIT), TrayMenuAction::Quit);
        assert_eq!(tray_menu_action("unknown"), TrayMenuAction::Ignore);
        assert_eq!(tray_menu_action(""), TrayMenuAction::Ignore);
    }

    #[test]
    fn tray_bridge_batch_ids_empty() {
        let none: [(&str, bool); 0] = [];
        assert!(tray_bridge_batch_ids(none, true).is_empty());
        assert!(tray_bridge_batch_ids(none, false).is_empty());
    }

    #[test]
    fn tray_bridge_batch_ids_one_stopped_start() {
        assert_eq!(
            tray_bridge_batch_ids([("p1", false)], true),
            vec!["p1".to_owned()]
        );
    }

    #[test]
    fn tray_bridge_batch_ids_one_running_start() {
        assert!(tray_bridge_batch_ids([("p1", true)], true).is_empty());
    }

    #[test]
    fn tray_bridge_batch_ids_one_running_stop() {
        assert_eq!(
            tray_bridge_batch_ids([("p1", true)], false),
            vec!["p1".to_owned()]
        );
    }

    #[test]
    fn tray_bridge_batch_ids_mixed_only_matching() {
        let profiles = [("a", false), ("b", true), ("c", false), ("d", true)];
        assert_eq!(
            tray_bridge_batch_ids(profiles, true),
            vec!["a".to_owned(), "c".to_owned()]
        );
        assert_eq!(
            tray_bridge_batch_ids(profiles, false),
            vec!["b".to_owned(), "d".to_owned()]
        );
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

    #[test]
    fn parse_tray_language_maps_zh_en_and_unknown() {
        use crate::tray_i18n::parse_tray_language;
        assert_eq!(parse_tray_language("zh-CN"), TrayUiLanguage::Zh);
        assert_eq!(parse_tray_language("zh"), TrayUiLanguage::Zh);
        assert_eq!(parse_tray_language(""), TrayUiLanguage::Zh);
        assert_eq!(parse_tray_language("en"), TrayUiLanguage::En);
        assert_eq!(parse_tray_language("en-US"), TrayUiLanguage::En);
        assert_eq!(parse_tray_language("EN"), TrayUiLanguage::En);
    }

    #[test]
    fn tray_menu_copy_zh_matches_current_labels() {
        use crate::tray_i18n::tray_menu_copy;
        let copy = tray_menu_copy(TrayUiLanguage::Zh);
        assert_eq!(copy.show, "打开 AgentHub");
        assert_eq!(copy.open_routes, "打开路由");
        assert_eq!(copy.start_routes, "启动路由");
        assert_eq!(copy.stop_routes, "停止路由");
        assert_eq!(copy.quit, "退出");
    }

    #[test]
    fn tray_menu_copy_en_matches_requested_labels() {
        use crate::tray_i18n::tray_menu_copy;
        let copy = tray_menu_copy(TrayUiLanguage::En);
        assert_eq!(copy.show, "Open AgentHub");
        assert_eq!(copy.open_routes, "Open routes");
        assert_eq!(copy.start_routes, "Start routes");
        assert_eq!(copy.stop_routes, "Stop routes");
        assert_eq!(copy.quit, "Quit");
    }

    #[test]
    fn tray_dialog_copy_zh_keeps_current_key_labels() {
        use crate::tray_i18n::tray_dialog_copy;
        let copy = tray_dialog_copy(TrayUiLanguage::Zh);
        assert_eq!(copy.running_title, "本机路由正在运行");
        assert_eq!(copy.hide_to_tray, "隐藏到托盘");
        assert_eq!(copy.stop_and_quit, "停止服务并退出");
        assert_eq!(copy.keep_running, "继续运行");
        assert_eq!(copy.cancel, "取消");
    }

    #[test]
    fn tray_dialog_copy_en_uses_requested_key_labels() {
        use crate::tray_i18n::tray_dialog_copy;
        let copy = tray_dialog_copy(TrayUiLanguage::En);
        assert_eq!(copy.running_title, "Local routes running");
        assert_eq!(copy.hide_to_tray, "Hide to tray");
        assert_eq!(copy.stop_and_quit, "Stop and quit");
        assert_eq!(copy.keep_running, "Keep running");
        assert_eq!(copy.cancel, "Cancel");
    }
}
