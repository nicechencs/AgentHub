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
    // Usage health covers every agent id (supported or not)
    assert_eq!(report.usage_health.len(), crate::models::AgentId::ALL.len());
    assert!(report.usage_health.iter().any(|h| h.supported));
    assert_eq!(hub.registry.all().len(), crate::models::AgentId::ALL.len());
    assert!(report.db_ok);
    assert!(report.ok);
    assert_eq!(report.version, AgentHub::version());
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
