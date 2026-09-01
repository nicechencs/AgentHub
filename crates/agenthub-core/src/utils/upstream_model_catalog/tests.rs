use serde_json::json;

use super::{catalog_endpoint, embedded_listed_models};

#[test]
fn catalog_endpoint_reads_workbuddy_url_and_key() {
    let blob = json!({
        "api_key": "sk-live",
        "url": "https://api.qooo.io/v1/chat/completions",
        "base_url": "https://api.qooo.io/v1/chat/completions"
    });
    assert_eq!(
        catalog_endpoint(&blob),
        Some((
            "https://api.qooo.io/v1/chat/completions".into(),
            "sk-live".into()
        ))
    );
}

#[test]
fn catalog_endpoint_reads_claude_env() {
    let blob = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://mytokens.cc",
            "ANTHROPIC_AUTH_TOKEN": "sk-ant"
        },
        "model": "claude-sonnet-4"
    });
    assert_eq!(
        catalog_endpoint(&blob),
        Some(("https://mytokens.cc".into(), "sk-ant".into()))
    );
}

#[test]
fn catalog_endpoint_reads_toml_and_skips_loopback() {
    let remote = json!({
        "format": "toml",
        "content": "model = \"deepseek-v4-flash\"\n\n[model_providers.deepseek]\nbase_url = \"https://api.deepseek.com\"\napi_key = \"sk-ds\"\n"
    });
    assert_eq!(
        catalog_endpoint(&remote),
        Some(("https://api.deepseek.com".into(), "sk-ds".into()))
    );
    let local = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "http://127.0.0.1:17034",
            "ANTHROPIC_AUTH_TOKEN": "ahb_local"
        }
    });
    assert_eq!(catalog_endpoint(&local), None);
}

#[test]
fn embedded_listed_models_reads_zcode_and_listed() {
    let blob = json!({
        "listedModels": ["keep-me"],
        "models": { "grok-4.6": { "limit": { "context": 1 } } },
        "model_id": "deepseek-v4-flash",
        "catalog_row": { "id": "grok-4.6" }
    });
    assert_eq!(
        embedded_listed_models(&blob),
        vec!["keep-me", "grok-4.6", "deepseek-v4-flash"]
    );
}
