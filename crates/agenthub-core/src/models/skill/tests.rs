use super::*;

#[test]
fn skill_sync_state_serde_lowercase() {
    for state in [
        SkillSyncState::Unsupported,
        SkillSyncState::Linked,
        SkillSyncState::Copied,
        SkillSyncState::Absent,
        SkillSyncState::Foreign,
        SkillSyncState::Conflict,
    ] {
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, format!("\"{}\"", state.as_str()));
        let back: SkillSyncState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, state);
    }
}

#[test]
fn skill_link_kind_serde_lowercase() {
    for kind in [
        SkillLinkKind::None,
        SkillLinkKind::Symlink,
        SkillLinkKind::Junction,
        SkillLinkKind::Hardlink,
    ] {
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, format!("\"{}\"", kind.as_str()));
        let back: SkillLinkKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, kind);
    }
}

#[test]
fn skill_map_status_serde_snake_case() {
    for status in [
        SkillMapStatus::Available,
        SkillMapStatus::PrivateSource,
        SkillMapStatus::AgentUnsupported,
        SkillMapStatus::AgentNotInstalled,
        SkillMapStatus::TargetUnavailable,
        SkillMapStatus::Conflict,
    ] {
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, format!("\"{}\"", status.as_str()));
        let back: SkillMapStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, status);
    }
    assert!(SkillMapStatus::Available.is_actionable());
    assert!(SkillMapStatus::Conflict.is_actionable());
    assert!(!SkillMapStatus::PrivateSource.is_actionable());
    assert!(!SkillMapStatus::AgentUnsupported.is_actionable());
}

#[test]
fn skill_serde_camel_case() {
    let skill = Skill {
        id: "dbs-diagnosis".into(),
        name: "Diagnosis".into(),
        description: "Business model diagnosis".into(),
        source_dir: PathBuf::from(r"D:\tmp\skills\dbs-diagnosis"),
        projections: vec![
            SkillProjection {
                agent: AgentId::Claude,
                state: SkillSyncState::Linked,
                link_kind: SkillLinkKind::Junction,
                target_dir: Some(PathBuf::from(r"D:\tmp\.claude\skills\dbs-diagnosis")),
                resolved_target: Some(PathBuf::from(r"D:\tmp\skills\dbs-diagnosis")),
                map_status: SkillMapStatus::Available,
            },
            SkillProjection {
                agent: AgentId::Codex,
                state: SkillSyncState::Absent,
                link_kind: SkillLinkKind::None,
                target_dir: Some(PathBuf::from(r"D:\tmp\.codex\skills\dbs-diagnosis")),
                resolved_target: None,
                map_status: SkillMapStatus::Available,
            },
            SkillProjection {
                agent: AgentId::Kimi,
                state: SkillSyncState::Unsupported,
                link_kind: SkillLinkKind::None,
                target_dir: None,
                resolved_target: None,
                map_status: SkillMapStatus::AgentUnsupported,
            },
            SkillProjection {
                agent: AgentId::Grok,
                state: SkillSyncState::Foreign,
                link_kind: SkillLinkKind::None,
                target_dir: Some(PathBuf::from(r"D:\tmp\.grok\skills\dbs-diagnosis")),
                resolved_target: None,
                map_status: SkillMapStatus::Conflict,
            },
        ],
    };
    let v = serde_json::to_value(&skill).unwrap();
    assert_eq!(v["id"], "dbs-diagnosis");
    assert_eq!(v["name"], "Diagnosis");
    assert_eq!(v["description"], "Business model diagnosis");
    assert_eq!(v["sourceDir"], r"D:\tmp\skills\dbs-diagnosis");
    assert_eq!(v["projections"][0]["agent"], "claude");
    assert_eq!(v["projections"][0]["state"], "linked");
    assert_eq!(v["projections"][0]["linkKind"], "junction");
    assert_eq!(v["projections"][0]["mapStatus"], "available");
    assert_eq!(
        v["projections"][0]["resolvedTarget"],
        r"D:\tmp\skills\dbs-diagnosis"
    );
    assert_eq!(v["projections"][1]["state"], "absent");
    assert_eq!(v["projections"][2]["state"], "unsupported");
    assert_eq!(v["projections"][2]["mapStatus"], "agent_unsupported");
    assert_eq!(v["projections"][2]["targetDir"], serde_json::Value::Null);
    assert_eq!(v["projections"][3]["state"], "foreign");
    assert_eq!(v["projections"][3]["mapStatus"], "conflict");
    assert_eq!(
        skill.state_for(AgentId::Grok),
        Some(SkillSyncState::Foreign)
    );
    assert!(SkillSyncState::Linked.is_mapped());
    assert!(SkillSyncState::Copied.is_mapped());
    assert!(!SkillSyncState::Foreign.is_mapped());
}

#[test]
fn skill_sync_report_serde_camel_case() {
    let report = SkillSyncReport {
        synced: vec![SkillAction {
            skill: "demo".into(),
            agent: AgentId::Claude,
        }],
        skipped: vec![SkillAction {
            skill: "demo".into(),
            agent: AgentId::Kimi,
        }],
        failed: vec![SkillFailure {
            skill: "demo".into(),
            agent: AgentId::Grok,
            code: "unsupported".into(),
            error: "nope".into(),
        }],
    };
    let v = serde_json::to_value(&report).unwrap();
    assert!(v["synced"].is_array());
    assert!(v["skipped"].is_array());
    assert!(v["failed"].is_array());
    assert_eq!(v["synced"][0]["skill"], "demo");
    assert_eq!(v["synced"][0]["agent"], "claude");
    assert_eq!(v["skipped"][0]["skill"], "demo");
    assert_eq!(v["skipped"][0]["agent"], "kimi");
    assert_eq!(v["failed"][0]["skill"], "demo");
    assert_eq!(v["failed"][0]["agent"], "grok");
    assert_eq!(v["failed"][0]["code"], "unsupported");
    assert_eq!(v["failed"][0]["error"], "nope");
}
