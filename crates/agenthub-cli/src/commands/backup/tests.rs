use super::*;

fn record() -> BackupRecord {
    BackupRecord {
        id: "backup-1".into(),
        agent_id: Some(AgentId::Codex),
        kind: BackupKind::Manual,
        path: r"D:\tmp\backup-1".into(),
        files: vec!["auth.json".into()],
        size: 42,
        note: Some("manual".into()),
        created_at: "2026-07-31T00:00:00Z".into(),
    }
}

#[test]
fn parses_optional_agent_filter() {
    assert_eq!(parse_agent_filter(None).unwrap(), None);
    assert_eq!(
        parse_agent_filter(Some("CODEX")).unwrap(),
        Some(AgentId::Codex)
    );
    assert_eq!(
        parse_agent_filter(Some("bad")).unwrap_err().code(),
        "invalid_arg"
    );
}

#[test]
fn create_requires_agent() {
    assert_eq!(require_agent(None).unwrap_err().code(), "invalid_arg");
}

#[test]
fn emit_list_quiet_is_ok() {
    emit_list(&[record()], OutputFormat::Quiet).unwrap();
}

#[test]
fn restore_result_has_json_shape() {
    let restored = record();
    let result = RestoreResult {
        restored: restored.clone(),
        pre_restore: Some(restored),
        restored_paths: vec![r"D:\live\auth.json".into()],
    };
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(value["restored"]["id"], "backup-1");
    assert_eq!(value["preRestore"]["id"], "backup-1");
    assert_eq!(value["restoredPaths"][0], r"D:\live\auth.json");
}
