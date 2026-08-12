use super::*;
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
