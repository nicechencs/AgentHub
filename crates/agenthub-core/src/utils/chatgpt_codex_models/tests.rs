use serde_json::json;

use super::{parse_chatgpt_codex_models, CHATGPT_CODEX_MODELS_URL};

#[test]
fn models_url_is_chatgpt_codex_not_platform_openai() {
    assert_eq!(
        CHATGPT_CODEX_MODELS_URL,
        "https://chatgpt.com/backend-api/codex/models"
    );
    assert!(!CHATGPT_CODEX_MODELS_URL.contains("api.openai.com"));
}

#[test]
fn parse_chatgpt_codex_models_reads_slugs() {
    let body = json!({
        "models": [
            { "slug": "gpt-5.6-sol" },
            { "slug": "gpt-5.4" },
            { "slug": "gpt-5.6-sol" },
            { "slug": "  " },
            { "id": "codex-auto-review" },
            "plain-id"
        ]
    });
    assert_eq!(
        parse_chatgpt_codex_models(&body),
        vec![
            "gpt-5.6-sol",
            "gpt-5.4",
            "codex-auto-review",
            "plain-id"
        ]
    );
}
