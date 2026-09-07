use std::path::Path;

use super::{
    linux_nautilus_script, linux_open_with_desktop, linux_servicemenu_desktop,
    parse_open_chat_cwd_arg, resolve_open_chat_cwd, shell_menu_label, windows_open_chat_command,
    OPEN_CHAT_FLAG,
};
use crate::tray_i18n::TrayUiLanguage;

#[test]
fn parse_skips_exe_and_unrelated_flags() {
    assert_eq!(parse_open_chat_cwd_arg(&["agenthub-gui"]), None);
    assert_eq!(
        parse_open_chat_cwd_arg(&["agenthub-gui", "--flag", "x"]),
        None
    );
}

#[test]
fn parse_open_chat_space_and_equals() {
    assert_eq!(
        parse_open_chat_cwd_arg(&["agenthub-gui", OPEN_CHAT_FLAG, r"D:\work\app"]),
        Some(r"D:\work\app".into())
    );
    assert_eq!(
        parse_open_chat_cwd_arg(&["agenthub-gui", "--open-chat=D:/work/app"]),
        Some("D:/work/app".into())
    );
    assert_eq!(
        parse_open_chat_cwd_arg(&["agenthub-gui", r#"--open-chat="C:\My Project""#]),
        Some(r"C:\My Project".into())
    );
}

#[test]
fn parse_open_chat_missing_or_empty_path() {
    assert_eq!(
        parse_open_chat_cwd_arg(&["agenthub-gui", OPEN_CHAT_FLAG]),
        None
    );
    assert_eq!(parse_open_chat_cwd_arg(&["agenthub-gui", "--open-chat="]), None);
}

#[test]
fn parse_strips_quotes_around_path() {
    assert_eq!(
        parse_open_chat_cwd_arg(&["agenthub-gui", OPEN_CHAT_FLAG, r#""/tmp/foo bar""#]),
        Some("/tmp/foo bar".into())
    );
}

#[test]
fn windows_command_uses_percent_v() {
    let cmd = windows_open_chat_command(Path::new(r"C:\Program Files\AgentHub\AgentHub.exe"));
    assert_eq!(
        cmd,
        r#""C:\Program Files\AgentHub\AgentHub.exe" --open-chat "%V""#
    );
}

#[test]
fn menu_label_follows_ui_language() {
    assert_eq!(shell_menu_label(TrayUiLanguage::Zh), "用 AgentHub 打开对话");
    assert_eq!(shell_menu_label(TrayUiLanguage::En), "Open Chat in AgentHub");
}

#[test]
fn linux_desktop_files_point_at_open_chat() {
    let exe = Path::new("/opt/AgentHub/agenthub-gui");
    let service = linux_servicemenu_desktop(exe, TrayUiLanguage::Zh);
    assert!(service.contains("inode/directory"));
    assert!(service.contains(r#""/opt/AgentHub/agenthub-gui" --open-chat %f"#));
    assert!(service.contains("用 AgentHub 打开对话"));
    let open_with = linux_open_with_desktop(exe, TrayUiLanguage::En);
    assert!(open_with.contains("NoDisplay=true"));
    assert!(open_with.contains("Open Chat in AgentHub"));
    let script = linux_nautilus_script(exe);
    assert!(script.starts_with("#!/bin/sh"));
    assert!(script.contains("--open-chat"));
}

#[test]
fn resolve_uses_folder_or_parent_of_file() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("repo");
    std::fs::create_dir(&folder).unwrap();
    let file = folder.join("README.md");
    std::fs::write(&file, "x").unwrap();

    assert_eq!(resolve_open_chat_cwd(folder.to_str().unwrap()).as_deref(), Some(folder.as_path()));
    assert_eq!(
        resolve_open_chat_cwd(file.to_str().unwrap()).as_deref(),
        Some(folder.as_path())
    );
    assert_eq!(resolve_open_chat_cwd(""), None);
    assert_eq!(resolve_open_chat_cwd("/this/path/does/not/exist-agenthub"), None);
}
