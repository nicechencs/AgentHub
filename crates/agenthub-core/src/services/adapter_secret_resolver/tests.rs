use super::*;
use crate::services::adapter_route_constants::KIMI_MEMBERSHIP_PRESET;
use serde_json::json;

fn provider(id: &str, agent_id: AgentId, settings_config: Value, meta: Value) -> Provider {
    Provider {
        id: id.into(),
        agent_id,
        name: id.into(),
        settings_config,
        meta,
        is_current: false,
        created_at: "now".into(),
        updated_at: "now".into(),
    }
}

fn pi_kimi_target(source_id: &str) -> Provider {
    provider(
        "generated-pi-kimi",
        AgentId::Pi,
        json!({
            "models": {
                "providers": {
                    KIMI_PI_PROVIDER_SLOT: {
                        "baseUrl": KIMI_PI_BASE_URL,
                        "apiKey": CONNECTION_SECRET_MARKER,
                        "api": "openai-completions",
                        "models": [{ "id": "kimi-k2.5" }]
                    }
                }
            }
        }),
        json!({
            "generatedBy": GENERATED_BY,
            "adapterRuleId": KIMI_TO_PI_RULE,
            "adapterRuleVersion": 1,
            "adapterSecretMode": SOURCE_REFERENCE_MODE,
            "adapterSourceRef": { "kind": SOURCE_KIND_PROVIDER, "id": source_id },
        }),
    )
}

fn pi_anthropic_target(source_id: &str) -> Provider {
    provider(
        "generated-pi-anthropic",
        AgentId::Pi,
        json!({
            "models": {
                "providers": {
                    ANTHROPIC_PI_PROVIDER_SLOT: {
                        "apiKey": CONNECTION_SECRET_MARKER
                    }
                }
            }
        }),
        json!({
            "generatedBy": GENERATED_BY,
            "adapterRuleId": ANTHROPIC_TO_PI_RULE,
            "adapterRuleVersion": 1,
            "adapterSecretMode": SOURCE_REFERENCE_MODE,
            "adapterSourceRef": { "kind": SOURCE_KIND_PROVIDER, "id": source_id },
        }),
    )
}

fn target(source_id: &str) -> Provider {
    provider(
        "generated-claude",
        AgentId::Claude,
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": KIMI_CLAUDE_BASE_URL,
                "ANTHROPIC_AUTH_TOKEN": CONNECTION_SECRET_MARKER,
            }
        }),
        json!({
            "generatedBy": GENERATED_BY,
            "adapterRuleId": KIMI_TO_CLAUDE_RULE,
            "adapterRuleVersion": 1,
            "adapterSecretMode": SOURCE_REFERENCE_MODE,
            "adapterSourceRef": { "kind": SOURCE_KIND_PROVIDER, "id": source_id },
        }),
    )
}

fn resolver_with(source: Provider) -> (tempfile::TempDir, AdapterSecretResolver) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("adapter-secret-resolver.db")).unwrap();
    ProviderRepo::new(db.clone()).create(&source).unwrap();
    (dir, AdapterSecretResolver::new(db))
}

#[test]
fn materializes_only_a_returned_clone_and_scrubs_backfill() {
    let source = provider(
        "kimi-source",
        AgentId::Kimi,
        json!({"apiKey": "test-kimi-secret"}),
        json!({"preset": KIMI_MEMBERSHIP_PRESET}),
    );
    let (_dir, resolver) = resolver_with(source.clone());
    let target = target(&source.id);

    let materialized = resolver.materialize_for_live(&target).unwrap();
    assert_eq!(
        materialized.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "test-kimi-secret"
    );
    assert_eq!(
        target.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        CONNECTION_SECRET_MARKER
    );
    assert_eq!(
        resolver
            .providers
            .get_by_id(&source.id)
            .unwrap()
            .unwrap()
            .settings_config,
        source.settings_config
    );

    let scrubbed = resolver
        .scrub_for_backfill(&target, &materialized.settings_config)
        .unwrap();
    assert_eq!(
        scrubbed["env"]["ANTHROPIC_AUTH_TOKEN"],
        CONNECTION_SECRET_MARKER
    );
    assert_eq!(scrubbed["env"]["ANTHROPIC_BASE_URL"], KIMI_CLAUDE_BASE_URL);
    assert!(!serde_json::to_string(&scrubbed)
        .unwrap()
        .contains("test-kimi-secret"));
}

#[test]
fn materializes_supported_toml_api_key_paths_without_recursive_guessing() {
    let source = provider(
        "kimi-source",
        AgentId::Kimi,
        json!({"format": "toml", "content": "[providers.x]\napi_key = 'toml-test-secret'\n[unrelated]\napi_key = 'nested-must-not-be-used'\n"}),
        json!({"preset": KIMI_MEMBERSHIP_PRESET}),
    );
    let (_dir, resolver) = resolver_with(source.clone());
    let materialized = resolver.materialize_for_live(&target(&source.id)).unwrap();
    assert_eq!(
        materialized.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "toml-test-secret"
    );
}

#[test]
fn toml_prefers_default_provider_then_first_non_empty_provider_then_legacy_key() {
    let source = provider(
        "kimi-source",
        AgentId::Kimi,
        json!({"format": "toml", "content": "default_provider = 'selected'\napi_key = 'legacy-secret'\n[providers.first]\napi_key = 'first-secret'\n[providers.selected]\napi_key = 'selected-secret'\n"}),
        json!({"preset": KIMI_MEMBERSHIP_PRESET}),
    );
    let (_dir, resolver) = resolver_with(source.clone());
    assert_eq!(
        resolver
            .materialize_for_live(&target(&source.id))
            .unwrap()
            .settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "selected-secret"
    );

    let fallback_source = provider(
        "kimi-fallback-source",
        AgentId::Kimi,
        json!({"format": "toml", "content": "default_provider = 'missing'\napi_key = 'legacy-secret'\n[providers.empty]\napi_key = '   '\n[providers.usable]\napi_key = 'first-non-empty-secret'\n"}),
        json!({"preset": KIMI_MEMBERSHIP_PRESET}),
    );
    let (_dir, resolver) = resolver_with(fallback_source.clone());
    assert_eq!(
        resolver
            .materialize_for_live(&target(&fallback_source.id))
            .unwrap()
            .settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "first-non-empty-secret"
    );

    let legacy_source = provider(
        "kimi-legacy-source",
        AgentId::Kimi,
        json!({"format": "toml", "content": "api_key = 'legacy-secret'\n[providers.empty]\napi_key = ''\n"}),
        json!({"preset": KIMI_MEMBERSHIP_PRESET}),
    );
    let (_dir, resolver) = resolver_with(legacy_source.clone());
    assert_eq!(
        resolver
            .materialize_for_live(&target(&legacy_source.id))
            .unwrap()
            .settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "legacy-secret"
    );
}

#[test]
fn rejects_invalid_reference_without_leaking_secret_or_raw() {
    let source = provider(
        "kimi-source",
        AgentId::Kimi,
        json!({"apiKey": "very-secret-value"}),
        json!({"preset": KIMI_MEMBERSHIP_PRESET}),
    );
    let (_dir, resolver) = resolver_with(source.clone());
    let mut malformed = target(&source.id);
    malformed.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"] = json!("not-a-marker");

    let error = resolver.materialize_for_live(&malformed).unwrap_err();
    assert_eq!(error.code(), "invalid_arg");
    assert!(!error.to_string().contains("very-secret-value"));
    assert!(!error.to_string().contains("not-a-marker"));
}

#[test]
fn ordinary_provider_passes_through_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("adapter-secret-resolver.db")).unwrap();
    let resolver = AdapterSecretResolver::new(db);
    let ordinary = provider(
        "ordinary",
        AgentId::Claude,
        json!({"env": {"ANTHROPIC_AUTH_TOKEN": "ordinary-secret"}}),
        json!({"preset": "custom"}),
    );
    assert_eq!(resolver.materialize_for_live(&ordinary).unwrap(), ordinary);
    assert_eq!(
        resolver
            .scrub_for_backfill(&ordinary, &ordinary.settings_config)
            .unwrap(),
        ordinary.settings_config
    );
}

#[test]
fn pi_kimi_source_reference_materializes_and_scrubs_slot() {
    let source = provider(
        "kimi-source",
        AgentId::Kimi,
        json!({"apiKey": "test-kimi-secret"}),
        json!({"preset": KIMI_MEMBERSHIP_PRESET}),
    );
    let (_dir, resolver) = resolver_with(source.clone());
    let target = pi_kimi_target(&source.id);

    assert!(resolver.is_reference_provider(&target).unwrap());
    let materialized = resolver.materialize_for_live(&target).unwrap();
    assert_eq!(
        materialized.settings_config["models"]["providers"][KIMI_PI_PROVIDER_SLOT]["apiKey"],
        "test-kimi-secret"
    );
    assert_eq!(
        target.settings_config["models"]["providers"][KIMI_PI_PROVIDER_SLOT]["apiKey"],
        CONNECTION_SECRET_MARKER
    );

    let live_raw = json!({
        "settings": {},
        "models": {
            "providers": {
                KIMI_PI_PROVIDER_SLOT: {
                    "baseUrl": KIMI_PI_BASE_URL,
                    "apiKey": "test-kimi-secret"
                },
                "keep": { "apiKey": "other-live-secret" }
            }
        },
        "paths": { "models": "models.json" }
    });
    let scrubbed = resolver.scrub_for_backfill(&target, &live_raw).unwrap();
    assert_eq!(
        scrubbed["models"]["providers"][KIMI_PI_PROVIDER_SLOT]["apiKey"],
        CONNECTION_SECRET_MARKER
    );
    assert_eq!(
        scrubbed["models"]["providers"]["keep"]["apiKey"],
        "other-live-secret"
    );
    assert!(!serde_json::to_string(&scrubbed)
        .unwrap()
        .contains("test-kimi-secret"));
}

#[test]
fn pi_anthropic_reads_auth_token_api_key_env_or_top_level_and_rejects_masked() {
    for (settings, expected) in [
        (
            json!({"env": { ANTHROPIC_AUTH_TOKEN_ENV: "sk-auth-token" }}),
            "sk-auth-token",
        ),
        (
            json!({"env": { ANTHROPIC_API_KEY_ENV: "sk-api-key" }}),
            "sk-api-key",
        ),
        (json!({"apiKey": "sk-top-level"}), "sk-top-level"),
    ] {
        let source = provider(
            "anthropic-source",
            AgentId::Claude,
            settings,
            json!({"preset": ANTHROPIC_PRESET}),
        );
        let (_dir, resolver) = resolver_with(source.clone());
        let target = pi_anthropic_target(&source.id);
        assert!(resolver.is_reference_provider(&target).unwrap());
        assert_eq!(
            resolver
                .materialize_for_live(&target)
                .unwrap()
                .settings_config["models"]["providers"][ANTHROPIC_PI_PROVIDER_SLOT]["apiKey"],
            expected
        );
    }

    for settings in [
        json!({"env": { ANTHROPIC_AUTH_TOKEN_ENV: "" }}),
        json!({"env": { ANTHROPIC_API_KEY_ENV: "***" }}),
        json!({"apiKey": CONNECTION_SECRET_MARKER}),
        json!({}),
    ] {
        let source = provider(
            "anthropic-empty",
            AgentId::Claude,
            settings,
            json!({"preset": ANTHROPIC_PRESET}),
        );
        let (_dir, resolver) = resolver_with(source.clone());
        assert_eq!(
            resolver
                .materialize_for_live(&pi_anthropic_target(&source.id))
                .unwrap_err()
                .code(),
            "invalid_arg"
        );
    }
}

#[test]
fn pi_invalid_reference_fails_closed_without_leaking_secret() {
    let source = provider(
        "kimi-source",
        AgentId::Kimi,
        json!({"apiKey": "very-secret-value"}),
        json!({"preset": KIMI_MEMBERSHIP_PRESET}),
    );
    let (_dir, resolver) = resolver_with(source.clone());
    let mut malformed = pi_kimi_target(&source.id);
    malformed.settings_config["models"]["providers"][KIMI_PI_PROVIDER_SLOT]["apiKey"] =
        json!("not-a-marker");

    let error = resolver.materialize_for_live(&malformed).unwrap_err();
    assert_eq!(error.code(), "invalid_arg");
    assert!(!error.to_string().contains("very-secret-value"));
    assert!(!error.to_string().contains("not-a-marker"));
}

#[test]
fn local_token_bridge_passes_through_but_unknown_generated_metadata_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("adapter-secret-resolver.db")).unwrap();
    let resolver = AdapterSecretResolver::new(db);
    let bridge = provider(
        "generated-codex",
        AgentId::Codex,
        json!({
            "format": "toml",
            "content": "model_provider = 'agenthub_kimi_bridge'\n",
            "auth": { "OPENAI_API_KEY": "local-bridge-token" },
        }),
        json!({
            "generatedBy": GENERATED_BY,
            "adapterRuleId": KIMI_TO_CODEX_BRIDGE_RULE,
            "adapterRuleVersion": 1,
            "adapterSecretMode": LOCAL_TOKEN_MODE,
            "adapterProfileId": "bridge-profile",
            "adapterSourceRef": { "kind": SOURCE_KIND_PROVIDER, "id": "kimi-source" },
        }),
    );
    assert!(!resolver.is_reference_provider(&bridge).unwrap());
    assert_eq!(resolver.materialize_for_live(&bridge).unwrap(), bridge);
    assert_eq!(
        resolver
            .scrub_for_backfill(&bridge, &bridge.settings_config)
            .unwrap(),
        bridge.settings_config
    );

    for mutation in [
        json!({"adapterRuleVersion": 2}),
        json!({"adapterRuleVersion": "1"}),
        json!({"adapterSecretMode": "source_reference"}),
        json!({"adapterProfileId": ""}),
    ] {
        let mut malformed = bridge.clone();
        let object = malformed.meta.as_object_mut().unwrap();
        for (key, value) in mutation.as_object().unwrap() {
            object.insert(key.clone(), value.clone());
        }
        assert_eq!(
            resolver
                .is_reference_provider(&malformed)
                .unwrap_err()
                .code(),
            "invalid_arg"
        );
    }
}

#[test]
fn coding_endpoint_without_preset_resolves_and_materializes() {
    let source = provider(
        "kimi-live-import",
        AgentId::Kimi,
        json!({
            "apiKey": "test-kimi-secret",
            "baseUrl": "https://api.kimi.com/coding/v1"
        }),
        json!({}),
    );
    let (_dir, resolver) = resolver_with(source.clone());
    resolver
        .validate_kimi_membership_source(&source.id)
        .unwrap();
    assert_eq!(
        resolver
            .resolve_kimi_membership_auth(&source.id)
            .unwrap()
            .token(),
        "test-kimi-secret"
    );

    let materialized = resolver.materialize_for_live(&target(&source.id)).unwrap();
    assert_eq!(
        materialized.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "test-kimi-secret"
    );
    assert_eq!(
        target(&source.id).settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        CONNECTION_SECRET_MARKER
    );
}

#[test]
fn membership_requires_kimi_agent_and_preset_or_coding_endpoint() {
    let cases: &[(&str, AgentId, Value, Value, bool)] = &[
        (
            "preset",
            AgentId::Kimi,
            json!({"apiKey": "secret"}),
            json!({"preset": KIMI_MEMBERSHIP_PRESET}),
            true,
        ),
        (
            "endpoint",
            AgentId::Kimi,
            json!({"apiKey": "secret", "baseUrl": "https://api.kimi.com/coding/v1"}),
            json!({}),
            true,
        ),
        (
            "moonshot",
            AgentId::Kimi,
            json!({"apiKey": "secret", "baseUrl": "https://api.moonshot.cn/v1"}),
            json!({"preset": "moonshot"}),
            false,
        ),
        (
            "bare",
            AgentId::Kimi,
            json!({"apiKey": "secret"}),
            json!({}),
            false,
        ),
        (
            "wrong-agent",
            AgentId::Claude,
            json!({"apiKey": "secret", "baseUrl": "https://api.kimi.com/coding/v1"}),
            json!({"preset": KIMI_MEMBERSHIP_PRESET}),
            false,
        ),
    ];
    for (id, agent, settings, meta, ok) in cases {
        let source = provider(id, *agent, settings.clone(), meta.clone());
        let (_dir, resolver) = resolver_with(source);
        let result = resolver.validate_kimi_membership_source(id);
        if *ok {
            result.unwrap();
        } else {
            assert_eq!(result.unwrap_err().code(), "invalid_arg", "{id}");
        }
    }
}

fn dsh_target(source_id: &str) -> Provider {
    provider(
        "generated-dsh",
        AgentId::Dsh,
        json!({
            "provider": DSH_DEEPSEEK_PROVIDER_SLOT,
            "model": "deepseek-v4-flash",
            "apiKeyEnv": DSH_API_KEY_ENV,
            "baseURL": DEEPSEEK_API_BASE_URL,
            "api_key": CONNECTION_SECRET_MARKER,
        }),
        json!({
            "generatedBy": GENERATED_BY,
            "adapterRuleId": DEEPSEEK_TO_DSH_RULE,
            "adapterRuleVersion": 1,
            "adapterSecretMode": SOURCE_REFERENCE_MODE,
            "adapterSourceRef": { "kind": SOURCE_KIND_PROVIDER, "id": source_id },
        }),
    )
}

#[test]
fn dsh_source_reference_materializes_and_scrubs() {
    let source = provider(
        "ds-source",
        AgentId::Claude,
        json!({"apiKey": "sk-deepseek-secret"}),
        json!({"preset": "deepseek"}),
    );
    let (_dir, resolver) = resolver_with(source.clone());
    let target = dsh_target(&source.id);

    assert!(resolver.is_reference_provider(&target).unwrap());
    let materialized = resolver.materialize_for_live(&target).unwrap();
    assert_eq!(materialized.settings_config["api_key"], "sk-deepseek-secret");
    assert_eq!(target.settings_config["api_key"], CONNECTION_SECRET_MARKER);

    let live_raw = json!({
        "provider": DSH_DEEPSEEK_PROVIDER_SLOT,
        "apiKeyEnv": DSH_API_KEY_ENV,
        "baseURL": DEEPSEEK_API_BASE_URL,
        "api_key": "sk-deepseek-secret",
        "keep": "other-live"
    });
    let scrubbed = resolver.scrub_for_backfill(&target, &live_raw).unwrap();
    assert_eq!(scrubbed["api_key"], CONNECTION_SECRET_MARKER);
    assert_eq!(scrubbed["keep"], "other-live");
    assert!(!serde_json::to_string(&scrubbed)
        .unwrap()
        .contains("sk-deepseek-secret"));
}

#[test]
fn dsh_rejects_agent_id_only_source_and_missing_secret() {
    let bare = provider(
        "dsh-only",
        AgentId::Dsh,
        json!({"apiKey": "sk-not-a-ticket"}),
        json!({}),
    );
    let (_dir, resolver) = resolver_with(bare);
    assert_eq!(
        resolver
            .materialize_for_live(&dsh_target("dsh-only"))
            .unwrap_err()
            .code(),
        "invalid_arg"
    );

    let empty = provider(
        "ds-empty",
        AgentId::Claude,
        json!({"apiKey": ""}),
        json!({"preset": "deepseek"}),
    );
    let (_dir, resolver) = resolver_with(empty);
    assert_eq!(
        resolver
            .materialize_for_live(&dsh_target("ds-empty"))
            .unwrap_err()
            .code(),
        "invalid_arg"
    );
}
