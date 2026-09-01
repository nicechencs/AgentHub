use serde_json::json;

use super::{
    cache_is_current, catalog_endpoint, embedded_listed_models, fingerprint_apikey,
    fingerprint_oauth, read_stored_catalog, with_wanted_models, write_stored_catalog,
    SourceModelCatalog, StoredModelCatalog,
};

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

#[test]
fn oauth_fingerprint_ignores_access_token() {
    let a = fingerprint_oauth("codex", "acct-1");
    let b = fingerprint_oauth("codex", "acct-1");
    let c = fingerprint_oauth("codex", "acct-2");
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn apikey_fingerprint_changes_with_url_or_key() {
    let one = json!({"api_key": "sk-a", "base_url": "https://api.example.com/v1"});
    let url = json!({"api_key": "sk-a", "base_url": "https://other.example.com/v1"});
    let key = json!({"api_key": "sk-b", "base_url": "https://api.example.com/v1"});
    assert_ne!(fingerprint_apikey("claude", &one), fingerprint_apikey("claude", &url));
    assert_ne!(fingerprint_apikey("claude", &one), fingerprint_apikey("claude", &key));
    assert_eq!(fingerprint_apikey("claude", &one), fingerprint_apikey("claude", &one));
}

#[test]
fn stored_catalog_roundtrip_and_cache_hit() {
    let mut extra = json!({"quota7dPct": 12});
    let stored = StoredModelCatalog {
        fingerprint: "fp-1".into(),
        source: "live".into(),
        models: vec!["gpt-5.4".into()],
        extra_models: Vec::new(),
        attempted: true,
        updated_at: "t0".into(),
    };
    write_stored_catalog(&mut extra, &stored);
    assert_eq!(extra["quota7dPct"], 12);
    let read = read_stored_catalog(&extra).expect("catalog");
    assert_eq!(read.models, vec!["gpt-5.4"]);
    assert!(cache_is_current(&read, "fp-1"));
    assert!(!cache_is_current(&read, "fp-2"));
}

#[test]
fn wanted_models_keep_live_and_store_extras() {
    let live = StoredModelCatalog {
        fingerprint: "fp".into(),
        source: "live".into(),
        models: vec!["gpt-5.4".into()],
        extra_models: Vec::new(),
        attempted: true,
        updated_at: "t0".into(),
    };
    let next = with_wanted_models(live, vec!["gpt-5.4".into(), "my-model".into()]);
    assert_eq!(next.source, "live");
    assert_eq!(next.models, vec!["gpt-5.4"]);
    assert_eq!(next.extra_models, vec!["my-model"]);
    assert_eq!(
        SourceModelCatalog::from_stored(&next).models,
        vec!["gpt-5.4", "my-model"]
    );
}
