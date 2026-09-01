use super::*;
use crate::file_manager::{
    applescript_terminal_do_script, enclosing_app_bundle, explorer_select_arg, file_manager_action,
    normalize_open_path_input, resolve_cli_launch_path, FileManagerAction,
};

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
