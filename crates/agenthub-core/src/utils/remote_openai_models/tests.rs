use serde_json::json;

use super::{
    api_endpoint_url, list_remote_openai_models, openai_models_url, openai_models_urls,
    parse_openai_model_list, ApiEndpointType,
};
use crate::error::AppError;

#[test]
fn openai_models_url_normalizes_trailing_slash_and_v1() {
    assert_eq!(
        openai_models_url("https://mytokens.cc"),
        "https://mytokens.cc/v1/models"
    );
    assert_eq!(
        openai_models_url("https://mytokens.cc/"),
        "https://mytokens.cc/v1/models"
    );
    assert_eq!(
        openai_models_url("https://mytokens.cc/v1"),
        "https://mytokens.cc/v1/models"
    );
    assert_eq!(
        openai_models_url("https://mytokens.cc/v1/"),
        "https://mytokens.cc/v1/models"
    );
    assert_eq!(
        openai_models_url("https://openrouter.ai/api/v1"),
        "https://openrouter.ai/api/v1/models"
    );
    assert_eq!(openai_models_url(""), "");
    assert_eq!(openai_models_url("   "), "");
    assert_eq!(
        openai_models_url("https://mytokens.cc/V1/"),
        "https://mytokens.cc/V1/models"
    );
    assert_eq!(
        openai_models_url("https://api.deepseek.com"),
        "https://api.deepseek.com/models"
    );
    assert_eq!(
        openai_models_url("https://api.deepseek.com/anthropic"),
        "https://api.deepseek.com/models"
    );
}

#[test]
fn openai_models_urls_adds_host_v1_and_root_models() {
    assert_eq!(
        openai_models_urls("https://mytokens.cc"),
        vec![
            "https://mytokens.cc/v1/models",
            "https://mytokens.cc/models",
        ]
    );
    assert_eq!(
        openai_models_urls("https://api.qooo.io/v1/chat/completions"),
        vec![
            "https://api.qooo.io/v1/chat/completions/v1/models",
            "https://api.qooo.io/v1/models",
            "https://api.qooo.io/models",
        ]
    );
    assert_eq!(
        openai_models_urls("https://api.deepseek.com"),
        vec![
            "https://api.deepseek.com/models",
            "https://api.deepseek.com/v1/models",
        ]
    );
    assert!(openai_models_urls("").is_empty());
}

#[test]
fn api_endpoint_url_normalizes_base_and_v1() {
    assert_eq!(
        api_endpoint_url("https://api.example.com", ApiEndpointType::Responses),
        "https://api.example.com/v1/responses"
    );
    assert_eq!(
        api_endpoint_url(
            "https://api.example.com/v1/",
            ApiEndpointType::ChatCompletions
        ),
        "https://api.example.com/v1/chat/completions"
    );
    assert_eq!(
        api_endpoint_url("https://api.anthropic.com", ApiEndpointType::Messages),
        "https://api.anthropic.com/v1/messages"
    );
}

#[test]
fn parse_openai_model_list_shapes_dedupe_and_blanks() {
    assert_eq!(
        parse_openai_model_list(&json!({
            "data": [{ "id": "gpt-4" }, { "id": "gpt-4o-mini" }]
        })),
        vec!["gpt-4", "gpt-4o-mini"]
    );
    assert_eq!(
        parse_openai_model_list(&json!({ "data": ["a", "b"] })),
        vec!["a", "b"]
    );
    assert_eq!(
        parse_openai_model_list(&json!({ "models": ["m1", "m2"] })),
        vec!["m1", "m2"]
    );
    assert_eq!(
        parse_openai_model_list(&json!({ "models": [{ "id": "x" }, { "id": "y" }] })),
        vec!["x", "y"]
    );
    assert_eq!(
        parse_openai_model_list(&json!(["one", { "id": "two" }])),
        vec!["one", "two"]
    );
    assert_eq!(
        parse_openai_model_list(&json!({
            "data": [
                { "id": "keep" },
                { "id": "keep" },
                { "id": "  " },
                { "name": "no-id" },
                { "id": "next" }
            ]
        })),
        vec!["keep", "next"]
    );
    assert!(parse_openai_model_list(&json!(null)).is_empty());
    assert!(parse_openai_model_list(&json!({})).is_empty());
}

#[test]
fn list_remote_openai_models_rejects_bad_args_without_network() {
    match list_remote_openai_models("", "sk-test-abcdefgh") {
        Err(AppError::InvalidArg(msg)) => assert!(msg.contains("base URL")),
        other => panic!("expected InvalidArg, got {other:?}"),
    }
    match list_remote_openai_models("ftp://example.com", "sk-test-abcdefgh") {
        Err(AppError::InvalidArg(msg)) => assert!(msg.contains("http")),
        other => panic!("expected InvalidArg, got {other:?}"),
    }
    match list_remote_openai_models("https://example.com", "") {
        Err(AppError::InvalidArg(msg)) => {
            assert!(msg.contains("API key"));
            assert!(!msg.contains("sk-"));
        }
        other => panic!("expected InvalidArg, got {other:?}"),
    }
}
