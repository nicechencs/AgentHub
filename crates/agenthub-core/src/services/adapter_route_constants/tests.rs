use super::*;
use serde_json::json;

#[test]
fn openrouter_url_and_openai_compat_preset_classify_as_openai_api() {
    assert!(is_openai_api_marker(
        None,
        &json!({ "baseURL": "https://openrouter.ai/api/v1", "apiKey": "test-key" }),
    ));
    assert!(is_custom_openai_compat(
        None,
        &json!({ "baseURL": "https://openrouter.ai/api/v1", "apiKey": "test-key" }),
    ));
    assert!(is_openai_api_marker(Some("openai-compat"), &json!({})));
    assert!(is_custom_openai_compat(Some("openai-compat"), &json!({})));
    assert!(is_openai_api_marker(
        Some("openai"),
        &json!({ "baseURL": "https://relay.example.com/v1", "apiKey": "test-key" }),
    ));
    assert!(is_custom_openai_compat(
        Some("openai"),
        &json!({ "baseURL": "https://relay.example.com/v1", "apiKey": "test-key" }),
    ));
}

#[test]
fn official_openai_host_is_not_custom() {
    assert!(is_openai_api_marker(
        Some("openai"),
        &json!({ "baseURL": "https://api.openai.com/v1" }),
    ));
    assert!(!is_custom_openai_compat(
        Some("openai"),
        &json!({ "baseURL": "https://api.openai.com/v1" }),
    ));
    assert!(!is_custom_openai_compat_url("https://api.openai.com/v1"));
    assert!(is_custom_openai_compat_url("https://openrouter.ai/api/v1"));
}

#[test]
fn other_vendor_urls_are_not_openai_compat() {
    assert!(!is_openai_api_marker(
        None,
        &json!({ "baseURL": "https://api.anthropic.com/v1", "apiKey": "test-key" }),
    ));
    assert!(!is_openai_api_marker(
        None,
        &json!({ "baseURL": "https://api.deepseek.com/v1", "apiKey": "test-key" }),
    ));
}

#[test]
fn mytokens_toml_custom_remote_classifies_as_openai_api() {
    let blob = json!({
        "format": "toml",
        "content": "model_provider = \"OpenAI\"\nmodel = \"gpt-5.5\"\n\n[model_providers.OpenAI]\nname = \"OpenAI\"\nbase_url = \"https://mytokens.cc/v1\"\n"
    });
    assert!(is_openai_api_marker(Some("openai-compatible"), &blob));
    assert!(settings_contain_custom_openai_compat_remote(&blob));
    assert!(!is_openai_api_marker(
        Some("openai-compatible"),
        &json!({"api_key": "must-not-leak"}),
    ));
    assert!(!settings_contain_custom_openai_compat_remote(&json!({
        "format": "toml",
        "content": "model_provider = \"agenthub_claude_bridge\"\n\n[model_providers.agenthub_claude_bridge]\nbase_url = \"http://127.0.0.1:33923/v1\"\n"
    })));
}
