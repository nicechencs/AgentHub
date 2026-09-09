use super::{
    chat_menu_submenu_label, chat_shortcut_menu_action, new_chat_menu_label, ACTION_NEW_CHAT,
    CHAT_SHORTCUT_EVENT, MENU_NEW_CHAT, NEW_CHAT_ACCELERATOR,
};
use crate::tray_i18n::TrayUiLanguage;

#[test]
fn menu_id_maps_to_new_chat() {
    assert_eq!(
        chat_shortcut_menu_action(MENU_NEW_CHAT),
        Some(ACTION_NEW_CHAT)
    );
    assert_eq!(chat_shortcut_menu_action("tray-show"), None);
    assert_eq!(chat_shortcut_menu_action(""), None);
}

#[test]
fn accelerator_is_cmd_or_ctrl_n() {
    assert_eq!(NEW_CHAT_ACCELERATOR, "CmdOrCtrl+N");
}

#[test]
fn label_follows_ui_language() {
    assert_eq!(new_chat_menu_label(TrayUiLanguage::Zh), "新建对话");
    assert_eq!(new_chat_menu_label(TrayUiLanguage::En), "New chat");
    assert_eq!(chat_menu_submenu_label(TrayUiLanguage::Zh), "对话");
    assert_eq!(chat_menu_submenu_label(TrayUiLanguage::En), "Chat");
}

#[test]
fn event_name_is_stable() {
    assert_eq!(CHAT_SHORTCUT_EVENT, "chat-shortcut");
}
