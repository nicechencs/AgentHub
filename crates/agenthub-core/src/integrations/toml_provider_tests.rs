//! Provider-managed TOML keys must cover every native key the projector writes.

use crate::integrations::agents::{codex, grok, kimi};
use crate::integrations::shared::toml_provider::managed_toml_provider_keys;
use crate::models::AgentId;

fn assert_projector_keys_subset(agent: &str, managed: &[&str], projector: &[&str]) {
    for key in projector {
        assert!(
            managed.contains(key),
            "{agent}: projector TOML key `{key}` is not in provider managed keys {managed:?}"
        );
    }
}

#[test]
fn toml_projector_native_keys_are_subset_of_provider_managed_keys() {
    assert_projector_keys_subset(
        "codex",
        codex::managed::PROVIDER_TOML_KEYS,
        codex::managed::PROJECTOR_TOML_KEYS,
    );
    assert_projector_keys_subset(
        "kimi",
        kimi::managed::PROVIDER_TOML_KEYS,
        kimi::managed::PROJECTOR_TOML_KEYS,
    );
    assert_projector_keys_subset(
        "grok",
        grok::managed::PROVIDER_TOML_KEYS,
        grok::managed::PROJECTOR_TOML_KEYS,
    );
}

#[test]
fn write_toml_config_reads_the_same_managed_lists() {
    assert_eq!(
        managed_toml_provider_keys(AgentId::Codex).unwrap(),
        codex::managed::PROVIDER_TOML_KEYS
    );
    assert_eq!(
        managed_toml_provider_keys(AgentId::Kimi).unwrap(),
        kimi::managed::PROVIDER_TOML_KEYS
    );
    assert_eq!(
        managed_toml_provider_keys(AgentId::Grok).unwrap(),
        grok::managed::PROVIDER_TOML_KEYS
    );
    assert!(managed_toml_provider_keys(AgentId::Claude).is_err());
    assert!(
        kimi::managed::PROVIDER_TOML_KEYS.contains(&"models"),
        "kimi provider switch must replace [models]"
    );
    assert!(
        kimi::managed::PROJECTOR_TOML_KEYS.contains(&"models"),
        "kimi projector must write [models]"
    );
}

#[test]
fn shared_codex_auth_writer_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("auth.json");
    crate::integrations::agents::codex::write_api_key_auth(&path, "sk-shared").unwrap();
    let auth: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(auth["OPENAI_API_KEY"], "sk-shared");
}
