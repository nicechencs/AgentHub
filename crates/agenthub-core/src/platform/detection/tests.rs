use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::adapters::register_all;
use crate::models::{AgentId, DetectStatus};
use crate::platform::detection::{
    builtin_detector_registry, AgentDetector, DetectorRegistry, FnDetector,
};
use crate::platform::lifecycle::InstallationObserved;
use crate::platform::AgentKey;

struct TestDetector {
    key: AgentKey,
    calls: Arc<AtomicUsize>,
}

impl AgentDetector for TestDetector {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn detect(&self) -> InstallationObserved {
        self.calls.fetch_add(1, Ordering::SeqCst);
        InstallationObserved {
            status: DetectStatus::Installed,
            version: Some("1.2.3".into()),
            binary_path: Some("demo-agent".into()),
            channel: Some("test".into()),
            notes: Vec::new(),
        }
    }
}

fn test_detector(key: &str, calls: Arc<AtomicUsize>) -> Arc<dyn AgentDetector> {
    Arc::new(TestDetector {
        key: AgentKey::parse(key).unwrap(),
        calls,
    })
}

#[test]
fn unknown_valid_key_registers_queries_and_detects() {
    let calls = Arc::new(AtomicUsize::new(0));
    let key = AgentKey::parse("demo-agent").unwrap();
    let mut registry = DetectorRegistry::new();
    registry
        .register(test_detector(key.as_str(), Arc::clone(&calls)))
        .unwrap();

    let observed = registry.get(&key).unwrap().detect();
    assert_eq!(observed.status, DetectStatus::Installed);
    assert_eq!(observed.version.as_deref(), Some("1.2.3"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn duplicate_detector_is_rejected_without_overwrite() {
    let first_calls = Arc::new(AtomicUsize::new(0));
    let second_calls = Arc::new(AtomicUsize::new(0));
    let key = AgentKey::parse("demo-agent").unwrap();
    let mut registry = DetectorRegistry::new();
    registry
        .register(test_detector(key.as_str(), Arc::clone(&first_calls)))
        .unwrap();

    let error = registry
        .register(test_detector(key.as_str(), Arc::clone(&second_calls)))
        .unwrap_err();
    assert_eq!(error.code(), "invalid_arg");
    registry.get(&key).unwrap().detect();
    assert_eq!(first_calls.load(Ordering::SeqCst), 1);
    assert_eq!(second_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn builtin_order_and_legacy_lookup_match_adapter_catalog() {
    let registry = builtin_detector_registry();
    let expected: Vec<_> = AgentId::ALL
        .iter()
        .copied()
        .map(AgentKey::from_agent_id)
        .collect();
    assert_eq!(registry.supported_agent_keys(), expected);
    for agent in AgentId::ALL {
        let detector = registry.get_agent_id(agent).expect("built-in detector");
        assert_eq!(detector.agent_key(), AgentKey::from_agent_id(agent));
    }
}

#[test]
fn fn_detector_registers_without_agent_adapter() {
    // P1-3 acceptance: production-shaped detector (FnDetector) registers with no
    // AgentAdapter impl and no AdapterRegistry.
    let key = AgentKey::parse("sparse-probe").unwrap();
    let mut registry = DetectorRegistry::new();
    registry
        .register(Arc::new(FnDetector::new(key.clone(), || InstallationObserved {
            status: DetectStatus::NotFound,
            version: None,
            binary_path: None,
            channel: Some("npm".into()),
            notes: vec!["no adapter".into()],
        })))
        .unwrap();

    let observed = registry.get(&key).unwrap().detect();
    assert_eq!(observed.status, DetectStatus::NotFound);
    assert_eq!(observed.notes, vec!["no adapter".to_string()]);
    assert!(registry.contains_key(&key));
}

#[test]
fn builtin_detectors_cover_eight_agents_without_register_all() {
    // builtin_detector_registry is built from sources::build_registry, not register_all.
    let detectors = builtin_detector_registry();
    assert_eq!(detectors.supported_agent_keys().len(), AgentId::ALL.len());
    for agent in AgentId::ALL {
        let observed = detectors.get_agent_id(agent).unwrap().detect();
        // Status may be Installed or NotFound depending on host; key coverage is the contract.
        assert!(
            matches!(
                observed.status,
                DetectStatus::Installed | DetectStatus::NotFound
            ),
            "{} detect returned unexpected status {:?}",
            agent.as_str(),
            observed.status
        );
    }

    // Parity: free probes match adapter.detect() for the same eight agents.
    let adapters = register_all();
    for agent in AgentId::ALL {
        let via_detector = detectors.get_agent_id(agent).unwrap().detect();
        let via_adapter = adapters.get(agent).unwrap().detect();
        assert_eq!(via_detector.status, via_adapter.status, "{}", agent.as_str());
        assert_eq!(
            via_detector.version, via_adapter.version,
            "{}",
            agent.as_str()
        );
        assert_eq!(
            via_detector.binary_path,
            via_adapter
                .binary_path
                .as_ref()
                .map(|p| p.display().to_string()),
            "{}",
            agent.as_str()
        );
        assert_eq!(
            via_detector.channel, via_adapter.channel,
            "{}",
            agent.as_str()
        );
        assert_eq!(via_detector.notes, via_adapter.notes, "{}", agent.as_str());
    }
}
