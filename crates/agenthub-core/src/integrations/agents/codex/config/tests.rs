use std::collections::BTreeMap;

use super::*;
use crate::platform::config::AgentConfigProjector;
use serde_json::json;

#[test]
fn materialize_official_scaffold_does_not_invent_model_provider() {
    let mut desired = BTreeMap::new();
    desired.insert("model".into(), json!("gpt-5.1-codex"));
    desired.insert("providerSlug".into(), json!(""));
    desired.insert("baseUrl".into(), json!(""));
    desired.insert("wireApi".into(), json!("responses"));
    let base = json!({
        "format": "toml",
        "content": "model = \"gpt-5.1-codex\"\n",
    });
    let raw = CodexConfigProjector
        .materialize_settings_config(Some(&base), &desired)
        .unwrap();
    let content = raw["content"].as_str().unwrap();
    assert!(!content.contains("model_provider"), "{content}");
    assert!(content.contains("gpt-5.1-codex"), "{content}");
}

#[test]
fn official_config_does_not_reactivate_retained_provider_table() {
    let dir = tempfile::tempdir().unwrap();
    let content = "model = \"gpt-5.1-codex\"\n\n[model_providers.old_relay]\nbase_url = \"https://relay.example/v1\"\n";
    std::fs::write(dir.path().join("config.toml"), content).unwrap();

    let read = CodexConfigProjector.read_normalized(dir.path()).unwrap();
    assert_eq!(read.values["providerSlug"], "");
    assert_eq!(read.values["baseUrl"], "");
    assert_eq!(read.values["wireApi"], "");
    assert!(live_import_hint(&json!({ "content": content })).is_none());

    let partial_edit = BTreeMap::from([("model".into(), json!("gpt-5.6"))]);
    let raw = CodexConfigProjector
        .materialize_settings_config(
            Some(&json!({ "format": "toml", "content": content })),
            &partial_edit,
        )
        .unwrap();
    let updated = raw["content"].as_str().unwrap();
    assert!(!updated.contains("model_provider ="), "{updated}");
    assert!(updated.contains("[model_providers.old_relay]"), "{updated}");
    assert!(updated.contains("model = \"gpt-5.6\""), "{updated}");
}

#[test]
fn schema_places_model_after_api_key() {
    let schema = CodexConfigProjector.schema();
    let keys: Vec<&str> = schema
        .fields
        .iter()
        .map(|field| field.key.as_str())
        .collect();
    assert_eq!(
        keys,
        [
            "baseUrl",
            "apiKey",
            "model",
            "reasoningEffort",
            "wireApi",
            "providerSlug",
        ]
    );
}

#[test]
fn live_import_uses_model_provider_instead_of_first_provider() {
    let content = r#"
model_provider = "active"
model = "gpt-5"

[model_providers.inactive]
name = "Inactive"
base_url = "https://api.openai.com/v1"

[model_providers.active]
name = "Active Relay"
base_url = "https://relay.example/v1"
"#;

    let hint = live_import_hint(&json!({
        "format": "toml",
        "content": content,
    }))
    .expect("active provider should be importable");
    assert_eq!(hint.preset, "openai-compat");
    assert!(hint.label.contains("Active Relay"));
}

#[test]
fn live_import_requires_exact_official_hosts() {
    let openai = live_import_hint(&json!({
        "format": "toml",
        "content": "model_provider = \"openai\"\n\n[model_providers.openai]\nbase_url = \"https://api.openai.com/v1\"\n",
    }))
    .unwrap();
    assert_eq!(openai.preset, "openai");

    let openrouter = live_import_hint(&json!({
        "format": "toml",
        "content": "model_provider = \"router\"\n\n[model_providers.router]\nbase_url = \"https://openrouter.ai/api/v1\"\n",
    }))
    .unwrap();
    assert_eq!(openrouter.preset, "openrouter");

    for base_url in [
        "https://api.openai.com.evil.example/v1",
        "https://relay.example/v1 https://api.openai.com/v1",
    ] {
        let hint = live_import_hint(&json!({
            "format": "toml",
            "content": format!(
                "model_provider = \"custom\"\n\n[model_providers.custom]\nbase_url = \"{base_url}\"\n"
            ),
        }))
        .unwrap();
        assert_eq!(hint.preset, "openai-compat", "{base_url}");
    }
}

#[test]
fn ambiguous_multi_provider_toml_is_not_imported_as_the_first_provider() {
    let hint = live_import_hint(&json!({
        "format": "toml",
        "content": "[model_providers.inactive]\nbase_url = \"https://api.openai.com/v1\"\n\n[model_providers.other]\nbase_url = \"https://relay.example/v1\"\n",
    }));
    assert!(hint.is_none());
}
