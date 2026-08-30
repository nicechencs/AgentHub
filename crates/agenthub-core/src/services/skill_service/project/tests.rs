use std::fs;

use crate::utils::test_temp::real_tempdir;
use crate::AgentHub;

fn isolated_hub() -> (tempfile::TempDir, AgentHub) {
    let dir = real_tempdir();
    let data = dir.path().join("data");
    let skills = dir.path().join("shared-skills");
    fs::create_dir_all(&skills).unwrap();
    let hub = AgentHub::open_with_skills_root(Some(&data), Some(&skills)).unwrap();
    (dir, hub)
}

fn write_pkg(dir: &std::path::Path, id: &str, body: &str) -> std::path::PathBuf {
    let pkg = dir.join(id);
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("SKILL.md"), body).unwrap();
    pkg
}

#[test]
fn install_list_read_uninstall_roundtrip() {
    let (dir, hub) = isolated_hub();
    let workspace = dir.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    let pkg = write_pkg(
        dir.path(),
        "notes",
        "---\nname: Notes\ndescription: Project notes\n---\n# Notes\nHello.\n",
    );
    let ws = workspace.to_str().unwrap();

    let skill = hub
        .skills()
        .install_project_skill(ws, pkg.to_str().unwrap(), false)
        .unwrap();
    assert_eq!(skill.id, "notes");
    assert_eq!(skill.name, "Notes");
    assert!(skill.projections.is_empty());
    assert!(workspace
        .join(".agents")
        .join("skills")
        .join("notes")
        .is_dir());
    assert!(
        !hub.skills().source_root().join("notes").exists(),
        "project install must not write the user shared library"
    );

    let listed = hub.skills().list_project_skills(ws).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].origin, ".agents/skills");
    assert!(!listed[0].projectable);

    let preview = hub
        .skills()
        .read_project_skill_markdown(ws, "notes", None)
        .unwrap();
    assert_eq!(preview.skill_id, "notes");
    assert!(preview.content.contains("Hello"));

    hub.skills()
        .uninstall_project_skill(ws, "notes", None)
        .unwrap();
    assert!(hub.skills().list_project_skills(ws).unwrap().is_empty());
}

#[test]
fn install_without_overwrite_rejects_duplicate() {
    let (dir, hub) = isolated_hub();
    let workspace = dir.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    let pkg = write_pkg(dir.path(), "dup", "---\nname: Dup\n---\n# Dup\n");
    let ws = workspace.to_str().unwrap();
    hub.skills()
        .install_project_skill(ws, pkg.to_str().unwrap(), false)
        .unwrap();
    let err = hub
        .skills()
        .install_project_skill(ws, pkg.to_str().unwrap(), false)
        .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    hub.skills()
        .install_project_skill(ws, pkg.to_str().unwrap(), true)
        .unwrap();
}

#[test]
fn list_includes_claude_project_folder() {
    let (dir, hub) = isolated_hub();
    let workspace = dir.path().join("workspace");
    let claude = workspace.join(".claude").join("skills").join("review");
    fs::create_dir_all(&claude).unwrap();
    fs::write(
        claude.join("SKILL.md"),
        "---\nname: Review\n---\n# Review\n",
    )
    .unwrap();
    let rows = hub
        .skills()
        .list_project_skills(workspace.to_str().unwrap())
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "review");
    assert_eq!(rows[0].origin, ".claude/skills");
}

#[test]
fn relative_workspace_is_rejected() {
    let (_dir, hub) = isolated_hub();
    let err = hub.skills().list_project_skills("relative/ws").unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn traversal_skill_id_is_rejected() {
    let (dir, hub) = isolated_hub();
    let workspace = dir.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    let err = hub
        .skills()
        .uninstall_project_skill(workspace.to_str().unwrap(), "../escape", None)
        .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn unknown_origin_is_rejected() {
    let (dir, hub) = isolated_hub();
    let workspace = dir.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    let err = hub
        .skills()
        .uninstall_project_skill(workspace.to_str().unwrap(), "notes", Some(".git/skills"))
        .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}
