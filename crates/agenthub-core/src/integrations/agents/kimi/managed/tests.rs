use super::{complete_kimi_live_toml, fill_missing_kimi_provider_type, kimi_provider_type_for_url};
use toml_edit::DocumentMut;

#[test]
fn kimi_provider_type_matches_endpoint_family() {
    assert_eq!(kimi_provider_type_for_url(None), "openai");
    assert_eq!(kimi_provider_type_for_url(Some("")), "openai");
    assert_eq!(
        kimi_provider_type_for_url(Some("https://api.anthropic.com")),
        "openai"
    );
    assert_eq!(
        kimi_provider_type_for_url(Some("https://api.anthropic.com/v1/messages")),
        "anthropic"
    );
    assert_eq!(
        kimi_provider_type_for_url(Some("https://api.z.ai/api/anthropic")),
        "anthropic"
    );
    assert_eq!(
        kimi_provider_type_for_url(Some("http://127.0.0.1:43121/v1/responses")),
        "openai_responses"
    );
    assert_eq!(
        kimi_provider_type_for_url(Some("https://api.moonshot.cn/v1")),
        "kimi"
    );
    assert_eq!(
        kimi_provider_type_for_url(Some("https://api.moonshot.ai/v1")),
        "kimi"
    );
    assert_eq!(
        kimi_provider_type_for_url(Some("https://api.kimi.com/coding/v1")),
        "openai"
    );
    assert_eq!(
        kimi_provider_type_for_url(Some("https://mytokens.cc/v1")),
        "openai"
    );
    assert_eq!(
        kimi_provider_type_for_url(Some("http://127.0.0.1:43121/v1")),
        "openai"
    );
}

#[test]
fn complete_live_toml_fills_type_from_base_url_and_keeps_existing() {
    let mut doc: DocumentMut = r#"
default_model = "kimi-k2"
default_provider = "moonshot"

[providers.moonshot]
base_url = "https://api.moonshot.cn/v1"
api_key = "sk-x"
"#
    .parse()
    .unwrap();
    complete_kimi_live_toml(&mut doc).unwrap();
    assert_eq!(doc["providers"]["moonshot"]["type"].as_str(), Some("kimi"));

    let mut kept: DocumentMut = r#"
default_provider = "custom"
[providers.custom]
type = "openai"
base_url = "https://api.moonshot.cn/v1"
api_key = "sk-x"
"#
    .parse()
    .unwrap();
    fill_missing_kimi_provider_type(&mut kept, "custom").unwrap();
    assert_eq!(
        kept["providers"]["custom"]["type"].as_str(),
        Some("openai"),
        "must not overwrite an explicit type"
    );
}
