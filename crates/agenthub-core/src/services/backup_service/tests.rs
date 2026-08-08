use super::*;
use crate::adapters::AgentAdapter;
use crate::models::{
    AgentConfig, AuthState, Capability, CapabilityState, DetectResult, DetectStatus,
    InstallChannel, RunOptions, RunSpec,
};
use std::sync::Arc;
use tempfile::tempdir;

struct FakeAdapter {
    id: AgentId,
    paths: Vec<PathBuf>,
}

impl AgentAdapter for FakeAdapter {
    fn id(&self) -> AgentId {
        self.id
    }

    fn detect(&self) -> DetectResult {
        DetectResult {
            agent: self.id,
            status: DetectStatus::NotFound,
            version: None,
            binary_path: None,
            channel: None,
            env_ready: true,
            notes: vec![],
        }
    }

    fn install_channels(&self) -> Vec<InstallChannel> {
        vec![]
    }

    fn read_config(&self) -> Result<AgentConfig> {
        Err(AppError::Unsupported("fake".into()))
    }

    fn read_auth(&self) -> Result<AuthState> {
        Err(AppError::Unsupported("fake".into()))
    }

    fn capability(&self, cap: Capability) -> CapabilityState {
        match cap {
            Capability::LiveBackup => CapabilityState::full(),
            _ => CapabilityState::unsupported("fake"),
        }
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        None
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        self.paths.clone()
    }

    fn build_run_spec(&self, _binary: &Path, _prompt: &str, _opts: &RunOptions) -> Result<RunSpec> {
        Err(AppError::Unsupported("fake".into()))
    }
}

fn write_file(path: &Path, content: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

fn try_symlink_dir(target: &Path, link: &Path) -> bool {
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link).is_ok()
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (target, link);
        false
    }
}

fn try_symlink_file(target: &Path, link: &Path) -> bool {
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link).is_ok()
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (target, link);
        false
    }
}

fn make_svc(agent: AgentId, paths: Vec<PathBuf>) -> (tempfile::TempDir, BackupService, PathBuf) {
    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let backups_root = root.path().join("backups");
    std::fs::create_dir_all(&backups_root).unwrap();

    let mut registry = AdapterRegistry::new();
    registry.register(Arc::new(FakeAdapter { id: agent, paths }));

    let svc = BackupService::new(db, registry, backups_root.clone());
    (root, svc, backups_root)
}

#[test]
fn snapshot_missing_files_is_not_found_no_row() {
    let live = tempdir().unwrap();
    let missing = live.path().join("settings.json");
    let (root, svc, backups_root) = make_svc(AgentId::Claude, vec![missing]);

    let err = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, None)
        .unwrap_err();
    assert_eq!(err.code(), "not_found");
    assert!(svc.list(None).unwrap().is_empty());
    let live_root = backups_root.join("live");
    if live_root.exists() {
        let mut any = false;
        if let Ok(agents) = std::fs::read_dir(&live_root) {
            for agent_ent in agents.flatten() {
                if let Ok(snaps) = std::fs::read_dir(agent_ent.path()) {
                    any |= snaps.count() > 0;
                }
            }
        }
        assert!(!any, "should leave no snapshot dirs");
    }
    drop(root);
}

#[test]
fn snapshot_copies_nested_and_duplicate_basenames() {
    let live = tempdir().unwrap();
    let a = live.path().join("cfg").join("settings.json");
    let b = live.path().join("other").join("settings.json");
    let c = live.path().join("nested").join("deep").join("auth.json");
    write_file(&a, b"alpha");
    write_file(&b, b"beta!!");
    write_file(&c, b"auth");

    let (_root, svc, backups_root) =
        make_svc(AgentId::Codex, vec![a.clone(), b.clone(), c.clone()]);

    let rec = svc
        .snapshot(
            AgentId::Codex,
            BackupKind::AutoSwitch,
            Some("before switch"),
        )
        .unwrap();

    assert_eq!(rec.agent_id, Some(AgentId::Codex));
    assert_eq!(rec.kind, BackupKind::AutoSwitch);
    assert_eq!(rec.note.as_deref(), Some("before switch"));
    assert_eq!(rec.files.len(), 3);
    assert_eq!(rec.files[0], "settings.json");
    assert_eq!(rec.files[1], "settings__2.json");
    assert_eq!(rec.files[2], "auth.json");
    assert_eq!(
        rec.size,
        (b"alpha".len() + b"beta!!".len() + b"auth".len()) as u64
    );

    let snap = PathBuf::from(&rec.path);
    assert!(snap.starts_with(backups_root.join("live").join("codex")));
    assert!(snap.join("settings.json").is_file());
    assert!(snap.join("settings__2.json").is_file());
    assert_eq!(std::fs::read(snap.join("settings.json")).unwrap(), b"alpha");
    assert_eq!(
        std::fs::read(snap.join("settings__2.json")).unwrap(),
        b"beta!!"
    );
    assert!(snap.join(MANIFEST_FILE).is_file());

    let listed = svc.list(Some(AgentId::Codex)).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, rec.id);
}

#[test]
fn snapshot_ids_are_unique() {
    let live = tempdir().unwrap();
    let f = live.path().join("only.toml");
    write_file(&f, b"x");

    let (_root, svc, _) = make_svc(AgentId::Grok, vec![f]);

    let r1 = svc
        .snapshot(AgentId::Grok, BackupKind::Manual, None)
        .unwrap();
    let r2 = svc
        .snapshot(AgentId::Grok, BackupKind::Manual, None)
        .unwrap();
    assert_ne!(r1.id, r2.id);
    assert_ne!(r1.path, r2.path);
    assert_eq!(svc.list(None).unwrap().len(), 2);
}

#[test]
fn list_filter_and_newest_first_order() {
    let live = tempdir().unwrap();
    let f = live.path().join("x.json");
    write_file(&f, b"1");

    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let backups_root = root.path().join("backups");
    std::fs::create_dir_all(&backups_root).unwrap();

    let mut registry = AdapterRegistry::new();
    registry.register(Arc::new(FakeAdapter {
        id: AgentId::Claude,
        paths: vec![f.clone()],
    }));
    registry.register(Arc::new(FakeAdapter {
        id: AgentId::Kimi,
        paths: vec![f.clone()],
    }));
    let svc = BackupService::new(db, registry, backups_root);

    let a = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, Some("first"))
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let b = svc
        .snapshot(AgentId::Kimi, BackupKind::PreUninstall, Some("second"))
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let c = svc
        .snapshot(AgentId::Claude, BackupKind::PreRestore, Some("third"))
        .unwrap();

    let all = svc.list(None).unwrap();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].id, c.id);
    assert_eq!(all[1].id, b.id);
    assert_eq!(all[2].id, a.id);

    let claude = svc.list(Some(AgentId::Claude)).unwrap();
    assert_eq!(claude.len(), 2);
    assert_eq!(claude[0].id, c.id);
    assert_eq!(claude[1].id, a.id);
}

#[test]
fn failure_atomicity_no_db_row_and_cleanup() {
    let live = tempdir().unwrap();
    let good = live.path().join("ok.json");
    write_file(&good, b"ok");

    let unsafe_name_dir = live.path().join("sub");
    std::fs::create_dir_all(&unsafe_name_dir).unwrap();
    let bad = unsafe_name_dir.join("bad$file.json");
    write_file(&bad, b"nope");

    let (_root, svc, backups_root) = make_svc(AgentId::Claude, vec![good.clone(), bad.clone()]);

    let err = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, None)
        .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(svc.list(None).unwrap().is_empty());

    let agent_live = backups_root.join("live").join("claude");
    if agent_live.exists() {
        let count = std::fs::read_dir(&agent_live).unwrap().count();
        assert_eq!(count, 0, "incomplete snapshot dir should be removed");
    }
}

#[test]
fn skips_directories_and_only_counts_regular_files() {
    let live = tempdir().unwrap();
    let file = live.path().join("config.toml");
    let dir = live.path().join("projects");
    write_file(&file, b"cfg");
    std::fs::create_dir_all(&dir).unwrap();

    let (_root, svc, _) = make_svc(AgentId::Kimi, vec![file, dir]);
    let rec = svc
        .snapshot(AgentId::Kimi, BackupKind::Manual, None)
        .unwrap();
    assert_eq!(rec.files, vec!["config.toml".to_string()]);
    assert_eq!(rec.size, 3);
}

#[test]
fn unregistered_agent_is_not_found() {
    let root = tempdir().unwrap();
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let backups_root = root.path().join("backups");
    std::fs::create_dir_all(&backups_root).unwrap();
    let svc = BackupService::new(db, AdapterRegistry::new(), backups_root);
    let err = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, None)
        .unwrap_err();
    assert_eq!(err.code(), "not_found");
}

#[test]
fn sanitize_and_allocate_helpers() {
    assert!(sanitize_basename("settings.json").is_ok());
    assert!(sanitize_basename("..").is_err());
    assert!(sanitize_basename("a/b").is_err());
    assert!(sanitize_basename("x$y").is_err());

    let mut occ = HashSet::new();
    let p1 = PathBuf::from("dir").join("settings.json");
    let p2 = PathBuf::from("other").join("settings.json");
    assert_eq!(allocate_dest_name(&p1, &mut occ).unwrap(), "settings.json");
    assert_eq!(
        allocate_dest_name(&p2, &mut occ).unwrap(),
        "settings__2.json"
    );
}

#[test]
fn is_path_inside_guard() {
    let root = PathBuf::from(r"D:\data\backups");
    assert!(is_path_inside(&root.join("live").join("claude"), &root));
    assert!(is_path_inside(&root, &root));
    assert!(!is_path_inside(Path::new(r"D:\data\other"), &root));
    assert!(!is_path_inside(Path::new(r"D:\data\backups_evil"), &root));
}

#[test]
fn canonical_containment_rejects_ancestor_symlink_when_creatable() {
    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let escaped_child = outside.path().join("snapshot");
    std::fs::create_dir_all(&escaped_child).unwrap();
    let link = root.path().join("escape");
    if !try_symlink_dir(outside.path(), &link) {
        return;
    }

    let lexical_child = link.join("snapshot");
    assert!(is_path_inside(&lexical_child, root.path()));
    let err = ensure_existing_path_strictly_inside(&lexical_child, root.path(), "backup.path")
        .unwrap_err();
    assert_eq!(err.code(), "backup.path");
}

#[test]
fn get_by_id_found_and_missing() {
    let live = tempdir().unwrap();
    let f = live.path().join("cfg.json");
    write_file(&f, b"v1");
    let (_root, svc, _) = make_svc(AgentId::Claude, vec![f]);
    let rec = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, None)
        .unwrap();
    let got = svc.get_by_id(&rec.id).unwrap();
    assert_eq!(got.id, rec.id);
    let err = svc.get_by_id("does-not-exist").unwrap_err();
    assert_eq!(err.code(), "not_found");
}

#[test]
fn restore_success_overwrites_live_and_creates_pre_restore() {
    let live = tempdir().unwrap();
    let f = live.path().join("settings.json");
    write_file(&f, b"original");
    let (_root, svc, _) = make_svc(AgentId::Claude, vec![f.clone()]);

    let snap = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, Some("keep me"))
        .unwrap();

    // Corrupt live after snapshot.
    write_file(&f, b"corrupted");

    let result = svc.restore(&snap.id).unwrap();
    assert_eq!(result.restored.id, snap.id);
    assert_eq!(std::fs::read(&f).unwrap(), b"original");
    let pre = result.pre_restore.expect("pre-restore snapshot");
    assert_eq!(pre.kind, BackupKind::PreRestore);
    assert_eq!(pre.agent_id, Some(AgentId::Claude));
    // Pre-restore captured the corrupted live.
    let pre_dir = PathBuf::from(&pre.path);
    assert_eq!(
        std::fs::read(pre_dir.join("settings.json")).unwrap(),
        b"corrupted"
    );
    // Catalog has original + pre-restore.
    assert_eq!(svc.list(None).unwrap().len(), 2);
}

#[test]
fn restore_duplicate_basenames_to_correct_destinations() {
    let live = tempdir().unwrap();
    let a = live.path().join("cfg").join("settings.json");
    let b = live.path().join("other").join("settings.json");
    write_file(&a, b"from-a");
    write_file(&b, b"from-b");

    let (_root, svc, _) = make_svc(AgentId::Codex, vec![a.clone(), b.clone()]);
    let snap = svc
        .snapshot(AgentId::Codex, BackupKind::Manual, None)
        .unwrap();
    assert_eq!(snap.files, vec!["settings.json", "settings__2.json"]);

    write_file(&a, b"live-a");
    write_file(&b, b"live-b");

    let result = svc.restore(&snap.id).unwrap();
    assert_eq!(result.restored_paths.len(), 2);
    assert_eq!(std::fs::read(&a).unwrap(), b"from-a");
    assert_eq!(std::fs::read(&b).unwrap(), b"from-b");
}

#[test]
fn restore_rejects_unregistered_agent() {
    let live = tempdir().unwrap();
    let f = live.path().join("x.json");
    write_file(&f, b"1");
    let (root, svc, _) = make_svc(AgentId::Claude, vec![f]);
    let snap = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, None)
        .unwrap();

    // New service with empty registry but same DB/backups.
    let db = Database::open(&root.path().join("ah.db")).unwrap();
    let svc2 = BackupService::new(db, AdapterRegistry::new(), root.path().join("backups"));
    // Re-open shares the same file; rows should still be visible.
    // Actually Database::open creates a new connection — same file, same data.
    let err = svc2.restore(&snap.id).unwrap_err();
    assert_eq!(err.code(), "not_found");
    // Keep original svc alive for path validity... drop order.
    drop(svc);
    drop(root);
}

#[test]
fn restore_rejects_path_outside_backups_root() {
    let live = tempdir().unwrap();
    let f = live.path().join("x.json");
    write_file(&f, b"1");
    let (root, svc, backups_root) = make_svc(AgentId::Claude, vec![f]);
    let mut snap = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, None)
        .unwrap();

    // Tamper DB path to escape backups_root.
    let evil = root.path().join("evil-outside");
    std::fs::create_dir_all(&evil).unwrap();
    std::fs::write(evil.join("settings.json"), b"nope").unwrap();
    snap.path = evil.display().to_string();
    // Replace row: delete + re-insert with evil path.
    svc.repo().delete(&snap.id).unwrap();
    svc.repo().insert(&snap).unwrap();

    let err = svc.restore(&snap.id).unwrap_err();
    assert_eq!(err.code(), "backup.path");
    let _ = backups_root;
}

#[test]
fn restore_rejects_tampered_manifest_destination() {
    let live = tempdir().unwrap();
    let f = live.path().join("settings.json");
    write_file(&f, b"safe");
    let (root, svc, _) = make_svc(AgentId::Claude, vec![f.clone()]);
    let snap = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, None)
        .unwrap();

    // Point manifest source at a path outside adapter allow-list.
    let evil_target = root.path().join("not-allowed.json");
    write_file(&evil_target, b"victim");
    let manifest = BackupManifest {
        version: MANIFEST_VERSION,
        entries: vec![ManifestEntry {
            stored: "settings.json".into(),
            source: evil_target.display().to_string(),
        }],
    };
    write_manifest(Path::new(&snap.path), &manifest).unwrap();

    let err = svc.restore(&snap.id).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    // Live file must remain unchanged.
    assert_eq!(std::fs::read(&f).unwrap(), b"safe");
    assert_eq!(std::fs::read(&evil_target).unwrap(), b"victim");
}

#[test]
fn restore_rejects_symlink_manifest_when_creatable() {
    let live = tempdir().unwrap();
    let f = live.path().join("settings.json");
    write_file(&f, b"safe");
    let (root, svc, _) = make_svc(AgentId::Claude, vec![f.clone()]);
    let snap = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, None)
        .unwrap();

    let manifest_path = PathBuf::from(&snap.path).join(MANIFEST_FILE);
    std::fs::remove_file(&manifest_path).unwrap();
    let outside_manifest = root.path().join("outside-manifest.json");
    write_file(&outside_manifest, b"{}");
    if !try_symlink_file(&outside_manifest, &manifest_path) {
        return;
    }

    let err = svc.restore(&snap.id).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert_eq!(std::fs::read(&f).unwrap(), b"safe");
}

#[test]
fn restore_rejects_missing_backup_record() {
    let live = tempdir().unwrap();
    let f = live.path().join("x.json");
    write_file(&f, b"1");
    let (_root, svc, _) = make_svc(AgentId::Claude, vec![f]);
    let err = svc.restore("missing-id").unwrap_err();
    assert_eq!(err.code(), "not_found");
}

#[test]
fn restore_rejects_directory_where_file_expected() {
    let live = tempdir().unwrap();
    let f = live.path().join("settings.json");
    write_file(&f, b"data");
    let (_root, svc, _) = make_svc(AgentId::Claude, vec![f.clone()]);
    let snap = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, None)
        .unwrap();

    // Replace snapshot file with a directory.
    let stored = PathBuf::from(&snap.path).join("settings.json");
    std::fs::remove_file(&stored).unwrap();
    std::fs::create_dir_all(&stored).unwrap();

    let err = svc.restore(&snap.id).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn restore_legacy_without_manifest_maps_unique_basenames() {
    let live = tempdir().unwrap();
    let a = live.path().join("cfg").join("settings.json");
    let b = live.path().join("other").join("auth.json");
    write_file(&a, b"A0");
    write_file(&b, b"B0");
    let (_root, svc, _) = make_svc(AgentId::Grok, vec![a.clone(), b.clone()]);
    let snap = svc
        .snapshot(AgentId::Grok, BackupKind::Manual, None)
        .unwrap();

    // Remove manifest to simulate pre-manifest snapshots.
    std::fs::remove_file(PathBuf::from(&snap.path).join(MANIFEST_FILE)).unwrap();

    write_file(&a, b"A1");
    write_file(&b, b"B1");
    let result = svc.restore(&snap.id).unwrap();
    assert_eq!(result.restored_paths.len(), 2);
    assert_eq!(std::fs::read(&a).unwrap(), b"A0");
    assert_eq!(std::fs::read(&b).unwrap(), b"B0");
}

#[test]
fn restore_legacy_rejects_ambiguous_duplicate_basenames() {
    let live = tempdir().unwrap();
    let a = live.path().join("cfg").join("settings.json");
    let b = live.path().join("other").join("settings.json");
    write_file(&a, b"A0");
    write_file(&b, b"B0");
    let (_root, svc, _) = make_svc(AgentId::Grok, vec![a.clone(), b.clone()]);
    let snap = svc
        .snapshot(AgentId::Grok, BackupKind::Manual, None)
        .unwrap();
    std::fs::remove_file(PathBuf::from(&snap.path).join(MANIFEST_FILE)).unwrap();

    write_file(&a, b"A1");
    write_file(&b, b"B1");
    let err = svc.restore(&snap.id).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert_eq!(std::fs::read(&a).unwrap(), b"A1");
    assert_eq!(std::fs::read(&b).unwrap(), b"B1");
}

#[test]
fn restore_rejects_manifest_that_omits_an_indexed_file() {
    let live = tempdir().unwrap();
    let a = live.path().join("first.json");
    let b = live.path().join("second.json");
    write_file(&a, b"A0");
    write_file(&b, b"B0");
    let (_root, svc, _) = make_svc(AgentId::Claude, vec![a.clone(), b.clone()]);
    let snap = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, None)
        .unwrap();

    let manifest = BackupManifest {
        version: MANIFEST_VERSION,
        entries: vec![ManifestEntry {
            stored: "first.json".into(),
            source: a.display().to_string(),
        }],
    };
    write_manifest(Path::new(&snap.path), &manifest).unwrap();

    write_file(&a, b"A1");
    write_file(&b, b"B1");
    let err = svc.restore(&snap.id).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert_eq!(std::fs::read(&a).unwrap(), b"A1");
    assert_eq!(std::fs::read(&b).unwrap(), b"B1");
}

#[test]
fn restore_rejects_manifest_with_duplicate_destination() {
    let live = tempdir().unwrap();
    let a = live.path().join("first.json");
    let b = live.path().join("second.json");
    write_file(&a, b"A0");
    write_file(&b, b"B0");
    let (_root, svc, _) = make_svc(AgentId::Claude, vec![a.clone(), b.clone()]);
    let snap = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, None)
        .unwrap();

    let manifest = BackupManifest {
        version: MANIFEST_VERSION,
        entries: vec![
            ManifestEntry {
                stored: "first.json".into(),
                source: a.display().to_string(),
            },
            ManifestEntry {
                stored: "second.json".into(),
                source: a.display().to_string(),
            },
        ],
    };
    write_manifest(Path::new(&snap.path), &manifest).unwrap();

    let err = svc.restore(&snap.id).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn apply_restore_plan_rolls_back_partial_live_writes() {
    let dir = tempdir().unwrap();
    let dest_a = dir.path().join("a.json");
    let dest_b = dir.path().join("b.json");
    write_file(&dest_a, b"old-a");
    write_file(&dest_b, b"old-b");
    let src_a = dir.path().join("src-a.json");
    write_file(&src_a, b"new-a");
    // Second source missing → second replace fails after first succeeds.
    let src_b = dir.path().join("missing-b.json");

    let plan = vec![
        RestoreItem {
            stored_path: src_a,
            dest: dest_a.clone(),
        },
        RestoreItem {
            stored_path: src_b,
            dest: dest_b.clone(),
        },
    ];
    let err = apply_restore_plan(&plan).unwrap_err();
    assert_eq!(err.code(), "not_found");
    assert_eq!(std::fs::read(&dest_a).unwrap(), b"old-a");
    assert_eq!(std::fs::read(&dest_b).unwrap(), b"old-b");
}

#[test]
fn restore_rejects_corrupt_snapshot_file_before_overwrite() {
    let live = tempdir().unwrap();
    let a = live.path().join("first.json");
    let b = live.path().join("second.json");
    write_file(&a, b"A-orig");
    write_file(&b, b"B-orig");
    let (_root, svc, _) = make_svc(AgentId::Claude, vec![a.clone(), b.clone()]);
    let snap = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, None)
        .unwrap();

    write_file(&a, b"A-live");
    write_file(&b, b"B-live");

    // Corrupt second stored file into a directory — plan validation fails
    // before any live overwrite.
    let stored_b = PathBuf::from(&snap.path).join("second.json");
    std::fs::remove_file(&stored_b).unwrap();
    std::fs::create_dir_all(&stored_b).unwrap();

    let err = svc.restore(&snap.id).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert_eq!(std::fs::read(&a).unwrap(), b"A-live");
    assert_eq!(std::fs::read(&b).unwrap(), b"B-live");
}

#[test]
fn delete_removes_files_and_index() {
    let live = tempdir().unwrap();
    let f = live.path().join("cfg.json");
    write_file(&f, b"x");
    let (_root, svc, _) = make_svc(AgentId::Claude, vec![f]);
    let snap = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, None)
        .unwrap();
    let path = PathBuf::from(&snap.path);
    assert!(path.is_dir());

    svc.delete(&snap.id).unwrap();
    assert!(!path.exists());
    assert!(svc.list(None).unwrap().is_empty());
    let err = svc.get_by_id(&snap.id).unwrap_err();
    assert_eq!(err.code(), "not_found");
}

#[test]
fn delete_missing_on_disk_still_drops_index() {
    let live = tempdir().unwrap();
    let f = live.path().join("cfg.json");
    write_file(&f, b"x");
    let (_root, svc, _) = make_svc(AgentId::Claude, vec![f]);
    let snap = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, None)
        .unwrap();
    std::fs::remove_dir_all(&snap.path).unwrap();
    svc.delete(&snap.id).unwrap();
    assert!(svc.list(None).unwrap().is_empty());
}

#[test]
fn delete_refuses_path_outside_backups_root() {
    let live = tempdir().unwrap();
    let f = live.path().join("cfg.json");
    write_file(&f, b"x");
    let (root, svc, _) = make_svc(AgentId::Claude, vec![f]);
    let mut snap = svc
        .snapshot(AgentId::Claude, BackupKind::Manual, None)
        .unwrap();
    let outside = root.path().join("outside-dir");
    std::fs::create_dir_all(&outside).unwrap();
    snap.path = outside.display().to_string();
    svc.repo().delete(&snap.id).unwrap();
    svc.repo().insert(&snap).unwrap();

    let err = svc.delete(&snap.id).unwrap_err();
    assert_eq!(err.code(), "backup.path");
    assert!(outside.exists());
    // Index row remains.
    assert!(svc.get_by_id(&snap.id).is_ok());
}
