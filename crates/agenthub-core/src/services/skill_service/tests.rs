use super::*;
use crate::adapters::AgentAdapter;
use crate::error::AppError;
use crate::models::{
    AgentConfig, AuthState, Capability, CapabilityState, DetectResult, DetectStatus,
    InstallChannel, RunOptions, RunSpec,
};
use crate::platform::skills::replace_target_with_staging;
use crate::utils::test_temp::real_tempdir;
use std::sync::Arc;

struct FakeAdapter {
    id: AgentId,
    supports: bool,
    skills_root: Option<PathBuf>,
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
            Capability::Skills if self.supports => CapabilityState::full(),
            Capability::Skills => CapabilityState::unsupported("fake skills unsupported"),
            _ => CapabilityState::unsupported("fake"),
        }
    }

    fn skills_dir(&self) -> Option<PathBuf> {
        self.skills_root.clone()
    }

    fn live_backup_paths(&self) -> Vec<PathBuf> {
        vec![]
    }

    fn build_run_spec(&self, _binary: &Path, _prompt: &str, _opts: &RunOptions) -> Result<RunSpec> {
        Err(AppError::Unsupported("fake".into()))
    }
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for ent in fs::read_dir(src).unwrap() {
        let ent = ent.unwrap();
        let from = ent.path();
        let to = dst.join(ent.file_name());
        if from.is_dir() {
            copy_tree(&from, &to);
        } else {
            fs::copy(&from, &to).unwrap();
        }
    }
}

/// Registry: Claude + Codex + Grok support skills under separate roots; Kimi unsupported.
fn make_registry(claude_root: PathBuf, codex_root: PathBuf, grok_root: PathBuf) -> AdapterRegistry {
    let mut reg = AdapterRegistry::new();
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Claude,
        supports: true,
        skills_root: Some(claude_root),
    }));
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Codex,
        supports: true,
        skills_root: Some(codex_root),
    }));
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Kimi,
        supports: false,
        skills_root: None,
    }));
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Grok,
        supports: true,
        skills_root: Some(grok_root),
    }));
    reg
}

fn skill_md(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n# body\n")
}

fn trees_equal(a: &Path, b: &Path) -> bool {
    collect_regular_files(a).ok() == collect_regular_files(b).ok()
        && collect_regular_files(a).is_ok()
}

fn list_dir_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn assert_no_helper_dirs(dir: &Path) {
    if !dir.exists() {
        return;
    }
    for name in list_dir_names(dir) {
        assert!(
            !name.starts_with(".agenthub-stage-") && !name.starts_with(".agenthub-bak-"),
            "leftover helper dir: {name}"
        );
    }
}

// -----------------------------------------------------------------------
// Existing list / matrix tests
// -----------------------------------------------------------------------

#[test]
fn construction_does_not_scan() {
    // Non-existent source — construction must not error or invent data.
    let reg = AdapterRegistry::new();
    let svc = SkillService::new(PathBuf::from("Z:\\does-not-exist-agenthub-skills"), reg);
    assert!(!svc.source_root().exists());
}

#[test]
fn missing_source_root_returns_empty() {
    let root = real_tempdir();
    let missing = root.path().join("nope");
    let reg = make_registry(
        root.path().join("c"),
        root.path().join("x"),
        root.path().join("g"),
    );
    let svc = SkillService::new(missing, reg);
    let list = svc.list().unwrap();
    assert!(list.is_empty());
}

#[test]
fn source_root_file_is_invalid_arg() {
    let root = real_tempdir();
    let file = root.path().join("not-a-dir");
    fs::write(&file, b"x").unwrap();
    let reg = AdapterRegistry::new();
    let svc = SkillService::new(file, reg);
    let err = svc.list().unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn list_deterministic_order_and_ignores_noise() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    fs::create_dir_all(&source).unwrap();

    // Noise: regular file, lock file, dot dir, skill dirs out of alpha order.
    write_file(&source.join("readme.txt"), "ignore me");
    write_file(&source.join(".skill-lock.json"), "{}");
    fs::create_dir_all(source.join(".hidden-skill")).unwrap();
    write_file(
        &source.join(".hidden-skill").join("SKILL.md"),
        &skill_md("Hidden", "no"),
    );

    write_file(
        &source.join("zebra").join("SKILL.md"),
        &skill_md("Zebra", "z"),
    );
    write_file(
        &source.join("alpha").join("SKILL.md"),
        &skill_md("Alpha", "a"),
    );
    write_file(&source.join("mid").join("SKILL.md"), &skill_md("Mid", "m"));

    let reg = make_registry(
        tmp.path().join("claude"),
        tmp.path().join("codex"),
        tmp.path().join("grok"),
    );
    let svc = SkillService::new(source, reg);
    let list = svc.list().unwrap();
    let ids: Vec<_> = list.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["alpha", "mid", "zebra"]);
}

#[test]
fn list_cache_hits_until_invalidate_or_fingerprint_change() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    fs::create_dir_all(&source).unwrap();
    write_file(
        &source.join("alpha").join("SKILL.md"),
        &skill_md("Alpha", "a"),
    );
    let reg = make_registry(
        tmp.path().join("claude"),
        tmp.path().join("codex"),
        tmp.path().join("grok"),
    );
    let svc = SkillService::new(source.clone(), reg);

    let first = svc.list().unwrap();
    assert_eq!(first.len(), 1);
    // Second call must return the same matrix without re-scan errors.
    let second = svc.list().unwrap();
    assert_eq!(first, second);

    let ids = svc.list_shared_ids().unwrap();
    assert!(ids.contains("alpha"));
    assert_eq!(ids.len(), 1);

    // Explicit invalidate forces rebuild path.
    svc.invalidate_list_cache();
    let third = svc.list().unwrap();
    assert_eq!(third.len(), 1);
    assert_eq!(third[0].id, "alpha");

    // Fingerprint change (new skill dir) must not serve stale cache.
    write_file(
        &source.join("beta").join("SKILL.md"),
        &skill_md("Beta", "b"),
    );
    let fourth = svc.list().unwrap();
    let ids: Vec<_> = fourth.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["alpha", "beta"]);
}

#[test]
fn metadata_from_frontmatter_and_fallback() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");

    write_file(
        &source.join("quoted").join("SKILL.md"),
        "---\nname: \"Quoted Name\"\ndescription: 'A description'\n---\nbody\n",
    );
    // Missing SKILL.md → fallback name, empty description.
    fs::create_dir_all(source.join("no-md")).unwrap();
    // Malformed frontmatter (no closing fence).
    write_file(
        &source.join("bad-fm").join("SKILL.md"),
        "---\nname: Broken\ndescription: nope\n# no close\n",
    );
    // Present but empty name value → fallback to directory name.
    write_file(
        &source.join("empty-name").join("SKILL.md"),
        "---\nname: \ndescription: only-desc\n---\n",
    );

    let reg = make_registry(
        tmp.path().join("claude"),
        tmp.path().join("codex"),
        tmp.path().join("grok"),
    );
    let svc = SkillService::new(source, reg);
    let list = svc.list().unwrap();
    let by_id = |id: &str| list.iter().find(|s| s.id == id).unwrap();

    assert_eq!(by_id("quoted").name, "Quoted Name");
    assert_eq!(by_id("quoted").description, "A description");

    assert_eq!(by_id("no-md").name, "no-md");
    assert_eq!(by_id("no-md").description, "");

    assert_eq!(by_id("bad-fm").name, "bad-fm");
    assert_eq!(by_id("bad-fm").description, "");

    assert_eq!(by_id("empty-name").name, "empty-name");
    assert_eq!(by_id("empty-name").description, "only-desc");
}

#[test]
fn parse_frontmatter_unit_cases() {
    let (n, d) = parse_skill_frontmatter("---\nname: plain\ndescription: desc text\n---\n", "fb");
    assert_eq!(n, "plain");
    assert_eq!(d, "desc text");

    // No frontmatter.
    let (n, d) = parse_skill_frontmatter("# just markdown\n", "fb");
    assert_eq!(n, "fb");
    assert_eq!(d, "");

    // Extra keys ignored; first name/description wins.
    let (n, d) = parse_skill_frontmatter(
        "---\nname: first\nname: second\nother: x\ndescription: d1\ndescription: d2\n---\n",
        "fb",
    );
    assert_eq!(n, "first");
    assert_eq!(d, "d1");

    // YAML block scalar `|` — multi-line description collapsed for list UI.
    let (n, d) = parse_skill_frontmatter(
        "---\nname: agent-builder\ndescription: |\n  Design and build AI agents.\n  Use when users ask to create an agent.\n---\n",
        "fb",
    );
    assert_eq!(n, "agent-builder");
    assert_eq!(
        d,
        "Design and build AI agents. Use when users ask to create an agent."
    );

    // Folded `>` and chomping indicators.
    let (n, d) = parse_skill_frontmatter(
        "---\nname: dbs\ndescription: >-\n  line one\n  line two\n---\n",
        "fb",
    );
    assert_eq!(n, "dbs");
    assert_eq!(d, "line one line two");

    // Bare `|` must never become the description text.
    let (n, d) = parse_skill_frontmatter("---\nname: x\ndescription: |\n---\n", "fb");
    assert_eq!(n, "x");
    assert_eq!(d, "");
}

#[test]
fn projection_states_unsupported_absent_copied_foreign() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");

    // Source skill with nested layout.
    write_file(
        &source.join("demo").join("SKILL.md"),
        &skill_md("Demo", "demo skill"),
    );
    write_file(&source.join("demo").join("nested").join("a.txt"), "alpha");

    // Claude: exact copy → Copied
    copy_tree(&source.join("demo"), &claude.join("demo"));

    // Codex: missing target → Absent (do not create)

    // Grok: target exists but different bytes → Foreign
    write_file(
        &grok.join("demo").join("SKILL.md"),
        &skill_md("Demo", "demo skill"),
    );
    write_file(&grok.join("demo").join("nested").join("a.txt"), "CHANGED");

    let reg = make_registry(claude, codex, grok);
    let svc = SkillService::new(source, reg);
    let list = svc.list().unwrap();
    assert_eq!(list.len(), 1);
    let skill = &list[0];
    assert_eq!(skill.id, "demo");

    // AgentId::ALL order (fake registry omits Pi → Unsupported for that row)
    assert_eq!(skill.projections.len(), AgentId::ALL.len());
    assert_eq!(skill.projections[0].agent, AgentId::Claude);
    assert_eq!(skill.projections[1].agent, AgentId::Codex);
    assert_eq!(skill.projections[2].agent, AgentId::Kimi);
    assert_eq!(skill.projections[3].agent, AgentId::Grok);
    assert_eq!(skill.projections[4].agent, AgentId::Pi);

    assert_eq!(skill.projections[0].state, SkillSyncState::Copied);
    assert_eq!(skill.projections[0].link_kind, SkillLinkKind::None);
    assert_eq!(skill.projections[1].state, SkillSyncState::Absent);
    assert_eq!(skill.projections[2].state, SkillSyncState::Unsupported);
    assert_eq!(skill.projections[2].target_dir, None);
    assert_eq!(skill.projections[3].state, SkillSyncState::Foreign);
    assert_eq!(skill.projections[4].state, SkillSyncState::Unsupported);
}

#[test]
fn nested_identical_trees_are_copied() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");

    write_file(
        &source.join("deep").join("a").join("b").join("c.txt"),
        "deep",
    );
    write_file(&source.join("deep").join("root.txt"), "root");
    copy_tree(&source.join("deep"), &claude.join("deep"));
    copy_tree(&source.join("deep"), &codex.join("deep"));
    copy_tree(&source.join("deep"), &grok.join("deep"));

    let reg = make_registry(claude, codex, grok);
    let svc = SkillService::new(source, reg);
    let skill = &svc.list().unwrap()[0];
    for p in &skill.projections {
        match p.agent {
            // Fake registry: Kimi has no skills; Pi/WorkBuddy/Cursor unregistered → Unsupported.
            AgentId::Kimi | AgentId::Pi | AgentId::WorkBuddy | AgentId::Cursor => {
                assert_eq!(p.state, SkillSyncState::Unsupported, "agent {}", p.agent);
            }
            AgentId::Claude | AgentId::Codex | AgentId::Grok => {
                assert_eq!(p.state, SkillSyncState::Copied, "agent {}", p.agent);
            }
        }
    }
}

#[test]
fn extra_target_file_is_foreign() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");

    write_file(&source.join("s").join("only.txt"), "one");
    copy_tree(&source.join("s"), &claude.join("s"));
    write_file(&claude.join("s").join("extra.txt"), "extra");

    let reg = make_registry(claude, codex, grok);
    let svc = SkillService::new(source, reg);
    let skill = &svc.list().unwrap()[0];
    assert_eq!(
        skill.state_for(AgentId::Claude),
        Some(SkillSyncState::Foreign)
    );
    assert_eq!(
        skill.state_for(AgentId::Codex),
        Some(SkillSyncState::Absent)
    );
}

#[test]
fn supports_true_but_no_skills_dir_is_unsupported() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    fs::create_dir_all(source.join("x")).unwrap();

    let mut reg = AdapterRegistry::new();
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Claude,
        supports: true,
        skills_root: None, // inconsistent but defensive
    }));
    // Other agents unregistered → also Unsupported
    let svc = SkillService::new(source, reg);
    let skill = &svc.list().unwrap()[0];
    for p in &skill.projections {
        assert_eq!(p.state, SkillSyncState::Unsupported);
        assert!(p.target_dir.is_none());
        assert_eq!(p.link_kind, SkillLinkKind::None);
        assert!(p.resolved_target.is_none());
        if p.agent == AgentId::Claude {
            assert_eq!(p.map_status, SkillMapStatus::TargetUnavailable);
        } else {
            assert_eq!(p.map_status, SkillMapStatus::AgentUnsupported);
        }
    }
}

#[test]
fn target_path_is_file_is_conflict() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    fs::create_dir_all(source.join("s")).unwrap();
    // Target skill path is a regular file, not a directory.
    write_file(&claude.join("s"), "not a dir");

    let mut reg = AdapterRegistry::new();
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Claude,
        supports: true,
        skills_root: Some(claude),
    }));
    let svc = SkillService::new(source, reg);
    let skill = &svc.list().unwrap()[0];
    assert_eq!(
        skill.state_for(AgentId::Claude),
        Some(SkillSyncState::Conflict)
    );
}

#[cfg(unix)]
#[test]
fn symlink_inside_tree_is_conflict() {
    use std::os::unix::fs::symlink;

    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");

    write_file(&source.join("s").join("real.txt"), "ok");
    copy_tree(&source.join("s"), &claude.join("s"));
    // Add a symlink on the target side.
    symlink("real.txt", claude.join("s").join("link.txt")).unwrap();

    let reg = make_registry(claude, codex, grok);
    let svc = SkillService::new(source, reg);
    let skill = &svc.list().unwrap()[0];
    assert_eq!(
        skill.state_for(AgentId::Claude),
        Some(SkillSyncState::Conflict)
    );
}

#[cfg(windows)]
#[test]
fn symlink_inside_tree_is_conflict_when_creatable() {
    use std::os::windows::fs::symlink_file;

    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");

    write_file(&source.join("s").join("real.txt"), "ok");
    copy_tree(&source.join("s"), &claude.join("s"));

    // Windows may require Developer Mode / elevation; skip if creation fails.
    if symlink_file("real.txt", claude.join("s").join("link.txt")).is_err() {
        return;
    }

    let reg = make_registry(claude, codex, grok);
    let svc = SkillService::new(source, reg);
    let skill = &svc.list().unwrap()[0];
    assert_eq!(
        skill.state_for(AgentId::Claude),
        Some(SkillSyncState::Conflict)
    );
}

// -----------------------------------------------------------------------
// sync / disable write-path tests
// -----------------------------------------------------------------------

fn setup_write_fixture() -> (
    tempfile::TempDir,
    PathBuf,
    PathBuf,
    PathBuf,
    PathBuf,
    SkillService,
) {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    fs::create_dir_all(&source).unwrap();
    let reg = make_registry(claude.clone(), codex.clone(), grok.clone());
    let svc = SkillService::new(source.clone(), reg);
    (tmp, source, claude, codex, grok, svc)
}

#[test]
fn sync_happy_path_creates_projection() {
    let (_tmp, source, claude, _codex, _grok, svc) = setup_write_fixture();
    write_file(
        &source.join("demo").join("SKILL.md"),
        &skill_md("Demo", "d"),
    );
    write_file(&source.join("demo").join("nested").join("a.txt"), "alpha");

    svc.sync("demo", AgentId::Claude, false).unwrap();

    let target = claude.join("demo");
    assert!(target.is_dir());
    assert!(trees_equal(&source.join("demo"), &target));
    let skill = &svc.list().unwrap()[0];
    assert_eq!(
        skill.state_for(AgentId::Claude),
        Some(SkillSyncState::Copied)
    );
}

#[test]
fn sync_unsupported_agent_errors() {
    let (_tmp, source, _c, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "d"));

    let err = svc.sync("demo", AgentId::Kimi, false).unwrap_err();
    assert_eq!(err.code(), "unsupported");

    // supports=true but no skills_dir
    let mut reg = AdapterRegistry::new();
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Claude,
        supports: true,
        skills_root: None,
    }));
    let svc2 = SkillService::new(source.clone(), reg);
    let err = svc2.sync("demo", AgentId::Claude, false).unwrap_err();
    assert_eq!(err.code(), "unsupported");

    // unregistered agent
    let svc3 = SkillService::new(source, AdapterRegistry::new());
    let err = svc3.sync("demo", AgentId::Codex, false).unwrap_err();
    assert_eq!(err.code(), "not_found");
}

#[test]
fn sync_rejects_invalid_skill_ids_and_traversal() {
    let (_tmp, source, _c, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "d"));

    for bad in [
        "",
        ".",
        "..",
        "a/b",
        "a\\b",
        "../demo",
        "..\\demo",
        "/demo",
        "demo/../x",
        "foo:bar",
        "foo<bar",
        "foo>bar",
        "foo|bar",
        "foo\"bar",
        "foo?bar",
        "foo*bar",
        "demo.",
        "demo ",
        "CON",
        "con.txt",
        "PRN",
        "AUX",
        "NUL",
        "COM1",
        "LPT9",
        "com5.data",
    ] {
        let err = svc.sync(bad, AgentId::Claude, false).unwrap_err();
        assert_eq!(err.code(), "invalid_arg", "id={bad:?}");
    }

    // Absolute / rooted forms (platform-dependent parsing still rejected).
    #[cfg(windows)]
    {
        let err = svc.sync(r"C:\demo", AgentId::Claude, false).unwrap_err();
        assert_eq!(err.code(), "invalid_arg");
    }
    #[cfg(unix)]
    {
        let err = svc.sync("/demo", AgentId::Claude, false).unwrap_err();
        assert_eq!(err.code(), "invalid_arg");
    }
}

#[test]
fn sync_missing_source_is_not_found() {
    let (_tmp, _source, _c, _x, _g, svc) = setup_write_fixture();
    let err = svc.sync("nope", AgentId::Claude, false).unwrap_err();
    assert_eq!(err.code(), "not_found");
}

#[cfg(unix)]
#[test]
fn sync_rejects_source_symlink_root_unix() {
    use std::os::unix::fs::symlink;

    let (tmp, source, _c, _x, _g, svc) = setup_write_fixture();
    let real = tmp.path().join("real-skill");
    write_file(&real.join("SKILL.md"), &skill_md("R", "r"));
    symlink(&real, source.join("demo")).unwrap();

    let err = svc.sync("demo", AgentId::Claude, false).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[cfg(unix)]
#[test]
fn sync_rejects_symlink_inside_source_unix() {
    use std::os::unix::fs::symlink;

    let (_tmp, source, _c, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("real.txt"), "ok");
    symlink("real.txt", source.join("demo").join("link.txt")).unwrap();

    let err = svc.sync("demo", AgentId::Claude, false).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[cfg(windows)]
#[test]
fn sync_rejects_symlink_inside_source_when_creatable() {
    use std::os::windows::fs::symlink_file;

    let (_tmp, source, _c, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("real.txt"), "ok");
    if symlink_file("real.txt", source.join("demo").join("link.txt")).is_err() {
        return;
    }
    let err = svc.sync("demo", AgentId::Claude, false).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn sync_conflict_without_force_leaves_target_unchanged() {
    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "new"));
    write_file(&source.join("demo").join("a.txt"), "source");

    write_file(&claude.join("demo").join("SKILL.md"), &skill_md("D", "old"));
    write_file(&claude.join("demo").join("a.txt"), "target-old");
    write_file(&claude.join("demo").join("extra.txt"), "keep-me");

    let before = collect_regular_files(&claude.join("demo")).unwrap();
    let err = svc.sync("demo", AgentId::Claude, false).unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    let after = collect_regular_files(&claude.join("demo")).unwrap();
    assert_eq!(before, after);
}

#[test]
fn sync_identical_is_noop_success() {
    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "d"));
    write_file(&source.join("demo").join("nested").join("x.txt"), "x");

    // Platform-created projection has ownership marker; second sync is no-op.
    svc.sync("demo", AgentId::Claude, false).unwrap();
    let before = collect_regular_files(&claude.join("demo")).unwrap();
    svc.sync("demo", AgentId::Claude, false).unwrap();
    svc.sync("demo", AgentId::Claude, true).unwrap();
    let after = collect_regular_files(&claude.join("demo")).unwrap();
    assert_eq!(before, after);
    // No leftover staging/backup siblings (ownership store is under .agenthub).
    let names = list_dir_names(&claude);
    assert!(names.contains(&"demo".to_string()));
    assert!(names.iter().all(|n| n == "demo" || n == ".agenthub"));
}

#[test]
fn sync_byte_identical_without_marker_is_conflict() {
    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "d"));
    write_file(&source.join("demo").join("nested").join("x.txt"), "x");
    // Manual user copy: content matches source but no ownership marker.
    copy_tree(&source.join("demo"), &claude.join("demo"));

    let before = collect_regular_files(&claude.join("demo")).unwrap();
    let err = svc.sync("demo", AgentId::Claude, false).unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    let after = collect_regular_files(&claude.join("demo")).unwrap();
    assert_eq!(before, after, "unmanaged content must be preserved");
}

#[test]
fn sync_force_cannot_take_over_foreign_directory() {
    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "new"));
    write_file(&source.join("demo").join("only-new.txt"), "fresh");

    write_file(&claude.join("demo").join("SKILL.md"), &skill_md("D", "old"));
    write_file(&claude.join("demo").join("only-old.txt"), "stale");
    // Sibling skill that must not be touched.
    write_file(&claude.join("sibling").join("keep.txt"), "sibling-data");

    let before = collect_regular_files(&claude.join("demo")).unwrap();
    let err = svc.sync("demo", AgentId::Claude, true).unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    assert!(
        !err.to_string().contains("use force to replace"),
        "must not suggest force takeover: {err}"
    );
    let after = collect_regular_files(&claude.join("demo")).unwrap();
    assert_eq!(before, after, "foreign content preserved under force");
    assert_eq!(
        fs::read_to_string(claude.join("sibling").join("keep.txt")).unwrap(),
        "sibling-data"
    );
}

#[test]
fn sync_force_refreshes_managed_copy_and_cleans_staging() {
    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "old"));
    write_file(&source.join("demo").join("only-old.txt"), "stale");
    write_file(&claude.join("sibling").join("keep.txt"), "sibling-data");

    // Platform-owned projection first.
    svc.sync("demo", AgentId::Claude, false).unwrap();

    // Source updates → force refreshes managed copy.
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "new"));
    write_file(&source.join("demo").join("only-new.txt"), "fresh");
    let _ = fs::remove_file(source.join("demo").join("only-old.txt"));

    svc.sync("demo", AgentId::Claude, true).unwrap();

    assert!(trees_equal(&source.join("demo"), &claude.join("demo")));
    assert!(!claude.join("demo").join("only-old.txt").exists());
    assert_eq!(
        fs::read_to_string(claude.join("sibling").join("keep.txt")).unwrap(),
        "sibling-data"
    );
    assert_no_helper_dirs(&claude);
    let names = list_dir_names(&claude);
    assert!(names.contains(&"demo".to_string()));
    assert!(names.contains(&"sibling".to_string()));
}

#[test]
fn sync_failed_source_validation_leaves_no_staging() {
    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    // Source missing → not_found; skills root may not even exist yet.
    let err = svc.sync("ghost", AgentId::Claude, true).unwrap_err();
    assert_eq!(err.code(), "not_found");
    if claude.exists() {
        assert_no_helper_dirs(&claude);
    }
    // Invalid id never creates staging either.
    let _ = svc.sync("../x", AgentId::Claude, true);
    if claude.exists() {
        assert_no_helper_dirs(&claude);
    }
    // Create skills root with sibling only, then reject bad source as file.
    write_file(&source.join("demo"), "not-a-dir");
    fs::create_dir_all(&claude).unwrap();
    write_file(&claude.join("sibling").join("k.txt"), "ok");
    let err = svc.sync("demo", AgentId::Claude, true).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert_eq!(list_dir_names(&claude), vec!["sibling".to_string()]);
}

#[test]
fn disable_missing_target_is_idempotent_success() {
    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "d"));
    // Target never created.
    svc.disable("demo", AgentId::Claude).unwrap();
    svc.disable("demo", AgentId::Claude).unwrap();
    assert!(!claude.join("demo").exists());
}

#[test]
fn disable_removes_exact_target_preserves_siblings() {
    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "d"));
    write_file(&source.join("other").join("SKILL.md"), &skill_md("O", "o"));
    svc.sync("demo", AgentId::Claude, false).unwrap();
    svc.sync("other", AgentId::Claude, false).unwrap();

    svc.disable("demo", AgentId::Claude).unwrap();
    assert!(!claude.join("demo").exists());
    assert!(claude.join("other").is_dir());
    assert_eq!(
        fs::read_to_string(claude.join("other").join("SKILL.md")).unwrap(),
        skill_md("O", "o")
    );
}

#[test]
fn disable_rejects_target_file() {
    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "d"));
    write_file(&claude.join("demo"), "not-a-dir");
    let err = svc.disable("demo", AgentId::Claude).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    // File left intact.
    assert_eq!(
        fs::read_to_string(claude.join("demo")).unwrap(),
        "not-a-dir"
    );
}

#[cfg(unix)]
#[test]
fn disable_removes_target_symlink_without_touching_source_unix() {
    use std::os::unix::fs::symlink;

    let (tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(
        &source.join("demo").join("SKILL.md"),
        &skill_md("D", "must-survive"),
    );
    write_file(&source.join("demo").join("payload.txt"), "source-bytes");
    let elsewhere = tmp.path().join("elsewhere");
    write_file(&elsewhere.join("keep.txt"), "other");
    fs::create_dir_all(&claude).unwrap();
    symlink(&source.join("demo"), claude.join("demo")).unwrap();

    // Linked projection should be recognized.
    let skill = &svc.list().unwrap()[0];
    assert_eq!(
        skill.state_for(AgentId::Claude),
        Some(SkillSyncState::Linked)
    );

    svc.disable("demo", AgentId::Claude).unwrap();

    // Link gone; source and foreign tree intact.
    assert!(fs::symlink_metadata(claude.join("demo")).is_err());
    assert_eq!(
        fs::read_to_string(source.join("demo").join("payload.txt")).unwrap(),
        "source-bytes"
    );
    assert_eq!(
        fs::read_to_string(elsewhere.join("keep.txt")).unwrap(),
        "other"
    );
}

#[cfg(windows)]
#[test]
fn disable_removes_junction_without_touching_source() {
    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(
        &source.join("demo").join("SKILL.md"),
        &skill_md("D", "must-survive"),
    );
    write_file(&source.join("demo").join("payload.txt"), "source-bytes");
    fs::create_dir_all(&claude).unwrap();
    if create_windows_junction(&claude.join("demo"), &source.join("demo")).is_err() {
        // Environment cannot create junctions — skip rather than false fail.
        return;
    }

    let skill = &svc.list().unwrap()[0];
    assert_eq!(
        skill.state_for(AgentId::Claude),
        Some(SkillSyncState::Linked),
        "junction to source must be Linked, not Conflict"
    );
    let proj = skill.projection_for(AgentId::Claude).unwrap();
    assert_eq!(proj.link_kind, SkillLinkKind::Junction);
    assert!(proj.resolved_target.is_some());

    // Marker proving source content survival after disable.
    let marker = source.join("demo").join("do-not-delete.txt");
    write_file(&marker, "alive");

    svc.disable("demo", AgentId::Claude).unwrap();

    assert!(
        fs::symlink_metadata(claude.join("demo")).is_err(),
        "junction entry must be removed"
    );
    assert!(
        source.join("demo").is_dir(),
        "source skill directory must remain"
    );
    assert_eq!(
        fs::read_to_string(source.join("demo").join("payload.txt")).unwrap(),
        "source-bytes"
    );
    assert_eq!(fs::read_to_string(&marker).unwrap(), "alive");
}

#[cfg(windows)]
#[test]
fn list_classifies_junction_as_linked_not_conflict() {
    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "d"));
    write_file(&source.join("demo").join("nested").join("a.txt"), "a");
    fs::create_dir_all(&claude).unwrap();
    if create_windows_junction(&claude.join("demo"), &source.join("demo")).is_err() {
        return;
    }

    let skill = &svc.list().unwrap()[0];
    let proj = skill.projection_for(AgentId::Claude).unwrap();
    assert_eq!(proj.state, SkillSyncState::Linked);
    assert_eq!(proj.link_kind, SkillLinkKind::Junction);
    assert!(proj.resolved_target.is_some());

    // Ancestor link still forbidden for write path.
    let nested_root = claude.join("wrapped");
    // If skills_root itself became a link, validate_skills_root would reject —
    // here we only assert leaf projection links are allowed on the list path.
    let _ = nested_root;
}

#[cfg(windows)]
#[test]
fn sync_noop_when_already_linked_to_source() {
    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "d"));
    fs::create_dir_all(&claude).unwrap();
    if create_windows_junction(&claude.join("demo"), &source.join("demo")).is_err() {
        return;
    }
    svc.sync("demo", AgentId::Claude, false).unwrap();
    // Still a link, not replaced with a copy.
    let meta = fs::symlink_metadata(claude.join("demo")).unwrap();
    assert!(is_link_or_reparse(&meta));
    assert_eq!(
        fs::read_to_string(source.join("demo").join("SKILL.md")).unwrap(),
        skill_md("D", "d")
    );
}

#[cfg(windows)]
fn create_windows_junction(link: &Path, target: &Path) -> std::io::Result<()> {
    use std::process::Command;
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)?;
    }
    // Target must exist; canonicalize it for a stable junction target.
    let target_s = target
        .canonicalize()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::NotFound, e))?
        .to_string_lossy()
        .to_string();
    // Link path may not exist yet — use the original path string.
    let link_arg = link.to_string_lossy().to_string();
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J", &link_arg, &target_s])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "mklink /J failed",
        ))
    }
}

#[test]
fn sync_and_disable_reject_unsupported_on_disable() {
    let (_tmp, source, _c, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "d"));
    let err = svc.disable("demo", AgentId::Kimi).unwrap_err();
    assert_eq!(err.code(), "unsupported");
}

#[test]
fn sync_rejects_target_file_without_force() {
    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "d"));
    write_file(&claude.join("demo"), "blocking-file");
    let err = svc.sync("demo", AgentId::Claude, false).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert_eq!(
        fs::read_to_string(claude.join("demo")).unwrap(),
        "blocking-file"
    );
    // force also rejects dangerous (non-dir) target rather than clobbering a file.
    let err = svc.sync("demo", AgentId::Claude, true).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert_eq!(
        fs::read_to_string(claude.join("demo")).unwrap(),
        "blocking-file"
    );
}

#[test]
fn validate_skill_id_unit() {
    assert!(validate_skill_id("demo").is_ok());
    assert!(validate_skill_id("my-skill_1").is_ok());
    assert!(validate_skill_id("").is_err());
    assert!(validate_skill_id(".").is_err());
    assert!(validate_skill_id("..").is_err());
    assert!(validate_skill_id("a/b").is_err());
    assert!(validate_skill_id("a\\b").is_err());
    assert!(validate_skill_id("../x").is_err());
    assert!(validate_skill_id("foo:bar").is_err());
    assert!(validate_skill_id("CON").is_err());
    assert!(validate_skill_id("con.txt").is_err());
    assert!(validate_skill_id("LPT1").is_err());
    assert!(validate_skill_id("demo.").is_err());
    assert!(validate_skill_id("demo ").is_err());
    assert!(validate_skill_id("a*b").is_err());
}

#[test]
fn read_skill_markdown_shared_and_private() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&claude).unwrap();
    fs::create_dir_all(&codex).unwrap();
    fs::create_dir_all(&grok).unwrap();

    write_file(
        &source.join("demo").join("SKILL.md"),
        "---\nname: Demo Skill\ndescription: shared demo\n---\n\n# Demo\n\nHello **world**\n",
    );
    write_file(
        &claude.join("private-only").join("SKILL.md"),
        "---\nname: Private\ndescription: agent private\n---\n\n# Private\n",
    );

    let reg = make_registry(claude, codex, grok);
    let svc = SkillService::new(source, reg);

    let shared = svc.read_skill_markdown("demo", None).unwrap();
    assert_eq!(shared.skill_id, "demo");
    assert_eq!(shared.name, "Demo Skill");
    assert!(shared.content.contains("Hello **world**"));
    assert!(!shared.truncated);
    assert!(shared.path.ends_with("SKILL.md"));

    let private = svc
        .read_skill_markdown("private-only", Some(AgentId::Claude))
        .unwrap();
    assert_eq!(private.name, "Private");
    assert!(private.content.contains("# Private"));

    let missing = svc.read_skill_markdown("nope", None).unwrap_err();
    assert_eq!(missing.code(), "not_found");
}

#[test]
fn read_skill_markdown_rejects_unsafe_id() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&claude).unwrap();
    fs::create_dir_all(&codex).unwrap();
    fs::create_dir_all(&grok).unwrap();
    let svc = SkillService::new(source, make_registry(claude, codex, grok));

    for bad in ["", "..", "../x", "a/b", "a\\b", "CON", "foo:bar"] {
        let err = svc.read_skill_markdown(bad, None).unwrap_err();
        assert_eq!(err.code(), "invalid_arg", "id={bad:?} err={err}");
    }
}

#[test]
fn read_skill_markdown_accepts_lowercase_skill_md() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&claude).unwrap();
    fs::create_dir_all(&codex).unwrap();
    fs::create_dir_all(&grok).unwrap();

    write_file(
        &source.join("lower").join("skill.md"),
        "---\nname: Lower\ndescription: d\n---\n\n# lower-case file\n",
    );

    let svc = SkillService::new(source, make_registry(claude, codex, grok));
    let preview = svc.read_skill_markdown("lower", None).unwrap();
    assert_eq!(preview.name, "Lower");
    assert!(preview.content.contains("lower-case file"));
    // Windows FS is case-insensitive: open may surface as SKILL.md or skill.md.
    let file_name = preview
        .path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    assert!(
        file_name.eq_ignore_ascii_case("skill.md"),
        "unexpected path file name: {file_name}"
    );
}

#[test]
fn read_skill_markdown_missing_md_is_not_found() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    fs::create_dir_all(source.join("empty-skill")).unwrap();
    fs::create_dir_all(&claude).unwrap();
    fs::create_dir_all(&codex).unwrap();
    fs::create_dir_all(&grok).unwrap();

    let svc = SkillService::new(source, make_registry(claude, codex, grok));
    let err = svc.read_skill_markdown("empty-skill", None).unwrap_err();
    assert_eq!(err.code(), "not_found");
}

#[test]
fn read_skill_markdown_truncates_large_body() {
    use crate::catalog::limits::SKILL_MARKDOWN_PREVIEW_CHARS;

    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&claude).unwrap();
    fs::create_dir_all(&codex).unwrap();
    fs::create_dir_all(&grok).unwrap();

    // ASCII-only body so byte cap ≈ char cap.
    let mut body = String::from("# Big\n\n");
    body.push_str(&"x".repeat(SKILL_MARKDOWN_PREVIEW_CHARS + 64));
    write_file(&source.join("huge").join("SKILL.md"), &body);

    let svc = SkillService::new(source, make_registry(claude, codex, grok));
    let preview = svc.read_skill_markdown("huge", None).unwrap();
    assert!(preview.truncated);
    assert!(preview.content.chars().count() <= SKILL_MARKDOWN_PREVIEW_CHARS);
    assert!(!preview
        .content
        .contains(&"x".repeat(SKILL_MARKDOWN_PREVIEW_CHARS + 1)));
}

#[test]
fn skill_markdown_preview_serde_camel_case() {
    use crate::models::SkillMarkdownPreview;
    use std::path::PathBuf;

    let row = SkillMarkdownPreview {
        skill_id: "demo".into(),
        name: "Demo".into(),
        path: PathBuf::from("/tmp/demo/SKILL.md"),
        content: "# hi\n".into(),
        truncated: false,
    };
    let v = serde_json::to_value(&row).unwrap();
    assert_eq!(v["skillId"], "demo");
    assert_eq!(v["name"], "Demo");
    assert!(v["path"].as_str().unwrap().contains("SKILL.md"));
    assert_eq!(v["content"], "# hi\n");
    assert_eq!(v["truncated"], false);

    let back: SkillMarkdownPreview = serde_json::from_value(v).unwrap();
    assert_eq!(back, row);
}

// -----------------------------------------------------------------------
// Safety gap fixes: nested target, overlap, rollback, staging cleanup
// -----------------------------------------------------------------------

#[test]
fn replace_rolls_back_when_staging_missing() {
    let tmp = real_tempdir();
    let skills_root = tmp.path().join("skills-root");
    fs::create_dir_all(&skills_root).unwrap();
    let target = skills_root.join("demo");
    write_file(&target.join("keep.txt"), "original-target");
    write_file(&skills_root.join("sibling").join("sib.txt"), "sibling-ok");

    let before = collect_regular_files(&target).unwrap();
    let missing_staging = skills_root.join(".agenthub-stage-demo-missing-does-not-exist");
    assert!(!missing_staging.exists());

    let err = replace_target_with_staging(&skills_root, "demo", &target, &target, &missing_staging)
        .unwrap_err();
    // rename(staging → target) fails after old target was moved aside.
    let _ = err;

    // Old target restored with original contents.
    assert!(target.is_dir());
    let after = collect_regular_files(&target).unwrap();
    assert_eq!(before, after);
    assert_eq!(
        fs::read_to_string(target.join("keep.txt")).unwrap(),
        "original-target"
    );
    // Sibling untouched; no helper dirs left behind.
    assert_eq!(
        fs::read_to_string(skills_root.join("sibling").join("sib.txt")).unwrap(),
        "sibling-ok"
    );
    assert_no_helper_dirs(&skills_root);
    let names = list_dir_names(&skills_root);
    assert!(names.contains(&"demo".to_string()));
    assert!(names.contains(&"sibling".to_string()));
}

#[test]
fn materialize_cleans_staging_on_invalid_relative_map() {
    let tmp = real_tempdir();
    let skills_root = tmp.path().join("skills-root");
    fs::create_dir_all(&skills_root).unwrap();
    write_file(&skills_root.join("sibling").join("ok.txt"), "sib");

    let mut bad = BTreeMap::new();
    bad.insert("../escape.txt".to_string(), b"nope".to_vec());

    let target = skills_root.join("demo");
    let err = materialize_projection(&skills_root, "demo", &target, &bad, None).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(!target.exists());
    assert_no_helper_dirs(&skills_root);
    assert_eq!(
        fs::read_to_string(skills_root.join("sibling").join("ok.txt")).unwrap(),
        "sib"
    );
    // Also reject reserved-name segments injected via the map.
    let mut reserved = BTreeMap::new();
    reserved.insert("nested/CON".to_string(), b"x".to_vec());
    let err = materialize_projection(&skills_root, "demo", &target, &reserved, None).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert_no_helper_dirs(&skills_root);
}

#[test]
fn portable_relative_path_validation_rejects_aliases() {
    assert_eq!(
        normalize_rel_path(Path::new("nested/valid-name.txt")).unwrap(),
        "nested/valid-name.txt"
    );
    assert!(normalize_rel_path(Path::new("nested/CON.txt")).is_err());
    assert!(normalize_rel_path(Path::new("nested/bad:name")).is_err());
    assert!(normalize_rel_path(Path::new("nested/trailing.")).is_err());
    assert!(normalize_rel_path(Path::new("../escape.txt")).is_err());
}

#[test]
fn materialize_missing_branch_preserves_late_conflict() {
    let tmp = real_tempdir();
    let skills_root = tmp.path().join("skills-root");
    fs::create_dir_all(&skills_root).unwrap();
    let target = skills_root.join("demo");
    write_file(&target.join("keep.txt"), "late-conflict");

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

#[cfg(unix)]
#[test]
fn sync_rejects_nonportable_and_case_alias_source_names_unix() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    let source_skill = source.join("demo");
    write_file(&source_skill.join("SKILL.md"), &skill_md("D", "d"));
    write_file(&source_skill.join("bad:name"), "unsafe");
    let err = svc.sync("demo", AgentId::Claude, false).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(!claude.join("demo").exists());

    fs::remove_file(source_skill.join("bad:name")).unwrap();

    // Case-alias rejection needs two distinct directory entries. On
    // case-insensitive volumes (default macOS APFS) `Name.txt` and `name.txt`
    // collapse to one file, so only run this branch when both can coexist.
    write_file(&source_skill.join("Name.txt"), "one");
    let case_sensitive = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(source_skill.join("name.txt"))
        .map(|mut f| {
            use std::io::Write;
            f.write_all(b"two").is_ok()
        })
        .unwrap_or(false);
    if case_sensitive {
        let err = svc.sync("demo", AgentId::Claude, false).unwrap_err();
        assert_eq!(err.code(), "invalid_arg");
        assert!(!claude.join("demo").exists());
        let _ = fs::remove_file(source_skill.join("Name.txt"));
        let _ = fs::remove_file(source_skill.join("name.txt"));
    } else {
        let _ = fs::remove_file(source_skill.join("Name.txt"));
        let _ = fs::remove_file(source_skill.join("name.txt"));
    }

    // Non-UTF8 path components are rejected when the host FS can store them.
    // Default macOS APFS rejects illegal byte sequences, so skip that branch.
    let invalid_utf8 = OsString::from_vec(vec![b'n', b'o', b'n', 0xff]);
    if fs::write(source_skill.join(&invalid_utf8), b"unsafe").is_ok() {
        let err = svc.sync("demo", AgentId::Claude, false).unwrap_err();
        assert_eq!(err.code(), "invalid_arg");
        assert!(!claude.join("demo").exists());
        let _ = fs::remove_file(source_skill.join(invalid_utf8));
    }
}

#[test]
fn same_source_and_skills_root_rejected_preserves_source() {
    let tmp = real_tempdir();
    let shared = tmp.path().join("shared-skills");
    write_file(
        &shared.join("demo").join("SKILL.md"),
        &skill_md("Demo", "must-keep"),
    );
    write_file(&shared.join("demo").join("data.txt"), "canonical-source");

    let mut reg = AdapterRegistry::new();
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Claude,
        supports: true,
        skills_root: Some(shared.clone()),
    }));
    let svc = SkillService::new(shared.clone(), reg);

    let err = svc.sync("demo", AgentId::Claude, true).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    let err = svc.disable("demo", AgentId::Claude).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");

    // Source skill fully preserved.
    assert_eq!(
        fs::read_to_string(shared.join("demo").join("data.txt")).unwrap(),
        "canonical-source"
    );
    assert!(shared.join("demo").join("SKILL.md").is_file());
    assert_no_helper_dirs(&shared);
}

#[cfg(unix)]
#[test]
fn force_sync_rejects_nested_target_symlink_unix() {
    use std::os::unix::fs::symlink;

    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "new"));
    write_file(&source.join("demo").join("a.txt"), "source");

    write_file(&claude.join("demo").join("SKILL.md"), &skill_md("D", "old"));
    write_file(&claude.join("demo").join("a.txt"), "target");
    symlink("a.txt", claude.join("demo").join("nested-link")).unwrap();
    write_file(&claude.join("sibling").join("s.txt"), "sib");

    // Force must still refuse nested symlink with invalid_arg (not replace).
    let err = svc.sync("demo", AgentId::Claude, true).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert_eq!(
        fs::read_to_string(claude.join("demo").join("SKILL.md")).unwrap(),
        skill_md("D", "old")
    );
    assert_eq!(
        fs::read_to_string(claude.join("demo").join("a.txt")).unwrap(),
        "target"
    );
    assert!(
        fs::symlink_metadata(claude.join("demo").join("nested-link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(claude.join("sibling").join("s.txt")).unwrap(),
        "sib"
    );
    assert_no_helper_dirs(&claude);
}

#[cfg(unix)]
#[test]
fn disable_rejects_nested_target_symlink_unix() {
    use std::os::unix::fs::symlink;

    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "d"));
    write_file(&claude.join("demo").join("a.txt"), "keep");
    symlink("a.txt", claude.join("demo").join("link")).unwrap();

    let err = svc.disable("demo", AgentId::Claude).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(claude.join("demo").is_dir());
    assert!(fs::symlink_metadata(claude.join("demo").join("link"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_to_string(claude.join("demo").join("a.txt")).unwrap(),
        "keep"
    );
}

#[cfg(windows)]
#[test]
fn force_sync_rejects_nested_target_symlink_when_creatable() {
    use std::os::windows::fs::symlink_file;

    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "new"));
    write_file(&source.join("demo").join("a.txt"), "source");

    write_file(&claude.join("demo").join("SKILL.md"), &skill_md("D", "old"));
    write_file(&claude.join("demo").join("a.txt"), "target");
    if symlink_file("a.txt", claude.join("demo").join("nested-link")).is_err() {
        return;
    }
    write_file(&claude.join("sibling").join("s.txt"), "sib");

    let before_link = fs::symlink_metadata(claude.join("demo").join("nested-link")).unwrap();
    assert!(before_link.file_type().is_symlink());
    let err = svc.sync("demo", AgentId::Claude, true).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert_eq!(
        fs::read_to_string(claude.join("demo").join("SKILL.md")).unwrap(),
        skill_md("D", "old")
    );
    assert_eq!(
        fs::read_to_string(claude.join("demo").join("a.txt")).unwrap(),
        "target"
    );
    assert!(
        fs::symlink_metadata(claude.join("demo").join("nested-link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(claude.join("sibling").join("s.txt")).unwrap(),
        "sib"
    );
    assert_no_helper_dirs(&claude);
}

#[cfg(windows)]
#[test]
fn disable_rejects_nested_target_symlink_when_creatable() {
    use std::os::windows::fs::symlink_file;

    let (_tmp, source, claude, _x, _g, svc) = setup_write_fixture();
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "d"));
    write_file(&claude.join("demo").join("a.txt"), "keep");
    if symlink_file("a.txt", claude.join("demo").join("link")).is_err() {
        return;
    }

    let err = svc.disable("demo", AgentId::Claude).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(claude.join("demo").is_dir());
    assert!(fs::symlink_metadata(claude.join("demo").join("link"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_to_string(claude.join("demo").join("a.txt")).unwrap(),
        "keep"
    );
}

#[test]
fn skills_root_as_file_is_rejected() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    write_file(&source.join("demo").join("SKILL.md"), &skill_md("D", "d"));
    let skills_file = tmp.path().join("not-a-dir");
    write_file(&skills_file, "x");

    let mut reg = AdapterRegistry::new();
    reg.register(Arc::new(FakeAdapter {
        id: AgentId::Claude,
        supports: true,
        skills_root: Some(skills_file),
    }));
    let svc = SkillService::new(source, reg);
    let err = svc.sync("demo", AgentId::Claude, false).unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
}

// -----------------------------------------------------------------------
// map_status + private import
// -----------------------------------------------------------------------

#[test]
fn shared_skill_is_projectable_with_available_map_status() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    write_file(
        &source.join("demo").join("SKILL.md"),
        &skill_md("Demo", "d"),
    );

    let svc = SkillService::new(source, make_registry(claude, codex, grok));
    let installed = svc.list_installed().unwrap();
    let shared = installed
        .iter()
        .find(|s| s.id == "demo" && s.origin == "shared")
        .expect("shared skill");
    assert!(shared.projectable);
    assert_eq!(shared.map_status, SkillMapStatus::Available);

    let skill = svc
        .list()
        .unwrap()
        .into_iter()
        .find(|s| s.id == "demo")
        .unwrap();
    let kimi = skill.projection_for(AgentId::Kimi).unwrap();
    assert_eq!(kimi.state, SkillSyncState::Unsupported);
    assert_eq!(kimi.map_status, SkillMapStatus::AgentUnsupported);

    let claude_p = skill.projection_for(AgentId::Claude).unwrap();
    assert_eq!(claude_p.state, SkillSyncState::Absent);
    assert_eq!(claude_p.map_status, SkillMapStatus::Available);
}

#[test]
fn private_skill_is_not_projectable_with_private_source() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    fs::create_dir_all(&source).unwrap();
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    write_file(
        &claude.join("hatch-pet").join("SKILL.md"),
        &skill_md("hatch-pet", "private"),
    );

    let svc = SkillService::new(source, make_registry(claude, codex, grok));
    let installed = svc.list_installed().unwrap();
    let private = installed
        .iter()
        .find(|s| s.id == "hatch-pet" && s.origin == "claude")
        .expect("private skill");
    assert!(!private.projectable);
    assert_eq!(private.map_status, SkillMapStatus::PrivateSource);
    assert!(private.projections.is_empty());
}

#[test]
fn import_private_to_shared_copies_without_deleting_private() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    fs::create_dir_all(&source).unwrap();
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    write_file(
        &claude.join("hatch-pet").join("SKILL.md"),
        &skill_md("hatch-pet", "private"),
    );
    write_file(&claude.join("hatch-pet").join("extra.txt"), "keep-me");

    let svc = SkillService::new(source.clone(), make_registry(claude.clone(), codex, grok));
    let skill = svc
        .import_private_to_shared("hatch-pet", AgentId::Claude, false)
        .unwrap();
    assert_eq!(skill.id, "hatch-pet");
    assert!(source.join("hatch-pet").join("SKILL.md").is_file());
    assert_eq!(
        fs::read_to_string(source.join("hatch-pet").join("extra.txt")).unwrap(),
        "keep-me"
    );
    // Original private skill remains.
    assert!(claude.join("hatch-pet").join("SKILL.md").is_file());
    assert_eq!(
        fs::read_to_string(claude.join("hatch-pet").join("extra.txt")).unwrap(),
        "keep-me"
    );

    let installed = svc.list_installed().unwrap();
    let shared = installed
        .iter()
        .find(|s| s.id == "hatch-pet" && s.origin == "shared")
        .expect("imported shared");
    assert!(shared.projectable);
    assert_eq!(shared.map_status, SkillMapStatus::Available);
    // Agent-root copy remains visible as workspace truth, marked already-in-library.
    let agent_row = installed
        .iter()
        .find(|s| s.id == "hatch-pet" && s.origin == "claude")
        .expect("agent workspace row after import");
    assert!(!agent_row.projectable);
    assert_eq!(agent_row.map_status, SkillMapStatus::Available);
}

#[test]
fn agent_skill_same_id_as_shared_is_available_not_private_source() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    let body = skill_md("Demo", "same-bytes");
    write_file(&source.join("demo").join("SKILL.md"), &body);
    // Identical content → already in library (content-hash equal).
    write_file(&claude.join("demo").join("SKILL.md"), &body);
    write_file(
        &claude.join("only-local").join("SKILL.md"),
        &skill_md("only-local", "private"),
    );

    let svc = SkillService::new(source, make_registry(claude, codex, grok));
    let installed = svc.list_installed().unwrap();

    let in_lib = installed
        .iter()
        .find(|s| s.id == "demo" && s.origin == "claude")
        .expect("agent row for shared id");
    assert_eq!(in_lib.map_status, SkillMapStatus::Available);

    let only = installed
        .iter()
        .find(|s| s.id == "only-local" && s.origin == "claude")
        .expect("true private");
    assert_eq!(only.map_status, SkillMapStatus::PrivateSource);
}

#[test]
fn agent_skill_same_id_content_differs_is_conflict() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    write_file(
        &source.join("demo").join("SKILL.md"),
        &skill_md("Demo", "shared-version"),
    );
    write_file(
        &claude.join("demo").join("SKILL.md"),
        &skill_md("Demo", "agent-local-version"),
    );

    let svc = SkillService::new(source, make_registry(claude, codex, grok));
    let installed = svc.list_installed().unwrap();
    let row = installed
        .iter()
        .find(|s| s.id == "demo" && s.origin == "claude")
        .expect("agent row");
    assert_eq!(
        row.map_status,
        SkillMapStatus::Conflict,
        "same id but different content must surface as conflict"
    );
}

#[test]
fn import_private_same_name_without_overwrite_is_conflict() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    write_file(
        &source.join("hatch-pet").join("SKILL.md"),
        &skill_md("shared", "shared"),
    );
    write_file(
        &claude.join("hatch-pet").join("SKILL.md"),
        &skill_md("private", "private"),
    );

    let svc = SkillService::new(source.clone(), make_registry(claude.clone(), codex, grok));
    let err = svc
        .import_private_to_shared("hatch-pet", AgentId::Claude, false)
        .unwrap_err();
    assert_eq!(err.code(), "skill.conflict");
    // Neither side mutated.
    assert_eq!(
        fs::read_to_string(source.join("hatch-pet").join("SKILL.md")).unwrap(),
        skill_md("shared", "shared")
    );
    assert!(claude.join("hatch-pet").join("SKILL.md").is_file());

    // Force overwrite replaces shared content; private remains.
    let skill = svc
        .import_private_to_shared("hatch-pet", AgentId::Claude, true)
        .unwrap();
    assert_eq!(skill.id, "hatch-pet");
    assert!(
        fs::read_to_string(source.join("hatch-pet").join("SKILL.md"))
            .unwrap()
            .contains("private")
    );
    assert!(claude.join("hatch-pet").join("SKILL.md").is_file());
}

#[test]
fn foreign_projection_map_status_is_conflict_not_blocked() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    write_file(&source.join("s").join("a.txt"), "src");
    write_file(&claude.join("s").join("a.txt"), "other");

    let svc = SkillService::new(source, make_registry(claude, codex, grok));
    let skill = &svc.list().unwrap()[0];
    let p = skill.projection_for(AgentId::Claude).unwrap();
    assert_eq!(p.state, SkillSyncState::Foreign);
    assert_eq!(p.map_status, SkillMapStatus::Conflict);
    assert!(p.map_status.is_actionable());
}

// -----------------------------------------------------------------------
// list_catalog
// -----------------------------------------------------------------------

#[test]
fn list_catalog_shared_skill_is_projectable_with_projections() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    write_file(
        &source.join("demo").join("SKILL.md"),
        &skill_md("Demo", "shared"),
    );

    let svc = SkillService::new(source, make_registry(claude, codex, grok));
    let listed = svc.list().unwrap();
    let catalog = svc.list_catalog().unwrap();
    let shared = catalog
        .iter()
        .find(|s| s.id == "demo" && s.origin == "shared")
        .expect("shared catalog row");
    assert!(shared.projectable);
    assert_eq!(shared.map_status, SkillMapStatus::Available);
    assert_eq!(shared.projections.len(), AgentId::ALL.len());
    assert_eq!(shared.projections, listed[0].projections);
}

#[test]
fn list_catalog_claude_private_skill_is_private_source() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    fs::create_dir_all(&source).unwrap();
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    write_file(
        &claude.join("hatch-pet").join("SKILL.md"),
        &skill_md("hatch-pet", "private"),
    );

    let svc = SkillService::new(source, make_registry(claude, codex, grok));
    let catalog = svc.list_catalog().unwrap();
    let private = catalog
        .iter()
        .find(|s| s.id == "hatch-pet" && s.origin == "claude")
        .expect("claude private catalog row");
    assert!(!private.projectable);
    assert_eq!(private.map_status, SkillMapStatus::PrivateSource);
    assert!(private.projections.is_empty());
}

#[test]
fn list_catalog_same_id_in_two_agents_is_two_rows() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    fs::create_dir_all(&source).unwrap();
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    write_file(
        &claude.join("solo").join("SKILL.md"),
        &skill_md("Solo", "claude-copy"),
    );
    write_file(
        &codex.join("solo").join("SKILL.md"),
        &skill_md("Solo", "codex-copy"),
    );

    let svc = SkillService::new(source, make_registry(claude, codex, grok));
    let catalog = svc.list_catalog().unwrap();
    let rows: Vec<_> = catalog.iter().filter(|s| s.id == "solo").collect();
    assert_eq!(rows.len(), 2, "same private id must stay two catalog rows");
    assert!(rows.iter().any(|s| s.origin == "claude"));
    assert!(rows.iter().any(|s| s.origin == "codex"));
    for row in rows {
        assert!(!row.projectable);
        assert_eq!(row.map_status, SkillMapStatus::PrivateSource);
        assert!(row.projections.is_empty());
    }
}

#[test]
fn list_catalog_omits_agent_copy_when_shared_has_same_id() {
    let tmp = real_tempdir();
    let source = tmp.path().join("skills");
    let claude = tmp.path().join("claude-skills");
    let codex = tmp.path().join("codex-skills");
    let grok = tmp.path().join("grok-skills");
    let same = skill_md("Demo", "same-bytes");
    write_file(&source.join("demo").join("SKILL.md"), &same);
    write_file(&claude.join("demo").join("SKILL.md"), &same);
    write_file(
        &source.join("other").join("SKILL.md"),
        &skill_md("Other", "shared-version"),
    );
    write_file(
        &claude.join("other").join("SKILL.md"),
        &skill_md("Other", "agent-local-version"),
    );

    let svc = SkillService::new(source, make_registry(claude, codex, grok));
    let catalog = svc.list_catalog().unwrap();
    assert!(catalog
        .iter()
        .any(|s| s.id == "demo" && s.origin == "shared"));
    assert!(catalog
        .iter()
        .any(|s| s.id == "other" && s.origin == "shared"));
    assert!(
        !catalog
            .iter()
            .any(|s| s.id == "demo" && s.origin == "claude"),
        "identical agent copy must not appear in catalog"
    );
    assert!(
        !catalog
            .iter()
            .any(|s| s.id == "other" && s.origin == "claude"),
        "conflicting agent copy must not appear in catalog"
    );

    // list_installed() still surfaces the Claude workspace rows.
    let installed = svc.list_installed().unwrap();
    let demo_agent = installed
        .iter()
        .find(|s| s.id == "demo" && s.origin == "claude")
        .expect("list_installed still shows Claude demo");
    assert_eq!(demo_agent.map_status, SkillMapStatus::Available);
    let other_agent = installed
        .iter()
        .find(|s| s.id == "other" && s.origin == "claude")
        .expect("list_installed still shows Claude other");
    assert_eq!(other_agent.map_status, SkillMapStatus::Conflict);
}
