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
    assert!(!is_openai_api_marker(Some("openai-compat"), &json!({})));
    assert!(!is_custom_openai_compat(Some("openai-compat"), &json!({})));
    assert!(!is_openai_api_marker(
        Some("openai"),
        &json!({ "baseURL": "https://relay.example.com/v1", "apiKey": "test-key" }),
    ));
    assert!(!is_custom_openai_compat(
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

#[test]
fn upstream_models_health_probe_skips_deepseek_glm_and_anthropic_relays() {
    assert!(!upstream_models_health_probe_supported("https://api.deepseek.com"));
    assert!(!upstream_models_health_probe_supported("https://api.deepseek.com/anthropic"));
    assert!(!upstream_models_health_probe_supported(
        "https://open.bigmodel.cn/api/anthropic",
    ));
    assert!(upstream_models_health_probe_supported("https://api.openai.com/v1"));
    assert!(upstream_models_health_probe_supported("https://api.anthropic.com/v1"));
    assert!(upstream_models_health_probe_supported("https://openrouter.ai/api/v1"));
}

#[test]
fn non_openai_tags_cannot_be_promoted_by_openai_or_relay_urls() {
    for tag in [
        "anthropic",
        "anthropic-api",
        "anthropic-compatible",
        "xai",
        "xai-api",
        "glm-coding-plan",
        "deepseek",
        "deepseek-api",
        "kimi",
        "kimi-code-membership",
    ] {
        for url in [
            "https://api.openai.com/v1",
            "https://api.openai.com.evil.example/v1",
            "https://relay.example/v1",
        ] {
            assert!(
                !is_openai_api_marker(Some(tag), &json!({ "base_url": url })),
                "{tag} must not become OpenAI from {url}"
            );
        }
    }
}

#[test]
fn kimi_providers_toml_custom_remote_classifies_as_openai_api() {
    let blob = json!({
        "format": "toml",
        "content": "default_model = \"kimi-k2\"\ndefault_provider = \"moonshot\"\n\n[providers.moonshot]\nbase_url = \"https://mytokens.cc/v1\"\napi_key = \"sk-test\"\n"
    });
    assert!(
        settings_contain_custom_openai_compat_remote(&blob),
        "Kimi [providers.*] base_url must count as a custom OpenAI-compat remote"
    );
    assert!(!is_unknown_custom_relay_provider(&crate::models::Provider {
        id: "qa-kimi".into(),
        agent_id: crate::models::AgentId::Kimi,
        name: "QA Kimi manual".into(),
        settings_config: blob.clone(),
        meta: serde_json::json!({ "preset": "custom" }),
        is_current: true,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }));
}

#[test]
fn mytokens_toml_custom_remote_classifies_as_openai_api() {
    let blob = json!({
        "format": "toml",
        "content": "model_provider = \"OpenAI\"\nmodel = \"gpt-5.5\"\n\n[model_providers.OpenAI]\nname = \"OpenAI\"\nbase_url = \"https://mytokens.cc/v1\"\n"
    });
    assert!(is_openai_api_marker(Some("openai-compatible"), &blob));
    assert!(settings_contain_custom_openai_compat_remote(&blob));
    assert!(!is_openai_api_marker(
        Some("openai-compatible"),
        &json!({"api_key": "must-not-leak"}),
    ));
    assert!(!settings_contain_custom_openai_compat_remote(&json!({
        "format": "toml",
        "content": "model_provider = \"agenthub_claude_bridge\"\n\n[model_providers.agenthub_claude_bridge]\nbase_url = \"http://127.0.0.1:33923/v1\"\n"
    })));
}
#[test]
fn official_hosts_require_a_single_exact_base_url_host() {
    assert!(settings_contain_openai_api_endpoint(&serde_json::json!({
        "base_url": "https://API.OPENAI.COM:443/v1"
    })));
    assert!(settings_contain_openrouter_endpoint(&serde_json::json!({
        "baseUrl": "https://openrouter.ai/api/v1"
    })));

    for value in [
        serde_json::json!({ "base_url": "https://api.openai.com.evil.example/v1" }),
        serde_json::json!({ "comment": "https://api.openai.com/v1" }),
        serde_json::json!({
            "base_url": "https://relay.example/v1 https://api.openai.com/v1"
        }),
        serde_json::json!({
            "base_url": "https://relay.example/v1",
            "other": "https://api.openai.com/v1"
        }),
    ] {
        assert!(!settings_contain_openai_api_endpoint(&value), "{value}");
    }

    // A valid high-priority alias wins over a conflicting lower-priority one.
    assert!(!settings_contain_openai_api_endpoint(&serde_json::json!({
        "baseURL": "https://relay.example/v1",
        "base_url": "https://api.openai.com/v1"
    })));
    // Invalid high-priority values are skipped in favor of the first valid alias.
    assert!(settings_contain_openai_api_endpoint(&serde_json::json!({
        "baseURL": "not a URL",
        "base_url": "https://api.openai.com/v1"
    })));
}

#[test]
fn openrouter_url_matching_is_host_exact() {
    assert!(is_custom_openai_compat_url("https://OPENROUTER.AI:443/v1"));
    assert!(!is_custom_openai_compat_url(
        "https://openrouter.ai.evil.example/v1"
    ));
    assert!(!is_custom_openai_compat_url(
        "https://relay.example/v1 https://openrouter.ai/v1"
    ));
}

#[test]
fn toml_base_url_is_classified_using_the_runtime_base_url() {
    let official_openai = serde_json::json!({
        "format": "toml",
        "content": "model_provider = \"OpenAI\"\n\n[model_providers.OpenAI]\nbase_url = \"https://api.openai.com/v1\"\n"
    });
    assert!(settings_contain_openai_api_endpoint(&official_openai));
    assert!(is_openai_api_marker(None, &official_openai));

    let official_openrouter = serde_json::json!({
        "format": "toml",
        "content": "[model_providers.router]\nbase_url = \"https://openrouter.ai/api/v1\"\n"
    });
    assert!(settings_contain_openrouter_endpoint(&official_openrouter));
    assert!(is_openai_api_marker(None, &official_openrouter));

    let custom = serde_json::json!({
        "format": "toml",
        "content": "[model_providers.OpenAI]\nbase_url = \"https://relay.example/v1\"\n"
    });
    assert!(!settings_contain_openai_api_endpoint(&custom));
    assert!(settings_contain_custom_openai_compat_remote(&custom));
    assert!(!is_unknown_custom_relay_provider(&crate::models::Provider {
        id: "relay".into(),
        agent_id: crate::models::AgentId::Codex,
        name: "relay".into(),
        settings_config: custom,
        meta: serde_json::json!({ "preset": "openai-compatible" }),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    }));
}

#[test]
fn unknown_custom_relay_helper_does_not_whitelist_urls_by_substring() {
    let provider = |settings_config| crate::models::Provider {
        id: "relay".into(),
        agent_id: crate::models::AgentId::Codex,
        name: "relay".into(),
        settings_config,
        meta: serde_json::json!({ "preset": "openai-compatible" }),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };

    assert!(is_unknown_custom_relay_provider(&provider(serde_json::json!({
        "base_url": "https://relay.example/v1 https://api.openai.com/v1"
    }))));
    assert!(!is_unknown_custom_relay_provider(&provider(serde_json::json!({
        "base_url": "https://api.openai.com.evil.example/v1"
    }))));
    assert!(!is_unknown_custom_relay_provider(&provider(serde_json::json!({
        "base_url": "https://relay.example/v1",
        "comment": "official https://api.openai.com/v1"
    }))));

    for (preset, settings) in [
        ("openai", serde_json::json!({})),
        ("openrouter", serde_json::json!({})),
        ("xai", serde_json::json!({})),
        ("kimi-code-membership", serde_json::json!({})),
    ] {
        let mut row = provider(serde_json::json!({
            "base_url": "https://relay.example/v1"
        }));
        row.meta = serde_json::json!({ "preset": preset });
        row.settings_config = settings;
        assert!(!is_unknown_custom_relay_provider(&row), "{preset}");
    }

    for (preset, url) in [
        ("anthropic", "https://api.anthropic.com/v1"),
        ("xai", "https://api.x.ai/v1"),
        ("glm-coding-plan", "https://open.bigmodel.cn/api/paas/v4"),
        ("deepseek-api", "https://api.deepseek.com/v1"),
        ("kimi-code-membership", "https://api.kimi.com/coding/v1"),
    ] {
        let mut row = provider(json!({ "base_url": url }));
        row.meta = json!({ "preset": preset });
        assert!(!is_unknown_custom_relay_provider(&row), "{preset}");
    }

    for preset in [
        "anthropic",
        "xai",
        "glm-coding-plan",
        "deepseek-api",
        "kimi-code-membership",
    ] {
        let mut row = provider(json!({ "base_url": "https://relay.example/v1" }));
        row.meta = json!({ "preset": preset });
        assert!(!is_unknown_custom_relay_provider(&row), "{preset}");
    }
}

#[test]
fn unknown_custom_relay_allows_only_exact_official_tag_host_pairs() {
    let provider = |tag, settings_config| crate::models::Provider {
        id: "official".into(),
        agent_id: crate::models::AgentId::Codex,
        name: "official".into(),
        settings_config,
        meta: json!({ "preset": tag }),
        is_current: false,
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };

    for (tag, url) in [
        ("openai", "https://api.openai.com/v1"),
        ("openai-api", "https://api.openai.com/v1"),
        ("openrouter", "https://openrouter.ai/api/v1"),
        ("openai-compat", "https://openrouter.ai/api/v1"),
        ("openai-compatible", "https://openrouter.ai/api/v1"),
    ] {
        assert!(!is_unknown_custom_relay_provider(&provider(
            tag,
            json!({ "base_url": url }),
        )));
    }

    for (tag, url) in [
        ("openai", "https://openrouter.ai/api/v1"),
        ("openai-api", "https://relay.example/v1"),
        ("openrouter", "https://api.openai.com/v1"),
        ("openrouter", "https://openrouter.ai.evil.example/v1"),
    ] {
        assert!(is_unknown_custom_relay_provider(&provider(
            tag,
            json!({ "base_url": url }),
        )));
    }
    assert!(!is_unknown_custom_relay_provider(&provider(
        "openai-compat",
        json!({ "base_url": "https://relay.example/v1" }),
    )));

    assert!(is_unknown_custom_relay_provider(&provider(
        "openai",
        json!({
            "base_url": "https://api.openai.com/v1",
            "baseUrl": "https://relay.example/v1",
        }),
    )));
}

#[test]
fn official_markers_must_match_the_active_exact_host() {
    for (tag, url) in [
        ("openai", "https://openrouter.ai/api/v1"),
        ("openrouter", "https://api.openai.com/v1"),
        ("openai", "https://api.openai.com.evil.example/v1"),
        ("openrouter", "https://openrouter.ai.evil.example/v1"),
        (
            "openai",
            "https://relay.example/v1 https://api.openai.com/v1",
        ),
    ] {
        let settings = json!({ "base_url": url });
        assert!(!is_openai_api_marker(Some(tag), &settings), "{tag}: {url}");
        assert!(is_unknown_custom_relay_provider(&crate::models::Provider {
            id: "bad-official-marker".into(),
            agent_id: crate::models::AgentId::Codex,
            name: "bad official marker".into(),
            settings_config: settings,
            meta: json!({ "preset": tag }),
            is_current: false,
            created_at: "t0".into(),
            updated_at: "t0".into(),
        }));
    }

    assert!(is_openai_api_marker(Some("openai"), &json!({})));
    assert!(!is_openai_api_marker(Some("openrouter"), &json!({})));
}

#[test]
fn toml_active_provider_wins_over_the_first_provider() {
    let blob = json!({
        "format": "toml",
        "content": "model_provider = \"active\"\n\n[model_providers.inactive]\nbase_url = \"https://api.openai.com/v1\"\n\n[model_providers.active]\nbase_url = \"https://relay.example/v1\"\n"
    });
    assert_eq!(
        openai_compat_base_url(&blob).as_deref(),
        Some("https://relay.example/v1")
    );
    assert!(!settings_contain_openai_api_endpoint(&blob));
    assert!(settings_contain_custom_openai_compat_remote(&blob));

    let official = json!({
        "format": "toml",
        "content": "model_provider = \"active\"\n\n[model_providers.inactive]\nbase_url = \"https://relay.example/v1\"\n\n[model_providers.active]\nbase_url = \"https://api.openai.com/v1\"\n"
    });
    assert_eq!(
        openai_compat_base_url(&official).as_deref(),
        Some("https://api.openai.com/v1")
    );
    assert!(settings_contain_openai_api_endpoint(&official));
}

#[test]
fn malformed_or_ambiguous_toml_does_not_fall_back_to_a_provider() {
    let multiple_without_active = json!({
        "format": "toml",
        "content": "[model_providers.inactive]\nbase_url = \"https://api.openai.com/v1\"\n\n[model_providers.other]\nbase_url = \"https://relay.example/v1\"\n"
    });
    assert!(openai_compat_base_url(&multiple_without_active).is_none());
    assert!(!is_openai_api_marker(
        Some("openai"),
        &multiple_without_active
    ));

    let malformed = json!({
        "format": "toml",
        "content": "model_provider = \"active\"\n[model_providers.active]\nbase_url = \"https://relay.example/v1 https://api.openai.com/v1\"\n"
    });
    assert!(openai_compat_base_url(&malformed).is_none());
    assert!(!is_openai_api_marker(Some("openai"), &malformed));
}
