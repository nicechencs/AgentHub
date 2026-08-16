//! Agent catalog unit tests (production code stays free of #[test] bodies).

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::adapters::{register_all, AdapterRegistry, AgentAdapter};
use crate::models::{AgentId, Capability, CapabilityLevel, CapabilityStateDto, RuntimeId};

use super::{AgentCatalogService, AgentDescriptor, AgentKey, InstallChannelDescriptor};

/// Minimal stub adapter for catalog composition tests (not a production agent).
struct StubAdapter {
    id: AgentId,
}

impl AgentAdapter for StubAdapter {
    fn id(&self) -> AgentId {
        self.id
    }

    fn detect(&self) -> crate::models::DetectResult {
        crate::models::DetectResult {
            agent: self.id,
            status: crate::models::DetectStatus::NotFound,
            version: None,
            binary_path: None,
            channel: None,
            env_ready: false,
            notes: Vec::new(),
        }
    }

    fn install_channels(&self) -> Vec<crate::models::InstallChannel> {
        Vec::new()
    }

    fn read_config(&self) -> crate::error::Result<crate::models::AgentConfig> {
        Err(crate::error::AppError::Unsupported("stub".into()))
    }

    fn read_auth(&self) -> crate::error::Result<crate::models::AuthState> {
        Err(crate::error::AppError::Unsupported("stub".into()))
    }

    fn skills_dir(&self) -> Option<std::path::PathBuf> {
        None
    }

    fn live_backup_paths(&self) -> Vec<std::path::PathBuf> {
        Vec::new()
    }

    fn build_run_spec(
        &self,
        binary: &std::path::Path,
        prompt: &str,
        opts: &crate::models::RunOptions,
    ) -> crate::error::Result<crate::models::RunSpec> {
        Ok(crate::models::RunSpec {
            agent: self.id,
            program: binary.to_path_buf(),
            args: vec![prompt.into()],
            cwd: opts.cwd.clone(),
            env: Vec::new(),
        })
    }

    fn capability(&self, _cap: Capability) -> crate::models::CapabilityState {
        crate::models::CapabilityState::unsupported("stub catalog adapter")
    }
}

#[test]
fn agent_key_accepts_kebab_case() {
    for raw in [
        "claude",
        "codex",
        "workbuddy",
        "claude-code",
        "a",
        "agent2",
        "my-agent-v2",
    ] {
        let key = AgentKey::parse(raw).unwrap_or_else(|e| panic!("expected ok for {raw}: {e}"));
        assert_eq!(key.as_str(), raw);
    }
}

#[test]
fn agent_key_rejects_invalid() {
    for (raw, _hint) in [
        ("", "empty"),
        ("Claude", "uppercase"),
        ("CLAUDE", "all caps"),
        ("claude_code", "underscore"),
        ("claude code", "space"),
        ("-claude", "leading hyphen"),
        ("claude-", "trailing hyphen"),
        ("claude--code", "double hyphen"),
        ("1claude", "leading digit"),
        ("claude.Code", "dot"),
    ] {
        assert!(AgentKey::parse(raw).is_err(), "expected error for {raw:?}");
    }
}

#[test]
fn agent_id_to_key_is_lossless_and_valid() {
    for id in AgentId::ALL {
        let key = AgentKey::from_agent_id(id);
        assert_eq!(key.as_str(), id.as_str());
        // Round-trip parse must succeed for every published AgentId string.
        let again = AgentKey::parse(key.as_str()).expect("AgentId as_str is valid AgentKey");
        assert_eq!(again, key);
    }
}

#[test]
fn builtin_catalog_keys_unique_and_cover_all_agents() {
    let catalog = AgentCatalogService::builtin().expect("builtin catalog");
    assert_eq!(catalog.len(), AgentId::ALL.len());

    let keys: Vec<&str> = catalog.list().iter().map(|d| d.key.as_str()).collect();
    let mut sorted = keys.clone();
    sorted.sort();
    let mut uniq = sorted.clone();
    uniq.dedup();
    assert_eq!(sorted, uniq, "duplicate keys in catalog");

    for id in AgentId::ALL {
        assert!(
            catalog.contains(&AgentKey::from_agent_id(id)),
            "missing {}",
            id.as_str()
        );
    }
}

#[test]
fn list_order_matches_agent_id_all() {
    let catalog = AgentCatalogService::builtin().expect("builtin");
    let expected: Vec<&str> = AgentId::ALL.iter().map(|id| id.as_str()).collect();
    let got: Vec<&str> = catalog.list().iter().map(|d| d.key.as_str()).collect();
    assert_eq!(got, expected, "catalog order must match AgentId::ALL");
}

#[test]
fn descriptor_display_name_matches_agent_id() {
    let catalog = AgentCatalogService::builtin().expect("builtin");
    for id in AgentId::ALL {
        let d = catalog
            .get(&AgentKey::from_agent_id(id))
            .expect("descriptor");
        assert_eq!(d.display_name, id.display_name());
        assert_eq!(d.integration_version, 1);
    }
}

#[test]
fn capabilities_match_registry_matrix() {
    let registry = register_all();
    let catalog = AgentCatalogService::from_registry(&registry).expect("catalog");

    for id in AgentId::ALL {
        let adapter = registry.get(id).expect("adapter");
        let d = catalog
            .get(&AgentKey::from_agent_id(id))
            .expect("descriptor");
        assert_eq!(d.capabilities.len(), Capability::ALL.len());
        for cap in Capability::ALL {
            let expected = adapter.capability(cap);
            let wire = d
                .capabilities
                .get(cap.as_str())
                .unwrap_or_else(|| panic!("{} missing {}", id.as_str(), cap.as_str()));
            assert_eq!(
                wire.level,
                expected.level,
                "{} / {}",
                id.as_str(),
                cap.as_str()
            );
            assert_eq!(
                wire.reason.as_deref(),
                expected.reason,
                "{} / {} reason",
                id.as_str(),
                cap.as_str()
            );
            // Full/Partial must be usable; Planned/Unsupported blocked — catalog is honest.
            if matches!(
                expected.level,
                CapabilityLevel::Full | CapabilityLevel::Partial
            ) {
                assert!(expected.is_usable());
            }
        }
    }
}

#[test]
fn install_channels_non_empty_for_every_agent() {
    let catalog = AgentCatalogService::builtin().expect("builtin");
    for d in catalog.list() {
        assert!(
            !d.install_channels.is_empty(),
            "{} has no install channels",
            d.key.as_str()
        );
        for ch in &d.install_channels {
            assert!(!ch.id.is_empty());
            assert!(!ch.command.is_empty());
        }
    }
}

#[test]
fn get_unknown_key_is_not_found() {
    let catalog = AgentCatalogService::builtin().expect("builtin");
    // Valid format, not registered.
    let key = AgentKey::parse("unknown-demo").expect("format ok");
    let err = catalog.get(&key).expect_err("must not fallback");
    assert_eq!(err.code(), "not_found");
    let msg = err.to_string();
    assert!(
        msg.contains("unknown-demo"),
        "message should keep original key: {msg}"
    );
    assert!(
        msg.contains("unavailable"),
        "unknown key must be marked unavailable: {msg}"
    );
}

#[test]
fn get_str_unknown_open_key_is_unavailable() {
    let catalog = AgentCatalogService::builtin().expect("builtin");
    let err = catalog
        .get_str("future-open-agent")
        .expect_err("open key absent from catalog");
    assert_eq!(err.code(), "not_found");
    let msg = err.to_string();
    assert!(msg.contains("future-open-agent"), "{msg}");
    assert!(msg.contains("unavailable"), "{msg}");
}

#[test]
fn get_str_invalid_format_is_invalid_arg() {
    let catalog = AgentCatalogService::builtin().expect("builtin");
    let err = catalog.get_str("Not_Valid").expect_err("format");
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn descriptor_serde_roundtrip() {
    let catalog = AgentCatalogService::builtin().expect("builtin");
    let original = catalog.list().first().expect("non-empty").clone();
    let json = serde_json::to_string(&original).expect("serialize");
    let back: AgentDescriptor = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, original);
    // Wire uses camelCase keys.
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(v.get("displayName").is_some());
    assert!(v.get("integrationVersion").is_some());
    assert!(v.get("installChannels").is_some());
    assert!(v.get("configSchemaVersion").is_some());
}

#[test]
fn test_only_extra_descriptor_does_not_require_service_changes() {
    // Acceptance: inject a test-only descriptor via `new` without touching from_registry.
    let mut caps = BTreeMap::new();
    caps.insert(
        Capability::Usage.as_str().to_string(),
        CapabilityStateDto {
            level: CapabilityLevel::Unsupported,
            reason: Some("demo".into()),
            min_version: None,
        },
    );
    let demo = AgentDescriptor {
        key: AgentKey::parse("unknown-demo").unwrap(),
        display_name: "Unknown Demo".into(),
        integration_version: 1,
        capabilities: caps,
        install_channels: vec![InstallChannelDescriptor {
            id: "npm".into(),
            label: "npm".into(),
            command: "npm i -g demo".into(),
            requires: vec![RuntimeId::NodeJs],
        }],
        config_schema_version: None,
    };
    let catalog = AgentCatalogService::new(vec![demo.clone()]).expect("catalog");
    assert_eq!(catalog.len(), 1);
    assert_eq!(
        catalog.get_str("unknown-demo").unwrap().display_name,
        "Unknown Demo"
    );
    assert_eq!(catalog.list()[0], demo);
}

#[test]
fn new_rejects_duplicate_keys() {
    let d = AgentDescriptor {
        key: AgentKey::parse("dup-agent").unwrap(),
        display_name: "Dup".into(),
        integration_version: 1,
        capabilities: BTreeMap::new(),
        install_channels: vec![],
        config_schema_version: None,
    };
    let err = AgentCatalogService::new(vec![d.clone(), d]).expect_err("dup");
    assert_eq!(err.code(), "invalid_arg");
}

#[test]
fn from_registry_follows_registration_order_not_agent_id_all() {
    // Partial registry registered in reverse-of-ALL order → catalog must follow
    // registration order and length, proving no AgentId::ALL loop.
    let mut registry = AdapterRegistry::new();
    registry.register(Arc::new(StubAdapter { id: AgentId::Dsh }));
    registry.register(Arc::new(StubAdapter {
        id: AgentId::Claude,
    }));

    let catalog = AgentCatalogService::from_registry(&registry).expect("catalog");
    assert_eq!(catalog.len(), 2, "must not expand to AgentId::ALL");
    assert_ne!(catalog.len(), AgentId::ALL.len());
    let keys: Vec<&str> = catalog.list().iter().map(|d| d.key.as_str()).collect();
    assert_eq!(keys, vec!["dsh", "claude"]);
}

#[test]
fn from_keys_open_key_without_adapter_is_unavailable() {
    let registry = register_all();
    let open = AgentKey::parse("open-fake-agent").expect("valid open key");
    // Open key is not a closed AgentId and has no adapter → fail closed.
    let err = AgentCatalogService::from_keys(&registry, &[open]).expect_err("unavailable");
    assert_eq!(err.code(), "not_found");
    let msg = err.to_string();
    assert!(msg.contains("open-fake-agent"), "{msg}");
    assert!(msg.contains("unavailable"), "{msg}");
}

#[test]
fn from_keys_explicit_order_ignores_agent_id_all() {
    let registry = register_all();
    let keys = [
        AgentKey::parse("cursor").unwrap(),
        AgentKey::parse("claude").unwrap(),
    ];
    let catalog = AgentCatalogService::from_keys(&registry, &keys).expect("catalog");
    assert_eq!(catalog.len(), 2);
    assert_ne!(catalog.len(), AgentId::ALL.len());
    let got: Vec<&str> = catalog.list().iter().map(|d| d.key.as_str()).collect();
    assert_eq!(got, vec!["cursor", "claude"]);
}

#[test]
fn open_key_descriptor_via_new_is_queryable_without_agent_id() {
    // Fake/open key descriptor proves catalog identity is AgentKey-native;
    // no AgentId::ALL membership required.
    let open_key = AgentKey::parse("open-probe").expect("open key");
    assert!(
        AgentId::parse(open_key.as_str()).is_none(),
        "fixture must not be a closed AgentId"
    );
    let demo = AgentDescriptor {
        key: open_key.clone(),
        display_name: "Open Probe".into(),
        integration_version: 1,
        capabilities: BTreeMap::new(),
        install_channels: vec![InstallChannelDescriptor {
            id: "npm".into(),
            label: "npm".into(),
            command: "npm i -g open-probe".into(),
            requires: vec![RuntimeId::NodeJs],
        }],
        config_schema_version: None,
    };
    let catalog = AgentCatalogService::new(vec![demo]).expect("catalog");
    assert_eq!(catalog.len(), 1);
    assert_eq!(
        catalog.get(&open_key).expect("present").display_name,
        "Open Probe"
    );
    // Still unavailable on the production builtin catalog.
    let builtin = AgentCatalogService::builtin().expect("builtin");
    let err = builtin.get(&open_key).expect_err("not in builtin");
    assert_eq!(err.code(), "not_found");
    assert!(err.to_string().contains("unavailable"));
}
