use super::*;
use std::fs;

use crate::utils::test_temp::real_tempdir;

fn write_skill(dir: &Path, id: &str, body: &str) {
    let skill = dir.join(id);
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), body).unwrap();
}

#[test]
fn resolve_rejects_empty_and_relative() {
    let err = resolve_project_workspace("").unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    let err = resolve_project_workspace("relative/workspace").unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn resolve_rejects_missing() {
    let tmp = real_tempdir();
    let missing = tmp.path().join("no-such-workspace");
    let err = resolve_project_workspace(missing.to_str().unwrap()).unwrap_err();
    assert_eq!(err.code(), "not_found");
}

#[test]
fn resolve_accepts_absolute_dir() {
    let tmp = real_tempdir();
    let ws = tmp.path().join("repo");
    fs::create_dir_all(&ws).unwrap();
    let got = resolve_project_workspace(ws.to_str().unwrap()).unwrap();
    assert_eq!(got, normalize_existing_dir(&ws));
}

#[test]
fn unknown_origin_is_rejected() {
    let err = project_skill_root(Path::new("/tmp/ws"), "../etc").unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    let err = normalize_origin(".git/skills").unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn list_empty_workspace_is_empty() {
    let tmp = real_tempdir();
    let ws = tmp.path().join("repo");
    fs::create_dir_all(&ws).unwrap();
    let rows = list_project_workspace_skills(&ws).unwrap();
    assert!(rows.is_empty());
}

#[test]
fn list_discovers_canonical_and_claude_project_skills() {
    let tmp = real_tempdir();
    let ws = tmp.path().join("repo");
    write_skill(
        &ws.join(".agents").join("skills"),
        "notes",
        "---\nname: Notes\ndescription: Shared project skill\n---\n# Notes\n",
    );
    write_skill(
        &ws.join(".claude").join("skills"),
        "review",
        "---\nname: Review\ndescription: Claude project skill\n---\n# Review\n",
    );
    let rows = list_project_workspace_skills(&ws).unwrap();
    assert_eq!(rows.len(), 2);
    let notes = rows.iter().find(|s| s.id == "notes").unwrap();
    assert_eq!(notes.origin, ".agents/skills");
    assert_eq!(notes.root_label, ".agents/skills");
    assert!(!notes.projectable);
    let review = rows.iter().find(|s| s.id == "review").unwrap();
    assert_eq!(review.origin, ".claude/skills");
    assert_eq!(review.name, "Review");
}

#[test]
fn canonical_root_is_agents_skills() {
    let ws = Path::new("/tmp/workspace");
    assert_eq!(
        canonical_project_skills_root(ws),
        ws.join(".agents").join("skills")
    );
}
