use serde_json::json;

use crate::models::{
    AdapterProfile, AdapterProfileMode, AdapterProfileStatus, AdapterRoute, AdapterSourceKind,
    AgentId, Provider,
};
use crate::utils::redact::secret_sha256_hex;

use super::{
    looks_like_uuid_provider_id, normalize_base_url, normalize_provider_base_url,
    pick_identity_keeper, provider_identity, retarget_profiles_from_loser, stamp_secret_hash,
};

fn row(id: &str, secret: &str, url: &str) -> Provider {
    Provider {
        id: id.into(),
        agent_id: AgentId::Codex,
        name: "OpenRouter 备选".into(),
        settings_config: json!({
            "baseURL": url,
            "baseUrl": url,
            "apiKey": secret,
            "api_key": secret,
            "model": "stealth/ox-alpha",
        }),
        meta: json!({ "preset": "openrouter" }),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

fn profile_for(source_id: &str) -> AdapterProfile {
    AdapterProfile {
        id: "profile-1".into(),
        name: "OpenAI → Claude".into(),
        source_kind: AdapterSourceKind::Provider,
        source_id: source_id.into(),
        target_agent_id: AgentId::Claude,
        route: AdapterRoute::LocalBridge,
        mode: AdapterProfileMode::Api,
        status: AdapterProfileStatus::Active,
        rule_id: "openai-api-to-claude-v1".into(),
        rule_version: "1".into(),
        generated_provider_id: Some("claude-openai-adapter-bridge".into()),
        local_port: Some(43111),
        auto_start: false,
        last_error_code: None,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }
}

#[test]
fn identity_matches_same_secret_and_url_ignores_last4() {
    let secret = "sk-or-v1-fixture-aaaa6aa9-not-real";
    let url = "https://openrouter.ai/api/v1/";
    let a = row("openai-compat-openrouter-backup", secret, url);
    let b = row(
        "openai-compat-0e08e310-97ba-4575-a50b-3e3db6eec38c",
        secret,
        "https://openrouter.ai/api/v1",
    );
    let left = provider_identity(&a).expect("identity");
    let right = provider_identity(&b).expect("identity");
    assert_eq!(left, right);
    assert_eq!(left.secret_hash, secret_sha256_hex(secret));
    assert_eq!(left.base_url, "https://openrouter.ai/api");
    assert_ne!(
        provider_identity(&row(
            "other",
            "sk-or-v1-fixture-bbbb6aa9-not-real",
            "https://openrouter.ai/api/v1"
        ))
        .unwrap()
        .secret_hash,
        left.secret_hash
    );
}

#[test]
fn identity_skips_generated_adapter_projections() {
    let mut generated = row(
        "proj",
        "sk-or-v1-fixture-aaaa6aa9-not-real",
        "https://openrouter.ai/api/v1",
    );
    generated.meta = json!({ "generatedBy": "adapter" });
    assert!(provider_identity(&generated).is_none());
}

#[test]
fn keeper_prefers_uuid_with_bindings_over_backup_slug() {
    let backup = row(
        "openai-compat-openrouter-backup",
        "sk-or-v1-fixture-aaaa6aa9-not-real",
        "https://openrouter.ai/api/v1",
    );
    let uuid = row(
        "openai-compat-0e08e310-97ba-4575-a50b-3e3db6eec38c",
        "sk-or-v1-fixture-aaaa6aa9-not-real",
        "https://openrouter.ai/api/v1",
    );
    let profiles = vec![profile_for(&uuid.id)];
    let rows = [backup, uuid];
    let keeper = pick_identity_keeper(&rows, &profiles).expect("keeper");
    assert_eq!(
        keeper.id,
        "openai-compat-0e08e310-97ba-4575-a50b-3e3db6eec38c"
    );
    assert!(looks_like_uuid_provider_id(&keeper.id));
}

#[test]
fn retarget_moves_profile_source_off_the_loser() {
    let mut profiles = vec![profile_for("openai-compat-openrouter-backup")];
    let changed = retarget_profiles_from_loser(
        &mut profiles,
        "openai-compat-openrouter-backup",
        "openai-compat-0e08e310-97ba-4575-a50b-3e3db6eec38c",
    );
    assert_eq!(changed, vec![0]);
    assert_eq!(
        profiles[0].source_id,
        "openai-compat-0e08e310-97ba-4575-a50b-3e3db6eec38c"
    );
}

#[test]
fn stamp_secret_hash_writes_meta_not_raw_secret() {
    let mut meta = json!({ "preset": "openrouter" });
    let settings = json!({ "api_key": "sk-or-v1-fixture-aaaa6aa9-not-real" });
    stamp_secret_hash(&mut meta, &settings);
    let hash = meta["secretHash"].as_str().expect("hash");
    assert_eq!(
        hash,
        secret_sha256_hex("sk-or-v1-fixture-aaaa6aa9-not-real")
    );
    assert!(!meta.to_string().contains("sk-or-v1-fixture"));
}

#[test]
fn normalize_provider_base_url_reads_kimi_default_provider_toml() {
    let settings = json!({
        "format": "toml",
        "content": r#"
default_provider = "moonshot"

[providers.moonshot]
type = "openai"
base_url = "http://127.0.0.1:17034/"
api_key = "sk-fixture"
"#
    });
    assert_eq!(
        normalize_provider_base_url(&settings).as_deref(),
        Some("http://127.0.0.1:17034")
    );
}

#[test]
fn normalize_provider_base_url_does_not_fall_back_when_named_provider_is_missing() {
    let settings = json!({
        "format": "toml",
        "content": r#"
default_provider = "missing"
base_url = "https://also-wrong.example.com"

[providers.other]
base_url = "https://wrong.example.com"
"#
    });
    assert_eq!(normalize_provider_base_url(&settings), None);
}

#[test]
fn normalize_provider_base_url_rejects_ambiguous_provider_tables() {
    let settings = json!({
        "format": "toml",
        "content": r#"
[providers.first]
base_url = "https://first.example.com"

[providers.second]
base_url = "https://second.example.com"
"#
    });
    assert_eq!(normalize_provider_base_url(&settings), None);
}

#[test]
fn normalize_base_url_strips_trailing_slash() {
    assert_eq!(
        normalize_base_url(" https://openrouter.ai/api/v1/ "),
        "https://openrouter.ai/api"
    );
    assert_eq!(
        normalize_base_url("https://mytokens.cc/v1"),
        "https://mytokens.cc"
    );
    assert_eq!(
        normalize_base_url("https://mytokens.cc"),
        "https://mytokens.cc"
    );
}
