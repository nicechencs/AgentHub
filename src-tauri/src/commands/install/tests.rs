use super::*;
use crate::file_manager::{
    applescript_terminal_do_script, codex_app_launch_kind, enclosing_app_bundle,
    explorer_select_arg, file_manager_action, looks_like_codex_bundled_cli,
    macos_codex_app_bundle_names, normalize_open_path_input,
    parse_windows_codex_app_id_from_registry, resolve_cli_launch_path,
    windows_codex_app_id_from_package_full_name, CodexAppLaunchKind, FileManagerAction,
};
use agenthub_core::models::{DetectResult, DetectStatus, DetectedBinaryCopy};
use std::path::PathBuf;

#[test]
fn file_manager_action_reveals_files_and_opens_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("mcp.json");
    std::fs::write(&file, "{}").unwrap();

    match file_manager_action(&file) {
        FileManagerAction::RevealFile(p) => assert_eq!(p, file),
        other => panic!("expected reveal file, got {other:?}"),
    }
    match file_manager_action(dir.path()) {
        FileManagerAction::OpenDir(p) => assert_eq!(p, dir.path()),
        other => panic!("expected open dir, got {other:?}"),
    }
}

#[test]
fn explorer_select_arg_points_at_the_file() {
    let arg = explorer_select_arg(std::path::Path::new(r"C:\Users\demo\.claude.json"));
    assert_eq!(arg, r#"/select,"C:\Users\demo\.claude.json""#);
}

#[test]
fn explorer_select_arg_quotes_paths_with_spaces() {
    let arg = explorer_select_arg(std::path::Path::new(
        r"C:\Users\Nice Chen\.claude\settings.json",
    ));
    assert_eq!(arg, r#"/select,"C:\Users\Nice Chen\.claude\settings.json""#);
}

#[test]
fn normalize_open_path_expands_tilde_and_nested_file() {
    let home = agenthub_core::utils::paths::home_dir().expect("home");
    let got = normalize_open_path_input("~/.grok/auth.json");
    assert_eq!(got, home.join(".grok").join("auth.json"));
}

#[test]
fn lifecycle_key_parser_accepts_open_keys_and_legacy_case() {
    assert_eq!(
        parse_lifecycle_agent_key("demo-agent").unwrap().as_str(),
        "demo-agent"
    );
    assert_eq!(
        parse_lifecycle_agent_key("  CLAUDE  ").unwrap().as_str(),
        "claude"
    );
    assert!(parse_lifecycle_agent_key("Future-Agent").is_err());
}

#[test]
fn resolve_cli_launch_path_picks_cmd_shim() {
    let dir = tempfile::tempdir().unwrap();
    let shim = dir.path().join("claude");
    let cmd = dir.path().join("claude.cmd");
    std::fs::write(&cmd, "@echo off\n").unwrap();
    assert_eq!(resolve_cli_launch_path(&shim), cmd);
}

#[test]
fn resolve_cli_launch_path_skips_windows_shebang_for_cmd() {
    let dir = tempfile::tempdir().unwrap();
    let shim = dir.path().join("claude");
    let cmd = dir.path().join("claude.cmd");
    std::fs::write(&shim, "#!/bin/sh\n").unwrap();
    std::fs::write(&cmd, "@echo off\n").unwrap();
    let got = resolve_cli_launch_path(&shim);
    #[cfg(windows)]
    assert_eq!(got, cmd);
    #[cfg(not(windows))]
    assert_eq!(got, shim);
}

#[test]
fn enclosing_app_bundle_walks_up_from_macos_binary() {
    let inner = std::path::PathBuf::from("Applications")
        .join("WorkBuddy.app")
        .join("Contents")
        .join("MacOS")
        .join("WorkBuddy");
    let got = enclosing_app_bundle(&inner).expect("bundle");
    assert_eq!(got.file_name().unwrap(), "WorkBuddy.app");
}

#[test]
fn enclosing_app_bundle_none_for_plain_cli() {
    assert!(enclosing_app_bundle(std::path::Path::new("/usr/local/bin/claude")).is_none());
}

#[test]
fn applescript_terminal_do_script_uses_quoted_form() {
    let path = std::path::Path::new("/Users/Nice Chen/.local/bin/claude");
    let script = applescript_terminal_do_script(path);
    assert!(script.contains("quoted form of"));
    assert!(script.contains("claude"));
    assert!(script.contains("Nice Chen"));
}

#[test]
fn looks_like_codex_bundled_cli_skips_gui_and_matches_hashed_bin() {
    assert!(looks_like_codex_bundled_cli(std::path::Path::new(
        r"C:\Users\demo\AppData\Local\OpenAI\Codex\bin\b99306303521e97e\codex.exe",
    )));
    assert!(looks_like_codex_bundled_cli(std::path::Path::new(
        r"C:\Program Files\WindowsApps\OpenAI.Codex_1.0.0_x64__2p2nqsd0c76g0\app\resources\codex.exe",
    )));
    assert!(!looks_like_codex_bundled_cli(std::path::Path::new(
        r"C:\Users\demo\AppData\Local\Programs\OpenAI\Codex\Codex.exe",
    )));
    assert!(!looks_like_codex_bundled_cli(std::path::Path::new(
        r"C:\Users\demo\AppData\Local\Programs\WorkBuddy\WorkBuddy.exe",
    )));
    assert!(looks_like_codex_bundled_cli(std::path::Path::new(
        "/Applications/Codex.app/Contents/MacOS/codex",
    )));
    assert!(looks_like_codex_bundled_cli(std::path::Path::new(
        "/Applications/ChatGPT.app/Contents/Resources/codex",
    )));
    assert!(!looks_like_codex_bundled_cli(std::path::Path::new(
        "/usr/local/bin/codex",
    )));
}

#[test]
fn windows_codex_store_app_id_from_package_and_registry() {
    assert_eq!(
        windows_codex_app_id_from_package_full_name(
            "OpenAI.Codex_26.831.1445.0_x64__2p2nqsd0c76g0"
        )
        .as_deref(),
        Some("OpenAI.Codex_2p2nqsd0c76g0!App")
    );
    assert_eq!(
        windows_codex_app_id_from_package_full_name("OpenAI.ChatGPT_1.2.3.4_arm64__2p2nqsd0c76g0")
            .as_deref(),
        Some("OpenAI.ChatGPT_2p2nqsd0c76g0!App")
    );
    assert!(windows_codex_app_id_from_package_full_name("Contoso.Other_1.0.0_x64__pub").is_none());

    let registry = r"HKEY_CURRENT_USER\Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages\OpenAI.Codex_26.715.4045.0_x64__2p2nqsd0c76g0";
    assert_eq!(
        parse_windows_codex_app_id_from_registry(registry).as_deref(),
        Some("OpenAI.Codex_2p2nqsd0c76g0!App")
    );
}

#[test]
fn codex_app_launch_kind_is_platform_specific() {
    let hashed = std::path::Path::new(
        r"C:\Users\demo\AppData\Local\OpenAI\Codex\bin\b99306303521e97e\codex.exe",
    );
    let expected = if cfg!(windows) {
        CodexAppLaunchKind::WindowsStoreOrGui
    } else if cfg!(target_os = "macos") {
        CodexAppLaunchKind::MacosBundle
    } else if cfg!(target_os = "linux") {
        CodexAppLaunchKind::UnsupportedOnLinux
    } else {
        CodexAppLaunchKind::Direct
    };
    assert_eq!(codex_app_launch_kind(hashed), expected);
    assert_eq!(
        codex_app_launch_kind(std::path::Path::new(
            r"C:\Users\demo\AppData\Local\Programs\WorkBuddy\WorkBuddy.exe",
        )),
        CodexAppLaunchKind::Direct
    );
    assert!(macos_codex_app_bundle_names().contains(&"ChatGPT.app"));
    assert!(macos_codex_app_bundle_names().contains(&"Codex.app"));
}

fn detect_installed(
    agent: AgentId,
    channel: &str,
    bin: &str,
    extras: &[(&str, &str)],
) -> DetectResult {
    DetectResult {
        agent,
        status: DetectStatus::Installed,
        version: Some("1".into()),
        binary_path: Some(PathBuf::from(bin)),
        channel: Some(channel.into()),
        env_ready: true,
        notes: vec![],
        extra_copies: extras
            .iter()
            .map(|(kind, path)| {
                DetectedBinaryCopy::from_kind(
                    agent,
                    PathBuf::from(path),
                    kind,
                    None,
                    Some((*kind).into()),
                )
            })
            .collect(),
    }
}

#[test]
fn resolve_agent_launch_path_matches_card_rules() {
    let npm = detect_installed(AgentId::Codex, "npm", "/npm/codex", &[]);
    assert_eq!(
        resolve_agent_launch_path(AgentId::Codex, &npm, "cli", false).unwrap(),
        PathBuf::from("/npm/codex")
    );
    assert!(resolve_agent_launch_path(AgentId::Codex, &npm, "app", false).is_err());

    let desktop = detect_installed(AgentId::Codex, "desktop", r"C:\Store\codex.exe", &[]);
    assert_eq!(
        resolve_agent_launch_path(AgentId::Codex, &desktop, "app", false).unwrap(),
        PathBuf::from(r"C:\Store\codex.exe")
    );

    let both = detect_installed(
        AgentId::Codex,
        "npm",
        "/npm/codex",
        &[("desktop", "/store/codex")],
    );
    assert_eq!(
        resolve_agent_launch_path(AgentId::Codex, &both, "cli", false).unwrap(),
        PathBuf::from("/npm/codex")
    );
    assert_eq!(
        resolve_agent_launch_path(AgentId::Codex, &both, "app", false).unwrap(),
        PathBuf::from("/store/codex")
    );
    assert_eq!(
        resolve_agent_launch_path(AgentId::Codex, &both, "app", true).unwrap_err(),
        "未找到可启动的程序"
    );

    let linux_desktop = detect_installed(AgentId::Codex, "desktop", "/opt/codex", &[]);
    assert_eq!(
        resolve_agent_launch_path(AgentId::Codex, &linux_desktop, "app", true).unwrap_err(),
        "未找到可启动的程序"
    );

    let workbuddy = detect_installed(AgentId::WorkBuddy, "native", "/opt/WorkBuddy.exe", &[]);
    assert_eq!(
        resolve_agent_launch_path(AgentId::WorkBuddy, &workbuddy, "app", false).unwrap(),
        PathBuf::from("/opt/WorkBuddy.exe")
    );
    assert_eq!(
        resolve_agent_launch_path(AgentId::WorkBuddy, &workbuddy, "cli", false).unwrap_err(),
        "未找到可启动的程序"
    );

    let zcode = detect_installed(
        AgentId::Zcode,
        "native",
        "/Applications/ZCode.app/Contents/MacOS/ZCode",
        &[],
    );
    assert_eq!(
        resolve_agent_launch_path(AgentId::Zcode, &zcode, "app", false).unwrap(),
        PathBuf::from("/Applications/ZCode.app/Contents/MacOS/ZCode")
    );

    let ide = detect_installed(AgentId::Codex, "ide", "/ide/codex", &[]);
    assert_eq!(
        resolve_agent_launch_path(AgentId::Codex, &ide, "cli", false).unwrap_err(),
        "未找到可启动的程序"
    );

    let claude = detect_installed(AgentId::Claude, "native", "/usr/local/bin/claude", &[]);
    assert_eq!(
        resolve_agent_launch_path(AgentId::Claude, &claude, "cli", false).unwrap(),
        PathBuf::from("/usr/local/bin/claude")
    );

    assert_eq!(
        resolve_agent_launch_path(AgentId::Claude, &claude, "gui", false).unwrap_err(),
        "unknown launch kind: gui"
    );
}
