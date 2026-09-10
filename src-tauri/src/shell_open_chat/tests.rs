use std::path::Path;

use super::{
    is_cargo_debug_exe, linux_nautilus_script, linux_open_with_desktop, linux_servicemenu_desktop,
    open_chat_arg_missing_folder, parse_open_chat_cwd_arg, resolve_open_chat_cwd,
    resolve_open_chat_from_launch, resolve_shell_register_exe, shell_menu_label,
    should_write_shell_registration, windows_open_chat_command, windows_open_chat_command_with,
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
        parse_open_chat_cwd_arg(&[OPEN_CHAT_FLAG, r"D:\work\app"]),
        Some(r"D:\work\app".into())
    );
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
    assert_eq!(
        parse_open_chat_cwd_arg(&["agenthub-gui", "--open-chat="]),
        None
    );
}

#[test]
fn missing_folder_is_distinct_from_a_normal_second_launch() {
    assert!(!open_chat_arg_missing_folder(&["agenthub-gui"]));
    assert!(open_chat_arg_missing_folder(&[
        "agenthub-gui",
        OPEN_CHAT_FLAG
    ]));
    assert!(open_chat_arg_missing_folder(&[
        "agenthub-gui",
        "--open-chat="
    ]));
    assert!(!open_chat_arg_missing_folder(&[
        "agenthub-gui",
        OPEN_CHAT_FLAG,
        r"D:\work\app"
    ]));
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
        r#""C:\Program Files\AgentHub\AgentHub.exe" --open-chat "%V\.""#
    );
}

/// CommandLineToArgvW rules used by Explorer when it launches the registered command.
fn command_line_to_argv(cmdline: &str) -> Vec<String> {
    let chars: Vec<char> = cmdline.chars().collect();
    let mut args = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let mut arg = String::new();
        let mut in_quotes = false;
        while i < chars.len() {
            if chars[i] == '\\' {
                let mut slashes = 0;
                while i < chars.len() && chars[i] == '\\' {
                    slashes += 1;
                    i += 1;
                }
                if i < chars.len() && chars[i] == '"' {
                    arg.push_str(&"\\".repeat(slashes / 2));
                    if slashes % 2 == 1 {
                        arg.push('"');
                        i += 1;
                    }
                    continue;
                }
                arg.push_str(&"\\".repeat(slashes));
                continue;
            }
            if chars[i] == '"' {
                in_quotes = !in_quotes;
                i += 1;
                continue;
            }
            if !in_quotes && (chars[i] == ' ' || chars[i] == '\t') {
                break;
            }
            arg.push(chars[i]);
            i += 1;
        }
        args.push(arg);
    }
    args
}

#[test]
fn command_line_to_argv_matches_drive_root_escape_bug() {
    assert_eq!(
        command_line_to_argv(r#""C:\AgentHub.exe" --open-chat "C:\""#),
        ["C:\\AgentHub.exe", "--open-chat", "C:\""]
    );
}

#[test]
fn windows_drive_root_and_spaced_folder_survive_argv_parsing() {
    let template = windows_open_chat_command(Path::new(r"C:\Program Files\AgentHub\AgentHub.exe"));
    let drive = command_line_to_argv(&template.replace("%V", r"C:\"));
    assert_eq!(
        drive,
        [
            r"C:\Program Files\AgentHub\AgentHub.exe",
            "--open-chat",
            r"C:\\.",
        ]
    );
    let spaced = command_line_to_argv(&template.replace("%V", r"C:\My Project"));
    assert_eq!(
        spaced,
        [
            r"C:\Program Files\AgentHub\AgentHub.exe",
            "--open-chat",
            r"C:\My Project\.",
        ]
    );
    let ordinary = command_line_to_argv(&template.replace("%V", r"C:\work\app"));
    assert_eq!(ordinary[2], r"C:\work\app\.");
}

#[test]
fn menu_label_follows_ui_language() {
    assert_eq!(shell_menu_label(TrayUiLanguage::Zh), "用 AgentHub 打开对话");
    assert_eq!(
        shell_menu_label(TrayUiLanguage::En),
        "Open Chat in AgentHub"
    );
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

    assert_eq!(
        resolve_open_chat_cwd(folder.to_str().unwrap()).as_deref(),
        Some(folder.as_path())
    );
    assert_eq!(
        resolve_open_chat_cwd(file.to_str().unwrap()).as_deref(),
        Some(folder.as_path())
    );
    assert_eq!(resolve_open_chat_cwd(""), None);
    assert_eq!(resolve_open_chat_cwd("."), None);
    assert_eq!(resolve_open_chat_cwd(r"\."), None);
    assert_eq!(
        resolve_open_chat_cwd("/this/path/does/not/exist-agenthub"),
        None
    );

    let dotted = folder.join(".");
    assert_eq!(
        resolve_open_chat_cwd(dotted.to_str().unwrap()).as_deref(),
        Some(folder.as_path())
    );
}

#[test]
fn appimage_env_wins_over_current_exe_when_file_exists() {
    let dir = tempfile::tempdir().unwrap();
    let appimage = dir.path().join("AgentHub.AppImage");
    std::fs::write(&appimage, b"fake").unwrap();
    let current = dir.path().join("tmp-mount").join("agenthub-gui");
    std::fs::create_dir_all(current.parent().unwrap()).unwrap();
    std::fs::write(&current, b"exe").unwrap();

    assert_eq!(
        resolve_shell_register_exe(Some(&current), Some(&appimage)).as_deref(),
        Some(appimage.as_path())
    );
}

#[test]
fn missing_or_relative_appimage_falls_back_to_current_exe() {
    let dir = tempfile::tempdir().unwrap();
    let current = dir.path().join("agenthub-gui");
    std::fs::write(&current, b"exe").unwrap();
    let missing = dir.path().join("missing.AppImage");
    assert_eq!(
        resolve_shell_register_exe(Some(&current), Some(&missing)).as_deref(),
        Some(current.as_path())
    );
    let relative = Path::new("AgentHub.AppImage");
    assert_eq!(
        resolve_shell_register_exe(Some(&current), Some(relative)).as_deref(),
        Some(current.as_path())
    );
    assert_eq!(
        resolve_shell_register_exe(Some(&current), None).as_deref(),
        Some(current.as_path())
    );
}

#[test]
fn directory_appimage_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let current = dir.path().join("agenthub-gui");
    std::fs::write(&current, b"exe").unwrap();
    assert_eq!(
        resolve_shell_register_exe(Some(&current), Some(dir.path())).as_deref(),
        Some(current.as_path())
    );
}

#[test]
fn selected_folder_verb_uses_percent_1() {
    let cmd = windows_open_chat_command_with(
        Path::new(r"C:\Program Files\AgentHub\AgentHub.exe"),
        r"%1\.",
    );
    assert_eq!(
        cmd,
        r#""C:\Program Files\AgentHub\AgentHub.exe" --open-chat "%1\.""#
    );
}

#[test]
fn cargo_debug_exe_must_not_overwrite_installed_menu() {
    assert!(is_cargo_debug_exe(Path::new(
        r"D:\repo\src-tauri\target\debug\agenthub-gui.exe"
    )));
    assert!(is_cargo_debug_exe(Path::new(
        "/home/demo/src-tauri/target/debug/agenthub-gui"
    )));
    assert!(is_cargo_debug_exe(Path::new(
        r"D:/repo/src-tauri/target\debug\agenthub-gui.exe"
    )));
    assert!(!is_cargo_debug_exe(Path::new(
        r"C:\Users\demo\AppData\Local\AgentHub\agenthub-gui.exe"
    )));
    assert!(!is_cargo_debug_exe(Path::new(
        r"D:\repo\src-tauri\target\release\agenthub-gui.exe"
    )));
    assert!(!should_write_shell_registration(
        Path::new(r"D:\repo\src-tauri\target\debug\agenthub-gui.exe"),
        false
    ));
    assert!(should_write_shell_registration(
        Path::new(r"D:\repo\src-tauri\target\debug\agenthub-gui.exe"),
        true
    ));
    assert!(should_write_shell_registration(
        Path::new(r"C:\Users\demo\AppData\Local\AgentHub\agenthub-gui.exe"),
        false
    ));
}

#[test]
fn launch_falls_back_to_process_cwd_when_explorer_path_is_unusable() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("repo");
    std::fs::create_dir(&folder).unwrap();
    let fallback = folder.to_str().unwrap();

    assert_eq!(
        resolve_open_chat_from_launch(
            &["agenthub-gui", OPEN_CHAT_FLAG, r"\."],
            Some(fallback)
        )
        .as_deref(),
        Some(folder.as_path())
    );
    assert_eq!(
        resolve_open_chat_from_launch(&["agenthub-gui", OPEN_CHAT_FLAG], Some(fallback)).as_deref(),
        Some(folder.as_path())
    );
    assert_eq!(
        resolve_open_chat_from_launch(&["agenthub-gui"], Some(fallback)),
        None
    );
    assert_eq!(
        resolve_open_chat_from_launch(
            &["agenthub-gui", OPEN_CHAT_FLAG, folder.to_str().unwrap()],
            Some("/this/path/does/not-exist")
        )
        .as_deref(),
        Some(folder.as_path())
    );
}
