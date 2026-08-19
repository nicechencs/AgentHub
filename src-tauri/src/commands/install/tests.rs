use super::*;

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
    assert_eq!(arg, r"/select,C:\Users\demo\.claude.json");
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
