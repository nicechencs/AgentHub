//! Unit tests for skill Tauri command inners (kept out of production source).

use super::*;
use std::fs;
use tempfile::tempdir;

/// Hub whose skills source is a unique temp dir, never `~/.agents/skills`.
fn isolated_hub() -> (tempfile::TempDir, AgentHub) {
    let dir = tempdir().unwrap();
    let base = fs::canonicalize(dir.path()).unwrap_or_else(|_| dir.path().to_path_buf());
    let skills_root = base.join("skills");
    fs::create_dir_all(&skills_root).unwrap();
    let hub = AgentHub::open_with_skills_root(Some(dir.path()), Some(&skills_root)).unwrap();
    (dir, hub)
}

fn hub_with_skills(skill_names: &[&str]) -> (tempfile::TempDir, AgentHub) {
    let _ = skill_names;
    isolated_hub()
}

#[test]
fn list_missing_source_is_empty() {
    let (_dir, hub) = hub_with_skills(&[]);
    let skills = list_skills_inner(&hub).unwrap();
    for s in skills {
        assert_eq!(s.projections.len(), AgentId::ALL.len());
    }
}

#[test]
fn list_skill_catalog_empty_source_is_empty_or_shared_only() {
    let (_dir, hub) = hub_with_skills(&[]);
    let catalog = list_skill_catalog_inner(&hub).unwrap();
    for s in catalog {
        if s.origin == "shared" {
            assert!(s.projectable);
            assert_eq!(
                s.map_status,
                agenthub_core::models::SkillMapStatus::Available
            );
            assert_eq!(s.projections.len(), AgentId::ALL.len());
        } else {
            assert!(!s.projectable);
            assert_eq!(
                s.map_status,
                agenthub_core::models::SkillMapStatus::PrivateSource
            );
            assert!(s.projections.is_empty());
        }
    }
}

#[test]
fn sync_rejects_invalid_agent() {
    let (_dir, hub) = hub_with_skills(&[]);
    let err = sync_skill_inner(&hub, "any", "bad-agent", false, None).unwrap_err();
    assert!(err.contains("invalid agent"));
}

#[test]
fn disable_rejects_invalid_agent() {
    let (_dir, hub) = hub_with_skills(&[]);
    let err = disable_skill_inner(&hub, "any", "nope").unwrap_err();
    assert!(err.contains("invalid agent"));
}

#[test]
fn sync_all_report_shape() {
    let (_dir, hub) = isolated_hub();
    let report = sync_all_skills_inner(&hub, Some("kimi"), false).unwrap();
    let v = serde_json::to_value(&report).unwrap();
    assert!(v["synced"].is_array());
    assert!(v["skipped"].is_array());
    assert!(v["failed"].is_array());

    let all = sync_all_skills_inner(&hub, None, false).unwrap();
    let all_v = serde_json::to_value(&all).unwrap();
    assert!(all_v["synced"].is_array());
    assert!(all_v["skipped"].is_array());
    assert!(all_v["failed"].is_array());
}

#[test]
fn skill_service_sync_disable_with_temp_source() {
    use agenthub_core::adapters::register_all;
    use agenthub_core::services::SkillService;

    let root = tempdir().unwrap();
    let source = root.path().join("skills");
    let skill_dir = source.join("demo-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), "# Demo\n").unwrap();

    let svc = SkillService::new(source.clone(), register_all());
    let listed = svc.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "demo-skill");
}

#[test]
fn read_skill_markdown_shared_via_command_inner() {
    use agenthub_core::adapters::register_all;
    use agenthub_core::services::SkillService;

    let (_root, hub) = isolated_hub();
    let source = hub.skills.source_root().to_path_buf();
    let skill_dir = source.join("preview-demo");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: Preview Demo\ndescription: d\n---\n\n# Hi\n\n**bold**\n",
    )
    .unwrap();
    hub.skills.invalidate_list_cache();

    // Ensure service path works even if list cache was warm elsewhere.
    let _ = SkillService::new(source, register_all());

    let preview = read_skill_markdown_inner(&hub, "preview-demo", None).unwrap();
    assert_eq!(preview.skill_id, "preview-demo");
    assert_eq!(preview.name, "Preview Demo");
    assert!(preview.content.contains("**bold**"));
    assert!(!preview.truncated);

    let err = read_skill_markdown_inner(&hub, "missing-skill", None).unwrap_err();
    assert!(
        err.contains("not found") || err.contains("not_found") || err.contains("SKILL.md"),
        "unexpected error: {err}"
    );

    let bad_id = read_skill_markdown_inner(&hub, "../escape", None).unwrap_err();
    assert!(
        bad_id.contains("invalid") || bad_id.contains("skill id"),
        "unexpected error: {bad_id}"
    );
}

#[test]
fn read_skill_markdown_rejects_invalid_private_agent() {
    let (_dir, hub) = hub_with_skills(&[]);
    let err = read_skill_markdown_inner(&hub, "any", Some("not-an-agent")).unwrap_err();
    assert!(err.contains("invalid agent"), "unexpected: {err}");
}
