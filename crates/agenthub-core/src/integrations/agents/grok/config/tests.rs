use std::collections::BTreeMap;

use serde_json::json;
use tempfile::tempdir;
use toml_edit::{DocumentMut, Item};

use super::*;
use crate::platform::config::AgentConfigProjector;

const MULTI_ALIAS: &str = r#"[models]
default = "grok-4.7"
web_search = "grok-4.7"

[auth]
preferred_method = "api_key"

[model_providers.proxy]
base_url = "https://old.example/v1"
api_backend = "responses"

[model_providers.other]
base_url = "https://other.example/v1"

[model."grok-4.7"]
model = "grok-4.7"
name = "Grok 4.7"
description = "Primary"
model_provider = "proxy"
api_key = "***"
context_window = 500000
reasoning_summary = "concise"

[model."grok-4.6"]
model = "grok-4.6"
name = "Grok 4.6"
description = "Fast"
model_provider = "proxy"
api_key = "***"
env_key = "STALE_SIBLING_ENV"
context_window = 500000

[model."other"]
model = "other-model"
name = "Other"
model_provider = "other"
api_key = "other-secret"
"#;

fn desired(api_key: &str) -> BTreeMap<String, serde_json::Value> {
    BTreeMap::from([
        ("model".into(), json!("grok-4.7")),
        ("baseUrl".into(), json!("https://new.example/v1")),
        ("apiKey".into(), json!(api_key)),
        ("apiBackend".into(), json!("responses")),
    ])
}

fn table<'a>(doc: &'a DocumentMut, root: &str, key: &str) -> &'a toml_edit::Table {
    doc.get(root)
        .and_then(Item::as_table)
        .and_then(|root| root.get(key))
        .and_then(Item::as_table)
        .expect("expected TOML table")
}

#[test]
fn apply_keeps_env_key_when_no_inline_api_key_is_written() {
    let dir = tempdir().unwrap();
    let source = r#"[models]
default = "grok"

[model."grok"]
model = "grok-4.5"
model_provider = "proxy"
env_key = "RELAY_KEY"

[model_providers.proxy]
base_url = "https://relay.example/v1"
"#;
    std::fs::write(dir.path().join("config.toml"), source).unwrap();
    let mut values = desired(crate::platform::config::SECRET_REDACTED);
    values.insert("model".into(), json!("grok-4.5"));
    GrokConfigProjector
        .apply(dir.path(), &values)
        .expect("apply config");
    let doc = std::fs::read_to_string(dir.path().join("config.toml"))
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap();
    assert_eq!(
        table(&doc, "model", "grok")["env_key"].as_str(),
        Some("RELAY_KEY")
    );
    assert!(table(&doc, "model", "grok").get("api_key").is_none());
}

#[test]
fn reads_active_provider_url_and_backend() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("config.toml"), MULTI_ALIAS).unwrap();

    let read = GrokConfigProjector
        .read_normalized(dir.path())
        .expect("read config");
    assert_eq!(read.values["model"], "grok-4.7");
    assert_eq!(read.values["baseUrl"], "https://old.example/v1");
    assert_eq!(read.values["apiBackend"], "responses");
    assert_eq!(
        read.values["apiKey"],
        crate::platform::config::SECRET_REDACTED
    );
}

#[test]
fn materialize_migrates_provider_fields_and_restores_same_provider_markers() {
    let materialized = GrokConfigProjector
        .materialize_settings_config(
            Some(&json!({"format": "toml", "content": MULTI_ALIAS})),
            &desired("xai-new-key"),
        )
        .expect("materialize config");
    let doc = materialized["content"]
        .as_str()
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap();

    assert_eq!(
        table(&doc, "model_providers", "proxy")["base_url"].as_str(),
        Some("https://new.example/v1")
    );
    assert_eq!(
        table(&doc, "model", "grok-4.7")["api_key"].as_str(),
        Some("xai-new-key")
    );
    assert_eq!(
        table(&doc, "model", "grok-4.6")["api_key"].as_str(),
        Some("xai-new-key")
    );
    assert!(table(&doc, "model", "grok-4.6").get("env_key").is_none());
    assert_eq!(
        table(&doc, "model", "other")["api_key"].as_str(),
        Some("other-secret")
    );
    assert_eq!(
        table(&doc, "model", "grok-4.7")["name"].as_str(),
        Some("Grok 4.7")
    );
    assert!(table(&doc, "model", "grok-4.7").get("base_url").is_none());
    assert!(table(&doc, "model", "grok-4.7").get("env_key").is_none());
    assert_eq!(doc["auth"]["preferred_method"].as_str(), Some("api_key"));
}

#[test]
fn apply_migrates_legacy_inline_config_and_creates_proxy_for_empty_home() {
    let dir = tempdir().unwrap();
    let legacy = r#"model = "grok-4.5"
base_url = "https://legacy.example/v1"
api_key = "legacy-key"
api_backend = "responses"
"#;
    std::fs::write(dir.path().join("config.toml"), legacy).unwrap();

    let projector = GrokConfigProjector;
    let mut apply_values = desired(crate::platform::config::SECRET_REDACTED);
    apply_values.insert("baseUrl".into(), json!("https://migrated.example/v1"));
    projector
        .apply(dir.path(), &apply_values)
        .expect("apply config");
    let doc = std::fs::read_to_string(dir.path().join("config.toml"))
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap();
    assert_eq!(doc["models"]["default"].as_str(), Some("grok"));
    assert_eq!(
        table(&doc, "model", "grok")["model_provider"].as_str(),
        Some("proxy")
    );
    assert_eq!(
        table(&doc, "model_providers", "proxy")["base_url"].as_str(),
        Some("https://migrated.example/v1")
    );
    assert_eq!(
        table(&doc, "model", "grok")["api_key"].as_str(),
        Some("legacy-key")
    );
    assert!(table(&doc, "model", "grok").get("base_url").is_none());

    let empty = tempdir().unwrap();
    let mut fresh = desired("xai-fresh-key");
    fresh.insert("model".into(), json!("grok-4.7"));
    projector
        .apply(empty.path(), &fresh)
        .expect("apply fresh config");
    let doc = std::fs::read_to_string(empty.path().join("config.toml"))
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap();
    assert_eq!(
        table(&doc, "model", "grok")["model_provider"].as_str(),
        Some("proxy")
    );
    assert_eq!(
        table(&doc, "model_providers", "proxy")["base_url"].as_str(),
        Some("https://new.example/v1")
    );

    let redacted = projector
        .materialize_settings_config(None, &desired(crate::platform::config::SECRET_REDACTED))
        .expect("materialize redacted config");
    assert!(!redacted["content"]
        .as_str()
        .unwrap()
        .contains("api_key = \"***\""));
}
