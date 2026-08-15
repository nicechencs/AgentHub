use super::*;
use agenthub_core::models::{Skill, SkillLinkKind, SkillProjection, SkillSyncState};

fn sample_skill() -> Skill {
    Skill {
        id: "preview-demo".into(),
        name: "Preview Demo".into(),
        description: String::new(),
        source_dir: std::path::PathBuf::from("/tmp/preview-demo"),
        projections: vec![SkillProjection {
            agent: AgentId::Dsh,
            state: SkillSyncState::Copied,
            link_kind: SkillLinkKind::None,
            target_dir: Some(std::path::PathBuf::from("/tmp/.dsh/skills/preview-demo")),
            resolved_target: None,
            map_status: Default::default(),
        }],
    }
}

#[test]
fn parses_agent_filter_and_requires_agent() {
    assert_eq!(parse_agent_filter(None).unwrap(), None);
    assert_eq!(
        parse_agent_filter(Some("GROK")).unwrap(),
        Some(AgentId::Grok)
    );
    assert_eq!(
        parse_agent_filter(Some("bad")).unwrap_err().code(),
        "invalid_arg"
    );
    assert_eq!(require_agent(None).unwrap_err().code(), "invalid_arg");
}

#[test]
fn empty_outputs_are_valid() {
    emit_list(&[], OutputFormat::Quiet).unwrap();
    emit_sync_report(&SkillSyncReport::default(), OutputFormat::Quiet).unwrap();
    let value = serde_json::to_value(SkillSyncReport::default()).unwrap();
    assert_eq!(value["synced"], serde_json::json!([]));
    assert_eq!(value["skipped"], serde_json::json!([]));
    assert_eq!(value["failed"], serde_json::json!([]));
}

#[test]
fn skill_list_table_covers_catalog_including_dsh() {
    let headers = skill_list_table_headers();
    assert_eq!(&headers[..2], ["Skill", "Name"]);
    let agents = &headers[2..];
    assert_eq!(agents.len(), AgentId::ALL.len());
    for agent in AgentId::ALL {
        assert!(
            agents.iter().any(|header| header == agent.as_str()),
            "missing column {}",
            agent.as_str()
        );
    }
    assert!(agents.iter().any(|header| header == "dsh"));

    let cells = skill_list_table_cells(&sample_skill());
    assert_eq!(cells.len(), headers.len());
    let dsh_index = headers.iter().position(|header| header == "dsh").unwrap();
    assert_eq!(cells[dsh_index], SkillSyncState::Copied.as_str());
}
