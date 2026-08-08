//! Install contribution registry tests (separate from production modules).

use std::sync::Arc;

use crate::models::AgentId;
use crate::platform::install::{
    builtin_install_registry, InstallContribution, InstallContributionRegistry,
};
use crate::platform::AgentKey;

struct TestContribution {
    key: AgentKey,
    package: &'static str,
}

impl InstallContribution for TestContribution {
    fn agent_key(&self) -> AgentKey {
        self.key.clone()
    }

    fn npm_package(&self) -> Option<&'static str> {
        Some(self.package)
    }
}

fn test_contribution(key: &str, package: &'static str) -> Arc<dyn InstallContribution> {
    Arc::new(TestContribution {
        key: AgentKey::parse(key).unwrap(),
        package,
    })
}

#[test]
fn every_agent_has_install_contribution() {
    let reg = builtin_install_registry();
    for id in AgentId::ALL {
        assert!(reg.contains(id), "missing install contrib for {id:?}");
        let c = reg.get_agent_id(id).unwrap();
        let has_plan = c.npm_package().is_some()
            || c.native_ps1_url().is_some()
            || c.native_sh_url().is_some()
            || c.native_setup_url().is_some();
        assert!(has_plan, "{id:?} has no install channel material");
    }
}

#[test]
fn pi_npm_extra_flags_and_codex_order() {
    let reg = builtin_install_registry();
    assert_eq!(
        reg.get_agent_id(AgentId::Pi)
            .unwrap()
            .npm_install_extra_flags(),
        &["--ignore-scripts"]
    );
    assert!(reg
        .get_agent_id(AgentId::Codex)
        .unwrap()
        .prefer_npm_channel_first());
    assert!(!reg
        .get_agent_id(AgentId::Claude)
        .unwrap()
        .prefer_npm_channel_first());
}

#[test]
fn unknown_valid_key_registers_queries_and_serves_spec() {
    let key = AgentKey::parse("demo-agent").unwrap();
    let mut registry = InstallContributionRegistry::new();
    registry
        .register(test_contribution(key.as_str(), "@agenthub/demo-agent"))
        .unwrap();

    let contribution = registry.get(&key).expect("test contribution");
    assert_eq!(contribution.agent_key(), key);
    assert_eq!(contribution.npm_package(), Some("@agenthub/demo-agent"));
}

#[test]
fn duplicate_contribution_is_rejected_without_overwrite() {
    let key = AgentKey::parse("demo-agent").unwrap();
    let mut registry = InstallContributionRegistry::new();
    registry
        .register(test_contribution(key.as_str(), "first-package"))
        .unwrap();

    let error = registry
        .register(test_contribution(key.as_str(), "second-package"))
        .unwrap_err();
    assert_eq!(error.code(), "invalid_arg");
    assert_eq!(
        registry.get(&key).unwrap().npm_package(),
        Some("first-package")
    );
}

#[test]
fn builtin_order_and_legacy_lookup_do_not_drift() {
    let registry = builtin_install_registry();
    let expected: Vec<_> = AgentId::ALL
        .iter()
        .copied()
        .map(AgentKey::from_agent_id)
        .collect();
    assert_eq!(registry.supported_agent_keys(), expected);

    for agent in AgentId::ALL {
        let contribution = registry.get_agent_id(agent).expect("built-in contribution");
        assert_eq!(contribution.agent_key(), AgentKey::from_agent_id(agent));
    }
}
