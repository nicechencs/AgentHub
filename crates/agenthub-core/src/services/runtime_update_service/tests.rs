use super::*;

fn status(id: RuntimeId, state: EnvStatusKind, version: Option<&str>) -> EnvStatus {
    EnvStatus {
        id,
        status: state,
        version: version.map(str::to_string),
        path: None,
        min_required: None,
        remediation: None,
        notes: Vec::new(),
    }
}

#[test]
fn compares_semver_after_normalizing_tool_output() {
    assert_eq!(compare_versions("v24.20.0", "24.21.0"), VersionCmp::Less);
    assert_eq!(
        compare_versions("git version 2.50.1", "2.50.1"),
        VersionCmp::Equal
    );
    assert_eq!(compare_versions("11.0", "11.0.1"), VersionCmp::Less);
    assert_eq!(
        compare_versions("2.43.0.windows.1", "2.55.0"),
        VersionCmp::Less
    );
}

#[test]
fn missing_and_broken_runtimes_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let rows = vec![
        status(RuntimeId::Git, EnvStatusKind::Missing, None),
        status(RuntimeId::NodeJs, EnvStatusKind::BrokenPath, Some("24.0.0")),
    ];
    let updates = check_runtime_updates(temp.path(), &rows, None, false).unwrap();
    assert_eq!(updates[0].state, RuntimeUpdateState::NotInstalled);
    assert_eq!(updates[1].state, RuntimeUpdateState::Unknown);
    assert!(updates[1].latest_version.is_none());
}

#[test]
fn fresh_disk_cache_is_used_without_network() {
    let temp = tempfile::tempdir().unwrap();
    let path = latest_cache_path(temp.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let cache = RuntimeLatestCache {
        entries: BTreeMap::from([(
            "git".into(),
            CachedRuntimeLatest {
                version: "2.51.0".into(),
                fetched_at: Utc::now().to_rfc3339(),
            },
        )]),
    };
    fs::write(&path, serde_json::to_string(&cache).unwrap()).unwrap();

    let rows = vec![status(RuntimeId::Git, EnvStatusKind::Ok, Some("2.50.1"))];
    let updates = check_runtime_updates(temp.path(), &rows, None, false).unwrap();
    assert_eq!(updates[0].state, RuntimeUpdateState::UpdateAvailable);
    assert_eq!(updates[0].latest_version.as_deref(), Some("2.51.0"));
    assert_eq!(updates[0].source.as_deref(), Some("git"));
}

#[test]
fn official_urls_are_stable_and_runtime_selection_is_respected() {
    assert_eq!(setup_url(RuntimeId::Git), "https://git-scm.com/downloads");
    assert_eq!(source_for(RuntimeId::NodeJs), "nodejs.org");
    let temp = tempfile::tempdir().unwrap();
    let rows = vec![status(RuntimeId::Git, EnvStatusKind::Missing, None)];
    let updates =
        check_runtime_updates(temp.path(), &rows, Some(&[RuntimeId::NodeJs]), false).unwrap();
    assert_eq!(updates[0].runtime_id, RuntimeId::NodeJs);
    assert_eq!(updates[0].state, RuntimeUpdateState::NotInstalled);
}
