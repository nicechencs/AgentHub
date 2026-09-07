use super::*;

#[test]
fn backup_kind_serde_and_parse_roundtrip() {
    for kind in [
        BackupKind::Manual,
        BackupKind::AutoSwitch,
        BackupKind::PreUninstall,
        BackupKind::PreRestore,
        BackupKind::PreSkillUninstall,
    ] {
        let s = kind.as_str();
        assert_eq!(BackupKind::parse(s), Some(kind));
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, format!("\"{s}\""));
        let back: BackupKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, kind);
    }
    assert_eq!(BackupKind::parse("unknown"), None);
    assert_eq!(BackupKind::parse(""), None);
}

#[test]
fn backup_record_serde_camel_case() {
    let rec = BackupRecord {
        id: "bk-1".into(),
        agent_id: Some(AgentId::Claude),
        kind: BackupKind::AutoSwitch,
        path: r"D:\tmp\backups\live\claude\bk-1".into(),
        files: vec!["settings.json".into(), "auth.json".into()],
        size: 42,
        note: Some("before switch".into()),
        created_at: "2026-07-01T12:00:00Z".into(),
    };
    let v = serde_json::to_value(&rec).unwrap();
    assert_eq!(v["id"], "bk-1");
    assert_eq!(v["agentId"], "claude");
    assert_eq!(v["kind"], "auto-switch");
    assert_eq!(v["path"], r"D:\tmp\backups\live\claude\bk-1");
    assert_eq!(v["files"][0], "settings.json");
    assert_eq!(v["size"], 42);
    assert_eq!(v["note"], "before switch");
    assert_eq!(v["createdAt"], "2026-07-01T12:00:00Z");
}

#[test]
fn backup_list_item_flattens_identity() {
    let rec = BackupRecord {
        id: "bk-1".into(),
        agent_id: Some(AgentId::Grok),
        kind: BackupKind::Manual,
        path: "/tmp/bk-1".into(),
        files: vec!["auth.json".into()],
        size: 8,
        note: None,
        created_at: "t0".into(),
    };
    let item = BackupListItem {
        record: rec,
        identity: Some("a@example.com".into()),
    };
    let v = serde_json::to_value(&item).unwrap();
    assert_eq!(v["id"], "bk-1");
    assert_eq!(v["identity"], "a@example.com");
    assert!(v.get("record").is_none());
}
