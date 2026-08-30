use super::*;
use crate::file_manager::{
    explorer_select_arg, file_manager_action, normalize_open_path_input, FileManagerAction,
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
    assert_eq!(
        arg,
        r#"/select,"C:\Users\Nice Chen\.claude\settings.json""#
    );
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
