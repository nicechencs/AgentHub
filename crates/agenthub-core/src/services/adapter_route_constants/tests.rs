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
