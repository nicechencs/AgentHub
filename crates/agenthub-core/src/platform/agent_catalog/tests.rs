//! Agent catalog unit tests (production code stays free of #[test] bodies).

use std::collections::BTreeMap;

use crate::adapters::register_all;
use crate::models::{AgentId, Capability, CapabilityLevel, CapabilityStateDto, RuntimeId};

use super::{AgentCatalogService, AgentDescriptor, AgentKey, InstallChannelDescriptor};

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
    assert!(
        err.to_string().contains("unknown-demo"),
        "message should keep original key: {err}"
    );
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
