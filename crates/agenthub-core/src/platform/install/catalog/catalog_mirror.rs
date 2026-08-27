//! Lockstep: core catalog vs shared `catalog-mirror-contract.json`.

use super::*;
use crate::models::{AgentId, Capability};
use crate::platform::config::builtin_config_registry;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

const CONTRACT_WATCH: &str =
    include_str!("../../../../../../src/lib/backend/contracts/catalog-mirror-contract.json");

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/lib/backend/contracts/catalog-mirror-contract.json")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogMirror {
    agents: Vec<String>,
    capabilities: Vec<String>,
    capability_labels: BTreeMap<String, String>,
    capability_reasons: CapabilityReasons,
    schema_fields: BTreeMap<String, Vec<String>>,
    channels: CatalogChannels,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapabilityReasons {
    core: BTreeMap<String, BTreeMap<String, Option<String>>>,
    mock: BTreeMap<String, BTreeMap<String, Option<String>>>,
    known_mismatches: Vec<ReasonMismatch>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct ReasonMismatch {
    agent: String,
    capability: String,
}

#[derive(Debug, Deserialize)]
struct CatalogChannels {
    unix: BTreeMap<String, Vec<String>>,
    #[allow(dead_code)]
    windows: BTreeMap<String, Vec<String>>,
}

fn load_contract() -> CatalogMirror {
    let path = contract_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!("read catalog-mirror-contract.json from {}: {err}", path.display())
    });
    let _ = CONTRACT_WATCH;
    serde_json::from_str(&text).expect("catalog-mirror-contract.json")
}

#[test]
fn shared_catalog_fixture_covers_every_agent_and_capability() {
    let contract = load_contract();
    let agents: Vec<&str> = AgentId::ALL.into_iter().map(AgentId::as_str).collect();
    assert_eq!(agents, contract.agents.iter().map(String::as_str).collect::<Vec<_>>());
    let caps: Vec<&str> = Capability::ALL.into_iter().map(Capability::as_str).collect();
    assert_eq!(
        caps,
        contract.capabilities.iter().map(String::as_str).collect::<Vec<_>>()
    );
}

#[test]
fn shared_catalog_fixture_matches_install_channel_ids() {
    let contract = load_contract();
    #[cfg(windows)]
    let expected = &contract.channels.windows;
    #[cfg(not(windows))]
    let expected = &contract.channels.unix;
    for agent in AgentId::ALL {
        let ids: Vec<String> = list_install_catalog()
            .into_iter()
            .find(|row| row.agent_id == agent)
            .unwrap()
            .channels
            .into_iter()
            .map(|ch| ch.id)
            .collect();
        assert_eq!(
            Some(&ids),
            expected.get(agent.as_str()),
            "{}",
            agent.as_str()
        );
    }
}

#[test]
fn shared_catalog_fixture_covers_capability_labels() {
    let contract = load_contract();
    let expected: BTreeMap<String, String> = Capability::ALL
        .into_iter()
        .map(|cap| (cap.as_str().to_string(), cap.label().to_string()))
        .collect();
    assert_eq!(
        contract.capability_labels, expected,
        "capabilityLabels drifted from Capability::label()"
    );
}

#[test]
fn shared_catalog_fixture_covers_capability_reasons() {
    let contract = load_contract();
    let reasons = &contract.capability_reasons;
    let reg = crate::adapters::register_all();
    let mut expected_mismatches = Vec::new();
    assert_eq!(reasons.core.len(), AgentId::ALL.len());
    assert_eq!(reasons.mock.len(), AgentId::ALL.len());
    for agent in AgentId::ALL {
        let core_row = reasons
            .core
            .get(agent.as_str())
            .unwrap_or_else(|| panic!("missing core capabilityReasons for {}", agent.as_str()));
        let mock_row = reasons
            .mock
            .get(agent.as_str())
            .unwrap_or_else(|| panic!("missing mock capabilityReasons for {}", agent.as_str()));
        assert_eq!(
            core_row.len(),
            Capability::ALL.len(),
            "core {}",
            agent.as_str()
        );
        assert_eq!(
            mock_row.len(),
            Capability::ALL.len(),
            "mock {}",
            agent.as_str()
        );
        let adapter = reg.get(agent).expect("adapter");
        for cap in Capability::ALL {
            let key = cap.as_str();
            assert!(
                core_row.contains_key(key),
                "missing core capabilityReasons cell {} {}",
                agent.as_str(),
                key
            );
            assert!(
                mock_row.contains_key(key),
                "missing mock capabilityReasons cell {} {}",
                agent.as_str(),
                key
            );
            let core_reason = core_row.get(key).and_then(Option::as_deref);
            let mock_reason = mock_row.get(key).and_then(Option::as_deref);
            assert_eq!(
                adapter.capability(cap).reason,
                core_reason,
                "core reason drifted {} {}",
                agent.as_str(),
                key
            );
            if core_reason != mock_reason {
                expected_mismatches.push(ReasonMismatch {
                    agent: agent.as_str().to_string(),
                    capability: key.to_string(),
                });
            }
        }
    }
    assert_eq!(
        reasons.known_mismatches, expected_mismatches,
        "knownMismatches drifted from core vs mock capabilityReasons"
    );
}

#[test]
fn shared_catalog_fixture_covers_config_schema_field_names() {
    let contract = load_contract();
    let registry = builtin_config_registry();
    let mut production = BTreeMap::new();
    for key in registry.supported_agent_keys() {
        let fields: Vec<String> = registry
            .get(&key)
            .unwrap_or_else(|| panic!("missing config projector {}", key.as_str()))
            .schema()
            .fields
            .into_iter()
            .map(|field| field.key)
            .collect();
        production.insert(key.as_str().to_string(), fields);
    }
    assert_eq!(
        production, contract.schema_fields,
        "schemaFields drifted from production config projectors"
    );
}
