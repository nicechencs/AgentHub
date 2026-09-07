use super::*;
use serde_json::json;

#[test]
fn redacts_known_keys_case_insensitive_and_nested() {
    let input = json!({
        "api_key": "sk-1",
        "API_KEY": "sk-2",
        "Token": "t1",
        "AUTH_TOKEN": "t2",
        "Authorization": "Bearer x",
        "base_url": "https://example.com",
        "env": {
            "auth_token": "nested",
            "ANTHROPIC_AUTH_TOKEN": "anthropic-secret",
            "OPENAI_API_KEY": "openai-secret",
            "apiKey": "camel-secret",
            "clientSecret": "oauth-secret",
            "token_count": 10,
            "safe": "ok"
        },
        "list": [
            { "token": "in-array" },
            "plain"
        ]
    });
    let out = redact_json(&input);
    assert_eq!(out["api_key"], "***");
    assert_eq!(out["API_KEY"], "***");
    assert_eq!(out["Token"], "***");
    assert_eq!(out["AUTH_TOKEN"], "***");
    assert_eq!(out["Authorization"], "***");
    assert_eq!(out["base_url"], "https://example.com");
    assert_eq!(out["env"]["auth_token"], "***");
    assert_eq!(out["env"]["ANTHROPIC_AUTH_TOKEN"], "***");
    assert_eq!(out["env"]["OPENAI_API_KEY"], "***");
    assert_eq!(out["env"]["apiKey"], "***");
    assert_eq!(out["env"]["clientSecret"], "***");
    assert_eq!(out["env"]["token_count"], 10);
    assert_eq!(out["env"]["safe"], "ok");
    assert_eq!(out["list"][0]["token"], "***");
    assert_eq!(out["list"][1], "plain");
}

#[test]
fn non_object_passthrough() {
    assert_eq!(redact_json(&json!(null)), Value::Null);
    assert_eq!(redact_json(&json!(42)), json!(42));
    assert_eq!(redact_json(&json!("x")), json!("x"));
}

#[test]
fn toml_content_field_redacts_secrets_and_keeps_structure() {
    let input = json!({
        "format": "toml",
        "content": "[models]\ndefault = \"grok\"\n\n[model.\"grok\"]\nmodel = \"grok-4.5\"\nbase_url = \"https://mytokens.cc/v1\"\napi_key = \"xai-secret\"\napi_backend = \"responses\"\n\n[endpoints]\napi = \"https://mytokens.cc/v1\"\n"
    });
    let output = redact_json(&input);
    assert_eq!(output["format"], "toml");
    let content = output["content"].as_str().expect("content");
    assert!(content.contains("grok-4.5"), "{content}");
    assert!(content.contains("https://mytokens.cc/v1"), "{content}");
    assert!(content.contains("api_backend"), "{content}");
    assert!(content.contains("[endpoints]"), "{content}");
    assert!(!content.contains("xai-secret"), "{content}");
    assert!(content.contains("***"), "{content}");
}

#[test]
fn toml_content_strips_export_secret_lines() {
    let input = json!({
        "format": "toml",
        "content": "export XAI_API_KEY=xai-not-real\n[models]\ndefault = \"grok\"\n"
    });
    let output = redact_json(&input);
    let content = output["content"].as_str().expect("content");
    assert!(!content.contains("xai-not-real"), "{content}");
    assert!(
        !content.to_ascii_lowercase().contains("export xai_api_key"),
        "{content}"
    );
    assert!(content.contains("[models]"), "{content}");
}

#[test]
fn api_key_format_toml_content_is_redacted() {
    let input = json!({
        "format": "api_key",
        "api_key": "xai-secret-value-here",
        "content": "[model.\"grok\"]\nmodel = \"grok-4.6\"\napi_key = \"xai-secret-value-here\"\n"
    });
    let output = redact_json(&input);
    assert_eq!(output["api_key"], "***");
    let content = output["content"].as_str().expect("content");
    assert!(content.contains("grok-4.6"), "{content}");
    assert!(!content.contains("xai-secret-value-here"), "{content}");
    assert!(content.contains("***"), "{content}");
}

#[test]
fn api_key_format_json_content_is_redacted() {
    let input = json!({
        "format": "api_key",
        "api_key": "sk-ant-secret",
        "content": "{\n  \"env\": {\n    \"ANTHROPIC_AUTH_TOKEN\": \"sk-ant-secret\"\n  }\n}"
    });
    let output = redact_json(&input);
    assert_eq!(output["api_key"], "***");
    let content = output["content"].as_str().expect("content");
    assert!(!content.contains("sk-ant-secret"), "{content}");
    assert!(content.contains("***"), "{content}");
    assert!(content.contains("ANTHROPIC_AUTH_TOKEN"), "{content}");
}

#[test]
fn unusable_secret_markers_are_not_live_keys() {
    assert!(is_unusable_secret("***"));
    assert!(is_unusable_secret("$AGENTHUB_CONNECTION_SECRET$"));
    assert!(is_unusable_secret("••••"));
    assert!(!is_unusable_secret("sk-abcdefghijklmnop8660"));
}

#[test]
fn mask_secret_preview_hides_middle() {
    let preview = mask_secret_preview("sk-abcdefghijklmnop");
    assert!(preview.contains("••••"));
    assert!(!preview.contains("abcdefghijklmnop"));
    assert!(preview.starts_with("sk-"));
    assert_eq!(mask_secret_preview(""), "••••");
}

#[test]
fn refresh_token_preview_uses_head_tail_and_nested_codex_shape() {
    let flat = json!({ "refresh_token": "rt-abcdefghijklmnopqrstuvwxyz" });
    let preview = refresh_token_preview(&flat).expect("preview");
    assert!(preview.contains("••••"));
    assert!(!preview.contains("abcdefghijklmnopqrstuvwxyz"));
    assert_eq!(
        preview,
        mask_secret_preview("rt-abcdefghijklmnopqrstuvwxyz")
    );

    let nested = json!({
        "format": "auth_json",
        "body": { "tokens": { "access_token": "at-secret", "refresh_token": "rt-nested-secret-value" } }
    });
    let nested_preview = refresh_token_preview(&nested).expect("nested preview");
    assert_eq!(
        nested_preview,
        mask_secret_preview("rt-nested-secret-value")
    );
    assert!(!nested_preview.contains("rt-nested-secret-value"));

    assert!(refresh_token_preview(&json!({ "refresh_token": "***" })).is_none());
    assert!(refresh_token_preview(&json!({ "access_token": "only-access" })).is_none());
}

#[test]
fn mask_secret_tail_uses_last_four() {
    assert_eq!(
        mask_secret_tail("rt-abcdefghijklmnopqrstuvwxyz").as_deref(),
        Some("**wxyz")
    );
    assert_eq!(mask_secret_tail("short"), None);
    assert_eq!(mask_secret_tail("***"), None);
    assert_eq!(mask_secret_tail(""), None);
}

#[test]
fn secret_tail_from_masked_preview_reads_stored_identity_only() {
    assert_eq!(
        secret_tail_from_masked_preview("xai-••••8660 (API Key)").as_deref(),
        Some("**8660")
    );
    assert_eq!(
        secret_tail_from_masked_preview("sk--••••272f (API Key)").as_deref(),
        Some("**272f")
    );
    assert_eq!(
        secret_tail_from_masked_preview("**8660").as_deref(),
        Some("**8660")
    );
    assert_eq!(secret_tail_from_masked_preview("API Key"), None);
    assert_eq!(secret_tail_from_masked_preview("•••• (API Key)"), None);
    assert_eq!(secret_tail_from_masked_preview("**•••• (API Key)"), None);
    assert_eq!(secret_tail_from_masked_preview("mytokens.cc"), None);
}

#[test]
fn refresh_and_api_key_tails_read_nested_shapes() {
    let rt = json!({
        "body": { "tokens": { "refresh_token": "rt-abcdefghijklmnopqrstuvwxyz" } }
    });
    assert_eq!(refresh_token_tail(&rt).as_deref(), Some("**wxyz"));

    let json_key = json!({ "api_key": "sk-abcdefghijklmnop" });
    assert_eq!(api_key_tail(&json_key).as_deref(), Some("**mnop"));

    let claude = json!({
        "env": { "ANTHROPIC_AUTH_TOKEN": "sk-ant-abcdefghijklmnopqrstuvwxyz" }
    });
    assert_eq!(api_key_tail(&claude).as_deref(), Some("**wxyz"));

    let toml = json!({
        "format": "toml",
        "content": "model = 'grok'\napi_key = 'xai-secret-value-here'\n"
    });
    assert_eq!(api_key_tail(&toml).as_deref(), Some("**here"));
    assert!(api_key_tail(&json!({ "refresh_token": "rt-not-a-key-value" })).is_none());
}

#[test]
fn secret_hash_is_stable_and_never_the_raw_secret() {
    let key = "sk-fixture-openrouter-aaaa6aa9";
    let hash = secret_sha256_hex(key);
    assert_eq!(
        hash,
        secret_sha256_hex("  sk-fixture-openrouter-aaaa6aa9\n")
    );
    assert_eq!(hash.len(), 64);
    assert!(!hash.contains(key));
    assert_ne!(hash, secret_sha256_hex("sk-fixture-openrouter-bbbb6aa9"));

    let openai_auth = json!({ "auth": { "OPENAI_API_KEY": key } });
    assert_eq!(
        api_key_secret_hash(&openai_auth).as_deref(),
        Some(hash.as_str())
    );
    assert!(api_key_secret_hash(&json!({ "api_key": "***" })).is_none());
}

#[test]
fn redact_text_masks_keys_and_prefixes() {
    let s = redact_text("api_key=sk-abcdefghijklmnop and Bearer supersecrettokenvalue");
    assert!(s.contains("api_key=***") || s.contains("api_key=***"));
    assert!(s.contains("Bearer ***"));
    assert!(!s.contains("sk-abcdefghijklmnop"));
    assert!(!s.contains("supersecrettokenvalue"));
    let s2 = redact_text("token xai-abcdefghijklmnopqrst");
    assert!(s2.contains("xai-***"));
}

#[test]
fn redact_text_preserves_utf8_chinese() {
    let msg = "powershell 不支持一键安装；windows 通常已自带。";
    let out = redact_text(msg);
    assert_eq!(out, msg);
    let io = "io error: 当文件已存在时，无法创建该文件。 (os error 183)";
    assert_eq!(redact_text(io), io);
}

#[test]
fn is_secret_key_covers_oauth_style_keys() {
    assert!(is_secret_key("refresh_token"));
    assert!(is_secret_key("id_token"));
    assert!(is_secret_key("session_token"));
    assert!(is_secret_key("OPENAI_REFRESH_TOKEN"));
    assert!(!is_secret_key("token_count"));
    assert!(!is_secret_key("base_url"));
}

#[test]
fn redact_text_masks_quoted_assignment() {
    let s = redact_text(r#"password="s3cret-value" ok"#);
    assert!(s.contains("password="));
    assert!(s.contains("***"));
    assert!(!s.contains("s3cret-value"));
}

#[test]
fn redact_text_masks_oauth_and_private_key_assignments() {
    for sample in [
        "private_key=-----BEGIN-RSA-----abcdefgh",
        "session_token: abcdefghijklmnop",
        "id_token=eyJhbGciOiJIUzI1NiJ9.payload",
    ] {
        let s = redact_text(sample);
        assert!(s.contains("***"), "expected mask in {s}");
        assert!(!s.contains("abcdefgh"), "leaked value in {s}");
        assert!(!s.contains("eyJhbGciOiJIUzI1NiJ9"), "leaked jwt in {s}");
    }
}

#[test]
fn redact_url_userinfo_masks_credentials_keeps_host() {
    let s = redact_url_userinfo(
        "git clone failed from https://user:s3cret-token@github.com/org/repo.git#main",
    );
    assert!(
        s.contains("https://***@github.com/org/repo.git#main"),
        "{s}"
    );
    assert!(!s.contains("s3cret-token"), "{s}");
    assert!(!s.contains("user:"), "{s}");

    let plain = "https://github.com/org/repo.git";
    assert_eq!(redact_url_userinfo(plain), plain);

    let via_text = redact_text(
            "skill.update: git clone failed for skill 'x' from https://x-access-token:ghp_abc123456789@host/p",
        );
    assert!(via_text.contains("***@host/p"), "{via_text}");
    assert!(!via_text.contains("ghp_abc123456789"), "{via_text}");
}
