//! Skills platform helper tests (no network, no user skill dirs).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::models::SkillSourceRecord;

use super::fs_safe::collect_regular_files;
use super::git_update::parse_git_locator;
use super::lockfile::{skill_lock_file, skill_lock_load, skill_lock_remove, skill_lock_upsert};
use super::packages::{
    materialize_projection, replace_target_with_staging, validate_and_collect_source,
    SkillPackageService,
};
use super::sources::{ensure_skill_md, infer_skill_id, SkillSourceService};

#[test]
fn safe_path_component_rejects_cmd_metacharacters() {
    use super::fs_safe::validate_safe_path_component;
    for name in [
        "x&calc",
        "a^b",
        "%PATH%",
        "bang!me",
        "open(close)",
        "ok|pipe",
    ] {
        let err = validate_safe_path_component(name).expect_err(name);
        assert_eq!(err.code(), "invalid_arg", "{name}");
    }
    validate_safe_path_component("normal-skill_id.v1").expect("portable id");
}

#[test]
fn agent_hub_open_with_skills_root_stays_off_user_tree() {
    let (tmp, skills) = crate::utils::test_temp::isolated_skills_root();
    let data = tmp.path().join("data");
    let hub = crate::AgentHub::open_with_skills_root(Some(&data), Some(&skills)).unwrap();
    assert_eq!(hub.skills.source_root(), skills.as_path());
    assert!(
        !skills.join(".locks").exists(),
        "empty recover must not create a lock dir on the skills root"
    );
    if let Ok(home) = crate::utils::paths::home_dir() {
        assert_ne!(
            hub.skills.source_root(),
            home.join(".agents").join("skills").as_path()
        );
    }
}

#[test]
fn git_locator_splits_branch() {
    let (u, b) = parse_git_locator("https://github.com/x/y.git#main");
    assert_eq!(u, "https://github.com/x/y.git");
    assert_eq!(b.as_deref(), Some("main"));
    let (u2, b2) = parse_git_locator("https://github.com/x/y.git");
    assert!(b2.is_none());
    assert_eq!(u2, "https://github.com/x/y.git");
}

#[test]
fn skill_lock_roundtrip_upsert_and_remove() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let root = tmp.path().join("skills");
    fs::create_dir_all(&root).unwrap();

    assert!(skill_lock_load(&root).unwrap().is_empty());
    assert_eq!(skill_lock_file(&root), root.join(".skill-lock.json"));

    let rec = SkillSourceRecord {
        kind: "local".into(),
        locator: r"C:\fake\pkg".into(),
        version: Some("1".into()),
        installed_at: "100".into(),
        updated_at: None,
    };
    skill_lock_upsert(&root, "demo", rec.clone()).unwrap();
    let loaded = skill_lock_load(&root).unwrap();
    assert_eq!(loaded.get("demo"), Some(&rec));

    // Format stays JSON object keyed by skill id (compatible).
    let raw = fs::read_to_string(skill_lock_file(&root)).unwrap();
    assert!(raw.contains("\"demo\""));
    assert!(raw.contains("\"kind\""));
    assert!(raw.contains("local"));

    skill_lock_remove(&root, "demo").unwrap();
    assert!(!skill_lock_load(&root).unwrap().contains_key("demo"));
}

#[test]
fn ensure_skill_md_and_infer_id() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let pkg = tmp.path().join("my-skill");
    fs::create_dir_all(&pkg).unwrap();
    assert!(ensure_skill_md(&pkg).is_err());
    fs::write(pkg.join("SKILL.md"), "# hi\n").unwrap();
    ensure_skill_md(&pkg).unwrap();
    assert_eq!(infer_skill_id(&pkg, r"C:\other\path").unwrap(), "my-skill");
}

#[test]
fn source_service_materializes_local_dir_without_network() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let pkg = tmp.path().join("local-skill");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("SKILL.md"), "---\nname: L\n---\n").unwrap();

    let sources = SkillSourceService::new();
    let (dir, cleanup, kind, locator) = sources.materialize(pkg.to_str().unwrap()).unwrap();
    assert!(cleanup.is_none());
    assert_eq!(kind, "local");
    assert_eq!(locator, pkg.to_str().unwrap());
    sources.ensure_skill_md(&dir).unwrap();
    assert_eq!(
        sources.infer_skill_id(&dir, &locator).unwrap(),
        "local-skill"
    );
}

#[test]
fn package_install_atomicity_first_and_overwrite() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("root");
    fs::create_dir_all(&skills_root).unwrap();

    let pkg = tmp.path().join("src-demo");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("SKILL.md"), "# v1\n").unwrap();
    fs::write(pkg.join("extra.txt"), "one").unwrap();

    let packages = SkillPackageService::new();
    let files = packages.validate_and_collect(&pkg, "demo").unwrap();
    let dest = skills_root.join("demo");
    packages
        .place(&skills_root, "demo", &dest, &files, None)
        .unwrap();
    assert_eq!(fs::read_to_string(dest.join("SKILL.md")).unwrap(), "# v1\n");
    assert_eq!(fs::read_to_string(dest.join("extra.txt")).unwrap(), "one");
    assert_no_helper_dirs(&skills_root);

    // Overwrite atomically with new content.
    fs::write(pkg.join("SKILL.md"), "# v2\n").unwrap();
    fs::write(pkg.join("extra.txt"), "two").unwrap();
    let files2 = packages.validate_and_collect(&pkg, "demo").unwrap();
    packages
        .place(&skills_root, "demo", &dest, &files2, Some(&dest))
        .unwrap();
    assert_eq!(fs::read_to_string(dest.join("SKILL.md")).unwrap(), "# v2\n");
    assert_eq!(fs::read_to_string(dest.join("extra.txt")).unwrap(), "two");
    assert_no_helper_dirs(&skills_root);
}

#[test]
fn package_rejects_missing_and_unsafe_source() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let missing = tmp.path().join("nope");
    let err = validate_and_collect_source(&missing, "x").unwrap_err();
    assert_eq!(err.code(), "not_found");

    let file = tmp.path().join("not-dir");
    fs::write(&file, b"x").unwrap();
    let err = validate_and_collect_source(&file, "x").unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn materialize_cleans_staging_on_path_traversal_map() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("skills-root");
    fs::create_dir_all(&skills_root).unwrap();
    let mut bad = BTreeMap::new();
    bad.insert("../escape.txt".to_string(), b"nope".to_vec());
    let target = skills_root.join("demo");
    let err = materialize_projection(&skills_root, "demo", &target, &bad, None).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(!target.exists());
    assert_no_helper_dirs(&skills_root);
}

#[test]
fn replace_rolls_back_when_staging_missing() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("skills-root");
    fs::create_dir_all(&skills_root).unwrap();
    let target = skills_root.join("demo");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("keep.txt"), "original-target").unwrap();

    let before = collect_regular_files(&target).unwrap();
    let missing_staging = skills_root.join(".agenthub-stage-demo-missing-does-not-exist");
    assert!(!missing_staging.exists());

    let _err =
        replace_target_with_staging(&skills_root, "demo", &target, &target, &missing_staging)
            .unwrap_err();

    assert!(target.is_dir());
    let after = collect_regular_files(&target).unwrap();
    assert_eq!(before, after);
    assert_eq!(
        fs::read_to_string(target.join("keep.txt")).unwrap(),
        "original-target"
    );
    assert_no_helper_dirs(&skills_root);
}

#[test]
fn materialize_missing_branch_preserves_late_conflict() {
    let tmp = crate::utils::test_temp::real_tempdir();
    let skills_root = tmp.path().join("skills-root");
    fs::create_dir_all(&skills_root).unwrap();
    let target = skills_root.join("demo");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("keep.txt"), "late-conflict").unwrap();

    let mut files = BTreeMap::new();
    files.insert("new.txt".to_string(), b"new projection".to_vec());
    let err = materialize_projection(&skills_root, "demo", &target, &files, None).unwrap_err();

    assert_eq!(err.code(), "skill.conflict");
    assert_eq!(
        fs::read_to_string(target.join("keep.txt")).unwrap(),
        "late-conflict"
    );
    assert!(!target.join("new.txt").exists());
    assert_no_helper_dirs(&skills_root);
}

fn assert_no_helper_dirs(skills_root: &Path) {
    for ent in fs::read_dir(skills_root).unwrap() {
        let name = ent.unwrap().file_name().to_string_lossy().into_owned();
        assert!(
            !name.starts_with(".agenthub-stage-") && !name.starts_with(".agenthub-bak-"),
            "leftover helper dir: {name}"
        );
    }
}

#[test]
fn builtin_skill_targets_cover_skills_capable_agents_without_adapter_registry() {
    use crate::adapters::register_all;
    use crate::models::AgentId;
    use crate::platform::skills::{
        builtin_skill_target_registry, SkillTargetRegistry, StaticSkillTarget,
    };
    use crate::platform::AgentKey;
    use std::sync::Arc;

    let builtin = builtin_skill_target_registry();
    let from_adapters =
        SkillTargetRegistry::from_adapter_registry(&register_all()).expect("adapter targets");

    let builtin_keys = builtin.supported_agent_keys();
    let adapter_keys = from_adapters.supported_agent_keys();
    assert_eq!(
        builtin_keys, adapter_keys,
        "builtin StaticSkillTarget set must match adapter-derived membership"
    );

    // Kimi has no skills root — must not appear.
    assert!(!builtin.contains_key(&AgentKey::from_agent_id(AgentId::Kimi)));

    for key in &builtin_keys {
        let target = builtin.get(key).expect("builtin target");
        assert!(target.supports_skills());
        assert!(target.skills_root().is_some());
    }

    // Standalone StaticSkillTarget registers without AgentAdapter.
    let mut custom = SkillTargetRegistry::new();
    custom
        .register(Arc::new(StaticSkillTarget {
            agent_key: AgentKey::parse("skills-only-agent").unwrap(),
            skills_root: Some(std::path::PathBuf::from("/tmp/skills-only")),
            supports: true,
        }))
        .unwrap();
    assert!(custom.contains_key(&AgentKey::parse("skills-only-agent").unwrap()));
}
