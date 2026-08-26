//! Lockstep: core catalog vs shared `catalog-mirror-contract.json`.

use super::*;
use crate::models::{AgentId, Capability};
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
struct CatalogMirror {
    agents: Vec<String>,
    capabilities: Vec<String>,
    channels: CatalogChannels,
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
