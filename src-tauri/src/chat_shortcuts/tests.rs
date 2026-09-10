use super::{
    chat_menu_submenu_label, chat_shortcut_menu_action, edit_menu_copy, new_chat_menu_label,
    ACTION_NEW_CHAT, CHAT_SHORTCUT_EVENT, MENU_NEW_CHAT, NEW_CHAT_ACCELERATOR,
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

#[test]
fn edit_menu_keeps_select_all_and_clipboard() {
    let zh = edit_menu_copy(TrayUiLanguage::Zh);
    assert_eq!(zh.submenu, "编辑");
    assert_eq!(zh.select_all, "全选");
    assert_eq!(zh.copy, "复制");
    assert_eq!(zh.cut, "剪切");
    assert_eq!(zh.paste, "粘贴");
    let en = edit_menu_copy(TrayUiLanguage::En);
    assert_eq!(en.submenu, "Edit");
    assert_eq!(en.select_all, "Select All");
    assert_eq!(en.copy, "Copy");
    assert_eq!(en.cut, "Cut");
    assert_eq!(en.paste, "Paste");
}
