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
    assert_eq!(tray_menu_action(MENU_ROUTES), TrayMenuAction::Ignore);
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
fn language_from_hub_defaults_zh_without_hub() {
    use crate::tray_i18n::language_from_hub;
    assert_eq!(language_from_hub(None), TrayUiLanguage::Zh);
}

#[test]
fn tray_menu_copy_zh_matches_current_labels() {
    use crate::tray_i18n::tray_menu_copy;
    let copy = tray_menu_copy(TrayUiLanguage::Zh);
    assert_eq!(copy.show, "打开 AgentHub");
    assert_eq!(copy.routes, "本机路由");
    assert_eq!(copy.open_routes, "打开页面");
    assert_eq!(copy.start_routes, "全部启动");
    assert_eq!(copy.stop_routes, "全部停止");
    assert_eq!(copy.quit, "退出");
}

#[test]
fn tray_menu_copy_en_matches_requested_labels() {
    use crate::tray_i18n::tray_menu_copy;
    let copy = tray_menu_copy(TrayUiLanguage::En);
    assert_eq!(copy.show, "Open AgentHub");
    assert_eq!(copy.routes, "Local routes");
    assert_eq!(copy.open_routes, "Open page");
    assert_eq!(copy.start_routes, "Start all");
    assert_eq!(copy.stop_routes, "Stop all");
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
