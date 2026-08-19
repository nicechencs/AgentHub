use super::*;

#[test]
fn agent_hub_open_doctor_has_all_runtimes_and_agents() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hub = AgentHub::open(Some(dir.path())).expect("open hub");
    assert_eq!(hub.data_dir, dir.path());

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
    assert_eq!(hub.registry.all().len(), crate::models::AgentId::ALL.len());
    assert!(report.db_ok);
    assert!(report.ok);
    assert_eq!(report.version, AgentHub::version());
    assert!(report.locks.is_empty());
    // Structure only: do not assert install status (machine-dependent).
    assert_eq!(report.data_dir, dir.path().display().to_string());
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
        crate::models::CapabilityLevel::Unsupported
    );
}

#[test]
fn legacy_repair_facade_keeps_detect_result_in_outcome() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hub = AgentHub::open(Some(dir.path())).expect("open hub");
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
    let hub = AgentHub::open(Some(dir.path())).expect("open hub");
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
