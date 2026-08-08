use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::models::{AgentId, DetectStatus};
use crate::platform::detection::{builtin_detector_registry, AgentDetector, DetectorRegistry};
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
