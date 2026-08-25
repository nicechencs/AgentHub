use super::*;
use serde_json::json;

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
