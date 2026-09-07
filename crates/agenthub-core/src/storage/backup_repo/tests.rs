use super::*;
use tempfile::tempdir;

fn sample(id: &str, agent: AgentId, kind: BackupKind, created_at: &str) -> BackupRecord {
    BackupRecord {
        id: id.into(),
        agent_id: Some(agent),
        kind,
        path: format!("live/{}/{id}", agent.as_str()),
        files: vec!["settings.json".into()],
        size: 10,
        note: None,
        created_at: created_at.into(),
    }
}

#[test]
fn insert_list_newest_first_and_filter() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = BackupRepo::new(db);

    repo.insert(&sample(
        "a",
        AgentId::Claude,
        BackupKind::Manual,
        "2026-01-01T10:00:00Z",
    ))
    .unwrap();
    repo.insert(&sample(
        "b",
        AgentId::Claude,
        BackupKind::AutoSwitch,
        "2026-01-03T10:00:00Z",
    ))
    .unwrap();
    repo.insert(&sample(
        "c",
        AgentId::Codex,
        BackupKind::PreUninstall,
        "2026-01-02T10:00:00Z",
    ))
    .unwrap();
    // Same timestamp — secondary order by id DESC.
    repo.insert(&sample(
        "z",
        AgentId::Claude,
        BackupKind::PreRestore,
        "2026-01-03T10:00:00Z",
    ))
    .unwrap();

    let all = repo.list(None).unwrap();
    assert_eq!(
        all.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        vec!["z", "b", "c", "a"]
    );

    let claude = repo.list(Some(AgentId::Claude)).unwrap();
    assert_eq!(
        claude.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        vec!["z", "b", "a"]
    );
    assert!(claude.iter().all(|r| r.agent_id == Some(AgentId::Claude)));

    let got = repo.get_by_id("c").unwrap().expect("found");
    assert_eq!(got.kind, BackupKind::PreUninstall);
    assert_eq!(got.agent_id, Some(AgentId::Codex));
    assert!(repo.get_by_id("missing").unwrap().is_none());

    assert!(repo.delete("c").unwrap());
    assert!(repo.get_by_id("c").unwrap().is_none());
    assert!(!repo.delete("c").unwrap());
    assert!(!repo.delete("missing").unwrap());
}

#[test]
fn touch_created_at_updates_only_timestamp() {
    let dir = tempdir().unwrap();
    let db = Database::open(&dir.path().join("t.db")).unwrap();
    let repo = BackupRepo::new(db);
    let rec = sample(
        "a",
        AgentId::Claude,
        BackupKind::Manual,
        "2026-01-01T10:00:00Z",
    );
    repo.insert(&rec).unwrap();

    let updated = repo.touch_created_at("a", "2026-02-01T10:00:00Z").unwrap();
    assert_eq!(updated.id, "a");
    assert_eq!(updated.created_at, "2026-02-01T10:00:00Z");
    assert_eq!(updated.kind, BackupKind::Manual);
    assert_eq!(updated.size, 10);
    assert_eq!(updated.files, rec.files);
    assert_eq!(updated.path, rec.path);
    assert_eq!(updated.note, rec.note);
    assert_eq!(updated.agent_id, rec.agent_id);

    let err = repo
        .touch_created_at("missing", "2026-03-01T00:00:00Z")
        .unwrap_err();
    assert_eq!(err.code(), "not_found");
}
