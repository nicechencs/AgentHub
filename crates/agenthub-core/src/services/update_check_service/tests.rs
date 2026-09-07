use super::*;

#[test]
fn npm_urls_encode_scope_slash() {
    assert_eq!(
        npm_package_url("@openai/codex"),
        "https://registry.npmjs.org/@openai%2Fcodex"
    );
    assert_eq!(
        npm_latest_url("@openai/codex"),
        "https://registry.npmjs.org/@openai%2Fcodex/latest"
    );
    assert_eq!(
        npm_package_url("left-pad"),
        "https://registry.npmjs.org/left-pad"
    );
}

#[test]
fn version_cmp_semver() {
    assert_eq!(version_cmp("1.0.0", "1.0.1"), VersionCmp::Less);
    assert_eq!(version_cmp("v1.2.3", "1.2.3"), VersionCmp::Equal);
    assert_eq!(version_cmp("2.0.0", "1.9.9"), VersionCmp::Greater);
    assert_eq!(version_cmp("1.2", "1.2.1"), VersionCmp::Less);
}

#[test]
fn version_cmp_strips_noise() {
    assert_eq!(version_cmp("claude 1.0.5 (x64)", "1.0.6"), VersionCmp::Less);
}

#[test]
fn version_cmp_prerelease_below_release() {
    // 2.0.0-beta.1 < 2.0.0 (semver)
    assert_eq!(version_cmp("2.0.0-beta.1", "2.0.0"), VersionCmp::Less);
    assert_eq!(version_cmp("2.0.0", "2.0.0-beta.1"), VersionCmp::Greater);
}

#[test]
fn normalize_channel_npm() {
    assert_eq!(normalize_channel("npm"), "npm");
    assert_eq!(normalize_channel("NPM"), "npm");
    assert_eq!(normalize_channel("native"), "native");
}

#[test]
fn cache_file_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = latest_cache_path(dir.path());
    let mut cache = LatestCacheFile::default();
    cache.entries.insert(
        cache_key("@openai/codex", Some("0.1.0")),
        CachedLatest {
            version: "0.1.0".into(),
            fetched_at: Utc::now().to_rfc3339(),
            tag: Some("latest".into()),
        },
    );
    save_cache(&path, &cache).unwrap();
    let loaded = load_cache(&path);
    assert_eq!(
        loaded
            .entries
            .get(&cache_key("@openai/codex", Some("0.1.0")))
            .unwrap()
            .version,
        "0.1.0"
    );
}

#[test]
fn compare_marks_update_available() {
    let info = compare_versions(
        AgentId::Codex,
        Some("1.0.0".into()),
        "1.1.0".into(),
        "npm".into(),
        Utc::now().to_rfc3339(),
    );
    assert_eq!(info.state, AgentUpdateState::UpdateAvailable);
    assert_eq!(info.latest_version.as_deref(), Some("1.1.0"));
}

#[test]
fn setup_only_agent_unsupported_includes_setup_url() {
    let info = AgentUpdateInfo::unsupported(
        AgentId::WorkBuddy,
        Some("1.0.0".into()),
        "该 Agent 仅提供官网 Setup，无法自动检测更新",
        native_setup_url(AgentId::WorkBuddy).map(str::to_string),
    );
    assert_eq!(info.state, AgentUpdateState::Unsupported);
    assert_eq!(
        info.setup_url.as_deref(),
        Some("https://www.codebuddy.cn/work/")
    );
}

#[test]
fn compare_marks_up_to_date() {
    let info = compare_versions(
        AgentId::Pi,
        Some("0.83.0".into()),
        "0.83.0".into(),
        "npm".into(),
        Utc::now().to_rfc3339(),
    );
    assert_eq!(info.state, AgentUpdateState::UpToDate);
}

#[test]
fn compare_incomparable_is_unknown_not_update() {
    let info = compare_versions(
        AgentId::Codex,
        Some("build-foo".into()),
        "build-bar".into(),
        "npm".into(),
        Utc::now().to_rfc3339(),
    );
    assert_eq!(info.state, AgentUpdateState::Unknown);
    assert_eq!(info.latest_version.as_deref(), Some("build-bar"));
    assert!(info.note.as_deref().unwrap_or("").contains("无法严格"));
}

#[test]
fn cursor_date_build_versions_compare() {
    // Leading-zero months/days must not break semver parse.
    assert_eq!(
        version_cmp("2026.07.23-e383d2b", "2026.07.23-e383d2b"),
        VersionCmp::Equal
    );
    assert_eq!(
        version_cmp("2026.07.23-e383d2b", "2026.08.01-aabbcc1"),
        VersionCmp::Less
    );
    assert_eq!(
        version_cmp("2026.08.01-aabbcc1", "2026.07.23-e383d2b"),
        VersionCmp::Greater
    );
    // Same calendar day, different commit → date-only equal (Cursor agent.ps1 style).
    assert_eq!(
        version_cmp("2026.07.23-aaaaaa1", "2026.07.23-bbbbbb2"),
        VersionCmp::Equal
    );

    let info = compare_versions(
        AgentId::Cursor,
        Some("2026.07.23-e383d2b".into()),
        "2026.08.01-deadbeef".into(),
        "install-script".into(),
        Utc::now().to_rfc3339(),
    );
    assert_eq!(info.state, AgentUpdateState::UpdateAvailable);
    assert_eq!(info.latest_version.as_deref(), Some("2026.08.01-deadbeef"));
}

#[test]
fn pick_latest_defaults_to_latest_tag() {
    let mut tags = BTreeMap::new();
    tags.insert("latest".into(), "1.0.0".into());
    tags.insert("next".into(), "1.1.0-beta.1".into());
    let (v, tag) = pick_latest_from_dist_tags(&tags, &["next"], Some("0.9.0")).unwrap();
    assert_eq!(v, "1.0.0");
    assert_eq!(tag, "latest");
}

#[test]
fn pick_latest_uses_next_when_local_ahead() {
    let mut tags = BTreeMap::new();
    tags.insert("latest".into(), "1.0.0".into());
    tags.insert("next".into(), "1.1.0-beta.1".into());
    // Local 1.0.5 is ahead of latest 1.0.0 → consult next
    let (v, tag) = pick_latest_from_dist_tags(&tags, &["next"], Some("1.0.5")).unwrap();
    assert_eq!(v, "1.1.0-beta.1");
    assert_eq!(tag, "next");
}

#[test]
fn pick_latest_keeps_latest_when_next_not_higher() {
    let mut tags = BTreeMap::new();
    tags.insert("latest".into(), "2.0.0".into());
    tags.insert("next".into(), "1.9.0".into()); // dirty / lower
    let (v, tag) = pick_latest_from_dist_tags(&tags, &["next"], Some("2.0.1")).unwrap();
    assert_eq!(v, "2.0.0");
    assert_eq!(tag, "latest");
}

#[test]
fn claude_has_next_prerelease_tag() {
    assert_eq!(npm_prerelease_tags(AgentId::Claude), &["next"]);
    assert!(npm_prerelease_tags(AgentId::Codex).is_empty());
}

#[test]
fn invalidate_removes_bucketed_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = latest_cache_path(dir.path());
    let mut cache = LatestCacheFile::default();
    cache.entries.insert(
        cache_key("@anthropic-ai/claude-code", Some("1.0.0")),
        CachedLatest {
            version: "1.0.0".into(),
            fetched_at: Utc::now().to_rfc3339(),
            tag: Some("latest".into()),
        },
    );
    cache.entries.insert(
        "@other/pkg".into(),
        CachedLatest {
            version: "9.0.0".into(),
            fetched_at: Utc::now().to_rfc3339(),
            tag: None,
        },
    );
    save_cache(&path, &cache).unwrap();
    invalidate_latest_cache(dir.path(), AgentId::Claude);
    let loaded = load_cache(&path);
    assert!(!loaded
        .entries
        .keys()
        .any(|k| k.contains("@anthropic-ai/claude-code")));
    assert!(loaded.entries.contains_key("@other/pkg"));
}

#[test]
fn missing_local_version_keeps_unread_note() {
    let info = compare_versions(
        AgentId::Codex,
        None,
        "1.2.3".into(),
        "npm".into(),
        Utc::now().to_rfc3339(),
    );
    assert_eq!(info.state, AgentUpdateState::Unknown);
    assert_eq!(info.note.as_deref(), Some(MISSING_LOCAL_VERSION_NOTE));
    assert!(node_too_old_update_note(AgentId::Codex, &[], &info).is_none());
}

#[test]
fn missing_local_version_pi_node_too_old_overrides_unread_note() {
    let info = compare_versions(
        AgentId::Pi,
        None,
        "0.83.0".into(),
        "npm".into(),
        Utc::now().to_rfc3339(),
    );
    assert_eq!(info.note.as_deref(), Some(MISSING_LOCAL_VERSION_NOTE));
    let note = node_too_old_update_note(
        AgentId::Pi,
        &[crate::runtime::PI_NODE_TOO_OLD_NOTE.into()],
        &info,
    )
    .expect("pi node-too-old note");
    assert!(note.contains("Node too old"));
    assert!(!note.contains(MISSING_LOCAL_VERSION_NOTE));
    assert!(node_too_old_update_note(AgentId::Pi, &[], &info).is_none());
}
