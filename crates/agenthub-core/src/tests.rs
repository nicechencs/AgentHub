use super::*;
use std::fs;
use std::path::{Path, PathBuf};

/// Open a hub whose skills source is under `dir`, never `~/.agents/skills`.
fn open_isolated_hub(dir: &Path) -> AgentHub {
    let skills = dir.join("skills");
    fs::create_dir_all(&skills).expect("isolated skills root");
    AgentHub::open_with_skills_root(Some(dir), Some(&skills)).expect("open hub")
}

#[test]
fn agent_hub_open_doctor_has_all_runtimes_and_agents() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hub = open_isolated_hub(dir.path());
    let expected_data_dir =
        crate::utils::paths::normalize_data_dir(dir.path()).expect("normalized temp data dir");
    assert_eq!(hub.data_dir(), expected_data_dir.as_path());

    let report = hub.doctor();
    assert_eq!(
        report.runtimes.len(),
        crate::runtime::host_runtimes().len(),
        "doctor runtimes must match host-relevant set (no PowerShell on macOS/Linux)"
    );
    assert!(
        report
            .runtimes
            .iter()
            .all(|r| r.id != crate::models::RuntimeId::PowerShell || cfg!(windows)),
        "PowerShell must not appear in doctor runtimes on non-Windows hosts"
    );
    assert_eq!(report.agents.len(), crate::models::AgentId::ALL.len());
    // Parser health matches Dashboard 解析: installed && !hidden only.
    assert!(
        report.usage_health.iter().all(|h| {
            report.agents.iter().any(|a| {
                a.agent == h.agent_id && a.status == crate::models::DetectStatus::Installed
            })
        }),
        "parser health must omit uninstalled agents"
    );
    assert_eq!(
        hub.registry().all().len(),
        crate::models::AgentId::ALL.len()
    );
    assert!(report.db_ok);
    assert!(report.ok);
    assert_eq!(report.version, AgentHub::version());
    assert!(report.locks.is_empty());
    // Structure only: do not assert install status (machine-dependent).
    assert_eq!(report.data_dir, expected_data_dir.display().to_string());
    assert_eq!(report.capabilities.len(), crate::models::AgentId::ALL.len());
    for agent in crate::models::AgentId::ALL {
        let row = report
            .capabilities
            .get(&agent)
            .unwrap_or_else(|| panic!("missing capabilities for {}", agent.as_str()));
        assert_eq!(row.len(), crate::models::Capability::ALL.len());
    }
    assert_eq!(
        report.capabilities[&crate::models::AgentId::Kimi][&crate::models::Capability::Skills]
            .level,
        crate::models::CapabilityLevel::Partial
    );
}

#[test]
fn agent_hub_open_freezes_relative_data_dir_before_lifecycle_use() {
    let cwd = std::env::current_dir().expect("current directory");
    let dir = tempfile::tempdir_in(&cwd).expect("relative data-dir fixture");
    let relative = Path::new(dir.path().file_name().expect("temp directory has a name"));

    let skills = dir.path().join("skills");
    fs::create_dir_all(&skills).expect("isolated skills root");
    let hub = AgentHub::open_with_skills_root(Some(relative), Some(&skills))
        .expect("open relative data dir");
    assert!(hub.data_dir().is_absolute());
    assert_eq!(
        hub.data_dir(),
        crate::utils::paths::normalize_data_dir(relative).expect("normalize relative data dir")
    );
}

#[test]
fn legacy_repair_facade_keeps_detect_result_in_outcome() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hub = open_isolated_hub(dir.path());
    let outcome = hub
        // Lifecycle concurrency tests intentionally hold Claude's global
        // process lock; use an otherwise idle built-in to avoid cross-test races.
        .repair_agent_detect(AgentId::WorkBuddy)
        .expect("legacy repair");
    assert_eq!(
        outcome.agent.as_ref().map(|detect| detect.agent),
        Some(AgentId::WorkBuddy)
    );
}

#[test]
fn parser_health_omits_hidden_installed_agents() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hub = open_isolated_hub(dir.path());
    let installed: Vec<_> = hub
        .agents
        .detect_all()
        .into_iter()
        .filter(|row| row.status == models::DetectStatus::Installed)
        .map(|row| row.agent)
        .collect();

    let before = hub.usage.parser_health().expect("parser_health");
    assert!(
        before.iter().all(|h| installed.contains(&h.agent_id)),
        "parser health must only include installed agents: {before:?}"
    );
    for id in &installed {
        hub.agent_visibility
            .set_agent_hidden(*id, true)
            .expect("hide");
    }
    let hidden = hub.usage.parser_health().expect("parser_health hidden");
    assert!(
        hidden.is_empty(),
        "hidden installed agents must not appear: {hidden:?}"
    );

    // Cursor can be installed but has no usage source — unhide a collectable agent.
    if let Some(id) = before.first().map(|h| h.agent_id) {
        hub.agent_visibility
            .set_agent_hidden(id, false)
            .expect("unhide");
        let shown = hub.usage.parser_health().expect("parser_health shown");
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].agent_id, id);
    }
}

#[test]
fn usage_cache_db_can_be_deleted_without_breaking_open() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cache_path;
    {
        let hub = open_isolated_hub(dir.path());
        cache_path = crate::utils::paths::cache_db_path(hub.data_dir());
        assert!(cache_path.exists(), "usage cache file should be created");
        let usage_on_main = hub
            .db()
            .with_conn(|conn| {
                let n: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'usage_records'",
                    [],
                    |row| row.get(0),
                )?;
                Ok(n)
            })
            .expect("sqlite_master");
        assert_eq!(
            usage_on_main, 0,
            "usage tables must not stay on the product db"
        );
        assert!(hub.usage.parser_health().is_ok());
        assert!(hub.doctor().db_ok);
    }
    let _ = fs::remove_file(&cache_path);
    for extra in ["-wal", "-shm", "-journal"] {
        let mut name = cache_path.as_os_str().to_os_string();
        name.push(extra);
        let _ = fs::remove_file(PathBuf::from(name));
    }
    let hub = open_isolated_hub(dir.path());
    assert!(hub.doctor().db_ok);
    assert!(hub.usage.parser_health().is_ok());
    assert!(hub.usage.connection_usage_summaries().is_empty());
}
