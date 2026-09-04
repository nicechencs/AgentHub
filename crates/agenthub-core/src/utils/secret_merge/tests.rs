use serde_json::json;

use super::merge_preserving_secrets;

#[test]
fn nested_json_keeps_secret_and_applies_non_secret_edits() {
    let old = json!({
        "env": { "ANTHROPIC_AUTH_TOKEN": "sk-real", "ANTHROPIC_BASE_URL": "https://a" },
        "note": "x"
    });
    let new = json!({
        "env": { "ANTHROPIC_AUTH_TOKEN": "***", "ANTHROPIC_BASE_URL": "https://b" },
        "note": "y"
    });
    let merged = merge_preserving_secrets(&old, &new);
    assert_eq!(merged["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-real");
    assert_eq!(merged["env"]["ANTHROPIC_BASE_URL"], "https://b");
    assert_eq!(merged["note"], "y");
}

#[test]
fn empty_or_omitted_secret_keeps_stored_value() {
    let old = json!({
        "env": { "ANTHROPIC_AUTH_TOKEN": "sk-real", "ANTHROPIC_BASE_URL": "https://a" },
        "auth": { "OPENAI_API_KEY": "sk-codex" }
    });
    let new = json!({
        "env": { "ANTHROPIC_AUTH_TOKEN": "", "ANTHROPIC_BASE_URL": "https://b" }
    });
    let merged = merge_preserving_secrets(&old, &new);
    assert_eq!(merged["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-real");
    assert_eq!(merged["env"]["ANTHROPIC_BASE_URL"], "https://b");
    assert_eq!(merged["auth"]["OPENAI_API_KEY"], "sk-codex");
}

#[test]
fn toml_content_marker_keeps_whole_document() {
    let old = json!({
        "format": "toml",
        "content": "model = \"grok\"\napi_key = \"xai-secret\"\n"
    });
    let new = json!({ "format": "toml", "content": "***" });
    let merged = merge_preserving_secrets(&old, &new);
    assert!(merged["content"].as_str().unwrap().contains("xai-secret"));
}

#[test]
fn toml_inline_redacted_key_keeps_secret_and_new_model() {
    let old = json!({
        "format": "toml",
        "content": "[model.\"grok\"]\nmodel = \"grok-4.5\"\nbase_url = \"https://relay.example.com/v1\"\napi_key = \"xai-secret\"\napi_backend = \"responses\"\n"
    });
    let new = json!({
        "format": "toml",
        "content": "[model.\"grok\"]\nmodel = \"grok-4.6\"\nbase_url = \"https://relay.example.com/v1\"\napi_key = \"***\"\napi_backend = \"responses\"\n"
    });
    let merged = merge_preserving_secrets(&old, &new);
    let content = merged["content"].as_str().unwrap();
    assert!(content.contains("xai-secret"), "{content}");
    assert!(content.contains("grok-4.6"), "{content}");
    assert!(!content.contains("grok-4.5"), "{content}");
    assert!(
        content.contains("https://relay.example.com/v1"),
        "{content}"
    );
}

#[test]
fn real_new_secret_replaces_stored_secret() {
    let old = json!({ "env": { "ANTHROPIC_AUTH_TOKEN": "sk-old" } });
    let new = json!({ "env": { "ANTHROPIC_AUTH_TOKEN": "sk-new" } });
    let merged = merge_preserving_secrets(&old, &new);
    assert_eq!(merged["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-new");
}
